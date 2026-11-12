//! M-S1 script execution: declared inputs in, native elements out.
//!
//! Protocol: the host binds an `INPUTS` dict in the script's namespace,
//! runs the source, then reads an `OUTPUTS` list of op dicts. Each op
//! carries a stable `key`; [`reconcile`] (core) maps keys onto element
//! ids so re-runs replace instead of duplicating. Missing `OUTPUTS`
//! means "no outputs" — still a valid run.
//!
//! The op vocabulary is native elements only (Route B contract):
//! `wall` builds a [`Wall`], `mesh` a [`FreeMesh`]. Exactly one per op.

use creabim_core::{
    reconcile, EditMesh, Element, ElementId, ElementKind, FreeMesh, Model, ParamDecl, ParamInput,
    ParamKind, ParamValue, ScriptObject, ScriptOutput, Transaction, Wall,
};
use glam::{DVec2, DVec3};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;

use crate::ScriptError;

/// One script runs at a time, process-wide: concurrent runs would share
/// the `__main__` namespace and the redirected `sys.stdout`, and the
/// GIL serializes them anyway. The app runner queues sequentially.
static RUN_LOCK: Mutex<()> = Mutex::new(());

/// A resolved input value.
#[derive(Clone, Debug)]
pub enum ResolvedValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
    Element(ElementId),
}

/// Inputs keyed by declared name.
#[derive(Clone, Debug, Default)]
pub struct ResolvedInputs(pub Vec<(String, ResolvedValue)>);

/// Resolve declared inputs: header default, overridden by the element's
/// stored inputs, overridden by CLI `name=value` strings. Element-typed
/// CLI values name a model element.
pub fn resolve(
    model: &Model,
    decls: &[ParamDecl],
    stored: &[ParamInput],
    cli: &[(String, String)],
) -> Result<ResolvedInputs, crate::ScriptError> {
    let err = crate::ScriptError::Op;
    let mut out = Vec::with_capacity(decls.len());
    for d in decls {
        let value = if let Some((_, raw)) = cli.iter().find(|(k, _)| *k == d.name) {
            return_value(&parse_cli(model, d, raw).map_err(err)?)
        } else if let Some(found) = stored.iter().find(|i| i.name == d.name) {
            check_kind(d, &found.value).map_err(err)?;
            return_value(&found.value)
        } else if let Some(default) = &d.default {
            return_value(default)
        } else {
            return Err(err(format!("input {:?} has no value", d.name)));
        };
        out.push((d.name.clone(), value));
    }
    Ok(ResolvedInputs(out))
}

fn return_value(v: &ParamValue) -> ResolvedValue {
    match v {
        ParamValue::Float(x) => ResolvedValue::Float(*x),
        ParamValue::Int(x) => ResolvedValue::Int(*x),
        ParamValue::Bool(x) => ResolvedValue::Bool(*x),
        ParamValue::Text(x) => ResolvedValue::Text(x.clone()),
        ParamValue::Element(id) => ResolvedValue::Element(*id),
    }
}

fn check_kind(d: &ParamDecl, v: &ParamValue) -> Result<(), String> {
    let ok = matches!(
        (&d.kind, v),
        (ParamKind::Float, ParamValue::Float(_))
            | (ParamKind::Int, ParamValue::Int(_))
            | (ParamKind::Bool, ParamValue::Bool(_))
            | (ParamKind::Text, ParamValue::Text(_))
            | (ParamKind::Element, ParamValue::Element(_))
    );
    if ok {
        Ok(())
    } else {
        Err(format!("input {:?} has the wrong kind of value", d.name))
    }
}

fn parse_cli(model: &Model, d: &ParamDecl, raw: &str) -> Result<ParamValue, String> {
    match d.kind {
        ParamKind::Float => raw
            .parse::<f64>()
            .or_else(|_| raw.parse::<i64>().map(|i| i as f64))
            .map(ParamValue::Float)
            .map_err(|_| format!("{:?} is not a float", raw)),
        ParamKind::Int => raw
            .parse::<i64>()
            .map(ParamValue::Int)
            .map_err(|_| format!("{:?} is not an integer", raw)),
        ParamKind::Bool => match raw {
            "true" => Ok(ParamValue::Bool(true)),
            "false" => Ok(ParamValue::Bool(false)),
            _ => Err(format!("{:?} is not true/false", raw)),
        },
        ParamKind::Text => Ok(ParamValue::Text(raw.to_string())),
        ParamKind::Element => model
            .iter()
            .find(|e| e.name == raw)
            .map(|e| ParamValue::Element(e.id))
            .ok_or_else(|| format!("no element named {raw:?}")),
    }
}

/// One script output: a stable key plus exactly one native op.
#[derive(pyo3::FromPyObject)]
#[pyo3(from_item_all)]
struct OpItem {
    key: String,
    #[pyo3(default)]
    wall: Option<WallOp>,
    #[pyo3(default)]
    mesh: Option<MeshOp>,
}

#[derive(pyo3::FromPyObject)]
#[pyo3(from_item_all)]
struct WallOp {
    level: String,
    start: [f64; 2],
    end: [f64; 2],
    #[pyo3(default)]
    thickness: Option<f64>,
    #[pyo3(default)]
    height: Option<f64>,
    #[pyo3(default)]
    name: Option<String>,
    #[pyo3(default)]
    mark: Option<String>,
}

#[derive(pyo3::FromPyObject)]
#[pyo3(from_item_all)]
struct MeshOp {
    verts: Vec<[f64; 3]>,
    faces: Vec<Vec<u32>>,
    #[pyo3(default)]
    name: Option<String>,
}

/// A script execution backend.
pub trait ScriptEngine {
    /// Short name, matching `ScriptObject.engine` (`"cpython"`).
    fn name(&self) -> &'static str;
    /// Run the script; return keyed fresh elements for [`reconcile`].
    fn run(
        &self,
        model: &Model,
        script: &ScriptObject,
        inputs: &ResolvedInputs,
    ) -> Result<Vec<(String, Element)>, crate::ScriptError>;
}

/// A finished run: the output (or the traceback) plus captured console.
pub struct TracedRun {
    pub result: Result<Vec<(String, Element)>, crate::ScriptError>,
    pub console: String,
}

/// CPython-backed [`ScriptEngine`].
pub struct CPythonEngine {
    home: std::path::PathBuf,
}

impl CPythonEngine {
    pub fn new(home: std::path::PathBuf) -> Self {
        Self { home }
    }

    /// Run with interruption support: the worker's Python thread id is
    /// published in `tid` while user code runs (0 when done), so
    /// [`engine::async_interrupt`][crate::engine::async_interrupt] can
    /// raise into it. `print` output is captured process-wide (the
    /// interpreter is process-global) and restored afterwards.
    pub fn run_traced(
        &self,
        model: &Model,
        script: &ScriptObject,
        inputs: &ResolvedInputs,
        tid: &AtomicU64,
    ) -> TracedRun {
        use std::sync::atomic::Ordering;
        if script.engine != self.name() {
            return TracedRun {
                result: Err(crate::ScriptError::UnsupportedEngine(script.engine.clone())),
                console: String::new(),
            };
        }
        // Poison-tolerant: a panicking run must not wedge later runs.
        let _guard = RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = crate::ensure_initialized(&self.home) {
            return TracedRun {
                result: Err(e),
                console: String::new(),
            };
        }
        Python::attach(|py| {
            let (result, console) = execute(py, model, script, inputs, tid);
            // The id must not outlive the thread, even on early exits:
            // a stale id interrupts whoever reuses the thread next.
            tid.store(0, Ordering::SeqCst);
            TracedRun { result, console }
        })
    }

    fn name(&self) -> &'static str {
        "cpython"
    }
}

/// Shared core: publish tid, redirect console, bind inputs, run,
/// restore console, read outputs. Never panics; the tid is cleared
/// by the caller.
fn execute(
    py: Python<'_>,
    model: &Model,
    script: &ScriptObject,
    inputs: &ResolvedInputs,
    tid: &AtomicU64,
) -> (Result<Vec<(String, Element)>, crate::ScriptError>, String) {
    use std::sync::atomic::Ordering;
    let fail = |e: ScriptError| (Err(e), String::new());
    let id: u64 = match py
        .eval(c"__import__('threading').get_ident()", None, None)
        .and_then(|v| v.extract())
    {
        Ok(id) => id,
        Err(e) => return fail(py_err(e)),
    };
    tid.store(id, Ordering::SeqCst);
    if let Err(e) = py.run(
        c"import io as _io, sys as _sys\n_out = _io.StringIO()\n_old_out, _old_err = _sys.stdout, _sys.stderr\n_sys.stdout = _sys.stderr = _out",
        None,
        None,
    ) {
        return fail(py_err(e));
    }
    let main = match py.import("__main__") {
        Ok(m) => m,
        Err(e) => return fail(py_err(e)),
    };
    let bound = PyDict::new(py);
    for (name, value) in &inputs.0 {
        let set = match value.clone() {
            ResolvedValue::Float(x) => bound.set_item(name.as_str(), x),
            ResolvedValue::Int(x) => bound.set_item(name.as_str(), x),
            ResolvedValue::Bool(x) => bound.set_item(name.as_str(), x),
            ResolvedValue::Text(x) => bound.set_item(name.as_str(), x),
            ResolvedValue::Element(id) => bound.set_item(name.as_str(), id.0.to_string()),
        };
        if let Err(e) = set {
            return fail(py_err(e));
        }
    }
    if let Err(e) = main.setattr("INPUTS", &bound) {
        return fail(py_err(e));
    }
    let user_code = match std::ffi::CString::new(script.source.clone()) {
        Ok(c) => c,
        Err(e) => return fail(crate::ScriptError::Python(e.to_string())),
    };
    let ran = py.run(&user_code, None, None);
    // Restore first: the console must survive user errors, and the
    // restore itself must survive (best-effort read after).
    let console = py
        .run(
            c"_sys.stdout, _sys.stderr = _old_out, _old_err\n_console = _out.getvalue()",
            None,
            None,
        )
        .ok()
        .and_then(|_| main.getattr("_console").ok())
        .and_then(|v| v.extract::<String>().ok())
        .unwrap_or_else(|| "<console lost>".to_string());
    if let Err(e) = ran {
        return (Err(py_err(e)), console);
    }
    let items: Vec<OpItem> = match main.getattr("OUTPUTS") {
        Ok(attr) => match attr.extract() {
            Ok(items) => items,
            Err(e) => return (Err(py_err(e)), console),
        },
        Err(_) => Vec::new(),
    };
    let mut out = Vec::with_capacity(items.len());
    for op in items {
        let key = op.key.clone();
        match materialize(model, &key, op) {
            Ok(el) => out.push(el),
            Err(e) => return (Err(crate::ScriptError::Op(e)), console),
        }
    }
    (Ok(out), console)
}

impl ScriptEngine for CPythonEngine {
    fn name(&self) -> &'static str {
        "cpython"
    }

    fn run(
        &self,
        model: &Model,
        script: &ScriptObject,
        inputs: &ResolvedInputs,
    ) -> Result<Vec<(String, Element)>, crate::ScriptError> {
        self.run_traced(model, script, inputs, &AtomicU64::new(0))
            .result
    }
}

fn py_err(e: PyErr) -> crate::ScriptError {
    crate::ScriptError::Python(e.to_string())
}

fn materialize(model: &Model, key: &str, op: OpItem) -> Result<(String, Element), String> {
    match (op.wall, op.mesh) {
        (Some(w), None) => materialize_wall(model, key, w),
        (None, Some(m)) => materialize_mesh(key, m),
        _ => Err(format!("op {key:?}: exactly one of wall/mesh")),
    }
}

fn materialize_wall(model: &Model, key: &str, w: WallOp) -> Result<(String, Element), String> {
    let (level_el, level) = model
        .levels()
        .find(|(e, _)| e.name == w.level)
        .ok_or_else(|| format!("op {key:?}: no level named {:?}", w.level))?;
    let wall = Wall::new(
        level_el.id,
        DVec2::new(w.start[0], w.start[1]),
        DVec2::new(w.end[0], w.end[1]),
        w.thickness.unwrap_or(0.1),
        w.height.unwrap_or(level.height),
    );
    let mut element = Element::new(
        w.name.unwrap_or_else(|| format!("wall {key}")),
        ElementKind::Wall(wall),
    );
    element.mark = w.mark;
    Ok((key.to_string(), element))
}

fn materialize_mesh(key: &str, m: MeshOp) -> Result<(String, Element), String> {
    let mesh = EditMesh {
        vertices: m
            .verts
            .into_iter()
            .map(|p| DVec3::new(p[0], p[1], p[2]))
            .collect(),
        faces: m.faces,
    };
    mesh.validate().map_err(|e| format!("op {key:?}: {e}"))?;
    let element = Element::new(
        m.name.unwrap_or_else(|| format!("mesh {key}")),
        ElementKind::FreeMesh(FreeMesh::new(None, DVec3::ZERO, mesh)),
    );
    Ok((key.to_string(), element))
}

/// Run a script to a committed transaction: outputs replace by key,
/// the script element records the new key map, all in one step.
pub fn run_to_transaction(
    engine: &impl ScriptEngine,
    model: &Model,
    script_id: ElementId,
    script: &ScriptObject,
    inputs: &ResolvedInputs,
    label: &str,
) -> Result<(Transaction, Vec<ScriptOutput>), crate::ScriptError> {
    let fresh = engine.run(model, script, inputs)?;
    let (mut tx, outputs) = reconcile(model, label, &script.outputs, fresh);
    let mut updated = script.clone();
    updated.outputs = outputs.clone();
    let before = model
        .get(script_id)
        .cloned()
        .ok_or_else(|| crate::ScriptError::Op("script element vanished".into()))?;
    let mut after = before.clone();
    after.kind = ElementKind::ScriptObject(updated);
    tx = tx.replace(before, after);
    Ok((tx, outputs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{Document, Level};

    const PARTITIONS: &str = "# partition walls\n# in: bays: int = 3\n# in: depth: float = 4.0\nn = INPUTS[\"bays\"]\nd = INPUTS[\"depth\"]\nOUTPUTS = [{\"key\": f\"w-{i}\", \"wall\": {\"level\": \"Ground\", \"start\": [i * d / n, 0.0], \"end\": [i * d / n, 4.0], \"thickness\": 0.1}} for i in range(n + 1)]\n";

    fn walls(model: &Model) -> usize {
        model
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
            .count()
    }

    fn scripted_model() -> (Document, ElementId) {
        let mut doc = Document::from_model(Model::new());
        doc.commit(Transaction::new("level").insert(Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 2.7,
            }),
        )))
        .unwrap();
        let script = Element::new(
            "partitions",
            ElementKind::ScriptObject(ScriptObject::new(PARTITIONS, "cpython")),
        );
        let id = script.id;
        doc.commit(Transaction::new("script").insert(script))
            .unwrap();
        (doc, id)
    }

    fn run_current(
        engine: &CPythonEngine,
        doc: &Document,
        script_id: ElementId,
        cli: &[(String, String)],
        label: &str,
    ) -> (Transaction, Vec<ScriptOutput>) {
        let model = doc.model().clone();
        let script = match &model.get(script_id).unwrap().kind {
            ElementKind::ScriptObject(s) => s.clone(),
            _ => panic!("not a script"),
        };
        let decls = script.declared().unwrap();
        let inputs = resolve(&model, &decls, &script.inputs, cli).unwrap();
        run_to_transaction(engine, &model, script_id, &script, &inputs, label).unwrap()
    }

    #[test]
    fn resolve_prefers_stored_then_cli() {
        let model = Model::new();
        let decls = creabim_core::parse_front_matter("# in: n: int = 2\n# in: t: string = \"a\"\n")
            .unwrap();
        let stored = vec![ParamInput {
            name: "n".into(),
            value: ParamValue::Int(5),
        }];
        let cli = vec![("t".to_string(), "b".to_string())];
        let resolved = resolve(&model, &decls, &stored, &cli).unwrap();
        assert!(matches!(resolved.0[0].1, ResolvedValue::Int(5)));
        assert!(matches!(resolved.0[1].1, ResolvedValue::Text(_)));
    }

    #[test]
    fn partition_walls_commit_undo_and_keep_ids() {
        let home = crate::ensure_provisioned().expect("provision the bundle");
        crate::ensure_packages(&["numpy"]).expect("install numpy");
        let engine = CPythonEngine::new(home);
        let (mut doc, script_id) = scripted_model();

        let (tx, outputs) = run_current(&engine, &doc, script_id, &[], "script:test");
        assert_eq!(outputs.len(), 4);
        doc.commit(tx).expect("commit the run");
        assert_eq!(walls(doc.model()), 4);

        assert!(doc.undo(), "the run undoes");
        assert_eq!(walls(doc.model()), 0);

        let cli = vec![("bays".to_string(), "5".to_string())];
        let (tx, outputs) = run_current(&engine, &doc, script_id, &cli, "script:test");
        assert_eq!(outputs.len(), 6);
        doc.commit(tx).expect("commit the re-run");
        assert_eq!(walls(doc.model()), 6);

        // Same keys, same elements: a no-change re-run only replaces.
        let (tx, outputs2) = run_current(&engine, &doc, script_id, &cli, "script:test");
        assert!(tx
            .ops
            .iter()
            .all(|o| matches!(o, creabim_core::Op::Replace { .. })));
        assert_eq!(
            outputs2.iter().map(|o| o.id).collect::<Vec<_>>(),
            outputs.iter().map(|o| o.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn interrupt_stops_a_runaway() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{Duration, Instant};
        let home = crate::ensure_provisioned().expect("provision the bundle");
        crate::ensure_packages(&["numpy"]).expect("install numpy");
        let engine = CPythonEngine::new(home);
        let model = Model::new();
        let script = ScriptObject::new("print(\"start\")\nwhile True:\n    pass\n", "cpython");
        let inputs = ResolvedInputs(vec![]);
        let tid = AtomicU64::new(0);
        std::thread::scope(|s| {
            let worker = s.spawn(|| engine.run_traced(&model, &script, &inputs, &tid));
            let start = Instant::now();
            while tid.load(Ordering::SeqCst) == 0 {
                if start.elapsed() > Duration::from_secs(10) {
                    panic!("worker never reached user code");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(crate::async_interrupt(tid.load(Ordering::SeqCst)));
            let done = worker.join().expect("worker survives interrupt");
            assert!(
                matches!(done.result, Err(crate::ScriptError::Python(_))),
                "runaway ends in an error"
            );
            assert!(done.console.contains("start"), "console survives interrupt");
            assert_eq!(tid.load(Ordering::SeqCst), 0, "tid cleared");
        });
    }
}
