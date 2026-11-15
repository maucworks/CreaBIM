//! The site layers editor: one linked GeoPackage as one package.
//!
//! GPKG-sourced geography is a special breed: the layers move, rotate
//! and reload together, never one drag at a time. Direct edits are
//! locked (the commit guard rejects them); this editor is the deliberate
//! path — per-layer visibility, package move/rotate with typed values,
//! reload from disc, and unlink.

use super::gridvis::VisTarget;
use super::*;
use creabim_core::{GeoPlacement, SiteFeature, SiteGeometry};

/// One linked file's layers, grouped for the editor.
pub struct SitePackage {
    pub file: String,
    pub layers: Vec<SiteLayer>,
}

pub struct SiteLayer {
    pub id: ElementId,
    pub name: String,
    pub category: String,
    pub features: usize,
    pub area: f64,
}

/// The linked packages in the model, layers in model order.
pub fn site_packages(model: &Model) -> Vec<SitePackage> {
    let mut out: Vec<SitePackage> = vec![];
    for e in model.iter() {
        let (ElementKind::SiteFeature(s), Some(src)) = (&e.kind, &e.source) else {
            continue;
        };
        let pkg = match out.iter_mut().find(|p| p.file == src.file) {
            Some(p) => p,
            None => {
                out.push(SitePackage {
                    file: src.file.clone(),
                    layers: vec![],
                });
                out.last_mut().unwrap()
            }
        };
        pkg.layers.push(SiteLayer {
            id: e.id,
            name: e.name.clone(),
            category: format!("{:?}", s.category),
            features: s.features.len(),
            area: s.area(),
        });
    }
    out
}

/// The editor window's own state, for the session.
#[derive(Default)]
pub(super) struct SiteUi {
    pub open: bool,
    /// Metres to move the package by, and degrees to turn it.
    shift: [f64; 2],
    turn: f64,
}

impl CreaBimApp {
    pub(super) fn open_site(&mut self) {
        self.site.open = true;
        self.site.shift = [0.0, 0.0];
        self.site.turn = 0.0;
    }

    /// Where visibility toggles go: the active view, or the 3D window.
    fn site_target(&self) -> Option<VisTarget> {
        match self.active_view {
            Some(id)
                if matches!(
                    self.doc.model().get(id).map(|e| &e.kind),
                    Some(ElementKind::View(_))
                ) =>
            {
                Some(VisTarget::View(id))
            }
            _ => Some(VisTarget::Window3d),
        }
    }

    fn site_target_name(&self) -> String {
        match self.site_target() {
            Some(VisTarget::View(id)) => self
                .doc
                .model()
                .get(id)
                .map(|e| e.name.clone())
                .unwrap_or_default(),
            _ => "the 3D view".into(),
        }
    }

    /// Move every layer of one linked file by metres.
    fn move_site_package(&mut self, file: &str, dx: f64, dy: f64) {
        let file = file.to_string();
        let mut tx = creabim_core::Transaction::new(format!("site move {}", stem(&file)));
        for e in self.doc.model().iter() {
            let (ElementKind::SiteFeature(s), Some(src)) = (&e.kind, &e.source) else {
                continue;
            };
            if src.file != file {
                continue;
            }
            let mut after = e.clone();
            if let ElementKind::SiteFeature(ns) = &mut after.kind {
                for f in &mut ns.features {
                    let at = |p: DVec2| DVec2::new(p.x + dx, p.y + dy);
                    match &mut f.geom {
                        SiteGeometry::Polygon { exterior, holes } => {
                            exterior.iter_mut().for_each(|p| *p = at(*p));
                            for h in holes {
                                h.iter_mut().for_each(|p| *p = at(*p));
                            }
                        }
                        SiteGeometry::Polyline { points } => {
                            points.iter_mut().for_each(|p| *p = at(*p));
                        }
                    }
                }
            }
            tx = tx.replace(e.clone(), after);
        }
        let r = self.doc.commit(tx);
        self.report(
            r,
            format!("Site package moved by ({dx}, {dy}) m; the next reload keeps the shift."),
        );
    }

    /// Turn every layer of one linked file round the model origin.
    fn rotate_site_package(&mut self, file: &str, degrees: f64) {
        let file = file.to_string();
        let t = degrees.to_radians();
        let (c, s) = (t.cos(), t.sin());
        let mut tx = creabim_core::Transaction::new(format!("site rotate {}", stem(&file)));
        for e in self.doc.model().iter() {
            let (ElementKind::SiteFeature(_), Some(src)) = (&e.kind, &e.source) else {
                continue;
            };
            if src.file != file {
                continue;
            }
            let mut after = e.clone();
            if let ElementKind::SiteFeature(ns) = &mut after.kind {
                for f in &mut ns.features {
                    let at = |p: DVec2| DVec2::new(p.x * c - p.y * s, p.x * s + p.y * c);
                    match &mut f.geom {
                        SiteGeometry::Polygon { exterior, holes } => {
                            exterior.iter_mut().for_each(|p| *p = at(*p));
                            for h in holes {
                                h.iter_mut().for_each(|p| *p = at(*p));
                            }
                        }
                        SiteGeometry::Polyline { points } => {
                            points.iter_mut().for_each(|p| *p = at(*p));
                        }
                    }
                }
            }
            tx = tx.replace(e.clone(), after);
        }
        let r = self.doc.commit(tx);
        self.report(
            r,
            format!("Site package turned by {degrees}° round the model origin."),
        );
    }

    /// Re-read one linked file and sync by source row-id.
    fn reload_site_package(&mut self, file: &str) {
        let file = file.to_string();
        let placement = self.doc.model().project_info().and_then(|(_, p)| p.placement());
        let Some(g) = placement else {
            self.status = "Site reload needs a placed project (project place first).".into();
            return;
        };
        match creabim_exchange::site::sync_site(self.doc.model(), &file, &g) {
            Ok((tx, report)) => {
                let r = self.doc.commit(tx);
                self.report(
                    r,
                    format!(
                        "Site {}: {} added, {} updated, {} removed.",
                        stem(&file),
                        report.added,
                        report.updated,
                        report.removed
                    ),
                );
            }
            Err(e) => {
                self.status = format!("Site reload failed: {e}");
            }
        }
    }

    /// Drop every element of one linked file.
    fn unlink_site_package(&mut self, file: &str) {
        let file = file.to_string();
        let mut tx = creabim_core::Transaction::new(format!("site unlink {}", stem(&file)));
        for e in self.doc.model().iter() {
            if matches!(&e.kind, ElementKind::SiteFeature(_))
                && e.source.as_ref().is_some_and(|s| s.file == file)
            {
                tx = tx.remove_cascade(self.doc.model(), e.id);
            }
        }
        let r = self.doc.commit(tx);
        self.report(r, format!("Site {} unlinked.", stem(&file)));
    }

    /// The editor window: one package section per linked file.
    pub(super) fn site_window(&mut self, ctx: &egui::Context) {
        let mut open = self.site.open;
        let mut force = None;
        let mut reload = None;
        let mut unlink = None;
        let mut shift: Option<(String, f64, f64)> = None;
        let mut turn: Option<(String, f64)> = None;
        crate::app::shell::window("Site layers")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "A linked GeoPackage draws as one underlay. Layers move, turn and reload together — never one drag at a time.",
                    )
                    .weak(),
                );
                let target = self.site_target();
                let target_name = self.site_target_name();
                for pkg in site_packages(self.doc.model()) {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.heading(stem(&pkg.file));
                        ui.label(
                            egui::RichText::new(format!(
                                "{} layers, {} features",
                                pkg.layers.len(),
                                pkg.layers.iter().map(|l| l.features).sum::<usize>()
                            ))
                            .weak(),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Reload").clicked() {
                            reload = Some(pkg.file.clone());
                        }
                        if ui.small_button("Unlink").clicked() {
                            unlink = Some(pkg.file.clone());
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Move m:");
                        ui.add(
                            egui::DragValue::new(&mut self.site.shift[0])
                                .speed(1.0)
                                .prefix("dx "),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.site.shift[1])
                                .speed(1.0)
                                .prefix("dy "),
                        );
                        if ui.small_button("Move package").clicked() {
                            shift = Some((
                                pkg.file.clone(),
                                self.site.shift[0],
                                self.site.shift[1],
                            ));
                        }
                        ui.add(
                            egui::DragValue::new(&mut self.site.turn)
                                .speed(0.5)
                                .suffix("°"),
                        );
                        if ui.small_button("Rotate").clicked() {
                            turn = Some((pkg.file.clone(), self.site.turn));
                        }
                    });
                    ui.label(format!("Eye toggles hide in {target_name}."));
                    for l in &pkg.layers {
                        ui.horizontal(|ui| {
                            let hidden = target.is_some_and(|t| {
                                self.view_hidden(t).is_some_and(|h| {
                                    h.elements.iter().any(|id| *id == l.id)
                                })
                            });
                            let mut eye = !hidden;
                            if ui.checkbox(&mut eye, "").clicked() {
                                if let Some(t) = target {
                                    let id = l.id;
                                    let name = l.name.clone();
                                    self.set_view_hidden(t, &format!("Site layer {name}"), |h| {
                                        h.set_element(id, !eye);
                                    });
                                }
                            }
                            ui.label(format!(
                                "{} · {} · {} · {:.0} m²",
                                l.name, l.category, l.features, l.area
                            ));
                        });
                    }
                    let _ = force;
                }
                if site_packages(self.doc.model()).is_empty() {
                    ui.label("No linked site layers. Link a GeoPackage with `site load`.");
                }
            });
        if let Some(file) = reload {
            self.reload_site_package(&file);
        }
        if let Some(file) = unlink {
            self.unlink_site_package(&file);
        }
        if let Some((file, dx, dy)) = shift {
            self.move_site_package(&file, dx, dy);
        }
        if let Some((file, deg)) = turn {
            self.rotate_site_package(&file, deg);
        }
        self.site.open = open;
    }
}

/// File name without folders, for headers and undo labels.
fn stem(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}
