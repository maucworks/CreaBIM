//! Profiles (MU27 block 5; MU37: approved Mockup_ProfileView_REV00).
//!
//! A profile is drawn like any drawing, in a Profile view: an area like a
//! plan, with its zoom column, a grid, guides, dimensions to type, snaps,
//! and an Insertion tool that puts the point that sits on the path where
//! it is wanted. One list (with a small picture of each profile) in the
//! area's header picks, makes, duplicates and deletes profiles; the same
//! list is what Sweep, Extrusion, columns, beams and railings take. Edits
//! reach the elements made with a profile only when Apply is clicked.

use super::*;
use crate::ui_model::Item;
use creabim_core::profiles::{self, NamedProfile, ProfileShape, PRESETS};
use glam::DVec2;
use std::collections::HashMap;

/// What the Profile view's pointer does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProfTool {
    /// Pick and drag corners; click an edge's length to type it.
    #[default]
    Select,
    /// Click corners of a new shape: the first again closes it, Enter or
    /// Enter leaves it open; a right-click cancels it.
    Draw,
    /// Click where the insertion point goes.
    Insertion,
    /// Two clicks: a guide through them.
    Guide,
    /// What is picked (a corner, an edge, a shape, a guide) by a base
    /// point to another (M).
    Move,
    /// What is picked about a centre: the centre, a point to turn from,
    /// a point to turn to (R).
    Rotate,
    /// A line to align to (an edge, a guide, an insertion axis), then an
    /// edge or guide to lie on it.
    Align,
    /// Two points on a trace reference, then the distance between them.
    Calibrate,
    /// Click a face: a fill made void, a void filled back (MU37).
    Void,
}

/// What is picked in the Profile view (MU37: move, rotate and align work
/// on it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PSel {
    Corner(usize, usize),
    Edge(usize, usize),
    Shape(usize),
    Guide(usize),
    /// A trace reference: a picture, a DXF drawing, a view.
    Trace(ElementId),
}

/// The Profile view between frames.
#[derive(Clone, Debug, Default)]
pub struct ProfilesUi {
    pub open: bool,
    /// The profile shown: a preset's or the project's name.
    pub current: Option<String>,
    pub tool: ProfTool,
    /// The camera: centre (metres) and points per metre.
    camera: Option<(DVec2, f32)>,
    /// A corner being dragged: (shape, corner).
    drag: Option<(usize, usize)>,
    /// The shape whose corners show.
    shape: usize,
    /// Corners of a shape being drawn.
    drawing: Vec<DVec2>,
    /// The next side drawn is an arc (A; L back to lines), and its end
    /// once clicked: the next click is a point it passes through (MU39).
    arc: bool,
    arc_end: Option<DVec2>,
    /// The first click of a guide.
    guide_from: Option<DVec2>,
    /// Guides by profile: lines through two points.
    guides: HashMap<String, Vec<(DVec2, DVec2)>>,
    /// An edge's length being typed: (shape, edge, text).
    typing: Option<(usize, usize, String)>,
    /// The name being typed.
    name: String,
    /// The profiles as a drag leaves them, shown until the drag ends and
    /// committed then, once (MU37: dragging felt laggy, the model was
    /// rebuilt on every frame).
    preview: Option<Vec<NamedProfile>>,
    /// An edge being dragged: (shape, edge, where the drag began, its two
    /// corners then).
    edge_drag: Option<(usize, usize, DVec2, DVec2, DVec2)>,
    /// What is picked: the last, then the rest (Shift adds; a box picks
    /// several, MU37).
    sel: Option<PSel>,
    extra: Vec<PSel>,
    /// Where a box pick began, on screen.
    boxsel: Option<egui::Pos2>,
    /// The profile the Name field was filled for.
    name_for: String,
    /// Move or Rotate leaves the original and drops a copy (Shift+D, or
    /// Shift held at the last click; MU37).
    copy: bool,
    /// A guide being dragged: (its place, where the drag began, its two
    /// points then).
    guide_drag: Option<(usize, DVec2, DVec2, DVec2)>,
    /// Edge lengths and overall dimensions shown (D, or the header toggle).
    hide_dims: bool,
    /// A trace being dragged by its middle: (it, where the drag began, its
    /// place then, how far it has gone); shown moved, committed on release.
    trace_drag: Option<(ElementId, DVec2, DVec2, DVec2)>,
    /// The pass the pointer was last over the view, for the app's keys
    /// to stand aside.
    hovered_pass: u64,
    /// The clicks of Move (base) and Rotate (centre, from).
    op: Vec<DVec2>,
    /// Align's line to align to.
    align_ref: Option<(DVec2, DVec2)>,
    /// Calibrate's two points and the distance being typed.
    calib: Option<(DVec2, DVec2, String)>,
}

/// What the profiles' trace references show, read once (a file's pixels
/// or lines).
/// An arc from `a` through `t` to `e` as points (not `a`, ending on `e`),
/// `n` sides at most: the profile keeps arcs as points, so every sweep,
/// extrusion and export takes them as it takes any shape (MU39). Three
/// points in a line give the straight side.
pub(crate) fn arc_points(a: DVec2, t: DVec2, e: DVec2, n: usize) -> Vec<DVec2> {
    let d = 2.0 * (a.x * (t.y - e.y) + t.x * (e.y - a.y) + e.x * (a.y - t.y));
    if d.abs() < 1e-12 {
        return vec![e];
    }
    let sq = |p: DVec2| p.length_squared();
    let c = DVec2::new(
        (sq(a) * (t.y - e.y) + sq(t) * (e.y - a.y) + sq(e) * (a.y - t.y)) / d,
        (sq(a) * (e.x - t.x) + sq(t) * (a.x - e.x) + sq(e) * (t.x - a.x)) / d,
    );
    let ang = |p: DVec2| (p - c).y.atan2((p - c).x);
    let (a0, at, ae) = (ang(a), ang(t), ang(e));
    let tau = std::f64::consts::TAU;
    let norm = |x: f64| x.rem_euclid(tau);
    // the way round from a to e that passes t
    let ccw = norm(at - a0) < norm(ae - a0);
    let sweep = if ccw { norm(ae - a0) } else { -norm(a0 - ae) };
    let r = (a - c).length();
    let k = ((sweep.abs() / (tau / 48.0)).ceil() as usize).clamp(2, n.max(2));
    (1..=k)
        .map(|i| {
            if i == k {
                e
            } else {
                c + DVec2::from_angle(a0 + sweep * i as f64 / k as f64) * r
            }
        })
        .collect()
}

fn trace_cache() -> std::sync::MutexGuard<'static, crate::trace::RefCache> {
    static C: std::sync::OnceLock<std::sync::Mutex<crate::trace::RefCache>> =
        std::sync::OnceLock::new();
    C.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// A small picture of a profile, as an icon the web menus can draw: its
/// name, registered once (MU37: a picture beside each name).
pub fn thumb_icon(name: &str) -> &'static str {
    static NAMES: std::sync::OnceLock<std::sync::Mutex<HashMap<String, &'static str>>> =
        std::sync::OnceLock::new();
    let m = NAMES.get_or_init(Default::default);
    let mut m = m.lock().expect("thumbs");
    if let Some(s) = m.get(name) {
        return s;
    }
    let s: &'static str = Box::leak(format!("profile-thumb:{name}").into_boxed_str());
    m.insert(name.to_string(), s);
    s
}

/// A profile's outline as an SVG path in a 24 × 24 box.
fn thumb_svg(points: &[DVec2]) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let lo = points.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
    let hi = points.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
    let span = (hi - lo).max_element().max(1e-6);
    let k = 18.0 / span;
    let off = DVec2::new(12.0, 12.0) - DVec2::new((lo.x + hi.x) / 2.0, -(lo.y + hi.y) / 2.0) * k;
    let d: Vec<String> = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let q = DVec2::new(p.x, -p.y) * k + off;
            format!("{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, q.x, q.y)
        })
        .collect();
    format!("<path d=\"{}Z\"></path>", d.join(""))
}

impl CreaBimApp {
    /// Every profile's picture, for the web menus to draw by name.
    pub(super) fn profile_thumbs(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        for p in profiles::project_profiles(self.doc.model()) {
            m.insert(thumb_icon(&p.name).into(), thumb_svg(&p.outline()).into());
        }
        for n in PRESETS {
            if let Some(p) = profiles::preset(n) {
                m.insert(thumb_icon(n).into(), thumb_svg(&p).into());
            }
        }
        m
    }

    /// Write the project's profiles as one undo step (merged while a drag
    /// goes on). The elements made with them wait for Apply (MU37).
    fn put_profiles(&mut self, all: Vec<NamedProfile>, label: &str, merge: bool) {
        let tx = profiles::set_profiles(self.doc.model(), all, label);
        let r = if merge {
            self.doc.commit_merged(tx)
        } else {
            self.doc.commit(tx)
        };
        if let Err(e) = r {
            self.status = format!("Couldn't do that: {e}");
        }
    }

    /// The elements made with the shown profile take its shape now.
    fn apply_profile(&mut self) {
        let Some(n) = self.profiles.current.clone() else {
            return;
        };
        let model = self.doc.model();
        let Some(p) = profiles::find(model, &n) else {
            return;
        };
        let k = profiles::stale(model, &n);
        let tx = profiles::follow(
            model,
            &n,
            &p,
            creabim_core::Transaction::new(format!("Apply profile {n}")),
        );
        if tx.ops.is_empty() {
            self.status = format!("Everything made with {n} has its shape already.");
            return;
        }
        let r = self.doc.commit(tx);
        self.report(
            r,
            format!(
                "{k} element{} made with {n} updated. Ctrl+Z puts them back.",
                if k == 1 { "" } else { "s" }
            ),
        );
    }

    /// A new project profile from the selection: every selected sketch
    /// (on any plane, open or closed), slab or extrusion, as one profile
    /// of several shapes; the first picked is the main one.
    pub(super) fn profile_from_selection(&mut self) {
        let model = self.doc.model();
        let ids = self.selection.all();
        let Some(mut p) = profiles::from_shapes(model, &ids) else {
            self.status = "Select a sketch (on any plane, open or closed), a slab or an extrusion first; Shift+click more to make one profile of several shapes.".into();
            return;
        };
        let base = ids
            .first()
            .and_then(|id| model.get(*id))
            .map_or("Profile".to_string(), |e| format!("{} profile", e.name));
        let name = profiles::free_name(model, &base);
        p.name = name.clone();
        let n = 1 + p.more.len();
        let mut all = profiles::project_profiles(model);
        all.push(p);
        self.put_profiles(all, "Profile from a shape", false);
        self.open_profile(&name);
        self.status = format!("Profile {name} made from {n} shape{}: open it in a Profile view to place its insertion point and size it.", if n == 1 { "" } else { "s" });
    }

    fn new_profile(&mut self, from: Option<NamedProfile>, base: &str) {
        let model = self.doc.model();
        let name = profiles::free_name(model, base);
        let mut all = profiles::project_profiles(model);
        let mut p = from.unwrap_or(NamedProfile {
            points: creabim_core::parts::rectangle(0.1, 0.05),
            ..Default::default()
        });
        p.name = name.clone();
        all.push(p);
        self.put_profiles(all, "New profile", false);
        self.open_profile(&name);
    }

    fn open_profile(&mut self, name: &str) {
        self.profiles.current = Some(name.to_string());
        self.profiles.name = name.to_string();
        self.profiles.camera = None;
        self.profiles.shape = 0;
        // in a Profile area: one there comes to the front (so Properties
        // shows the profile), else the focused area turns into one
        let shows = self.areas.root.areas();
        if let Some(i) = shows
            .iter()
            .position(|s| *s == crate::areas::Shows::Profiles)
        {
            self.areas.focus = i;
        } else {
            let i = self.areas.focus;
            if let Some(s) = self.areas.root.get_mut(i) {
                *s = crate::areas::Shows::Profiles;
            }
            self.store_areas();
        }
    }

    /// Profile actions from menus, headers, the browser and Manage.
    pub(super) fn profiles_action(&mut self, head: &str, arg: &str) -> bool {
        match head {
            "profiles.browser" => {
                let n = self
                    .profiles
                    .current
                    .clone()
                    .unwrap_or_else(|| PRESETS[0].to_string());
                self.open_profile(&n);
            }
            "profiles.new" => self.new_profile(None, "Profile"),
            "profiles.fromsel" => self.profile_from_selection(),
            "profiles.open" => self.open_profile(arg),
            "profiles.duplicate" => {
                let n = self.profiles.current.clone().unwrap_or_default();
                let model = self.doc.model();
                let from = profiles::project_profiles(model)
                    .into_iter()
                    .find(|p| p.name == n)
                    .or_else(|| {
                        Some(NamedProfile {
                            points: profiles::preset(&n)?,
                            ..Default::default()
                        })
                    });
                self.new_profile(from, &n);
            }
            "profiles.delete" => {
                let n = self.profiles.current.clone().unwrap_or_default();
                let all: Vec<NamedProfile> = profiles::project_profiles(self.doc.model())
                    .into_iter()
                    .filter(|p| p.name != n)
                    .collect();
                self.put_profiles(all, "Delete profile", false);
                // the view keeps the name: Ctrl+Z puts the profile back on
                // screen (MU37: Undo seemed to do nothing)
                self.status =
                    format!("Profile {n} deleted; what was made with it keeps its shape.");
            }
            "profiles.apply" => self.apply_profile(),
            "prof.trace" => self.profile_trace(arg),
            "prof.dims" => self.profiles.hide_dims = !self.profiles.hide_dims,
            "prof.arc" => {
                self.profiles.tool = ProfTool::Draw;
                self.profiles.arc = !self.profiles.arc;
                self.profiles.arc_end = None;
                self.status = if self.profiles.arc {
                    "Arc: click where the arc ends, then a point it passes through. L goes back to straight sides.".into()
                } else {
                    "Straight sides.".into()
                };
            }
            "prof.tool" => {
                self.profiles.tool = match arg {
                    "draw" => ProfTool::Draw,
                    "insert" => ProfTool::Insertion,
                    "guide" => ProfTool::Guide,
                    "move" => ProfTool::Move,
                    "rotate" => ProfTool::Rotate,
                    "align" => ProfTool::Align,
                    "calibrate" => ProfTool::Calibrate,
                    "void" => ProfTool::Void,
                    _ => ProfTool::Select,
                };
                self.profiles.drawing.clear();
                self.profiles.guide_from = None;
                self.profiles.op.clear();
                self.profiles.align_ref = None;
                self.profiles.calib = None;
                // the Select tool lets go of everything, as elsewhere (MU37)
                if self.profiles.tool == ProfTool::Select {
                    self.profiles.sel = None;
                    self.profiles.extra.clear();
                    self.profiles.boxsel = None;
                }
                self.status = match self.profiles.tool {
                    ProfTool::Draw => "Draw: click the corners of a shape; click the first to close it; double-click (or click the last corner again) or Enter finishes (it closes by itself when its ends meet, or rest on another shape); a right-click cancels. It snaps to corners, guides and the grid.".into(),
                    ProfTool::Insertion => "Insertion: click where the profile sits on its path (it snaps to corners and guides).".into(),
                    ProfTool::Guide => "Guide: click two points for a guide through them.".into(),
                    ProfTool::Select => "Click a corner, an edge, a guide or inside a shape to pick it; drag corners and edges; click an edge's length to type it.".into(),
                    ProfTool::Move => "Move: pick what to move (or keep what is picked), click a base point, then where it goes.".into(),
                    ProfTool::Rotate => "Rotate: pick what to turn, click the centre, a point to turn from, then where it turns to (5° steps unless it snaps).".into(),
                    ProfTool::Align => "Align: click the line to align to (an edge, a guide or an insertion axis), then the edge or guide to lie on it.".into(),
                    ProfTool::Calibrate => "Calibrate: click two points on the trace a known distance apart, then type that distance.".into(),
                    ProfTool::Void => "Pick void: click a face where shapes cross or nest to empty it; click a void to fill it back.".into(),
                };
            }
            "prof.shape" => self.edit_current("Profile open or closed", |p, s| {
                let open = arg == "open";
                if s == 0 {
                    p.open = open;
                } else if let Some(m) = p.more.get_mut(s - 1) {
                    m.open = open;
                }
            }),
            "prof.mirror" => self.edit_current("Mirror profile", |p, _| {
                for q in p
                    .points
                    .iter_mut()
                    .chain(p.more.iter_mut().flat_map(|m| m.points.iter_mut()))
                {
                    q.x = -q.x;
                }
                p.points.reverse();
                for m in &mut p.more {
                    m.points.reverse();
                }
            }),
            "beam.corner" => {
                if let Some(c) = creabim_core::members::BeamCorner::ALL
                    .into_iter()
                    .find(|c| c.label() == arg)
                {
                    self.plan.members.beam_corner = c;
                    self.status =
                        format!("Beam corners: {}. Chain beams to see them meet.", c.label());
                }
            }
            "railing.lib" => {
                let lib = (!arg.is_empty()).then(|| arg.to_string());
                self.plan.members.railing_section = lib.clone();
                self.status = match lib {
                    Some(n) => format!("New railings' top rail is {n}."),
                    None => "New railings take their own square rail again.".into(),
                };
            }
            "column.lib" | "beam.lib" => {
                let lib = (!arg.is_empty()).then(|| arg.to_string());
                let what = if head == "column.lib" {
                    self.plan.members.column_section = lib.clone();
                    "column"
                } else {
                    self.plan.members.beam_section = lib.clone();
                    "beam"
                };
                self.status = match lib {
                    Some(n) => format!("New {what}s are {n}: click to place them. Square, Round or Its own goes back."),
                    None => format!("New {what}s take their own shape again."),
                };
            }
            "sweep.lib" | "extrusion.lib" => {
                let lib = (!arg.is_empty()).then(|| arg.to_string());
                let (tool, what) = if head == "sweep.lib" {
                    self.plan.parts.library = lib.clone();
                    (Tool::Sweep, "Sweep")
                } else {
                    self.plan.parts.extrusion_library = lib.clone();
                    (Tool::Extrusion, "Extrusion")
                };
                let pick = if self.plan.tool == tool {
                    String::new()
                } else {
                    format!(" (pick {what} in the Model tab)")
                };
                self.status = match (lib, tool) {
                    (Some(n), Tool::Sweep) => format!("The Sweep tool{pick} uses {n}: click a wall to run it along, or click a path; its section shows as you go."),
                    (Some(n), _) => format!("The Extrusion tool{pick} places {n}: click where its insertion point goes."),
                    (None, _) => format!("The {what} tool draws its own shape again."),
                };
            }
            _ => return false,
        }
        true
    }

    /// Change the shown profile (the project's own) in one undo step.
    fn edit_current(&mut self, label: &str, f: impl FnOnce(&mut NamedProfile, usize)) {
        let Some(n) = self.profiles.current.clone() else {
            return;
        };
        let mut all = profiles::project_profiles(self.doc.model());
        let Some(p) = all.iter_mut().find(|p| p.name == n) else {
            self.status =
                format!("{n} is a preset: Duplicate it to make one of the project's to change.");
            return;
        };
        f(p, self.profiles.shape);
        self.put_profiles(all, label, false);
    }

    /// The pictured profile list: the project's, the presets, then ways to
    /// make one. `head` is what picking one runs.
    pub(super) fn library_items(
        &self,
        head: &str,
        now: Option<String>,
        own_label: Option<&str>,
    ) -> Vec<Item> {
        // what to do first, always in reach; then the profiles (MU37)
        let model = self.doc.model();
        let own = profiles::project_profiles(model);
        let mut items = vec![Item::new("profiles.new", "New profile").icon("plus")];
        items.push(
            Item::new("profiles.fromsel", "From the selection")
                .icon("scan")
                .hint("Sketches on any plane (open or closed), slabs or extrusions: one profile of their shapes"),
        );
        if head == "profiles.open" && self.profiles.current.is_some() {
            items.push(
                Item::new("profiles.duplicate", "Duplicate")
                    .icon("copy")
                    .hint("A copy in the project, to change slightly"),
            );
            if own
                .iter()
                .any(|p| Some(&p.name) == self.profiles.current.as_ref())
            {
                items.push(Item::new("profiles.delete", "Delete").icon("trash-2"));
            }
        }
        if let Some(l) = own_label {
            items.push(Item::new(format!("{head}:"), l).checked(now.is_none()));
        }
        items.push(Item::Header("This project".into()));
        if own.is_empty() {
            items.push(Item::new("profiles.new", "None yet").enabled(false));
        }
        for p in &own {
            let kind = if p.open {
                "open"
            } else if p.more.is_empty() {
                "closed"
            } else {
                "several"
            };
            items.push(
                Item::new(format!("{head}:{}", p.name), &p.name)
                    .icon(thumb_icon(&p.name))
                    .key(kind)
                    .checked(now.as_deref() == Some(p.name.as_str())),
            );
        }
        items.push(Item::Header("Presets".into()));
        for p in PRESETS {
            items.push(
                Item::new(format!("{head}:{p}"), p)
                    .icon(thumb_icon(p))
                    .checked(now.as_deref() == Some(p)),
            );
        }
        items
    }

    /// The Profile view's name menu: the profiles to show, the project's
    /// first, then what to do with the one shown (MU37: the library is the
    /// list; + beside it makes a new one).
    fn profile_list(&self) -> Vec<Item> {
        let own = profiles::project_profiles(self.doc.model());
        let now = self.profiles.current.clone();
        let mut items = vec![Item::Header("This project".into())];
        if own.is_empty() {
            // none yet: making one, with an icon as the rows below (MU39)
            items.push(
                Item::new("profiles.new", "New profile")
                    .icon("plus")
                    .hint("None yet: start one here, a square to shape"),
            );
        }
        for p in &own {
            let kind = if p.open {
                "open"
            } else if p.more.is_empty() {
                "closed"
            } else {
                "several"
            };
            items.push(
                Item::new(format!("profiles.open:{}", p.name), &p.name)
                    .icon(thumb_icon(&p.name))
                    .key(kind)
                    .checked(now.as_deref() == Some(p.name.as_str())),
            );
        }
        items.push(Item::Header("Presets".into()));
        for p in PRESETS {
            items.push(
                Item::new(format!("profiles.open:{p}"), p)
                    .icon(thumb_icon(p))
                    .checked(now.as_deref() == Some(p)),
            );
        }
        items
    }

    /// The Trace menu: add a picture, a DXF drawing or a view; then the
    /// references here, each to show or hide, calibrate or remove.
    fn trace_items(&self) -> Vec<Item> {
        let model = self.doc.model();
        let mut items = vec![
            Item::new("prof.trace:image", "Add a picture…").icon("file-plus").hint("A PNG or JPEG: a scanned detail, a photo of a sketch"),
            Item::new("prof.trace:dxf", "Add a DXF drawing…").icon("file-text").hint("A CAD block or detail's lines; a DWG saved as DXF first (the free ODA File Converter does it offline)"),
        ];
        let views: Vec<(ElementId, String)> = model
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::View(v)
                    if matches!(v.kind, ViewKind::Plan { .. } | ViewKind::Section { .. }) =>
                {
                    Some((e.id, e.name.clone()))
                }
                _ => None,
            })
            .collect();
        if !views.is_empty() {
            items.push(Item::Header("A view of the project".into()));
            for (id, name) in views {
                items.push(
                    Item::new(format!("prof.trace:view:{}", id.ifc_guid()), name).icon("underlay"),
                );
            }
        }
        let Some(cur) = self.profiles.current.as_deref() else {
            return items;
        };
        let here = creabim_core::reference::in_view(model, profiles::trace_view(cur));
        if !here.is_empty() {
            items.push(Item::Header("Under this profile".into()));
            for (id, r) in here {
                let name = crate::trace::label(model, &r.source);
                items.push(
                    Item::new(format!("prof.trace:pick:{}", id.ifc_guid()), &name)
                        .icon(if r.hidden { "eye-off" } else { "eye" })
                        .hint("Pick it: then Move (M) or Rotate (R) it, or Calibrate"),
                );
                items.push(Item::new(
                    format!("prof.trace:hide:{}", id.ifc_guid()),
                    if r.hidden {
                        "    Show it"
                    } else {
                        "    Hide it"
                    },
                ));
                items.push(Item::new(
                    format!("prof.trace:calibrate:{}", id.ifc_guid()),
                    "    Calibrate…",
                ));
                items.push(
                    Item::new(
                        format!("prof.trace:remove:{}", id.ifc_guid()),
                        "    Remove it",
                    )
                    .icon("trash-2"),
                );
            }
        }
        items
    }

    /// Trace menu actions.
    fn profile_trace(&mut self, arg: &str) {
        use creabim_core::RefSource;
        let Some(cur) = self.profiles.current.clone() else {
            self.status = "Pick or make a profile first.".into();
            return;
        };
        let pv = profiles::trace_view(&cur);
        let id_of = ElementId::from_ifc_guid;
        let (what, rest) = arg.split_once(':').unwrap_or((arg, ""));
        let src = match what {
            "image" => rfd::FileDialog::new()
                .set_title("A picture to trace over")
                .add_filter("Pictures", &["png", "jpg", "jpeg"])
                .pick_file()
                .map(|p| RefSource::Image {
                    path: p.display().to_string(),
                    width: 0.3,
                }),
            "dxf" => rfd::FileDialog::new()
                .set_title("A DXF drawing to trace over")
                .add_filter("DXF", &["dxf"])
                .pick_file()
                .map(|p| RefSource::Dxf {
                    path: p.display().to_string(),
                }),
            "view" => id_of(rest).map(RefSource::View),
            _ => None,
        };
        if let Some(src) = src {
            let model = self.doc.model();
            let shown = crate::trace::read(model, &src);
            if let crate::trace::Shown::Missing(why) = &shown {
                self.status = why.clone();
                return;
            }
            let mut r = creabim_core::Reference::new(src, vec![pv]);
            // kept to this profile until it is unticked (MU38)
            r.only_profile = true;
            if let Some(b) = crate::trace::bounds_of(&r.source, &shown) {
                r.in_place(b);
            }
            // centred on the insertion point, where the profile is
            r.at = DVec2::ZERO;
            if matches!(shown, crate::trace::Shown::Picture(_)) {
                r.colour = None;
                r.opacity = 0.6;
            }
            let name = crate::trace::label(model, &r.source);
            let e = Element::new(name.clone(), ElementKind::Reference(r));
            let id = e.id;
            let res = self.doc.commit(
                creabim_core::Transaction::new(format!("Trace {name} under {cur}")).insert(e),
            );
            if res.is_ok() {
                self.profiles.sel = Some(PSel::Trace(id));
                self.profiles.camera = None;
            }
            self.report(res, format!("{name} is under {cur}: Move (M), Rotate (R) or Calibrate it to size; it snaps."));
            return;
        }
        let Some(id) = id_of(rest) else { return };
        match what {
            "pick" => {
                self.profiles.sel = Some(PSel::Trace(id));
                self.profiles.tool = ProfTool::Move;
                self.profiles.op.clear();
                self.status =
                    "Move: click a base point, then where the trace goes (R rotates it).".into();
            }
            "hide" => {
                let r = self.doc.edit(id, "Show or hide trace", |e| {
                    if let ElementKind::Reference(r) = &mut e.kind {
                        r.hidden = !r.hidden;
                    }
                });
                if let Err(e) = r {
                    self.status = format!("Couldn't do that: {e}");
                }
            }
            "calibrate" => {
                self.profiles.sel = Some(PSel::Trace(id));
                self.profiles.tool = ProfTool::Calibrate;
                self.profiles.calib = None;
                self.profiles.op.clear();
                self.status = "Calibrate: click two points on the trace a known distance apart, then type that distance.".into();
            }
            "remove" => {
                let Some(e) = self.doc.model().get(id).cloned() else {
                    return;
                };
                let _ = e;
                let r = self.doc.commit(
                    creabim_core::Transaction::new("Remove trace")
                        .remove_cascade(self.doc.model(), id),
                );
                if self.profiles.sel == Some(PSel::Trace(id)) {
                    self.profiles.sel = None;
                }
                self.report(r, "Trace removed.");
            }
            _ => {}
        }
    }

    /// The Profile view header's Name field, beside + (MU37: "insert name
    /// field next to the +", as Cut height is a field): typed and let go,
    /// the profile is renamed everywhere it is used.
    pub(super) fn profile_name_field(&mut self, ui: &mut egui::Ui, pal: &crate::theme::Palette) {
        let Some(cur) = self.profiles.current.clone() else {
            return;
        };
        let mine = profiles::project_profiles(self.doc.model())
            .iter()
            .any(|p| p.name == cur);
        if self.profiles.name_for != cur {
            self.profiles.name = cur.clone();
            self.profiles.name_for = cur.clone();
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().interact_size.y = 20.0;
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(12.0));
            ui.label(egui::RichText::new("Name").size(12.0).color(pal.muted));
            let r = ui
                .add_enabled(
                    mine,
                    egui::TextEdit::singleline(&mut self.profiles.name)
                        .vertical_align(egui::Align::Center)
                        .desired_width(84.0),
                )
                .on_hover_text(if mine {
                    "The profile's name: type a new one, then Enter"
                } else {
                    "A preset keeps its name: Duplicate it to make your own"
                })
                .on_disabled_hover_text("A preset keeps its name: Duplicate it to make your own");
            let done = r.lost_focus();
            if done {
                let new = self.profiles.name.trim().to_string();
                if !new.is_empty() && new != cur {
                    self.rename_profile(&cur, &new);
                } else {
                    self.profiles.name = cur.clone();
                }
            }
        });
    }

    /// The Profile tab of Properties while a Profile view is in front
    /// (MU40): the profile's name and size, its faces (a void filled back
    /// in one click), flips, and the picked corner's place to type.
    pub(super) fn profile_props(&mut self, ui: &mut egui::Ui) {
        let Some(cur) = self.profiles.current.clone() else {
            crate::props::section(ui, "Profile");
            crate::props::note(
                ui,
                "Pick a profile in the view's header, or make a New one.",
            );
            return;
        };
        let model = self.doc.model();
        let mine = profiles::project_profiles(model)
            .into_iter()
            .find(|p| p.name == cur);
        let users = profiles::users(model, &cur).len();
        if self.prop_header(ui, "prof_about", "Profile", true, Some("profiles")) {
            crate::props::row(ui, "Name", |ui| self.profile_name_field_bare(ui));
            let pts: Vec<DVec2> = match &mine {
                Some(p) => p.shapes().iter().flat_map(|s| s.points.clone()).collect(),
                None => profiles::preset(&cur).unwrap_or_default(),
            };
            if let Some(first) = pts.first() {
                let (lo, hi) = pts
                    .iter()
                    .fold((*first, *first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                let d = hi - lo;
                crate::props::row(ui, "Size", |ui| {
                    ui.label(format!("{:.0} × {:.0} mm", d.x * 1000.0, d.y * 1000.0))
                });
            }
            crate::props::row(ui, "Used by", |ui| {
                ui.label(match users {
                    0 => "Nothing yet".to_string(),
                    1 => "1 element".to_string(),
                    n => format!("{n} elements"),
                })
            });
            if mine.is_none() {
                crate::props::note(
                    ui,
                    "A preset: Duplicate it in the header to make one of your own to change.",
                );
            }
        }
        let Some(p) = mine else {
            return;
        };
        // its faces: what is filled, what is void (MU40: a void read as a
        // face turned the wrong way)
        let faces = p.faces();
        let voids: Vec<DVec2> = faces
            .iter()
            .filter(|f| !f.filled)
            .map(|f| f.sample)
            .collect();
        let filled = faces.len() - voids.len();
        if self.prop_header(ui, "prof_faces", "Faces", true, None) {
            crate::props::row(ui, "Faces", |ui| {
                ui.label(format!(
                    "{filled} filled, {} void{}",
                    voids.len(),
                    if voids.len() == 1 { "" } else { "s" }
                ))
            });
            let (fill, pick) = crate::props::actions2(
                ui,
                "",
                (
                    "Fill every void",
                    "Fill every hole of the profile: a hollow one turned solid",
                    !voids.is_empty(),
                ),
                (
                    "Pick a face…",
                    "Click a face in the view to turn it over: a void filled, a fill made void",
                    true,
                ),
            );
            crate::props::note(
                ui,
                "A void is white in the view and a hole in what it makes. Offset all, or a shape inside another, makes one.",
            );
            if fill {
                let n = voids.len();
                self.edit_current("Fill the voids", |x, _| {
                    for s in &voids {
                        x.toggle_face(*s);
                    }
                });
                self.status = format!(
                    "{} filled. Ctrl+Z empties {} again.",
                    if n == 1 { "The void" } else { "Every void" },
                    if n == 1 { "it" } else { "them" }
                );
            }
            if pick {
                self.profiles.tool = ProfTool::Void;
                self.status =
                    "Click a face to turn it over: a void filled, a fill made void.".into();
            }
        }
        if self.prop_header(ui, "prof_flip", "Flip", true, None) {
            let (across, up) = crate::props::actions2(
                ui,
                "",
                (
                    "Left to right",
                    "Mirror it across its insertion point's vertical",
                    true,
                ),
                (
                    "Upside down",
                    "Mirror it across its insertion point's level",
                    true,
                ),
            );
            let m = if across {
                Some(DVec2::new(-1.0, 1.0))
            } else if up {
                Some(DVec2::new(1.0, -1.0))
            } else {
                None
            };
            if let Some(k) = m {
                self.edit_current("Flip profile", |x, _| {
                    x.points.iter_mut().for_each(|q| *q *= k);
                    for s in &mut x.more {
                        s.points.iter_mut().for_each(|q| *q *= k);
                    }
                    x.flips.iter_mut().for_each(|q| *q *= k);
                });
                self.status = if users > 0 {
                    format!("{cur} flipped. Apply in the header gives what is made with it the new shape.")
                } else {
                    format!("{cur} flipped.")
                };
            }
        }
        // the corner picked: its place, to type (mm from the insertion point)
        if let Some(PSel::Corner(si, ci)) = self.profiles.sel {
            let at = p.shapes().get(si).and_then(|s| s.points.get(ci)).copied();
            if let Some(at) = at {
                if self.prop_header(ui, "prof_picked", "Picked corner", true, None) {
                    let (mut x, mut y) = (at.x * 1000.0, at.y * 1000.0);
                    let cx = crate::props::row(ui, "X", |ui| {
                        crate::props::fill(
                            ui,
                            egui::DragValue::new(&mut x).suffix(" mm").speed(1.0),
                        )
                    });
                    let cy = crate::props::row(ui, "Y", |ui| {
                        crate::props::fill(
                            ui,
                            egui::DragValue::new(&mut y).suffix(" mm").speed(1.0),
                        )
                    });
                    if cx.changed() || cy.changed() {
                        let to = DVec2::new(x, y) / 1000.0;
                        self.edit_current("Move corner", |q, _| {
                            if let Some(c) = point_mut(q, si).and_then(|v| v.get_mut(ci)) {
                                *c = to;
                            }
                        });
                    }
                }
            }
        }
    }

    /// The Name field without its label, for a Properties row.
    fn profile_name_field_bare(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let cur = self.profiles.current.clone().unwrap_or_default();
        let mine = profiles::project_profiles(self.doc.model())
            .iter()
            .any(|p| p.name == cur);
        if self.profiles.name_for != cur {
            self.profiles.name = cur.clone();
            self.profiles.name_for = cur.clone();
        }
        let r = crate::props::fill(
            ui,
            egui::TextEdit::singleline(&mut self.profiles.name)
                .vertical_align(egui::Align::Center)
                .interactive(mine),
        );
        if r.lost_focus() {
            let new = self.profiles.name.trim().to_string();
            if !new.is_empty() && new != cur {
                self.rename_profile(&cur, &new);
            } else {
                self.profiles.name = cur.clone();
            }
        }
        r
    }

    /// Is a Profile view the area in front (focused)?
    pub(super) fn profiles_in_front(&self) -> bool {
        matches!(
            self.areas.root.areas().get(self.areas.focus),
            Some(crate::areas::Shows::Profiles)
        )
    }

    /// Rename profile `old`: in the library, on what is made with it, its
    /// guides and its trace references.
    fn rename_profile(&mut self, old: &str, new: &str) {
        let model = self.doc.model();
        if profiles::find(model, new).is_some() || PRESETS.contains(&new) {
            self.status = format!("There is a profile called {new} already.");
            self.profiles.name = old.to_string();
            return;
        }
        let mut all = profiles::project_profiles(model);
        for p in &mut all {
            if p.name == old {
                p.name = new.to_string();
            }
        }
        let mut tx = profiles::set_profiles(model, all, &format!("Rename profile {old}"));
        for e in profiles::users(model, old) {
            let mut n = e.clone();
            match &mut n.kind {
                ElementKind::Part(p) => p.profile = Some(new.to_string()),
                ElementKind::Column(c) => {
                    if let Some(s) = &mut c.section {
                        s.name = new.to_string();
                    }
                }
                ElementKind::Beam(b) => {
                    if let Some(s) = &mut b.section {
                        s.name = new.to_string();
                    }
                }
                ElementKind::Railing(r) => r.rail_profile = Some(new.to_string()),
                _ => {}
            }
            tx = tx.replace(e.clone(), n);
        }
        let (ov, nv) = (profiles::trace_view(old), profiles::trace_view(new));
        for (id, _) in creabim_core::reference::in_view(model, ov) {
            if let Some(e) = model.get(id) {
                let mut n = e.clone();
                if let ElementKind::Reference(r) = &mut n.kind {
                    for v in &mut r.shown_in {
                        if *v == ov {
                            *v = nv;
                        }
                    }
                }
                tx = tx.replace(e.clone(), n);
            }
        }
        let r = self.doc.commit(tx);
        if r.is_ok() {
            if let Some(g) = self.profiles.guides.remove(old) {
                self.profiles.guides.insert(new.to_string(), g);
            }
            self.profiles.current = Some(new.to_string());
            self.profiles.name_for = new.to_string();
        }
        self.report(
            r,
            format!("Renamed {old} to {new}, on everything made with it."),
        );
    }

    /// Whether the pointer was over a Profile view in this pass or the last.
    pub(super) fn profile_view_hovered(&self, ctx: &egui::Context) -> bool {
        let p = self.profiles.hovered_pass;
        p > 0 && p + 1 >= ctx.cumulative_pass_nr()
    }

    /// The Profile view header's buttons beside the list: id, icon, tip,
    /// and whether it can be used now.
    pub(super) fn profile_buttons(&self) -> Vec<(&'static str, &'static str, String, bool)> {
        let own = profiles::project_profiles(self.doc.model());
        let now = self.profiles.current.clone();
        let mine = now
            .as_ref()
            .is_some_and(|n| own.iter().any(|p| &p.name == n));
        let has_sel = !self.selection.is_empty();
        vec![
            (
                "profiles.new",
                "plus",
                "New profile: a square to start from, in this view".into(),
                true,
            ),
            (
                "profiles.fromsel",
                "scan",
                if has_sel {
                    "From the selection: sketches on any plane (open or closed), slabs or extrusions, as one profile".into()
                } else {
                    "From the selection: select sketches, slabs or extrusions first".into()
                },
                has_sel,
            ),
            (
                "profiles.duplicate",
                "copy",
                match &now {
                    Some(n) => format!("Duplicate {n}: a copy in the project, to change slightly"),
                    None => "Duplicate: pick a profile first".into(),
                },
                now.is_some(),
            ),
            (
                "profiles.delete",
                "trash-2",
                match (&now, mine) {
                    (Some(n), true) => format!("Delete {n} from the project"),
                    (Some(_), false) => {
                        "A preset can't be deleted: duplicate it to make your own".into()
                    }
                    _ => "Delete: pick a profile first".into(),
                },
                mine,
            ),
        ]
    }

    pub(super) fn sweep_library_items(&self) -> Vec<Item> {
        self.library_items(
            "sweep.lib",
            self.plan.parts.library.clone(),
            Some("The tool's own (Coping, Plinth, Bar, Steps)"),
        )
    }

    pub(super) fn extrusion_library_items(&self) -> Vec<Item> {
        self.library_items(
            "extrusion.lib",
            self.plan.parts.extrusion_library.clone(),
            Some("Draw its outline (no profile)"),
        )
    }

    /// The Profile view's header controls.
    pub(super) fn profile_controls(&self) -> Vec<serde_json::Value> {
        use serde_json::json;
        let t = self.profiles.tool;
        let model = self.doc.model();
        let cur = self.profiles.current.clone();
        let own = cur.as_deref().and_then(|c| {
            profiles::project_profiles(model)
                .into_iter()
                .find(|p| p.name == c)
        });
        let stale = cur.as_deref().map_or(0, |c| profiles::stale(model, c));
        let shape_open = own.as_ref().is_some_and(|p| {
            if self.profiles.shape == 0 {
                p.open
            } else {
                p.more.get(self.profiles.shape - 1).is_some_and(|m| m.open)
            }
        });
        let mut v = vec![
            json!({"kind": "toggle", "id": "prof.tool:select", "icon": "mouse-pointer-2", "label": "Select", "on": t == ProfTool::Select,
                   "hint": "Pick and drag corners, edges and guides; click an edge's length to type it; right-click for quick actions"}),
            json!({"kind": "toggle", "id": "prof.tool:draw", "icon": "pencil", "label": "Draw", "on": t == ProfTool::Draw,
                   "hint": "Draw a shape: click its corners; the first again closes it, Enter leaves it open"}),
            json!({"kind": "toggle", "id": "prof.arc", "icon": "spline", "label": "Arc", "on": t == ProfTool::Draw && self.profiles.arc,
                   "hint": "Arc (A): the next side is an arc: click its end, then a point it passes through. L goes back to straight sides"}),
            json!({"kind": "toggle", "id": "prof.tool:insert", "icon": "crosshair", "label": "Insertion", "on": t == ProfTool::Insertion,
                   "hint": "Place the insertion point: where the profile sits on its path"}),
            json!({"kind": "toggle", "id": "prof.tool:guide", "icon": "guides", "label": "Guide", "on": t == ProfTool::Guide,
                   "hint": "A guide through two points, to snap to"}),
            json!({"kind": "toggle", "id": "prof.tool:move", "icon": "move", "label": "Move", "on": t == ProfTool::Move,
                   "hint": "Move (M): what is picked, from a base point to another"}),
            json!({"kind": "toggle", "id": "prof.tool:rotate", "icon": "rotate-cw", "label": "Rotate", "on": t == ProfTool::Rotate,
                   "hint": "Rotate (R): what is picked, about a centre"}),
            json!({"kind": "toggle", "id": "prof.tool:align", "icon": "align-tool", "label": "Align", "on": t == ProfTool::Align,
                   "hint": "Align: an edge or a guide onto a line (an edge, a guide or an axis)"}),
            json!({"kind": "toggle", "id": "prof.dims", "icon": "dimension", "label": "Dimensions", "on": !self.profiles.hide_dims,
                   "hint": "Show or hide the edge lengths and overall sizes (D)"}),
            json!({"kind": "toggle", "id": "prof.tool:void", "icon": "void", "label": "Pick void", "on": t == ProfTool::Void,
                   "hint": "Pick void: click a face where shapes cross or nest to empty it, a void to fill it back"}),
            json!({"kind": "menu", "key": "prof.trace", "icon": "underlay", "label": "Trace",
                   "badge": cur.as_deref().map_or(0, |c| creabim_core::reference::in_view(model, profiles::trace_view(c)).len()),
                   "hint": "Trace references: a picture, a DXF drawing or a view of the project under the profile, to trace over and snap to"}),
        ];
        if own.is_some() {
            v.push(json!({"kind": "menu", "key": "prof.shapekind", "label": "Shape", "value": if shape_open { "Open" } else { "Closed" },
                          "hint": "The picked shape: closed (a solid) or open (a wire, thickened)"}));
            v.push(json!({"kind": "button", "id": "prof.mirror", "icon": "flip", "label": "Mirror", "hint": "Mirror it about its insertion point"}));
        }
        if stale > 0 {
            v.push(json!({"kind": "button", "id": "profiles.apply", "icon": "check", "label": "Apply", "badge": stale,
                          "hint": format!("Give the {stale} element(s) made with this profile its new shape (one undo step)")}));
        }
        v
    }

    /// The Profile view's header menus.
    pub(super) fn profile_menu(&self, key: &str) -> Option<Vec<Item>> {
        Some(match key {
            "prof.pick" => self.profile_list(),
            "prof.trace" => self.trace_items(),
            "prof.shapekind" => vec![
                Item::new("prof.shape:closed", "Closed: a solid"),
                Item::new("prof.shape:open", "Open: a wire, thickened"),
            ],
            _ => return None,
        })
    }

    /// The browser's Profiles group.
    pub(super) fn profiles_group(&self) -> serde_json::Value {
        let own = profiles::project_profiles(self.doc.model());
        let rows: Vec<serde_json::Value> = own
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": format!("profile:{}", p.name), "name": p.name, "icon": "profile",
                    "sel": self.profiles.current.as_deref() == Some(p.name.as_str()),
                    "hint": format!("{}: click to open it in a Profile view", p.name),
                })
            })
            .collect();
        serde_json::json!({
            "key": "profiles", "title": "Profiles", "icon": "profile", "open": !rows.is_empty(),
            "rows": rows,
            "note": if own.is_empty() { format!("{} presets", PRESETS.len()) } else { String::new() },
            "acts": [
                {"id": "profiles.new", "icon": "plus", "hint": "A new profile of the project, in a Profile view"},
                {"id": "profiles.browser", "icon": "ellipsis", "hint": "Open the Profile view"},
            ],
        })
    }

    /// Manage › Profiles used to open a window; it opens the view now.
    pub(super) fn profiles_window(&mut self, _ctx: &egui::Context) {
        if std::mem::take(&mut self.profiles.open) {
            let n = self
                .profiles
                .current
                .clone()
                .unwrap_or_else(|| PRESETS[0].to_string());
            self.open_profile(&n);
        }
    }

    /// The Profile view in an area (MU37).
    pub(super) fn profiles_area(&mut self, ui: &mut egui::Ui, body: Rect) {
        let _perf = crate::app::Perf::new("profile view");
        if ui.rect_contains_pointer(body) {
            self.profiles.hovered_pass = ui.ctx().cumulative_pass_nr();
        }
        let pal = self.shell.theme.palette();
        let model = self.doc.model();
        let response = ui.interact(
            body,
            ui.id().with("profile.view"),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(body);
        // the theme's paper (MU37: right in dark too)
        let dark = pal.dark;
        let paper = if dark {
            pal.panel2
        } else {
            egui::Color32::WHITE
        };
        painter.rect_filled(body, 0.0, paper);
        let cur = self.profiles.current.clone();
        let own_list = self
            .profiles
            .preview
            .clone()
            .unwrap_or_else(|| profiles::project_profiles(model));
        let mine = cur
            .as_deref()
            .and_then(|c| own_list.iter().find(|p| p.name == c).cloned());
        let shown: Option<NamedProfile> = mine.clone().or_else(|| {
            let n = cur.clone()?;
            Some(NamedProfile {
                points: profiles::preset(&n)?,
                name: n,
                ..Default::default()
            })
        });
        let Some(prof) = shown else {
            painter.text(
                body.center(),
                egui::Align2::CENTER_CENTER,
                &match self.profiles.current.as_deref() {
                    Some(n) => format!(
                        "{n} was deleted. Ctrl+Z brings it back; or pick another in the header."
                    ),
                    None => "Pick a profile in the header, or make a New one.".to_string(),
                },
                egui::FontId::proportional(14.0),
                pal.muted,
            );
            return;
        };
        let editable = mine.is_some();
        let shapes = prof.shapes();
        // the camera: fit once, then wheel and right-drag, and the column
        let all_pts: Vec<DVec2> = shapes
            .iter()
            .flat_map(|s| s.points.iter().copied())
            .chain([DVec2::ZERO])
            .collect();
        let fit = || {
            let lo = all_pts
                .iter()
                .fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
            let hi = all_pts
                .iter()
                .fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
            let span = (hi - lo).max(DVec2::splat(0.02));
            // room for its dimensions and the zoom column at the right
            let k = (((body.width() - 220.0).max(80.0)) as f64 / span.x)
                .min(((body.height() - 150.0).max(80.0)) as f64 / span.y)
                as f32;
            let c = (lo + hi) / 2.0 + DVec2::new(30.0 / k as f64, -10.0 / k as f64);
            (c, k.max(1.0))
        };
        let (mut center, mut scale) = self.profiles.camera.unwrap_or_else(fit);
        if response.hovered() {
            let (pan, scroll) = crate::planview::wheel(ui);
            if pan != egui::Vec2::ZERO {
                center -= DVec2::new(pan.x as f64, -pan.y as f64) / scale as f64;
            }
            if scroll != 0.0 {
                let k = (1.0 + scroll / 300.0).clamp(0.5, 2.0);
                if let Some(h) = response.hover_pos() {
                    let w = center
                        + DVec2::new(
                            (h.x - body.center().x) as f64,
                            -(h.y - body.center().y) as f64,
                        ) / scale as f64;
                    scale = (scale * k).clamp(1.0, 1e6);
                    center = w - DVec2::new(
                        (h.x - body.center().x) as f64,
                        -(h.y - body.center().y) as f64,
                    ) / scale as f64;
                }
            }
        }
        // Ctrl + middle-drag zooms about where the drag began, as in the
        // drawings (MU40); otherwise right or middle drag pans
        let ctrl_zoom =
            response.dragged_by(egui::PointerButton::Middle) && ui.input(|i| i.modifiers.command);
        if ctrl_zoom {
            let dy = response.drag_delta().y;
            if let Some(h) = ui.input(|i| i.pointer.press_origin()) {
                let k = (1.0 - dy / 200.0).clamp(0.5, 2.0);
                let off = DVec2::new(
                    (h.x - body.center().x) as f64,
                    -(h.y - body.center().y) as f64,
                );
                let w = center + off / scale as f64;
                scale = (scale * k).clamp(1.0, 1e6);
                center = w - off / scale as f64;
            }
        } else if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            let d = response.drag_delta();
            center -= DVec2::new(d.x as f64, -d.y as f64) / scale as f64;
        }
        // the zoom column's frame-selection button: what is picked here (MU37)
        let fit_picked = {
            let gl = self
                .profiles
                .guides
                .get(&prof.name)
                .cloned()
                .unwrap_or_default();
            let mut pts: Vec<DVec2> = vec![];
            for p in self
                .profiles
                .sel
                .into_iter()
                .chain(self.profiles.extra.iter().copied())
            {
                match p {
                    PSel::Corner(si, ci) => {
                        pts.extend(shapes.get(si).and_then(|s| s.points.get(ci)).copied())
                    }
                    PSel::Edge(si, e) => {
                        if let Some(s) = shapes.get(si).filter(|s| s.points.len() >= 2) {
                            let n = s.points.len();
                            pts.push(s.points[e % n]);
                            pts.push(s.points[(e + 1) % n]);
                        }
                    }
                    PSel::Shape(si) => {
                        pts.extend(shapes.get(si).map(|s| s.points.clone()).unwrap_or_default())
                    }
                    PSel::Guide(i) => {
                        pts.extend(gl.get(i).map(|(a, b)| vec![*a, *b]).unwrap_or_default())
                    }
                    PSel::Trace(_) => {}
                }
            }
            (!pts.is_empty()).then(|| {
                let lo = pts.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
                let hi = pts.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
                let span = (hi - lo).max(DVec2::splat(0.005));
                let k = ((body.width() - 160.0).max(60.0) as f64 / span.x)
                    .min((body.height() - 120.0).max(60.0) as f64 / span.y)
                    as f32;
                ((lo + hi) / 2.0, k.clamp(1.0, 1e6))
            })
        };
        let (nav, look) =
            crate::planview::nav_input(ui, body, ElementId::NIL, fit, fit_picked, center, scale);
        if let Some((c, k)) = nav {
            center = c;
            scale = k;
        }
        self.profiles.camera = Some((center, scale));
        let to_s = |p: DVec2| {
            body.center()
                + egui::vec2(
                    ((p.x - center.x) * scale as f64) as f32,
                    (-(p.y - center.y) * scale as f64) as f32,
                )
        };
        let to_w = |q: egui::Pos2| {
            center
                + DVec2::new(
                    (q.x - body.center().x) as f64,
                    -(q.y - body.center().y) as f64,
                ) / scale as f64
        };
        // the grid: a line every 10 mm (or 100 mm when small), stronger
        // every 50 mm
        // the finest step at least 10 px apart: 1, 5, 10, 50 mm…
        let step = [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
            .into_iter()
            .find(|g| g * scale as f64 >= 10.0)
            .unwrap_or(10.0);
        let (wlo, whi) = (to_w(body.left_bottom()), to_w(body.right_top()));
        let grid = egui::Stroke::new(1.0, pal.border.gamma_multiply(0.55));
        let grid5 = egui::Stroke::new(1.0, pal.border);
        let mut x = (wlo.x / step).floor() * step;
        while x < whi.x {
            let s = if ((x / step).round() as i64) % 5 == 0 {
                grid5
            } else {
                grid
            };
            painter.line_segment([to_s(DVec2::new(x, wlo.y)), to_s(DVec2::new(x, whi.y))], s);
            x += step;
        }
        let mut y = (wlo.y / step).floor() * step;
        while y < whi.y {
            let s = if ((y / step).round() as i64) % 5 == 0 {
                grid5
            } else {
                grid
            };
            painter.line_segment([to_s(DVec2::new(wlo.x, y)), to_s(DVec2::new(whi.x, y))], s);
            y += step;
        }
        // the trace references under it (MU37), and their lines to snap to
        let traces: Vec<(ElementId, creabim_core::Reference)> =
            creabim_core::reference::in_view(model, profiles::trace_view(&prof.name))
                .into_iter()
                .filter(|(_, r)| !r.hidden)
                .map(|(i, r)| {
                    let mut r = r.clone();
                    // a trace being dragged shows where it is going
                    if let Some((d, _, at0, delta)) = self.profiles.trace_drag {
                        if d == i {
                            r.at = at0 + delta;
                        }
                    }
                    (i, r)
                })
                .collect();
        let mut trace_lines: Vec<Vec<DVec2>> = vec![];
        {
            let mut cache = trace_cache();
            let mut n = 0;
            for (id, r) in &traces {
                let shown = cache.get(model, *id, r);
                crate::trace::draw(&painter, r, &shown, &to_s);
                if let crate::trace::Shown::Lines(ls) = &shown {
                    for l in ls.iter() {
                        // a huge drawing is cut short rather than slowing snaps
                        if n > 20_000 {
                            break;
                        }
                        n += l.len();
                        trace_lines.push(l.iter().map(|p| r.place(*p)).collect());
                    }
                }
            }
        }
        // the insertion point's axes and the guides
        let violet = egui::Color32::from_rgb(170, 120, 230);
        let dash = |a: egui::Pos2, b: egui::Pos2, c: egui::Color32| {
            crate::snaps::dashed_clipped(&painter, &[a, b], egui::Stroke::new(1.0, c), 6.0, 4.0);
        };
        let o = to_s(DVec2::ZERO);
        dash(
            egui::pos2(body.left(), o.y),
            egui::pos2(body.right(), o.y),
            violet,
        );
        dash(
            egui::pos2(o.x, body.top()),
            egui::pos2(o.x, body.bottom()),
            violet,
        );
        let key = prof.name.clone();
        let guides = self.profiles.guides.get(&key).cloned().unwrap_or_default();
        for (a, b) in &guides {
            let d = (*b - *a).normalize_or_zero() * 1e4;
            dash(to_s(*a - d), to_s(*a + d), violet);
        }
        // snaps, magnet-like (MU37): first points (corners, the insertion
        // point, where guides cross, where the temporary guides through two
        // corners cross), then lines (the guides, the insertion axes, the
        // temporary level and plumb guides through every corner), then the
        // grid. The temporary guides followed come back to be drawn.
        let pointer = response.hover_pos();
        let drawing_pts = self.profiles.drawing.clone();
        // the point worked from: perpendicular, parallel and angled guides
        // run through it (MU37)
        let track_from: Option<DVec2> = match self.profiles.tool {
            ProfTool::Draw => drawing_pts.last().copied(),
            ProfTool::Guide => self.profiles.guide_from,
            ProfTool::Move | ProfTool::Rotate => self.profiles.op.first().copied(),
            _ => None,
        };
        type Pick = Option<(f64, DVec2, Vec<(DVec2, DVec2)>)>;
        let snap =
            |q: egui::Pos2, skip: Option<(usize, usize)>| -> (DVec2, bool, Vec<(DVec2, DVec2)>) {
                let w = to_w(q);
                let tol = 9.0 / scale as f64;
                // the corners the temporary guides run through
                let mut corners: Vec<DVec2> = vec![DVec2::ZERO];
                for (si, s) in shapes.iter().enumerate() {
                    for (ci, c) in s.points.iter().enumerate() {
                        if Some((si, ci)) != skip {
                            corners.push(*c);
                        }
                    }
                }
                corners.extend(drawing_pts.iter().copied());
                let take = |p: DVec2, temp: Vec<(DVec2, DVec2)>, best: &mut Pick| {
                    let d = p.distance(w);
                    if d < tol && best.as_ref().is_none_or(|b| d < b.0) {
                        *best = Some((d, p, temp));
                    }
                };
                // points
                let mut best: Pick = None;
                for c in &corners {
                    take(*c, vec![], &mut best);
                }
                // the ends and corners of the traces' lines
                for l in &trace_lines {
                    for p in l {
                        take(*p, vec![], &mut best);
                    }
                }
                for (i, (a, b)) in guides.iter().enumerate() {
                    let u = (*b - *a).normalize_or_zero();
                    for (c, e) in &guides[i + 1..] {
                        let v = (*e - *c).normalize_or_zero();
                        let den = u.perp_dot(v);
                        if den.abs() > 1e-9 {
                            take(*a + u * ((*c - *a).perp_dot(v) / den), vec![], &mut best);
                        }
                    }
                }
                let near_x: Vec<DVec2> = corners
                    .iter()
                    .copied()
                    .filter(|c| (c.x - w.x).abs() < tol)
                    .collect();
                let near_y: Vec<DVec2> = corners
                    .iter()
                    .copied()
                    .filter(|c| (c.y - w.y).abs() < tol)
                    .collect();
                if best.is_none() {
                    // where a plumb guide through one corner meets a level one
                    // through another
                    for a in &near_x {
                        for b in &near_y {
                            let p = DVec2::new(a.x, b.y);
                            if p.distance(*a) > 1e-9 && p.distance(*b) > 1e-9 {
                                take(p, vec![(*a, p), (*b, p)], &mut best);
                            }
                        }
                    }
                    // where a guide line meets a temporary guide
                    for (a, b) in &guides {
                        let u = (*b - *a).normalize_or_zero();
                        for (cs, v) in [(&near_x, DVec2::Y), (&near_y, DVec2::X)] {
                            let den = u.perp_dot(v);
                            if den.abs() < 1e-9 {
                                continue;
                            }
                            for c in cs {
                                let p = *a + u * ((*c - *a).perp_dot(v) / den);
                                if p.distance(*c) > 1e-9 {
                                    take(p, vec![(*c, p)], &mut best);
                                }
                            }
                        }
                    }
                }
                if let Some((_, p, temp)) = best {
                    return (p, true, temp);
                }
                // lines: the guides, then the temporary guides through corners
                let g = if scale > 2000.0 { 0.001 } else { 0.005 };
                let round = |v: f64| (v / g).round() * g;
                let mut line: Pick = None;
                for (a, b) in &guides {
                    let d = (*b - *a).normalize_or_zero();
                    take(*a + d * (w - *a).dot(d), vec![], &mut line);
                }
                // on a trace's line
                for l in &trace_lines {
                    for seg in l.windows(2) {
                        let (a, b) = (seg[0], seg[1]);
                        let ab = b - a;
                        let t = ((w - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                        take(a + ab * t, vec![], &mut line);
                    }
                }
                for c in &near_x {
                    let p = DVec2::new(c.x, round(w.y));
                    take(p, vec![(*c, p)], &mut line);
                }
                for c in &near_y {
                    let p = DVec2::new(round(w.x), c.y);
                    take(p, vec![(*c, p)], &mut line);
                }
                if let Some(f) = track_from {
                    // parallel and square to every edge and guide, and every 15°
                    let mut dirs: Vec<DVec2> = (0..12)
                        .map(|k| DVec2::from_angle((k as f64 * 15.0).to_radians()))
                        .collect();
                    for sh in shapes.iter() {
                        let n = sh.points.len();
                        let k = if sh.open { n.saturating_sub(1) } else { n };
                        for e in 0..k {
                            let d = (sh.points[(e + 1) % n] - sh.points[e]).normalize_or_zero();
                            dirs.push(d);
                            dirs.push(d.perp());
                        }
                    }
                    for (a, b) in &guides {
                        let d = (*b - *a).normalize_or_zero();
                        dirs.push(d);
                        dirs.push(d.perp());
                    }
                    for d in dirs {
                        if d == DVec2::ZERO {
                            continue;
                        }
                        let t = (w - f).dot(d);
                        if (f + d * t).distance(w) < tol && t.abs() > tol {
                            let p = f + d * round(t);
                            take(p, vec![(f, p)], &mut line);
                        }
                    }
                }
                if let Some((_, p, temp)) = line {
                    return (p, true, temp);
                }
                // to the grid's millimetre
                (DVec2::new(round(w.x), round(w.y)), false, vec![])
            };
        // what is under the pointer: a corner, an edge, a guide, inside a shape
        let pick = |q: egui::Pos2| -> Option<PSel> {
            for (si, sh) in shapes.iter().enumerate() {
                if let Some(ci) = sh.points.iter().position(|c| to_s(*c).distance(q) < 8.0) {
                    return Some(PSel::Corner(si, ci));
                }
            }
            for (si, sh) in shapes.iter().enumerate() {
                let n = sh.points.len();
                let k = if sh.open { n.saturating_sub(1) } else { n };
                for e in 0..k {
                    let (a, b) = (to_s(sh.points[e]), to_s(sh.points[(e + 1) % n]));
                    let t = ((q - a).dot(b - a) / (b - a).length_sq().max(1e-6)).clamp(0.0, 1.0);
                    if (a + (b - a) * t).distance(q) < 6.0 {
                        return Some(PSel::Edge(si, e));
                    }
                }
            }
            for (i, (a, b)) in guides.iter().enumerate() {
                let (sa, sb) = (to_s(*a), to_s(*b));
                let ab = sb - sa;
                if ab.length() > 1e-3
                    && ((q - sa).x * ab.y - (q - sa).y * ab.x).abs() / ab.length() < 6.0
                {
                    return Some(PSel::Guide(i));
                }
            }
            let w = to_w(q);
            shapes
                .iter()
                .enumerate()
                .rev()
                .find(|(_, sh)| {
                    !sh.open
                        && sh.points.len() >= 3
                        && creabim_core::polygon::contains(&sh.points, w)
                })
                .map(|(si, _)| PSel::Shape(si))
                .or_else(|| {
                    traces
                        .iter()
                        .rev()
                        .find(|(_, r)| {
                            r.outline()
                                .is_some_and(|o| creabim_core::polygon::contains(&o, w))
                        })
                        .map(|(id, _)| PSel::Trace(*id))
                })
        };
        // a line to align to or with: an edge, a guide, an insertion axis
        let pick_line = |q: egui::Pos2| -> Option<(DVec2, DVec2, Option<PSel>)> {
            match pick(q) {
                Some(PSel::Edge(si, e)) => {
                    let sh = &shapes[si];
                    let n = sh.points.len();
                    Some((
                        sh.points[e],
                        sh.points[(e + 1) % n],
                        Some(PSel::Edge(si, e)),
                    ))
                }
                Some(PSel::Guide(i)) => guides.get(i).map(|(a, b)| (*a, *b, Some(PSel::Guide(i)))),
                _ => {
                    let o = to_s(DVec2::ZERO);
                    if (q.y - o.y).abs() < 6.0 {
                        Some((DVec2::ZERO, DVec2::X, None))
                    } else if (q.x - o.x).abs() < 6.0 {
                        Some((DVec2::ZERO, DVec2::Y, None))
                    } else {
                        None
                    }
                }
            }
        };
        // the shapes, the picked one with its corners and lengths
        let ink = pal.text;
        let accent = pal.accent;
        let blue = if dark {
            egui::Color32::from_rgb(120, 165, 245)
        } else {
            egui::Color32::from_rgb(59, 111, 214)
        };
        {
            let xf = crate::planview::Xf {
                rect: body,
                center,
                scale,
            };
            if prof.more.is_empty() && prof.flips.is_empty() {
                let outline = prof.outline();
                if outline.len() >= 3 {
                    crate::planview::fill_polygon(
                        &painter,
                        &xf,
                        &outline,
                        accent.gamma_multiply(0.10),
                    );
                }
            } else {
                let mut faces = prof.faces();
                faces.sort_by(|a, b| {
                    let (fa, fb) = (
                        creabim_core::polygon::signed_area2(&a.outer).abs(),
                        creabim_core::polygon::signed_area2(&b.outer).abs(),
                    );
                    fb.total_cmp(&fa)
                });
                let fill = accent.gamma_multiply(0.10);
                for (i, f) in faces.iter().enumerate() {
                    if f.outer.len() < 3 {
                        continue;
                    }
                    if f.filled {
                        crate::planview::fill_polygon(&painter, &xf, &f.outer, fill);
                    } else if faces[..i].iter().any(|g| g.filled && g.holds(f.sample)) {
                        // a void in a fill: the paper shows through
                        crate::planview::fill_polygon(&painter, &xf, &f.outer, paper);
                    }
                }
            }
        }
        let picked = self.profiles.shape.min(shapes.len().saturating_sub(1));
        let mut clicked_len: Option<(usize, usize, f64)> = None;
        for (si, s) in shapes.iter().enumerate() {
            let scr: Vec<egui::Pos2> = s.points.iter().map(|p| to_s(*p)).collect();
            let stroke = egui::Stroke::new(if si == picked { 2.0 } else { 1.5 }, ink);
            if s.open {
                painter.add(egui::Shape::line(scr.clone(), stroke));
            } else {
                painter.add(egui::Shape::closed_line(scr.clone(), stroke));
            }
            if si != picked || !editable {
                continue;
            }
            let n = scr.len();
            let edges = if s.open { n.saturating_sub(1) } else { n };
            for e in 0..edges {
                let (a, b) = (s.points[e], s.points[(e + 1) % n]);
                let (sa, sb) = (scr[e], scr[(e + 1) % n]);
                if sa.distance(sb) < 36.0 || self.profiles.hide_dims {
                    continue;
                }
                let mid = sa + (sb - sa) / 2.0;
                let nrm = egui::vec2(-(sb - sa).y, (sb - sa).x).normalized() * 12.0;
                let at = mid + nrm;
                let text = crate::units::len(a.distance(b));
                let g =
                    painter.layout_no_wrap(text.clone(), egui::FontId::proportional(11.0), blue);
                let r = Rect::from_center_size(at, g.size() + egui::vec2(6.0, 2.0));
                let hot = pointer.is_some_and(|p| r.contains(p));
                painter.rect(
                    r,
                    3.0,
                    paper,
                    egui::Stroke::new(if hot { 1.2 } else { 0.0 }, blue),
                    egui::StrokeKind::Middle,
                );
                painter.galley(r.center() - g.size() / 2.0, g, blue);
                if hot && response.clicked() && self.profiles.tool == ProfTool::Select {
                    clicked_len = Some((si, e, a.distance(b)));
                }
            }
            for (ci, c) in scr.iter().enumerate() {
                let hot = pointer.is_some_and(|p| p.distance(*c) < 7.0)
                    || self.profiles.drag == Some((si, ci));
                painter.circle(
                    *c,
                    if hot { 5.0 } else { 3.5 },
                    paper,
                    egui::Stroke::new(1.4, accent),
                );
            }
        }
        // the overall size, dimensioned under and beside it (D hides them)
        let lo = all_pts
            .iter()
            .fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
        let hi = all_pts
            .iter()
            .fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
        let (bl, br, tr) = (to_s(lo), to_s(DVec2::new(hi.x, lo.y)), to_s(hi));
        if !self.profiles.hide_dims {
            let y0 = bl.y + 44.0;
            painter.line_segment(
                [egui::pos2(bl.x, y0), egui::pos2(br.x, y0)],
                egui::Stroke::new(1.0, blue),
            );
            painter.text(
                egui::pos2((bl.x + br.x) / 2.0, y0 + 3.0),
                egui::Align2::CENTER_TOP,
                crate::units::len(hi.x - lo.x),
                egui::FontId::proportional(11.0),
                blue,
            );
            let x0 = br.x + 50.0;
            painter.line_segment(
                [egui::pos2(x0, br.y), egui::pos2(x0, tr.y)],
                egui::Stroke::new(1.0, blue),
            );
            painter.text(
                egui::pos2(x0 + 4.0, (br.y + tr.y) / 2.0),
                egui::Align2::LEFT_CENTER,
                crate::units::len(hi.y - lo.y),
                egui::FontId::proportional(11.0),
                blue,
            );
        }
        // what is picked, in the accent
        let picked: Vec<PSel> = self
            .profiles
            .sel
            .into_iter()
            .chain(self.profiles.extra.iter().copied())
            .collect();
        for &sel in &picked {
            if let PSel::Trace(id) = sel {
                if let Some(c) = traces
                    .iter()
                    .find(|t| t.0 == id)
                    .and_then(|t| t.1.outline())
                {
                    let orange = crate::planview::accent_selected();
                    let mut pts: Vec<egui::Pos2> = c.iter().map(|p| to_s(*p)).collect();
                    pts.push(pts[0]);
                    crate::snaps::dashed_clipped(
                        &painter,
                        &pts,
                        egui::Stroke::new(1.5, orange),
                        6.0,
                        4.0,
                    );
                    let mid = c.iter().fold(DVec2::ZERO, |a, p| a + *p) / 4.0;
                    for p in c.iter().chain(std::iter::once(&mid)) {
                        let r = Rect::from_center_size(to_s(*p), egui::vec2(9.0, 9.0));
                        painter.rect(
                            r,
                            1.0,
                            paper,
                            egui::Stroke::new(1.5, orange),
                            egui::StrokeKind::Middle,
                        );
                    }
                }
                continue;
            }
            for line in sel_lines(&shapes, &guides, &traces, sel) {
                let pts: Vec<egui::Pos2> = line.iter().map(|p| to_s(*p)).collect();
                if pts.len() == 1 {
                    painter.circle_filled(pts[0], 5.0, accent);
                } else if matches!(sel, PSel::Guide(_)) {
                    let d = (line[1] - line[0]).normalize_or_zero() * 1e4;
                    painter.line_segment(
                        [to_s(line[0] - d), to_s(line[0] + d)],
                        egui::Stroke::new(1.6, violet),
                    );
                } else {
                    painter.add(egui::Shape::line(pts, egui::Stroke::new(3.0, accent)));
                }
            }
        }
        // the insertion point
        painter.line_segment(
            [o - egui::vec2(9.0, 0.0), o + egui::vec2(9.0, 0.0)],
            egui::Stroke::new(2.0, accent),
        );
        painter.line_segment(
            [o - egui::vec2(0.0, 9.0), o + egui::vec2(0.0, 9.0)],
            egui::Stroke::new(2.0, accent),
        );
        painter.text(
            o + egui::vec2(-8.0, -6.0),
            egui::Align2::RIGHT_BOTTOM,
            "insertion",
            egui::FontId::proportional(11.0),
            accent,
        );
        painter.text(
            body.left_top() + egui::vec2(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "Profile · {}{}",
                prof.name,
                if editable {
                    ""
                } else {
                    " · preset (Duplicate to change it)"
                }
            ),
            egui::FontId::proportional(12.0),
            pal.muted,
        );
        // the snapped point under the pointer, with the temporary guides it
        // follows dashed from their corners
        let snapped = pointer.map(|p| snap(p, self.profiles.drag));
        let here = snapped.as_ref().map(|(p, hit, _)| (*p, *hit));
        let snapping = self.profiles.tool != ProfTool::Select || self.profiles.drag.is_some();
        if let (Some((h, true, temp)), true) = (snapped.as_ref(), snapping) {
            let tg = crate::snaps::GUIDE;
            for (a, b) in temp {
                let (sa, sb) = (to_s(*a), to_s(*b));
                let u = (sb - sa).normalized();
                let reach = if u.x.is_finite() {
                    u * 30.0
                } else {
                    egui::Vec2::ZERO
                };
                crate::snaps::dashed_clipped(
                    &painter,
                    &[sa, sb + reach],
                    egui::Stroke::new(1.1, tg),
                    5.0,
                    4.0,
                );
            }
            painter.rect_stroke(
                Rect::from_center_size(to_s(*h), egui::vec2(10.0, 10.0)),
                1.0,
                egui::Stroke::new(1.5, accent),
                egui::StrokeKind::Middle,
            );
        }
        // the Insertion tool on: the insertion point waits at the pointer,
        // its axes across the view, the present one ringed (MU37)
        if self.profiles.tool == ProfTool::Insertion && editable {
            painter.circle_stroke(o, 13.0, egui::Stroke::new(1.5, accent));
            if let Some((h, _)) = here {
                let p = to_s(h);
                let faint = egui::Stroke::new(1.0, accent.gamma_multiply(0.55));
                painter.add(egui::Shape::dashed_line(
                    &[egui::pos2(body.left(), p.y), egui::pos2(body.right(), p.y)],
                    faint,
                    4.0,
                    4.0,
                ));
                painter.add(egui::Shape::dashed_line(
                    &[egui::pos2(p.x, body.top()), egui::pos2(p.x, body.bottom())],
                    faint,
                    4.0,
                    4.0,
                ));
                let st = egui::Stroke::new(2.0, accent);
                painter.line_segment([p - egui::vec2(11.0, 0.0), p + egui::vec2(11.0, 0.0)], st);
                painter.line_segment([p - egui::vec2(0.0, 11.0), p + egui::vec2(0.0, 11.0)], st);
                painter.circle_stroke(p, 6.0, st);
                painter.text(
                    p + egui::vec2(12.0, 12.0),
                    egui::Align2::LEFT_TOP,
                    format!(
                        "insertion here  ({}, {})",
                        crate::units::len(h.x),
                        crate::units::len(h.y)
                    ),
                    egui::FontId::proportional(11.0),
                    accent,
                );
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        let mut write: Option<(Vec<NamedProfile>, &'static str, bool)> = None;
        let mut trace_move: Vec<(ElementId, DVec2, f64)> = vec![];
        let mut trace_calib: Option<(ElementId, DVec2, DVec2, f64)> = None;
        let edit = |f: &mut dyn FnMut(&mut NamedProfile)| -> Vec<NamedProfile> {
            let mut all = own_list.clone();
            if let Some(x) = all
                .iter_mut()
                .find(|x| Some(x.name.as_str()) == cur.as_deref())
            {
                f(x);
            }
            all
        };
        // a right-click cancels what is under way, as elsewhere (MU37): a
        // shape being drawn, a guide's first point, a move or rotation, an
        // align, a calibration, a box; with nothing under way it puts the
        // tool down (back to Select)
        let mut menu_act: Option<&'static str> = None;
        let idle_select = {
            let p = &self.profiles;
            p.tool == ProfTool::Select
                && p.drawing.is_empty()
                && p.guide_from.is_none()
                && p.op.is_empty()
                && p.align_ref.is_none()
                && p.calib.is_none()
                && p.boxsel.is_none()
                && !p.copy
        };
        if idle_select && response.secondary_clicked() {
            // the menu acts on what is under the pointer when it isn't picked
            if let Some(h) = pointer.and_then(pick) {
                let picked_now: Vec<PSel> = self
                    .profiles
                    .sel
                    .into_iter()
                    .chain(self.profiles.extra.iter().copied())
                    .collect();
                if !picked_now.contains(&h) {
                    self.profiles.sel = Some(h);
                    self.profiles.extra.clear();
                }
            }
        }
        if idle_select {
            let has_pick = self.profiles.sel.is_some();
            let dims_label = if self.profiles.hide_dims {
                "Show dimensions"
            } else {
                "Hide dimensions"
            };
            response.context_menu(|ui| {
                ui.set_min_width(190.0);
                let item = |ui: &mut egui::Ui, label: &str, key: &str, on: bool| {
                    ui.add_enabled(on, egui::Button::new(label).shortcut_text(key))
                        .clicked()
                };
                if item(ui, "Delete", "Del", has_pick) {
                    menu_act = Some("delete");
                }
                ui.separator();
                if item(ui, "Move", "M", has_pick) {
                    menu_act = Some("move");
                }
                if item(ui, "Rotate", "R", has_pick) {
                    menu_act = Some("rotate");
                }
                if item(ui, "Duplicate", "Shift D", has_pick) {
                    menu_act = Some("copy");
                }
                if item(ui, "Align", "", true) {
                    menu_act = Some("align");
                }
                ui.separator();
                if item(ui, "Frame what is picked", "F", has_pick) {
                    menu_act = Some("frame");
                }
                if item(ui, "Select all", "A", true) {
                    menu_act = Some("all");
                }
                if item(ui, "Pick void", "", true) {
                    menu_act = Some("void");
                }
                if item(ui, dims_label, "D", true) {
                    menu_act = Some("dims");
                }
                if menu_act.is_some() {
                    ui.close();
                }
            });
        }
        if response.secondary_clicked() && !idle_select {
            let p = &mut self.profiles;
            let busy = !p.drawing.is_empty()
                || p.guide_from.is_some()
                || !p.op.is_empty()
                || p.align_ref.is_some()
                || p.calib.is_some()
                || p.boxsel.is_some()
                || p.copy;
            if busy {
                p.drawing.clear();
                p.guide_from = None;
                p.op.clear();
                p.align_ref = None;
                p.calib = None;
                p.boxsel = None;
                p.copy = false;
                self.status = "Cancelled.".into();
            } else if p.tool != ProfTool::Select {
                p.tool = ProfTool::Select;
                self.status = "Select: the tool is put down (right-click).".into();
            }
        }
        match self.profiles.tool {
            ProfTool::Select if editable => {
                if response.drag_started_by(egui::PointerButton::Primary) {
                    let q = ui.input(|i| i.pointer.press_origin());
                    self.profiles.drag = q.and_then(|q| {
                        shapes.iter().enumerate().find_map(|(si, s)| {
                            s.points
                                .iter()
                                .position(|c| to_s(*c).distance(q) < 8.0)
                                .map(|ci| (si, ci))
                        })
                    });
                    // not a corner: an edge, dragged whole
                    self.profiles.edge_drag = None;
                    if let (None, Some(q)) = (self.profiles.drag, q) {
                        'find: for (si, s) in shapes.iter().enumerate() {
                            let n = s.points.len();
                            let k = if s.open { n.saturating_sub(1) } else { n };
                            for e in 0..k {
                                let (a, b) = (s.points[e], s.points[(e + 1) % n]);
                                let (sa, sb) = (to_s(a), to_s(b));
                                let t = ((q - sa).dot(sb - sa) / (sb - sa).length_sq().max(1e-6))
                                    .clamp(0.0, 1.0);
                                if (sa + (sb - sa) * t).distance(q) < 6.0 {
                                    self.profiles.edge_drag = Some((si, e, snap(q, None).0, a, b));
                                    self.profiles.shape = si;
                                    break 'find;
                                }
                            }
                        }
                    }
                    if let Some((si, _)) = self.profiles.drag {
                        self.profiles.shape = si;
                    }
                    // a guide under it: dragged square to itself, as in 3D
                    self.profiles.guide_drag = None;
                    if let (None, None, Some(q)) = (self.profiles.drag, self.profiles.edge_drag, q)
                    {
                        if let Some(PSel::Guide(i)) = pick(q) {
                            if let Some(&(a, b)) = guides.get(i) {
                                self.profiles.guide_drag = Some((i, snap(q, None).0, a, b));
                                self.profiles.sel = Some(PSel::Guide(i));
                                self.profiles.extra.clear();
                            }
                        }
                    }
                    // a picked trace under it: dragged by its middle
                    if let (None, None, None, Some(q)) = (
                        self.profiles.drag,
                        self.profiles.edge_drag,
                        self.profiles.guide_drag,
                        q,
                    ) {
                        if let Some(PSel::Trace(id)) = pick(q) {
                            if let Some((_, r)) = traces.iter().find(|t| t.0 == id) {
                                self.profiles.trace_drag = Some((id, to_w(q), r.at, DVec2::ZERO));
                                self.profiles.sel = Some(PSel::Trace(id));
                                self.profiles.extra.clear();
                                self.selection.only(id);
                            }
                        }
                    }
                    // nothing under it: a box pick
                    if self.profiles.drag.is_none()
                        && self.profiles.edge_drag.is_none()
                        && self.profiles.guide_drag.is_none()
                        && self.profiles.trace_drag.is_none()
                    {
                        self.profiles.boxsel = q;
                    }
                }
                if let (Some(a), Some(b)) = (self.profiles.boxsel, pointer) {
                    let r = Rect::from_two_pos(a, b);
                    painter.rect(
                        r,
                        0.0,
                        accent.gamma_multiply(0.08),
                        egui::Stroke::new(1.0, accent),
                        egui::StrokeKind::Middle,
                    );
                }
                if let Some(a) = self.profiles.boxsel.take_if(|_| response.drag_stopped()) {
                    if let Some(b) = pointer.or(ui.input(|i| i.pointer.latest_pos())) {
                        let r = Rect::from_two_pos(a, b);
                        let inside = |p: DVec2| r.contains(to_s(p));
                        let mut got: Vec<PSel> = vec![];
                        for (si, sh) in shapes.iter().enumerate() {
                            let n = sh.points.len();
                            if n > 0 && sh.points.iter().all(|p| inside(*p)) {
                                got.push(PSel::Shape(si));
                                continue;
                            }
                            let k = if sh.open { n.saturating_sub(1) } else { n };
                            for e in 0..k {
                                if inside(sh.points[e]) && inside(sh.points[(e + 1) % n]) {
                                    got.push(PSel::Edge(si, e));
                                }
                            }
                            for (ci, p) in sh.points.iter().enumerate() {
                                if inside(*p) {
                                    got.push(PSel::Corner(si, ci));
                                }
                            }
                        }
                        for (i, (ga, gb)) in guides.iter().enumerate() {
                            if inside(*ga) && inside(*gb) {
                                got.push(PSel::Guide(i));
                            }
                        }
                        for (id, t) in &traces {
                            if t.outline().is_some_and(|o| o.iter().all(|p| inside(*p))) {
                                got.push(PSel::Trace(*id));
                            }
                        }
                        let add = ui.input(|i| i.modifiers.shift);
                        let mut all: Vec<PSel> = if add { picked.clone() } else { vec![] };
                        for g in got {
                            if !all.contains(&g) {
                                all.push(g);
                            }
                        }
                        self.profiles.sel = all.first().copied();
                        self.profiles.extra = all.into_iter().skip(1).collect();
                        let k = picked.len();
                        let _ = k;
                        self.status = match self.profiles.sel {
                            Some(_) => format!("{} picked: M moves, R rotates, F frames them, Delete removes corners and guides.", 1 + self.profiles.extra.len()),
                            None => "Nothing inside the box.".into(),
                        };
                    }
                }
                if let (Some((si, ci)), Some((w, _)), true) = (
                    self.profiles.drag,
                    here,
                    response.dragged_by(egui::PointerButton::Primary),
                ) {
                    write = Some((
                        edit(&mut |x| {
                            if let Some(v) = point_mut(x, si).and_then(|v| v.get_mut(ci)) {
                                *v = w;
                            }
                        }),
                        "Shape profile",
                        true,
                    ));
                }
                if let (Some((si, e, from, a0, b0)), Some((w, _)), true) = (
                    self.profiles.edge_drag,
                    here,
                    response.dragged_by(egui::PointerButton::Primary),
                ) {
                    // square to itself: an edge slides out or in, not along
                    let nrm = (b0 - a0).perp().normalize_or_zero();
                    let d = nrm * (w - from).dot(nrm);
                    write = Some((
                        edit(&mut |x| {
                            if let Some(v) = point_mut(x, si) {
                                let n = v.len();
                                if n >= 2 {
                                    v[e % n] = a0 + d;
                                    v[(e + 1) % n] = b0 + d;
                                }
                            }
                        }),
                        "Move profile edge",
                        true,
                    ));
                }
                // (the pointer as it is: the guide would snap to itself)
                if let (Some((i, from, a0, b0)), Some(w), true) = (
                    self.profiles.guide_drag,
                    pointer.map(to_w),
                    response.dragged_by(egui::PointerButton::Primary),
                ) {
                    let nrm = (b0 - a0).perp().normalize_or_zero();
                    let d = nrm * (w - from).dot(nrm);
                    if let Some(g) = self
                        .profiles
                        .guides
                        .get_mut(&key)
                        .and_then(|g| g.get_mut(i))
                    {
                        *g = (a0 + d, b0 + d);
                    }
                    painter.text(
                        to_s(w) + egui::vec2(14.0, -14.0),
                        egui::Align2::LEFT_BOTTOM,
                        crate::units::len((w - from).dot(nrm).abs()),
                        egui::FontId::proportional(11.0),
                        violet,
                    );
                }
                if let (Some((id, from, at0, _)), Some(w), true) = (
                    self.profiles.trace_drag,
                    pointer.map(to_w),
                    response.dragged_by(egui::PointerButton::Primary),
                ) {
                    self.profiles.trace_drag = Some((id, from, at0, w - from));
                }
                if response.drag_stopped() {
                    self.profiles.drag = None;
                    self.profiles.edge_drag = None;
                    self.profiles.guide_drag = None;
                    // the trace lands: one change, one undo step
                    if let Some((id, _, at0, delta)) = self.profiles.trace_drag.take() {
                        if delta.length() > 1e-9 {
                            trace_move.push((id, at0 + delta, 0.0));
                        }
                    }
                }
                // a click picks a corner, an edge, a guide or a shape; Shift
                // adds it to what is picked, or takes it off
                if response.clicked() && clicked_len.is_none() {
                    let hit = pointer.and_then(pick);
                    if ui.input(|i| i.modifiers.shift) {
                        if let Some(h) = hit {
                            let mut all = picked.clone();
                            match all.iter().position(|x| *x == h) {
                                Some(i) => {
                                    all.remove(i);
                                }
                                None => all.insert(0, h),
                            }
                            self.profiles.sel = all.first().copied();
                            self.profiles.extra = all.into_iter().skip(1).collect();
                        }
                    } else {
                        self.profiles.sel = hit;
                        // a trace picked here is the reference picked in the
                        // app: Properties shows its settings (MU37)
                        if let Some(PSel::Trace(id)) = hit {
                            self.selection.only(id);
                        }
                        self.profiles.extra.clear();
                    }
                    if let Some(PSel::Corner(si, _) | PSel::Edge(si, _) | PSel::Shape(si)) = hit {
                        self.profiles.shape = si;
                    }
                }
            }
            ProfTool::Draw if editable => {
                // A and L switch the next side between an arc and a line
                let (key_a, key_l) = ui.input(|i| {
                    (
                        i.modifiers.is_none() && i.key_pressed(egui::Key::A),
                        i.modifiers.is_none() && i.key_pressed(egui::Key::L),
                    )
                });
                if key_a && !ui.ctx().egui_wants_keyboard_input() {
                    self.profiles.arc = true;
                    self.profiles.arc_end = None;
                }
                if key_l && !ui.ctx().egui_wants_keyboard_input() {
                    self.profiles.arc = false;
                    self.profiles.arc_end = None;
                }
                let (arc, arc_end) = (self.profiles.arc, self.profiles.arc_end);
                let d = &mut self.profiles.drawing;
                if let Some((w, _)) = here {
                    let mut pts: Vec<egui::Pos2> = d.iter().map(|p| to_s(*p)).collect();
                    if !pts.is_empty() {
                        // the arc being drawn follows the pointer through
                        // its middle
                        match (arc, arc_end, d.last()) {
                            (true, Some(e), Some(p0)) => {
                                pts.extend(arc_points(*p0, w, e, 24).into_iter().map(&to_s))
                            }
                            _ => pts.push(to_s(w)),
                        }
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.6, accent)));
                    }
                }
                let tol = 9.0 / scale as f64;
                let on_shape = |q: DVec2| {
                    shapes.iter().any(|sh| {
                        let n = sh.points.len();
                        let k = if sh.open { n.saturating_sub(1) } else { n };
                        (0..k).any(|e| {
                            let (a, b) = (sh.points[e], sh.points[(e + 1) % n]);
                            let ab = b - a;
                            let t =
                                ((q - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                            (a + ab * t).distance(q) < tol
                        })
                    })
                };
                let finish = |d: &mut Vec<DVec2>, closed: bool| -> Option<Vec<NamedProfile>> {
                    let mut pts = std::mem::take(d);
                    // the same corner twice in a row (a double-click) counts once
                    pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
                    let mut closed = closed;
                    if !closed && pts.len() >= 3 {
                        let (first, last) = (pts[0], pts[pts.len() - 1]);
                        if first.distance(last) < tol {
                            pts.pop();
                            closed = true;
                        } else if on_shape(first) && on_shape(last) {
                            // its ends rest on another shape: it closes against it
                            closed = true;
                        }
                    }
                    if pts.len() < if closed { 3 } else { 2 } {
                        return None;
                    }
                    Some(edit(&mut |x| {
                        x.more.push(ProfileShape {
                            points: pts.clone(),
                            open: !closed,
                            thickness: None,
                        });
                    }))
                };
                // a click, or a press let go after a small slip, places a
                // corner (MU37: quick clicks were lost)
                let dbl = response.double_clicked()
                    || (response.hovered()
                        && ui.input(|i| {
                            i.pointer
                                .button_double_clicked(egui::PointerButton::Primary)
                        }));
                if dbl {
                    // a double-click finishes the shape where it is
                    if let Some(all) = finish(d, false) {
                        let closed = all
                            .iter()
                            .find(|x| Some(x.name.as_str()) == cur.as_deref())
                            .and_then(|x| x.more.last())
                            .is_some_and(|m| !m.open);
                        write = Some((
                            all,
                            if closed {
                                "Draw profile shape"
                            } else {
                                "Draw profile wire"
                            },
                            false,
                        ));
                    }
                } else if response.clicked()
                    || response.drag_stopped_by(egui::PointerButton::Primary)
                {
                    if let Some((w, _)) = here {
                        if d.len() >= 3 && to_s(d[0]).distance(to_s(w)) < 9.0 {
                            if let Some(all) = finish(d, true) {
                                write = Some((all, "Draw profile shape", false));
                            }
                        } else if d.len() >= 2
                            && d.last().is_some_and(|l| to_s(*l).distance(to_s(w)) < 6.0)
                        {
                            // the last corner clicked again (as a double-click
                            // does) finishes the shape (MU37)
                            if let Some(all) = finish(d, false) {
                                write = Some((all, "Draw profile shape", false));
                            }
                        } else if arc && !d.is_empty() {
                            // an arc: its end, then a point it passes through
                            match self.profiles.arc_end.take() {
                                None => self.profiles.arc_end = Some(w),
                                Some(e) => {
                                    let p0 = d[d.len() - 1];
                                    d.extend(arc_points(p0, w, e, 24));
                                }
                            }
                        } else {
                            d.push(w);
                        }
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(all) = finish(d, false) {
                        write = Some((all, "Draw profile shape", false));
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    d.clear();
                }
            }
            ProfTool::Insertion if editable => {
                if let (true, Some((w, _))) = (response.clicked(), here) {
                    write = Some((
                        edit(&mut |x| {
                            for q in x
                                .points
                                .iter_mut()
                                .chain(x.more.iter_mut().flat_map(|m| m.points.iter_mut()))
                            {
                                *q -= w;
                            }
                        }),
                        "Place insertion point",
                        false,
                    ));
                    // the drawing stays still on screen
                    self.profiles.camera = Some((center - w, scale));
                    let g = self.profiles.guides.entry(key.clone()).or_default();
                    for (a, b) in g.iter_mut() {
                        *a -= w;
                        *b -= w;
                    }
                }
            }
            ProfTool::Guide => {
                // two clicks, or press at one point and let go at the other
                if response.drag_started_by(egui::PointerButton::Primary)
                    && self.profiles.guide_from.is_none()
                {
                    if let Some(q) = ui.input(|i| i.pointer.press_origin()) {
                        self.profiles.guide_from = Some(snap(q, None).0);
                    }
                }
                let tap =
                    response.clicked() || response.drag_stopped_by(egui::PointerButton::Primary);
                if let (true, Some((w, _))) = (tap, here) {
                    match self.profiles.guide_from.take() {
                        None => self.profiles.guide_from = Some(w),
                        Some(a) if to_s(a).distance(to_s(w)) > 6.0 => self
                            .profiles
                            .guides
                            .entry(key.clone())
                            .or_default()
                            .push((a, w)),
                        Some(a) => self.profiles.guide_from = Some(a),
                    }
                }
                if let (Some(a), Some((w, _))) = (self.profiles.guide_from, here) {
                    // the whole line it will be, and the two points
                    let d = (w - a).normalize_or_zero() * 1e4;
                    if d != DVec2::ZERO {
                        dash(to_s(a - d), to_s(a + d), violet.gamma_multiply(0.6));
                    }
                    painter.line_segment([to_s(a), to_s(w)], egui::Stroke::new(1.4, violet));
                    painter.circle_filled(to_s(a), 3.5, violet);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.profiles.guide_from = None;
                }
            }
            ProfTool::Move | ProfTool::Rotate
                if editable
                    || picked
                        .iter()
                        .all(|p| matches!(p, PSel::Guide(_) | PSel::Trace(_))) =>
            {
                let tap =
                    response.clicked() || response.drag_stopped_by(egui::PointerButton::Primary);
                let moving = self.profiles.tool == ProfTool::Move;
                // nothing picked yet: the first click picks
                if picked.is_empty() {
                    if let (true, Some(q)) = (tap, pointer) {
                        self.profiles.sel = pick(q);
                    }
                } else if let Some((w, hit)) = here {
                    let op = self.profiles.op.clone();
                    // the transform the pointer shows now
                    let f: Option<Box<dyn Fn(DVec2) -> DVec2>> = match (moving, op.as_slice()) {
                        (true, [base]) => {
                            let d = w - *base;
                            Some(Box::new(move |p| p + d))
                        }
                        (false, [c, from]) => {
                            let mut a = (w - *c).to_angle() - (*from - *c).to_angle();
                            if !hit {
                                let k = 5f64.to_radians();
                                a = (a / k).round() * k;
                            }
                            let (c, r) = (*c, DVec2::from_angle(a));
                            painter.text(
                                to_s(c) + egui::vec2(12.0, -18.0),
                                egui::Align2::LEFT_BOTTOM,
                                format!("{:.1}°", a.to_degrees()),
                                egui::FontId::proportional(12.0),
                                accent,
                            );
                            painter.line_segment(
                                [to_s(c), to_s(*from)],
                                egui::Stroke::new(1.0, accent.gamma_multiply(0.6)),
                            );
                            Some(Box::new(move |p| c + r.rotate(p - c)))
                        }
                        _ => None,
                    };
                    for (f, &sel) in f.iter().flat_map(|f| picked.iter().map(move |s| (f, s))) {
                        // a ghost of it where it goes
                        for line in sel_lines(&shapes, &guides, &traces, sel) {
                            let pts: Vec<egui::Pos2> = line.iter().map(|p| to_s(f(*p))).collect();
                            if pts.len() == 1 {
                                painter.circle_stroke(pts[0], 5.0, egui::Stroke::new(1.5, accent));
                            } else {
                                crate::snaps::dashed_clipped(
                                    &painter,
                                    &pts,
                                    egui::Stroke::new(1.6, accent),
                                    6.0,
                                    4.0,
                                );
                            }
                        }
                    }
                    let copying = self.profiles.copy || ui.input(|i| i.modifiers.shift);
                    if copying && f.is_some() {
                        painter.text(
                            to_s(w) + egui::vec2(14.0, -16.0),
                            egui::Align2::LEFT_BOTTOM,
                            "+ copy",
                            egui::FontId::proportional(12.0),
                            accent,
                        );
                    }
                    for p in &op {
                        painter.circle_filled(to_s(*p), 3.5, accent);
                    }
                    if tap {
                        let need = if moving { 1 } else { 2 };
                        if op.len() < need {
                            self.profiles.op.push(w);
                        } else if let Some(f) = f {
                            self.profiles.op.clear();
                            let copy = copying;
                            self.profiles.copy = false;
                            for &sel in &picked {
                                match sel {
                                    PSel::Guide(i) if copy => {
                                        if let Some(g) = self.profiles.guides.get_mut(&key) {
                                            if let Some(&(a, b)) = g.get(i) {
                                                g.push((f(a), f(b)));
                                            }
                                        }
                                    }
                                    PSel::Guide(i) => {
                                        if let Some(g) = self
                                            .profiles
                                            .guides
                                            .get_mut(&key)
                                            .and_then(|g| g.get_mut(i))
                                        {
                                            *g = (f(g.0), f(g.1));
                                        }
                                    }
                                    PSel::Trace(_) if copy => {}
                                    PSel::Trace(id) => {
                                        if let Some((_, r)) = traces.iter().find(|t| t.0 == id) {
                                            let at = f(r.at);
                                            let turn =
                                                (f(r.at + DVec2::X) - at).to_angle().to_degrees();
                                            trace_move.push((id, at, turn));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            if editable
                                && picked
                                    .iter()
                                    .any(|p| !matches!(p, PSel::Guide(_) | PSel::Trace(_)))
                            {
                                if copy {
                                    // the shapes picked (or holding what is picked), copied
                                    let sis: std::collections::BTreeSet<usize> = picked
                                        .iter()
                                        .filter_map(|p| match *p {
                                            PSel::Corner(si, _)
                                            | PSel::Edge(si, _)
                                            | PSel::Shape(si) => Some(si),
                                            _ => None,
                                        })
                                        .collect();
                                    let n = sis.len();
                                    write = Some((
                                        edit(&mut |x| {
                                            let all = x.shapes();
                                            for &si in &sis {
                                                if let Some(sh) = all.get(si) {
                                                    x.more.push(ProfileShape {
                                                        points: sh
                                                            .points
                                                            .iter()
                                                            .map(|p| f(*p))
                                                            .collect(),
                                                        open: sh.open,
                                                        thickness: sh.thickness,
                                                    });
                                                }
                                            }
                                        }),
                                        if moving {
                                            "Copy in profile"
                                        } else {
                                            "Rotate a copy in profile"
                                        },
                                        false,
                                    ));
                                    self.status = format!("{n} shape{} copied. Shift at the last click (or Shift+D first) copies; without it, it moves.", if n == 1 { "" } else { "s" });
                                } else {
                                    write = Some((
                                        edit(&mut |x| apply_many(x, &picked, &*f)),
                                        if moving {
                                            "Move in profile"
                                        } else {
                                            "Rotate in profile"
                                        },
                                        false,
                                    ));
                                }
                            }
                        }
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    if self.profiles.op.is_empty() {
                        self.profiles.sel = None;
                        self.profiles.extra.clear();
                    }
                    self.profiles.op.clear();
                }
            }
            ProfTool::Void if editable => {
                // the face under the pointer, outlined; a click turns it over
                if let Some(q) = pointer {
                    let w = to_w(q);
                    let fs = prof.faces();
                    let under = fs.iter().filter(|f| f.holds(w)).min_by(|a, b| {
                        creabim_core::polygon::signed_area2(&a.outer)
                            .total_cmp(&creabim_core::polygon::signed_area2(&b.outer))
                    });
                    if let Some(f) = under {
                        let xf = crate::planview::Xf {
                            rect: body,
                            center,
                            scale,
                        };
                        let tint = if f.filled {
                            egui::Color32::from_rgb(220, 80, 80)
                        } else {
                            accent
                        };
                        crate::planview::fill_polygon(
                            &painter,
                            &xf,
                            &f.outer,
                            tint.gamma_multiply(0.22),
                        );
                        let pts: Vec<egui::Pos2> = f.outer.iter().map(|p| to_s(*p)).collect();
                        painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(2.0, tint)));
                        painter.text(
                            q + egui::vec2(14.0, 14.0),
                            egui::Align2::LEFT_TOP,
                            if f.filled { "make void" } else { "fill back" },
                            egui::FontId::proportional(11.0),
                            tint,
                        );
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            let filled = f.filled;
                            write = Some((
                                edit(&mut |x| {
                                    x.toggle_face(w);
                                }),
                                if filled { "Make void" } else { "Fill void" },
                                false,
                            ));
                        }
                    }
                }
            }
            ProfTool::Calibrate => {
                let id = match self.profiles.sel {
                    Some(PSel::Trace(id)) => Some(id),
                    _ => None,
                };
                match (id, self.profiles.calib.clone()) {
                    (None, _) => {
                        // the first click picks the trace
                        if let (true, Some(q)) = (response.clicked(), pointer) {
                            self.profiles.sel = pick(q).filter(|p| matches!(p, PSel::Trace(_)));
                        }
                    }
                    (Some(_), None) => {
                        let op = self.profiles.op.clone();
                        if let (Some(a), Some((w, _))) = (op.first(), here) {
                            painter
                                .line_segment([to_s(*a), to_s(w)], egui::Stroke::new(1.6, accent));
                            painter.text(
                                to_s((*a + w) / 2.0) + egui::vec2(6.0, -6.0),
                                egui::Align2::LEFT_BOTTOM,
                                crate::units::len(a.distance(w)),
                                egui::FontId::proportional(11.0),
                                accent,
                            );
                        }
                        for p in &op {
                            painter.circle_filled(to_s(*p), 3.5, accent);
                        }
                        if let (true, Some((w, _))) = (response.clicked(), here) {
                            self.profiles.op.push(w);
                            if let [a, b] = self.profiles.op[..] {
                                if a.distance(b) > 1e-9 {
                                    self.profiles.calib =
                                        Some((a, b, format!("{:.0}", a.distance(b) * 1000.0)));
                                }
                                self.profiles.op.clear();
                            }
                        }
                    }
                    (Some(id), Some((a, b, mut text))) => {
                        painter.line_segment([to_s(a), to_s(b)], egui::Stroke::new(1.6, accent));
                        let mut done = false;
                        egui::Area::new(egui::Id::new("prof.calib"))
                            .order(egui::Order::Foreground)
                            .fixed_pos(to_s((a + b) / 2.0) + egui::vec2(10.0, -12.0))
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Really");
                                        let r = ui.add(
                                            egui::TextEdit::singleline(&mut text)
                                                .vertical_align(egui::Align::Center)
                                                .desired_width(70.0),
                                        );
                                        r.request_focus();
                                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                            done = true;
                                        }
                                    });
                                });
                            });
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.profiles.calib = None;
                        } else if done {
                            self.profiles.calib = None;
                            match crate::units::get().parse(&text) {
                                Some(d) => trace_calib = Some((id, a, b, d)),
                                None => self.status = format!("\"{text}\" isn't a length."),
                            }
                        } else {
                            self.profiles.calib = Some((a, b, text));
                        }
                    }
                }
            }
            ProfTool::Align => {
                let tap = response.clicked();
                if let Some((a, b)) = self.profiles.align_ref {
                    let d = (b - a).normalize_or_zero() * 1e4;
                    painter
                        .line_segment([to_s(a - d), to_s(a + d)], egui::Stroke::new(1.6, accent));
                }
                if let (true, Some(q)) = (tap, pointer) {
                    match (self.profiles.align_ref, pick_line(q)) {
                        (None, Some((a, b, _))) => self.profiles.align_ref = Some((a, b)),
                        (Some(r), Some((a, b, Some(what)))) => {
                            // the picked edge alone when it is the edge picked
                            // before; else its whole shape
                            let whole = match what {
                                PSel::Edge(si, e)
                                    if self.profiles.sel != Some(PSel::Edge(si, e)) =>
                                {
                                    PSel::Shape(si)
                                }
                                w => w,
                            };
                            let f = align_fn((a, b), r);
                            match whole {
                                PSel::Guide(i) => {
                                    if let Some(g) = self
                                        .profiles
                                        .guides
                                        .get_mut(&key)
                                        .and_then(|g| g.get_mut(i))
                                    {
                                        *g = (f(g.0), f(g.1));
                                    }
                                }
                                sel if editable => {
                                    write = Some((
                                        edit(&mut |x| apply_sel(x, sel, &f)),
                                        "Align in profile",
                                        false,
                                    ));
                                }
                                _ => {}
                            }
                            self.profiles.align_ref = None;
                        }
                        _ => {}
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.profiles.align_ref = None;
                }
            }
            _ => {
                if response.clicked() && !editable {
                    self.status = format!(
                        "{} is a preset: Duplicate it (in the Profile list) to change it.",
                        prof.name
                    );
                }
            }
        }
        // an edge's length typed: its far corner moves along the edge
        if let Some((si, e, now)) = clicked_len {
            self.profiles.typing = Some((si, e, format!("{:.0}", now * 1000.0)));
        } else if self.profiles.typing.is_some()
            && (response.clicked() || response.secondary_clicked() || response.drag_started())
        {
            // a click anywhere else lets it go (MU37: it stayed over the sketch)
            self.profiles.typing = None;
        }
        if let Some((si, e, mut text)) = self.profiles.typing.clone() {
            let s = &shapes[si.min(shapes.len() - 1)];
            if let (Some(a), Some(b)) = (s.points.get(e), s.points.get((e + 1) % s.points.len())) {
                let mid = to_s((*a + *b) / 2.0);
                let mut done = false;
                // out from the edge, off the sketch; drag it by its frame
                let (sa, sb) = (to_s(*a), to_s(*b));
                let away = egui::vec2(-(sb - sa).y, (sb - sa).x).normalized() * 46.0;
                egui::Area::new(egui::Id::new(("prof.len", si, e)))
                    .order(egui::Order::Foreground)
                    .movable(true)
                    .default_pos(mid + away - egui::vec2(60.0, 14.0))
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Length");
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut text)
                                        .vertical_align(egui::Align::Center)
                                        .desired_width(70.0),
                                );
                                r.request_focus();
                                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    done = true;
                                }
                            });
                        });
                    });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.profiles.typing = None;
                } else if done {
                    self.profiles.typing = None;
                    if let Some(len) = crate::units::get().parse(&text) {
                        let (a, b) = (*a, *b);
                        let dir = (b - a).normalize_or_zero();
                        let to = a + dir * len;
                        let n = s.points.len();
                        write = Some((
                            edit(&mut |x| {
                                if let Some(v) =
                                    point_mut(x, si).and_then(|v| v.get_mut((e + 1) % n))
                                {
                                    *v = to;
                                }
                            }),
                            "Profile edge length",
                            false,
                        ));
                    }
                } else {
                    self.profiles.typing = Some((si, e, text));
                }
            }
        }
        // keys over the view: M moves, R rotates, Delete removes a picked
        // guide or corner
        match menu_act {
            Some("dims") => self.profiles.hide_dims = !self.profiles.hide_dims,
            Some("align") => {
                self.profiles.tool = ProfTool::Align;
                self.profiles.align_ref = None;
            }
            Some("void") => self.profiles.tool = ProfTool::Void,
            Some("copy") => {
                self.profiles.tool = ProfTool::Move;
                self.profiles.op.clear();
                self.profiles.copy = true;
                self.status = "Duplicate: click a base point, then where the copy goes.".into();
            }
            _ => {}
        }
        let keys_here = response.hovered() && !ui.ctx().egui_wants_keyboard_input();
        if keys_here || menu_act.is_some() {
            if keys_here && ui.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::D)) {
                self.profiles.hide_dims = !self.profiles.hide_dims;
            }
            // A picks every shape and guide
            if menu_act == Some("all")
                || (keys_here && ui.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::A)))
            {
                let mut all: Vec<PSel> = (0..shapes.len()).map(PSel::Shape).collect();
                all.extend((0..guides.len()).map(PSel::Guide));
                self.profiles.sel = all.first().copied();
                self.profiles.extra = all.into_iter().skip(1).collect();
            }
            let (m, r, del, frame) = ui.input(|i| {
                (
                    keys_here && !i.modifiers.any() && i.key_pressed(egui::Key::M),
                    keys_here && !i.modifiers.any() && i.key_pressed(egui::Key::R),
                    keys_here && i.key_pressed(egui::Key::Delete),
                    keys_here
                        && !i.modifiers.any()
                        && (i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::Period)),
                )
            });
            let (m, r, del, frame) = (
                m || menu_act == Some("move"),
                r || menu_act == Some("rotate"),
                del || menu_act == Some("delete"),
                frame || menu_act == Some("frame"),
            );
            // F (or the keypad's .) frames what is picked, as Blender's
            if frame && !picked.is_empty() {
                let pts: Vec<DVec2> = picked
                    .iter()
                    .flat_map(|p| sel_lines(&shapes, &guides, &traces, *p))
                    .flatten()
                    .collect();
                let lo = pts.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
                let hi = pts.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
                let span = (hi - lo).max(DVec2::splat(0.005));
                let k = ((body.width() - 160.0).max(60.0) as f64 / span.x)
                    .min((body.height() - 120.0).max(60.0) as f64 / span.y)
                    as f32;
                self.profiles.camera = Some(((lo + hi) / 2.0, k.clamp(1.0, 1e6)));
            }
            if m || r {
                self.profiles.tool = if m { ProfTool::Move } else { ProfTool::Rotate };
                self.profiles.op.clear();
                self.profiles.copy = false;
            }
            // Shift+D: a copy to move, as elsewhere
            if ui
                .input(|i| i.modifiers.shift && !i.modifiers.command && i.key_pressed(egui::Key::D))
            {
                self.profiles.tool = ProfTool::Move;
                self.profiles.op.clear();
                self.profiles.copy = true;
                self.status = "Duplicate: click a base point, then where the copy goes.".into();
            }
            // a picked edge opens its shape there (or splits an open one); a
            // picked shape goes (MU37: Delete did nothing to an edge)
            let cut: Vec<PSel> = picked
                .iter()
                .copied()
                .filter(|p| matches!(p, PSel::Edge(..) | PSel::Shape(..)))
                .collect();
            if del && editable && !cut.is_empty() {
                self.profiles.sel = None;
                self.profiles.extra.clear();
                let n = cut.len();
                write = Some((
                    edit(&mut |x| delete_parts(x, &cut)),
                    if n == 1 {
                        "Delete profile edge"
                    } else {
                        "Delete profile parts"
                    },
                    false,
                ));
            } else if del && !picked.is_empty() {
                self.profiles.sel = None;
                self.profiles.extra.clear();
                // guides from the last, so the others keep their places
                let mut gs: Vec<usize> = picked
                    .iter()
                    .filter_map(|p| {
                        if let PSel::Guide(i) = p {
                            Some(*i)
                        } else {
                            None
                        }
                    })
                    .collect();
                gs.sort_unstable_by(|a, b| b.cmp(a));
                if let Some(g) = self.profiles.guides.get_mut(&key) {
                    for i in gs {
                        if i < g.len() {
                            g.remove(i);
                        }
                    }
                }
                let mut cs: Vec<(usize, usize)> = picked
                    .iter()
                    .filter_map(|p| {
                        if let PSel::Corner(si, ci) = p {
                            Some((*si, *ci))
                        } else {
                            None
                        }
                    })
                    .collect();
                cs.sort_unstable_by(|a, b| b.cmp(a));
                if editable && !cs.is_empty() {
                    write = Some((
                        edit(&mut |x| {
                            for &(si, ci) in &cs {
                                let min = if si == 0 {
                                    if x.open {
                                        2
                                    } else {
                                        3
                                    }
                                } else {
                                    2
                                };
                                if let Some(v) = point_mut(x, si) {
                                    if v.len() > min && ci < v.len() {
                                        v.remove(ci);
                                    }
                                }
                            }
                        }),
                        "Remove profile corners",
                        false,
                    ));
                }
            }
        }
        if let Some(look) = look {
            crate::planview::nav_draw(ui, &look);
        }
        for (id, at, turn) in trace_move {
            let r = self.doc.edit(id, "Move trace", |e| {
                if let ElementKind::Reference(r) = &mut e.kind {
                    r.at = at;
                    r.rotation = (r.rotation + turn).rem_euclid(360.0);
                }
            });
            if let Err(e) = r {
                self.status = format!("Couldn't do that: {e}");
            }
        }
        if let Some((id, a, b, d)) = trace_calib {
            let mut why = None;
            let r = self.doc.edit(id, "Calibrate trace", |e| {
                if let ElementKind::Reference(r) = &mut e.kind {
                    why = r.calibrate(a, b, d).err();
                }
            });
            self.status = match (r, why) {
                (Err(e), _) => format!("Couldn't do that: {e}"),
                (_, Some(w)) => w,
                _ => format!(
                    "Calibrated: those two points are {} apart now.",
                    crate::units::len(d)
                ),
            };
            self.profiles.tool = ProfTool::Select;
        }
        // a drag shows its preview and is committed once, when let go
        match write {
            Some((all, _, true)) => self.profiles.preview = Some(all),
            Some((all, label, false)) => {
                self.profiles.preview = None;
                self.put_profiles(all, label, false);
            }
            None => {}
        }
        if self.profiles.drag.is_none() && self.profiles.edge_drag.is_none() {
            if let Some(all) = self.profiles.preview.take() {
                self.put_profiles(all, "Shape profile", false);
            }
        }
    }

    /// A sweep's, column's or beam's Profile rows: its profile picked from
    /// the pictured list, opened in a view, updated, saved or duplicated.
    /// A sweep's, column's or beam's Profile section; false for anything
    /// else (which has no profile of its own).
    pub(super) fn sweep_profile_rows(&mut self, ui: &mut egui::Ui, id: ElementId) -> bool {
        let model = self.doc.model();
        let Some(e) = model.get(id) else { return false };
        let (shape, link): (Option<Vec<DVec2>>, Option<String>) = match &e.kind {
            ElementKind::Part(p) => match &p.form {
                creabim_core::parts::Form::Sweep { profile, .. } => {
                    (Some(profile.clone()), p.profile.clone())
                }
                _ => return false,
            },
            ElementKind::Column(c) => (
                c.section.as_ref().map(|s| s.points.clone()),
                c.section.as_ref().map(|s| s.name.clone()),
            ),
            ElementKind::Beam(b) => (
                b.section.as_ref().map(|s| s.points.clone()),
                b.section.as_ref().map(|s| s.name.clone()),
            ),
            _ => return false,
        };
        let member = !matches!(e.kind, ElementKind::Part(_));
        let mut all: Vec<String> = profiles::project_profiles(model)
            .into_iter()
            .map(|p| p.name)
            .collect();
        all.extend(PRESETS.iter().map(|s| s.to_string()));
        let stale = link
            .as_deref()
            .and_then(|n| profiles::find(model, n))
            .is_some_and(|p| Some(p) != shape);
        let mut pick: Option<Option<String>> = None;
        if !self.prop_header(ui, "profile", "Profile", true, None) {
            return true;
        }
        crate::props::row(ui, "Profile", |ui| {
            let w = ui.available_width();
            let pal = crate::theme::Palette::of(ui.ctx());
            egui::ComboBox::from_id_salt(("member.profile", id))
                .icon(crate::props::combo_icon)
                .width(w)
                .truncate()
                .selected_text(link.clone().unwrap_or_else(|| {
                    if member {
                        "Its own (width × depth)".into()
                    } else {
                        "Its own".into()
                    }
                }))
                .show_ui(ui, |ui| {
                    if member
                        && ui
                            .selectable_label(link.is_none(), "Its own (width × depth)")
                            .clicked()
                    {
                        pick = Some(None);
                    }
                    for n in &all {
                        let pts = profiles::find(self.doc.model(), n).unwrap_or_default();
                        let r = crate::props::picture_option(
                            ui,
                            link.as_deref() == Some(n.as_str()),
                            n,
                            |p, rr, _| {
                                paint_thumb(p, rr, &pts, pal.text);
                            },
                        );
                        if r.clicked() {
                            pick = Some(Some(n.clone()));
                        }
                    }
                });
        });
        if stale {
            crate::props::note(
                ui,
                "Its profile was changed: Update gives it the new shape.",
            );
        }
        let has = link.is_some();
        let (open_it, upd) = crate::props::actions2(
            ui,
            "",
            ("Edit in a view", "Open its profile in a Profile view", has),
            ("Update", "Take its profile's current shape", stale),
        );
        let (save, dup) = crate::props::actions2(
            ui,
            "",
            (
                "Save as new",
                "Its shape as a new profile of the project",
                shape.is_some(),
            ),
            (
                "Duplicate",
                "A copy of its profile to change slightly, given to it",
                has,
            ),
        );
        let n = link.clone().unwrap_or_default();
        if open_it {
            self.open_profile(&n);
        }
        if upd {
            if let Some(p) = profiles::find(self.doc.model(), &n) {
                let r = self
                    .doc
                    .edit(id, "Update profile", |e| set_section(e, &n, &p));
                self.report(r, format!("Updated from {n}."));
            }
        }
        if save {
            if let Some(pts) = shape.clone() {
                let name = self
                    .doc
                    .model()
                    .get(id)
                    .map_or("Profile".into(), |e| e.name.clone());
                self.new_profile(
                    Some(NamedProfile {
                        points: pts,
                        ..Default::default()
                    }),
                    &name,
                );
            }
        }
        if dup {
            self.profiles.current = Some(n.clone());
            self.profiles_action("profiles.duplicate", "");
            if let Some(newname) = self.profiles.current.clone() {
                if let Some(p) = profiles::find(self.doc.model(), &newname) {
                    let r = self
                        .doc
                        .edit(id, "Duplicate profile", |e| set_section(e, &newname, &p));
                    self.report(
                        r,
                        format!(
                            "It takes {newname} now: change it in the Profile view, then Apply."
                        ),
                    );
                }
            }
        }
        if let Some(choice) = pick {
            let r = match choice {
                Some(n) => {
                    let p = profiles::find(self.doc.model(), &n).unwrap_or_default();
                    self.doc
                        .edit(id, "Change profile", |e| set_section(e, &n, &p))
                }
                None => self.doc.edit(id, "Own section", |e| match &mut e.kind {
                    ElementKind::Column(c) => c.section = None,
                    ElementKind::Beam(b) => b.section = None,
                    _ => {}
                }),
            };
            self.report(r, "Profile changed. Ctrl+Z puts it back.");
        }
        true
    }
}

/// Give element `e` library profile `name` shaped `p`.
fn set_section(e: &mut creabim_core::Element, name: &str, p: &[DVec2]) {
    match &mut e.kind {
        ElementKind::Part(q) => {
            if let creabim_core::parts::Form::Sweep { profile, .. } = &mut q.form {
                *profile = p.to_vec();
            }
            q.profile = Some(name.to_string());
        }
        ElementKind::Column(c) => {
            c.section = Some(creabim_core::members::Section {
                name: name.into(),
                points: p.to_vec(),
            })
        }
        ElementKind::Beam(b) => {
            b.section = Some(creabim_core::members::Section {
                name: name.into(),
                points: p.to_vec(),
            })
        }
        _ => {}
    }
}

/// A profile's outline drawn small in `r`.
fn paint_thumb(painter: &egui::Painter, r: Rect, pts: &[DVec2], c: egui::Color32) {
    if pts.len() < 2 {
        return;
    }
    let lo = pts.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
    let hi = pts.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
    let span = (hi - lo).max_element().max(1e-6);
    let k = (r.width().min(r.height()) as f64 - 2.0) / span;
    let mid = (lo + hi) / 2.0;
    let s: Vec<egui::Pos2> = pts
        .iter()
        .map(|p| r.center() + egui::vec2(((p.x - mid.x) * k) as f32, (-(p.y - mid.y) * k) as f32))
        .collect();
    painter.add(egui::Shape::closed_line(s, egui::Stroke::new(1.0, c)));
}

/// Shape `si`'s corners of a profile: 0 its first, then the rest.
/// The lines of what is picked, for drawing it and its ghost: one point
/// for a corner, two for an edge or a guide, the outline for a shape.
fn sel_lines(
    shapes: &[ProfileShape],
    guides: &[(DVec2, DVec2)],
    traces: &[(ElementId, creabim_core::Reference)],
    sel: PSel,
) -> Vec<Vec<DVec2>> {
    match sel {
        PSel::Trace(id) => traces
            .iter()
            .find(|t| t.0 == id)
            .and_then(|(_, r)| r.outline())
            .map(|o| vec![vec![o[0], o[1], o[2], o[3], o[0]]])
            .unwrap_or_default(),
        PSel::Corner(si, ci) => shapes
            .get(si)
            .and_then(|s| s.points.get(ci))
            .map(|p| vec![vec![*p]])
            .unwrap_or_default(),
        PSel::Edge(si, e) => shapes
            .get(si)
            .filter(|s| s.points.len() >= 2)
            .map(|s| {
                vec![vec![
                    s.points[e % s.points.len()],
                    s.points[(e + 1) % s.points.len()],
                ]]
            })
            .unwrap_or_default(),
        PSel::Shape(si) => shapes
            .get(si)
            .map(|s| {
                let mut v = s.points.clone();
                if !s.open {
                    v.extend(s.points.first().copied());
                }
                vec![v]
            })
            .unwrap_or_default(),
        PSel::Guide(i) => guides
            .get(i)
            .map(|(a, b)| vec![vec![*a, *b]])
            .unwrap_or_default(),
    }
}

/// Move the corners of what is picked by `f`.
fn apply_sel(x: &mut NamedProfile, sel: PSel, f: &dyn Fn(DVec2) -> DVec2) {
    let (si, which): (usize, Vec<usize>) = match sel {
        PSel::Corner(si, ci) => (si, vec![ci]),
        PSel::Edge(si, e) => {
            let n = point_mut(x, si).map_or(0, |v| v.len());
            if n < 2 {
                return;
            }
            (si, vec![e % n, (e + 1) % n])
        }
        PSel::Shape(si) => (si, (0..point_mut(x, si).map_or(0, |v| v.len())).collect()),
        PSel::Guide(_) | PSel::Trace(_) => return,
    };
    if let Some(v) = point_mut(x, si) {
        for i in which {
            if let Some(p) = v.get_mut(i) {
                *p = f(*p);
            }
        }
    }
}

/// Delete picked edges and shapes from a profile: a closed shape opens at
/// its edge, an open one loses its end or splits in two; a picked shape is
/// removed (the main shape gives its place to the next one).
fn delete_parts(x: &mut NamedProfile, picked: &[PSel]) {
    let mut shapes = x.shapes();
    let gone: std::collections::BTreeSet<usize> = picked
        .iter()
        .filter_map(|p| {
            if let PSel::Shape(si) = p {
                Some(*si)
            } else {
                None
            }
        })
        .collect();
    let mut edges: Vec<(usize, usize)> = picked
        .iter()
        .filter_map(|p| {
            if let PSel::Edge(si, e) = p {
                Some((*si, *e))
            } else {
                None
            }
        })
        .filter(|(si, _)| !gone.contains(si))
        .collect();
    edges.sort_unstable_by(|a, b| b.cmp(a));
    let mut extra: Vec<ProfileShape> = vec![];
    let mut done: Vec<(usize, usize)> = vec![]; // (shape, rotation) of closed shapes opened
    for (si, mut e) in edges {
        let Some(sh) = shapes.get_mut(si) else {
            continue;
        };
        let n = sh.points.len();
        if n < 2 {
            continue;
        }
        if let Some(&(_, rot)) = done.iter().find(|d| d.0 == si) {
            // the shape was opened already: edge numbers moved round
            e = (e + n - rot) % n;
        }
        if !sh.open {
            // open it at this edge: the path runs from its far corner round to
            // its near one
            let k = (e + 1) % n;
            sh.points.rotate_left(k);
            sh.open = true;
            done.push((si, k));
        } else if e == 0 {
            sh.points.remove(0);
        } else if e + 2 >= n {
            sh.points.pop();
        } else {
            let tail = sh.points.split_off(e + 1);
            extra.push(ProfileShape {
                points: tail,
                open: true,
                thickness: sh.thickness,
            });
        }
    }
    let mut kept: Vec<ProfileShape> = shapes
        .into_iter()
        .enumerate()
        .filter(|(i, s)| !gone.contains(i) && s.points.len() >= 2)
        .map(|(_, s)| s)
        .collect();
    kept.extend(extra.into_iter().filter(|s| s.points.len() >= 2));
    if kept.is_empty() {
        return; // a profile keeps at least one shape
    }
    let first = kept.remove(0);
    x.points = first.points;
    x.open = first.open;
    x.thickness = first.thickness;
    x.more = kept;
    x.flips.clear();
}

/// Move every corner of what is picked by `f`, each once however many
/// picks share it (an edge and its corner, a shape and its edges).
fn apply_many(x: &mut NamedProfile, picked: &[PSel], f: &dyn Fn(DVec2) -> DVec2) {
    let mut corners: std::collections::BTreeSet<(usize, usize)> = Default::default();
    for p in picked {
        match *p {
            PSel::Corner(si, ci) => {
                corners.insert((si, ci));
            }
            PSel::Edge(si, e) => {
                let n = point_mut(x, si).map_or(0, |v| v.len());
                if n >= 2 {
                    corners.insert((si, e % n));
                    corners.insert((si, (e + 1) % n));
                }
            }
            PSel::Shape(si) => {
                let n = point_mut(x, si).map_or(0, |v| v.len());
                corners.extend((0..n).map(|c| (si, c)));
            }
            PSel::Guide(_) | PSel::Trace(_) => {}
        }
    }
    for (si, ci) in corners {
        if let Some(p) = point_mut(x, si).and_then(|v| v.get_mut(ci)) {
            *p = f(*p);
        }
    }
}

/// What lays the line `ab` on the line `r`: turned about its middle by the
/// least angle that makes it parallel, then moved square onto it.
fn align_fn(ab: (DVec2, DVec2), r: (DVec2, DVec2)) -> impl Fn(DVec2) -> DVec2 {
    let (a, b) = ab;
    let u = (b - a).normalize_or_zero();
    let v = (r.1 - r.0).normalize_or_zero();
    let mut ang = v.to_angle() - u.to_angle();
    let pi = std::f64::consts::PI;
    while ang > pi / 2.0 {
        ang -= pi;
    }
    while ang <= -pi / 2.0 {
        ang += pi;
    }
    let m = (a + b) / 2.0;
    let turn = DVec2::from_angle(ang);
    let n = v.perp();
    let shift = n * (r.0 - m).dot(n);
    move |p| m + turn.rotate(p - m) + shift
}

fn point_mut(p: &mut NamedProfile, si: usize) -> Option<&mut Vec<DVec2>> {
    if si == 0 {
        Some(&mut p.points)
    } else {
        p.more.get_mut(si - 1).map(|m| &mut m.points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arc_through_three_points_keeps_to_its_circle_and_ends_on_its_end() {
        let (a, t, e) = (
            DVec2::new(-1.0, 0.0),
            DVec2::new(0.0, 1.0),
            DVec2::new(1.0, 0.0),
        );
        let pts = arc_points(a, t, e, 24);
        assert_eq!(*pts.last().unwrap(), e);
        assert!(pts.len() >= 8);
        assert!(pts
            .iter()
            .all(|p| (p.length() - 1.0).abs() < 1e-9 && p.y >= -1e-9));
        // three in a line: the straight side
        assert_eq!(arc_points(a, DVec2::ZERO, e, 24), vec![e]);
    }
}
