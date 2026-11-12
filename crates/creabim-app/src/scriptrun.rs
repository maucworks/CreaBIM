//! Background script recompute, Builds-style.
//!
//! One run at a time (the engine serializes runs anyway): a worker
//! thread recomputes from a model snapshot while the UI stays live;
//! results come back over `mpsc`; commits happen on the UI thread.
//! Stale results are discarded, never committed: a result only lands
//! when the script element is unchanged since the request.
//!
//! Dirty rule: source, engine or inputs changed since the last run —
//! or, for scripts with element inputs, any model change (any edit may
//! have touched a referenced element). Unchanged results skip the
//! commit so a no-op re-run leaves no history behind.

use creabim_core::{
    reconcile, Document, Element, ElementId, ElementKind, Model, ParamValue, ScriptObject,
    ScriptOutput, Transaction,
};
use creabim_script::{ensure_packages, ensure_provisioned, CPythonEngine, ResolvedInputs};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Give up on a run and interrupt it after this long.
const TIMEOUT: Duration = Duration::from_secs(60);
/// Wait this long after the last model change before starting a run,
/// so typing a script does not queue one per keystroke.
const DEBOUNCE: Duration = Duration::from_millis(500);
/// A timed-out worker gets this long to die before its slot is freed;
/// a late result is still discarded by key check.
const GRACE: Duration = Duration::from_secs(5);
/// Console kept per script, truncated.
const CONSOLE_CAP: usize = 8192;

/// What a run was asked to compute. Compared by value (never hashed:
/// `DefaultHasher` is randomly seeded per instance, so hashes never
/// match across calls). The model revision is deliberately excluded:
/// it decides *whether* to run, while `commit` re-checks the element.
#[derive(Clone, PartialEq, Eq)]
struct RequestKey {
    source: String,
    engine: String,
    inputs: String,
}

fn request_key(script: &ScriptObject) -> RequestKey {
    RequestKey {
        source: script.source.clone(),
        engine: script.engine.clone(),
        inputs: format!("{:?}", script.inputs),
    }
}

fn has_element_inputs(script: &ScriptObject) -> bool {
    script
        .inputs
        .iter()
        .any(|i| matches!(i.value, ParamValue::Element(_)))
}

pub enum Status {
    Idle,
    Running {
        since: Instant,
    },
    Ok {
        ms: u128,
        added: usize,
        replaced: usize,
        removed: usize,
    },
    Failed {
        traceback: String,
    },
    Cancelled,
    TimedOut,
}

struct Entry {
    last_key: Option<RequestKey>,
    last_revision: u64,
    status: Status,
    console: String,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            last_key: None,
            last_revision: 0,
            status: Status::Idle,
            console: String::new(),
        }
    }
}

struct Running {
    script_id: ElementId,
    key: RequestKey,
    tid: Arc<AtomicU64>,
    started: Instant,
    cancel_asked: bool,
    interrupted: bool,
    timed_out: bool,
    timed_out_at: Option<Instant>,
}

struct Done {
    script_id: ElementId,
    key: RequestKey,
    result: RunOutcome,
    ms: u128,
}

/// A finished run: fresh keyed elements plus captured `print` output,
/// or the traceback plus whatever was printed before it failed.
type RunOutcome = Result<(Vec<(String, Element)>, String), (String, String)>;

pub struct ScriptRunner {
    tx: mpsc::Sender<Done>,
    rx: mpsc::Receiver<Done>,
    running: Option<Running>,
    entries: HashMap<ElementId, Entry>,
    queue: VecDeque<ElementId>,
    timeout: Duration,
    last_revision: u64,
    last_change: Instant,
}

impl Default for ScriptRunner {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            running: None,
            entries: HashMap::new(),
            queue: VecDeque::new(),
            timeout: TIMEOUT,
            last_revision: 0,
            last_change: Instant::now(),
        }
    }
}

impl ScriptRunner {
    #[cfg(test)]
    fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            ..Self::default()
        }
    }

    /// Run `id` as soon as the worker is free, bypassing the debounce.
    pub fn force(&mut self, id: ElementId) {
        if !self.queue.contains(&id) {
            self.queue.push_back(id);
        }
    }

    /// Stop the running script, if it is `id`; queued runs are dropped.
    /// The interrupt is retried on poll until the worker publishes its
    /// thread id, so cancelling a starting run still lands.
    pub fn cancel(&mut self, id: ElementId) {
        self.queue.retain(|q| *q != id);
        if let Some(r) = &mut self.running {
            if r.script_id == id {
                r.cancel_asked = true;
            }
        }
    }

    pub fn status(&self, id: ElementId) -> &Status {
        self.entries
            .get(&id)
            .map(|e| &e.status)
            .unwrap_or(&Status::Idle)
    }

    #[cfg(test)]
    pub fn status_string(&self, id: ElementId) -> String {
        match self.status(id) {
            Status::Idle => "idle".into(),
            Status::Running { .. } => "running".into(),
            Status::Ok {
                ms,
                added,
                replaced,
                removed,
            } => {
                format!("ok {ms}ms +{added} ~{replaced} -{removed}")
            }
            Status::Failed { traceback } => format!("failed: {traceback:.200}"),
            Status::Cancelled => "cancelled".into(),
            Status::TimedOut => "timed_out".into(),
        }
    }

    /// Tail of captured `print` output (empty when the script was quiet).
    pub fn console(&self, id: ElementId) -> &str {
        self.entries
            .get(&id)
            .map(|e| e.console.as_str())
            .unwrap_or("")
    }

    fn entry(&mut self, id: ElementId) -> &mut Entry {
        self.entries.entry(id).or_default()
    }

    /// Poll once per frame. Returns true when the UI should repaint.
    pub fn poll(&mut self, doc: &mut Document) -> bool {
        let mut changed = false;
        let revision = doc.model().revision();
        if revision != self.last_revision {
            self.last_revision = revision;
            self.last_change = Instant::now();
        }
        changed |= self.drain(doc);
        changed |= self.nudge_worker();
        changed |= self.check_timeout();
        changed |= self.maybe_start(doc);
        changed
    }

    /// Retry the interrupt once the worker has published its thread id.
    fn nudge_worker(&mut self) -> bool {
        if let Some(r) = &mut self.running {
            if (r.cancel_asked || r.timed_out) && !r.interrupted {
                let tid = r.tid.load(Ordering::SeqCst);
                if tid != 0 {
                    creabim_script::async_interrupt(tid);
                    r.interrupted = true;
                    return true;
                }
            }
        }
        false
    }

    fn drain(&mut self, doc: &mut Document) -> bool {
        let mut changed = false;
        while let Ok(done) = self.rx.try_recv() {
            changed = true;
            let stale = match &self.running {
                Some(r) => r.script_id != done.script_id || r.key != done.key,
                None => true,
            };
            // The slot frees on every arrival; a second interrupt is a no-op.
            let asked = self.running.as_ref().map(|r| (r.cancel_asked, r.timed_out));
            self.running = None;
            if stale {
                continue;
            }
            let (cancel_asked, timed_out) = asked.unwrap_or((false, false));
            let entry = self.entry(done.script_id);
            // Terminal for this key either way: without recording it,
            // the next poll sees a dirty script and respawns immediately.
            entry.last_key = Some(done.key.clone());
            let run_name = doc
                .model()
                .get(done.script_id)
                .map(|e| e.name.clone())
                .unwrap_or_default();
            match done.result {
                Ok((fresh, console)) => {
                    entry.console = truncate(console);
                    if timed_out {
                        entry.status = Status::TimedOut;
                        crate::actlog::line(format!("script {run_name}: timed out"));
                        eprintln!("[diag] script {run_name}: timed out");
                    } else if cancel_asked {
                        entry.status = Status::Cancelled;
                        crate::actlog::line(format!("script {run_name}: cancelled"));
                        eprintln!("[diag] script {run_name}: cancelled");
                    } else {
                        changed |= self.commit(doc, done.script_id, done.key, fresh, done.ms);
                    }
                }
                Err((traceback, console)) => {
                    entry.console = truncate(console);
                    if timed_out {
                        entry.status = Status::TimedOut;
                        crate::actlog::line(format!("script {run_name}: timed out"));
                        eprintln!("[diag] script {run_name}: timed out");
                    } else if cancel_asked {
                        entry.status = Status::Cancelled;
                        crate::actlog::line(format!("script {run_name}: cancelled"));
                        eprintln!("[diag] script {run_name}: cancelled");
                    } else {
                        entry.status = Status::Failed { traceback };
                        crate::actlog::line(format!("script {run_name}: failed"));
                        eprintln!("[diag] script {run_name}: failed");
                    }
                }
            }
        }
        changed
    }

    /// Commit fresh outputs when the script element is unchanged since
    /// the request; anything else means the user moved on — drop it.
    fn commit(
        &mut self,
        doc: &mut Document,
        script_id: ElementId,
        key: RequestKey,
        fresh: Vec<(String, Element)>,
        ms: u128,
    ) -> bool {
        let model = doc.model().clone();
        let script = match model.get(script_id) {
            Some(e) => match &e.kind {
                ElementKind::ScriptObject(s) => s.clone(),
                _ => {
                    return false;
                }
            },
            None => {
                return false;
            }
        };
        if request_key(&script) != key {
            return false;
        }
        if unchanged(&model, &script.outputs, &fresh) {
            let entry = self.entry(script_id);
            entry.last_key = Some(key);
            entry.last_revision = model.revision();
            entry.status = Status::Ok {
                ms,
                added: 0,
                replaced: 0,
                removed: 0,
            };
            return true;
        }
        let (tx, outputs) = reconcile(
            &model,
            &format!("script {}", script_id),
            &script.outputs,
            fresh,
        );
        let (added, replaced, removed) = counts(&tx);
        let mut updated = script.clone();
        updated.outputs = outputs.clone();
        let before = match model.get(script_id).cloned() {
            Some(e) => e,
            None => return false,
        };
        let mut after = before.clone();
        after.kind = ElementKind::ScriptObject(updated);
        let name = after.name.clone();
        let tx = tx.replace(before, after);
        let entry = self.entry(script_id);
        match doc.commit(tx) {
            Ok(()) => {
                entry.last_key = Some(key);
                entry.last_revision = doc.model().revision();
                entry.status = Status::Ok {
                    ms,
                    added,
                    replaced,
                    removed,
                };
                crate::actlog::line(format!(
                    "script {name}: ok in {ms} ms (+{added} ~{replaced} -{removed})"
                ));
                eprintln!("[diag] script {name}: ok in {ms} ms (+{added} ~{replaced} -{removed})");
            }
            Err(e) => {
                entry.status = Status::Failed {
                    traceback: format!("commit failed: {e}"),
                };
                crate::actlog::line(format!("script {name}: commit failed"));
                eprintln!("[diag] script {name}: commit failed");
            }
        }
        true
    }

    fn check_timeout(&mut self) -> bool {
        let now = Instant::now();
        let expired = match &self.running {
            Some(r) if !r.timed_out && now.duration_since(r.started) > self.timeout => true,
            Some(r) if r.timed_out => r
                .timed_out_at
                .is_some_and(|at| now.duration_since(at) > GRACE),
            _ => false,
        };
        if !expired {
            return false;
        }
        if let Some(r) = &mut self.running {
            if !r.timed_out {
                // The interrupt itself goes out in `nudge_worker`, which
                // retries once the worker publishes its thread id.
                r.timed_out = true;
                r.timed_out_at = Some(now);
                return true;
            }
            // Grace over, worker never answered: free the slot; a late
            // result finds no running request and is discarded.
            self.running = None;
            return true;
        }
        false
    }

    fn maybe_start(&mut self, doc: &mut Document) -> bool {
        if self.running.is_some() {
            return false;
        }
        let forced = self.queue.pop_front();
        let now = Instant::now();
        if forced.is_none()
            && now.duration_since(self.last_change) < DEBOUNCE
            && self.last_revision != 0
        {
            return false;
        }
        let model = doc.model();
        let revision = model.revision();
        // Forced runs jump the queue; otherwise scan for dirty scripts.
        let mut candidates: Vec<ElementId> = forced.into_iter().collect();
        if candidates.is_empty() {
            candidates.extend(model.iter().filter_map(|e| match &e.kind {
                ElementKind::ScriptObject(s) => {
                    let entry = self.entries.get(&e.id);
                    let key = request_key(s);
                    let known = entry.and_then(|en| en.last_key.clone());
                    let dirty = known != Some(key)
                        || (has_element_inputs(s)
                            && entry.map(|en| en.last_revision).unwrap_or(0) != revision);
                    dirty.then_some(e.id)
                }
                _ => None,
            }));
        }
        let Some(id) = candidates.into_iter().next() else {
            return false;
        };
        let script = match model.get(id).cloned() {
            Some(e) => match e.kind {
                ElementKind::ScriptObject(s) => s,
                _ => return false,
            },
            None => return false,
        };
        let key = request_key(&script);
        let decls = match script.declared() {
            Ok(d) => d,
            Err(e) => {
                let entry = self.entry(id);
                entry.status = Status::Failed { traceback: e };
                entry.last_key = Some(key);
                entry.last_revision = revision;
                return true;
            }
        };
        let inputs = match creabim_script::resolve(model, &decls, &script.inputs, &[]) {
            Ok(i) => i,
            Err(e) => {
                let entry = self.entry(id);
                entry.status = Status::Failed {
                    traceback: e.to_string(),
                };
                entry.last_key = Some(key);
                entry.last_revision = revision;
                return true;
            }
        };
        self.start(doc, id, key, script, inputs);
        true
    }

    fn start(
        &mut self,
        doc: &Document,
        id: ElementId,
        key: RequestKey,
        script: ScriptObject,
        inputs: ResolvedInputs,
    ) {
        let name = doc
            .model()
            .get(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        crate::actlog::line(format!("script {name}: recompute started"));
        // TEMP-DIAG: mirror to stderr so a frozen UI still leaves a trace.
        eprintln!("[diag] script {name}: recompute started");
        let snapshot = Arc::new(doc.model().clone());
        let tid = Arc::new(AtomicU64::new(0));
        let worker = tid.clone();
        let tx = self.tx.clone();
        let done_key = key.clone();
        std::thread::spawn(move || {
            creabim_geom::prio::background();
            let t0 = Instant::now();
            let outcome = (|| {
                let home = ensure_provisioned().map_err(|e| e.to_string())?;
                let reqs: Vec<&str> = script.requirements.iter().map(String::as_str).collect();
                if !reqs.is_empty() {
                    ensure_packages(&reqs).map_err(|e| e.to_string())?;
                }
                Ok::<_, String>(home)
            })();
            let result = match outcome {
                Err(e) => Err((format!("provisioning failed: {e}"), String::new())),
                Ok(home) => {
                    let engine = CPythonEngine::new(home);
                    let traced = engine.run_traced(&snapshot, &script, &inputs, &worker);
                    match traced.result {
                        Ok(fresh) => Ok((fresh, traced.console)),
                        Err(e) => Err((e.to_string(), traced.console)),
                    }
                }
            };
            let _ = tx.send(Done {
                script_id: id,
                key: done_key,
                result,
                ms: t0.elapsed().as_millis(),
            });
        });
        self.running = Some(Running {
            script_id: id,
            key,
            tid,
            started: Instant::now(),
            cancel_asked: false,
            interrupted: false,
            timed_out: false,
            timed_out_at: None,
        });
        self.entry(id).status = Status::Running {
            since: Instant::now(),
        };
    }
}

fn truncate(mut console: String) -> String {
    if console.len() > CONSOLE_CAP {
        console.truncate(CONSOLE_CAP);
        console.push_str("\n…[truncated]");
    }
    console
}

fn counts(tx: &Transaction) -> (usize, usize, usize) {
    let (mut added, mut replaced, mut removed) = (0, 0, 0);
    for op in &tx.ops {
        match op {
            creabim_core::Op::Insert(_) => added += 1,
            creabim_core::Op::Replace { .. } => replaced += 1,
            creabim_core::Op::Remove(_) => removed += 1,
        }
    }
    (added, replaced, removed)
}

/// Every fresh output already present unchanged: skip the commit so a
/// no-op re-run leaves no history behind (and no regen loop follows).
fn unchanged(model: &Model, prev: &[ScriptOutput], fresh: &[(String, Element)]) -> bool {
    if fresh.len() != prev.len() {
        return false;
    }
    // Compare content, not identity: fresh elements carry new ids that
    // reconcile() maps onto the stored ones. Comparing ids would make
    // every re-run look changed and commit forever.
    fresh.iter().all(|(key, el)| {
        prev.iter()
            .find(|o| &o.key == key)
            .and_then(|o| model.get(o.id).map(|cur| (o, cur)))
            .is_some_and(|(o, cur)| {
                let mut el = el.clone();
                el.id = o.id;
                cur == &el
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::Level;

    const PARTITIONS: &str = "# in: bays: int = 3\nOUTPUTS = [{\"key\": f\"w-{i}\", \"wall\": {\"level\": \"Ground\", \"start\": [float(i), 0.0], \"end\": [float(i), 4.0], \"thickness\": 0.1}} for i in range(INPUTS[\"bays\"] + 1)]\n";

    fn doc_with_script(source: &str) -> (Document, ElementId) {
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
            ElementKind::ScriptObject(ScriptObject::new(source, "cpython")),
        );
        let id = script.id;
        doc.commit(Transaction::new("script").insert(script))
            .unwrap();
        (doc, id)
    }

    fn walls(doc: &Document) -> usize {
        doc.model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
            .count()
    }

    fn pump_until(
        runner: &mut ScriptRunner,
        doc: &mut Document,
        id: ElementId,
        secs: u64,
        done: impl Fn(&ScriptRunner, &Document) -> bool,
    ) {
        let start = Instant::now();
        let mut last = String::new();
        loop {
            runner.poll(doc);
            if done(runner, doc) {
                return;
            }
            let st = runner.status_string(id);
            if st != last {
                eprintln!("[test] status -> {st}");
                last = st;
            }
            if start.elapsed() > Duration::from_secs(secs) {
                panic!("runner settled on nothing new (last: {last})");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn runs_to_completion_and_stays_quiet_after() {
        let (mut doc, id) = doc_with_script(PARTITIONS);
        let mut runner = ScriptRunner::default();
        pump_until(&mut runner, &mut doc, id, 120, |r, d| {
            walls(d) == 4 && matches!(r.status(id), Status::Ok { .. })
        });
        let revision = doc.model().revision();
        // Keep polling a settled model: no re-runs, no new history.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            runner.poll(&mut doc);
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(walls(&doc), 4);
        assert_eq!(doc.model().revision(), revision);
    }

    #[test]
    fn cancel_stops_a_runaway() {
        let (mut doc, id) = doc_with_script("while True:\n    pass\n");
        let mut runner = ScriptRunner::default();
        pump_until(&mut runner, &mut doc, id, 30, |r, _| {
            matches!(r.status(id), Status::Running { .. })
        });
        runner.cancel(id);
        pump_until(&mut runner, &mut doc, id, 30, |r, _| {
            matches!(r.status(id), Status::Cancelled)
        });
        assert_eq!(walls(&doc), 0);
    }

    #[test]
    fn timeout_gives_up_on_a_runaway() {
        let (mut doc, id) = doc_with_script("while True:\n    pass\n");
        let mut runner = ScriptRunner::with_timeout(Duration::from_secs(2));
        pump_until(&mut runner, &mut doc, id, 30, |r, _| {
            matches!(r.status(id), Status::TimedOut)
        });
        assert_eq!(walls(&doc), 0);
    }
}
