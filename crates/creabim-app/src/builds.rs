//! Drawings and the 3D scene built off the UI thread (Addendum A6-1.2).
//!
//! A view whose drawing takes long to build (sections and elevations:
//! 100-450 ms on the starter house) is rebuilt on a worker thread when the
//! model changes. Its area keeps showing the last finished drawing until
//! the new one arrives, and the app is woken then. A build in flight for
//! an older revision is let finish (its drawing is still newer than the
//! one shown); the next build starts from the model as it is by then, so
//! a drag gets as many updates as the builds allow and never waits for
//! them. Cheap drawings (plans, about 15 ms) are still built on the spot.

use creabim_core::{ElementId, Model};
use creabim_geom::{section::Drawing, SceneItem};
use eframe::egui;
use std::collections::HashMap;
use std::sync::{mpsc, Arc};
use std::time::Instant;

/// Drawings that took less than this to build are built on the spot.
pub const CHEAP_MS: f64 = 20.0;

/// A view's drawing (or x-ray drawing), as a build key.
pub type Key = (ElementId, bool);

enum Done {
    Drawing {
        key: Key,
        revision: u64,
        drawing: Option<Drawing>,
        ms: f64,
    },
    Scene {
        revision: u64,
        scene: Arc<Vec<SceneItem>>,
    },
}

/// A finished drawing, for the drawing caches.
pub struct Built {
    pub key: Key,
    pub revision: u64,
    pub drawing: Option<Drawing>,
}

pub struct Builds {
    tx: mpsc::Sender<Done>,
    rx: mpsc::Receiver<Done>,
    /// Builds running, by key, with the revision each started from.
    running: HashMap<Key, u64>,
    /// How long each view's drawing last took to build, in ms.
    cost: HashMap<Key, f64>,
    /// The model as of a revision, shared with the workers.
    snapshot: Option<(u64, Arc<Model>)>,
    /// The revision a 3D scene is being built for.
    scene_running: Option<u64>,
    /// The newest scene a worker finished.
    scene_done: Option<(u64, Arc<Vec<SceneItem>>)>,
    /// Drawings received and not yet handed over.
    pending: Vec<Built>,
    /// To wake the app when a build is done.
    ctx: Option<egui::Context>,
}

impl Default for Builds {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            running: HashMap::new(),
            cost: HashMap::new(),
            snapshot: None,
            scene_running: None,
            scene_done: None,
            pending: vec![],
            ctx: None,
        }
    }
}

impl Builds {
    /// Without a context nothing is built off the UI thread (tests).
    pub fn set_context(&mut self, ctx: egui::Context) {
        self.ctx = Some(ctx);
    }

    fn snapshot(&mut self, model: &Model) -> Arc<Model> {
        let rev = model.revision();
        match &self.snapshot {
            Some((r, m)) if *r == rev => m.clone(),
            _ => {
                let m = Arc::new(model.clone());
                self.snapshot = Some((rev, m.clone()));
                m
            }
        }
    }

    /// Note how long `key`'s drawing took to build on the spot.
    pub fn record(&mut self, key: Key, ms: f64) {
        self.cost.insert(key, ms);
    }

    /// Is `key`'s drawing cheap enough to build on the spot? Unknown ones
    /// are not: the first build tells.
    pub fn cheap(&self, key: Key) -> bool {
        self.cost.get(&key).is_some_and(|ms| *ms < CHEAP_MS)
    }

    /// Can work go to a worker thread at all?
    pub fn enabled(&self) -> bool {
        self.ctx.is_some()
    }

    /// Build `key`'s drawing of `model` on a worker, unless one is
    /// running already.
    pub fn start(&mut self, model: &Model, key: Key) {
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        if self.running.contains_key(&key) {
            return;
        }
        let revision = model.drawing_revision();
        let snapshot = self.snapshot(model);
        let tx = self.tx.clone();
        self.running.insert(key, revision);
        if self.running.len() > 1 {
            crate::actlog::line(format!(
                "worker: {} drawings building at once",
                self.running.len()
            ));
        }
        std::thread::spawn(move || {
            creabim_geom::prio::background();
            let t = Instant::now();
            let drawing = creabim_geom::section::view_drawing_with(&snapshot, key.0, key.1);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let _ = tx.send(Done::Drawing {
                key,
                revision,
                drawing,
                ms,
            });
            ctx.request_repaint();
        });
    }

    /// Build the 3D scene of `model` on a worker (it lands in the shared
    /// scene cache too), unless one is running already.
    pub fn start_scene(&mut self, model: &Model) {
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        if self.scene_running.is_some() {
            return;
        }
        let revision = model.revision();
        let snapshot = self.snapshot(model);
        let tx = self.tx.clone();
        self.scene_running = Some(revision);
        std::thread::spawn(move || {
            creabim_geom::prio::background();
            let scene = creabim_geom::shared_scene(&snapshot);
            let _ = tx.send(Done::Scene { revision, scene });
            ctx.request_repaint();
        });
    }

    /// The newest scene a worker has finished, with its revision.
    pub fn latest_scene(&self) -> Option<(u64, Arc<Vec<SceneItem>>)> {
        self.scene_done.clone()
    }

    /// Take in what the workers have finished: scenes now, drawings for
    /// [`Self::finished`].
    pub fn poll(&mut self) {
        while let Ok(done) = self.rx.try_recv() {
            match done {
                Done::Drawing {
                    key,
                    revision,
                    drawing,
                    ms,
                } => {
                    self.running.remove(&key);
                    self.cost.insert(key, ms);
                    // what the workers did, for problem reports (MU40)
                    if ms >= crate::actlog::SLOW_MS {
                        crate::actlog::line(format!(
                            "worker: drawing {:?}{} built in {ms:.0} ms",
                            key.0,
                            if key.1 { " (x-ray)" } else { "" }
                        ));
                    }
                    self.pending.push(Built {
                        key,
                        revision,
                        drawing,
                    });
                }
                Done::Scene { revision, scene } => {
                    self.scene_running = None;
                    if self.scene_done.as_ref().is_none_or(|(r, _)| *r < revision) {
                        self.scene_done = Some((revision, scene));
                    }
                }
            }
        }
    }

    /// The drawings finished since the last call.
    pub fn finished(&mut self) -> Vec<Built> {
        self.poll();
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{Document, ElementKind, Transaction};

    /// A drag with 3D, two sections and an elevation open: each frame
    /// changes the model and asks for every drawing. Asking never waits
    /// for a build, and when the drag stops the drawings of the final
    /// model arrive.
    #[test]
    fn frames_never_wait_for_heavy_drawings() {
        let mut doc = Document::from_model(creabim_core::starter::starter_house());
        let views: Vec<ElementId> = doc
            .model()
            .iter()
            .filter(|e| {
                matches!(e.kind, ElementKind::View(_))
                    && ["Section A-A", "Section B-B", "South Elevation"].contains(&e.name.as_str())
            })
            .map(|e| e.id)
            .collect();
        assert_eq!(views.len(), 3);
        let wall = doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap()
            .clone();
        let mut b = Builds::default();
        b.set_context(egui::Context::default());
        let mut worst = 0.0f64;
        for frame in 0..30 {
            let mut moved = doc.model().get(wall.id).unwrap().clone();
            if let ElementKind::Wall(w) = &mut moved.kind {
                w.height += 0.001 * frame as f64;
            }
            let before = doc.model().get(wall.id).unwrap().clone();
            doc.commit(Transaction::new("drag").replace(before, moved))
                .unwrap();
            let t = Instant::now();
            for v in &views {
                b.start(doc.model(), (*v, false));
            }
            b.start_scene(doc.model());
            b.poll();
            worst = worst.max(t.elapsed().as_secs_f64() * 1000.0);
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        // a frame's asking costs the model snapshot at most. Shared CI
        // machines stall now and then for reasons of their own, so there
        // the bound is looser: it still catches a frame that builds
        let bound = if std::env::var_os("CI").is_some() {
            60.0
        } else {
            16.0
        };
        assert!(worst < bound, "a frame waited {worst:.1} ms");
        let last = doc.model().drawing_revision();
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        let mut newest: HashMap<Key, u64> = HashMap::new();
        while Instant::now() < deadline {
            for d in b.finished() {
                newest.insert(d.key, d.revision);
            }
            for v in &views {
                b.start(doc.model(), (*v, false));
            }
            if views
                .iter()
                .all(|v| newest.get(&(*v, false)) == Some(&last))
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the final drawings never arrived: {newest:?}, wanted {last}");
    }
}
