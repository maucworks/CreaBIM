//! Masters (MU40): the sheet templates, drawn in a view of their own as
//! profiles are. The header picks a master and makes, duplicates, brings
//! in, writes out and deletes them; its tools draw lines, rectangles,
//! text, the sheet's fields as chips and pictures (logos), snapping to a
//! millimetre grid, the paper's edges and margins and what is drawn
//! already. Shapes take a pen colour, a fill and an opacity. Properties
//! sets the master's paper, border and own fields, and what is picked.
//!
//! A master is the frame; a sheet uses one. Change a master and every
//! sheet on it changes.

use super::*;
use crate::ui_model::Item;
use creabim_core::titleblock::{self as tb, Corner, SheetTemplate, TextAlign, TitleItem};
use glam::DVec2;

/// What the Masters view's pointer does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MTool {
    /// Pick, drag, resize by the corners and nudge what is drawn.
    #[default]
    Select,
    /// Click corners; Enter, a right-click or the first corner again ends.
    Line,
    /// Two corners.
    Rect,
    /// Click where text goes.
    Text,
    /// Click where a field goes: the sheet fills it in.
    Field,
    /// Two clicks: a guide through them, to snap to (MU40, as profiles'
    /// guides); nearly level or upright, it goes straight.
    Guide,
}

/// A drag of the picked item: moving it, or resizing it by a corner.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Grabbed at (mm), gone so far.
    Move(DVec2, DVec2),
    /// The fixed corner, and where the dragged corner is now (mm, paper).
    Size(DVec2, DVec2, DVec2),
}

/// The Masters view between frames.
#[derive(Clone, Debug, Default)]
pub struct MastersUi {
    /// The master shown.
    pub current: Option<ElementId>,
    pub tool: MTool,
    /// Centre (paper mm) and points per mm.
    camera: Option<(DVec2, f32)>,
    /// The item picked.
    pub sel: Option<usize>,
    drag: Option<Drag>,
    /// Move (M): the picked item follows the pointer from here (paper mm)
    /// until a click puts it down.
    follow: Option<DVec2>,
    /// Corners of a line or rectangle being drawn (paper mm).
    drawing: Vec<DVec2>,
    /// The field the Field tool places.
    pub field: String,
    /// The pass the pointer was last over the view.
    hovered_pass: u64,
    /// The view's size last frame: a new size fits the sheet again.
    seen: egui::Vec2,
    /// The Guide tool's first click (paper mm).
    guide_from: Option<DVec2>,
    /// The guide picked, and one being dragged: (it, grabbed at, gone).
    guide_sel: Option<usize>,
    guide_drag: Option<(usize, DVec2, DVec2)>,
    /// Guides hidden (and not snapped to) while looking at the result.
    pub hide_guides: bool,
    /// A field of your own being named, from the Field menu.
    asking_field: Option<String>,
    /// What is typed in Properties, kept while a field has the focus and
    /// saved when it loses it: a picked text's words (master, item) and
    /// the values of a master's own fields (master, field).
    text_drafts: std::collections::HashMap<(ElementId, usize), String>,
    field_drafts: std::collections::HashMap<(ElementId, String), String>,
    /// A master's name as typed in Properties, kept while its field has
    /// the focus and saved when it loses it.
    names: std::collections::HashMap<ElementId, String>,
    /// The border picked (its margins in Properties), and an edge being
    /// dragged: (0 left, 1 bottom, 2 right, 3 top; where it is now, mm).
    pub border_sel: bool,
    border_drag: Option<(usize, f64)>,
    /// What the Shape tool draws: 0 a rectangle (two corners), 1 a circle
    /// (centre, then a point on it), 2 an ellipse (two corners of its box).
    pub shape_kind: u8,
}

const SHAPE_KINDS: [(&str, &str, &str); 3] = [
    ("Rectangle", "square", "Two corners"),
    ("Circle", "circle", "Its centre, then a point on it"),
    ("Ellipse", "ellipse", "Two corners of the box round it"),
];

/// The outline the Shape tool makes from its two clicks, paper mm.
fn shape_points(kind: u8, a: DVec2, b: DVec2) -> Vec<DVec2> {
    let ring = |c: DVec2, r: DVec2| -> Vec<DVec2> {
        (0..72)
            .map(|i| {
                let t = i as f64 / 72.0 * std::f64::consts::TAU;
                c + DVec2::new(t.cos() * r.x, t.sin() * r.y)
            })
            .collect()
    };
    match kind {
        1 => ring(a, DVec2::splat(a.distance(b))),
        2 => ring((a + b) / 2.0, (b - a).abs() / 2.0),
        _ => {
            let (lo, hi) = (a.min(b), a.max(b));
            vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)]
        }
    }
}

/// The guide's colour: a cyan apart from ink, fields and the selection.
const GUIDE: egui::Color32 = egui::Color32::from_rgb(0, 168, 196);

/// Where lines `a`→`b` and `c`→`d` cross, if they do.
fn cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<DVec2> {
    let (r, s) = (b - a, d - c);
    let den = r.perp_dot(s);
    (den.abs() > 1e-9).then(|| a + r * ((c - a).perp_dot(s) / den))
}

/// `p` brought onto the line through `a` and `b`, and how far it moved.
fn onto(p: DVec2, a: DVec2, b: DVec2) -> (DVec2, f64) {
    let d = (b - a).normalize_or_zero();
    let q = a + d * (p - a).dot(d);
    (q, q.distance(p))
}

/// A guide's second point, straightened when it is nearly level or
/// upright.
fn straighten(a: DVec2, mut b: DVec2) -> DVec2 {
    let d = b - a;
    if d.y.abs() < d.x.abs() * 0.035 {
        b.y = a.y;
    } else if d.x.abs() < d.y.abs() * 0.035 {
        b.x = a.x;
    }
    b
}

/// Sheet sizes, mm, landscape.
const PAPERS: [(&str, f64, f64); 5] = [
    ("A4", 297.0, 210.0),
    ("A3", 420.0, 297.0),
    ("A2", 594.0, 420.0),
    ("A1", 841.0, 594.0),
    ("A0", 1189.0, 841.0),
];

/// A text's box, roughly, mm from the corner: for picking.
fn text_box(at: DVec2, size: f64, text: &str, align: TextAlign) -> (DVec2, DVec2) {
    let w = size * 0.55 * text.chars().count().max(1) as f64;
    let x0 = match align {
        TextAlign::Start => at.x,
        TextAlign::Middle => at.x - w / 2.0,
        TextAlign::End => at.x - w,
    };
    (
        DVec2::new(x0, at.y - size * 0.6),
        DVec2::new(x0 + w, at.y + size * 0.6),
    )
}

/// An item's box, mm from the corner.
fn item_box(it: &TitleItem) -> (DVec2, DVec2) {
    let pts = match it {
        TitleItem::Text {
            at,
            size,
            text,
            align,
            angle,
        } => {
            // its corners turned with it, about its point
            let (lo, hi) = text_box(*at, *size, text, *align);
            let turn = DVec2::from_angle(*angle);
            [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)]
                .map(|c| *at + turn.rotate(c - *at))
                .to_vec()
        }
        _ => it.points(),
    };
    let lo = pts.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
    let hi = pts.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
    (lo, hi)
}

fn seg_dist(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-12 {
        0.0
    } else {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    };
    p.distance(a + ab * t)
}

/// How far `p` is from an item, mm (0 inside a text, a picture or a fill).
fn item_dist(it: &TitleItem, p: DVec2) -> f64 {
    let edges = |points: &[DVec2], closed: bool| {
        let mut d = points
            .windows(2)
            .map(|w| seg_dist(p, w[0], w[1]))
            .fold(f64::MAX, f64::min);
        if closed && points.len() > 2 {
            d = d.min(seg_dist(p, points[points.len() - 1], points[0]));
        }
        d
    };
    match it {
        TitleItem::Line { points, .. } => edges(points, false),
        TitleItem::Fill { points, .. } => {
            if creabim_core::polygon::contains(points, p) {
                0.0
            } else {
                edges(points, true)
            }
        }
        TitleItem::Shape {
            points,
            closed,
            fill,
            ..
        } => {
            if fill.is_some() && points.len() > 2 && creabim_core::polygon::contains(points, p) {
                0.0
            } else {
                edges(points, *closed)
            }
        }
        TitleItem::Text {
            at,
            size,
            text,
            align,
            angle,
        } => {
            // measured along the text: the point turned back by its angle
            let q = *at + DVec2::from_angle(-*angle).rotate(p - *at);
            let (lo, hi) = text_box(*at, *size, text, *align);
            q.distance(q.clamp(lo, hi))
        }
        TitleItem::Image { .. } | TitleItem::Logo { .. } => {
            let (lo, hi) = item_box(it);
            p.distance(p.clamp(lo, hi))
        }
    }
}

/// Scale an item about `anchor` by `k` (each way): a resize by a corner.
/// Text grows with the height.
fn scale_item(it: &mut TitleItem, anchor: DVec2, k: DVec2) {
    let f = |p: &mut DVec2| *p = anchor + (*p - anchor) * k;
    match it {
        TitleItem::Line { points, .. }
        | TitleItem::Fill { points, .. }
        | TitleItem::Shape { points, .. } => points.iter_mut().for_each(f),
        TitleItem::Text { at, size, .. } => {
            f(at);
            *size = (*size * k.y.abs()).clamp(0.5, 200.0);
        }
        TitleItem::Image { lo, hi, .. } | TitleItem::Logo { lo, hi, .. } => {
            f(lo);
            f(hi);
            let (a, b) = (lo.min(*hi), lo.max(*hi));
            (*lo, *hi) = (a, b);
        }
    }
}

/// Turn an item a quarter about the middle of its box.
fn quarter_turn(it: &mut TitleItem) {
    let (lo, hi) = item_box(it);
    let c = (lo + hi) / 2.0;
    let r = |p: &mut DVec2| {
        let d = *p - c;
        *p = c + DVec2::new(-d.y, d.x);
    };
    match it {
        TitleItem::Line { points, .. }
        | TitleItem::Fill { points, .. }
        | TitleItem::Shape { points, .. } => points.iter_mut().for_each(r),
        TitleItem::Text { angle, .. } => {
            *angle = (*angle + std::f64::consts::FRAC_PI_2).rem_euclid(std::f64::consts::TAU)
        }
        // a picture keeps its pixels upright; its box turns
        TitleItem::Image { lo, hi, .. } => {
            let (mut a, mut b) = (*lo, *hi);
            r(&mut a);
            r(&mut b);
            (*lo, *hi) = (a.min(b), a.max(b));
        }
        // a logo turns whole: its box, and its shapes inside it
        TitleItem::Logo { lo, hi, shapes } => {
            let (mut a, mut b) = (*lo, *hi);
            r(&mut a);
            r(&mut b);
            (*lo, *hi) = (a.min(b), a.max(b));
            for s in shapes.iter_mut() {
                if let TitleItem::Shape { points, .. } = s {
                    for p in points.iter_mut() {
                        *p = DVec2::new(1.0 - p.y, p.x);
                    }
                }
            }
        }
    }
}

thread_local! {
    /// The pictures' textures, by their raster's key.
    static TEXTURES: std::cell::RefCell<std::collections::HashMap<u64, egui::TextureHandle>> =
        Default::default();
}

fn texture(ctx: &egui::Context, r: &creabim_geom::raster::Raster) -> egui::TextureId {
    TEXTURES.with(|t| {
        t.borrow_mut()
            .entry(r.key)
            .or_insert_with(|| {
                ctx.load_texture(
                    format!("master-picture-{}", r.key),
                    egui::ColorImage::from_rgba_unmultiplied([r.width, r.height], &r.rgba),
                    egui::TextureOptions::LINEAR,
                )
            })
            .id()
    })
}

fn rgb(c: [f32; 3], a: f32) -> egui::Color32 {
    let b = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    egui::Color32::from_rgba_unmultiplied(b[0], b[1], b[2], (a.clamp(0.0, 1.0) * 255.0) as u8)
}

impl CreaBimApp {
    /// The masters of the project, by name.
    fn masters(&self) -> Vec<(ElementId, String)> {
        super::drafting::templates(self.doc.model())
    }

    /// The master shown: the one picked, else the first.
    fn master(&self) -> Option<(ElementId, String, SheetTemplate)> {
        let model = self.doc.model();
        let get = |id: ElementId| match model.get(id).map(|e| (&e.kind, &e.name)) {
            Some((ElementKind::SheetTemplate(t), n)) => Some((id, n.clone(), t.clone())),
            _ => None,
        };
        self.masters
            .current
            .and_then(get)
            .or_else(|| self.masters().first().and_then(|(id, _)| get(*id)))
    }

    /// Change the master shown, as one undo step.
    fn edit_master(&mut self, label: &str, f: impl FnOnce(&mut SheetTemplate)) -> bool {
        let Some((id, _, mut t)) = self.master() else {
            return false;
        };
        let broken_before = t.items.iter().filter(|it| !it.is_valid()).count();
        f(&mut t);
        if let Err(e) = t.validate() {
            // an item broken before this edit no longer blocks every
            // change (MU40: the tools seemed to stop drawing); it is
            // taken out, and said so
            let bad: Vec<String> = t
                .items
                .iter()
                .filter(|it| !it.is_valid())
                .map(|it| format!("{it:?}").chars().take(160).collect())
                .collect();
            crate::actlog::line(format!("master edit refused: {e}: {bad:?}"));
            if broken_before > 0 {
                t.items.retain(|it| it.is_valid());
                self.status = format!(
                    "{broken_before} broken item(s) on the master taken out so it can be changed."
                );
            } else {
                self.status = format!("Not changed: {e}.");
                return false;
            }
        }
        let r = self
            .doc
            .edit_merged(id, label, move |e| e.kind = ElementKind::SheetTemplate(t));
        if let Err(e) = r {
            self.status = format!("Couldn't change the master: {e}");
            return false;
        }
        true
    }

    /// Show master `id` in a Masters view: one there, else the focused
    /// area turns into one.
    pub(super) fn open_master(&mut self, id: Option<ElementId>) {
        if id.is_some() {
            self.masters.current = id;
        }
        self.masters.sel = None;
        self.masters.camera = None;
        self.masters.drawing.clear();
        self.prop_tab = PropTab::Master;
        // a Masters view already up comes to the front, so Properties
        // shows the master (not what was picked before)
        let shows = self.areas.root.areas();
        if let Some(i) = shows
            .iter()
            .position(|s| *s == crate::areas::Shows::Masters)
        {
            self.areas.focus = i;
        } else {
            let i = self.areas.focus;
            if let Some(s) = self.areas.root.get_mut(i) {
                *s = crate::areas::Shows::Masters;
            }
            self.store_areas();
        }
    }

    /// Is a Masters view the area in front?
    pub(super) fn masters_in_front(&self) -> bool {
        matches!(
            self.areas.root.areas().get(self.areas.focus),
            Some(crate::areas::Shows::Masters)
        )
    }

    /// Is the pointer over a Masters view (its keys win there)?
    pub(super) fn masters_view_hovered(&self, ctx: &egui::Context) -> bool {
        self.masters.hovered_pass + 1 >= ctx.cumulative_pass_nr()
            && self
                .areas
                .root
                .areas()
                .contains(&crate::areas::Shows::Masters)
    }

    /// The master's name for the header.
    pub(super) fn master_title(&self) -> String {
        self.master()
            .map_or_else(|| "Pick a master".to_string(), |(_, n, _)| n)
    }

    /// The toolbar's Move, Rotate, Duplicate and Delete with a Masters
    /// view in front: they act on what is picked on the master, not on the
    /// model's selection (MU40: "New Sheet can't be moved this way").
    /// False for anything else.
    pub(super) fn masters_command(&mut self, c: Command) -> bool {
        if !self.masters_in_front()
            || !matches!(
                c,
                Command::Move
                    | Command::Rotate
                    | Command::Duplicate
                    | Command::Delete
                    | Command::Copy
                    | Command::Cut
            )
        {
            return false;
        }
        let Some(i) = self.masters.sel else {
            self.status = "Pick something on the master first.".into();
            return true;
        };
        match c {
            Command::Move => {
                self.masters.follow = None;
                self.masters.tool = MTool::Select;
                // it follows the pointer from where the pointer next is
                self.masters.follow = Some(DVec2::NAN);
                self.status =
                    "Move: it follows the pointer; click to put it down, Esc to leave it.".into();
            }
            Command::Rotate => {
                self.edit_master("Turn", |t| {
                    if let Some(it) = t.items.get_mut(i) {
                        quarter_turn(it);
                    }
                });
                self.status = "Turned a quarter. R again turns it further.".into();
            }
            Command::Duplicate | Command::Copy => {
                let n = self.master().map_or(0, |(_, _, t)| t.items.len());
                self.edit_master("Duplicate on master", |t| {
                    if let Some(it) = t.items.get(i).cloned() {
                        let mut it = it;
                        it.shift(DVec2::new(5.0, -5.0));
                        t.items.push(it);
                    }
                });
                self.masters.sel = Some(n);
                self.status = "A copy, 5 mm along: drag it where it goes.".into();
            }
            _ => {
                self.edit_master("Delete from master", |t| {
                    if i < t.items.len() {
                        t.items.remove(i);
                    }
                });
                self.masters.sel = None;
                self.status = "Deleted. Ctrl+Z brings it back.".into();
            }
        }
        true
    }

    /// The buttons beside the name: new (or bring in), duplicate, write
    /// out, delete; few, so the tools fit beside them (MU40). A sheet on it
    /// is in Properties.
    pub(super) fn master_buttons(&self) -> Vec<(&'static str, &'static str, String, bool)> {
        let cur = self.master();
        let name = cur.as_ref().map(|(_, n, _)| n.clone());
        let has = cur.is_some();
        vec![
            (
                "master.newmenu",
                "plus",
                "New master: from Standard, Minimal or blank".into(),
                true,
            ),
            (
                "master.duplicate",
                "copy",
                match &name {
                    Some(n) => format!("Duplicate {n}: a copy to change"),
                    None => "Duplicate: pick a master first".into(),
                },
                has,
            ),
            (
                "master.export",
                "upload",
                "Write it out as SVG, to finish in Inkscape or any drawing program".into(),
                has,
            ),
            (
                "master.delete",
                "trash-2",
                match &name {
                    Some(n) => format!("Delete {n}: its sheets go back to the built-in block"),
                    None => "Delete: pick a master first".into(),
                },
                has,
            ),
        ]
    }

    /// The tools, after the buttons.
    pub(super) fn master_controls(&self) -> Vec<serde_json::Value> {
        use serde_json::json;
        let t = self.masters.tool;
        let field = if self.masters.field.is_empty() {
            "project"
        } else {
            self.masters.field.as_str()
        };
        vec![
            json!({"kind": "toggle", "id": "master.tool:select", "icon": "mouse-pointer-2", "label": "Select", "on": t == MTool::Select,
                   "hint": "Pick, drag, and resize by its corners; M moves, R turns, Ctrl+D copies, Delete removes; arrows nudge 1 mm (Shift 5 mm)"}),
            json!({"kind": "toggle", "id": "master.tool:line", "icon": "slash", "label": "Line", "on": t == MTool::Line,
                   "hint": "Lines: click corners. Enter or a right-click ends it there; the first corner again closes it; Esc drops it"}),
            json!({"kind": "menu", "key": "master.shape", "icon": SHAPE_KINDS[self.masters.shape_kind as usize % 3].1, "label": "Shape",
                   "value": SHAPE_KINDS[self.masters.shape_kind as usize % 3].0, "on": t == MTool::Rect,
                   "hint": "A rectangle: two corners. Its pen, fill and opacity are in Properties"}),
            json!({"kind": "toggle", "id": "master.tool:text", "icon": "text-tool", "label": "Text", "on": t == MTool::Text,
                   "hint": "Text: click where it goes, then type it in Properties"}),
            json!({"kind": "menu", "key": "master.field", "icon": "field", "label": "Field", "value": format!("{{{field}}}"), "on": t == MTool::Field,
                   "hint": "A field: what each sheet fills in (project, sheet, number, scale, date, or one of the master's own). Pick one, then click where it goes"}),
            json!({"kind": "toggle", "id": "master.tool:guide", "icon": "guides", "label": "Guide", "on": t == MTool::Guide,
                   "hint": "A guide: two clicks, a line to draw by and snap to; nearly level or upright, it goes straight. Kept with the master, never printed"}),
            json!({"kind": "button", "id": "master.picture", "icon": "image", "label": "Picture",
                   "hint": "A logo or picture: an SVG comes in as a crisp vector logo, a PNG or JPEG as a picture. As many as you like; drag its corners to size it"}),
        ]
    }

    /// The Masters view's header menus.
    pub(super) fn master_menu(&self, key: &str) -> Option<Vec<Item>> {
        Some(match key {
            "master.pick" => {
                let cur = self.master().map(|(id, _, _)| id);
                let mut v: Vec<Item> = self
                    .masters()
                    .into_iter()
                    .map(|(id, n)| {
                        Item::new(format!("master.open:{}", id.0), n)
                            .icon("sheet-template")
                            .checked(Some(id) == cur)
                    })
                    .collect();
                if !v.is_empty() {
                    v.push(Item::Separator);
                }
                v.extend(self.master_new_items());
                v
            }
            "master.newmenu" => self.master_new_items(),
            "master.shape" => SHAPE_KINDS
                .iter()
                .enumerate()
                .map(|(k, (n, icon, hint))| {
                    Item::new(format!("master.shapekind:{k}"), *n)
                        .icon(icon)
                        .hint(*hint)
                        .checked(self.masters.shape_kind as usize == k)
                })
                .collect(),
            "master.field" => {
                let own: Vec<(String, String)> =
                    self.master().map(|(_, _, t)| t.fields).unwrap_or_default();
                let mut v: Vec<Item> = tb::BUILT_IN_FIELDS
                    .iter()
                    .map(|(f, what)| {
                        Item::new(format!("master.field:{f}"), format!("{{{f}}}")).hint(*what)
                    })
                    .collect();
                if !own.is_empty() {
                    v.push(Item::Separator);
                    for (f, _) in own {
                        v.push(
                            Item::new(format!("master.field:{f}"), format!("{{{f}}}"))
                                .hint("One of the master's own fields"),
                        );
                    }
                }
                // one not in the list: typed, added to the master's own
                // fields and placed (MU40)
                v.push(Item::Separator);
                v.push(
                    Item::new("master.newfield", "Your own field…")
                        .icon("plus")
                        .hint("Name a field of your own (client, drawn by, checked…): each sheet fills it in"),
                );
                v
            }
            _ => return None,
        })
    }

    fn master_new_items(&self) -> Vec<Item> {
        let mut v: Vec<Item> = tb::styles()
            .into_iter()
            .map(|(n, _)| {
                Item::new(format!("master.new:{n}"), format!("New from {n}")).icon("plus")
            })
            .collect();
        v.push(Item::new("master.new:", "New blank master").icon("plus"));
        v.push(Item::Separator);
        v.push(
            Item::new("master.import", "Bring in an SVG or DXF…")
                .icon("download")
                .hint("A frame drawn in Inkscape, AutoCAD or any drawing program"),
        );
        v
    }

    /// The browser's Masters group: the frames sheets are drawn on, apart
    /// from the sheets themselves.
    pub(super) fn masters_group(&self) -> serde_json::Value {
        let cur = self.master().map(|(id, _, _)| id);
        let model = self.doc.model();
        let rows: Vec<serde_json::Value> = self
            .masters()
            .into_iter()
            .map(|(id, n)| {
                let users = super::drafting::template_users(model, id).len();
                serde_json::json!({
                    "id": format!("master:{}", id.0), "name": n, "icon": "sheet-template",
                    "sel": Some(id) == cur && self.areas.root.areas().contains(&crate::areas::Shows::Masters),
                    "hint": format!("{n}: {users} sheet(s) on it. Click to open it in a Masters view"),
                })
            })
            .collect();
        serde_json::json!({
            "key": "masters", "title": "Masters", "icon": "sheet-template", "open": !rows.is_empty(),
            "rows": rows,
            "note": if rows.is_empty() { "The built-in title block".to_string() } else { String::new() },
            "acts": [
                {"id": "master.new:Standard A3", "icon": "plus", "hint": "A new master from the Standard strip, in a Masters view"},
                {"id": "master.view", "icon": "ellipsis", "hint": "Open the Masters view"},
            ],
        })
    }

    /// Masters actions from the header, menus and the browser.
    pub(super) fn masters_action(&mut self, head: &str, arg: &str) -> bool {
        match head {
            "master.view" => {
                let id = self.master().map(|(id, _, _)| id);
                self.open_master(id);
            }
            "master.open" => {
                let id = self
                    .doc
                    .model()
                    .iter()
                    .find(|e| e.id.0.to_string() == arg)
                    .map(|e| e.id);
                self.open_master(id);
            }
            "master.new" => {
                let (base, t) = match tb::styles().into_iter().find(|(n, _)| *n == arg) {
                    Some((n, t)) => (n.to_string(), t),
                    None => (
                        "Master".to_string(),
                        SheetTemplate {
                            corner: Corner::BottomRight,
                            items: vec![],
                            border: true,
                            fields: vec![],
                            paper: Some(DVec2::new(420.0, 297.0)),
                            guides: vec![],
                            margins: None,
                        },
                    ),
                };
                let name = self.master_name(&base);
                self.add_master(name, t);
            }
            "master.duplicate" => {
                if let Some((_, n, t)) = self.master() {
                    let name = self.master_name(&format!("{n} copy"));
                    self.add_master(name, t);
                }
            }
            "master.delete" => {
                if let Some((id, n, _)) = self.master() {
                    self.delete_template(id);
                    self.masters.current = None;
                    self.masters.sel = None;
                    self.status = format!(
                        "{n} deleted; its sheets have the built-in block. Ctrl+Z brings it back."
                    );
                }
            }
            "master.sheet" => {
                if let Some((id, n, _)) = self.master() {
                    self.new_sheet_from(id);
                    self.status = format!("A new sheet on {n}. It is open in the sheet views.");
                }
            }
            "master.import" => {
                let Some(p) = rfd::FileDialog::new()
                    .set_title("Bring in a frame or title block")
                    .add_filter("Drawings", &["svg", "dxf"])
                    .pick_file()
                else {
                    return true;
                };
                let path = p.to_string_lossy().to_string();
                match creabim_exchange::titleblock::read(&path) {
                    Ok(t) => {
                        let stem = p
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Master")
                            .to_string();
                        let name = self.master_name(&stem);
                        self.add_master(name, t);
                    }
                    Err(e) => self.status = format!("Couldn't bring it in: {e}"),
                }
            }
            "master.export" => {
                let Some((_, n, t)) = self.master() else {
                    return true;
                };
                let Some(p) = rfd::FileDialog::new()
                    .set_title("Write the master out as SVG")
                    .add_filter("SVG drawing", &["svg"])
                    .set_file_name(format!("{n}.svg"))
                    .save_file()
                else {
                    return true;
                };
                self.status = match std::fs::write(&p, creabim_exchange::titleblock::to_svg(&t)) {
                    Ok(()) => format!(
                        "{n} written to {}. Change it anywhere, then bring it back in.",
                        p.display()
                    ),
                    Err(e) => format!("Couldn't write {}: {e}", p.display()),
                };
            }
            "master.picture" => self.add_master_picture(),
            "master.tool" => {
                self.masters.tool = match arg {
                    "line" => MTool::Line,
                    "rect" => MTool::Rect,
                    "text" => MTool::Text,
                    "field" => MTool::Field,
                    "guide" => MTool::Guide,
                    _ => MTool::Select,
                };
                self.masters.drawing.clear();
                self.masters.follow = None;
                self.status = match self.masters.tool {
                    MTool::Select => "Select: pick, drag, or resize by a corner.",
                    MTool::Line => "Line: click corners; Enter or a right-click ends it.",
                    MTool::Rect => "Shape: click two points.",
                    MTool::Text => "Text: click where it goes.",
                    MTool::Field => "Field: click where it goes.",
                    MTool::Guide => "Guide: click two points it runs through.",
                }
                .into();
            }
            "master.shapekind" => {
                self.masters.shape_kind = arg.parse().unwrap_or(0);
                self.masters.tool = MTool::Rect;
                self.masters.drawing.clear();
                self.status = format!(
                    "{}: {}.",
                    SHAPE_KINDS[self.masters.shape_kind as usize % 3].0,
                    SHAPE_KINDS[self.masters.shape_kind as usize % 3]
                        .2
                        .to_lowercase()
                );
            }
            "master.newfield" => {
                self.masters.asking_field = Some(String::new());
            }
            "master.field" => {
                self.masters.field = arg.to_string();
                self.masters.tool = MTool::Field;
                self.status = format!("Click where {{{arg}}} goes: each sheet fills it in.");
            }
            _ => return false,
        }
        true
    }

    /// Pictures from files onto `sheet` (MU40: collages, site context,
    /// mood boards): as many as picked at once, side by side; an SVG as
    /// crisp vector shapes, a PNG or JPEG as a picture.
    pub(super) fn add_sheet_picture(&mut self, sheet: ElementId) {
        let Some(files) = rfd::FileDialog::new()
            .set_title("Pictures for the sheet: pick one or several")
            .add_filter(
                "Pictures",
                &["png", "jpg", "jpeg", "bmp", "gif", "webp", "svg"],
            )
            .pick_files()
        else {
            return;
        };
        let Some(s) = creabim_geom::sheet::sheet_of(self.doc.model(), sheet) else {
            return;
        };
        let (sw, sh) = (s.width, s.height);
        let n = files.len().max(1);
        let cols = (n as f64).sqrt().ceil() as usize;
        let rows = n.div_ceil(cols);
        let cell = ((sw - 60.0) / cols as f64).min(120.0);
        let mut tx = Transaction::new(if n == 1 {
            "Picture on sheet"
        } else {
            "Pictures on sheet"
        });
        let mut ids = vec![];
        let mut failed = vec![];
        for (k, p) in files.iter().enumerate() {
            let (c, r) = ((k % cols) as f64, (k / cols) as f64);
            let mid = DVec2::new(
                sw / 2.0 + (c - (cols as f64 - 1.0) / 2.0) * (cell + 6.0),
                sh / 2.0 - (r - (rows as f64 - 1.0) / 2.0) * (cell * 0.75 + 6.0),
            );
            match sheet_picture_kind(p, cell) {
                Ok(kind) => {
                    let name = p
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Picture")
                        .to_string();
                    let e = Element::new(
                        name,
                        ElementKind::PaperMark(creabim_core::PaperMark {
                            sheet,
                            at: mid,
                            size: 1.0,
                            kind,
                            color: None,
                        }),
                    );
                    ids.push(e.id);
                    tx = tx.insert(e);
                }
                Err(e) => failed.push(format!("{}: {e}", p.display())),
            }
        }
        if ids.is_empty() {
            self.status = format!("Couldn't read {}", failed.join("; "));
            return;
        }
        let r = self.doc.commit(tx);
        self.selection.extend(ids.iter().copied(), false);
        let mut msg = if ids.len() == 1 {
            "Picture placed".to_string()
        } else {
            format!("{} pictures placed", ids.len())
        };
        msg.push_str(
            ": drag each where it goes; its corner grips size it, its opacity is in Properties.",
        );
        if !failed.is_empty() {
            msg.push_str(&format!(" Couldn't read {}.", failed.join("; ")));
        }
        self.report(r, msg);
    }

    /// A picture from a file onto the master, 40 mm across in the middle
    /// of the view, picked to size and place (MU40: a logo, or several).
    fn add_master_picture(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .set_title("A picture for the master: a logo, a stamp, a background")
            .add_filter(
                "Logos and pictures",
                &["svg", "png", "jpg", "jpeg", "bmp", "gif", "webp"],
            )
            .pick_file()
        else {
            return;
        };
        // an SVG comes in as a vector logo: crisp at any size (MU40)
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
            let read = std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|t| creabim_exchange::titleblock::svg_logo(&t));
            let (shapes, aspect) = match read {
                Ok(x) => x,
                Err(e) => {
                    self.status = format!("Couldn't read {} as a logo: {e}", p.display());
                    return;
                }
            };
            let Some((_, _, t)) = self.master() else {
                self.status = "Make a master first: + in the header.".into();
                return;
            };
            let paper = t.paper.unwrap_or(DVec2::new(420.0, 297.0));
            let o = t.corner.at(paper.x, paper.y);
            let mid = self.masters.camera.map_or(paper / 2.0, |(c, _)| c) - o;
            let size = DVec2::new(40.0, 40.0 / aspect.max(1e-6));
            let n = t.items.len();
            let count = shapes.len();
            self.edit_master("Logo", |t| {
                t.items.push(TitleItem::Logo {
                    lo: mid - size / 2.0,
                    hi: mid + size / 2.0,
                    shapes,
                })
            });
            self.masters.sel = Some(n);
            self.masters.tool = MTool::Select;
            self.prop_tab = PropTab::Master;
            self.status = format!(
                "Logo placed ({count} shapes, crisp at any size): drag it into place, its corners size it."
            );
            return;
        }
        let img = match image::open(&p) {
            Ok(i) => i,
            Err(e) => {
                self.status = format!("Couldn't read {}: {e}", p.display());
                return;
            }
        };
        // a logo needs no more than this; the project stays light
        let img = if img.width().max(img.height()) > 1024 {
            img.resize(1024, 1024, image::imageops::FilterType::Triangle)
        } else {
            img
        };
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let Some((_, _, t)) = self.master() else {
            self.status = "Make a master first: + in the header.".into();
            return;
        };
        let paper = t.paper.unwrap_or(DVec2::new(420.0, 297.0));
        let o = t.corner.at(paper.x, paper.y);
        let mid = self.masters.camera.map_or(paper / 2.0, |(c, _)| c) - o;
        let size = DVec2::new(40.0, 40.0 * h as f64 / w.max(1) as f64);
        let n = t.items.len();
        let data = creabim_geom::raster::store_image(rgba.as_raw());
        self.edit_master("Picture", |t| {
            t.items.push(TitleItem::Image {
                lo: mid - size / 2.0,
                hi: mid + size / 2.0,
                width: w,
                height: h,
                rgba: data,
                opacity: 1.0,
            })
        });
        self.masters.sel = Some(n);
        self.masters.tool = MTool::Select;
        self.prop_tab = PropTab::Master;
        self.status = "Picture placed: drag it into place, its corners size it.".into();
    }

    /// `base`, or `base 2`, `base 3`…, whichever no element has (a name
    /// ending in a number, "Standard A3", keeps it).
    fn master_name(&self, base: &str) -> String {
        let model = self.doc.model();
        let taken = |n: &str| model.iter().any(|e| e.name == n);
        if !taken(base) {
            return base.to_string();
        }
        (2..)
            .map(|k| format!("{base} {k}"))
            .find(|n| !taken(n))
            .expect("unbounded")
    }

    /// Bring item `i` over everything (`front`) or under it.
    fn master_order(&mut self, i: usize, front: bool) {
        let mut to = 0;
        self.edit_master(if front { "To front" } else { "To back" }, |t| {
            if i < t.items.len() {
                let it = t.items.remove(i);
                if front {
                    t.items.push(it);
                    to = t.items.len() - 1;
                } else {
                    t.items.insert(0, it);
                }
            }
        });
        self.masters.sel = Some(to);
    }

    fn add_master(&mut self, name: String, t: SheetTemplate) {
        let e = Element::new(name.clone(), ElementKind::SheetTemplate(t));
        let id = e.id;
        let r = self.doc.commit(Transaction::new("New master").insert(e));
        self.report(
            r,
            format!("{name} made. Draw on it here; a sheet takes it in its Properties."),
        );
        self.open_master(Some(id));
    }

    /// The Masters view: the master's sheet, its drawing and the tools.
    pub(super) fn masters_area(&mut self, ui: &mut egui::Ui, body: Rect) {
        // what an edit this frame made shows at once, not on the next
        // pointer move (MU40: a rectangle seemed not to be drawn)
        let before = self.master().map(|m| m.2);
        if ui.rect_contains_pointer(body) {
            self.masters.hovered_pass = ui.ctx().cumulative_pass_nr();
        }
        let pal = self.shell.theme.palette();
        let dark = self.dark_paper(ui.ctx());
        let painter = ui.painter_at(body);
        // the desk the sheet lies on: a step darker than the panel in the
        // theme's own hue, so a white sheet stands out in light themes
        // without a harsh frame (MU40: white on light green read as one)
        let desk = if dark {
            pal.panel2
        } else {
            let k = 0.88;
            let c = pal.panel2;
            egui::Color32::from_rgb(
                (c.r() as f32 * k) as u8,
                (c.g() as f32 * k) as u8,
                (c.b() as f32 * k) as u8,
            )
        };
        painter.rect_filled(body, 0.0, desk);
        let response = ui.interact(
            body,
            ui.id().with("masters.view"),
            egui::Sense::click_and_drag(),
        );
        let Some((_id, _name, t)) = self.master() else {
            painter.text(
                body.center(),
                egui::Align2::CENTER_CENTER,
                "No masters yet. + in the header makes one from Standard, Minimal or blank,\nor brings in a frame drawn elsewhere.",
                egui::FontId::proportional(14.0),
                pal.muted,
            );
            return;
        };
        let paper = t.paper.unwrap_or(DVec2::new(420.0, 297.0));
        let o = t.corner.at(paper.x, paper.y);
        // a new size (maximised, a divider dragged) fits the sheet again
        if (self.masters.seen - body.size()).length() > 1.0 {
            self.masters.seen = body.size();
            self.masters.camera = None;
        }
        // the camera: the sheet fitted, then wheel and right-drag
        let fit = || {
            let k = ((body.width() - 120.0).max(80.0) as f64 / paper.x)
                .min((body.height() - 60.0).max(60.0) as f64 / paper.y) as f32;
            (paper / 2.0 + DVec2::new(14.0 / k as f64, 0.0), k.max(0.2))
        };
        let (mut center, mut scale) = self.masters.camera.unwrap_or_else(fit);
        if response.hovered() {
            let (pan, scroll) = crate::planview::wheel(ui);
            if pan != egui::Vec2::ZERO {
                center -= DVec2::new(pan.x as f64, -pan.y as f64) / scale as f64;
            }
            if scroll != 0.0 {
                if let Some(h) = response.hover_pos() {
                    let k = (1.0 + scroll / 300.0).clamp(0.5, 2.0);
                    let off = DVec2::new(
                        (h.x - body.center().x) as f64,
                        -(h.y - body.center().y) as f64,
                    );
                    let w = center + off / scale as f64;
                    scale = (scale * k).clamp(0.2, 200.0);
                    center = w - off / scale as f64;
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
                scale = (scale * k).clamp(0.2, 200.0);
                center = w - off / scale as f64;
            }
        } else if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            let d = response.drag_delta();
            center -= DVec2::new(d.x as f64, -d.y as f64) / scale as f64;
        }
        let picked_fit = self.masters.sel.and_then(|i| t.items.get(i)).map(|it| {
            let (lo, hi) = item_box(it);
            let span = (hi - lo).max(DVec2::splat(10.0));
            let k = ((body.width() - 160.0).max(60.0) as f64 / span.x)
                .min((body.height() - 120.0).max(60.0) as f64 / span.y) as f32;
            (o + (lo + hi) / 2.0, k.clamp(0.2, 200.0))
        });
        // the zoom column, as every view has (MU40), drawn last
        let (nav, look) =
            crate::planview::nav_input(ui, body, ElementId::NIL, fit, picked_fit, center, scale);
        if let Some((c, k)) = nav {
            center = c;
            scale = k;
        }
        self.masters.camera = Some((center, scale));
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
        // the sheet, white, or dark as the drawings are (MU40: "design in
        // dark also"); prints are always on white
        let (sheet_fill, ink, faint_c, strong_c) = if dark {
            (
                egui::Color32::from_gray(32),
                egui::Color32::from_gray(225),
                egui::Color32::from_gray(44),
                egui::Color32::from_gray(58),
            )
        } else {
            (
                egui::Color32::WHITE,
                egui::Color32::from_gray(25),
                egui::Color32::from_gray(238),
                egui::Color32::from_gray(222),
            )
        };
        // black on white turns light on dark paper; colours keep theirs
        let ink_of = |c: [f32; 3], a: f32| {
            if dark && c.iter().all(|v| *v < 0.12) {
                ink.gamma_multiply(a)
            } else {
                rgb(c, a)
            }
        };
        let sheet = Rect::from_two_pos(to_s(DVec2::ZERO), to_s(paper));
        painter.rect_filled(
            sheet.translate(egui::vec2(0.0, 3.0)),
            2.0,
            egui::Color32::from_black_alpha(40),
        );
        painter.rect_filled(
            sheet.translate(egui::vec2(0.0, 6.0)).expand(2.0),
            4.0,
            egui::Color32::from_black_alpha(if dark { 40 } else { 26 }),
        );
        painter.rect_filled(sheet, 0.0, sheet_fill);
        // its edge, fine and a shade of the desk
        painter.rect_stroke(
            sheet,
            0.0,
            egui::Stroke::new(
                1.0,
                egui::Color32::from_black_alpha(if dark { 90 } else { 60 }),
            ),
            egui::StrokeKind::Outside,
        );
        // its millimetre grid, the finest step at least 8 px apart
        let step = [1.0, 5.0, 10.0, 50.0]
            .into_iter()
            .find(|g| g * scale as f64 >= 8.0)
            .unwrap_or(100.0);
        let faint = egui::Stroke::new(1.0, faint_c);
        let strong = egui::Stroke::new(1.0, strong_c);
        let mut x = step;
        while x < paper.x {
            let s = if ((x / step).round() as i64) % 5 == 0 {
                strong
            } else {
                faint
            };
            painter.line_segment([to_s(DVec2::new(x, 0.0)), to_s(DVec2::new(x, paper.y))], s);
            x += step;
        }
        let mut y = step;
        while y < paper.y {
            let s = if ((y / step).round() as i64) % 5 == 0 {
                strong
            } else {
                faint
            };
            painter.line_segment([to_s(DVec2::new(0.0, y)), to_s(DVec2::new(paper.x, y))], s);
            y += step;
        }
        let pen =
            |w: f64, c: egui::Color32| egui::Stroke::new(((w * scale as f64) as f32).max(1.0), c);
        // the margins, dashed, and the border if it has one
        let (mut blo, mut bhi) = t.border_box(paper);
        match self.masters.border_drag {
            Some((0, v)) => blo.x = v,
            Some((1, v)) => blo.y = v,
            Some((2, v)) => bhi.x = v,
            Some((3, v)) => bhi.y = v,
            _ => {}
        }
        let margin = [
            blo,
            DVec2::new(bhi.x, blo.y),
            bhi,
            DVec2::new(blo.x, bhi.y),
            blo,
        ];
        if t.border {
            painter.add(egui::Shape::line(
                margin.iter().map(|p| to_s(*p)).collect(),
                pen(0.5, ink),
            ));
        } else {
            let m: Vec<egui::Pos2> = margin.iter().map(|p| to_s(*p)).collect();
            painter.extend(egui::Shape::dashed_line(
                &m,
                egui::Stroke::new(1.0, egui::Color32::from_gray(140)),
                5.0,
                4.0,
            ));
        }
        // the border picked: in the accent, a grip on each edge to drag
        let edge_mids = [
            DVec2::new(blo.x, (blo.y + bhi.y) / 2.0),
            DVec2::new((blo.x + bhi.x) / 2.0, blo.y),
            DVec2::new(bhi.x, (blo.y + bhi.y) / 2.0),
            DVec2::new((blo.x + bhi.x) / 2.0, bhi.y),
        ];
        if self.masters.border_sel {
            let accent = crate::theme::interaction();
            painter.add(egui::Shape::line(
                margin.iter().map(|p| to_s(*p)).collect(),
                egui::Stroke::new(2.0, accent),
            ));
            for m in edge_mids {
                painter.rect_filled(
                    Rect::from_center_size(to_s(m), egui::vec2(9.0, 9.0)),
                    2.0,
                    accent,
                );
            }
        }
        // which border edge is near a point on screen, if any
        let border_edge = |s: egui::Pos2| -> Option<usize> {
            let (a, b) = (to_s(blo), to_s(bhi));
            let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
            let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
            let along_x = s.x > x0 - 6.0 && s.x < x1 + 6.0;
            let along_y = s.y > y0 - 6.0 && s.y < y1 + 6.0;
            [
                (0, (s.x - a.x).abs() < 5.0 && along_y),
                (1, (s.y - a.y).abs() < 5.0 && along_x),
                (2, (s.x - b.x).abs() < 5.0 && along_y),
                (3, (s.y - b.y).abs() < 5.0 && along_x),
            ]
            .into_iter()
            .find(|(_, near)| *near)
            .map(|(e, _)| e)
        };
        // the picked item as a drag, a resize or a Move leaves it
        let here_raw = response.hover_pos().map(to_w);
        let mut items = t.items.clone();
        if let Some(i) = self.masters.sel {
            if let Some(it) = items.get_mut(i) {
                match self.masters.drag {
                    Some(Drag::Move(_, d)) => it.shift(d),
                    Some(Drag::Size(anchor, from, now)) => {
                        let (a, f, n) = (anchor - o, from - o, now - o);
                        let k = (n - a) / (f - a);
                        if k.is_finite() && k.abs().min_element() > 0.02 {
                            scale_item(it, a, k);
                        }
                    }
                    None => {}
                }
                if let (Some(from), Some(w)) = (self.masters.follow, here_raw) {
                    if from.is_finite() {
                        it.shift(w - from);
                    }
                }
            }
        }
        let accent = crate::theme::interaction();
        // where each text is on screen, as drawn: the pick box follows it
        // (MU40: the box sat off its field)
        let mut text_rects: Vec<(usize, Rect)> = vec![];
        for (idx, item) in items.iter().enumerate() {
            let laid = match item {
                TitleItem::Logo { .. } => item.logo_shapes(),
                _ => vec![item.clone()],
            };
            for it in &laid {
                match it {
                    TitleItem::Logo { .. } => {}
                    TitleItem::Line { points, width } => {
                        painter.add(egui::Shape::line(
                            points.iter().map(|p| to_s(o + *p)).collect(),
                            pen(*width, ink),
                        ));
                    }
                    TitleItem::Fill { points, gray } => {
                        let g = (gray.clamp(0.0, 1.0) * 255.0) as u8;
                        let c = if dark {
                            egui::Color32::from_gray(255 - g)
                        } else {
                            egui::Color32::from_gray(g)
                        };
                        painter.add(egui::Shape::convex_polygon(
                            points.iter().map(|p| to_s(o + *p)).collect(),
                            c,
                            egui::Stroke::NONE,
                        ));
                    }
                    TitleItem::Shape {
                        points,
                        closed,
                        pen: w,
                        colour,
                        fill,
                        opacity,
                    } => {
                        let pts: Vec<egui::Pos2> = points.iter().map(|p| to_s(o + *p)).collect();
                        let a = *opacity as f32;
                        if let (Some(f), true) = (fill, pts.len() >= 3) {
                            // any outline, not only a convex one, as one mesh:
                            // triangles drawn one by one showed their seams
                            let mut mesh = egui::Mesh::default();
                            let c = ink_of(*f, a);
                            for p in &pts {
                                mesh.colored_vertex(*p, c);
                            }
                            for tri in creabim_geom::triangulate::triangulate(points) {
                                mesh.add_triangle(tri[0] as u32, tri[1] as u32, tri[2] as u32);
                            }
                            painter.add(egui::Shape::mesh(mesh));
                        }
                        if let Some(w) = w {
                            let mut line = pts.clone();
                            if *closed && line.len() >= 2 {
                                line.push(line[0]);
                            }
                            painter.add(egui::Shape::line(line, pen(*w, ink_of(*colour, a))));
                        }
                    }
                    TitleItem::Image {
                        lo,
                        hi,
                        width,
                        height,
                        rgba,
                        opacity,
                    } => {
                        let r = Rect::from_two_pos(to_s(o + *lo), to_s(o + *hi));
                        match creabim_geom::raster::stored_image(*width, *height, rgba) {
                            Some(ras) => {
                                let tex = texture(ui.ctx(), &ras);
                                painter.image(
                                    tex,
                                    r,
                                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                    egui::Color32::WHITE.gamma_multiply(*opacity as f32),
                                );
                            }
                            None => {
                                painter.rect_stroke(
                                    r,
                                    0.0,
                                    egui::Stroke::new(1.0, pal.muted),
                                    egui::StrokeKind::Middle,
                                );
                            }
                        }
                    }
                    TitleItem::Text {
                        at,
                        size,
                        text,
                        align,
                        angle,
                    } => {
                        // as sheets draw it: the size is the font's, in mm
                        let px = (*size as f32 * scale).max(4.0);
                        let field = text.contains('{');
                        let colour = if field {
                            if dark {
                                egui::Color32::from_rgb(120, 170, 240)
                            } else {
                                egui::Color32::from_rgb(30, 90, 170)
                            }
                        } else {
                            ink
                        };
                        let g = painter.layout_no_wrap(
                            text.clone(),
                            egui::FontId::proportional(px),
                            colour,
                        );
                        let sz = g.size();
                        let dx = match align {
                            TextAlign::Start => 0.0,
                            TextAlign::Middle => sz.x / 2.0,
                            TextAlign::End => sz.x,
                        };
                        let a = -(*angle as f32);
                        let rot = |v: egui::Vec2| {
                            let (s, c) = a.sin_cos();
                            egui::vec2(v.x * c - v.y * s, v.x * s + v.y * c)
                        };
                        let p = to_s(o + *at);
                        let top_left = p + rot(egui::vec2(-dx, -sz.y / 2.0));
                        // the box round the letters as drawn, turned or not:
                        // a guess from the letter count fell short of capitals
                        let corners = [
                            egui::vec2(0.0, 0.0),
                            egui::vec2(sz.x, 0.0),
                            egui::vec2(sz.x, sz.y),
                            egui::vec2(0.0, sz.y),
                        ]
                        .map(|v| top_left + rot(v));
                        text_rects.push((idx, Rect::from_points(&corners)));
                        // a field is a chip: what each sheet fills in
                        if field && a == 0.0 {
                            painter.rect_filled(
                                Rect::from_min_size(top_left, sz).expand(1.5),
                                3.0,
                                egui::Color32::from_rgba_unmultiplied(40, 110, 220, 30),
                            );
                        }
                        painter
                            .add(egui::epaint::TextShape::new(top_left, g, colour).with_angle(a));
                    }
                }
            }
        }
        // the guides, dashed, across the whole view
        let guides: Vec<(DVec2, DVec2)> = if self.masters.hide_guides {
            vec![]
        } else {
            t.guides
                .iter()
                .enumerate()
                .map(|(g, (a, b))| match self.masters.guide_drag {
                    Some((k, _, d)) if k == g => (o + *a + d, o + *b + d),
                    _ => (o + *a, o + *b),
                })
                .collect()
        };
        let reach = (body.width() + body.height()) as f64 / scale as f64;
        for (g, (a, b)) in guides.iter().enumerate() {
            let d = (*b - *a).normalize_or(DVec2::X);
            let ends = [to_s(*a - d * reach), to_s(*a + d * reach)];
            let ink_g = if self.masters.guide_sel == Some(g) {
                accent
            } else {
                GUIDE
            };
            painter.extend(egui::Shape::dashed_line(
                &ends,
                egui::Stroke::new(1.0, ink_g),
                7.0,
                4.0,
            ));
        }
        // the picked item's box and corner handles
        let mut handles: Vec<(egui::Pos2, DVec2, DVec2)> = vec![];
        if let Some(it) = self.masters.sel.and_then(|i| items.get(i)) {
            let (lo, hi) = match text_rects
                .iter()
                .find(|(k, _)| Some(*k) == self.masters.sel)
            {
                Some((_, r)) => (to_w(r.left_bottom()) - o, to_w(r.right_top()) - o),
                None => item_box(it),
            };
            let r = Rect::from_two_pos(to_s(o + lo), to_s(o + hi)).expand(3.0);
            painter.rect_stroke(
                r,
                2.0,
                egui::Stroke::new(1.5, accent),
                egui::StrokeKind::Middle,
            );
            // each corner, and the one across from it that stays put
            for (c, far) in [
                (DVec2::new(lo.x, lo.y), DVec2::new(hi.x, hi.y)),
                (DVec2::new(hi.x, lo.y), DVec2::new(lo.x, hi.y)),
                (DVec2::new(hi.x, hi.y), DVec2::new(lo.x, lo.y)),
                (DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y)),
            ] {
                let s = to_s(o + c);
                let s = s + (s - r.center()).normalized() * 3.0;
                painter.rect_filled(Rect::from_center_size(s, egui::vec2(7.0, 7.0)), 1.0, accent);
                handles.push((s, o + c, o + far));
            }
        }
        // snapping: what is drawn, the paper's edges and margins, then the
        // millimetre grid; Shift lets go of all but the grid
        let shift_held = ui.input(|i| i.modifiers.shift);
        let tol = 8.0 / scale as f64;
        let mut targets: Vec<DVec2> = vec![
            DVec2::ZERO,
            DVec2::new(paper.x, 0.0),
            paper,
            DVec2::new(0.0, paper.y),
        ];
        targets.extend(margin.iter().take(4));
        let moving = self.masters.drag.is_some() || self.masters.follow.is_some();
        for (i, it) in t.items.iter().enumerate() {
            if Some(i) == self.masters.sel && moving {
                continue;
            }
            targets.extend(it.points().iter().map(|p| o + *p));
            if let TitleItem::Line { points, .. } | TitleItem::Shape { points, .. } = it {
                targets.extend(points.windows(2).map(|w| o + (w[0] + w[1]) / 2.0));
            }
        }
        targets.extend(self.masters.drawing.iter().copied());
        // where guides cross each other
        let dragged_guide = self.masters.guide_drag.map(|g| g.0);
        for (k, (a, b)) in guides.iter().enumerate() {
            if Some(k) == dragged_guide {
                continue;
            }
            for (_, (c, d)) in guides
                .iter()
                .enumerate()
                .skip(k + 1)
                .filter(|(j, _)| Some(*j) != dragged_guide)
            {
                targets.extend(cross(*a, *b, *c, *d));
            }
        }
        let xs: Vec<f64> = [0.0, blo.x, bhi.x, paper.x]
            .into_iter()
            .chain(targets.iter().map(|p| p.x))
            .collect();
        let ys: Vec<f64> = [0.0, blo.y, bhi.y, paper.y]
            .into_iter()
            .chain(targets.iter().map(|p| p.y))
            .collect();
        let grid_step = if scale >= 12.0 { 0.5 } else { 1.0 };
        let snap = |w: DVec2| -> (DVec2, bool) {
            if !shift_held {
                if let Some(p) = targets
                    .iter()
                    .filter(|p| p.distance(w) < tol)
                    .min_by(|a, b| a.distance(w).total_cmp(&b.distance(w)))
                {
                    return (*p, true);
                }
                // onto the nearest guide, then the grid along it
                if let Some(q) = guides
                    .iter()
                    .enumerate()
                    .filter(|(g, _)| Some(*g) != dragged_guide)
                    .map(|(_, (a, b))| onto(w, *a, *b))
                    .filter(|(_, d)| *d < tol)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                {
                    return (q.0, true);
                }
            }
            let near = |v: f64, all: &[f64]| {
                all.iter()
                    .filter(|a| (*a - v).abs() < tol * 0.75)
                    .min_by(|a, b| (*a - v).abs().total_cmp(&(*b - v).abs()))
                    .copied()
            };
            let gx = (w.x / grid_step).round() * grid_step;
            let gy = (w.y / grid_step).round() * grid_step;
            if shift_held {
                return (DVec2::new(gx, gy), false);
            }
            let x = near(w.x, &xs);
            let y = near(w.y, &ys);
            (
                DVec2::new(x.unwrap_or(gx), y.unwrap_or(gy)),
                x.is_some() || y.is_some(),
            )
        };
        let here = here_raw.map(snap);
        // the snap ring and the place only while drawing or moving: Select
        // at rest shows none (MU40: "uncalled for")
        let busy = self.masters.tool != MTool::Select || moving;
        if let (Some((w, on)), true) = (here, busy) {
            if on {
                painter.circle_stroke(to_s(w), 5.0, egui::Stroke::new(1.5, accent));
            }
            let rel = w - o;
            painter.text(
                to_s(w) + egui::vec2(12.0, 14.0),
                egui::Align2::LEFT_TOP,
                format!("{:.1}, {:.1} mm", rel.x, rel.y),
                egui::FontId::proportional(11.0),
                pal.muted,
            );
        }
        let typing = ui.ctx().egui_wants_keyboard_input();
        let hovered = response.hovered();
        let keys = |k: egui::Key| hovered && !typing && ui.input(|i| i.key_pressed(k));
        // Move (M): the picked item follows the pointer; a click puts it
        // down where it snaps, Esc leaves it
        if let Some(from) = self.masters.follow {
            if !from.is_finite() {
                self.masters.follow = here_raw;
            } else if keys(egui::Key::Escape) || response.secondary_clicked() {
                self.masters.follow = None;
                self.status = "Move: left where it was.".into();
            } else if response.clicked() {
                if let (Some(i), Some((w, _))) = (self.masters.sel, here) {
                    // the grabbed point lands on the snap
                    let d = w - from;
                    self.edit_master("Move", |t| {
                        if let Some(it) = t.items.get_mut(i) {
                            it.shift(d);
                        }
                    });
                }
                self.masters.follow = None;
            }
        } else {
            match self.masters.tool {
                MTool::Select => {
                    let picked_at = |p: DVec2| {
                        // a text where it is drawn
                        let s = to_s(p);
                        if let Some((k, _)) = text_rects
                            .iter()
                            .rev()
                            .find(|(_, r)| r.expand(2.0).contains(s))
                        {
                            return Some(*k);
                        }
                        t.items
                            .iter()
                            .enumerate()
                            .rev()
                            .map(|(i, it)| (i, item_dist(it, p - o)))
                            .filter(|(_, d)| *d < 6.0 / scale as f64)
                            .min_by(|a, b| a.1.total_cmp(&b.1))
                            .map(|(i, _)| i)
                    };
                    // a guide near the pointer, when nothing drawn is
                    let guide_at = |p: DVec2| {
                        guides
                            .iter()
                            .enumerate()
                            .map(|(g, (a, b))| (g, onto(p, *a, *b).1))
                            .filter(|(_, d)| *d < 5.0 / scale as f64)
                            .min_by(|a, b| a.1.total_cmp(&b.1))
                            .map(|(g, _)| g)
                    };
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        // where the button went down: by the time a drag
                        // starts the pointer has left a small handle
                        let down = ui
                            .input(|i| i.pointer.press_origin())
                            .or(response.interact_pointer_pos());
                        if let Some(p) = down {
                            let w = to_w(p);
                            // a corner of what is picked resizes it
                            if let Some((_, c, far)) =
                                handles.iter().find(|(s, _, _)| s.distance(p) < 8.0)
                            {
                                self.masters.drag = Some(Drag::Size(*far, *c, *c));
                            } else if let Some(i) = picked_at(w) {
                                self.masters.sel = Some(i);
                                self.masters.guide_sel = None;
                                self.masters.drag = Some(Drag::Move(w, DVec2::ZERO));
                            } else if let Some(g) = guide_at(w) {
                                self.masters.guide_sel = Some(g);
                                self.masters.sel = None;
                                self.masters.guide_drag = Some((g, w, DVec2::ZERO));
                            } else if let Some(e) = border_edge(p) {
                                // the border's edge, dragged: its margin
                                self.masters.border_sel = true;
                                self.masters.sel = None;
                                self.masters.guide_sel = None;
                                let v = [blo.x, blo.y, bhi.x, bhi.y][e];
                                self.masters.border_drag = Some((e, v));
                            }
                        }
                    }
                    if let (Some((e, _)), Some(w)) = (self.masters.border_drag, here_raw) {
                        let at = snap(w).0;
                        let v = if e % 2 == 0 { at.x } else { at.y };
                        // never past the paper, nor across the other edge
                        let v = match e {
                            0 => v.clamp(0.0, bhi.x - 10.0),
                            1 => v.clamp(0.0, bhi.y - 10.0),
                            2 => v.clamp(blo.x + 10.0, paper.x),
                            _ => v.clamp(blo.y + 10.0, paper.y),
                        };
                        self.masters.border_drag = Some((e, v));
                        if response.drag_stopped_by(egui::PointerButton::Primary) {
                            self.masters.border_drag = None;
                            let (lo, hi) = t.border_box(paper);
                            let mut m = [lo.x, lo.y, paper.x - hi.x, paper.y - hi.y];
                            m[e] = if e < 2 {
                                v
                            } else if e == 2 {
                                paper.x - v
                            } else {
                                paper.y - v
                            };
                            self.edit_master("Border margin", |t| t.margins = Some(m));
                            self.status = format!(
                                "Margins {:.0}, {:.0}, {:.0}, {:.0} mm (left, bottom, right, top).",
                                m[0], m[1], m[2], m[3]
                            );
                        }
                    }
                    // a guide dragged goes with the pointer, snapping
                    if let (Some((g, from, _)), Some(w)) = (self.masters.guide_drag, here_raw) {
                        let d = snap(w).0 - snap(from).0;
                        self.masters.guide_drag = Some((g, from, d));
                        if response.drag_stopped_by(egui::PointerButton::Primary) {
                            self.masters.guide_drag = None;
                            if d.length() > 1e-9 {
                                self.edit_master("Move guide", |t| {
                                    if let Some((a, b)) = t.guides.get_mut(g) {
                                        *a += d;
                                        *b += d;
                                    }
                                });
                            }
                        }
                    }
                    match (self.masters.drag, here_raw) {
                        (Some(Drag::Move(from, _)), Some(w)) => {
                            // the item's first point snaps; the rest follow
                            let first = self
                                .masters
                                .sel
                                .and_then(|i| t.items.get(i))
                                .and_then(|it| it.points().first().copied());
                            let d = match first {
                                Some(f) => snap(o + f + (w - from)).0 - (o + f),
                                None => w - from,
                            };
                            self.masters.drag = Some(Drag::Move(from, d));
                        }
                        (Some(Drag::Size(far, from, _)), Some(w)) => {
                            let mut to = snap(w).0;
                            // Shift keeps its proportions
                            if shift_held {
                                let d0 = from - far;
                                let k = ((to - far) / d0).abs().max_element();
                                to = far + d0 * k;
                            }
                            self.masters.drag = Some(Drag::Size(far, from, to));
                        }
                        _ => {}
                    }
                    if response.drag_stopped_by(egui::PointerButton::Primary) {
                        match (self.masters.sel, self.masters.drag.take()) {
                            (Some(i), Some(Drag::Move(_, d))) if d.length() > 1e-9 => {
                                self.edit_master("Move", |t| {
                                    if let Some(it) = t.items.get_mut(i) {
                                        it.shift(d);
                                    }
                                });
                            }
                            (Some(i), Some(Drag::Size(far, from, now))) => {
                                let (a, f, n) = (far - o, from - o, now - o);
                                let k = (n - a) / (f - a);
                                if k.is_finite()
                                    && k.abs().min_element() > 0.02
                                    && (k - DVec2::ONE).length() > 1e-6
                                {
                                    self.edit_master("Resize", |t| {
                                        if let Some(it) = t.items.get_mut(i) {
                                            scale_item(it, a, k);
                                        }
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                    if response.clicked() {
                        let s = response.interact_pointer_pos();
                        let at = s.map(to_w);
                        self.masters.sel = at.and_then(picked_at);
                        self.masters.guide_sel = if self.masters.sel.is_none() {
                            at.and_then(guide_at)
                        } else {
                            None
                        };
                        // nothing else there: the border's edge picks it
                        self.masters.border_sel = self.masters.sel.is_none()
                            && self.masters.guide_sel.is_none()
                            && s.and_then(border_edge).is_some();
                        if self.masters.border_sel {
                            self.status = "The border: drag an edge to move its margin, or type them in Properties.".into();
                        }
                    }
                    // a guide picked: Delete takes it away, Esc lets go
                    if let (Some(g), true) = (self.masters.guide_sel, hovered && !typing) {
                        if keys(egui::Key::Delete) || keys(egui::Key::Backspace) {
                            self.edit_master("Delete guide", |t| {
                                if g < t.guides.len() {
                                    t.guides.remove(g);
                                }
                            });
                            self.masters.guide_sel = None;
                            self.status = "Guide deleted. Ctrl+Z brings it back.".into();
                        } else if keys(egui::Key::Escape) {
                            self.masters.guide_sel = None;
                        }
                    }
                    if self.masters.sel.is_some() && hovered && !typing {
                        let i = self.masters.sel.unwrap_or(0);
                        let step = if shift_held { 5.0 } else { 1.0 };
                        let (l, r, u, dn) = ui.input(|k| {
                            (
                                k.key_pressed(egui::Key::ArrowLeft),
                                k.key_pressed(egui::Key::ArrowRight),
                                k.key_pressed(egui::Key::ArrowUp),
                                k.key_pressed(egui::Key::ArrowDown),
                            )
                        });
                        let d =
                            DVec2::new((r as i32 - l as i32) as f64, (u as i32 - dn as i32) as f64)
                                * step;
                        if d != DVec2::ZERO {
                            self.edit_master("Nudge", |t| {
                                if let Some(it) = t.items.get_mut(i) {
                                    it.shift(d);
                                }
                            });
                        }
                        let cmd = ui.input(|k| k.modifiers.command);
                        if keys(egui::Key::Delete) || keys(egui::Key::Backspace) {
                            self.masters_command(Command::Delete);
                        } else if cmd && keys(egui::Key::D) {
                            self.masters_command(Command::Duplicate);
                        } else if cmd && keys(egui::Key::CloseBracket) {
                            self.master_order(i, true);
                        } else if cmd && keys(egui::Key::OpenBracket) {
                            self.master_order(i, false);
                        } else if !cmd && keys(egui::Key::M) {
                            self.masters_command(Command::Move);
                        } else if !cmd && keys(egui::Key::R) {
                            self.masters_command(Command::Rotate);
                        } else if keys(egui::Key::Escape) {
                            self.masters.sel = None;
                        }
                    }
                }
                MTool::Line | MTool::Rect => {
                    let rect_tool = self.masters.tool == MTool::Rect;
                    if let (Some(last), Some((w, _))) = (self.masters.drawing.last(), here) {
                        if rect_tool {
                            let mut pts = shape_points(self.masters.shape_kind, *last, w);
                            pts.push(pts[0]);
                            painter.add(egui::Shape::line(
                                pts.iter().map(|p| to_s(*p)).collect(),
                                egui::Stroke::new(1.5, accent),
                            ));
                        } else {
                            let mut pts: Vec<egui::Pos2> =
                                self.masters.drawing.iter().map(|p| to_s(*p)).collect();
                            pts.push(to_s(w));
                            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, accent)));
                        }
                    }
                    let mut finish = false;
                    let mut close = false;
                    if response.clicked() {
                        if let Some((w, _)) = here {
                            let d = &mut self.masters.drawing;
                            if rect_tool && d.len() == 1 {
                                let a = d[0];
                                d.clear();
                                let kind = self.masters.shape_kind;
                                let big = if kind == 1 {
                                    a.distance(w) > 0.1
                                } else {
                                    (w - a).abs().min_element() > 0.1
                                };
                                if big {
                                    let pts: Vec<DVec2> = shape_points(kind, a, w)
                                        .into_iter()
                                        .map(|p| p - o)
                                        .collect();
                                    let n = t.items.len();
                                    let made =
                                        self.edit_master(SHAPE_KINDS[kind as usize % 3].0, |t| {
                                            t.items.push(TitleItem::Shape {
                                                points: pts,
                                                closed: true,
                                                pen: Some(0.35),
                                                colour: [0.0; 3],
                                                fill: None,
                                                opacity: 1.0,
                                            })
                                        });
                                    if made {
                                        self.masters.sel = Some(n);
                                        self.status = format!(
                                            "{} drawn: its pen, fill and opacity are in Properties.",
                                            SHAPE_KINDS[kind as usize % 3].0
                                        );
                                    }
                                }
                            } else if !rect_tool && d.len() >= 2 && d[0].distance(w) < 1e-6 {
                                finish = true;
                                close = true;
                            } else if !rect_tool && d.last().is_some_and(|q| q.distance(w) < 1e-6) {
                                finish = true;
                            } else {
                                d.push(w);
                            }
                        }
                    }
                    // one rule for every tool (MU40): Enter or a right-click
                    // ends what is being drawn and keeps what you see; Esc
                    // drops the piece not yet made; either, with nothing
                    // going, puts the tool down (back to Select)
                    let idle = self.masters.drawing.is_empty();
                    if keys(egui::Key::Enter) || response.secondary_clicked() {
                        if idle {
                            self.masters.tool = MTool::Select;
                        } else if rect_tool {
                            self.masters.drawing.clear();
                        } else {
                            finish = true;
                        }
                    }
                    if keys(egui::Key::Escape) {
                        if idle {
                            self.masters.tool = MTool::Select;
                        }
                        self.masters.drawing.clear();
                    }
                    if finish {
                        let pts: Vec<DVec2> = std::mem::take(&mut self.masters.drawing)
                            .into_iter()
                            .map(|p| p - o)
                            .collect();
                        if pts.len() >= 2 {
                            let n = t.items.len();
                            self.edit_master("Line", |t| {
                                t.items.push(TitleItem::Shape {
                                    points: pts,
                                    closed: close,
                                    pen: Some(0.25),
                                    colour: [0.0; 3],
                                    fill: None,
                                    opacity: 1.0,
                                })
                            });
                            self.masters.sel = Some(n);
                        }
                    }
                }
                MTool::Guide => {
                    if let (Some(a), Some((w, _))) = (self.masters.guide_from, here) {
                        let b = straighten(a, w);
                        let d = (b - a).normalize_or(DVec2::X);
                        painter.extend(egui::Shape::dashed_line(
                            &[to_s(a - d * reach), to_s(a + d * reach)],
                            egui::Stroke::new(1.0, accent),
                            7.0,
                            4.0,
                        ));
                    }
                    if response.clicked() {
                        if let Some((w, _)) = here {
                            match self.masters.guide_from.take() {
                                None => {
                                    self.masters.guide_from = Some(w);
                                    self.status =
                                        "Guide: click a second point it runs through.".into();
                                }
                                Some(a) => {
                                    let b = straighten(a, w);
                                    if a.distance(b) > 0.1 {
                                        self.masters.hide_guides = false;
                                        self.edit_master("Guide", |t| {
                                            t.guides.push((a - o, b - o))
                                        });
                                        self.status = "Guide added: lines, text and pictures snap to it. Another two clicks make another.".into();
                                    }
                                }
                            }
                        }
                    }
                    if (keys(egui::Key::Escape) || response.secondary_clicked())
                        && self.masters.guide_from.take().is_none()
                    {
                        self.masters.tool = MTool::Select;
                    }
                }
                MTool::Text | MTool::Field => {
                    if response.clicked() {
                        if let Some((w, _)) = here {
                            let words = if self.masters.tool == MTool::Field {
                                let f = if self.masters.field.is_empty() {
                                    "project".to_string()
                                } else {
                                    self.masters.field.clone()
                                };
                                format!("{{{f}}}")
                            } else {
                                "Text".to_string()
                            };
                            let n = t.items.len();
                            self.edit_master("Text", |t| {
                                t.items.push(TitleItem::Text {
                                    at: w - o,
                                    size: 3.0,
                                    text: words,
                                    align: TextAlign::Start,
                                    angle: 0.0,
                                })
                            });
                            self.masters.sel = Some(n);
                            self.prop_tab = PropTab::Master;
                            self.status =
                                "Placed. Its words, size and alignment are in Properties.".into();
                        }
                    }
                    // a right-click or Esc ends the tool: what is placed stays
                    if keys(egui::Key::Escape) || response.secondary_clicked() {
                        self.masters.tool = MTool::Select;
                        self.status = "Select.".into();
                    }
                }
            }
        }
        // what the pointer does, at the foot of the view
        let hint = if self.masters.follow.is_some() {
            "Move: click to put it down, Esc or a right-click leaves it"
        } else {
            match self.masters.tool {
                MTool::Select => "Click to pick, drag to move, a corner resizes (Shift keeps its shape); M moves, R turns, Ctrl+D copies, Delete removes",
                MTool::Line => "Click corners; Enter or a right-click ends it, the first corner again closes it; Esc drops it",
                MTool::Rect => "Rectangle and ellipse: two corners; circle: its centre, then a point on it. Esc or a right-click puts the tool down",
                MTool::Text => "Click where the text goes; Esc back to Select",
                MTool::Field => "Click where the field goes: each sheet fills it in; Esc back to Select",
                MTool::Guide => "Click two points the guide runs through; nearly level or upright, it goes straight. Esc back to Select",
            }
        };
        painter.text(
            body.left_bottom() + egui::vec2(12.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            hint,
            egui::FontId::proportional(11.5),
            pal.muted,
        );
        if let Some(look) = look {
            crate::planview::nav_draw(ui, &look);
        }
        if self.master().map(|m| m.2) != before {
            ui.ctx().request_repaint();
        }
        // naming a field of your own, over the view's top left
        if let Some(mut name) = self.masters.asking_field.take() {
            let mut keep = true;
            let mut add = false;
            egui::Area::new(egui::Id::new("master.newfield"))
                .order(egui::Order::Foreground)
                .fixed_pos(body.left_top() + egui::vec2(16.0, 12.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(egui::RichText::new("Your own field").strong());
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut name)
                                .vertical_align(egui::Align::Center)
                                .hint_text("client, drawn by, checked…")
                                .desired_width(200.0),
                        );
                        r.request_focus();
                        ui.horizontal(|ui| {
                            add = ui.button("Add and place").clicked()
                                || (r.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                keep = false;
                            }
                        });
                        ui.label(
                            egui::RichText::new(
                                "Each sheet fills it in; it shows its default until then.",
                            )
                            .small()
                            .weak(),
                        );
                    });
                });
            let clean: String = name
                .trim()
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect();
            if add && !clean.is_empty() {
                let builtin = tb::BUILT_IN_FIELDS.iter().any(|(b, _)| *b == clean);
                let have = t.fields.iter().any(|(f, _)| *f == clean);
                if !builtin && !have {
                    let c = clean.clone();
                    self.edit_master("Master field", |t| t.fields.push((c, String::new())));
                }
                self.masters.field = clean.clone();
                self.masters.tool = MTool::Field;
                self.status = format!("Click where {{{clean}}} goes: each sheet fills it in.");
            } else if keep && !add {
                self.masters.asking_field = Some(name);
            }
        }
    }

    /// The Master tab of Properties while a Masters view is in front.
    pub(super) fn master_props(&mut self, ui: &mut egui::Ui) {
        // colours are the print's; dark paper shows black light
        let dark = self.dark_paper(ui.ctx());
        let Some((id, name, t)) = self.master() else {
            crate::props::section(ui, "Master");
            crate::props::note(
                ui,
                "No masters yet: + in the Masters view's header makes one.",
            );
            return;
        };
        let users = super::drafting::template_users(self.doc.model(), id);
        if self.prop_header(ui, "master_about", "Master", true, None) {
            // a draft that outlives the frame: a copy remade each frame
            // threw away what was typed
            let draft = self.masters.names.entry(id).or_insert_with(|| name.clone());
            let r = crate::props::row(ui, "Name", |ui| {
                crate::props::fill(
                    ui,
                    egui::TextEdit::singleline(draft)
                        .vertical_align(egui::Align::Center)
                        .id(master_name_id(id)),
                )
            });
            if r.lost_focus() && !draft.trim().is_empty() && draft.trim() != name {
                let new = draft.trim().to_string();
                let res = self.doc.edit(id, "Rename master", |e| e.name = new.clone());
                self.report(res, format!("Renamed {new}."));
            } else if !r.has_focus() && *draft != name {
                *draft = name.clone();
            }
            let size = t.paper.unwrap_or(DVec2::new(420.0, 297.0));
            let now = PAPERS
                .iter()
                .find(|(_, w, h)| {
                    (size - DVec2::new(*w, *h)).length() < 0.5
                        || (size - DVec2::new(*h, *w)).length() < 0.5
                })
                .map_or("Custom", |p| p.0);
            let portrait = size.y > size.x;
            let mut pick: Option<DVec2> = None;
            crate::props::row(ui, "Paper", |ui| {
                egui::ComboBox::from_id_salt(("master.paper", id))
                    .icon(crate::props::combo_icon)
                    .width(ui.available_width())
                    .selected_text(format!("{now}{}", if portrait { ", portrait" } else { "" }))
                    .show_ui(ui, |ui| {
                        for (n, w, h) in PAPERS {
                            if ui.selectable_label(n == now && !portrait, n).clicked() {
                                pick = Some(DVec2::new(w, h));
                            }
                            if ui
                                .selectable_label(n == now && portrait, format!("{n}, portrait"))
                                .clicked()
                            {
                                pick = Some(DVec2::new(h, w));
                            }
                        }
                    });
            });
            if let Some(p) = pick {
                self.edit_master("Master paper", |t| t.paper = Some(p));
                self.masters.camera = None;
            }
            let mut corner = t.corner;
            crate::props::row(ui, "Measured from", |ui| {
                egui::ComboBox::from_id_salt(("master.corner", id))
                    .icon(crate::props::combo_icon)
                    .width(ui.available_width())
                    .selected_text(corner.label())
                    .show_ui(ui, |ui| {
                        for c in Corner::ALL {
                            ui.selectable_value(&mut corner, c, c.label());
                        }
                    })
            })
            .response
            .on_hover_text(
                "The sheet corner its drawing is measured from: it stays there on any paper",
            );
            if corner != t.corner {
                // the drawing stays where it is on the sheet
                let d = t.corner.at(size.x, size.y) - corner.at(size.x, size.y);
                self.edit_master("Master corner", |t| {
                    t.corner = corner;
                    t.items.iter_mut().for_each(|it| it.shift(d));
                    t.guides.iter_mut().for_each(|(a, b)| {
                        *a += d;
                        *b += d;
                    });
                });
            }
            let mut border = t.border;
            if crate::props::check(ui, "Border", &mut border, "Drawn").changed() {
                self.edit_master("Master border", |t| t.border = border);
            }
            crate::props::row(ui, "Sheets", |ui| {
                ui.label(match users.len() {
                    0 => "None on it yet".to_string(),
                    1 => "1 sheet".to_string(),
                    n => format!("{n} sheets"),
                })
            });
            let all_sheets: Vec<ElementId> = creabim_geom::sheet::sheets(self.doc.model())
                .into_iter()
                .map(|(i, _)| i)
                .collect();
            let new_sheet = crate::props::action(
                ui,
                "",
                "New sheet on it",
                "A new sheet using this master",
                true,
            )
            .clicked();
            let every = crate::props::action(
                ui,
                "",
                "Use on every sheet",
                "Every sheet of the project takes this master (one undo step)",
                !all_sheets.is_empty() && users.len() < all_sheets.len(),
            )
            .clicked();
            if new_sheet {
                self.new_sheet_from(id);
            }
            if every {
                self.set_sheet_template(&all_sheets, Some(id));
            }
        }
        // its own fields: what each sheet can fill in beyond the built-in
        // ones, and what shows until it does
        if self.prop_header(ui, "master_fields", "Its own fields", true, None) {
            let mut fields = t.fields.clone();
            let mut changed = false;
            let mut remove = None;
            for (k, (f, v)) in fields.iter_mut().enumerate() {
                crate::props::row(ui, &format!("{{{f}}}"), |ui| {
                    ui.horizontal(|ui| {
                        let w = ui.available_width() - 26.0;
                        // a draft that outlives the frame, as the name's
                        let draft = self
                            .masters
                            .field_drafts
                            .entry((id, f.clone()))
                            .or_insert_with(|| v.clone());
                        let r = ui.add(
                            egui::TextEdit::singleline(draft)
                                .vertical_align(egui::Align::Center)
                                .desired_width(w)
                                .hint_text("Until a sheet fills it")
                                .id(master_field_id(id, f)),
                        );
                        if r.lost_focus() && *draft != *v {
                            *v = draft.clone();
                            changed = true;
                        } else if !r.has_focus() && *draft != *v {
                            *draft = v.clone();
                        }
                        if crate::props::icon_button(ui, "trash-2", "Remove this field").clicked() {
                            remove = Some(k);
                        }
                    });
                });
            }
            if let Some(k) = remove {
                fields.remove(k);
                changed = true;
            }
            let key = egui::Id::new(("master.newfield", id));
            let mut new = ui
                .ctx()
                .data(|d| d.get_temp::<String>(key))
                .unwrap_or_default();
            let add = crate::props::row(ui, "New field", |ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut new)
                        .vertical_align(egui::Align::Center)
                        .desired_width(ui.available_width())
                        .hint_text("client, drawn by, revision…"),
                );
                r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
            });
            let clean: String = new
                .trim()
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect();
            if add && !clean.is_empty() && !fields.iter().any(|(f, _)| *f == clean) {
                fields.push((clean.clone(), String::new()));
                changed = true;
                new.clear();
                self.masters.field = clean.clone();
                self.status = format!("{{{clean}}} added: the Field tool places it now.");
            }
            ui.ctx().data_mut(|d| d.insert_temp(key, new));
            if changed {
                self.edit_master("Master fields", |t| t.fields = fields);
            }
            crate::props::note(
                ui,
                "Every sheet fills in {sheet}, {number}, {project}, {scale} and {date} itself.",
            );
        }
        // the guides it is drawn by
        if self.prop_header(ui, "master_guides", "Guides", true, None) {
            let n = t.guides.len();
            crate::props::row(ui, "Guides", |ui| {
                ui.label(match n {
                    0 => "None yet".to_string(),
                    1 => "1 guide".to_string(),
                    n => format!("{n} guides"),
                })
            });
            let mut show = !self.masters.hide_guides;
            if crate::props::check(ui, "Show", &mut show, "On").changed() {
                self.masters.hide_guides = !show;
            }
            let add =
                crate::props::action(ui, "", "Draw a guide", "The Guide tool: two clicks", true)
                    .clicked();
            let clear = crate::props::action(
                ui,
                "",
                "Clear all",
                "Take every guide away (one undo step)",
                n > 0,
            )
            .clicked();
            if add {
                self.masters.tool = MTool::Guide;
                self.masters.guide_from = None;
                self.status = "Guide: click two points it runs through.".into();
            }
            if clear {
                self.edit_master("Clear guides", |t| t.guides.clear());
                self.masters.guide_sel = None;
            }
            crate::props::note(
                ui,
                "Guides are kept with the master and never printed. Pick one to drag it; Delete removes it.",
            );
        }
        // its pictures and logos, each to pick (MU40: one place for them)
        let pics: Vec<(usize, String)> = t
            .items
            .iter()
            .enumerate()
            .filter_map(|(k, it)| match it {
                TitleItem::Image { lo, hi, .. } => Some((
                    k,
                    format!("Picture, {:.0} × {:.0} mm", hi.x - lo.x, hi.y - lo.y),
                )),
                TitleItem::Logo { lo, hi, .. } => Some((
                    k,
                    format!("Logo, {:.0} × {:.0} mm", hi.x - lo.x, hi.y - lo.y),
                )),
                _ => None,
            })
            .collect();
        if self.prop_header(ui, "master_pictures", "Pictures and logos", true, None) {
            for (k, label) in &pics {
                let on = self.masters.sel == Some(*k);
                if crate::props::row(ui, "", |ui| {
                    ui.add_sized(
                        [ui.available_width(), ui.spacing().interact_size.y],
                        egui::Button::selectable(on, label.as_str()),
                    )
                })
                .clicked()
                {
                    self.masters.sel = Some(*k);
                    self.masters.border_sel = false;
                    self.masters.guide_sel = None;
                }
            }
            if crate::props::action(
                ui,
                "",
                "Add a logo or picture…",
                "An SVG comes in as a crisp vector logo, a PNG or JPEG as a picture",
                true,
            )
            .clicked()
            {
                self.add_master_picture();
            }
        }
        // the border picked: its margins to type
        if self.masters.border_sel
            && self.prop_header(ui, "master_border", "Picked border", true, None)
        {
            let paper = t.paper.unwrap_or(DVec2::new(420.0, 297.0));
            let mut m = t.margins.unwrap_or(tb::MARGINS);
            let mut changed = false;
            for (k, name) in ["Left", "Bottom", "Right", "Top"].into_iter().enumerate() {
                let max = if k % 2 == 0 { paper.x } else { paper.y } / 2.0;
                changed |= crate::props::row(ui, name, |ui| {
                    crate::props::fill(
                        ui,
                        egui::DragValue::new(&mut m[k])
                            .range(0.0..=max)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                })
                .changed();
            }
            if changed {
                self.edit_master("Border margins", |t| t.margins = Some(m));
            }
            if crate::props::action(
                ui,
                "",
                "Usual margins",
                "20 mm at the binding side, 10 mm the others",
                t.margins.is_some_and(|x| x != tb::MARGINS),
            )
            .clicked()
            {
                self.edit_master("Border margins", |t| t.margins = None);
            }
        }
        // what is picked
        let Some(i) = self.masters.sel.filter(|i| *i < t.items.len()) else {
            return;
        };
        let it = t.items[i].clone();
        let title = match &it {
            TitleItem::Text { text, .. } if text.contains('{') => "Picked field",
            TitleItem::Text { .. } => "Picked text",
            TitleItem::Line { .. } => "Picked line",
            TitleItem::Fill { .. } => "Picked fill",
            TitleItem::Shape { closed: true, .. } => "Picked shape",
            TitleItem::Shape { .. } => "Picked line",
            TitleItem::Image { .. } => "Picked picture",
            TitleItem::Logo { .. } => "Picked logo",
        };
        if !self.prop_header(ui, "master_picked", title, true, None) {
            return;
        }
        let mut edited = it.clone();
        let mut label = None;
        let colour_row = |ui: &mut egui::Ui, name: &str, c: &mut [f32; 3]| -> bool {
            crate::props::row(ui, name, |ui| ui.color_edit_button_rgb(c)).changed()
        };
        match &mut edited {
            TitleItem::Text {
                at,
                size,
                text,
                align,
                angle,
            } => {
                // a draft that outlives the frame: `text` is remade from
                // the master each frame and lost what was typed
                let draft = self
                    .masters
                    .text_drafts
                    .entry((id, i))
                    .or_insert_with(|| text.clone());
                let r = crate::props::row(ui, "Words", |ui| {
                    crate::props::fill(
                        ui,
                        egui::TextEdit::singleline(draft)
                            .vertical_align(egui::Align::Center)
                            .id(master_text_id(id, i)),
                    )
                });
                if r.lost_focus() && *draft != *text {
                    *text = draft.clone();
                    label = Some("Master text");
                } else if !r.has_focus() && *draft != *text {
                    *draft = text.clone();
                }
                if crate::props::row(ui, "Size", |ui| {
                    crate::props::fill(
                        ui,
                        egui::DragValue::new(size)
                            .range(0.5..=60.0)
                            .speed(0.1)
                            .suffix(" mm"),
                    )
                })
                .changed()
                {
                    label = Some("Text size");
                }
                if crate::props::choice(
                    ui,
                    "Align",
                    align,
                    &[
                        (TextAlign::Start, "Left", "Text runs right from its point"),
                        (TextAlign::Middle, "Centre", "Text centred on its point"),
                        (TextAlign::End, "Right", "Text ends at its point"),
                    ],
                ) {
                    label = Some("Text alignment");
                }
                let mut deg = angle.to_degrees();
                if crate::props::row(ui, "Turned", |ui| {
                    crate::props::fill(ui, egui::DragValue::new(&mut deg).speed(1.0).suffix("°"))
                })
                .changed()
                {
                    *angle = deg.to_radians();
                    label = Some("Text angle");
                }
                let (mut x, mut y) = (at.x, at.y);
                let cx = crate::props::row(ui, "X", |ui| {
                    crate::props::fill(ui, egui::DragValue::new(&mut x).speed(0.5).suffix(" mm"))
                });
                let cy = crate::props::row(ui, "Y", |ui| {
                    crate::props::fill(ui, egui::DragValue::new(&mut y).speed(0.5).suffix(" mm"))
                });
                if cx.changed() || cy.changed() {
                    *at = DVec2::new(x, y);
                    label = Some("Move text");
                }
            }
            TitleItem::Line { points, width } => {
                if crate::props::row(ui, "Pen", |ui| {
                    crate::props::fill(
                        ui,
                        egui::DragValue::new(width)
                            .range(0.05..=3.0)
                            .speed(0.01)
                            .suffix(" mm"),
                    )
                })
                .changed()
                {
                    label = Some("Line pen");
                }
                let len: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
                crate::props::row(ui, "Length", |ui| ui.label(format!("{len:.1} mm")));
                crate::props::note(ui, "Brought in: a line drawn here takes colours too.");
            }
            TitleItem::Fill { gray, .. } => {
                let mut g = *gray * 100.0;
                if crate::props::row(ui, "Grey", |ui| {
                    crate::props::fill(
                        ui,
                        egui::DragValue::new(&mut g).range(0.0..=100.0).suffix(" %"),
                    )
                })
                .changed()
                {
                    *gray = g / 100.0;
                    label = Some("Fill grey");
                }
            }
            TitleItem::Shape {
                closed,
                pen,
                colour,
                fill,
                opacity,
                ..
            } => {
                // the pen: on or off, its width and colour
                let mut has_pen = pen.is_some();
                if crate::props::check(ui, "Outline", &mut has_pen, "Drawn").changed() {
                    *pen = has_pen.then_some(0.25);
                    label = Some("Outline");
                }
                if let Some(w) = pen {
                    if crate::props::row(ui, "Pen", |ui| {
                        crate::props::fill(
                            ui,
                            egui::DragValue::new(w)
                                .range(0.05..=5.0)
                                .speed(0.01)
                                .suffix(" mm"),
                        )
                    })
                    .changed()
                    {
                        label = Some("Pen width");
                    }
                    if colour_row(ui, "Pen colour", colour) {
                        label = Some("Pen colour");
                    }
                    if dark && colour.iter().all(|v| *v < 0.12) {
                        crate::props::note(
                            ui,
                            "Prints black. On dark paper black shows light, so you can see it as you work.",
                        );
                    }
                }
                // the fill: closed shapes only
                if *closed {
                    let mut has_fill = fill.is_some();
                    if crate::props::check(ui, "Fill", &mut has_fill, "Filled").changed() {
                        *fill = has_fill.then_some([0.85, 0.85, 0.85]);
                        label = Some("Fill");
                    }
                    if let Some(f) = fill {
                        if colour_row(ui, "Fill colour", f) {
                            label = Some("Fill colour");
                        }
                    }
                } else if crate::props::action(
                    ui,
                    "",
                    "Close it",
                    "Join its last corner to its first, so it can be filled",
                    true,
                )
                .clicked()
                {
                    *closed = true;
                    label = Some("Close shape");
                }
                let mut pct = *opacity * 100.0;
                if crate::props::row(ui, "Opacity", |ui| {
                    // the slider and its number inside the field column
                    ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(40.0);
                    ui.add(
                        egui::Slider::new(&mut pct, 0.0..=100.0)
                            .suffix(" %")
                            .max_decimals(0),
                    )
                })
                .changed()
                {
                    *opacity = (pct / 100.0).clamp(0.0, 1.0);
                    label = Some("Opacity");
                }
            }
            TitleItem::Logo { lo, hi, shapes } => {
                // its size, its proportions kept as either changes
                let (mut w, mut h) = (hi.x - lo.x, hi.y - lo.y);
                let aspect = h / w.max(1e-9);
                let cw = crate::props::row(ui, "Width", |ui| {
                    crate::props::fill(
                        ui,
                        egui::DragValue::new(&mut w)
                            .range(1.0..=1200.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                });
                let ch = crate::props::row(ui, "Height", |ui| {
                    crate::props::fill(
                        ui,
                        egui::DragValue::new(&mut h)
                            .range(1.0..=1200.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                });
                if cw.changed() {
                    *hi = *lo + DVec2::new(w, w * aspect);
                    label = Some("Logo size");
                } else if ch.changed() {
                    *hi = *lo + DVec2::new(h / aspect, h);
                    label = Some("Logo size");
                }
                crate::props::row(ui, "Shapes", |ui| ui.label(format!("{}", shapes.len())));
                // one colour for all of it: a logo in the title block's ink
                let mut one = match shapes.first() {
                    Some(TitleItem::Shape { fill: Some(c), .. })
                    | Some(TitleItem::Shape { colour: c, .. }) => *c,
                    _ => [0.0; 3],
                };
                if colour_row(ui, "One colour", &mut one) {
                    for sh in shapes.iter_mut() {
                        if let TitleItem::Shape { colour, fill, .. } = sh {
                            *colour = one;
                            if fill.is_some() {
                                *fill = Some(one);
                            }
                        }
                    }
                    label = Some("Logo colour");
                }
                crate::props::note(
                    ui,
                    "A vector logo: crisp on sheets and in PDF at any size. Its corners size it.",
                );
            }
            TitleItem::Image {
                lo,
                hi,
                width,
                height,
                opacity,
                ..
            } => {
                let mut pct = *opacity * 100.0;
                if crate::props::row(ui, "Opacity", |ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(40.0);
                    ui.add(
                        egui::Slider::new(&mut pct, 0.0..=100.0)
                            .suffix(" %")
                            .max_decimals(0),
                    )
                })
                .changed()
                {
                    *opacity = (pct / 100.0).clamp(0.0, 1.0);
                    label = Some("Picture opacity");
                }
                let (mut w, mut h) = (hi.x - lo.x, hi.y - lo.y);
                let aspect = *height as f64 / (*width).max(1) as f64;
                let cw = crate::props::row(ui, "Width", |ui| {
                    crate::props::fill(
                        ui,
                        egui::DragValue::new(&mut w)
                            .range(1.0..=1200.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                });
                let ch = crate::props::row(ui, "Height", |ui| {
                    crate::props::fill(
                        ui,
                        egui::DragValue::new(&mut h)
                            .range(1.0..=1200.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                });
                // its own proportions, kept as either changes
                if cw.changed() {
                    *hi = *lo + DVec2::new(w, w * aspect);
                    label = Some("Picture size");
                } else if ch.changed() {
                    *hi = *lo + DVec2::new(h / aspect, h);
                    label = Some("Picture size");
                }
                crate::props::row(ui, "Pixels", |ui| ui.label(format!("{width} × {height}")));
                if crate::props::action(
                    ui,
                    "",
                    "Its own proportions",
                    "Undo any stretching: its height from its width",
                    ((h / w) - aspect).abs() > 1e-3,
                )
                .clicked()
                {
                    *hi = *lo + DVec2::new(w, w * aspect);
                    label = Some("Picture proportions");
                }
            }
        }
        // what lies over what: drawn in order, the last on top (MU40)
        let last = t.items.len().saturating_sub(1);
        let (front, back) = crate::props::actions2(
            ui,
            "Order",
            ("To front", "Over everything else (Ctrl+])", i < last),
            ("To back", "Under everything else (Ctrl+[)", i > 0),
        );
        if front || back {
            self.master_order(i, front);
            return;
        }
        let dup = crate::props::action(ui, "", "Duplicate", "A copy, 5 mm along (Ctrl+D)", true)
            .clicked();
        let del = crate::props::action(ui, "", "Delete", "Take it off the master (Delete)", true)
            .clicked();
        if dup {
            self.masters_command(Command::Duplicate);
            return;
        }
        if del {
            self.masters_command(Command::Delete);
            return;
        }
        if let Some(l) = label {
            if edited != it {
                self.edit_master(l, |t| {
                    if let Some(x) = t.items.get_mut(i) {
                        *x = edited;
                    }
                });
            }
        }
    }
}

/// A sheet picture from file `p`, `w` mm wide: an SVG as vector shapes,
/// a PNG or JPEG as pixels (at most 2400 across: sharp at A3, light).
fn sheet_picture_kind(p: &std::path::Path, w: f64) -> Result<creabim_core::PaperMarkKind, String> {
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
        let (shapes, aspect) = creabim_exchange::titleblock::svg_logo(&text)?;
        return Ok(creabim_core::PaperMarkKind::Vector {
            w,
            h: w / aspect.max(1e-6),
            shapes,
        });
    }
    let img = image::open(p).map_err(|e| e.to_string())?;
    let img = if img.width().max(img.height()) > 2400 {
        img.resize(2400, 2400, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (pw, ph) = (rgba.width(), rgba.height());
    Ok(creabim_core::PaperMarkKind::Picture {
        w,
        h: w * ph as f64 / pw.max(1) as f64,
        width: pw,
        height: ph,
        rgba: creabim_geom::raster::store_image(rgba.as_raw()),
        opacity: 1.0,
    })
}

/// A picked master text's Words field in Properties.
fn master_text_id(id: ElementId, item: usize) -> egui::Id {
    egui::Id::new(("master-text", id, item))
}

/// The value field of a master's own field in Properties.
fn master_field_id(id: ElementId, field: &str) -> egui::Id {
    egui::Id::new(("master-own-field", id, field))
}

/// The master name field's id in Properties.
fn master_name_id(id: ElementId) -> egui::Id {
    egui::Id::new(("master-name", id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames of the master's Properties, typing `words` into the field
    /// `field` and then leaving it.
    fn type_into(app: &mut CreaBimApp, field: egui::Id, words: &[&str]) {
        let ctx = egui::Context::default();
        let frame = |app: &mut CreaBimApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.master_props(ui);
            });
            out.textures_delta.clear();
        };
        frame(app, vec![]);
        ctx.memory_mut(|m| m.request_focus(field));
        frame(app, vec![]);
        for w in words {
            frame(app, vec![egui::Event::Text((*w).into())]);
        }
        ctx.memory_mut(|m| m.surrender_focus(field));
        frame(app, vec![]);
    }

    #[test]
    fn a_picked_text_and_a_field_of_its_own_take_what_is_typed() {
        // both were remade from the saved master every frame: what was
        // typed flickered and was gone
        let mut app = crate::app::test_app();
        app.open_master(None);
        app.add_master("Studio".into(), tb::minimal_band());
        let (id, _, t) = app.master().expect("a master");
        let i = t
            .items
            .iter()
            .position(|it| matches!(it, TitleItem::Text { .. }))
            .expect("a text in the master");
        app.masters.sel = Some(i);
        type_into(&mut app, master_text_id(id, i), &["X", "Y"]);
        let (_, _, t) = app.master().unwrap();
        let TitleItem::Text { text, .. } = &t.items[i] else {
            unreachable!()
        };
        assert!(text.contains("XY"), "{text}");
        // a field of its own
        app.edit_master("Master field", |t| {
            t.fields.push(("client".into(), String::new()))
        });
        type_into(&mut app, master_field_id(id, "client"), &["A", "B"]);
        let (_, _, t) = app.master().unwrap();
        let v = &t.fields.iter().find(|(f, _)| f == "client").unwrap().1;
        assert_eq!(v, "AB");
    }

    #[test]
    fn a_turned_text_is_boxed_and_picked_along_its_letters() {
        // a vertical text kept the box of a level one: picked and outlined
        // across, beside its letters
        let text = TitleItem::Text {
            at: DVec2::ZERO,
            size: 3.0,
            text: "ABCDEFGHIJ".into(),
            align: TextAlign::Start,
            angle: std::f64::consts::FRAC_PI_2,
        };
        // it runs up from its point
        assert_eq!(item_dist(&text, DVec2::new(0.0, 10.0)), 0.0);
        assert!(item_dist(&text, DVec2::new(10.0, 0.0)) > 4.0);
        let (lo, hi) = item_box(&text);
        assert!(hi.y - lo.y > 10.0, "tall: {lo} {hi}");
        assert!(hi.x - lo.x < 5.0, "narrow: {lo} {hi}");
    }

    #[test]
    fn a_master_is_renamed_by_typing_in_its_name_field() {
        // the field was remade from the saved name every frame: what was
        // typed flickered and was gone, and the name never changed
        let mut app = crate::app::test_app();
        app.open_master(None);
        app.add_master("Studio".into(), tb::minimal_band());
        let (id, name, _) = app.master().expect("a master to rename");
        let ctx = egui::Context::default();
        let frame = |app: &mut CreaBimApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.master_props(ui);
            });
            out.textures_delta.clear();
        };
        frame(&mut app, vec![]);
        ctx.memory_mut(|m| m.request_focus(master_name_id(id)));
        frame(&mut app, vec![]);
        // typed over two frames
        frame(&mut app, vec![egui::Event::Text("X".into())]);
        frame(&mut app, vec![egui::Event::Text("Y".into())]);
        // and left: the name is saved
        ctx.memory_mut(|m| m.surrender_focus(master_name_id(id)));
        frame(&mut app, vec![]);
        let now = app.doc.model().get(id).unwrap().name.clone();
        assert_ne!(now, name, "the master was renamed");
        assert!(now.contains("XY"), "{now}");
    }

    #[test]
    fn a_master_picked_in_the_browser_brings_its_view_to_the_front() {
        // with the Masters view beside a plan that had the focus, Properties
        // kept showing what was picked before (a wall type)
        use crate::areas::{Node, Shows};
        let mut app = crate::app::test_app();
        app.areas.root = Node::split(
            false,
            0.5,
            Node::Area(Shows::Model),
            Node::Area(Shows::Masters),
        );
        app.areas.focus = 0;
        app.open_master(None);
        assert_eq!(app.areas.focus, 1, "the Masters view is in front");
        assert!(app.prop_tabs().contains(&PropTab::Master));
        // and the master shows, though a wall is still picked
        let wall = app
            .doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, creabim_core::ElementKind::Wall(_)))
            .map(|e| e.id);
        app.selection.set(wall);
        assert!(app.selection.active.is_some());
        assert!(PropTab::Master.of_its_view());
        assert_eq!(
            app.areas.root.areas(),
            [Shows::Model, Shows::Masters],
            "the plan stays as it was"
        );
    }

    #[test]
    fn text_shapes_and_pictures_are_picked_near_or_in_them() {
        let line = TitleItem::Line {
            points: vec![DVec2::ZERO, DVec2::new(10.0, 0.0)],
            width: 0.25,
        };
        assert!(item_dist(&line, DVec2::new(5.0, 0.5)) < 0.6);
        assert!(item_dist(&line, DVec2::new(5.0, 5.0)) > 4.0);
        let text = TitleItem::Text {
            at: DVec2::new(0.0, 0.0),
            size: 3.0,
            text: "{project}".into(),
            align: TextAlign::End,
            angle: 0.0,
        };
        // right-aligned: it runs leftward from its point
        assert_eq!(item_dist(&text, DVec2::new(-5.0, 0.0)), 0.0);
        assert!(item_dist(&text, DVec2::new(5.0, 0.0)) > 4.0);
        // a filled square is picked inside, an empty one only at its edge
        let sq = vec![
            DVec2::ZERO,
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 10.0),
            DVec2::new(0.0, 10.0),
        ];
        let mut shape = TitleItem::Shape {
            points: sq,
            closed: true,
            pen: Some(0.25),
            colour: [0.0; 3],
            fill: None,
            opacity: 1.0,
        };
        assert!(item_dist(&shape, DVec2::new(5.0, 5.0)) > 4.0);
        assert!(item_dist(&shape, DVec2::new(0.0, 5.0)) < 1e-9);
        if let TitleItem::Shape { fill, .. } = &mut shape {
            *fill = Some([1.0, 0.0, 0.0]);
        }
        assert_eq!(item_dist(&shape, DVec2::new(5.0, 5.0)), 0.0);
        let pic = TitleItem::Image {
            lo: DVec2::ZERO,
            hi: DVec2::new(40.0, 20.0),
            width: 2,
            height: 1,
            rgba: String::new(),
            opacity: 1.0,
        };
        assert_eq!(item_dist(&pic, DVec2::new(20.0, 10.0)), 0.0);
    }

    #[test]
    fn a_corner_drag_scales_about_the_far_corner_and_quarter_turns_keep_the_middle() {
        let mut pic = TitleItem::Image {
            lo: DVec2::ZERO,
            hi: DVec2::new(40.0, 20.0),
            width: 2,
            height: 1,
            rgba: String::new(),
            opacity: 1.0,
        };
        // the top right corner dragged to (80, 40): twice the size
        scale_item(&mut pic, DVec2::ZERO, DVec2::splat(2.0));
        assert_eq!(item_box(&pic), (DVec2::ZERO, DVec2::new(80.0, 40.0)));
        let mut text = TitleItem::Text {
            at: DVec2::new(10.0, 10.0),
            size: 3.0,
            text: "A".into(),
            align: TextAlign::Start,
            angle: 0.0,
        };
        scale_item(&mut text, DVec2::ZERO, DVec2::splat(2.0));
        assert!(matches!(text, TitleItem::Text { size, .. } if (size - 6.0).abs() < 1e-9));
        let mut line = TitleItem::Shape {
            points: vec![DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0)],
            closed: false,
            pen: Some(0.25),
            colour: [0.0; 3],
            fill: None,
            opacity: 1.0,
        };
        quarter_turn(&mut line);
        let (lo, hi) = item_box(&line);
        assert!(
            (lo - DVec2::new(5.0, -5.0)).length() < 1e-9
                && (hi - DVec2::new(5.0, 5.0)).length() < 1e-9
        );
    }

    #[test]
    fn guides_cross_take_points_onto_them_and_go_straight_when_nearly_so() {
        let x = cross(
            DVec2::ZERO,
            DVec2::new(10.0, 0.0),
            DVec2::new(3.0, -5.0),
            DVec2::new(3.0, 5.0),
        );
        assert_eq!(x, Some(DVec2::new(3.0, 0.0)));
        assert!(cross(DVec2::ZERO, DVec2::X, DVec2::Y, DVec2::new(1.0, 1.0)).is_none());
        let (q, d) = onto(DVec2::new(4.0, 2.0), DVec2::ZERO, DVec2::new(10.0, 0.0));
        assert_eq!((q, d), (DVec2::new(4.0, 0.0), 2.0));
        // a hair off level is level; a real slope stays
        assert_eq!(
            straighten(DVec2::ZERO, DVec2::new(100.0, 2.0)),
            DVec2::new(100.0, 0.0)
        );
        assert_eq!(
            straighten(DVec2::ZERO, DVec2::new(1.0, 80.0)),
            DVec2::new(0.0, 80.0)
        );
        assert_eq!(
            straighten(DVec2::ZERO, DVec2::new(10.0, 10.0)),
            DVec2::new(10.0, 10.0)
        );
    }
}
