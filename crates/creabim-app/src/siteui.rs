//! The site layers editor: one linked GeoPackage as one package.
//!
//! GPKG-sourced geography is a special breed: the layers move, rotate
//! and reload together, never one drag at a time. Direct edits are
//! locked (the commit guard rejects them); this editor is the deliberate
//! path — per-layer visibility, package move/rotate with typed values,
//! reload from disc, and unlink.

use super::gridvis::VisTarget;
use super::planview::{SitePick, SitePickHit};
use super::*;
use creabim_core::{Cardinal, LayerStyle, ProjectInfo, SiteGeometry};
use creabim_core::styles::{LinePattern, Rgb};

/// One linked file's layers, for the editor: the file element and
/// its layers in load order.
pub struct SitePackage {
    pub file: String,
    pub id: ElementId,
    pub layers: Vec<SiteLayerRow>,
}

pub struct SiteLayerRow {
    pub name: String,
    pub category: String,
    pub features: usize,
    pub area: f64,
    /// How it draws, project-wide: what the editor's widgets sit on.
    pub style: LayerStyle,
}

/// The linked files in the model, in model order: one homogenous
/// element per file.
pub fn site_packages(model: &Model) -> Vec<SitePackage> {
    let mut out: Vec<SitePackage> = vec![];
    for e in model.iter() {
        let (ElementKind::SiteFeature(s), Some(src)) = (&e.kind, &e.source) else {
            continue;
        };
        out.push(SitePackage {
            file: src.file.clone(),
            id: e.id,
            layers: s
                .layers()
                .iter()
                .map(|l| SiteLayerRow {
                    name: l.layer.clone(),
                    category: format!("{:?}", l.category),
                    features: l.features.len(),
                    area: l.area(),
                    style: l.style.clone(),
                })
                .collect(),
        });
    }
    out
}

/// The editor's own state, for the session.
#[derive(Default)]
pub(super) struct SiteUi {
    /// Metres to move the package by, and degrees to turn it.
    shift: [f64; 2],
    turn: f64,
    /// The file the editor was opened for (from its Properties): its
    /// section draws highlighted.
    focus: Option<String>,
}

impl CreaBimApp {
    /// Open Settings on the Site tab — the editor lives there, there
    /// is no separate panel. `focus` highlights one file's section,
    /// from its Properties.
    pub(super) fn open_site_settings(&mut self, focus: Option<String>) {
        self.settings_open = true;
        self.settings_tab = super::appmenu::SITE_TAB;
        self.site.focus = focus;
        self.site.shift = [0.0, 0.0];
        self.site.turn = 0.0;
    }

    /// One layer's style inside the file element, written under a
    /// `site …` label: the only writes the commit guard lets through
    /// for linked geography.
    fn apply_layer_style(&mut self, id: ElementId, layer: &str, style: LayerStyle) {
        let Some(e) = self.doc.model().get(id).cloned() else {
            crate::app::diag(&format!("style: element {id:?} gone"));
            return;
        };
        let mut after = e.clone();
        if let ElementKind::SiteFeature(s) = &mut after.kind {
            let Some(l) = s.layers_mut().into_iter().find(|l| l.layer == layer) else {
                crate::app::diag(&format!("style: layer {layer:?} not in element"));
                return;
            };
            l.style = style;
        }
        let file = e
            .source
            .as_ref()
            .map(|s| stem(&s.file).to_string())
            .unwrap_or_default();
        let tx =
            creabim_core::Transaction::new(format!("site style {file}")).replace(e, after);
        let r = self.doc.commit(tx);
        crate::app::diag(&format!("style: commit {layer:?} ok={}", r.is_ok()));
        self.report(r, "Layer style set: it draws so in every view.".to_string());
    }

    /// Link the GeoPackage at `path`: the project takes the file's CRS
    /// when it has no location yet (the origin starts at the extent's
    /// middle), the layers load onto it, and picking finishes the
    /// placement. Creation lives in File › Import and link.
    pub(super) fn link_geopackage_path(&mut self, path: &str) {
        crate::app::diag(&format!("link {path}"));
        let path = path.to_string();
        if self.doc.model().iter().any(|e| {
            matches!(&e.kind, ElementKind::SiteFeature(_))
                && e.source.as_ref().is_some_and(|s| s.file == path)
        }) {
            crate::app::diag("already linked");
            self.status = format!("{} is already linked.", stem(&path));
            return;
        }
        let geo = match creabim_exchange::site::file_geo(&path) {
            Ok(g) => {
                crate::app::diag(&format!(
                    "file geo: {} bounds {:?}",
                    g.crs,
                    g.bounds.map(|(x0, y0, x1, y1)| {
                        [x0.round(), y0.round(), x1.round(), y1.round()]
                    })
                ));
                g
            }
            Err(e) => {
                crate::app::diag(&format!("file_geo failed: {e}"));
                self.status = format!("Site link failed: {e}");
                return;
            }
        };
        let info = self
            .doc
            .model()
            .project_info()
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        match info.placement() {
            None => {
                let (cx, cy) = geo
                    .bounds
                    .map(|(x0, y0, x1, y1)| ((x0 + x1) / 2.0, (y0 + y1) / 2.0))
                    .unwrap_or((0.0, 0.0));
                crate::app::diag(&format!("placing project at extent middle ({cx:.2}, {cy:.2})"));
                let placed = ProjectInfo {
                    srid: Some(geo.crs.clone()),
                    easting: Some(cx),
                    northing: Some(cy),
                    ..info.clone()
                };
                let tx = creabim_core::set_project(self.doc.model(), placed);
                let stem = stem(&path).to_string();
                let crs = geo.crs.clone();
                let ok = self.doc.commit(tx).is_ok();
                crate::app::diag(&format!("placement commit ok={ok}"));
                self.status = if ok {
                    format!("{stem} linked: the project is placed in {crs} at the extent's middle. Pick zero and a north line to finish.")
                } else {
                    format!("{stem} linked but project placement would not commit.")
                };
            }
            Some(p) if p.srid != geo.crs => {
                crate::app::diag(&format!("CRS mismatch: file {} vs project {}", geo.crs, p.srid));
                self.status = format!(
                    "Site link failed: {} is in {}, the project is placed in {}. Reprojecting on load is out.",
                    stem(&path),
                    geo.crs,
                    p.srid
                );
                return;
            }
            Some(_) => {
                crate::app::diag("project already placed, loading onto it");
            }
        }
        self.reload_site_package(&path);
    }

    /// The site editor's picks, applied: the picked point becomes the
    /// origin, the picked line turns the project to point the cardinal
    /// it was called; every linked layer re-projects onto the new
    /// placement at once, so site and GIS stay synched.
    pub(super) fn site_steps(&mut self) {
        let Some(hit) = self.plan.site_picked.take() else {
            return;
        };
        crate::app::diag(&format!("pick hit: {hit:?}"));
        let info = self
            .doc
            .model()
            .project_info()
            .map(|(_, p)| p.clone());
        let Some(info) = info else {
            self.status = "The project has no location: link a GeoPackage first.".into();
            return;
        };
        let Some(placement) = info.placement() else {
            self.status = "The project has no location: link a GeoPackage first.".into();
            return;
        };
        let (after, said) = match hit {
            SitePickHit::Zero(p) => {
                let world = placement.world_of(p);
                (
                    ProjectInfo {
                        easting: Some(world.x),
                        northing: Some(world.y),
                        ..info.clone()
                    },
                    format!(
                        "Zero set: the picked point is model (0,0) from E {:.2}, N {:.2}. The layers re-project onto it.",
                        world.x, world.y
                    ),
                )
            }
            SitePickHit::Line(cardinal, a, b) => {
                let d = b - a;
                let world =
                    placement.world_of(d) - placement.world_of(DVec2::ZERO);
                let Some(rot) = placement.rotation_towards(world, cardinal) else {
                    self.status = "Site pick failed: the picked line has no direction.".into();
                    return;
                };
                (
                    ProjectInfo {
                        true_north: rot,
                        ..info.clone()
                    },
                    format!(
                        "North line set: the picked line runs {} now, the project turned to {rot:.2}°. The layers re-project with it.",
                        cardinal.label().to_lowercase()
                    ),
                )
            }
        };
        let tx = creabim_core::set_project(self.doc.model(), after);
        if self.doc.commit(tx).is_ok() {
            self.status = said;
            let mut files: Vec<String> = self
                .doc
                .model()
                .iter()
                .filter_map(|e| {
                    matches!(&e.kind, ElementKind::SiteFeature(_))
                        .then(|| e.source.as_ref().map(|s| s.file.clone()))
                        .flatten()
                })
                .collect();
            files.sort();
            files.dedup();
            for f in files {
                self.reload_site_package(&f);
            }
        }
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
            let (ElementKind::SiteFeature(_), Some(src)) = (&e.kind, &e.source) else {
                continue;
            };
            if src.file != file {
                continue;
            }
            let mut after = e.clone();
            if let ElementKind::SiteFeature(ns) = &mut after.kind {
                for l in ns.layers_mut() {
                    for f in &mut l.features {
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
                for l in ns.layers_mut() {
                    for f in &mut l.features {
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
        crate::app::diag(&format!("reload {file}"));
        let placement = self.doc.model().project_info().and_then(|(_, p)| p.placement());
        let Some(g) = placement else {
            crate::app::diag("reload needs a placed project");
            self.status = "Site reload needs a placed project (project place first).".into();
            return;
        };
        match creabim_exchange::site::sync_site(self.doc.model(), &file, &g) {
            Ok((tx, report)) => {
                crate::app::diag(&format!(
                    "sync: {} added, {} updated, {} removed, {} unchanged, {} skipped layers",
                    report.added,
                    report.updated,
                    report.removed,
                    report.unchanged,
                    report.skipped_layers.len()
                ));
                let r = self.doc.commit(tx);
                crate::app::diag(&format!("sync commit ok={}", r.is_ok()));
                let mut said = format!(
                    "Site {}: {} added, {} updated, {} removed.",
                    stem(&file),
                    report.added,
                    report.updated,
                    report.removed
                );
                if !report.skipped_layers.is_empty() {
                    said += &format!(
                        " Skipped: {}.",
                        report.skipped_layers.join("; ")
                    );
                }
                for n in &report.notes {
                    said += &format!(" {n}");
                }
                self.report(r, said);
            }
            Err(e) => {
                crate::app::diag(&format!("sync failed: {e}"));
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

    /// The editor: the project location, one package section per
    /// linked file, its layers editable. Lives in Settings › Site.
    pub(super) fn site_editor(&mut self, ui: &mut egui::Ui) {

        let mut reload = None;
        let mut unlink = None;
        let mut shift: Option<(String, f64, f64)> = None;
        let mut turn: Option<(String, f64)> = None;
        let mut style_edits: Vec<(ElementId, String, LayerStyle)> = vec![];
        let mut pick: Option<SitePick> = None;
        ui.label(
            egui::RichText::new(
                "A linked GeoPackage draws as one underlay. Layers move, turn and reload together — never one drag at a time. Links are created in File › Import and link….",
            )
            .weak(),
        );
        // The project's location: what every linked file and the
        // model share — one origin, one north.
        match self.doc.model().project_info().and_then(|(_, p)| p.placement()) {
            Some(g) => {
                ui.separator();
                ui.heading("Location");
                ui.label(format!(
                    "{} · origin E {:.2}, N {:.2} · north turned {:.1}°",
                    g.srid, g.easting, g.northing, g.rotation_deg
                ));
                crate::props::flow(ui, |ui| {
                    if ui
                        .button("Set zero: pick a point…")
                        .on_hover_text("Click a point of the geography on a plan; it becomes the model origin (0,0), and every layer re-projects onto it.")
                        .clicked()
                    {
                        pick = Some(SitePick::Zero);
                    }
                    ui.label("North line:");
                    for c in Cardinal::ALL {
                        let letter = match c {
                            Cardinal::North => "N",
                            Cardinal::East => "E",
                            Cardinal::South => "S",
                            Cardinal::West => "W",
                        };
                        if ui
                            .small_button(letter)
                            .on_hover_text(format!(
                                "Pick a line of the geography; the project turns until it runs {}.",
                                c.label().to_lowercase()
                            ))
                            .clicked()
                        {
                            pick = Some(SitePick::Line(c));
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "Picks read the layers as last loaded; Reload first if you moved the location by hand.",
                    )
                    .weak(),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new(
                        "The project has no location yet: linking a GeoPackage places it in the file's CRS.",
                    )
                    .weak(),
                );
            }
        }
        let mut packages = site_packages(self.doc.model());
        for pkg in &mut packages {
            ui.separator();
            let focused = self.site.focus.as_deref() == Some(pkg.file.as_str());
            ui.horizontal(|ui| {
                if focused {
                    ui.heading(
                        egui::RichText::new(stem(&pkg.file))
                            .color(crate::theme::interaction()),
                    );
                } else {
                    ui.heading(stem(&pkg.file));
                }
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
            crate::props::flow(ui, |ui| {
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
            ui.label(format!("Eye toggles hide in {}.", self.site_target_name()));
            for l in &mut pkg.layers {
                ui.horizontal(|ui| {
                    let target = self.site_target();
                    let hidden = target.is_some_and(|t| {
                        self.view_hidden(t).is_some_and(|h| {
                            h.elements.contains(&pkg.id)
                        })
                    });
                    let mut eye = !hidden;
                    if ui
                        .checkbox(&mut eye, "")
                        .on_hover_text("Hide the whole file in the active view (or the 3D window) only")
                        .clicked()
                    {
                        if let Some(t) = target {
                            let id = pkg.id;
                            let name = stem(&pkg.file).to_string();
                            self.set_view_hidden(t, &format!("Site file {name}"), |h| {
                                h.set_element(id, !eye);
                            });
                        }
                    }
                    if ui
                        .checkbox(&mut l.style.visible, "")
                        .on_hover_text("Show the layer in every view (the project's own visibility)")
                        .changed()
                    {
                        style_edits.push((pkg.id, l.name.clone(), l.style.clone()));
                    }
                    ui.label(format!(
                        "{} · {} · {} · {:.0} m²",
                        l.name, l.category, l.features, l.area
                    ));
                });
                let layer_name = l.name.clone();
                crate::props::flow(ui, |ui| {
                    ui.label("Fill");
                    let mut fill = srgb(l.style.fill);
                    if ui
                        .color_edit_button_srgb(&mut fill)
                        .on_hover_text("The colour under the category's hatch")
                        .changed()
                    {
                        l.style.fill = rgb(fill);
                        style_edits.push((pkg.id, layer_name.clone(), l.style.clone()));
                    }
                    ui.label("Line");
                    let mut line = srgb(l.style.line);
                    if ui
                        .color_edit_button_srgb(&mut line)
                        .on_hover_text("The outlines' and polylines' colour")
                        .changed()
                    {
                        l.style.line = rgb(line);
                        style_edits.push((pkg.id, layer_name.clone(), l.style.clone()));
                    }
                    ui.label("Line type");
                    let mut pattern = l.style.pattern.clone();
                    egui::ComboBox::from_id_salt(("site.line", (pkg.id, l.name.clone())))
                        .selected_text(
                            pattern
                                .as_ref()
                                .map_or("Pen".to_string(), |p| p.label().to_string()),
                        )
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(pattern.is_none(), "Pen").clicked() {
                                pattern = None;
                            }
                            for p in LinePattern::NAMED {
                                if matches!(p, LinePattern::Custom(_)) {
                                    continue;
                                }
                                if ui
                                    .selectable_label(
                                        pattern.as_ref() == Some(&p),
                                        p.label(),
                                    )
                                    .clicked()
                                {
                                    pattern = Some(p);
                                }
                            }
                        });
                    if pattern != l.style.pattern {
                        l.style.pattern = pattern;
                        style_edits.push((pkg.id, layer_name.clone(), l.style.clone()));
                    }
                });
            }
        }
        if packages.is_empty() {
            ui.label("No linked site layers. Link one in File › Import and link….");
        }
        for (id, layer, style) in style_edits {
            self.apply_layer_style(id, &layer, style);
        }
        if let Some(p) = pick {
            self.plan.site_pick = Some(p);
            self.plan.align_ref = None;
            self.plan.calibrate = None;
            self.status = match p {
                SitePick::Zero => {
                    "Set zero: click a point of the geography on the plan.".into()
                }
                SitePick::Line(c) => format!(
                    "Point the line: click a line of the geography, it is to run {}.",
                    c.label().to_lowercase()
                ),
            };
        }
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
    }
}

/// sRGB 0..1 to the picker's bytes; none reads white.
fn srgb(c: Option<Rgb>) -> [u8; 3] {
    c.map_or([255, 255, 255], |c| {
        c.0.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    })
}

/// The picker's bytes back to sRGB 0..1.
fn rgb(b: [u8; 3]) -> Option<Rgb> {
    Some(Rgb(b.map(|v| v as f32 / 255.0)))
}

/// File name without folders, for headers and undo labels.
fn stem(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{
        Document, Element, ElementId, ElementKind, Level, Model, SiteCategory, SiteFeature,
        SiteLayer, SiteRow,
    };
    use glam::DVec2;

    /// The app around one linked file with one water layer, as the
    /// editor sees it: level, file element, source.
    fn file_app() -> (CreaBimApp, ElementId) {
        let mut app = CreaBimApp::blank(None);
        let l = Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let lid = l.id;
        let mut doc = Document::from_model(Model::new());
        doc.commit(creabim_core::Transaction::new("level").insert(l))
            .unwrap();
        let mut el = Element::new(
            "Site demo",
            ElementKind::SiteFeature(SiteFeature::file(vec![SiteLayer {
                layer: "bgt_waterdeel".into(),
                category: SiteCategory::Water,
                features: vec![SiteRow {
                    key: "1".into(),
                    geom: SiteGeometry::Polygon {
                        exterior: vec![
                            DVec2::new(0.0, 0.0),
                            DVec2::new(4.0, 0.0),
                            DVec2::new(4.0, 3.0),
                            DVec2::new(0.0, 3.0),
                        ],
                        holes: vec![],
                    },
                }],
                style: LayerStyle::default(),
            }])),
        );
        el.source = Some(creabim_core::Source {
            file: "demo.gpkg".into(),
            object: "demo".into(),
            stamp: 1,
        });
        doc.commit(creabim_core::Transaction::new("link").insert(el))
            .unwrap();
        app.doc = doc;
        (app, lid)
    }

    #[test]
    fn editor_style_and_visibility_reach_the_plan() {
        use creabim_core::styles::Rgb;
        use creabim_geom::plan::{plan_view, Pen, PlanItem};
        let (mut app, lid) = file_app();
        // what the editor lists
        let pkgs = site_packages(app.doc.model());
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].layers.len(), 1);
        let (id, name) = (pkgs[0].id, pkgs[0].layers[0].name.clone());
        // the editor styles the layer exactly as its widgets do
        let mut style = pkgs[0].layers[0].style.clone();
        style.fill = Some(Rgb([0.2, 0.4, 0.6]));
        app.apply_layer_style(id, &name, style);
        let plan = plan_view(app.doc.model(), lid, 1.2);
        let fills: Vec<_> = plan
            .items
            .iter()
            .filter_map(|it| match it {
                PlanItem::Fill { rgb, .. } => Some(*rgb),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 1);
        assert!(fills[0].is_some_and(|c| (c[0] - 0.2).abs() < 1e-6));
        assert!(plan.items.iter().any(|it| matches!(
            it,
            PlanItem::Polygon {
                pen: Pen::SiteLayer { .. },
                ..
            }
        )));
        // and hiding the layer draws nothing
        let mut style = site_packages(app.doc.model())[0].layers[0]
            .style
            .clone();
        style.visible = false;
        app.apply_layer_style(id, &name, style);
        let plan = plan_view(app.doc.model(), lid, 1.2);
        assert!(
            plan.items.is_empty(),
            "hidden layer draws nothing, has {:?}",
            plan.items.len()
        );
    }
}
