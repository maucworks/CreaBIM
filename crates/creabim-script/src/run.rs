//! M-S1 script execution: declared inputs in, native elements out.
//!
//! Protocol: the host binds an `INPUTS` dict in the script's namespace,
//! runs the source, then reads an `OUTPUTS` list of op dicts. Each op
//! carries a stable `key`; [`reconcile`] (core) maps keys onto element
//! ids so re-runs replace instead of duplicating. Missing `OUTPUTS`
//! means "no outputs" — still a valid run.
//!
//! The op vocabulary is native elements only (Route B contract):
//! `wall` builds a [`Wall`], `mesh` a [`FreeMesh`],
//! `opening`/`door`/`window` an [`Opening`]. Exactly one per op.

use creabim_core::{
    reconcile, EditMesh, Element, ElementId, ElementKind, FreeMesh, Model, Opening, OpeningFrame,
    OpeningKind, ParamDecl, ParamInput, ParamKind, ParamValue, ScriptObject, ScriptOutput,
    Transaction, Wall,
};
use glam::{DVec2, DVec3};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
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
    #[pyo3(default)]
    opening: Option<OpeningOp>,
    #[pyo3(default)]
    door: Option<DoorOp>,
    #[pyo3(default)]
    window: Option<WindowOp>,
}

/// A bare rectangular hole: `host` names a wall element in the model
/// or the key of a wall op in the same `OUTPUTS` list (same-run keys
/// win — they are the script's own namespace). `offset` runs from the
/// wall start to the hole's left edge; `sill` is the hole bottom above
/// the wall base.
#[derive(pyo3::FromPyObject)]
#[pyo3(from_item_all)]
struct OpeningOp {
    host: String,
    #[pyo3(default)]
    offset: Option<f64>,
    #[pyo3(default)]
    sill: Option<f64>,
    width: f64,
    height: f64,
    #[pyo3(default)]
    name: Option<String>,
    #[pyo3(default)]
    mark: Option<String>,
}

/// A door cut into its host wall: an `opening` plus leaf semantics.
/// `sill` defaults to the wall base; `flip_hand` hinges the door on
/// its other jamb, `flip_facing` opens it to the exterior.
#[derive(pyo3::FromPyObject)]
#[pyo3(from_item_all)]
struct DoorOp {
    host: String,
    #[pyo3(default)]
    offset: Option<f64>,
    #[pyo3(default)]
    sill: Option<f64>,
    #[pyo3(default)]
    width: Option<f64>,
    #[pyo3(default)]
    height: Option<f64>,
    #[pyo3(default)]
    flip_hand: Option<bool>,
    #[pyo3(default)]
    flip_facing: Option<bool>,
    #[pyo3(default)]
    name: Option<String>,
    #[pyo3(default)]
    mark: Option<String>,
}

/// A window cut into its host wall: an `opening` plus frame and panes.
/// `sill` defaults to 0.9 m; `panes_across`/`panes_up` split the glass
/// with bars; `frame_width`/`frame_depth` size the frame members
/// (defaults fit the opening and the wall thickness).
#[derive(pyo3::FromPyObject)]
#[pyo3(from_item_all)]
struct WindowOp {
    host: String,
    #[pyo3(default)]
    offset: Option<f64>,
    #[pyo3(default)]
    sill: Option<f64>,
    #[pyo3(default)]
    width: Option<f64>,
    #[pyo3(default)]
    height: Option<f64>,
    #[pyo3(default)]
    panes_across: Option<u32>,
    #[pyo3(default)]
    panes_up: Option<u32>,
    #[pyo3(default)]
    frame_width: Option<f64>,
    #[pyo3(default)]
    frame_depth: Option<f64>,
    #[pyo3(default)]
    flip_facing: Option<bool>,
    #[pyo3(default)]
    name: Option<String>,
    #[pyo3(default)]
    mark: Option<String>,
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
    /// Centreline bend: 0 (default) draws straight; otherwise the arc
    /// through the chord `start` → `end` with sweep `4·atan(bulge)` —
    /// a quarter circle bends at `bulge = ±tan(π/8) ≈ ±0.4142`, a
    /// semicircle at `±1`. Sign follows the sweep: positive is
    /// counter-clockwise, bulging right of `start` → `end` (see
    /// `bulge_through`). Full circles cannot be one wall (the chord
    /// vanishes): split them into semicircles.
    #[pyo3(default)]
    bulge: Option<f64>,
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
    /// `imports` is the project's script directory, put on `sys.path`
    /// for the run so `import helpers` finds the project's own modules
    /// (M-S6 disc sources; removed again after the run).
    fn run(
        &self,
        model: &Model,
        script: &ScriptObject,
        inputs: &ResolvedInputs,
        imports: Option<&std::path::Path>,
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
        imports: Option<&std::path::Path>,
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
        // The builder package travels with the binary (M-S7b): write it
        // into the site dir when missing or stale, before user code runs.
        if let Err(e) = crate::ensure_api(&self.home) {
            return TracedRun {
                result: Err(crate::ScriptError::Python(e)),
                console: String::new(),
            };
        }
        Python::attach(|py| {
            let (result, console) = execute(py, model, script, inputs, imports, tid);
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
    imports: Option<&std::path::Path>,
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
    // Project imports (M-S6): the scripts directory goes on `sys.path`
    // for this run only, so `import helpers` finds the project's own
    // modules. Recorded on `__main__` so the cleanup below removes
    // exactly what this run added, whatever the script did to the path.
    let mut cleanup: Option<std::ffi::CString> = None;
    if let Some(dir) = imports {
        let literal = match PyString::new(py, &dir.to_string_lossy())
            .repr()
            .and_then(|r| r.extract::<String>())
        {
            Ok(l) => l,
            Err(e) => return fail(py_err(e)),
        };
        let setup = match std::ffi::CString::new(format!(
            "import sys as _sys\nif {literal} not in _sys.path:\n    _sys.path.insert(0, {literal})"
        )) {
            Ok(s) => s,
            Err(e) => return fail(crate::ScriptError::Python(e.to_string())),
        };
        if let Err(e) = py.run(&setup, None, None) {
            return fail(py_err(e));
        }
        if let Err(e) = main.setattr("_script_import_dir", dir.to_string_lossy().as_ref()) {
            return fail(py_err(e));
        }
        cleanup = Some(
            std::ffi::CString::new(
                "import sys as _sys\n_sp = __import__('__main__')._script_import_dir\nif _sp in _sys.path:\n    _sys.path.remove(_sp)",
            )
            .unwrap(),
        );
    }
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
    if let Some(clean) = cleanup {
        let _ = py.run(&clean, None, None);
    }
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
    // Two passes: host-less ops first (wall/mesh), so hosted ops
    // (opening/door/window) can name a wall the same run builds. The
    // fresh wall is not in the model yet, so its id and thickness ride
    // along in the hosts map.
    let mut out = Vec::with_capacity(items.len());
    let mut hosts = std::collections::HashMap::new();
    let mut hosted = Vec::new();
    for op in items {
        let key = op.key.clone();
        let is_hosted = op.opening.is_some() || op.door.is_some() || op.window.is_some();
        if is_hosted {
            hosted.push(op);
            continue;
        }
        match materialize(model, &key, op, &hosts) {
            Ok((k, el)) => {
                if let ElementKind::Wall(w) = &el.kind {
                    hosts.insert(k.clone(), (el.id, w.clone()));
                }
                out.push((k, el));
            }
            Err(e) => return (Err(crate::ScriptError::Op(e)), console),
        }
    }
    for op in hosted {
        let key = op.key.clone();
        match materialize(model, &key, op, &hosts) {
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
        imports: Option<&std::path::Path>,
    ) -> Result<Vec<(String, Element)>, crate::ScriptError> {
        self.run_traced(model, script, inputs, imports, &AtomicU64::new(0))
            .result
    }
}

fn py_err(e: PyErr) -> crate::ScriptError {
    crate::ScriptError::Python(e.to_string())
}

fn materialize(
    model: &Model,
    key: &str,
    op: OpItem,
    hosts: &std::collections::HashMap<String, (ElementId, Wall)>,
) -> Result<(String, Element), String> {
    // Exactly one op per item; hosted ops (opening/door/window) resolve
    // after the host-less pass, so a script can cut a hole in a wall it
    // builds in the same run.
    let set = op.wall.is_some() as u8
        + op.mesh.is_some() as u8
        + op.opening.is_some() as u8
        + op.door.is_some() as u8
        + op.window.is_some() as u8;
    if set != 1 {
        return Err(format!(
            "op {key:?}: exactly one of wall/mesh/opening/door/window"
        ));
    }
    if let Some(w) = op.wall {
        return materialize_wall(model, key, w);
    }
    if let Some(m) = op.mesh {
        return materialize_mesh(key, m);
    }
    if let Some(o) = op.opening {
        return materialize_opening(
            model,
            key,
            hosts,
            OpeningKind::Hole,
            o.host,
            o.offset,
            o.sill,
            o.width,
            o.height,
            false,
            false,
            OpeningFrame::default(),
            o.name,
            o.mark,
            "opening",
        );
    }
    if let Some(d) = op.door {
        return materialize_opening(
            model,
            key,
            hosts,
            OpeningKind::Door,
            d.host,
            d.offset,
            d.sill,
            d.width.unwrap_or(0.9),
            d.height.unwrap_or(2.1),
            d.flip_hand.unwrap_or(false),
            d.flip_facing.unwrap_or(false),
            OpeningFrame::default(),
            d.name,
            d.mark,
            "door",
        );
    }
    if let Some(w) = op.window {
        let frame = OpeningFrame {
            width: w.frame_width.unwrap_or(0.06),
            depth: w.frame_depth.unwrap_or(0.1),
            panes_across: w.panes_across.unwrap_or(1),
            panes_up: w.panes_up.unwrap_or(1),
        };
        return materialize_opening(
            model,
            key,
            hosts,
            OpeningKind::Window,
            w.host,
            w.offset,
            Some(w.sill.unwrap_or(0.9)),
            w.width.unwrap_or(1.2),
            w.height.unwrap_or(1.3),
            false,
            w.flip_facing.unwrap_or(false),
            frame,
            w.name,
            w.mark,
            "window",
        );
    }
    unreachable!("counted exactly one op above");
}

fn materialize_wall(model: &Model, key: &str, w: WallOp) -> Result<(String, Element), String> {
    let (level_el, level) = model
        .levels()
        .find(|(e, _)| e.name == w.level)
        .ok_or_else(|| format!("op {key:?}: no level named {:?}", w.level))?;
    let bulge = w.bulge.unwrap_or(0.0);
    if !bulge.is_finite() {
        return Err(format!("op {key:?}: bulge must be finite"));
    }
    let mut wall = Wall::new(
        level_el.id,
        DVec2::new(w.start[0], w.start[1]),
        DVec2::new(w.end[0], w.end[1]),
        w.thickness.unwrap_or(0.1),
        w.height.unwrap_or(level.height),
    );
    wall.bulge = bulge;
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

/// Resolve an opening's host wall: a key of a wall op materialized in
/// the same run first (the script's own namespace — the fresh wall is
/// not in the model yet, so its element rides along), then an element
/// name in the model. Anything else — or a non-wall — is an error
/// naming the op key.
fn resolve_host(
    model: &Model,
    key: &str,
    hosts: &std::collections::HashMap<String, (ElementId, Wall)>,
    host: &str,
) -> Result<(ElementId, Wall), String> {
    if let Some((id, wall)) = hosts.get(host) {
        return Ok((*id, wall.clone()));
    }
    let found = model
        .iter()
        .find(|e| e.name == *host)
        .ok_or_else(|| format!("op {key:?}: no wall named {host:?} (or op key)"))?;
    match &found.kind {
        ElementKind::Wall(w) => Ok((found.id, w.clone())),
        _ => Err(format!("op {key:?}: {host:?} is not a wall")),
    }
}

#[allow(clippy::too_many_arguments)]
fn materialize_opening(
    model: &Model,
    key: &str,
    hosts: &std::collections::HashMap<String, (ElementId, Wall)>,
    kind: OpeningKind,
    host: String,
    offset: Option<f64>,
    sill: Option<f64>,
    width: f64,
    height: f64,
    flip_hand: bool,
    flip_facing: bool,
    frame: OpeningFrame,
    name: Option<String>,
    mark: Option<String>,
    what: &str,
) -> Result<(String, Element), String> {
    let (host, wall) = resolve_host(model, key, hosts, &host)?;
    if width <= 0.0 || height <= 0.0 {
        return Err(format!("op {key:?}: width and height must be positive"));
    }
    let offset = offset.unwrap_or(0.0);
    if offset < 0.0 {
        return Err(format!("op {key:?}: offset must not be negative"));
    }
    let sill = sill.unwrap_or(0.0);
    if sill < 0.0 {
        return Err(format!("op {key:?}: sill must not be negative"));
    }
    // The opening must sit on its host, not past its end or above its
    // top: arc length round a curved host (the fresh wall rides along
    // in `hosts`, so same-run arcs check too). Without this a typo
    // commits floating joinery that only shows up in the drawing.
    let host_len = match wall.curve() {
        creabim_core::sketch::Curve::Line(a, b) => a.distance(b),
        creabim_core::sketch::Curve::Arc { radius, sweep, .. } => sweep.abs() * radius,
    };
    if offset + width > host_len + 1e-6 {
        return Err(format!(
            "op {key:?}: offset {offset} + width {width} runs past the host's end ({host_len:.1})"
        ));
    }
    if sill + height > wall.height + 1e-6 {
        return Err(format!(
            "op {key:?}: sill {sill} + height {height} runs past the host's top ({:.1})",
            wall.height
        ));
    }
    let mut element = Element::new(
        name.unwrap_or_else(|| format!("{what} {key}")),
        ElementKind::Opening(Opening {
            host,
            kind,
            offset,
            sill,
            width,
            height,
            material: None,
            frame: frame.fitted(width, height, wall.thickness),
            flip_hand,
            flip_facing,
        }),
    );
    element.mark = mark;
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
    imports: Option<&std::path::Path>,
) -> Result<(Transaction, Vec<ScriptOutput>), crate::ScriptError> {
    let fresh = engine.run(model, script, inputs, imports)?;
    // Hosted ops point at fresh host ids; reconcile remaps them onto
    // stored ids for known keys (or re-runs dangle).
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
        run_to_transaction(engine, &model, script_id, &script, &inputs, label, None).unwrap()
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
            let worker = s.spawn(|| engine.run_traced(&model, &script, &inputs, None, &tid));
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

    /// A model with a level and one pre-existing wall for host-by-name.
    fn hosted_model() -> (Document, ElementId, ElementId) {
        let mut doc = Document::from_model(Model::new());
        doc.commit(Transaction::new("level").insert(Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 2.7,
            }),
        )))
        .unwrap();
        let level = doc
            .model()
            .levels()
            .find(|(e, _)| e.name == "Ground")
            .map(|(e, _)| e.id)
            .unwrap();
        let south = Element::new(
            "South wall",
            ElementKind::Wall(Wall::new(
                level,
                DVec2::new(0.0, 0.0),
                DVec2::new(8.0, 0.0),
                0.365,
                2.7,
            )),
        );
        let south_id = south.id;
        doc.commit(Transaction::new("wall").insert(south)).unwrap();
        (doc, south_id, level)
    }

    const OPENINGS_SCRIPT: &str = "from creabim import wall, door, window, opening\nOUTPUTS = [wall(\"w-0\", \"Ground\", [0.0, 4.0], [4.0, 4.0], thickness=0.1), door(\"d-0\", \"w-0\", offset=1.0, mark=\"D01\"), window(\"win-0\", \"w-0\", offset=2.5, panes_across=3, mark=\"W01\"), opening(\"h-0\", \"South wall\", width=0.5, height=0.5, sill=1.0)]\n";

    fn openings_of(doc: &Document) -> Vec<(String, ElementId, OpeningKind, ElementId)> {
        doc.model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Opening(o) => Some((e.name.clone(), e.id, o.kind, o.host)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn builders_round_trip_through_the_parser() {
        let home = crate::ensure_provisioned().expect("provision the bundle");
        let engine = CPythonEngine::new(home);
        let (mut doc, south_id, _) = hosted_model();
        let script = Element::new(
            "openings",
            ElementKind::ScriptObject(ScriptObject::new(OPENINGS_SCRIPT, "cpython")),
        );
        let script_id = script.id;
        doc.commit(Transaction::new("script").insert(script))
            .unwrap();

        let (tx, outputs) = run_current(&engine, &doc, script_id, &[], "script:test");
        assert_eq!(outputs.len(), 4);
        doc.commit(tx).expect("commit the run");
        assert_eq!(walls(doc.model()), 2);

        let mut ops = openings_of(&doc);
        ops.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(ops.len(), 3);
        // The door: defaults (900 x 2100 at the base), hosted on the
        // same-run wall, mark carried for the schedule.
        let door = ops.iter().find(|(n, ..)| n == "door d-0").expect("door");
        assert!(matches!(door.2, OpeningKind::Door));
        let wall_id = doc
            .model()
            .iter()
            .find(|e| e.name == "wall w-0")
            .map(|e| e.id)
            .expect("same-run wall");
        assert_eq!(door.3, wall_id, "door hosts the same-run wall");
        let door_el = doc.model().get(door.1).unwrap();
        assert_eq!(door_el.mark.as_deref(), Some("D01"));
        match &door_el.kind {
            ElementKind::Opening(o) => {
                assert_eq!((o.width, o.height, o.sill, o.offset), (0.9, 2.1, 0.0, 1.0));
                assert_eq!((o.flip_hand, o.flip_facing), (false, false));
            }
            _ => panic!("not an opening"),
        }
        // The window: sill default 0.9, three panes, same-run host.
        let win = ops
            .iter()
            .find(|(n, ..)| n == "window win-0")
            .expect("window");
        assert!(matches!(win.2, OpeningKind::Window));
        assert_eq!(win.3, wall_id);
        match &doc.model().get(win.1).unwrap().kind {
            ElementKind::Opening(o) => {
                assert_eq!(o.sill, 0.9);
                assert_eq!((o.frame.panes_across, o.width), (3, 1.2));
            }
            _ => panic!("not an opening"),
        }
        // The hole: hosted on the pre-existing wall by element name.
        let hole = ops.iter().find(|(n, ..)| n == "opening h-0").expect("hole");
        assert!(matches!(hole.2, OpeningKind::Hole));
        assert_eq!(hole.3, south_id);

        // Re-run: same keys, same ids, and every host still resolves —
        // the remap in run_to_transaction holds (no dangling hosts).
        let (tx, outputs2) = run_current(&engine, &doc, script_id, &[], "script:test");
        doc.commit(tx).expect("commit the re-run");
        assert_eq!(
            outputs2.iter().map(|o| o.id).collect::<Vec<_>>(),
            outputs.iter().map(|o| o.id).collect::<Vec<_>>()
        );
        for (_, _, _, host) in openings_of(&doc) {
            assert!(
                matches!(
                    doc.model().get(host).map(|e| &e.kind),
                    Some(ElementKind::Wall(_))
                ),
                "host {host:?} resolves to a wall after re-run"
            );
        }
    }

    const RUN_SCRIPT: &str = "from creabim import walls\nOUTPUTS = walls(\"r\", \"Ground\", [[0.0, 0.0], [3.0, 0.0], [3.0, 2.0]], thickness=0.2)\n";

    const ARC_SCRIPT: &str = "import math\nfrom creabim import arc_wall\nOUTPUTS = [arc_wall(\"a\", \"Ground\", [0.0, 0.0], [2.0, 0.0], math.pi / 2)]\n";

    #[test]
    fn arc_walls_follow_the_sweep_not_the_chord() {
        let home = crate::ensure_provisioned().expect("provision the bundle");
        let engine = CPythonEngine::new(home);
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
            "arc",
            ElementKind::ScriptObject(ScriptObject::new(ARC_SCRIPT, "cpython")),
        );
        let script_id = script.id;
        doc.commit(Transaction::new("script").insert(script))
            .unwrap();
        let (tx, outputs) = run_current(&engine, &doc, script_id, &[], "script:test");
        assert_eq!(outputs.len(), 1);
        doc.commit(tx).expect("commit the run");

        // Chord (2,0)→(0,2), quarter circle counter-clockwise:
        // bulge tan(π/8), apex at (√2, √2) — the wall bends, unsegmented.
        let wall = doc
            .model()
            .iter()
            .find(|e| e.name == "wall a")
            .expect("arc wall");
        match &wall.kind {
            ElementKind::Wall(w) => {
                assert!(w.start.distance(DVec2::new(2.0, 0.0)) < 1e-9);
                assert!(w.end.distance(DVec2::new(0.0, 2.0)) < 1e-9);
                assert!((w.bulge - (std::f64::consts::FRAC_PI_2 / 4.0).tan()).abs() < 1e-9);
                let apex = w.curve().at(0.5);
                let expected = DVec2::new(2.0_f64.sqrt(), 2.0_f64.sqrt());
                assert!(apex.distance(expected) < 1e-6, "apex {apex:?}");
                assert!(w.is_arc());
            }
            _ => panic!("not a wall"),
        }
    }

    #[test]
    fn wall_runs_commit_as_joined_segments() {
        let home = crate::ensure_provisioned().expect("provision the bundle");
        let engine = CPythonEngine::new(home);
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
            "run",
            ElementKind::ScriptObject(ScriptObject::new(RUN_SCRIPT, "cpython")),
        );
        let script_id = script.id;
        doc.commit(Transaction::new("script").insert(script))
            .unwrap();
        let (tx, outputs) = run_current(&engine, &doc, script_id, &[], "script:test");
        assert_eq!(outputs.len(), 2);
        doc.commit(tx).expect("commit the run");

        let mut found: Vec<(String, DVec2, DVec2, f64)> = doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.name.clone(), w.start, w.end, w.thickness)),
                _ => None,
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(found.len(), 2);
        // Consecutive segments share their endpoints exactly, so the
        // join solver corners them with no extra elements.
        assert_eq!(found[0].1, DVec2::new(0.0, 0.0));
        assert_eq!(found[0].2, DVec2::new(3.0, 0.0));
        assert_eq!(found[1].1, found[0].2);
        assert_eq!(found[1].2, DVec2::new(3.0, 2.0));
        assert!(found.iter().all(|(_, _, _, t)| *t == 0.2));

        // Re-run: same keys, same segments.
        let (tx, outputs2) = run_current(&engine, &doc, script_id, &[], "script:test");
        doc.commit(tx).expect("commit the re-run");
        assert_eq!(
            outputs2.iter().map(|o| o.id).collect::<Vec<_>>(),
            outputs.iter().map(|o| o.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn host_errors_name_the_op() {
        let home = crate::ensure_provisioned().expect("provision the bundle");
        let engine = CPythonEngine::new(home);
        let (mut doc, _, _) = hosted_model();
        for (op_key, source) in [
            (
                "\"d\"",
                "from creabim import door\nOUTPUTS = [door(\"d\", \"no such wall\")]",
            ),
            (
                "\"w\"",
                "from creabim import window\nOUTPUTS = [window(\"w\", \"Ground\")]",
            ),
            (
                "\"past\"",
                "from creabim import wall, door\nOUTPUTS = [wall(\"h\", \"Ground\", [0.0, 0.0], [2.0, 0.0]), door(\"past\", \"h\", offset=1.5)]",
            ),
            (
                "\"tall\"",
                "from creabim import wall, window\nOUTPUTS = [wall(\"h\", \"Ground\", [0.0, 0.0], [4.0, 0.0]), window(\"tall\", \"h\", offset=1.0, sill=2.0)]",
            ),
        ] {
            let script = Element::new(
                "err",
                ElementKind::ScriptObject(ScriptObject::new(source, "cpython")),
            );
            let script_id = script.id;
            doc.commit(Transaction::new("script").insert(script))
                .unwrap();
            let model = doc.model().clone();
            let script = match &model.get(script_id).unwrap().kind {
                ElementKind::ScriptObject(s) => s.clone(),
                _ => panic!("not a script"),
            };
            let decls = script.declared().unwrap();
            let inputs = resolve(&model, &decls, &script.inputs, &[]).unwrap();
            let err = run_to_transaction(
                &engine,
                &model,
                script_id,
                &script,
                &inputs,
                "script:test",
                None,
            )
            .unwrap_err();
            assert!(
                err.to_string().contains(op_key),
                "error names the op key: {err}"
            );
            assert_eq!(openings_of(&doc).len(), 0, "a failed run commits nothing");
        }
    }

    const BIARC_SCRIPT: &str = "from geometrykit.geometry import Curve\nfrom geometrykit.geometry.primitives import Vec\nc = Curve.from_tangents(Vec(0, 0, 2.5), Vec(1, 0, 0), Vec(8, 0, 3.0), Vec(1, 0, 0), scale=3.0)\np = c.to_biarcs(tol=0.01, max_iteration=5)\narcs = [s for s in p.segments if hasattr(s, \"radius\")]\nassert len(arcs) > 0, \"expected arc segments\"\nassert all(a.radius > 0 for a in arcs)\nOUTPUTS = [{\"key\": \"probe\", \"mesh\": {\"verts\": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], \"faces\": [[0, 1, 2]]}}]\n";

    #[test]
    fn geometrykit_biarcs_run_embedded() {
        let home = crate::ensure_provisioned().expect("provision the bundle");
        crate::ensure_packages(&["geometrykit"]).expect("install geometrykit");
        let engine = CPythonEngine::new(home);
        let mut script = ScriptObject::new(BIARC_SCRIPT, "cpython");
        script.requirements = vec!["geometrykit".to_string()];
        let mut doc = Document::from_model(Model::new());
        let script_el = Element::new("biarcs", ElementKind::ScriptObject(script.clone()));
        let id = script_el.id;
        doc.commit(Transaction::new("script").insert(script_el))
            .expect("add the script");
        let model = doc.model().clone();
        let script = match &model.get(id).unwrap().kind {
            ElementKind::ScriptObject(s) => s.clone(),
            _ => panic!("not a script"),
        };
        let inputs = ResolvedInputs(vec![]);
        let (tx, outputs) =
            run_to_transaction(&engine, &model, id, &script, &inputs, "script:test", None)
                .expect("run the biarc script");
        assert_eq!(outputs.len(), 1);
        doc.commit(tx).expect("commit the probe mesh");
        assert_eq!(doc.model().iter().count(), 2);
    }
}
