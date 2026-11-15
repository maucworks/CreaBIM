//! Drag handles on a selected element, defined once in model space and
//! shown in every pane that can use them: the 3D view, plans and sections.
//! Each handle slides along a line or across a plane; the pane turns the
//! pointer into a ray (a perspective ray in 3D, a straight-on one in 2D) and
//! the handle's constraint picks the point on it.
//!
//! Walls stretch by their ends (joined walls follow), grow taller by their
//! top and thicker by their face. Slabs move corners and edges, and get
//! thicker by their underside (the top stays) or by their top (the
//! underside stays). Doors and windows widen from either jamb and
//! change height and sill. A selected face of a mass pushes and pulls along
//! its normal. Shafts move their corners, base and top.
//!
//! Every outline (a slab's, a roof's, a shaft's, a sketch's, a railing's
//! path) has the same grips, from [`crate::editmode::outline_handles`]:
//! its corners and edges when selected, and in Edit mode its holes and
//! add-corner grips too, with corners to pick and remove (see
//! [`GripSel`]).

use crate::modal::LiveEdit;
use crate::snaps::{Snap, SnapKind};
use creabim_core::{
    transform, walls, DetailKind, Document, Element, ElementId, ElementKind, Model, OpeningKind,
    ViewKind, WallTop, Xform,
};
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, Vec2};
use glam::{DVec2, DVec3};

/// A pointer ray in model space.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub o: DVec3,
    pub d: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Constraint {
    /// Anywhere on the plane through `point` with `normal`.
    Plane { point: DVec3, normal: DVec3 },
    /// Along the line through `point` in `dir` (a unit vector).
    Line { point: DVec3, dir: DVec3 },
}

impl Constraint {
    /// The point on the constraint the ray points at.
    pub fn hit(&self, r: Ray) -> Option<DVec3> {
        match *self {
            Constraint::Plane { point, normal } => {
                let den = r.d.dot(normal);
                if den.abs() < 1e-9 {
                    return None;
                }
                let t = (point - r.o).dot(normal) / den;
                Some(r.o + r.d * t)
            }
            Constraint::Line { point, dir } => {
                // closest point on the line to the ray
                let w = point - r.o;
                let (a, b, c) = (dir.dot(dir), dir.dot(r.d), r.d.dot(r.d));
                let (d, e) = (dir.dot(w), r.d.dot(w));
                let den = a * c - b * b;
                if den.abs() < 1e-12 {
                    return None;
                }
                let s = (b * e - c * d) / den;
                Some(point + dir * s)
            }
        }
    }

    /// Can this be dragged in a pane looking along `look`? A line seen end
    /// on can't.
    pub fn usable(&self, look: DVec3) -> bool {
        match *self {
            Constraint::Line { dir, .. } => dir.dot(look).abs() < 0.97,
            Constraint::Plane { normal, .. } => normal.dot(look).abs() > 0.2,
        }
    }
}

/// Which outline of an element an Edit-mode grip works on: its own, or
/// hole `k` of a slab or a roof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ring {
    Outline,
    Hole(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandleKind {
    WallEnd {
        end: bool,
    },
    WallTop,
    WallFace,
    SlabUnderside,
    SlabTop,
    OpeningJamb {
        right: bool,
    },
    OpeningHead,
    OpeningSill,
    GridEnd {
        end: bool,
    },
    /// Push or pull a face of a mass along its normal.
    Face(usize),
    /// Corner `i` of a mass, moved across the plan (A7-9).
    MeshVertex(usize),
    /// The edge between corners `a` and `b` of a mass, moved across the
    /// plan by its middle.
    MeshEdge(u32, u32),
    /// A whole mass, by its base point: across the plan.
    MeshMove,
    /// A whole mass, up or down.
    MeshLift,
    /// Grip `i` of a column, beam, roof, stair or railing; `free` ones
    /// move across the plan, others along a line.
    Member {
        i: usize,
        free: bool,
    },
    /// An end of a section or elevation line.
    ViewEnd {
        end: bool,
    },
    /// How far a section or elevation sees, from its line.
    ViewDepth,
    /// Face `i` of a crop box (see [`creabim_core::CropBox::face`]).
    CropFace(usize),
    /// A crop box as a whole, across the plan.
    CropMove,
    /// A reference as a whole, by its middle (Trace & Reference v2).
    RefMove,
    /// Corner `i` of a reference's outline: scales it about its middle.
    RefScale(usize),
    /// Corner `i` of an outline: a slab's, a roof's, a shaft's, a
    /// sketch's, a railing's path; in Edit mode a hole's too.
    Vertex {
        ring: Ring,
        i: usize,
    },
    /// Edge `i` of a closed outline, pushed out or pulled in square to
    /// itself.
    Edge {
        ring: Ring,
        i: usize,
    },
    /// In Edit mode, the middle of edge `i`: dragging it pulls a new
    /// corner out of the edge (a click adds nothing, so a double-click
    /// can't add a corner and take it away again).
    AddVertex {
        ring: Ring,
        i: usize,
    },
    /// In Edit mode, a whole outline moved: a hole dragged from inside.
    Ring {
        ring: Ring,
    },
    /// A shaft's base and its top, up and down.
    ShaftBase,
    ShaftTop,
    /// Turn an object, a column or a stair about its own point.
    Rotate,
    /// A rectangular column's width (along its own x) or depth; a round
    /// one's diameter.
    ColumnSize {
        depth: bool,
    },
    /// Slide a whole wall square to itself; the walls joined to it stretch.
    WallSlide,
    /// The middle of a curved wall: drag it to bend the wall more or less,
    /// its ends staying put (it passes through the point dragged to).
    WallBend,
    /// Slide a door or window along its wall at the same width.
    OpeningSlide,
    /// A beam's width, both sides at once.
    BeamWidth,
    /// A detail component's width across its path, both sides at once.
    DetailWidth,
    /// A repeating detail's unit length, at the end of its first unit.
    DetailSpacing,
    /// A plant's canopy, at the edge of its crown.
    PlantCanopy,
    /// A plant's height, at its top (in 3D and sections).
    PlantHeight,
    /// Corner `i` of a callout's box on its parent view (lo, then
    /// counter-clockwise); the opposite corner stays.
    CalloutCorner(usize),
    /// Edge `i` of a callout's box (bottom, right, top, left), pushed out
    /// or pulled in.
    CalloutEdge(usize),
    /// In Edit mode's Layers (Tab), the start or end of layer `layer` of
    /// a layered wall, run on past its join or held back.
    LayerEnd {
        layer: usize,
        end: bool,
    },
    /// In Layers, the bottom (or `top`) of layer `layer` of a layered
    /// wall, run down past the wall's base or up past its top (A7-10).
    LayerHeight {
        layer: usize,
        top: bool,
    },
    /// In Layers, edge `edge` of layer `layer` of a layered slab or roof,
    /// pushed out past the element's edge or pulled in.
    LayerEdge {
        layer: usize,
        edge: usize,
    },
}

impl HandleKind {
    /// Round handles slide along a line; square ones move freely.
    pub fn round(self) -> bool {
        !matches!(
            self,
            HandleKind::WallEnd { .. }
                | HandleKind::GridEnd { .. }
                | HandleKind::Member { free: true, .. }
                | HandleKind::ViewEnd { .. }
                | HandleKind::CropMove
                | HandleKind::RefMove
                | HandleKind::RefScale(_)
                | HandleKind::Vertex { .. }
                | HandleKind::AddVertex { .. }
                | HandleKind::CalloutCorner(_)
                | HandleKind::MeshVertex(_)
                | HandleKind::MeshEdge(..)
                | HandleKind::MeshMove
        )
    }

    pub fn verb(self) -> &'static str {
        match self {
            HandleKind::WallTop
            | HandleKind::OpeningHead
            | HandleKind::ShaftBase
            | HandleKind::ShaftTop => "Change height of",
            HandleKind::Vertex { .. }
            | HandleKind::Edge { .. }
            | HandleKind::AddVertex { .. }
            | HandleKind::Ring { .. } => "Reshape",
            HandleKind::Rotate => "Rotate",
            HandleKind::ColumnSize { .. } => "Resize",
            HandleKind::WallSlide | HandleKind::OpeningSlide => "Slide",
            HandleKind::WallBend => "Bend",
            HandleKind::BeamWidth | HandleKind::DetailWidth => "Change width of",
            HandleKind::DetailSpacing => "Change the unit of",
            HandleKind::PlantCanopy => "Change the canopy of",
            HandleKind::PlantHeight => "Change height of",
            HandleKind::CalloutCorner(_) | HandleKind::CalloutEdge(_) => "Resize the callout of",
            HandleKind::LayerEnd { .. } => "Move a layer end of",
            HandleKind::LayerEdge { .. } => "Move a layer edge of",
            HandleKind::LayerHeight { .. } => "Move a layer's base or top of",
            HandleKind::WallFace | HandleKind::SlabUnderside | HandleKind::SlabTop => {
                "Change thickness of"
            }
            HandleKind::OpeningJamb { .. } => "Change width of",
            HandleKind::OpeningSill => "Change sill of",
            HandleKind::Face(_) => "Push/pull",
            HandleKind::ViewDepth => "Change depth of",
            HandleKind::CropFace(_) => "Resize",
            HandleKind::CropMove
            | HandleKind::RefMove
            | HandleKind::MeshMove
            | HandleKind::MeshLift => "Move",
            HandleKind::RefScale(_) => "Scale",
            HandleKind::MeshVertex(_) | HandleKind::MeshEdge(..) => "Reshape",
            _ => "Stretch",
        }
    }

    /// The undo label of a drag of this grip on `name`.
    pub fn label(self, name: &str) -> String {
        match self {
            HandleKind::Vertex { .. } => format!("Move a corner of {name}"),
            HandleKind::AddVertex { .. } => format!("Add a corner to {name}"),
            HandleKind::Edge { .. } => format!("Offset an edge of {name}"),
            HandleKind::Ring {
                ring: Ring::Hole(k),
            } => format!("Move hole {} of {name}", k + 1),
            HandleKind::Ring { .. } => format!("Move the outline of {name}"),
            k => format!("{} {name}", k.verb()),
        }
    }

    /// The grip's short name, shown by it under the pointer.
    pub fn name(self) -> &'static str {
        match self {
            HandleKind::WallEnd { .. } => "Wall end",
            HandleKind::WallTop => "Top: raise",
            HandleKind::WallFace => "Face: thicken",
            HandleKind::SlabTop => "Top: raise",
            HandleKind::SlabUnderside => "Underside: thicken",
            HandleKind::OpeningJamb { .. } => "Jamb: widen",
            HandleKind::OpeningHead => "Head: raise",
            HandleKind::OpeningSill => "Sill: raise",
            HandleKind::GridEnd { .. } => "Grid end",
            HandleKind::Face(_) => "Face: push",
            HandleKind::Member { .. } => "Point",
            HandleKind::ViewEnd { .. } => "Line end",
            HandleKind::ViewDepth => "Depth",
            HandleKind::CropFace(_) => "Crop face",
            HandleKind::CropMove => "Crop box: move",
            HandleKind::RefMove => "Reference: move",
            HandleKind::RefScale(_) => "Corner: scale",
            HandleKind::Vertex { .. } => "Corner",
            HandleKind::Edge { .. } => "Edge: push",
            HandleKind::AddVertex { .. } => "Add a corner",
            HandleKind::Ring { .. } => "Outline: move",
            HandleKind::ShaftBase => "Base",
            HandleKind::ShaftTop => "Top",
            HandleKind::Rotate => "Turn",
            HandleKind::ColumnSize { depth: false } => "Width",
            HandleKind::ColumnSize { depth: true } => "Depth",
            HandleKind::WallSlide => "Wall: slide",
            HandleKind::WallBend => "Wall: bend",
            HandleKind::OpeningSlide => "Slide along the wall",
            HandleKind::BeamWidth | HandleKind::DetailWidth => "Width",
            HandleKind::DetailSpacing => "Unit",
            HandleKind::PlantCanopy => "Canopy",
            HandleKind::PlantHeight => "Height",
            HandleKind::CalloutCorner(_) => "Callout corner",
            HandleKind::CalloutEdge(_) => "Callout edge: push",
            HandleKind::LayerEnd { .. } => "Layer end",
            HandleKind::LayerEdge { .. } => "Layer edge",
            HandleKind::LayerHeight { top: false, .. } => "Layer bottom",
            HandleKind::LayerHeight { top: true, .. } => "Layer top",
            HandleKind::MeshVertex(_) => "Corner",
            HandleKind::MeshEdge(..) => "Edge",
            HandleKind::MeshMove => "Base point: move",
            HandleKind::MeshLift => "Up or down",
        }
    }

    /// What the grip does, for the tooltip by the pointer.
    pub fn tip(self, edit: bool) -> &'static str {
        match self {
            HandleKind::Vertex { .. } if edit => {
                "Drag: move corner · Click: select (Shift adds) · Del: remove · Right-click: more"
            }
            HandleKind::Vertex { .. } => {
                "Drag: move corner · Double-click: edit the outline (add and remove corners)"
            }
            HandleKind::AddVertex { .. } => "Drag: pull out a new corner · Right-click: more",
            HandleKind::Edge { .. } if edit => "Drag: offset the edge · Right-click: more",
            HandleKind::Edge { .. } => "Drag: offset the edge",
            HandleKind::WallEnd { .. } => "Drag: move the end (Ctrl: free of its joins)",
            HandleKind::WallTop | HandleKind::ShaftTop => "Drag: change the top",
            HandleKind::ShaftBase => "Drag: change the base",
            HandleKind::WallFace => "Drag: change the thickness",
            HandleKind::SlabTop => "Drag: move the top (the underside stays)",
            HandleKind::SlabUnderside => "Drag: change the thickness (the top stays)",
            HandleKind::OpeningJamb { .. } => "Drag: change the width",
            HandleKind::OpeningHead => "Drag: change the height",
            HandleKind::OpeningSill => "Drag: change the sill",
            HandleKind::Face(_) => "Drag: push or pull the face",
            HandleKind::MeshVertex(_) => "Drag: move the corner across the plan",
            HandleKind::MeshEdge(..) => "Drag: move the edge across the plan",
            HandleKind::MeshMove => "Drag: move the mass across the plan",
            HandleKind::MeshLift => "Drag: move the mass up or down",
            HandleKind::ViewDepth => "Drag: change how deep it sees",
            HandleKind::CropFace(_) => "Drag: move the crop face",
            HandleKind::RefMove => "Drag: move the reference",
            HandleKind::RefScale(_) => "Drag: scale it about its middle (Calibrate in Properties sets an exact size)",
            HandleKind::Rotate => "Drag: turn it about its middle (steps of 5°; Alt for any angle)",
            HandleKind::ColumnSize { depth: false } => "Drag: change the width",
            HandleKind::ColumnSize { depth: true } => "Drag: change the depth",
            HandleKind::WallSlide => "Drag: slide the wall square to itself (joined walls follow)",
            HandleKind::WallBend => "Drag: bend the wall through this point; its ends stay (its radius is in Properties)",
            HandleKind::OpeningSlide => "Drag: slide it along the wall · Tab: type how far",
            HandleKind::BeamWidth | HandleKind::DetailWidth => "Drag: change the width",
            HandleKind::DetailSpacing => "Drag: change the unit's length (the joint stays)",
            HandleKind::PlantCanopy => "Drag: change how wide the crown is",
            HandleKind::PlantHeight => "Drag: change how tall it is",
            HandleKind::CalloutCorner(_) => {
                "Drag: resize the callout's box (its detail view follows)"
            }
            HandleKind::CalloutEdge(_) => "Drag: push the box's edge (its detail view follows)",
            HandleKind::LayerEnd { .. } => {
                "Drag: run the layer on or hold it back · Tab: type how far · Right-click: reset"
            }
            HandleKind::LayerEdge { .. } => {
                "Drag: push the layer's edge out or pull it in · Tab: type how far · Right-click: reset"
            }
            HandleKind::LayerHeight { .. } => {
                "Drag: run the layer down or up past the wall, or stop it short · Tab: type how far · Right-click: reset"
            }
            _ => "Drag to change it",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub kind: HandleKind,
    pub at: DVec3,
    pub constraint: Constraint,
    /// Screen pixels the grip is drawn out from `at` along its line, to
    /// keep it clear of another grip at the same point.
    pub nudge: f32,
}

/// The handles of element `id`, with `edit` those Edit mode adds (see
/// [`crate::editmode`]).
pub fn handles_in(model: &Model, id: ElementId, face: Option<usize>, edit: bool) -> Vec<Handle> {
    if edit && layers() {
        let ends = layer_handles(model, id);
        if !ends.is_empty() {
            return ends;
        }
    }
    let mut out = handles(model, id, face);
    out.extend(crate::editmode::outline_handles(model, id, edit));
    if edit {
        out.extend(mass_edit_handles(model, id, face));
        // vertex, edge or face picking (MU27)
        out.retain(|h| pick_allows(&h.kind));
    }
    out
}

static LAYERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What Edit mode picks (MU27, as Blender): 0 everything, 1 points, 2
/// edges, 3 faces.
static PICK: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Edit mode's selection kind: vertices, edges or faces (0 any).
pub fn set_pick(k: u8) {
    PICK.store(k.min(3), std::sync::atomic::Ordering::Relaxed);
}

pub fn pick() -> u8 {
    PICK.load(std::sync::atomic::Ordering::Relaxed)
}

/// Does grip `k` belong to Edit mode's selection kind?
pub fn pick_allows(k: &HandleKind) -> bool {
    use HandleKind as K;
    match pick() {
        1 => matches!(
            k,
            K::Vertex { .. }
                | K::MeshVertex(_)
                | K::WallEnd { .. }
                | K::GridEnd { .. }
                | K::Member { .. }
                | K::MeshMove
                | K::Rotate
        ),
        2 => matches!(
            k,
            K::Edge { .. }
                | K::AddVertex { .. }
                | K::WallBend
                | K::MeshEdge(..)
                | K::OpeningJamb { .. }
                | K::OpeningHead
                | K::OpeningSill
                | K::MeshMove
                | K::Rotate
        ),
        3 => matches!(
            k,
            K::Face(_)
                | K::WallFace
                | K::WallTop
                | K::SlabTop
                | K::SlabUnderside
                | K::MeshLift
                | K::ShaftBase
                | K::ShaftTop
                | K::MeshMove
                | K::Rotate
        ),
        _ => true,
    }
}

/// Edit mode's Layers (3.7): Tab in Edit mode on a layered element shows
/// a grip at each of its layers' ends instead of its own grips.
pub fn set_layers(on: bool) {
    LAYERS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn layers() -> bool {
    LAYERS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Does `id` have layers to edit one by one?
pub fn has_layers(model: &Model, id: ElementId) -> bool {
    !layer_handles(model, id).is_empty()
}

/// A grip at each end of each layer of wall `id`, where the joins and any
/// edit put it, sliding along the wall. Empty unless it is layered.
pub fn layer_handles(model: &Model, id: ElementId) -> Vec<Handle> {
    let Some(e) = model.get(id) else {
        return vec![];
    };
    let ElementKind::Wall(w) = &e.kind else {
        return edge_handles(model, e);
    };
    let Some((_, a)) = creabim_core::assembly::assembly_of(model, e) else {
        return vec![];
    };
    let bands = creabim_core::assembly::wall_bands(a, w);
    if bands.len() < 2 {
        return vec![];
    }
    let ends = creabim_core::wall_joins(model)
        .remove(&id)
        .unwrap_or_default();
    let z0 = elevation(model, Some(w.level)) + w.base_offset;
    let (d, n) = (w.direction(), w.direction().perp());
    let last = (bands.len() - 1) as f64;
    let mut out = vec![];
    for (k, b) in bands.iter().enumerate() {
        let s = (b.s0 + b.s1) / 2.0;
        // stepped up the end face, so each reads on its own in 3D
        let z = z0 + w.height * (0.3 + 0.4 * k as f64 / last);
        for end in [false, true] {
            let u = ends.place_layer(w, if end { w.length() } else { 0.0 }, s, b.core, b.ext);
            let at = (w.start + d * u + n * s).extend(z);
            out.push(Handle {
                kind: HandleKind::LayerEnd {
                    layer: b.index,
                    end,
                },
                at,
                constraint: Constraint::Line {
                    point: at,
                    dir: d.extend(0.0),
                },
                nudge: 0.0,
            });
        }
        // its bottom and top, up and down (A7-10), in the middle of the
        // wall's length
        let mid = w.start + d * (w.length() / 2.0) + n * s;
        let [down, up] = b.vext;
        for (top, z) in [(false, z0 - down), (true, z0 + w.height + up)] {
            let at = mid.extend(z);
            out.push(Handle {
                kind: HandleKind::LayerHeight {
                    layer: b.index,
                    top,
                },
                at,
                constraint: Constraint::Line {
                    point: at,
                    dir: DVec3::Z,
                },
                nudge: 0.0,
            });
        }
    }
    out
}

/// How far the layer end or edge of grip `kind` was moved by hand on
/// `e`; `None` when `kind` is no such grip of it.
fn layer_offset(e: &ElementKind, kind: HandleKind) -> Option<f64> {
    match (e, kind) {
        (ElementKind::Wall(w), HandleKind::LayerEnd { layer, end }) => {
            Some(w.layer_ext(layer)[usize::from(end)])
        }
        (ElementKind::Wall(w), HandleKind::LayerHeight { layer, top }) => {
            Some(w.layer_vext(layer)[usize::from(top)])
        }
        (ElementKind::Slab(s), HandleKind::LayerEdge { layer, edge }) => {
            Some(creabim_core::layer_edge_offsets(&s.layer_edges, layer, s.boundary.len())[edge])
        }
        (ElementKind::Roof(r), HandleKind::LayerEdge { layer, edge }) => {
            Some(creabim_core::layer_edge_offsets(&r.layer_edges, layer, r.boundary.len())[edge])
        }
        _ => None,
    }
}

/// Set how far the layer end or edge of grip `kind` is moved.
fn set_layer_offset(e: &mut ElementKind, kind: HandleKind, by: f64) {
    match (e, kind) {
        (ElementKind::Wall(w), HandleKind::LayerEnd { layer, end }) => {
            w.set_layer_ext(layer, end, by)
        }
        (ElementKind::Wall(w), HandleKind::LayerHeight { layer, top }) => {
            w.set_layer_vext(layer, top, by)
        }
        (ElementKind::Slab(s), HandleKind::LayerEdge { layer, edge }) => {
            creabim_core::set_layer_edge(&mut s.layer_edges, layer, edge, by)
        }
        (ElementKind::Roof(r), HandleKind::LayerEdge { layer, edge }) => {
            creabim_core::set_layer_edge(&mut r.layer_edges, layer, edge, by)
        }
        _ => {}
    }
}

/// The outward normal of edge `i` of outline `poly`, in plan.
fn edge_out(poly: &[DVec2], i: usize) -> DVec2 {
    let n = poly.len();
    let d = (poly[(i + 1) % n] - poly[i]).normalize_or_zero();
    if creabim_core::polygon::signed_area2(poly) > 0.0 {
        -d.perp()
    } else {
        d.perp()
    }
}

/// A layer's index, its outline, the element's own outline (whose edge
/// normals the grips slide along), and how high its middle is over a
/// point of the plan.
type LayerRow = (usize, Vec<DVec2>, Vec<DVec2>, Box<dyn Fn(DVec2) -> f64>);

/// A grip on each edge of each layer of a layered slab or roof, at the
/// middle of the edge and of the layer's depth, sliding square to the
/// edge. In plan they would sit on one another, so each is drawn a step
/// further out.
fn edge_handles(model: &Model, e: &Element) -> Vec<Handle> {
    let Some((_, a)) = creabim_core::assembly::assembly_of(model, e) else {
        return vec![];
    };
    // each layer's outline, the normal of its edges, and where its
    // middle is at a point of the plan
    let rows: Vec<LayerRow> = match &e.kind {
        ElementKind::Slab(s) => {
            let top = s.top(elevation(model, Some(s.level)));
            creabim_core::assembly::depth_bands(a, s.thickness)
                .into_iter()
                .map(|b| {
                    let mid = top - (b.s0 + b.s1) / 2.0;
                    (
                        b.index,
                        s.layer_boundary(b.index),
                        s.boundary.clone(),
                        Box::new(move |_| mid) as Box<dyn Fn(DVec2) -> f64>,
                    )
                })
                .collect()
        }
        ElementKind::Roof(r) => {
            let base = r.base(model);
            let k = r.vertical_thickness() / r.thickness.max(1e-9);
            let tv = r.vertical_thickness();
            creabim_core::assembly::depth_bands(a, r.thickness)
                .into_iter()
                .map(|b| {
                    let faces = r.layer_faces(b.index);
                    let up = tv - (b.s0 + b.s1) / 2.0 * k;
                    let z = move |p: DVec2| {
                        // the underside under p: the lowest face over it
                        let under = faces.iter().map(|f| f.z(p)).fold(f64::INFINITY, f64::min);
                        base + if under.is_finite() { under } else { 0.0 } + up
                    };
                    (
                        b.index,
                        r.layer_eaves(b.index),
                        r.boundary.clone(),
                        Box::new(z) as Box<dyn Fn(DVec2) -> f64>,
                    )
                })
                .collect()
        }
        _ => return vec![],
    };
    if rows.len() < 2 {
        return vec![];
    }
    let mut out = vec![];
    for (k, (layer, poly, own, z)) in rows.iter().enumerate() {
        let n = poly.len();
        for edge in 0..n {
            let mid = (poly[edge] + poly[(edge + 1) % n]) / 2.0;
            let at = mid.extend(z(mid));
            out.push(Handle {
                kind: HandleKind::LayerEdge {
                    layer: *layer,
                    edge,
                },
                at,
                constraint: Constraint::Line {
                    point: at,
                    dir: edge_out(own, edge).extend(0.0),
                },
                nudge: 14.0 * k as f32,
            });
        }
    }
    out
}

/// Put the layer end or edge of grip `which` of `id` back where its join
/// or the element's own edge puts it, or with `None` every layer's, as
/// one undo step.
/// Draw wall `id` by the line between its layers `k - 1` and `k`, from
/// the exterior: only its location line changes, the wall stays put.
pub fn draw_by_boundary(doc: &mut Document, id: ElementId, k: u8) -> Result<String, String> {
    let e = doc.model().get(id).ok_or("it is gone")?.clone();
    let mut after = e.clone();
    let ElementKind::Wall(w) = &mut after.kind else {
        return Err("only walls have a location line".into());
    };
    let line = creabim_core::LocationLine::Boundary(k);
    if w.location == line {
        return Err("The wall is drawn by that line already.".into());
    }
    w.location = line;
    let text = creabim_core::walls::location_label(doc.model(), w);
    doc.commit(creabim_core::Transaction::new("Wall location line").replace(e, after))
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "The wall is drawn by {text} now. It stays where it is; new widths and layers keep that line put."
    ))
}

pub fn reset_layer_ends(
    doc: &mut Document,
    id: ElementId,
    which: Option<HandleKind>,
) -> Result<String, String> {
    let e = doc.model().get(id).ok_or("it is gone")?.clone();
    let mut after = e.clone();
    let done = match (which, &mut after.kind) {
        (Some(k), kind) => {
            set_layer_offset(kind, k, 0.0);
            (
                "Reset layer end",
                "The layer is back where its element puts it.",
            )
        }
        (None, ElementKind::Wall(w)) => {
            w.layer_ends.clear();
            ("Reset all layers", "Every layer's end is back on its join.")
        }
        (None, ElementKind::Slab(s)) => {
            s.layer_edges.clear();
            (
                "Reset all layers",
                "Every layer's edge is back on the slab's.",
            )
        }
        (None, ElementKind::Roof(r)) => {
            r.layer_edges.clear();
            (
                "Reset all layers",
                "Every layer's edge is back on the roof's.",
            )
        }
        _ => return Err("it has no layers to reset".into()),
    };
    if after == e {
        return Err("Nothing to reset: no layer was moved.".into());
    }
    doc.commit(creabim_core::Transaction::new(done.0).replace(e, after))
        .map_err(|e| e.to_string())?;
    Ok(done.1.into())
}

/// What a layer grip says by it: its layer, which end or edge, and how
/// far it was moved.
fn layer_name(model: &Model, id: ElementId, kind: HandleKind) -> Option<String> {
    let (layer, side) = match kind {
        HandleKind::LayerEnd { layer, end } => {
            (layer, if end { "end" } else { "start" }.to_string())
        }
        HandleKind::LayerEdge { layer, edge } => (layer, format!("edge {}", edge + 1)),
        HandleKind::LayerHeight { layer, top } => {
            (layer, if top { "top" } else { "bottom" }.to_string())
        }
        _ => return None,
    };
    let e = model.get(id)?;
    let (_, a) = creabim_core::assembly::assembly_of(model, e)?;
    let l = a.layers.get(layer)?;
    let what = l
        .material
        .and_then(|m| model.get(m))
        .map_or_else(|| format!("Layer {}", layer + 1), |m| m.name.clone());
    let by = layer_offset(&e.kind, kind).unwrap_or(0.0);
    Some(if by == 0.0 {
        format!("{what}: {side}")
    } else {
        format!("{what}: {side}, {:+.0} mm", by * 1000.0)
    })
}

/// Has the layer end or edge of grip `kind` on `e` been moved by hand?
fn edited_end(e: Option<&Element>, kind: HandleKind) -> bool {
    e.and_then(|e| layer_offset(&e.kind, kind))
        .is_some_and(|by| by != 0.0)
}

fn elevation(model: &Model, level: Option<ElementId>) -> f64 {
    level
        .and_then(|l| model.level(l))
        .map_or(0.0, |l| l.elevation)
}

/// The handles of element `id`. `face` is the selected face of a mass.
/// The section a reference is shown in, as its line's start and direction;
/// none when it is shown in plans (its coordinates are then the plan's).
fn ref_plane(model: &Model, r: &creabim_core::Reference) -> Option<(DVec2, DVec2)> {
    let v = r.shown_in.first()?;
    match &model.get(*v)?.kind {
        ElementKind::View(creabim_core::View {
            kind: creabim_core::ViewKind::Section { start, end, .. },
            ..
        }) => Some((*start, (*end - *start).normalize_or_zero())),
        _ => None,
    }
}

/// A reference's point (its views' coordinates) in the model: on a plan's
/// ground, or standing on its section's plane.
pub fn ref_world(model: &Model, r: &creabim_core::Reference, p: DVec2) -> DVec3 {
    match ref_plane(model, r) {
        Some((s, d)) => (s + d * p.x).extend(p.y),
        // on the level of the plan it is shown in, so 3D shows it there
        None => {
            let z = r.shown_in.first().and_then(|v| match &model.get(*v)?.kind {
                ElementKind::View(creabim_core::View {
                    kind: creabim_core::ViewKind::Plan { level, .. },
                    ..
                }) => model.level(*level).map(|l| l.elevation),
                _ => None,
            });
            p.extend(z.unwrap_or(0.0))
        }
    }
}

/// The other way: a model point in the reference's views' coordinates.
fn ref_local(model: &Model, r: &creabim_core::Reference, w: DVec3) -> DVec2 {
    match ref_plane(model, r) {
        Some((s, d)) => DVec2::new((w.truncate() - s).dot(d), w.z),
        None => w.truncate(),
    }
}

pub fn handles(model: &Model, id: ElementId, face: Option<usize>) -> Vec<Handle> {
    let Some(e) = model.get(id) else {
        return vec![];
    };
    let line = |kind, at: DVec3, dir: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Line {
            point: at,
            dir: dir.normalize(),
        },
        nudge: 0.0,
    };
    let flat = |kind, at: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Plane {
            point: at,
            normal: DVec3::Z,
        },
        nudge: 0.0,
    };
    match &e.kind {
        ElementKind::Wall(w) => {
            let z0 = elevation(model, Some(w.level)) + w.base_offset;
            // a curved wall's middle is on its arc, square to it there
            let half = w.length() / 2.0;
            let mid = w.place(half, 0.0);
            let n = w.tangent_at(half).perp();
            // the ends are gripped on the wall's location line
            let (a, b) = walls::location_line(model, w);
            let bend = w
                .is_arc()
                .then(|| flat(HandleKind::WallBend, mid.extend(z0)));
            // on a curved wall the bend grip has the middle: the top and
            // thickness grips stand a quarter along, so seen from above
            // they don't sit on it and fight it for the pointer
            let (mid, n) = if w.is_arc() {
                let q = w.length() / 4.0;
                (w.place(q, 0.0), w.tangent_at(q).perp())
            } else {
                (mid, n)
            };
            bend.into_iter()
                .chain([
                    flat(HandleKind::WallEnd { end: false }, a.extend(z0)),
                    flat(HandleKind::WallEnd { end: true }, b.extend(z0)),
                    line(HandleKind::WallTop, mid.extend(z0 + w.height), DVec3::Z),
                    line(
                        HandleKind::WallFace,
                        (mid + n * w.thickness / 2.0).extend(z0 + w.height / 2.0),
                        n.extend(0.0),
                    ),
                    // no grip of its own on screen: a drag of the wall's body
                    // slides it (see interact); kept here for that drag
                    line(
                        HandleKind::WallSlide,
                        (mid - n * (w.thickness / 2.0 + 0.3)).extend(z0 + w.height / 2.0),
                        n.extend(0.0),
                    ),
                ])
                .collect()
        }
        ElementKind::Slab(s) => {
            // its outline's grips are every outline's (see handles_in)
            let z = s.top(elevation(model, Some(s.level)));
            let n = s.boundary.len();
            // the face a thickness handle sits on is the one that moves
            let c = s.boundary.iter().copied().sum::<DVec2>() / n.max(1) as f64;
            vec![
                line(HandleKind::SlabTop, c.extend(z), DVec3::Z),
                line(
                    HandleKind::SlabUnderside,
                    c.extend(z - s.thickness),
                    DVec3::Z,
                ),
            ]
        }
        ElementKind::Opening(o) => {
            let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                return vec![];
            };
            let z0 = elevation(model, Some(w.level)) + w.base_offset;
            let mid = o.offset + o.width / 2.0;
            // along the wall at the opening: on a curved wall its tangent
            // there, and the grips on the arc where the opening is
            let u = w.tangent_at(mid);
            // on the jambs, at the middle of the wall's depth, so they read
            // as part of the opening at every scale (A5-5); not on a face,
            // where at 1:10 they floated beside it
            let at = |along: f64, up: f64| w.place(along, 0.0).extend(z0 + up);
            let mut out = vec![
                line(
                    HandleKind::OpeningJamb { right: false },
                    at(o.offset, o.sill + o.height / 2.0),
                    u.extend(0.0),
                ),
                line(
                    HandleKind::OpeningJamb { right: true },
                    at(o.offset + o.width, o.sill + o.height / 2.0),
                    u.extend(0.0),
                ),
                line(
                    HandleKind::OpeningHead,
                    at(mid, o.sill + o.height),
                    DVec3::Z,
                ),
            ];
            if matches!(o.kind, OpeningKind::Window | OpeningKind::Hole) {
                out.push(line(HandleKind::OpeningSill, at(mid, o.sill), DVec3::Z));
            }
            // in the middle, to slide it along its wall
            out.push(line(
                HandleKind::OpeningSlide,
                at(mid, o.sill + o.height / 2.0),
                u.extend(0.0),
            ));
            out
        }
        ElementKind::Shaft(s) => {
            let (z0, z1) = s.span(model);
            let n = s.boundary.len().max(1) as f64;
            let c = s.boundary.iter().copied().sum::<DVec2>() / n;
            vec![
                line(HandleKind::ShaftBase, c.extend(z0), DVec3::Z),
                line(HandleKind::ShaftTop, c.extend(z1), DVec3::Z),
            ]
        }
        // a detail view's box on its parent, not the line it was cut on
        ElementKind::View(v) if v.callout.is_some() => callout_grips(model, v),
        ElementKind::View(v) => match v.kind {
            ViewKind::Section {
                start, end, depth, ..
            } => {
                let look = (end - start).normalize_or_zero().perp();
                vec![
                    flat(HandleKind::ViewEnd { end: false }, start.extend(0.0)),
                    flat(HandleKind::ViewEnd { end: true }, end.extend(0.0)),
                    line(
                        HandleKind::ViewDepth,
                        ((start + end) / 2.0 + look * depth).extend(0.0),
                        look.extend(0.0),
                    ),
                ]
            }
            _ => vec![],
        },
        ElementKind::CropBox(c) => {
            let mut out: Vec<Handle> = (0..6)
                .map(|i| {
                    let (at, n) = c.face(i);
                    line(HandleKind::CropFace(i), at, n)
                })
                .collect();
            out.push(flat(HandleKind::CropMove, c.min));
            out
        }
        ElementKind::Detail(d) => detail_grips(model, d),
        // a reference: its middle moves it, its corners scale it, the
        // turn grip above its top turns it; none when it is locked
        ElementKind::Reference(r) if !r.locked => {
            // in a section the grips stand on its plane (MU32: they lay
            // on the ground at the section's numbers)
            let w = |p: DVec2| ref_world(model, r, p);
            // dragged on its own plane: a section's upright one, so the
            // middle moves up and down in 3D too (MU33)
            let normal = ref_plane(model, r).map_or(DVec3::Z, |(_, d)| d.perp().extend(0.0));
            let on = |kind, at: DVec3| Handle {
                kind,
                at,
                constraint: Constraint::Plane { point: at, normal },
                nudge: 0.0,
            };
            let Some(c) = r.outline() else {
                return vec![on(HandleKind::RefMove, w(r.at))];
            };
            let mut out: Vec<Handle> = (0..4)
                .map(|i| on(HandleKind::RefScale(i), w(c[i])))
                .collect();
            out.push(on(HandleKind::RefMove, w(r.at)));
            // turning is in plan only; a section's is typed in Properties
            if ref_plane(model, r).is_none() {
                let top = (c[2] + c[3]) / 2.0;
                let up = (top - r.at).normalize_or_zero();
                out.push(flat(HandleKind::Rotate, (top + up * 0.6).extend(0.0)));
            }
            out
        }
        ElementKind::Site(s) => match s.form {
            creabim_core::SiteForm::Plant {
                position,
                base,
                canopy,
                height,
                ..
            } => {
                let z = elevation(model, Some(s.level)) + base;
                vec![
                    line(
                        HandleKind::PlantCanopy,
                        (position + DVec2::X * canopy / 2.0).extend(z),
                        DVec3::X,
                    ),
                    line(
                        HandleKind::PlantHeight,
                        position.extend(z + height),
                        DVec3::Z,
                    ),
                ]
            }
            // its outline's grips are every outline's (see handles_in)
            _ => vec![],
        },
        ElementKind::Grid(g) => vec![
            flat(HandleKind::GridEnd { end: false }, g.start.extend(0.0)),
            flat(HandleKind::GridEnd { end: true }, g.end.extend(0.0)),
        ],
        ElementKind::FreeMesh(m) => {
            // at rest only the main shape editing (MU18): its base point
            // to move it, the turn grip, and the clicked face's push/pull;
            // corners, edges and every face's arrow wait for Edit mode
            let base = m.origin + DVec3::Z * elevation(model, m.level);
            let mut out: Vec<Handle> = face
                .filter(|f| *f < m.mesh.faces.len())
                .and_then(|f| {
                    let c = base + m.to_model(m.mesh.face_center(f));
                    let n = m.to_model(m.mesh.face_normal(f)).normalize_or_zero();
                    (n != DVec3::ZERO).then(|| line(HandleKind::Face(f), c, n))
                })
                .into_iter()
                .collect();
            out.push(flat(HandleKind::MeshMove, base));
            let reach = m
                .mesh
                .vertices
                .iter()
                .map(|v| m.to_model(*v).truncate().length())
                .fold(0.0_f64, f64::max);
            let turn = (DVec2::from_angle(m.rotation.to_radians()) * (reach + 0.4)).extend(0.0);
            out.push(flat(HandleKind::Rotate, base + turn));
            out
        }
        ElementKind::Object(o) => {
            let z = creabim_core::objects::base_z(model, e).unwrap_or(0.0);
            let (lo, hi) = creabim_core::objects::type_of(model, e)
                .map(|t| t.bounds())
                .unwrap_or((DVec3::new(-0.3, -0.6, 0.0), DVec3::new(0.3, 0.0, 1.0)));
            let front = DVec2::new((lo.x + hi.x) / 2.0, lo.y - 0.25);
            vec![flat(HandleKind::Rotate, o.to_plan(front).extend(z))]
        }
        _ => creabim_core::members::grips(model, e)
            .into_iter()
            .enumerate()
            // a roof's corners and a railing's path points are outline
            // corners (see handles_in); the grips after them keep their
            // numbers
            .filter(|(i, _)| *i >= outline_grips(e))
            .map(|(i, g)| match g.how {
                creabim_core::members::GripMove::Flat => {
                    flat(HandleKind::Member { i, free: true }, g.at)
                }
                creabim_core::members::GripMove::Vertical => {
                    line(HandleKind::Member { i, free: false }, g.at, DVec3::Z)
                }
                creabim_core::members::GripMove::Along(d) => {
                    line(HandleKind::Member { i, free: false }, g.at, d.extend(0.0))
                }
            })
            .chain(member_extras(model, e))
            .collect(),
    }
}

/// Is a callout on `parent` drawn as a ring? (See the parent's annotations.)
fn callout_is_round(model: &Model, parent: ElementId) -> bool {
    matches!(
        model.get(parent).map(|e| &e.kind),
        Some(ElementKind::View(v)) if matches!(v.kind, ViewKind::Section { .. })
    ) && creabim_core::styles::active_pack(model)
        .marker_parts()
        .is_some()
}

/// The corners of a callout's box, `lo` first and counter-clockwise.
fn box_corners(lo: DVec2, hi: DVec2) -> [DVec2; 4] {
    [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)]
}

/// A callout's grips on its parent view: a square on each corner of its
/// box and a bar on each edge.
fn callout_grips(model: &Model, v: &creabim_core::View) -> Vec<Handle> {
    let Some(c) = v.callout else { return vec![] };
    let Some(frame) = creabim_core::DrawingFrame::of(model, c.parent) else {
        return vec![];
    };
    let normal = frame.normal();
    let k = box_corners(c.lo, c.hi);
    let dir = |d: DVec2| (frame.to_model(d) - frame.to_model(DVec2::ZERO)).normalize_or_zero();
    let outs = [-DVec2::Y, DVec2::X, DVec2::Y, -DVec2::X];
    let mut out: Vec<Handle> = (0..4)
        .map(|i| {
            let at = frame.to_model(k[i]);
            Handle {
                kind: HandleKind::CalloutCorner(i),
                at,
                constraint: Constraint::Plane { point: at, normal },
                nudge: 0.0,
            }
        })
        .collect();
    for (i, o) in outs.iter().enumerate() {
        let at = frame.to_model((k[i] + k[(i + 1) % 4]) / 2.0);
        out.push(Handle {
            kind: HandleKind::CalloutEdge(i),
            at,
            constraint: Constraint::Line {
                point: at,
                dir: dir(*o),
            },
            nudge: 0.0,
        });
    }
    out
}

/// A detail component's own grips besides its points (see handles_in): a
/// width grip off the middle of its first stretch, and a repeating
/// detail's unit grip at the end of its first unit.
fn detail_grips(model: &Model, d: &creabim_core::DetailItem) -> Vec<Handle> {
    let Some(frame) = creabim_core::DrawingFrame::of(model, d.view) else {
        return vec![];
    };
    let mut out = vec![];
    let (Some(a), Some(b)) = (d.points.first(), d.points.get(1)) else {
        return out;
    };
    let across = d.across();
    let dir = |v: DVec2| (frame.to_model(v) - frame.to_model(DVec2::ZERO)).normalize_or_zero();
    let widened = !matches!(
        d.kind,
        DetailKind::Membrane | DetailKind::Region { .. } | DetailKind::BreakLine
    );
    if widened {
        let at = frame.to_model((*a + *b) / 2.0 + across * d.width / 2.0);
        out.push(Handle {
            kind: HandleKind::DetailWidth,
            at,
            constraint: Constraint::Line {
                point: at,
                dir: dir(across),
            },
            nudge: 0.0,
        });
    }
    if let DetailKind::Repeat { module, .. } = d.kind {
        let along = (*b - *a).normalize_or_zero();
        let at = frame.to_model(d.at_length(module));
        out.push(Handle {
            kind: HandleKind::DetailSpacing,
            at,
            constraint: Constraint::Line {
                point: at,
                dir: dir(along),
            },
            nudge: 0.0,
        });
    }
    out
}

/// The grips a column and a stair have besides their members' own: a
/// turning grip, and a column's width and depth.
fn member_extras(model: &Model, e: &Element) -> Vec<Handle> {
    let line = |kind, at: DVec3, dir: DVec2| Handle {
        kind,
        at,
        constraint: Constraint::Line {
            point: at,
            dir: dir.normalize().extend(0.0),
        },
        nudge: 0.0,
    };
    let flat = |kind, at: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Plane {
            point: at,
            normal: DVec3::Z,
        },
        nudge: 0.0,
    };
    match &e.kind {
        ElementKind::Column(c) => {
            let (z0, z1) = c.span(model);
            // its size bars half-way up its sides, in sections too (A4-8)
            let zm = (z0 + z1) / 2.0;
            let x = DVec2::from_angle(c.rotation.to_radians());
            let (w, d) = match c.profile {
                creabim_core::members::Profile::Rectangle { width, depth } => (width, depth),
                creabim_core::members::Profile::Round { diameter } => (diameter, diameter),
            };
            let mut out = vec![
                flat(
                    HandleKind::Rotate,
                    (c.position + x * (w / 2.0 + 0.35)).extend(z0),
                ),
                line(
                    HandleKind::ColumnSize { depth: false },
                    (c.position + x * w / 2.0).extend(zm),
                    x,
                ),
            ];
            if matches!(c.profile, creabim_core::members::Profile::Rectangle { .. }) {
                out.push(line(
                    HandleKind::ColumnSize { depth: true },
                    (c.position + x.perp() * d / 2.0).extend(zm),
                    x.perp(),
                ));
            }
            out
        }
        ElementKind::Beam(b) => {
            let n = (b.end - b.start).normalize_or_zero().perp();
            let mid = (b.start + b.end) / 2.0;
            vec![line(
                HandleKind::BeamWidth,
                // half-way down its side, in sections too (A4-8)
                (mid + n * b.width / 2.0).extend(b.top(model) - b.depth / 2.0),
                n,
            )]
        }
        ElementKind::Stair(s) => {
            let dir = DVec2::from_angle(s.direction.to_radians());
            vec![flat(
                HandleKind::Rotate,
                (s.start - dir * 0.6).extend(s.base(model)),
            )]
        }
        _ => vec![],
    }
}

/// The point a rotate grip turns its element about, in plan.
fn pivot_of(model: &Model, e: &Element) -> Option<DVec2> {
    match &e.kind {
        ElementKind::Object(o) => {
            let (lo, hi) = creabim_core::objects::type_of(model, e)?.bounds();
            Some(o.to_plan((lo.truncate() + hi.truncate()) / 2.0))
        }
        ElementKind::Column(c) => Some(c.position),
        ElementKind::Stair(s) => Some(s.start),
        ElementKind::FreeMesh(m) => Some(m.origin.truncate()),
        ElementKind::Reference(r) => Some(r.at),
        _ => None,
    }
}

/// What Edit mode adds on a mass of a few faces (A7-9, MU18): an arrow on
/// every other face, a grip on every corner and edge middle, and the
/// up/down arrow.
fn mass_edit_handles(model: &Model, id: ElementId, face: Option<usize>) -> Vec<Handle> {
    let Some(ElementKind::FreeMesh(m)) = model.get(id).map(|e| &e.kind) else {
        return vec![];
    };
    if m.mesh.faces.len() > MASS_GRIP_FACES || m.mesh.vertices.len() > MASS_GRIP_CORNERS {
        return vec![];
    }
    let base = m.origin + DVec3::Z * elevation(model, m.level);
    let line = |kind, at: DVec3, dir: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Line {
            point: at,
            dir: dir.normalize(),
        },
        nudge: 0.0,
    };
    let flat = |kind, at: DVec3| Handle {
        kind,
        at,
        constraint: Constraint::Plane {
            point: at,
            normal: DVec3::Z,
        },
        nudge: 0.0,
    };
    let mut out: Vec<Handle> = (0..m.mesh.faces.len())
        .filter(|f| Some(*f) != face)
        .filter_map(|f| {
            let c = base + m.to_model(m.mesh.face_center(f));
            let n = m.to_model(m.mesh.face_normal(f)).normalize_or_zero();
            (n != DVec3::ZERO).then(|| line(HandleKind::Face(f), c, n))
        })
        .collect();
    let at = |i: u32| base + m.to_model(m.mesh.vertices[i as usize]);
    for i in 0..m.mesh.vertices.len() {
        out.push(flat(HandleKind::MeshVertex(i), at(i as u32)));
    }
    for (a, b) in mesh_edges(&m.mesh) {
        out.push(flat(HandleKind::MeshEdge(a, b), (at(a) + at(b)) / 2.0));
    }
    let top = m
        .mesh
        .vertices
        .iter()
        .map(|v| m.to_model(*v).z)
        .fold(0.0_f64, f64::max);
    out.push(line(
        HandleKind::MeshLift,
        base + DVec3::Z * (top + 0.4),
        DVec3::Z,
    ));
    out
}

/// Above these, a mass shows only the clicked face's arrow: grips on
/// every corner of a detailed mesh would bury it.
const MASS_GRIP_FACES: usize = 24;
const MASS_GRIP_CORNERS: usize = 48;

/// Each edge of a mesh once, its corners in order.
pub fn mesh_edges(mesh: &creabim_core::editmesh::EditMesh) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = mesh
        .faces
        .iter()
        .flat_map(|f| (0..f.len()).map(move |i| (f[i], f[(i + 1) % f.len()])))
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// An object turned `by` degrees about the middle of its footprint.
pub fn turn_object(model: &Model, e: &mut Element, by: f64) {
    let pivot = pivot_of(model, e);
    if let ElementKind::Object(o) = &mut e.kind {
        let c = pivot.unwrap_or(o.position);
        let r = DVec2::from_angle(by.to_radians());
        o.position = c + r.rotate(o.position - c);
        o.rotation = (o.rotation + by).rem_euclid(360.0);
    }
}

/// How many of a member's grips are corners of its outline, given as
/// outline grips instead.
fn outline_grips(e: &Element) -> usize {
    match &e.kind {
        ElementKind::Roof(r) => r.boundary.len(),
        ElementKind::Railing(r) if r.stair.is_none() => r.path.len(),
        _ => 0,
    }
}

/// A handle that moves across the plan, seen in a section or elevation
/// (looking level along `look`): there it slides across the paper, along
/// the section line, so wall ends, slab corners, beam ends and the like
/// can be dragged there too.
pub fn in_section(h: Handle, look: DVec3) -> Handle {
    // layer edges are drawn apart only in plan, where they would sit on
    // one another; elsewhere each is at its own layer's depth
    if matches!(h.kind, HandleKind::LayerEdge { .. }) && look.z.abs() < 0.99 {
        return Handle { nudge: 0.0, ..h };
    }
    match h.constraint {
        Constraint::Plane { point, normal } if normal.z.abs() > 0.99 && look.z.abs() < 0.1 => {
            Handle {
                constraint: Constraint::Line {
                    point,
                    dir: DVec3::Z.cross(look).normalize_or(DVec3::X),
                },
                ..h
            }
        }
        _ => h,
    }
}

/// The elements a drag of `kind` on `id` changes: the element first, then
/// for a wall end the walls whose ends sit on it (so corners stay closed).
/// The second list says which end of each joined wall moves.
pub fn originals(
    model: &Model,
    id: ElementId,
    kind: HandleKind,
) -> (Vec<Element>, Vec<(usize, bool)>) {
    let Some(e) = model.get(id) else {
        return (vec![], vec![]);
    };
    let mut out = vec![e.clone()];
    let mut joined = vec![];
    if let (HandleKind::WallEnd { end }, ElementKind::Wall(w)) = (kind, &e.kind) {
        let p = if end { w.end } else { w.start };
        // an end that may not join drags alone
        let models = if w.disallow_join[usize::from(end)] {
            None
        } else {
            Some(model)
        };
        for other in models.into_iter().flat_map(|m| m.iter()) {
            if let ElementKind::Wall(o) = &other.kind {
                if other.id == id || o.level != w.level {
                    continue;
                }
                for (oe, q) in [(false, o.start), (true, o.end)] {
                    if q.distance(p) < 1e-3 && !o.disallow_join[usize::from(oe)] {
                        out.push(other.clone());
                        joined.push((out.len() - 1, oe));
                    }
                }
            }
        }
    }
    (out, joined)
}

fn round(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

/// The dragged elements with handle `kind` moved from `grab` to `now`
/// (model points on the handle's constraint). `snapped` is `now` already
/// snapped to something, for handles that move freely. Lengths round to
/// `step`. Returns an explanation when the drag would break the element.
pub fn apply(
    model: &Model,
    originals: &[Element],
    joined: &[(usize, bool)],
    kind: HandleKind,
    grab: DVec3,
    now: DVec3,
    step: f64,
) -> Result<Vec<Element>, String> {
    let mut out = originals.to_vec();
    let d = now - grab;
    let mut moved_end = None;
    let first = out.first_mut().ok_or("nothing to drag")?;
    match (&mut first.kind, kind) {
        (ElementKind::Wall(w), HandleKind::WallEnd { end }) => {
            // the grip is on the location line: move that, and the
            // centreline with it
            let p = now.truncate();
            let (mut a, mut b) = walls::location_line(model, w);
            if end {
                b = p;
            } else {
                a = p;
            }
            if a.distance(b) < 0.05 {
                return Err("a wall needs some length".into());
            }
            walls::set_location_line(model, w, a, b);
            moved_end = Some(if end { w.end } else { w.start });
        }
        (ElementKind::Wall(w), HandleKind::LayerEnd { layer, end }) => {
            // out past the end is more, back along the wall less
            let along = d.truncate().dot(w.direction());
            let by = round(if end { along } else { -along }, step / 5.0);
            let old = w.layer_ext(layer)[usize::from(end)];
            let len = w.length();
            if old + by < -len / 2.0 {
                return Err("a layer can't be held back past the wall's middle".into());
            }
            w.set_layer_ext(layer, end, old + by);
        }
        (ElementKind::Wall(w), HandleKind::LayerHeight { layer, top }) => {
            // up is more at the top, down is more at the bottom
            let by = round(if top { d.z } else { -d.z }, step / 5.0);
            let old = w.layer_vext(layer)[usize::from(top)];
            if old + by < -w.height / 2.0 {
                return Err("a layer can't stop short past the wall's middle height".into());
            }
            w.set_layer_vext(layer, top, old + by);
        }
        (
            ek @ (ElementKind::Slab(_) | ElementKind::Roof(_)),
            HandleKind::LayerEdge { edge, .. },
        ) => {
            let boundary = match ek {
                ElementKind::Slab(s) => s.boundary.clone(),
                ElementKind::Roof(r) => r.boundary.clone(),
                _ => unreachable!(),
            };
            let out = edge_out(&boundary, edge);
            let by = round(d.truncate().dot(out), step / 5.0);
            let old = layer_offset(ek, kind).unwrap_or(0.0);
            set_layer_offset(ek, kind, old + by);
        }
        (ElementKind::Wall(w), HandleKind::WallTop) => match &mut w.top {
            // tied to a level: the drag sets how far above it the top is
            WallTop::UpToLevel { offset, .. } => {
                let h = round(w.height + d.z, step);
                if h < 0.1 {
                    return Err("too low".into());
                }
                *offset += h - w.height;
                w.height = h;
            }
            WallTop::Unconnected => {
                w.height = round(w.height + d.z, step);
                if w.height < 0.1 {
                    return Err("too low".into());
                }
            }
        },
        (ElementKind::Wall(w), HandleKind::WallFace) => {
            let n = w.direction().perp().extend(0.0);
            w.thickness = round(w.thickness + 2.0 * d.dot(n), step / 5.0);
            if w.thickness < 0.02 {
                return Err("too thin".into());
            }
        }
        (ElementKind::Slab(s), HandleKind::SlabUnderside) => {
            s.set_thickness(round(s.thickness - d.z, step / 5.0), false);
            if s.thickness < 0.02 {
                return Err("too thin".into());
            }
        }
        (ElementKind::Slab(s), HandleKind::SlabTop) => {
            s.set_thickness(round(s.thickness + d.z, step / 5.0), true);
            if s.thickness < 0.02 {
                return Err("too thin".into());
            }
        }
        (ElementKind::Opening(o), HandleKind::OpeningJamb { right }) => {
            let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                return Err("its wall is gone".into());
            };
            let along = round(d.truncate().dot(w.direction()), step);
            // a jamb stops at a wall joined at the corner (MU23)
            let (lo, hi) = creabim_core::walls::clear_span(model, o.host);
            let (lo, hi) = if o.offset >= lo - 1e-9 && o.offset + o.width <= hi + 1e-9 {
                (lo, hi)
            } else {
                (0.0, w.length())
            };
            if right {
                o.width = (o.width + along).min(hi - o.offset);
            } else {
                let along = along.max(lo - o.offset);
                o.offset += along;
                o.width -= along;
            }
            if o.width < 0.2 {
                return Err("too narrow".into());
            }
        }
        (ElementKind::Opening(o), HandleKind::OpeningHead) => {
            o.height = round(o.height + d.z, step);
        }
        (ElementKind::Opening(o), HandleKind::OpeningSill) => {
            let dz = round(d.z, step).clamp(-o.sill, o.height - 0.2);
            o.sill += dz;
            o.height -= dz;
        }
        (ElementKind::Grid(g), HandleKind::GridEnd { end }) => {
            if end {
                g.end = now.truncate();
            } else {
                g.start = now.truncate();
            }
        }
        (ElementKind::View(v), HandleKind::ViewEnd { end: which }) => {
            if let ViewKind::Section { start, end, .. } = &mut v.kind {
                let p = now.truncate();
                if which {
                    *end = p;
                } else {
                    *start = p;
                }
                if start.distance(*end) < 0.2 {
                    return Err("a view line needs some length".into());
                }
            }
        }
        (ElementKind::View(v), HandleKind::ViewDepth) => {
            if let ViewKind::Section {
                start, end, depth, ..
            } = &mut v.kind
            {
                let look = (*end - *start).normalize_or_zero().perp();
                let mid = (*start + *end) / 2.0;
                *depth = round((now.truncate() - mid).dot(look), step).max(0.5);
            }
        }
        (ElementKind::FreeMesh(m), HandleKind::Face(f)) => {
            let n = m.to_model(m.mesh.face_normal(f)).normalize_or_zero();
            let dist = round(d.dot(n), step / 5.0);
            let verts = m.mesh.faces[f].clone();
            let local = m.to_local(n * dist);
            m.mesh.translate_vertices(&verts, local);
        }
        (ElementKind::FreeMesh(m), HandleKind::MeshVertex(i)) => {
            let by = DVec3::new(round(d.x, step), round(d.y, step), 0.0);
            let local = m.to_local(by);
            m.mesh.translate_vertices(&[i as u32], local);
        }
        (ElementKind::FreeMesh(m), HandleKind::MeshEdge(a, b)) => {
            let by = DVec3::new(round(d.x, step), round(d.y, step), 0.0);
            let local = m.to_local(by);
            m.mesh.translate_vertices(&[a, b], local);
        }
        (ElementKind::FreeMesh(m), HandleKind::MeshMove) => {
            m.origin.x = round(m.origin.x + d.x, step);
            m.origin.y = round(m.origin.y + d.y, step);
        }
        (ElementKind::FreeMesh(m), HandleKind::MeshLift) => {
            m.origin.z = round(m.origin.z + d.z, step);
        }
        (ElementKind::CropBox(c), HandleKind::CropFace(i)) => {
            let axis = i / 2;
            let (at, _) = c.face(i);
            c.set_face(i, round(at[axis] + d[axis], step));
        }
        (ElementKind::Reference(r), HandleKind::RefMove) => {
            let dl = ref_local(model, r, now) - ref_local(model, r, grab);
            r.at = DVec2::new(round(r.at.x + dl.x, step), round(r.at.y + dl.y, step));
        }
        (ElementKind::Reference(r), HandleKind::RefScale(_)) => {
            let (g, n) = (
                ref_local(model, r, grab).distance(r.at),
                ref_local(model, r, now).distance(r.at),
            );
            if g < 1e-6 || n < 1e-3 {
                return Err("too close to its middle".into());
            }
            r.scale *= n / g;
        }
        (ElementKind::CropBox(c), HandleKind::CropMove) => {
            // the grabbed corner lands on a rounded spot
            let to = DVec3::new(
                round(c.min.x + d.x, step),
                round(c.min.y + d.y, step),
                c.min.z,
            );
            let by = to - c.min;
            c.min += by;
            c.max += by;
        }
        (ElementKind::Opening(o), HandleKind::OpeningSlide) => {
            let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                return Err("its wall is gone".into());
            };
            let along = round(d.truncate().dot(w.direction()), step);
            o.offset = (o.offset + along).clamp(0.0, (w.length() - o.width).max(0.0));
            creabim_core::walls::clamp_opening(model, o);
        }
        (ElementKind::Beam(b), HandleKind::BeamWidth) => {
            let n = (b.end - b.start).normalize_or_zero().perp();
            b.width = round(b.width + 2.0 * d.truncate().dot(n), step / 5.0).max(0.05);
        }
        (ElementKind::Detail(dt), HandleKind::DetailWidth | HandleKind::DetailSpacing) => {
            let frame = creabim_core::DrawingFrame::of(model, dt.view).ok_or("its view is gone")?;
            let moved = frame.to_drawing(now) - frame.to_drawing(grab);
            if kind == HandleKind::DetailWidth {
                let by = round(2.0 * moved.dot(dt.across()), step / 5.0);
                dt.width = (dt.width + by).max(0.001);
            } else if let (DetailKind::Repeat { module, .. }, [a, b, ..]) =
                (&mut dt.kind, dt.points.as_slice())
            {
                let along = (*b - *a).normalize_or_zero();
                *module = round(*module + moved.dot(along), step / 5.0).max(0.01);
            }
        }
        (ElementKind::View(v), HandleKind::CalloutCorner(_) | HandleKind::CalloutEdge(_)) => {
            let c = v.callout.as_mut().ok_or("not a callout")?;
            let frame =
                creabim_core::DrawingFrame::of(model, c.parent).ok_or("its view is gone")?;
            let to = frame.to_drawing(now);
            let moved = to - frame.to_drawing(grab);
            let r = |x: f64| round(x, step);
            let (mut lo, mut hi) = (c.lo, c.hi);
            match kind {
                HandleKind::CalloutCorner(i) => {
                    // the opposite corner stays where it is
                    let k = box_corners(lo, hi);
                    let stay = k[(i + 2) % 4];
                    let p = k[i] + DVec2::new(r(moved.x), r(moved.y));
                    lo = stay.min(p);
                    hi = stay.max(p);
                }
                HandleKind::CalloutEdge(0) => lo.y += r(moved.y),
                HandleKind::CalloutEdge(1) => hi.x += r(moved.x),
                HandleKind::CalloutEdge(2) => hi.y += r(moved.y),
                _ => lo.x += r(moved.x),
            }
            if hi.x - lo.x < 0.05 || hi.y - lo.y < 0.05 {
                return Err("a callout needs some size".into());
            }
            // a ringed callout (on a section, in the Classic family) stays
            // a circle: its box stays square
            if callout_is_round(model, c.parent) {
                let side = match kind {
                    HandleKind::CalloutEdge(0 | 2) => hi.y - lo.y,
                    HandleKind::CalloutEdge(_) => hi.x - lo.x,
                    _ => (hi - lo).max_element(),
                };
                match kind {
                    HandleKind::CalloutCorner(i) => {
                        let stay = box_corners(c.lo, c.hi)[(i + 2) % 4];
                        let away = (lo + hi) / 2.0 - stay;
                        let far = stay + DVec2::new(side * away.x.signum(), side * away.y.signum());
                        lo = stay.min(far);
                        hi = stay.max(far);
                    }
                    HandleKind::CalloutEdge(0 | 2) => {
                        let mid = (lo.x + hi.x) / 2.0;
                        lo.x = mid - side / 2.0;
                        hi.x = mid + side / 2.0;
                    }
                    _ => {
                        let mid = (lo.y + hi.y) / 2.0;
                        lo.y = mid - side / 2.0;
                        hi.y = mid + side / 2.0;
                    }
                }
            }
            c.lo = lo;
            c.hi = hi;
        }
        (ElementKind::Site(st), HandleKind::PlantCanopy | HandleKind::PlantHeight) => {
            let creabim_core::SiteForm::Plant { canopy, height, .. } = &mut st.form else {
                return Err("only plants have that grip".into());
            };
            if kind == HandleKind::PlantCanopy {
                *canopy = round(*canopy + 2.0 * d.x, step).max(0.1);
            } else {
                *height = round(*height + d.z, step).max(0.1);
            }
        }
        (ElementKind::Wall(w), HandleKind::WallBend) => {
            // through the point dragged to, its ends where they are; a
            // point on the straight line between them straightens it
            let through = now.truncate();
            w.bulge = crate::planview::bulge_through(w.start, through, w.end)
                .filter(|k| k.abs() < 20.0)
                .unwrap_or(0.0);
        }
        (ElementKind::Wall(w), HandleKind::WallSlide) => {
            let n = w.direction().perp();
            let by = n * round(d.truncate().dot(n), step);
            w.start += by;
            w.end += by;
        }
        (ElementKind::Column(c), HandleKind::ColumnSize { depth }) => {
            let x = DVec2::from_angle(c.rotation.to_radians());
            let axis = if depth { x.perp() } else { x };
            // both sides move: the column stays on its point
            let by = round(2.0 * d.truncate().dot(axis), step / 5.0);
            match &mut c.profile {
                creabim_core::members::Profile::Rectangle { width, depth: dp } => {
                    let v = if depth { dp } else { width };
                    *v = (*v + by).max(0.05);
                }
                creabim_core::members::Profile::Round { diameter } => {
                    *diameter = (*diameter + by).max(0.05);
                }
            }
        }
        (_, HandleKind::Rotate) => {
            let original = originals.first().ok_or("nothing to turn")?;
            let c = pivot_of(model, original).ok_or("this doesn't turn")?;
            let (a0, a1) = (grab.truncate() - c, now.truncate() - c);
            if a1.length() < 1e-6 {
                return Err("too close to its middle".into());
            }
            // in 5° steps, free with Alt or Step off, as lengths are
            let raw = a0.angle_to(a1).to_degrees();
            let by = if step > 0.0 { round(raw, 5.0) } else { raw };
            match &mut first.kind {
                ElementKind::Object(_) => turn_object(model, first, by),
                ElementKind::Column(col) => col.rotation = (col.rotation + by).rem_euclid(360.0),
                ElementKind::Stair(s) => s.direction = (s.direction + by).rem_euclid(360.0),
                ElementKind::FreeMesh(m) => m.rotation = (m.rotation + by).rem_euclid(360.0),
                ElementKind::Reference(r) => r.rotation = (r.rotation + by).rem_euclid(360.0),
                _ => return Err("this doesn't turn".into()),
            }
        }
        (_, HandleKind::Member { i, .. }) => {
            creabim_core::members::drag_grip(model, first, i, grab, now, step)?
        }
        (
            _,
            HandleKind::Vertex { .. }
            | HandleKind::Edge { .. }
            | HandleKind::AddVertex { .. }
            | HandleKind::Ring { .. }
            | HandleKind::ShaftBase
            | HandleKind::ShaftTop,
        ) => crate::editmode::reshape(model, first, kind, grab, now, step)?,
        _ => return Err("this handle doesn't fit this element".into()),
    }
    let opening_ok = match &first.kind {
        ElementKind::Opening(o) => match model.get(o.host).map(|h| &h.kind) {
            Some(ElementKind::Wall(w)) => o.sill + o.height <= w.height + 1e-9,
            _ => true,
        },
        _ => true,
    };
    if !opening_ok {
        return Err("taller than its wall".into());
    }
    if let Some(p) = moved_end {
        for &(i, oe) in joined {
            if let ElementKind::Wall(o) = &mut out[i].kind {
                if oe {
                    o.end = p;
                } else {
                    o.start = p;
                }
            }
        }
    }
    for e in &out {
        e.validate()?;
    }
    Ok(out)
}

/// How far a handle has moved, for the status bar.
pub fn readout(changed: &[Element]) -> String {
    match changed.first().map(|e| &e.kind) {
        Some(ElementKind::Wall(w)) => format!(
            "length {}, height {}, thickness {}",
            crate::units::len(w.length()),
            crate::units::len(w.height),
            crate::units::len(w.thickness)
        ),
        Some(ElementKind::Opening(o)) => format!(
            "width {}, height {}, sill {}",
            crate::units::len(o.width),
            crate::units::len(o.height),
            crate::units::len(o.sill)
        ),
        Some(ElementKind::View(v)) if v.callout.is_some() => v
            .callout
            .map(|c| {
                format!(
                    "its box {} × {}",
                    crate::units::len(c.hi.x - c.lo.x),
                    crate::units::len(c.hi.y - c.lo.y)
                )
            })
            .unwrap_or_default(),
        Some(ElementKind::View(v)) => match v.kind {
            ViewKind::Section {
                start, end, depth, ..
            } => format!(
                "line {} long, sees {} deep",
                crate::units::len(start.distance(end)),
                crate::units::len(depth)
            ),
            _ => String::new(),
        },
        Some(ElementKind::Slab(s)) => format!(
            "area {}, thickness {}, top {}, underside {} from its level",
            crate::units::area(s.area()),
            crate::units::len(s.thickness),
            crate::units::elevation(s.top_offset),
            s.top_offset - s.thickness
        ),
        Some(ElementKind::Column(c)) => match c.height {
            Some(h) => format!(
                "{}, height {}",
                c.profile.label_in(&crate::units::get()),
                crate::units::len(h)
            ),
            None => c.profile.label_in(&crate::units::get()),
        },
        Some(ElementKind::Beam(b)) => format!(
            "length {}, {} × {}",
            crate::units::len(b.length()),
            crate::units::len(b.width),
            crate::units::len(b.depth)
        ),
        Some(ElementKind::Roof(r)) => format!(
            "{} roof, pitch {:.1}°, {} with overhang",
            r.effective_shape().label(),
            r.pitch,
            crate::units::area(r.plan_area())
        ),
        Some(ElementKind::Stair(s)) => {
            format!(
                "width {}, treads {}",
                crate::units::len(s.width),
                crate::units::len(s.tread)
            )
        }
        Some(ElementKind::Railing(r)) => format!("height {}", crate::units::len(r.height)),
        Some(ElementKind::Shaft(s)) => format!(
            "{}, {} corners",
            crate::units::area(creabim_core::polygon::signed_area2(&s.boundary).abs() / 2.0),
            s.boundary.len()
        ),
        Some(ElementKind::CropBox(c)) => {
            let s = c.size();
            format!(
                "crop box {} × {} × {}",
                crate::units::len(s.x),
                crate::units::len(s.y),
                crate::units::len(s.z)
            )
        }
        _ => String::new(),
    }
}

pub type SnapFn<'a> = dyn Fn(Pos2, Option<DVec3>) -> Option<(DVec3, Snap)> + 'a;

/// How a pane turns pointer positions into the model, for one frame.
pub struct PaneInput<'a> {
    /// The pointer as a ray into the model.
    pub ray: &'a dyn Fn(Pos2) -> Option<Ray>,
    /// A model point on screen.
    pub project: &'a dyn Fn(DVec3) -> Option<Pos2>,
    /// Which way the pane looks, to hide handles seen end on.
    pub look: DVec3,
    /// The best snap near the pointer as a model point, with the line's
    /// start for perpendicular and axis snaps. The pane leaves the dragged
    /// elements out.
    pub snap: &'a SnapFn<'a>,
    /// The point of `id` under the pointer, if it is there: grabbing the
    /// selected element anywhere but a handle moves it.
    pub grab_body: &'a dyn Fn(Pos2, ElementId) -> Option<DVec3>,
    /// A corner, middle or centre of `id` itself near the pointer: a drag
    /// that starts there moves the element by that point, so it lands
    /// exactly on what it snaps to.
    pub grab_snap: &'a dyn Fn(Pos2, ElementId) -> Option<DVec3>,
    /// The plane a grabbed element slides on: level in plans and 3D, the
    /// section plane in sections.
    pub body_normal: DVec3,
    /// The view the pane shows: drafting has grips only in its own.
    pub view: Option<ElementId>,
    /// The face under the pointer and its normal, leaving out what is
    /// being edited: 3D only (plans and sections give none). A grip
    /// sliding along a line stops where it meets it.
    pub face: &'a dyn Fn(Pos2) -> Option<(DVec3, DVec3)>,
}

/// A handle or a whole element being dragged.
#[derive(Debug)]
pub struct Drag {
    /// `None` moves the whole element.
    kind: Option<HandleKind>,
    constraint: Constraint,
    edit: LiveEdit,
    joined: Vec<(usize, bool)>,
    grab: DVec3,
    /// Where the press was on screen: nothing moves until the pointer
    /// leaves it.
    press: Pos2,
    /// Where the pointer last put the grip, in the model.
    last: DVec3,
    /// A length being typed (Tab, or a digit, starts it): the grip goes
    /// that far from where it was grabbed, the way the pointer went.
    typed: Option<String>,
    /// The pane (its interaction id) the drag started in. Every pane runs
    /// [`interact`] with the one shared drag; only this one may go on with
    /// it or end it. Without this, a second area showing a plan saw "my
    /// pane isn't dragged" and ended the drag after its first frame, so 2D
    /// drags only moved at the press and at the release.
    owner: egui::Id,
}

/// The corners and holes picked in Edit mode, on the element being
/// edited, and what a pane is doing with them: a box over the grips, the
/// right-click menu of a corner, an edge or a hole. Shared by every pane.
#[derive(Debug, Default)]
pub struct GripSel {
    /// The element the picks are on.
    pub id: Option<ElementId>,
    pub corners: Vec<(Ring, usize)>,
    pub holes: Vec<usize>,
    /// A mass's picked vertices in Edit mode (an edge picks its two ends,
    /// a face its corners), kept lit until another pick (MU39).
    pub mesh: Vec<u32>,
    /// The last click on a grip: which, when and where, so a double-click
    /// still counts when the pointer moved a little between the clicks.
    last_tap: Option<(HandleKind, f64, Pos2)>,
    /// A box being dragged over the grips: the pane and where it began.
    boxsel: Option<(egui::Id, Pos2)>,
    /// The right-click menu, open in one pane.
    menu: Option<GripMenu>,
    /// A length is being typed into a drag: keys are the drag's.
    pub typing: bool,
    /// What the app should do next with Edit mode.
    pub request: Option<GripRequest>,
    /// The small palette on the outline edge under the pointer (3.6).
    palette: Option<EdgePalette>,
}

/// The palette on a selected element's outline edge: Offset edge, Offset
/// all, Insert point.
#[derive(Clone, Copy, Debug)]
struct EdgePalette {
    pane: egui::Id,
    ring: Ring,
    i: usize,
    /// The model point the pointer was at on the edge.
    at: DVec3,
    /// Where it shows, on screen.
    pos: Pos2,
    /// When the pointer came to the edge (seconds): it shows after a rest.
    since: f64,
    /// Where it was drawn last frame, to keep it while the pointer is on it.
    rect: Option<Rect>,
}

/// How long (seconds) the pointer rests on an edge before its palette shows.
const PALETTE_REST: f64 = 0.35;

/// Something only the app can do, asked for from a grip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GripRequest {
    /// Leave Edit mode.
    Finish,
    /// Open the selected element in Edit mode.
    Enter,
    /// From the edge palette: push edge `i` of outline `ring` out or in,
    /// following the pointer.
    OffsetEdge(Ring, usize),
    /// From the edge palette: offset the whole outline, in place.
    OffsetAll,
    /// From the edge palette: a corner on edge `i` of `ring` at this
    /// point, then moved with the pointer.
    InsertPoint(Ring, usize, DVec3),
}

#[derive(Clone, Copy, Debug)]
struct GripMenu {
    pane: egui::Id,
    at: Pos2,
    on: MenuOn,
}

/// What a right-click in Edit mode was on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MenuOn {
    Corner(Ring, usize),
    /// An edge, and the model point clicked on it.
    Edge(Ring, usize, DVec3),
    Hole(usize),
    /// A layer's start, end or edge, in Layers.
    Layer(HandleKind),
    Blank,
}

/// Two clicks on the same grip this close in time (seconds) and on
/// screen (pixels) are a double-click.
const DOUBLE_TAP: f64 = 0.5;
const DOUBLE_TAP_PX: f32 = 8.0;

thread_local! {
    /// The mass vertices picked in Edit mode, for Move, Rotate and Scale.
    static MESH_PICK: std::cell::RefCell<Option<(ElementId, Vec<u32>)>> =
        const { std::cell::RefCell::new(None) };
}

/// The vertices picked on mass `id` in Edit mode, if any.
pub fn mesh_pick(id: ElementId) -> Option<Vec<u32>> {
    MESH_PICK.with(|p| {
        p.borrow()
            .as_ref()
            .filter(|(i, v)| *i == id && !v.is_empty())
            .map(|(_, v)| v.clone())
    })
}

impl GripSel {
    pub fn is_empty(&self) -> bool {
        self.corners.is_empty() && self.holes.is_empty() && self.mesh.is_empty()
    }

    /// Pick a mass's vertices `vs` (a vertex, an edge's ends, a face's
    /// corners): only them, or with `add` (Shift) in, or out if they were
    /// all in already.
    pub fn pick_mesh(&mut self, vs: &[u32], add: bool) {
        if !add {
            self.mesh = vs.to_vec();
            self.corners.clear();
            self.holes.clear();
        } else if vs.iter().all(|v| self.mesh.contains(v)) {
            self.mesh.retain(|v| !vs.contains(v));
        } else {
            for v in vs {
                if !self.mesh.contains(v) {
                    self.mesh.push(*v);
                }
            }
        }
        MESH_PICK.with(|p| *p.borrow_mut() = self.id.map(|id| (id, self.mesh.clone())));
    }

    /// Drop the picks, and any box or menu. The last click is kept, so a
    /// double-click still counts while this runs every frame.
    pub fn clear(&mut self) {
        self.corners.clear();
        self.holes.clear();
        self.mesh.clear();
        MESH_PICK.with(|p| *p.borrow_mut() = None);
        self.boxsel = None;
        self.menu = None;
    }

    /// Start afresh on element `id` (or none).
    pub fn reset(&mut self, id: Option<ElementId>) {
        if self.id != id {
            self.clear();
            self.last_tap = None;
            self.palette = None;
            self.id = id;
        }
    }

    pub fn picked(&self, ring: Ring, i: usize) -> bool {
        self.corners.contains(&(ring, i))
    }

    /// Pick corner `i` of `ring`: only it, or with `add` (Shift) in or out
    /// of the picks.
    pub fn pick(&mut self, ring: Ring, i: usize, add: bool) {
        if add {
            if let Some(k) = self.corners.iter().position(|c| *c == (ring, i)) {
                self.corners.remove(k);
            } else {
                self.corners.push((ring, i));
            }
        } else {
            self.corners = vec![(ring, i)];
            self.holes.clear();
        }
    }

    /// Pick hole `k`, as [`GripSel::pick`] does a corner.
    pub fn pick_hole(&mut self, k: usize, add: bool) {
        if add {
            if let Some(j) = self.holes.iter().position(|h| *h == k) {
                self.holes.remove(j);
            } else {
                self.holes.push(k);
            }
        } else {
            self.holes = vec![k];
            self.corners.clear();
        }
    }

    /// Is a box or a menu open?
    pub fn busy(&self) -> bool {
        self.boxsel.is_some() || self.menu.is_some()
    }

    /// A click on grip `kind` at `t` (seconds) and `at` (screen): is it
    /// the second of a double-click?
    fn tap(&mut self, kind: HandleKind, t: f64, at: Pos2) -> bool {
        let double = self.last_tap.is_some_and(|(k, t0, p)| {
            k == kind && t - t0 < DOUBLE_TAP && p.distance(at) < DOUBLE_TAP_PX
        });
        self.last_tap = if double { None } else { Some((kind, t, at)) };
        double
    }
}

/// Delete or Backspace in Edit mode: remove the picked holes, else the
/// picked corners, in one undo step. Never the element itself.
pub fn delete_picked(
    doc: &mut Document,
    id: ElementId,
    sel: &mut GripSel,
) -> Result<String, String> {
    if sel.id != Some(id) || sel.is_empty() {
        return Err(
            "Select a corner to remove it: click it (Shift+click or drag a box for more), or click inside a hole to remove the hole. Esc finishes editing."
                .into(),
        );
    }
    let r = if sel.holes.is_empty() {
        crate::editmode::remove_corners(doc, id, &sel.corners)
    } else {
        crate::editmode::remove_holes(doc, id, &sel.holes)
    };
    if r.is_ok() {
        sel.clear();
    }
    r
}

fn interaction_ink() -> Color32 {
    crate::theme::interaction()
}
/// The colour of a shape that would be broken.
const INVALID: Color32 = Color32::from_rgb(210, 40, 40);

/// A short note on a light plate beside the pointer: what a grip does,
/// a live length, a typed one.
fn note(painter: &Painter, at: Pos2, text: &str, color: Color32) {
    let font = egui::FontId::proportional(12.0);
    let galley = painter.layout_no_wrap(text.to_owned(), font, color);
    let mut pos = at + Vec2::new(16.0, 14.0);
    let clip = painter.clip_rect();
    if pos.x + galley.size().x + 8.0 > clip.right() {
        pos.x = at.x - galley.size().x - 16.0;
    }
    if pos.y + galley.size().y + 6.0 > clip.bottom() {
        pos.y = at.y - galley.size().y - 18.0;
    }
    let rect = Rect::from_min_size(pos, galley.size()).expand2(Vec2::new(5.0, 3.0));
    painter.rect(
        rect,
        3.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 235),
        Stroke::new(1.0, color.gamma_multiply(0.6)),
        egui::StrokeKind::Middle,
    );
    painter.galley(pos, galley, color);
}

/// Under this many pixels across on screen, a wall shows its thickness
/// grip only when the pointer comes near its face.
const THIN_PX: f32 = 14.0;

/// How thick wall `id` is on screen, in pixels.
fn wall_px(model: &Model, id: ElementId, input: &PaneInput) -> Option<f32> {
    let ElementKind::Wall(w) = &model.get(id)?.kind else {
        return None;
    };
    let z = elevation(model, Some(w.level)) + w.base_offset;
    let mid = (w.start + w.end) / 2.0;
    let n = w.direction().perp() * w.thickness / 2.0;
    let a = (input.project)((mid + n).extend(z))?;
    let b = (input.project)((mid - n).extend(z))?;
    Some(a.distance(b))
}

/// How long a let-go grip's value shows, seconds: about one, the last
/// third fading.
const FLASH: f64 = 1.3;

/// The values a grip can change on an element, by name, in metres.
pub fn values(e: &Element) -> Vec<(&'static str, f64)> {
    match &e.kind {
        ElementKind::Wall(w) => vec![
            ("Length", w.length()),
            ("Height", w.height),
            ("Thickness", w.thickness),
        ],
        ElementKind::Opening(o) => vec![
            ("Width", o.width),
            ("Height", o.height),
            ("Sill", o.sill),
            ("Offset", o.offset),
        ],
        ElementKind::View(v) if v.callout.is_some() => v
            .callout
            .map(|c| vec![("Width", c.hi.x - c.lo.x), ("Height", c.hi.y - c.lo.y)])
            .unwrap_or_default(),
        ElementKind::View(v) => match v.kind {
            ViewKind::Section {
                start, end, depth, ..
            } => vec![("Length", start.distance(end)), ("Depth", depth)],
            _ => vec![],
        },
        ElementKind::Slab(s) => vec![("Thickness", s.thickness), ("Top", s.top_offset)],
        ElementKind::Column(c) => c.height.map(|h| vec![("Height", h)]).unwrap_or_default(),
        ElementKind::Beam(b) => vec![("Length", b.start.distance(b.end))],
        ElementKind::Detail(d) => {
            let mut out = vec![("Length", d.length()), ("Width", d.width)];
            if let DetailKind::Repeat { module, .. } = d.kind {
                out.push(("Unit", module));
            }
            out
        }
        ElementKind::Site(s) => match s.form {
            creabim_core::SiteForm::Plant { canopy, height, .. } => {
                vec![("Canopy", canopy), ("Height", height)]
            }
            _ => vec![],
        },
        _ => vec![],
    }
}

/// A length's number alone, in the project's units: `7200`, `3'-6"`.
fn bare(metres: f64) -> String {
    let s = crate::units::len(metres);
    let unit = format!(" {}", crate::units::symbol());
    s.strip_suffix(&unit).map(str::to_string).unwrap_or(s)
}

/// The one value a drag is changing, as the pill shows it: its name, then
/// "old → ", then the new value and the change, which the pill draws in
/// the accent. `None` when nothing named has changed.
pub fn changing(old: &Element, new: &Element) -> Option<(String, String)> {
    let (a, b) = (values(old), values(new));
    let (name, was, now) = a
        .iter()
        .zip(&b)
        .filter(|((n, _), (m, _))| n == m)
        .map(|((n, x), (_, y))| (*n, *x, *y))
        .filter(|(_, x, y)| (y - x).abs() > 1e-6)
        .max_by(|p, q| (p.2 - p.1).abs().total_cmp(&(q.2 - q.1).abs()))?;
    let d = now - was;
    let sign = if d >= 0.0 { "+" } else { "−" };
    Some((
        format!("{name} {} → ", bare(was)),
        format!("{} {sign}{}", bare(now), bare(d.abs())),
    ))
}

/// How far an element moved between `old` and `new`, by a point of its
/// own (a wall's start, a column's base, ...), so the pill says the
/// rounded distance the model took, not the pointer's.
fn moved_by(old: Option<&Element>, new: Option<&Element>) -> Option<f64> {
    let point = |e: &Element| match &e.kind {
        ElementKind::Wall(w) => Some(w.start),
        ElementKind::Column(c) => Some(c.position),
        ElementKind::Beam(b) => Some(b.start),
        _ => None,
    };
    Some(point(old?)?.distance(point(new?)?))
}

/// Are live dimensions (the pill while dragging) on? D switches them.
pub fn live_on(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("grip.live")))
        .unwrap_or(true)
}

/// D, outside a text field: live dimensions off or on. True when it
/// switched them.
pub fn toggle_live(ctx: &egui::Context) -> bool {
    if ctx.egui_wants_keyboard_input() {
        return false;
    }
    // consume_key ignores Shift, which took Shift+D (Duplicate) for D
    let pressed =
        ctx.input_mut(|i| !i.modifiers.shift && i.consume_key(egui::Modifiers::NONE, egui::Key::D));
    if pressed {
        let on = !live_on(ctx);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("grip.live"), on));
    }
    pressed
}

/// The four states of a grip, the same on every element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GripState {
    Rest,
    /// Under the pointer: it grows a little and says what it is.
    Hover,
    /// Held: a white core and a soft halo.
    Held,
    /// Another grip is held: this one fades back.
    Faded,
}

/// The colours grips and their pills take from the theme.
struct GripLook {
    accent: Color32,
    /// A picked corner's outer ring.
    ink: Color32,
    paper: Color32,
    pill: PillLook,
}

/// A value pill's colours: a soft raised surface with a line round it,
/// the theme's text, and the changing number in the theme's accent, made
/// legible (4.5:1) on that surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PillLook {
    pub ground: Color32,
    pub line: Color32,
    pub text: Color32,
    pub number: Color32,
}

impl PillLook {
    pub fn of(p: &crate::theme::Palette) -> Self {
        Self {
            ground: p.raise,
            line: p.edge,
            text: crate::theme::legible(
                p.text,
                p.raise,
                if p.dark {
                    Color32::WHITE
                } else {
                    Color32::BLACK
                },
            ),
            number: crate::theme::legible(
                p.accent,
                p.raise,
                if p.dark {
                    Color32::WHITE
                } else {
                    Color32::BLACK
                },
            ),
        }
    }
}

impl GripLook {
    fn of(ctx: &egui::Context) -> Self {
        let v = ctx.global_style().visuals.clone();
        Self {
            accent: interaction_ink(),
            ink: v.strong_text_color(),
            paper: v.panel_fill,
            pill: PillLook::of(&crate::theme::Palette::of(ctx)),
        }
    }
}

/// A grip at `c`: a square, or a bar across `bar` (a screen direction;
/// `Some(None)` is a bar seen end on, drawn as a dot).
/// How grips are drawn: a personal setting (brief 02, 2.13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GripStyle {
    /// Squares at corners and ends, short bars on edges.
    #[default]
    Quiet,
    /// Almost nothing at rest: dots at corners; an edge's bar shows under
    /// the pointer.
    Edges,
    /// The same small circle everywhere.
    OneShape,
}

impl GripStyle {
    pub const ALL: [GripStyle; 3] = [GripStyle::Quiet, GripStyle::Edges, GripStyle::OneShape];

    pub fn label(self) -> &'static str {
        match self {
            GripStyle::Quiet => "Quiet",
            GripStyle::Edges => "Edges are the handles",
            GripStyle::OneShape => "One grip shape",
        }
    }
}

static GRIP_STYLE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Draw grips this way from now on.
static EDGE_W: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);
static VERT_S: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x4080_0000);

/// Edit mode's wire width and vertex dot size, in points (MU39).
pub fn set_edit_display(edge: f32, vertex: f32) {
    EDGE_W.store(edge.to_bits(), std::sync::atomic::Ordering::Relaxed);
    VERT_S.store(vertex.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn edit_display() -> (f32, f32) {
    (
        f32::from_bits(EDGE_W.load(std::sync::atomic::Ordering::Relaxed)),
        f32::from_bits(VERT_S.load(std::sync::atomic::Ordering::Relaxed)),
    )
}

pub fn set_grip_style(s: GripStyle) {
    GRIP_STYLE.store(s as u8, std::sync::atomic::Ordering::Relaxed);
}

pub fn grip_style() -> GripStyle {
    match GRIP_STYLE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => GripStyle::Edges,
        2 => GripStyle::OneShape,
        _ => GripStyle::Quiet,
    }
}

/// An object's turn grip: a round handle with a curved arrow, on a thin
/// leader from `front` (its front edge on screen).
fn draw_turn_grip(
    painter: &Painter,
    c: Pos2,
    front: Option<Pos2>,
    state: GripState,
    look: &GripLook,
) {
    let accent = if state == GripState::Faded {
        look.accent.gamma_multiply(0.4)
    } else {
        look.accent
    };
    let r = match state {
        GripState::Hover | GripState::Held => 9.5,
        _ => 8.0,
    };
    if let Some(f) = front {
        if f.distance(c) > r + 2.0 {
            let tip = c + (f - c).normalized() * r;
            painter.line_segment([f, tip], Stroke::new(1.0, accent.gamma_multiply(0.7)));
        }
    }
    let fill = if state == GripState::Held {
        accent
    } else {
        Color32::WHITE
    };
    let ink = if state == GripState::Held {
        Color32::WHITE
    } else {
        accent
    };
    painter.circle(c, r, fill, Stroke::new(1.5, accent));
    // three quarters of a circle and an arrowhead at its end
    let a = r * 0.5;
    let steps = 14;
    let (from, to) = (-0.5_f32, 4.2_f32);
    let pts: Vec<Pos2> = (0..=steps)
        .map(|k| {
            let t = from + (to - from) * k as f32 / steps as f32;
            c + Vec2::new(t.cos(), -t.sin()) * a
        })
        .collect();
    painter.add(egui::Shape::line(pts.clone(), Stroke::new(1.5, ink)));
    let end = *pts.last().unwrap();
    let along = (end - pts[pts.len() - 2]).normalized();
    let side = along.rot90();
    let head = 3.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            end + along * head,
            end - along * 1.0 + side * head,
            end - along * 1.0 - side * head,
        ],
        ink,
        Stroke::NONE,
    ));
}

fn draw_grip(
    painter: &Painter,
    c: Pos2,
    bar: Option<Option<Vec2>>,
    state: GripState,
    filled: bool,
    picked: bool,
    look: &GripLook,
) {
    let alpha = if state == GripState::Faded { 0.4 } else { 1.0 };
    let accent = look.accent.gamma_multiply(alpha);
    let grow = match state {
        GripState::Hover => 2.0,
        GripState::Held => 1.0,
        _ => 0.0,
    };
    if state == GripState::Held {
        painter.circle_filled(c, 12.0, look.accent.gamma_multiply(0.18));
    }
    let core = if state == GripState::Held {
        Color32::WHITE
    } else if filled || picked || bar.is_some() {
        accent
    } else {
        Color32::WHITE.gamma_multiply(alpha)
    };
    match grip_style() {
        GripStyle::Quiet => {}
        GripStyle::Edges => {
            match bar {
                // an edge shows only under the pointer, long and heavy
                Some(Some(dir)) => {
                    if matches!(state, GripState::Hover | GripState::Held) {
                        let len = 14.0;
                        painter
                            .line_segment([c - dir * len, c + dir * len], Stroke::new(4.0, accent));
                    }
                }
                _ => {
                    let r = if filled || picked { 3.5 } else { 2.5 } + grow;
                    painter.circle_filled(c, r, accent);
                }
            }
            return;
        }
        GripStyle::OneShape => {
            let r = 4.0 + grow;
            let fill = if filled || picked || state == GripState::Held {
                accent
            } else {
                Color32::WHITE.gamma_multiply(alpha)
            };
            painter.circle(c, r, fill, Stroke::new(1.5, accent));
            return;
        }
    }
    match bar {
        Some(Some(dir)) => {
            // a short rounded bar across the way it moves
            let len = 6.0 + grow;
            let a = c - dir * len;
            let b = c + dir * len;
            painter.line_segment([a, b], Stroke::new(5.0 + grow, accent));
            for p in [a, b] {
                painter.circle_filled(p, (5.0 + grow) / 2.0, accent);
            }
            if state == GripState::Held {
                painter.line_segment([a, b], Stroke::new(2.0, core));
            }
        }
        Some(None) => {
            painter.circle(c, 3.5 + grow, core, Stroke::new(1.5, accent));
        }
        None => {
            let side = 8.0 + 2.0 * grow;
            painter.rect(
                Rect::from_center_size(c, Vec2::splat(side)),
                1.0,
                core,
                Stroke::new(1.5, accent),
                egui::StrokeKind::Middle,
            );
            if picked {
                painter.rect_stroke(
                    Rect::from_center_size(c, Vec2::splat(side + 5.0)),
                    2.0,
                    Stroke::new(1.0, look.ink.gamma_multiply(0.7)),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }
}

/// The "+" that pulls a new corner out of an edge in Edit mode.
fn draw_plus(painter: &Painter, c: Pos2, state: GripState, look: &GripLook) {
    let r = if state == GripState::Rest { 5.5 } else { 7.0 };
    let fill = if state == GripState::Held {
        Color32::WHITE
    } else {
        look.paper
    };
    painter.circle(c, r, fill, Stroke::new(1.5, look.accent));
    let s = Stroke::new(1.5, look.accent);
    painter.line_segment([c - Vec2::X * (r - 2.5), c + Vec2::X * (r - 2.5)], s);
    painter.line_segment([c - Vec2::Y * (r - 2.5), c + Vec2::Y * (r - 2.5)], s);
}

/// A pill with its top-left corner at `at`, in a pill's look: a hint by
/// the pointer.
pub fn plain_pill(painter: &Painter, at: Pos2, text: &str, p: &PillLook) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.5), p.text);
    let size = galley.size() + Vec2::new(14.0, 6.0);
    let mut rect = Rect::from_min_size(at, size);
    let clip = painter.clip_rect().shrink(4.0);
    rect = rect.translate(Vec2::new(
        (clip.right() - rect.right()).min(0.0),
        (clip.bottom() - rect.bottom()).min(0.0),
    ));
    painter.rect(
        rect,
        6.0,
        p.ground,
        Stroke::new(1.0, p.line),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + Vec2::new(7.0, 3.0), galley, p.text);
}

/// A small pill centred under `at`: `text` in the theme's text colour,
/// then `number` in the accent.
fn pill(painter: &Painter, at: Pos2, text: &str, look: &GripLook, alpha: f32) {
    pill_value(painter, at, text, "", look, alpha);
}

/// A value pill: "Length 6600 → " then "7200 +600" in the accent, on the
/// theme's raised surface with a 1 px line, 11.5 px, 6 px corners.
fn pill_value(painter: &Painter, at: Pos2, text: &str, number: &str, look: &GripLook, alpha: f32) {
    let p = look.pill;
    let font = egui::FontId::proportional(11.5);
    // the UI font has no arrow: "old → new" draws its arrow as a stroke
    let (text, arrow) = match text.strip_suffix(" → ") {
        Some(t) => (t, true),
        None => (text, false),
    };
    let gap = if arrow { 18.0 } else { 0.0 };
    let mut job = egui::text::LayoutJob::default();
    let fmt = |c: Color32| egui::TextFormat {
        font_id: font.clone(),
        color: c.gamma_multiply(alpha),
        ..Default::default()
    };
    job.append(text, 0.0, fmt(p.text));
    let first = painter.layout_job(job.clone()).size().x;
    job.append(number, gap, fmt(p.number));
    let galley = painter.layout_job(job);
    let size = galley.size() + Vec2::new(14.0, 6.0);
    let mut rect = Rect::from_center_size(at + Vec2::new(0.0, size.y / 2.0), size);
    let clip = painter.clip_rect().shrink(4.0);
    rect = rect.translate(Vec2::new(
        (clip.left() - rect.left()).max(0.0) + (clip.right() - rect.right()).min(0.0),
        (clip.top() - rect.top()).max(0.0) + (clip.bottom() - rect.bottom()).min(0.0),
    ));
    painter.rect(
        rect,
        6.0,
        p.ground.gamma_multiply(alpha),
        Stroke::new(1.0, p.line.gamma_multiply(alpha)),
        egui::StrokeKind::Inside,
    );
    let y = rect.center().y;
    painter.galley(rect.min + Vec2::new(7.0, 3.0), galley, p.text);
    if arrow {
        let x0 = rect.min.x + 7.0 + first + 4.0;
        let (a, b) = (Pos2::new(x0, y), Pos2::new(x0 + gap - 8.0, y));
        let s = Stroke::new(1.2, p.text.gamma_multiply(alpha));
        painter.line_segment([a, b], s);
        painter.line_segment([b, b + Vec2::new(-3.0, -3.0)], s);
        painter.line_segment([b, b + Vec2::new(-3.0, 3.0)], s);
    }
}

/// The two-way pointer for a grip that moves along screen direction `d`.
fn resize_cursor(d: Vec2) -> egui::CursorIcon {
    let a = d.y.atan2(d.x).to_degrees().rem_euclid(180.0);
    if !(22.5..157.5).contains(&a) {
        egui::CursorIcon::ResizeHorizontal
    } else if (67.5..112.5).contains(&a) {
        egui::CursorIcon::ResizeVertical
    } else if a < 90.0 {
        egui::CursorIcon::ResizeNwSe
    } else {
        egui::CursorIcon::ResizeNeSw
    }
}

/// The outlines of `e` on screen, each with its ring and corners.
fn screen_rings(
    model: &Model,
    e: &Element,
    input: &PaneInput,
) -> Vec<(crate::editmode::RingInfo, Vec<Pos2>)> {
    crate::editmode::rings(model, e)
        .into_iter()
        .filter_map(|r| {
            let pts: Option<Vec<Pos2>> = r.corners.iter().map(|p| (input.project)(*p)).collect();
            Some((r, pts?))
        })
        .collect()
}

fn in_screen_polygon(pts: &[Pos2], p: Pos2) -> bool {
    let poly: Vec<DVec2> = pts
        .iter()
        .map(|q| DVec2::new(q.x as f64, q.y as f64))
        .collect();
    pts.len() >= 3 && creabim_core::polygon::contains(&poly, DVec2::new(p.x as f64, p.y as f64))
}

/// How far `p` is from segment `a`–`b` on screen, and where along it (0 to 1).
fn to_segment(a: Pos2, b: Pos2, p: Pos2) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    ((a + ab * t).distance(p), t)
}

/// The hole of `e` whose inside `p` (on screen) is in.
fn hole_at(model: &Model, e: &Element, input: &PaneInput, p: Pos2) -> Option<usize> {
    screen_rings(model, e, input)
        .into_iter()
        .find_map(|(r, pts)| match r.ring {
            Ring::Hole(k) if in_screen_polygon(&pts, p) => Some(k),
            _ => None,
        })
}

/// The edge of `e`'s outlines under `p` (within a few pixels), and the
/// model point there.
fn edge_at(model: &Model, e: &Element, input: &PaneInput, p: Pos2) -> Option<(Ring, usize, DVec3)> {
    let mut best: Option<(f32, Ring, usize, DVec3)> = None;
    for (r, pts) in screen_rings(model, e, input) {
        let n = pts.len();
        let edges = if r.closed { n } else { n.saturating_sub(1) };
        for i in 0..edges {
            let (d, t) = to_segment(pts[i], pts[(i + 1) % n], p);
            if d < 6.0 && best.is_none_or(|b| d < b.0) {
                let at = r.corners[i].lerp(r.corners[(i + 1) % n], t as f64);
                best = Some((d, r.ring, i, at));
            }
        }
    }
    best.map(|(_, r, i, at)| (r, i, at))
}

/// The edge palette (3.6) of selected element `e` in this pane: after the
/// pointer rests on an edge of its outline, three choices by the pointer,
/// which ask the app through `sel.request`. It stays while the pointer is
/// on it. `corner`: a corner grip is under the pointer, which wins.
/// Returns a status line when it shows.
fn edge_palette(
    model: &Model,
    e: &Element,
    ctx: &egui::Context,
    response: &egui::Response,
    input: &PaneInput,
    sel: &mut GripSel,
    corner: bool,
) -> Option<String> {
    let pane = response.id;
    // the palette covers the pane, so the pane itself no longer hovers
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    let now = ctx.input(|i| i.time);
    let mine = sel.palette.filter(|p| p.pane == pane);
    let on_palette = |p: &EdgePalette| {
        p.rect
            .zip(pointer)
            .is_some_and(|(r, at)| r.expand(10.0).contains(at))
    };
    match mine {
        Some(p) if on_palette(&p) => {}
        _ => {
            let edge = pointer
                .filter(|p| response.rect.contains(*p) && !corner)
                .and_then(|p| Some((p, edge_at(model, e, input, p)?)));
            sel.palette = match (edge, mine) {
                (Some((_, (ring, i, _))), Some(p)) if p.ring == ring && p.i == i => Some(p),
                (Some((pos, (ring, i, at))), _) => Some(EdgePalette {
                    pane,
                    ring,
                    i,
                    at,
                    pos,
                    since: now,
                    rect: None,
                }),
                // another pane's stays for that pane
                (None, None) => sel.palette.filter(|p| p.pane != pane),
                (None, Some(_)) => None,
            };
        }
    }
    let mut p = sel.palette.filter(|p| p.pane == pane)?;
    if now - p.since < PALETTE_REST {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(PALETTE_REST));
        return None;
    }
    let info = crate::editmode::ring_of(model, e, p.ring);
    let shown = handles_in(model, e.id, None, true);
    let can_edge = shown.iter().any(|h| {
        h.kind
            == HandleKind::Edge {
                ring: p.ring,
                i: p.i,
            }
    });
    let normal = info.as_ref().map_or(DVec3::Z, |r| r.normal);
    let can_all = creabim_core::offset::offset(e, 0.1, normal).is_some();
    let can_insert = info.as_ref().is_some_and(|r| r.reshapes);
    let mut asked = None;
    let area = egui::Area::new(egui::Id::new(("edge-palette", pane)))
        .order(egui::Order::Foreground)
        .fixed_pos(p.pos + Vec2::new(14.0, 14.0))
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut item = |ui: &mut egui::Ui, on: bool, text: &str, tip: &str, r| {
                        let b = ui
                            .add_enabled(on, egui::Button::new(text).small())
                            .on_hover_text(tip)
                            .on_disabled_hover_text(tip);
                        if b.clicked() {
                            asked = Some(r);
                        }
                    };
                    item(
                        ui,
                        can_edge,
                        "Offset edge",
                        "Push this edge out or pull it in, square to itself: move the pointer (it snaps) or type a distance",
                        GripRequest::OffsetEdge(p.ring, p.i),
                    );
                    item(
                        ui,
                        can_all,
                        "Offset all",
                        "Grow or shrink the whole outline by one distance: the pointer's side, or a typed distance",
                        GripRequest::OffsetAll,
                    );
                    item(
                        ui,
                        can_insert,
                        "Insert point",
                        "Add a corner here on the edge, then move it with the pointer or by a typed distance",
                        GripRequest::InsertPoint(p.ring, p.i, p.at),
                    );
                });
            });
        });
    p.rect = Some(area.response.rect);
    sel.palette = Some(p);
    if let Some(r) = asked {
        sel.request = Some(r);
        sel.palette = None;
        // typed distances go to the edit that starts, not to the button
        crate::modal::unfocus_soon(ctx);
    }
    // the edge it is for, traced
    if let Some(r) = info {
        let n = r.corners.len();
        if n >= 2 {
            let (a, b) = (r.corners[p.i % n], r.corners[(p.i + 1) % n]);
            if let (Some(a), Some(b)) = ((input.project)(a), (input.project)(b)) {
                response.ctx.layer_painter(response.layer_id).line_segment(
                    [a, b],
                    Stroke::new(3.0, interaction_ink().gamma_multiply(0.6)),
                );
            }
        }
    }
    Some("Edge: Offset edge pushes it square to itself, Offset all grows the outline, Insert point adds a corner. Or drag its grip.".into())
}

/// In Edit mode, the outlines being edited, traced so a hole inside a
/// slab or a shaft inside walls reads clearly; picked holes filled. A
/// `broken` shape is drawn red over them.
fn draw_rings(
    model: &Model,
    e: &Element,
    painter: &Painter,
    input: &PaneInput,
    sel: &GripSel,
    color: Color32,
) {
    for (r, pts) in screen_rings(model, e, input) {
        if pts.len() < 2 {
            continue;
        }
        let n = pts.len();
        let picked = matches!(r.ring, Ring::Hole(k) if sel.holes.contains(&k));
        if picked && n >= 3 {
            // a hole may be concave: a fan from its middle is only a tint
            painter.add(egui::Shape::closed_line(
                pts.clone(),
                Stroke::new(4.0, color.gamma_multiply(0.35)),
            ));
        }
        let edges = if r.closed { n } else { n - 1 };
        let width = if picked { 2.6 } else { 1.6 };
        for i in 0..edges {
            // a slab's bent edge is traced along its arc
            let arc = match (&e.kind, r.ring) {
                (ElementKind::Slab(s), Ring::Outline) if s.bulge(i) != 0.0 => {
                    let c = s.edge(i);
                    let z = r.corners[i].z;
                    (0..=16)
                        .map(|k| (input.project)(c.at(k as f64 / 16.0).extend(z)))
                        .collect::<Option<Vec<Pos2>>>()
                }
                _ => None,
            };
            match arc {
                Some(line) => {
                    painter.add(egui::Shape::line(line, Stroke::new(width, color)));
                }
                None => {
                    painter.line_segment([pts[i], pts[(i + 1) % n]], Stroke::new(width, color));
                }
            }
        }
    }
}

/// The live lengths while a grip of an outline is dragged: the edges
/// either side of a corner, how far an edge or a hole moved.
fn live_lengths(model: &Model, e: &Element, kind: HandleKind, grab: DVec3, now: DVec3) -> String {
    let len = crate::units::len;
    match kind {
        HandleKind::Vertex { ring, i } | HandleKind::AddVertex { ring, i } => {
            let i = if matches!(kind, HandleKind::AddVertex { .. }) {
                i + 1
            } else {
                i
            };
            let Some(r) = crate::editmode::ring_of(model, e, ring) else {
                return String::new();
            };
            let n = r.corners.len();
            if i >= n {
                return String::new();
            }
            let c = r.corners[i];
            let mut parts = vec![];
            if i > 0 || r.closed {
                parts.push(len(c.distance(r.corners[(i + n - 1) % n])));
            }
            if i + 1 < n || r.closed {
                parts.push(len(c.distance(r.corners[(i + 1) % n])));
            }
            parts.join(" · ")
        }
        HandleKind::Edge { .. } => format!("offset {}", len((now - grab).length())),
        HandleKind::Ring { .. } => format!("moved {}", len((now - grab).length())),
        _ => len((now - grab).length()),
    }
}

/// The right-click menu of Edit mode, for one frame. Runs what is picked
/// and returns the status line; `None` while nothing was picked.
fn grip_menu(
    doc: &mut Document,
    id: ElementId,
    ui_ctx: &egui::Context,
    sel: &mut GripSel,
) -> Option<String> {
    let m = sel.menu?;
    let e = doc.model().get(id)?.clone();
    let model = doc.model();
    let reshapes = |ring| crate::editmode::ring_of(model, &e, ring).is_some_and(|r| r.reshapes);
    let slab_bulge = |ring: Ring, i: usize| match (&e.kind, ring) {
        (ElementKind::Slab(s), Ring::Outline) => Some(s.bulge(i)),
        _ => None,
    };
    #[derive(Clone, Copy, PartialEq)]
    enum Act {
        RemoveCorners,
        AddCorner(Ring, usize, DVec3),
        Arc(usize, bool),
        RemoveHole(usize),
        MoveHole(usize),
        AllCorners,
        Deselect,
        Finish,
        ResetLayer(HandleKind),
        ResetLayers,
        DrawBy(u8),
    }
    let edited_layers = match &e.kind {
        ElementKind::Wall(w) => !w.layer_ends.is_empty(),
        ElementKind::Slab(s) => !s.layer_edges.is_empty(),
        ElementKind::Roof(r) => !r.layer_edges.is_empty(),
        _ => false,
    };
    let in_layers = layers() && has_layers(model, id);
    let mut act = None;
    let area = egui::Area::new(egui::Id::new(("grip-menu", m.pane)))
        .order(egui::Order::Foreground)
        .fixed_pos(m.at)
        .show(ui_ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(170.0);
                let mut item = |ui: &mut egui::Ui, on: bool, text: &str, a: Act| {
                    if ui
                        .add_enabled(on, egui::Button::new(text).frame(false))
                        .clicked()
                    {
                        act = Some(a);
                    }
                };
                match m.on {
                    MenuOn::Corner(ring, _) => {
                        let n = sel.corners.len();
                        let text = if n > 1 {
                            format!("Remove {n} corners")
                        } else {
                            "Remove corner".into()
                        };
                        item(ui, reshapes(ring), &text, Act::RemoveCorners);
                        item(ui, true, "Select all corners", Act::AllCorners);
                    }
                    MenuOn::Edge(ring, i, at) => {
                        item(
                            ui,
                            reshapes(ring),
                            "Add corner here",
                            Act::AddCorner(ring, i, at),
                        );
                        match slab_bulge(ring, i) {
                            Some(b) if b != 0.0 => item(ui, true, "Straighten", Act::Arc(i, false)),
                            Some(_) => item(ui, true, "Make arc", Act::Arc(i, true)),
                            None => {
                                ui.add_enabled(
                                    false,
                                    egui::Button::new("Make arc (slab outlines)").frame(false),
                                );
                            }
                        }
                        if let Ring::Hole(k) = ring {
                            ui.separator();
                            item(ui, true, "Remove hole", Act::RemoveHole(k));
                        }
                    }
                    MenuOn::Hole(k) => {
                        item(ui, true, "Remove hole", Act::RemoveHole(k));
                        item(ui, true, "Move hole", Act::MoveHole(k));
                    }
                    MenuOn::Layer(k) => {
                        let moved = edited_end(Some(&e), k);
                        let text = match k {
                            HandleKind::LayerEdge { .. } => "Reset layer edge",
                            HandleKind::LayerHeight { top: false, .. } => "Reset layer bottom",
                            HandleKind::LayerHeight { top: true, .. } => "Reset layer top",
                            _ => "Reset layer end",
                        };
                        item(ui, moved, text, Act::ResetLayer(k));
                        item(ui, edited_layers, "Reset all layers", Act::ResetLayers);
                        if let (HandleKind::LayerEnd { layer, .. }, ElementKind::Wall(w)) =
                            (k, &e.kind)
                        {
                            ui.separator();
                            let at = |b: usize| {
                                let l = creabim_core::LocationLine::Boundary(b.min(255) as u8);
                                (w.location != l, b.min(255) as u8)
                            };
                            let (on, b) = at(layer);
                            item(ui, on, "Draw by its exterior face", Act::DrawBy(b));
                            let (on, b) = at(layer + 1);
                            item(ui, on, "Draw by its interior face", Act::DrawBy(b));
                        }
                        ui.separator();
                        item(ui, true, "Finish editing   Esc", Act::Finish);
                    }
                    MenuOn::Blank if in_layers => {
                        item(ui, edited_layers, "Reset all layers", Act::ResetLayers);
                        ui.separator();
                        item(ui, true, "Finish editing   Esc", Act::Finish);
                    }
                    MenuOn::Blank => {
                        item(ui, true, "Select all corners", Act::AllCorners);
                        item(ui, !sel.is_empty(), "Deselect corners", Act::Deselect);
                        ui.separator();
                        item(ui, true, "Finish editing   Esc", Act::Finish);
                    }
                }
            });
        });
    let pressed_out = ui_ctx.input(|i| {
        (i.pointer.any_pressed() || i.key_pressed(egui::Key::Escape))
            && i.pointer
                .interact_pos()
                .is_none_or(|p| !area.response.rect.contains(p))
    });
    let Some(a) = act else {
        if pressed_out {
            sel.menu = None;
        }
        return None;
    };
    sel.menu = None;
    let r = match a {
        Act::RemoveCorners => delete_picked(doc, id, sel),
        Act::RemoveHole(k) => {
            let r = crate::editmode::remove_holes(doc, id, &[k]);
            if r.is_ok() {
                sel.clear();
            }
            r
        }
        Act::AddCorner(ring, i, at) => crate::editmode::add_corner(doc, id, ring, i, Some(at)),
        Act::Arc(i, on) => crate::editmode::set_arc(doc, id, i, on),
        Act::MoveHole(k) => {
            sel.pick_hole(k, false);
            Ok("Drag inside the hole to move it. Esc cancels the drag.".into())
        }
        Act::AllCorners => {
            sel.holes.clear();
            sel.corners = crate::editmode::rings(doc.model(), &e)
                .iter()
                .flat_map(|r| (0..r.corners.len()).map(move |i| (r.ring, i)))
                .collect();
            Ok(format!(
                "{} corners selected: Delete removes them.",
                sel.corners.len()
            ))
        }
        Act::Deselect => {
            sel.clear();
            Ok("Nothing picked.".into())
        }
        Act::Finish => {
            sel.request = Some(GripRequest::Finish);
            Ok("Finished editing.".into())
        }
        Act::ResetLayer(k) => reset_layer_ends(doc, id, Some(k)),
        Act::ResetLayers => reset_layer_ends(doc, id, None),
        Act::DrawBy(k) => draw_by_boundary(doc, id, k),
    };
    Some(r.unwrap_or_else(|e| e))
}

/// What a selected section or elevation sees: its line, and the band
/// as deep as it looks, so it can be found and its depth dragged.
/// Whether `snapped` shows close to `free` on screen: in 3D a point that
/// lies under the pointer can be far away along the line being slid on.
/// Where handle `h` shows on screen: at its point, or nudged out along
/// its line.
/// A mass's edit grips that would sit on top of each other on screen
/// (a mass seen small, far off) are left out, so a few clear grips show
/// instead of a cluster (MU39): its corners first, then its faces, then
/// its edge middles; the move, lift and turn grips always stay.
fn declutter(shown: Vec<Handle>, input: &PaneInput) -> Vec<Handle> {
    let rank = |k: &HandleKind| match k {
        HandleKind::MeshVertex(_) => 1,
        HandleKind::Face(_) => 2,
        HandleKind::MeshEdge(..) => 3,
        _ => 0,
    };
    if !shown.iter().any(|h| rank(&h.kind) > 0) {
        return shown;
    }
    // the grips that always stay, then the rest in their order
    let mut order: Vec<usize> = (0..shown.len()).collect();
    order.sort_by_key(|i| rank(&shown[*i].kind));
    let mut placed: Vec<Pos2> = vec![];
    let mut keep = vec![false; shown.len()];
    for i in order {
        let h = &shown[i];
        let Some(c) = screen_at(h, input) else {
            keep[i] = true;
            continue;
        };
        // corners need room of their own; faces and edges a little more
        let room = match rank(&h.kind) {
            0 => 0.0,
            1 => 14.0,
            _ => 22.0,
        };
        if room > 0.0 && placed.iter().any(|p| p.distance(c) < room) {
            continue;
        }
        placed.push(c);
        keep[i] = true;
    }
    shown
        .into_iter()
        .zip(keep)
        .filter_map(|(h, k)| k.then_some(h))
        .collect()
}

fn screen_at(h: &Handle, input: &PaneInput) -> Option<Pos2> {
    let c = (input.project)(h.at)?;
    let Constraint::Line { dir, .. } = h.constraint else {
        return Some(c);
    };
    if h.nudge == 0.0 {
        return Some(c);
    }
    let tip = (input.project)(h.at + dir * 0.3)?;
    let v = (tip - c).normalized();
    Some(if v.is_finite() { c + v * h.nudge } else { c })
}

/// Where a grip sliding along the line through `point` (direction `dir`)
/// meets the face under the pointer, if that is within reach of where the
/// pointer puts it (`free`) and the face isn't edge-on to the line.
fn on_face(input: &PaneInput, pos: Pos2, point: DVec3, dir: DVec3, free: DVec3) -> Option<DVec3> {
    let (hit, n) = (input.face)(pos)?;
    let across = dir.dot(n);
    if across.abs() < 0.2 {
        return None;
    }
    let on = point + dir * ((hit - point).dot(n) / across);
    near_on_screen(input, on, free).then_some(on)
}

/// Where a grip sliding along the line through `point` (direction `dir`)
/// meets what the snap at `p` found. A point snap: the foot of `p` on the
/// line. An edge, a level line or a guide: the snap gives the point on it
/// nearest the pointer, so ask again from the line, and again: for a line
/// crossing the path at any angle this closes on the crossing in a few
/// rounds (exact at once for one square to it, as a slab's top under a
/// wall's base).
fn crossing(
    input: &PaneInput,
    fixed: Option<DVec3>,
    point: DVec3,
    dir: DVec3,
    p: DVec3,
    kind: SnapKind,
) -> DVec3 {
    let foot = |q: DVec3| point + dir * (q - point).dot(dir);
    let mut on = foot(p);
    if !matches!(
        kind,
        SnapKind::OnEdge
            | SnapKind::OnGridLine
            | SnapKind::Guide
            | SnapKind::OnTempGuide
            | SnapKind::UnderlayFace
    ) {
        return on;
    }
    for _ in 0..6 {
        let Some(at) = (input.project)(on) else {
            break;
        };
        let Some((q, again)) = (input.snap)(at, fixed) else {
            break;
        };
        if again.kind != kind {
            break;
        }
        let next = foot(q);
        let moved = (next - on).length();
        on = next;
        if moved < 1e-6 {
            break;
        }
    }
    on
}

fn near_on_screen(input: &PaneInput, snapped: DVec3, free: DVec3) -> bool {
    match ((input.project)(snapped), (input.project)(free)) {
        (Some(a), Some(b)) => a.distance(b) < 16.0,
        _ => false,
    }
}

fn draw_view_range(painter: &Painter, input: &PaneInput, a: DVec2, b: DVec2, depth: f64) {
    let look = (b - a).normalize_or_zero().perp();
    let corners = [a, b, b + look * depth, a + look * depth];
    let pts: Option<Vec<Pos2>> = corners
        .iter()
        .map(|p| (input.project)(p.extend(0.0)))
        .collect();
    let Some(pts) = pts else {
        return;
    };
    painter.add(egui::Shape::convex_polygon(
        pts.clone(),
        interaction_ink().gamma_multiply(0.08),
        Stroke::NONE,
    ));
    painter.line_segment([pts[0], pts[1]], Stroke::new(2.5, interaction_ink()));
    for (p, q) in [(pts[1], pts[2]), (pts[2], pts[3]), (pts[3], pts[0])] {
        painter.add(egui::Shape::dashed_line(
            &[p, q],
            Stroke::new(1.2, interaction_ink().gamma_multiply(0.8)),
            6.0,
            4.0,
        ));
    }
}

/// Draw the selected element's handles and run a drag on them (or on the
/// element itself). Returns `true` while a drag runs, so the pane leaves
/// the pointer alone, and puts a message for the status bar in `status`.
/// In Edit mode (`edit`) it also picks corners and holes into `grips`
/// (a click, Shift+click, a box), opens their right-click menu, and moves
/// a hole dragged from inside.
#[allow(clippy::too_many_arguments)]
pub fn interact(
    doc: &mut Document,
    painter: &Painter,
    response: &egui::Response,
    selection: Option<ElementId>,
    face: Option<usize>,
    edit: bool,
    drag: &mut Option<Drag>,
    grips: &mut GripSel,
    input: &PaneInput,
    status: &mut Option<String>,
) -> bool {
    let Some(id) = selection else {
        *drag = None;
        grips.reset(None);
        return false;
    };
    grips.reset(Some(id));
    if toggle_live(painter.ctx()) {
        *status = Some(if live_on(painter.ctx()) {
            "Live dimensions on: a drag shows the value it changes. D hides them.".into()
        } else {
            "Live dimensions off. D shows them again.".into()
        });
    }
    if !edit {
        grips.clear();
    }
    let ctx = painter.ctx().clone();
    let shift = ctx.input(|i| i.modifiers.shift);
    // drafting is on its own view's paper only
    let elsewhere = match doc.model().get(id).map(|e| &e.kind) {
        Some(ElementKind::Detail(d)) => Some(d.view) != input.view,
        Some(ElementKind::View(v)) => v.callout.is_some_and(|c| Some(c.parent) != input.view),
        // a reference has grips only where it is shown: in its views, or
        // in 3D when it shows there (MU38: a profile's trace had grips in
        // the plans)
        Some(ElementKind::Reference(r)) => match input.view {
            Some(v) => !r.shown_in.contains(&v),
            None => !r.shows_in_3d(),
        },
        _ => false,
    };
    let shown: Vec<Handle> = handles_in(doc.model(), id, face, edit)
        .into_iter()
        .filter(|_| !elsewhere)
        .map(|h| in_section(h, input.look))
        .filter(|h| h.constraint.usable(input.look))
        // grips sit on the element: a wall slides by its body
        .filter(|h| h.kind != HandleKind::WallSlide)
        .collect();
    let shown = declutter(shown, input);
    let screen = |h: &Handle| screen_at(h, input);
    // a mass's edge is taken anywhere along it, not only at its middle,
    // as Blender's (MU39: edges were hard to pick)
    let mesh_ends: Option<(DVec3, creabim_core::FreeMesh)> =
        match doc.model().get(id).map(|e| &e.kind) {
            Some(ElementKind::FreeMesh(m)) if edit => Some((
                m.origin + DVec3::Z * elevation(doc.model(), m.level),
                m.clone(),
            )),
            _ => None,
        };
    let along = |h: &Handle, p: Pos2| -> Option<f32> {
        let HandleKind::MeshEdge(a, b) = h.kind else {
            return None;
        };
        let (base, m) = mesh_ends.as_ref()?;
        let s = |i: u32| (input.project)(*base + m.to_model(m.mesh.vertices[i as usize]));
        let (pa, pb) = (s(a)?, s(b)?);
        let ab = pb - pa;
        let t = ((p - pa).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.08, 0.92);
        Some((pa + ab * t).distance(p))
    };
    let nearest = |p: Pos2| {
        shown
            .iter()
            .filter_map(|h| Some((h, along(h, p).or_else(|| Some(screen(h)?.distance(p)))?)))
            // 8 px to see, 16 px to hit
            .filter(|(_, d)| *d < 8.0)
            // a corner wins over an add-corner grip or an edge's at the
            // same spot
            .min_by(|a, b| {
                let rank = |h: &Handle| match h.kind {
                    HandleKind::Vertex { .. } => 0.0,
                    _ => 1.5,
                };
                (a.1 + rank(a.0)).total_cmp(&(b.1 + rank(b.0)))
            })
            .map(|(h, _)| *h)
    };
    let element = doc.model().get(id).cloned();
    if let (true, Some(e)) = (edit, &element) {
        draw_rings(doc.model(), e, painter, input, grips, interaction_ink());
    }
    if let Some(ElementKind::View(v)) = element.as_ref().map(|e| &e.kind) {
        if let ViewKind::Section {
            start, end, depth, ..
        } = v.kind
        {
            draw_view_range(painter, input, start, end, depth);
        }
    }
    let pointer = response.hover_pos();
    let hot = pointer
        .filter(|_| drag.is_none() && !grips.busy())
        .and_then(nearest)
        .map(|h| h.kind);
    let held = drag.as_ref().and_then(|d| d.kind);
    // in Edit mode the "+" shows only on the edge under the pointer
    let hovered_edge = match (edit, pointer, &element) {
        (true, Some(p), Some(e)) if drag.is_none() => {
            edge_at(doc.model(), e, input, p).map(|(r, i, _)| (r, i))
        }
        _ => None,
    };
    let look = GripLook::of(&ctx);
    // a mass in Edit mode, as Blender's: its wires thin and dark, its
    // corners small dots; edges and faces light up only under the pointer
    // (MU39)
    let mesh_edit = edit
        && matches!(
            element.as_ref().map(|e| &e.kind),
            Some(ElementKind::FreeMesh(_))
        );
    let (wire_w, dot) = edit_display();
    if mesh_edit {
        if let Some(ElementKind::FreeMesh(m)) = element.as_ref().map(|e| &e.kind) {
            let base = m.origin + DVec3::Z * elevation(doc.model(), m.level);
            let ink = Color32::from_gray(20);
            for (a, b) in mesh_edges(&m.mesh) {
                let p = |i: u32| (input.project)(base + m.to_model(m.mesh.vertices[i as usize]));
                if let (Some(pa), Some(pb)) = (p(a), p(b)) {
                    painter.line_segment([pa, pb], Stroke::new(wire_w, ink));
                }
            }
        }
    }
    // what is picked on the mass: its faces tinted, its edges and points
    // in the accent, lit until another pick (MU39)
    if mesh_edit && !grips.mesh.is_empty() {
        if let Some(ElementKind::FreeMesh(m)) = element.as_ref().map(|e| &e.kind) {
            let base = m.origin + DVec3::Z * elevation(doc.model(), m.level);
            let accent = crate::theme::interaction();
            let p = |i: u32| (input.project)(base + m.to_model(m.mesh.vertices[i as usize]));
            let on = |v: &u32| grips.mesh.contains(v);
            for f in &m.mesh.faces {
                if f.iter().all(on) {
                    let pts: Option<Vec<Pos2>> = f.iter().map(|v| p(*v)).collect();
                    if let Some(pts) = pts {
                        painter.add(egui::Shape::Path(egui::epaint::PathShape {
                            points: pts,
                            closed: true,
                            fill: accent.gamma_multiply(0.28),
                            stroke: Stroke::NONE.into(),
                        }));
                    }
                }
            }
            for (a, b) in mesh_edges(&m.mesh) {
                if on(&a) && on(&b) {
                    if let (Some(pa), Some(pb)) = (p(a), p(b)) {
                        painter.line_segment([pa, pb], Stroke::new(edit_display().0 + 1.5, accent));
                    }
                }
            }
            for v in &grips.mesh {
                if let Some(c) = p(*v) {
                    painter.circle_filled(c, edit_display().1 / 2.0 + 1.0, accent);
                }
            }
        }
    }
    for h in &shown {
        let Some(c) = screen(h) else {
            continue;
        };
        if mesh_edit {
            let state = if hot == Some(h.kind) || held == Some(h.kind) {
                GripState::Hover
            } else {
                GripState::Rest
            };
            let accent = crate::theme::interaction();
            match h.kind {
                HandleKind::MeshVertex(_) => {
                    let lit = state == GripState::Hover;
                    let ink = if lit { accent } else { Color32::from_gray(15) };
                    painter.circle_filled(c, dot / 2.0 + if lit { 1.0 } else { 0.0 }, ink);
                    if lit {
                        ctx.set_cursor_icon(egui::CursorIcon::Move);
                    }
                    continue;
                }
                HandleKind::MeshEdge(a, b) if state == GripState::Hover => {
                    if let Some(ElementKind::FreeMesh(m)) = element.as_ref().map(|e| &e.kind) {
                        let base = m.origin + DVec3::Z * elevation(doc.model(), m.level);
                        let p = |i: u32| {
                            (input.project)(base + m.to_model(m.mesh.vertices[i as usize]))
                        };
                        if let (Some(pa), Some(pb)) = (p(a), p(b)) {
                            painter.line_segment([pa, pb], Stroke::new(wire_w + 1.5, accent));
                        }
                    }
                    ctx.set_cursor_icon(egui::CursorIcon::Move);
                    continue;
                }
                // Edges picked (2): every edge shows, to grab (MU39)
                HandleKind::MeshEdge(a, b) if pick() == 2 => {
                    if let Some(ElementKind::FreeMesh(m)) = element.as_ref().map(|e| &e.kind) {
                        let base = m.origin + DVec3::Z * elevation(doc.model(), m.level);
                        let p = |i: u32| {
                            (input.project)(base + m.to_model(m.mesh.vertices[i as usize]))
                        };
                        if let (Some(pa), Some(pb)) = (p(a), p(b)) {
                            painter.line_segment(
                                [pa, pb],
                                Stroke::new(wire_w + 1.0, accent.gamma_multiply(0.55)),
                            );
                        }
                    }
                    continue;
                }
                HandleKind::MeshEdge(..) => continue,
                // Faces picked (3): a small square at each face's middle,
                // as Blender's face dots
                HandleKind::Face(_) if state == GripState::Rest && pick() == 3 => {
                    let r = egui::Rect::from_center_size(c, egui::vec2(dot + 2.0, dot + 2.0));
                    painter.rect(
                        r,
                        1.0,
                        accent.gamma_multiply(0.35),
                        Stroke::new(1.0, accent),
                        egui::StrokeKind::Middle,
                    );
                    continue;
                }
                HandleKind::Face(_) if state == GripState::Rest => continue,
                _ => {}
            }
        }
        let state = if held == Some(h.kind) {
            GripState::Held
        } else if held.is_some() {
            GripState::Faded
        } else if hot == Some(h.kind) {
            GripState::Hover
        } else {
            GripState::Rest
        };
        if let HandleKind::AddVertex { ring, i } = h.kind {
            if state == GripState::Rest && hovered_edge != Some((ring, i)) {
                continue;
            }
            draw_plus(painter, c, state, &look);
            continue;
        }
        let picked = matches!(h.kind, HandleKind::Vertex { ring, i } if grips.picked(ring, i));
        // a square is a point you move, a bar an edge or a face you push
        let bar = h.kind.round().then(|| match h.constraint {
            Constraint::Line { dir, .. } => (input.project)(h.at + dir * 0.3)
                .map(|tip| (tip - c).normalized())
                .filter(|v| v.is_finite())
                .map(|v| v.rot90()),
            Constraint::Plane { .. } => None,
        });
        // a wall too thin on screen for its face grip shows it only when
        // the pointer comes near the face
        if h.kind == HandleKind::WallFace
            && state == GripState::Rest
            && wall_px(doc.model(), id, input).is_some_and(|px| px < THIN_PX)
            && pointer.is_none_or(|p| p.distance(c) > 14.0)
        {
            continue;
        }
        // a corner in Edit mode, or a layer end moved by hand: a dot
        let filled = edit
            && (matches!(h.kind, HandleKind::Vertex { .. })
                || edited_end(element.as_ref(), h.kind));
        if h.kind == HandleKind::Rotate {
            // an object's turn grip: a curved arrow on a short leader from
            // its front edge, so it reads as part of the object (A7-7)
            let front = element.as_ref().and_then(|e| {
                let ElementKind::Object(o) = &e.kind else {
                    return None;
                };
                let (lo, hi) = creabim_core::objects::type_of(doc.model(), e)?.bounds();
                let z = creabim_core::objects::base_z(doc.model(), e).unwrap_or(0.0);
                (input.project)(o.to_plan(DVec2::new((lo.x + hi.x) / 2.0, lo.y)).extend(z))
            });
            draw_turn_grip(painter, c, front, state, &look);
            if state == GripState::Hover {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            }
            continue;
        }
        draw_grip(painter, c, bar, state, filled, picked, &look);
        if state == GripState::Hover {
            if let Some(dir) = bar.flatten() {
                ctx.set_cursor_icon(resize_cursor(dir.rot90()));
            } else {
                ctx.set_cursor_icon(egui::CursorIcon::Move);
            }
        }
    }
    // the held grip's old place stays as a faint tick
    if let Some(d) = drag.as_ref().filter(|d| d.kind.is_some()) {
        if let Some(c) = (input.project)(d.grab) {
            let s = Stroke::new(1.0, look.accent.gamma_multiply(0.45));
            painter.line_segment([c - Vec2::splat(4.0), c + Vec2::splat(4.0)], s);
            painter.line_segment([c + Vec2::new(-4.0, 4.0), c + Vec2::new(4.0, -4.0)], s);
        }
    }
    // a let-go grip's new value, for a second
    // only in the pane it was dragged in
    let flash_id = egui::Id::new("grip.flash").with(response.id);
    if let Some((text, number, at, t0)) =
        ctx.data(|d| d.get_temp::<(String, String, Pos2, f64)>(flash_id))
    {
        let age = ctx.input(|i| i.time) - t0;
        if age < FLASH {
            // steady for a second, then gone in the last third
            let fade = ((FLASH - age) / (FLASH * 0.3)).clamp(0.0, 1.0) as f32;
            pill_value(
                painter,
                at + Vec2::new(0.0, -34.0),
                &text,
                &number,
                &look,
                fade,
            );
            ctx.request_repaint();
        } else {
            ctx.data_mut(|d| d.remove::<(String, String, Pos2, f64)>(flash_id));
        }
    }
    if let (Some(k), Some(p)) = (hot, pointer) {
        let fixed = matches!(k, HandleKind::Vertex { ring, .. } | HandleKind::AddVertex { ring, .. }
            if element.as_ref().and_then(|e| crate::editmode::ring_of(doc.model(), e, ring)).is_some_and(|r| !r.reshapes));
        let at = shown
            .iter()
            .find(|h| h.kind == k)
            .and_then(screen)
            .unwrap_or(p);
        // the grip says what it is; in Edit mode, what more it does
        let name = layer_name(doc.model(), id, k);
        pill(
            painter,
            at + Vec2::new(0.0, -34.0),
            name.as_deref().unwrap_or(k.name()),
            &look,
            1.0,
        );
        if edit {
            let tip = if fixed {
                "Drag: move corner · This sketch's arcs or constraints keep its corners: no adding or removing"
            } else {
                k.tip(edit)
            };
            note(
                painter,
                p + Vec2::new(0.0, 22.0),
                tip,
                Color32::from_gray(40),
            );
        }
    }

    // the palette on the outline edge under the pointer
    if drag.is_none() && !grips.busy() && !elsewhere {
        if let Some(e) = &element {
            let corner = matches!(hot, Some(HandleKind::Vertex { .. }));
            if let Some(s) = edge_palette(doc.model(), e, &ctx, response, input, grips, corner) {
                *status = Some(s);
            }
        }
    }

    // the right-click menu, open in this pane
    if grips.menu.is_some_and(|m| m.pane == response.id) {
        if let Some(s) = grip_menu(doc, id, &ctx, grips) {
            *status = Some(s);
        }
        return true;
    }
    if edit && drag.is_none() && response.secondary_clicked() {
        if let (Some(p), Some(e)) = (response.interact_pointer_pos(), &element) {
            let on = match nearest(p).map(|h| h.kind) {
                Some(
                    k @ (HandleKind::LayerEnd { .. }
                    | HandleKind::LayerEdge { .. }
                    | HandleKind::LayerHeight { .. }),
                ) => MenuOn::Layer(k),
                Some(HandleKind::Vertex { ring, i }) => {
                    if !grips.picked(ring, i) {
                        grips.pick(ring, i, false);
                    }
                    MenuOn::Corner(ring, i)
                }
                Some(HandleKind::AddVertex { ring, i } | HandleKind::Edge { ring, i }) => {
                    let at = edge_at(doc.model(), e, input, p)
                        .filter(|(r, j, _)| *r == ring && *j == i)
                        .map(|(_, _, at)| at)
                        .unwrap_or_else(|| {
                            shown
                                .iter()
                                .find(|h| h.kind == HandleKind::AddVertex { ring, i })
                                .map_or(DVec3::ZERO, |h| h.at)
                        });
                    MenuOn::Edge(ring, i, at)
                }
                _ => match edge_at(doc.model(), e, input, p) {
                    Some((ring, i, at)) => MenuOn::Edge(ring, i, at),
                    None => match hole_at(doc.model(), e, input, p) {
                        Some(k) => {
                            grips.pick_hole(k, false);
                            MenuOn::Hole(k)
                        }
                        None => MenuOn::Blank,
                    },
                },
            };
            grips.menu = Some(GripMenu {
                pane: response.id,
                at: p,
                on,
            });
            return true;
        }
    }

    // a box over the grips, in Edit mode
    if let Some((pane, from)) = grips.boxsel {
        if pane == response.id {
            let to = response.interact_pointer_pos().or(pointer).unwrap_or(from);
            let rect = Rect::from_two_pos(from, to);
            painter.rect(
                rect,
                0.0,
                interaction_ink().gamma_multiply(0.08),
                Stroke::new(1.0, interaction_ink()),
                egui::StrokeKind::Middle,
            );
            if response.drag_stopped() || !response.dragged() {
                if !shift {
                    grips.corners.clear();
                    grips.holes.clear();
                }
                let mut mesh = vec![];
                for h in &shown {
                    if let HandleKind::Vertex { ring, i } = h.kind {
                        if screen(h).is_some_and(|c| rect.contains(c)) && !grips.picked(ring, i) {
                            grips.corners.push((ring, i));
                        }
                    }
                }
                // a mass's vertices in the box, whichever grips show
                if let Some(ElementKind::FreeMesh(m)) = element.as_ref().map(|e| &e.kind) {
                    let base = m.origin + DVec3::Z * elevation(doc.model(), m.level);
                    for (i, v) in m.mesh.vertices.iter().enumerate() {
                        if (input.project)(base + m.to_model(*v)).is_some_and(|c| rect.contains(c))
                        {
                            mesh.push(i as u32);
                        }
                    }
                }
                if !mesh.is_empty() || !grips.mesh.is_empty() {
                    grips.pick_mesh(&mesh, shift);
                    grips.boxsel = None;
                    *status = Some(format!(
                        "{} points picked: G or M moves them, R turns, S scales.",
                        grips.mesh.len()
                    ));
                    return true;
                }
                grips.boxsel = None;
                *status = Some(match grips.corners.len() {
                    0 => "No corners in the box.".into(),
                    n => {
                        format!("{n} corners selected: Delete removes them, right-click for more.")
                    }
                });
            }
            return true;
        }
    }

    // a selected wall's location line, and a cross at an end that may not join
    if let Some(ElementKind::Wall(w)) = element.as_ref().map(|e| &e.kind) {
        let z = walls::base_z(doc.model(), w);
        let (a, b) = walls::location_line(doc.model(), w);
        if let (Some(pa), Some(pb)) = ((input.project)(a.extend(z)), (input.project)(b.extend(z))) {
            painter.add(egui::Shape::dashed_line(
                &[pa, pb],
                Stroke::new(1.2, interaction_ink()),
                8.0,
                3.0,
            ));
            for (i, p) in [pa, pb].into_iter().enumerate() {
                if w.disallow_join[i] {
                    let o = Vec2::new(10.0, -10.0);
                    let s = Stroke::new(2.0, Color32::from_rgb(200, 40, 40));
                    let c = p + o;
                    painter.line_segment([c - Vec2::splat(4.0), c + Vec2::splat(4.0)], s);
                    painter.line_segment([c + Vec2::new(-4.0, 4.0), c + Vec2::new(4.0, -4.0)], s);
                }
            }
        }
    }

    // flips are Space (facing) and Shift+Space (hand), and the action
    // bar: no circles on the plan (see flip_arrows)

    // a click on a grip keeps the selection rather than picking what lies
    // under it; in Edit mode it picks the corner (Shift adds), and a
    // second click on the same corner takes it away
    if drag.is_none() && response.clicked() {
        if let Some(p) = response.interact_pointer_pos() {
            let hit = nearest(p).map(|h| h.kind);
            let now = ctx.input(|i| i.time);
            if let Some(k) = hit {
                // when the frames are slow both clicks can come in one
                // frame: egui still counts them as a double-click
                if response.double_clicked() {
                    grips.last_tap = Some((k, now, p));
                }
                if let Some(s) = tap_grip(doc, id, k, edit, shift, now, p, grips) {
                    *status = Some(s);
                }
                return true;
            }
            if edit {
                if let Some(k) = element
                    .as_ref()
                    .and_then(|e| hole_at(doc.model(), e, input, p))
                {
                    grips.pick_hole(k, shift);
                    *status = Some(format!(
                        "Hole {} selected: Delete removes it, a drag inside moves it.",
                        k + 1
                    ));
                    return true;
                }
                if !shift && !grips.is_empty() {
                    grips.clear();
                }
            }
        }
    }

    if drag.is_none() && response.drag_started_by(egui::PointerButton::Primary) {
        let Some(press) = ctx
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())
        else {
            return false;
        };
        let near = nearest(press);
        // in Edit mode a drag from inside a hole moves the hole
        let hole = (near.is_none() && edit)
            .then(|| {
                element
                    .as_ref()
                    .and_then(|e| hole_at(doc.model(), e, input, press))
            })
            .flatten()
            .and_then(|k| {
                let e = element.as_ref()?;
                let r = crate::editmode::ring_of(doc.model(), e, Ring::Hole(k))?;
                let at = (input.ray)(press)
                    .and_then(|ray| {
                        Constraint::Plane {
                            point: r.corners[0],
                            normal: r.normal,
                        }
                        .hit(ray)
                    })
                    .unwrap_or(r.corners[0]);
                let h = in_section(
                    Handle {
                        kind: HandleKind::Ring {
                            ring: Ring::Hole(k),
                        },
                        at,
                        constraint: Constraint::Plane {
                            point: at,
                            normal: r.normal,
                        },
                        nudge: 0.0,
                    },
                    input.look,
                );
                h.constraint.usable(input.look).then_some(h)
            });
        let start = match near.or(hole) {
            Some(h) => {
                let grab = (input.ray)(press)
                    .and_then(|r| h.constraint.hit(r))
                    .unwrap_or(h.at);
                Some((Some(h.kind), h.constraint, grab))
            }
            // in Edit mode a drag off the grips draws a box round some, in
            // drawings; in 3D it orbits as ever (MU18)
            None if edit && input.view.is_some() => {
                grips.boxsel = Some((response.id, press));
                grips.menu = None;
                return true;
            }
            None => {
                let movable = !matches!(
                    element.as_ref().map(|e| &e.kind),
                    Some(
                        ElementKind::View(_)
                            | ElementKind::Dimension(_)
                            | ElementKind::Tag(_)
                            | ElementKind::Note(_)
                            | ElementKind::TextStyle(_)
                            | ElementKind::SheetTemplate(_)
                            | ElementKind::Level(_)
                            | ElementKind::Sheet(_)
                            | ElementKind::Viewport(_)
                    ) | None
                );
                (input.grab_body)(press, id).filter(|_| movable).map(|p| {
                    let p = (input.grab_snap)(press, id).unwrap_or(p);
                    // a wall's body slides it square to itself, its joined
                    // walls stretching; Shift frees the direction
                    if let (Some(ElementKind::Wall(w)), false) =
                        (element.as_ref().map(|e| &e.kind), shift)
                    {
                        let c = Constraint::Line {
                            point: p,
                            dir: w.direction().perp().extend(0.0),
                        };
                        return (Some(HandleKind::WallSlide), c, p);
                    }
                    let c = match element.as_ref().map(|e| &e.kind) {
                        // a grid line slides square to itself
                        Some(ElementKind::Grid(g)) => Constraint::Line {
                            point: p,
                            dir: (g.end - g.start).normalize_or_zero().perp().extend(0.0),
                        },
                        _ => Constraint::Plane {
                            point: p,
                            normal: input.body_normal,
                        },
                    };
                    (None, c, p)
                })
            }
        };
        if let Some((kind, constraint, grab)) = start {
            let (mut originals, mut joined) = match kind {
                Some(k) => originals(doc.model(), id, k),
                None => (element.clone().into_iter().collect(), vec![]),
            };
            // Ctrl or Alt drags a wall end free of the walls joined to it
            let free = matches!(kind, Some(HandleKind::WallEnd { .. }))
                && ctx.input(|i| i.modifiers.command || i.modifiers.alt);
            if free {
                originals.truncate(1);
                joined.clear();
            }
            let name = originals.first().map_or(String::new(), |e| e.name.clone());
            let label = match kind {
                Some(k) if free => format!("{} {name} free", k.verb()),
                Some(k) => k.label(&name),
                None => format!("Move {name}"),
            };
            crate::actlog::line(format!(
                "drag start: {label} | pane {} | grab {} | step {} mm | grip {:?}",
                pane_name(input),
                crate::actlog::pt(grab),
                crate::actlog::mm(crate::snaps::current_step()),
                kind,
            ));
            *drag = Some(Drag {
                kind,
                constraint,
                edit: LiveEdit::new(label, originals).free(free),
                joined,
                grab,
                press,
                last: grab,
                typed: None,
                owner: response.id,
            });
        }
    }

    let Some(d) = drag.as_mut() else {
        grips.typing = false;
        return false;
    };
    // another pane's drag: leave it to that pane
    if d.owner != response.id {
        return false;
    }
    // Tab, or a digit, starts typing a length
    let typed_now = ctx.input_mut(|i| {
        let tab = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
        let digit = i.events.iter().find_map(|e| match e {
            egui::Event::Text(t) if t.chars().all(|c| c.is_ascii_digit() || c == '.') => {
                Some(t.clone())
            }
            _ => None,
        });
        (tab, digit)
    });
    if d.kind.is_some() && d.typed.is_none() {
        match typed_now {
            (true, _) => d.typed = Some(String::new()),
            (_, Some(t)) => d.typed = Some(t),
            _ => {}
        }
        if d.typed.is_some() {
            return true;
        }
    }
    if d.typed.is_some() {
        grips.typing = true;
        if typed_length(doc, painter, d, input, status) {
            *drag = None;
            grips.typing = false;
        }
        return true;
    }
    grips.typing = false;
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        d.edit.cancel(doc);
        *drag = None;
        *status = Some("Drag cancelled. Tab finishes editing.".into());
        return true;
    }
    if response.drag_stopped() || !response.dragged() {
        let kind = d.kind;
        let applied = d.edit.applied();
        if applied {
            // the full summary in the status bar; by the grip, for a
            // second, only the value that changed
            let now = doc.model().get(id).cloned();
            let summary = now
                .as_ref()
                .map(|e| readout(std::slice::from_ref(e)))
                .filter(|v| !v.is_empty());
            *status = doc.undo_label().map(|l| match &summary {
                Some(s) => format!("{l}: {s}. Ctrl+Z to undo."),
                None => format!("{l}. Ctrl+Z to undo."),
            });
            let change = match (d.kind, d.edit.originals().first(), &now) {
                (None | Some(HandleKind::WallSlide), old, new) => Some((
                    "Moved ".to_string(),
                    bare(moved_by(old, new.as_ref()).unwrap_or((d.last - d.grab).length())),
                )),
                (Some(_), Some(old), Some(new)) => changing(old, new),
                _ => None,
            };
            if let (Some((text, number)), Some(at), true) =
                (change, (input.project)(d.last), live_on(&ctx))
            {
                let t = ctx.input(|i| i.time);
                ctx.data_mut(|m| {
                    m.insert_temp(
                        egui::Id::new("grip.flash").with(response.id),
                        (text, number, at, t),
                    )
                });
            }
        }
        let press = d.press;
        crate::actlog::line(format!(
            "drag end: {} | applied {}",
            d.edit.label(),
            d.edit.applied()
        ));
        *drag = None;
        match kind {
            // a press on a grip that never moved is a click on it
            Some(k) if !applied => {
                let now = ctx.input(|i| i.time);
                if let Some(s) = tap_grip(doc, id, k, edit, shift, now, press, grips) {
                    *status = Some(s);
                }
            }
            // the corner dragged stays picked
            Some(HandleKind::Vertex { ring, i }) if edit => grips.pick(ring, i, false),
            Some(HandleKind::AddVertex { ring, i }) if edit => grips.pick(ring, i + 1, false),
            Some(HandleKind::Ring {
                ring: Ring::Hole(k),
            }) if edit => grips.pick_hole(k, false),
            _ => {}
        }
        return true;
    }
    let Some(pos) = response.interact_pointer_pos() else {
        return true;
    };
    // a click held a moment, or a shaky one, moves nothing
    if pos.distance(d.press) < crate::modal::STILL && !d.edit.applied() {
        return true;
    }
    let Some(free) = (input.ray)(pos).and_then(|r| d.constraint.hit(r)) else {
        return true;
    };
    let fixed = match (d.kind, d.edit.originals().first().map(|e| &e.kind)) {
        (Some(HandleKind::WallEnd { end }), Some(ElementKind::Wall(w))) => {
            let (a, b) = walls::location_line(doc.model(), w);
            Some(if end { a } else { b }.extend(free.z))
        }
        (None, _) => Some(d.grab),
        _ => None,
    };
    let snapped = (input.snap)(pos, fixed);
    let now = match (d.constraint, snapped) {
        (Constraint::Plane { point, normal }, Some((p, snap))) => {
            if snap.kind != SnapKind::Grid {
                if let Some(at) = (input.project)(p) {
                    crate::snaps::draw(painter, snap, at, fixed.and_then(|f| (input.project)(f)));
                }
            }
            // onto the plane, along its normal
            p - normal * (p - point).dot(normal)
        }
        // a grip sliding along a line stops on points near its path, and
        // where an edge, a level or grid line, or a guide crosses it (the
        // snap's point is the nearest on that line, so near the crossing):
        // a wall's base dragged in a section stops on a slab's top
        (Constraint::Line { point, dir }, Some((p, snap)))
            if matches!(
                snap.kind,
                SnapKind::Endpoint
                    | SnapKind::Midpoint
                    | SnapKind::Center
                    | SnapKind::Intersection
                    | SnapKind::Quadrant
                    | SnapKind::OnEdge
                    | SnapKind::OnGridLine
                    | SnapKind::Guide
                    | SnapKind::OnTempGuide
                    | SnapKind::GuideCross
                    | SnapKind::UnderlayFace
            ) && near_on_screen(input, point + dir * (p - point).dot(dir), free) =>
        {
            let on = crossing(input, fixed, point, dir, p, snap.kind);
            if let Some(at) = (input.project)(on) {
                crate::snaps::draw(painter, snap, at, None);
            }
            on
        }
        // nothing caught it: a face it slides into (3D), when near
        (Constraint::Line { point, dir }, _) => match on_face(input, pos, point, dir, free) {
            Some(on) => {
                if let Some(at) = (input.project)(on) {
                    let snap = crate::snaps::Snap {
                        p: on,
                        kind: SnapKind::OnFace,
                        host: None,
                    };
                    crate::snaps::draw(painter, snap, at, None);
                }
                on
            }
            None => free,
        },
        _ => free,
    };
    // distance marks from where it was grabbed, when no real snap caught it
    let now = if snapped
        .as_ref()
        .is_none_or(|(_, sn)| sn.kind == SnapKind::Grid)
    {
        distance_marks(painter, input, d.grab, now, crate::snaps::current_marks())
    } else {
        now
    };
    crate::actlog::line(format!(
        "drag frame: pane {} | pointer ({:.1}, {:.1}) px | free {} | snap {} | used {} | step {} mm | frame {:.1} ms",
        pane_name(input),
        pos.x,
        pos.y,
        crate::actlog::pt(free),
        match &snapped {
            Some((p, sn)) => format!("{:?} at {}", sn.kind, crate::actlog::pt(*p)),
            None => "none".into(),
        },
        crate::actlog::pt(now),
        crate::actlog::mm(crate::snaps::current_step()),
        painter.ctx().input(|i| i.unstable_dt) * 1000.0,
    ));
    d.last = now;
    let grid = snapped.is_none_or(|(_, s)| s.kind == SnapKind::Grid);
    follow(doc, painter, d, input, now, grid, Some(pos), status);
    painter.ctx().request_repaint();
    true
}

/// Distance marks (the Grid snap menu): faint marks every `every` metres
/// from `grab`, along the way the drag is going, drawn while it lasts.
/// Within 10 px of one the drag holds on it; elsewhere it stays where the
/// pointer put it. Marks closer than 6 px apart on screen are neither drawn
/// nor caught.
fn distance_marks(
    painter: &Painter,
    input: &PaneInput,
    grab: DVec3,
    now: DVec3,
    every: f64,
) -> DVec3 {
    if every <= 0.0 {
        return now;
    }
    let v = now - grab;
    let len = v.length();
    if len < 1e-9 {
        return now;
    }
    let dir = v / len;
    let (Some(a), Some(b)) = ((input.project)(grab), (input.project)(grab + dir * every)) else {
        return now;
    };
    let px = a.distance(b);
    if px < 6.0 {
        return now;
    }
    let held = mark_held(len, every, px as f64);
    let k = held.unwrap_or(0.0) / every;
    let held = held.is_some();
    let side = (b - a).normalized().rot90();
    let quiet = Stroke::new(1.0, Color32::from_rgb(79, 107, 76).gamma_multiply(0.75));
    let hot = Stroke::new(2.0, crate::planview::accent_selected());
    let n = ((len / every).ceil() as i64 + 1).min(400);
    for i in 1..=n {
        let Some(c) = (input.project)(grab + dir * (every * i as f64)) else {
            continue;
        };
        let caught = held && i as f64 == k;
        let (h, stroke) = if caught { (7.0, hot) } else { (4.0, quiet) };
        painter.line_segment([c - side * h, c + side * h], stroke);
    }
    if held {
        grab + dir * (k * every)
    } else {
        now
    }
}

/// The distance mark a drag `len` metres long holds on, if the pointer is
/// within 10 px of one (marks `every` metres apart, `px` pixels apart on
/// screen); never the grab point itself.
fn mark_held(len: f64, every: f64, px: f64) -> Option<f64> {
    let k = (len / every).round();
    (k >= 1.0 && ((len - k * every) / every).abs() * px <= 10.0).then_some(k * every)
}

/// Which kind of pane a drag is in, for the activity log.
fn pane_name(input: &PaneInput) -> &'static str {
    match input.view {
        None => "3D",
        Some(_) if input.look.z.abs() > 0.99 => "plan",
        Some(_) => "section or elevation",
    }
}

/// A click on grip `kind`: in Edit mode it picks a corner (Shift adds or
/// drops it), and a second click on the same corner takes it away; an
/// add-corner grip only says to drag it. Outside Edit mode a double-click
/// on a corner opens Edit mode. What the status bar says.
#[allow(clippy::too_many_arguments)]
fn tap_grip(
    doc: &mut Document,
    id: ElementId,
    kind: HandleKind,
    edit: bool,
    shift: bool,
    time: f64,
    at: Pos2,
    grips: &mut GripSel,
) -> Option<String> {
    let double = grips.tap(kind, time, at);
    // a mass in Edit mode: a click picks the vertex, the edge or the face,
    // and it stays picked to act on, as in Blender (MU39)
    if edit {
        if let Some(ElementKind::FreeMesh(m)) = doc.model().get(id).map(|e| &e.kind) {
            let vs: Option<Vec<u32>> = match kind {
                HandleKind::MeshVertex(i) => Some(vec![i as u32]),
                HandleKind::MeshEdge(a, b) => Some(vec![a, b]),
                HandleKind::Face(f) => m.mesh.faces.get(f).cloned(),
                _ => None,
            };
            if let Some(vs) = vs {
                grips.pick_mesh(&vs, shift);
                let what = match (crate::handles::pick(), kind) {
                    (_, HandleKind::MeshEdge(..)) | (2, _) => "edge",
                    (_, HandleKind::Face(_)) | (3, _) => "face",
                    _ => "point",
                };
                return Some(match grips.mesh.len() {
                    0 => "Nothing picked.".into(),
                    n => format!(
                        "{n} points picked ({what}): G or M moves them, R turns, S scales; Shift+click picks more."
                    ),
                });
            }
        }
    }
    match kind {
        HandleKind::Vertex { ring, i } if edit => {
            if double && !shift {
                let r = crate::editmode::remove_corners(doc, id, &[(ring, i)]);
                if r.is_ok() {
                    grips.clear();
                }
                return Some(r.unwrap_or_else(|e| e));
            }
            grips.pick(ring, i, shift);
            Some(match grips.corners.len() {
                0 => "Nothing picked.".into(),
                1 => "Corner selected: Delete removes it, a drag moves it, right-click for more."
                    .into(),
                n => format!("{n} corners selected: Delete removes them."),
            })
        }
        HandleKind::Vertex { .. } if double => {
            grips.request = Some(GripRequest::Enter);
            None
        }
        HandleKind::AddVertex { .. } => Some(
            "Drag the ◇ to pull out a new corner, or right-click the edge › Add corner here."
                .into(),
        ),
        _ => None,
    }
}

/// Remake the drag with the grip at `now`: the elements it changes shown
/// in place, or (an outline that would be broken) red, with the reason.
#[allow(clippy::too_many_arguments)]
fn follow(
    doc: &mut Document,
    painter: &Painter,
    d: &mut Drag,
    input: &PaneInput,
    now: DVec3,
    grid: bool,
    pointer: Option<Pos2>,
    status: &mut Option<String>,
) {
    let model = doc.model().clone();
    let result = match d.kind {
        Some(k) => apply(
            &model,
            d.edit.originals(),
            &d.joined,
            k,
            d.grab,
            now,
            crate::snaps::current_step(),
        ),
        None => {
            let delta = now - d.grab;
            let delta = if grid {
                let s = crate::snaps::current_step();
                if s > 0.0 {
                    (delta / s).round() * s
                } else {
                    delta
                }
            } else {
                delta
            };
            d.edit.originals()[..1]
                .iter()
                .map(|e| {
                    transform::transformed(&model, e, &Xform::Move(delta))
                        .ok_or_else(|| "this can't move that way".to_string())
                })
                .collect()
        }
    };
    let outline = matches!(
        d.kind,
        Some(
            HandleKind::Vertex { .. }
                | HandleKind::AddVertex { .. }
                | HandleKind::Edge { .. }
                | HandleKind::Ring { .. }
        )
    );
    match result {
        Ok(changed) => {
            let mut said = match d.kind {
                // a whole element moved: how far
                None => format!(
                    "{}: {}",
                    d.edit.label(),
                    crate::units::len((now - d.grab).length())
                ),
                Some(_) => format!("{}  {}", d.edit.label(), readout(&changed)),
            };
            crate::actlog::line(format!(
                "  result: {said} | step {} mm",
                crate::actlog::mm(crate::snaps::current_step())
            ));
            if let Some(ElementKind::Shaft(s)) = changed.first().map(|e| &e.kind) {
                let missed: Vec<String> = creabim_core::slab::shaft_misses(&model, s)
                    .into_iter()
                    .filter_map(|m| model.get(m).map(|e| e.name.clone()))
                    .collect();
                if !missed.is_empty() {
                    said += &format!(
                        "  Warning: it doesn't cut {}, as it crosses the edge",
                        missed.join(", ")
                    );
                }
            }
            if let (true, Some(k), Some(p), Some(e)) = (outline, d.kind, pointer, changed.first()) {
                note(
                    painter,
                    p,
                    &live_lengths(&model, e, k, d.grab, now),
                    interaction_ink(),
                );
            }
            // the live pill: only the value that's changing, by the grip
            let ctx = painter.ctx();
            if live_on(ctx) && !outline {
                let change = match (d.kind, d.edit.originals().first(), changed.first()) {
                    (None | Some(HandleKind::WallSlide), old, new) => Some((
                        "Moved ".to_string(),
                        bare(moved_by(old, new).unwrap_or((now - d.grab).length())),
                    )),
                    (Some(_), Some(old), Some(new)) => changing(old, new),
                    _ => None,
                };
                if let (Some((text, number)), Some(at)) = (change, (input.project)(now)) {
                    let look = GripLook::of(ctx);
                    pill_value(
                        painter,
                        at + Vec2::new(0.0, -34.0),
                        &text,
                        &number,
                        &look,
                        1.0,
                    );
                }
            }
            *status = Some(said);
            if let Err(e) = d.edit.update(doc, changed) {
                *status = Some(format!("Can't go there: {e}"));
            }
        }
        Err(e) => {
            // the broken shape in red, never kept: the last good one stays
            if let (true, Some(k), Some(orig)) = (outline, d.kind, d.edit.originals().first()) {
                let mut broken = orig.clone();
                if crate::editmode::reshape_unchecked(
                    &model,
                    &mut broken,
                    k,
                    d.grab,
                    now,
                    crate::snaps::current_step(),
                )
                .is_ok()
                {
                    draw_rings(
                        &model,
                        &broken,
                        painter,
                        input,
                        &GripSel::default(),
                        INVALID,
                    );
                }
                if let Some(p) = pointer {
                    note(painter, p, &format!("Not allowed: {e}"), INVALID);
                }
            }
            *status = Some(format!(
                "Can't go there: {e}. Release keeps the last good shape; Esc cancels."
            ));
        }
    }
}

/// A length typed into a drag: shown by the grip; Enter moves the grip
/// that far from where it was grabbed, the way the pointer went, and
/// finishes; Esc goes back to following the pointer. True when the drag
/// is done.
fn typed_length(
    doc: &mut Document,
    painter: &Painter,
    d: &mut Drag,
    input: &PaneInput,
    status: &mut Option<String>,
) -> bool {
    let ctx = painter.ctx().clone();
    let (text, back, enter, esc) = ctx.input_mut(|i| {
        let text: String = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        (
            text,
            i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
        )
    });
    let Some(t) = d.typed.as_mut() else {
        return false;
    };
    for c in text.chars() {
        match c {
            '0'..='9' | '.' | '-' | '\'' | '"' | '/' | ' ' => t.push(c),
            ',' => t.push('.'),
            'm' | 'c' | 'f' | 't' | 'i' | 'n' if !t.is_empty() => t.push(c),
            _ => {}
        }
    }
    if back {
        t.pop();
    }
    if esc {
        d.typed = None;
        *status = Some("Typing cancelled: the grip follows the pointer again.".into());
        return false;
    }
    let dir = match d.constraint {
        Constraint::Line { dir, .. } => {
            let s = (d.last - d.grab).dot(dir);
            if s < 0.0 {
                -dir
            } else {
                dir
            }
        }
        Constraint::Plane { .. } => (d.last - d.grab).normalize_or_zero(),
    };
    let shown = t.clone();
    let at = (input.project)(d.grab + dir * crate::units::parse(&shown).unwrap_or(0.0));
    if let Some(p) = at.or((input.project)(d.grab)) {
        note(
            painter,
            p,
            &format!(
                "{shown}| {}  Enter: apply · Esc: back to the pointer",
                crate::units::symbol()
            ),
            interaction_ink(),
        );
    }
    *status = Some(format!(
        "Type a length ({}) and press Enter: the grip goes that far the way you dragged it.",
        crate::units::symbol()
    ));
    if !enter {
        return false;
    }
    let Some(v) = crate::units::parse(&shown) else {
        *status = Some(format!("“{shown}” isn't a length."));
        return false;
    };
    if dir == DVec3::ZERO {
        *status = Some("Drag the grip a little first, to show which way it goes.".into());
        return false;
    }
    let now = d.grab + dir * v;
    follow(doc, painter, d, input, now, false, None, status);
    if d.edit.applied() {
        *status = doc.undo_label().map(|l| format!("{l}. Ctrl+Z to undo."));
    }
    true
}

#[cfg(test)]
mod reference_grip_tests {
    use super::*;
    use creabim_core::{RefSource, Reference};

    #[test]
    fn a_reference_moves_by_its_middle_scales_by_a_corner_and_turns() {
        let mut r = Reference::new(
            RefSource::Dxf {
                path: "survey.dxf".into(),
            },
            vec![],
        );
        r.in_place([DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0)]);
        let e = Element::new("Reference", ElementKind::Reference(r));
        let id = e.id;
        let model = Model::from_elements([e]).unwrap();
        let hs = handles(&model, id, None);
        let kinds: Vec<HandleKind> = hs.iter().map(|h| h.kind).collect();
        assert!(kinds.contains(&HandleKind::RefMove) && kinds.contains(&HandleKind::Rotate));
        assert_eq!(
            kinds
                .iter()
                .filter(|k| matches!(k, HandleKind::RefScale(_)))
                .count(),
            4
        );
        let drag = |kind: HandleKind, grab: DVec2, now: DVec2| {
            let (orig, joined) = originals(&model, id, kind);
            let out = apply(
                &model,
                &orig,
                &joined,
                kind,
                grab.extend(0.0),
                now.extend(0.0),
                0.0,
            )
            .unwrap();
            let ElementKind::Reference(r) = &out[0].kind else {
                panic!()
            };
            r.clone()
        };
        // its middle, dragged 2 m right
        let moved = drag(
            HandleKind::RefMove,
            DVec2::new(5.0, 3.0),
            DVec2::new(7.0, 3.0),
        );
        assert!(moved.at.distance(DVec2::new(7.0, 3.0)) < 1e-9);
        // a corner dragged twice as far from the middle doubles it
        let big = drag(
            HandleKind::RefScale(2),
            DVec2::new(10.0, 6.0),
            DVec2::new(15.0, 9.0),
        );
        assert!((big.scale - 2.0).abs() < 1e-9, "{}", big.scale);
        // the turn grip, a quarter turn about the middle
        let turned = drag(
            HandleKind::Rotate,
            DVec2::new(5.0, 9.0),
            DVec2::new(-1.0, 3.0),
        );
        assert!((turned.rotation - 90.0).abs() < 1e-9, "{}", turned.rotation);
        // locked, it has no grips
        let mut e2 = model.get(id).unwrap().clone();
        if let ElementKind::Reference(r) = &mut e2.kind {
            r.locked = true;
        }
        let locked = Model::from_elements([e2]).unwrap();
        assert!(handles(&locked, id, None).is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    /// A layered wall of the typed house, and the index of its core layer.
    fn layered_wall(m: &Model) -> (ElementId, usize) {
        m.iter()
            .find_map(|e| {
                let ElementKind::Wall(w) = &e.kind else {
                    return None;
                };
                let (_, a) = creabim_core::assembly::assembly_of(m, e)?;
                let bands = creabim_core::assembly::wall_bands(a, w);
                (bands.len() > 1)
                    .then(|| bands.iter().find(|b| b.core).map(|b| (e.id, b.index)))
                    .flatten()
            })
            .expect("a layered wall")
    }

    #[test]
    fn drawing_by_a_layer_boundary_keeps_the_wall_where_it_is() {
        let m = creabim_core::sample::typed_house();
        let (id, core) = layered_wall(&m);
        let mut doc = Document::from_model(m);
        let w = |doc: &Document| match &doc.model().get(id).unwrap().kind {
            ElementKind::Wall(w) => w.clone(),
            _ => unreachable!(),
        };
        let before = w(&doc);
        let k = (core + 1) as u8;
        let text = draw_by_boundary(&mut doc, id, k).unwrap();
        let after = w(&doc);
        assert_eq!(after.location, creabim_core::LocationLine::Boundary(k));
        assert_eq!((after.start, after.end), (before.start, before.end));
        assert!(text.contains("stays where it is"), "{text}");
        // the line drawn by is that boundary, on the core's interior face
        let off = creabim_core::walls::offset_of(doc.model(), &after, after.location);
        let want = creabim_core::walls::boundary_offset(doc.model(), &after, k as usize).unwrap();
        assert!((off - want).abs() < 1e-9);
        assert!(off.abs() < after.thickness / 2.0);
        let core_in = creabim_core::LocationLine::CoreInterior;
        let core_in = creabim_core::walls::offset_of(doc.model(), &after, core_in);
        assert!((off - core_in).abs() < 1e-9, "{off} vs {core_in}");
        // asking again says so
        assert!(draw_by_boundary(&mut doc, id, k).is_err());
    }

    #[test]
    fn a_layer_end_runs_on_by_its_grip_keeps_through_a_move_and_resets() {
        let m = creabim_core::sample::typed_house();
        let (id, core) = layered_wall(&m);
        assert!(has_layers(&m, id));
        let mut doc = Document::from_model(m);
        let kind = HandleKind::LayerEnd {
            layer: core,
            end: true,
        };
        let h = layer_handles(doc.model(), id)
            .into_iter()
            .find(|h| h.kind == kind)
            .unwrap();
        let w = |doc: &Document| match &doc.model().get(id).unwrap().kind {
            ElementKind::Wall(w) => w.clone(),
            _ => unreachable!(),
        };
        let dir = w(&doc).direction().extend(0.0);
        // 20 mm on past the end
        let (orig, joined) = originals(doc.model(), id, kind);
        let model = doc.model().clone();
        let out = apply(&model, &orig, &joined, kind, h.at, h.at + dir * 0.02, 0.0).unwrap();
        let new = out[0].clone();
        doc.edit(id, "drag", |e| *e = new.clone()).unwrap();
        assert!((w(&doc).layer_ext(core)[1] - 0.02).abs() < 1e-9);
        // its grip moved with it, and shows filled
        let moved = layer_handles(doc.model(), id)
            .into_iter()
            .find(|h| h.kind == kind)
            .unwrap();
        assert!((moved.at - h.at).dot(dir) > 0.019);
        assert!(edited_end(doc.model().get(id), kind));
        // the other layers and the start are as they were
        assert_eq!(w(&doc).layer_ext(core)[0], 0.0);
        // moved, the wall keeps its edit
        doc.edit(id, "move", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                w.start += DVec2::new(1.0, 0.5);
                w.end += DVec2::new(1.0, 0.5);
            }
        })
        .unwrap();
        assert!((w(&doc).layer_ext(core)[1] - 0.02).abs() < 1e-9);
        // and resetting puts it back
        reset_layer_ends(&mut doc, id, Some(kind)).unwrap();
        assert!(w(&doc).layer_ends.is_empty());
        assert!(reset_layer_ends(&mut doc, id, None).is_err());
    }

    #[test]
    fn a_wall_layer_runs_down_past_the_base_by_its_bottom_grip() {
        let m = creabim_core::sample::typed_house();
        let (id, core) = layered_wall(&m);
        // an outer layer: the one that runs down past the slab's edge
        let outer = if core == 0 { 1 } else { 0 };
        let mut doc = Document::from_model(m);
        let kind = HandleKind::LayerHeight {
            layer: outer,
            top: false,
        };
        let h = layer_handles(doc.model(), id)
            .into_iter()
            .find(|h| h.kind == kind)
            .expect("a bottom grip on every layer");
        let Constraint::Line { dir, .. } = h.constraint else {
            panic!()
        };
        assert_eq!(dir, DVec3::Z, "it slides up and down");
        let (orig, joined) = originals(doc.model(), id, kind);
        let model = doc.model().clone();
        let out = apply(
            &model,
            &orig,
            &joined,
            kind,
            h.at,
            h.at - DVec3::Z * 0.5,
            0.0,
        )
        .unwrap();
        let new = out[0].clone();
        doc.edit(id, "drag", |e| *e = new.clone()).unwrap();
        let ElementKind::Wall(w) = &doc.model().get(id).unwrap().kind else {
            panic!()
        };
        assert!((w.layer_vext(outer)[0] - 0.5).abs() < 1e-9);
        assert_eq!(w.layer_vext(core), [0.0, 0.0], "the core stays on the slab");
        // its solid goes 500 mm lower; the core's does not
        let low = |layer: usize| {
            creabim_geom::scene::build_scene_of(doc.model(), Some(&[id].into_iter().collect()))
                .iter()
                .filter(|i| i.layer.is_some_and(|l| l.index == layer))
                .flat_map(|i| i.mesh.positions.iter().map(|p| p[2] as f64))
                .fold(f64::MAX, f64::min)
        };
        assert!(
            (low(core) - low(outer) - 0.5).abs() < 1e-4,
            "{} {}",
            low(core),
            low(outer)
        );
        // the grip follows, and Reset puts it back
        let moved = layer_handles(doc.model(), id)
            .into_iter()
            .find(|h| h.kind == kind)
            .unwrap();
        assert!((h.at.z - moved.at.z - 0.5).abs() < 1e-9);
        reset_layer_ends(&mut doc, id, Some(kind)).unwrap();
        assert!(!edited_end(doc.model().get(id), kind));
    }

    #[test]
    fn slab_and_roof_layer_edges_push_out_by_their_grips() {
        let m = creabim_core::sample::typed_house();
        let layered = |f: fn(&ElementKind) -> bool| {
            m.iter()
                .find(|e| f(&e.kind) && has_layers(&m, e.id))
                .map(|e| e.id)
        };
        let slab = layered(|k| matches!(k, ElementKind::Slab(_))).expect("a layered slab");
        let roof = layered(|k| matches!(k, ElementKind::Roof(_)));
        for id in std::iter::once(slab).chain(roof) {
            let mut doc = Document::from_model(m.clone());
            let hs = layer_handles(doc.model(), id);
            let h = hs[0];
            assert!(matches!(
                h.kind,
                HandleKind::LayerEdge { layer: 0, edge: 0 }
            ));
            // the next layer's grip on the same edge is drawn further out
            assert!(hs.iter().any(|g| g.nudge > 0.0));
            let Constraint::Line { dir, .. } = h.constraint else {
                panic!()
            };
            let (orig, joined) = originals(doc.model(), id, h.kind);
            let model = doc.model().clone();
            let out = apply(&model, &orig, &joined, h.kind, h.at, h.at + dir * 0.05, 0.0).unwrap();
            let new = out[0].clone();
            doc.edit(id, "drag", |e| *e = new.clone()).unwrap();
            let e = doc.model().get(id).unwrap();
            assert!((layer_offset(&e.kind, h.kind).unwrap() - 0.05).abs() < 1e-9);
            assert!(edited_end(Some(e), h.kind));
            let moved = layer_handles(doc.model(), id)[0];
            assert!((moved.at - h.at).truncate().dot(dir.truncate()) > 0.049);
            reset_layer_ends(&mut doc, id, None).unwrap();
            assert!(!edited_end(doc.model().get(id), h.kind));
        }
    }

    #[test]
    fn a_layer_end_held_back_goes_the_other_way_at_the_start() {
        let m = creabim_core::sample::typed_house();
        let (id, core) = layered_wall(&m);
        let kind = HandleKind::LayerEnd {
            layer: core,
            end: false,
        };
        let h = layer_handles(&m, id)
            .into_iter()
            .find(|h| h.kind == kind)
            .unwrap();
        let ElementKind::Wall(w) = &m.get(id).unwrap().kind else {
            unreachable!()
        };
        // dragged back along the wall from its start: held back 50 mm
        let into = w.direction().extend(0.0) * 0.05;
        let (orig, joined) = originals(&m, id, kind);
        let out = apply(&m, &orig, &joined, kind, h.at, h.at + into, 0.0).unwrap();
        let ElementKind::Wall(w) = &out[0].kind else {
            unreachable!()
        };
        assert!((w.layer_ext(core)[0] + 0.05).abs() < 1e-9);
    }

    /// How far a wall's location line sits off its centreline.
    fn walls_offset(m: &Model, w: &creabim_core::Wall) -> f64 {
        let (a, _) = walls::location_line(m, w);
        (a - w.start).dot(w.direction().perp()).abs()
    }

    fn wall_with_door(m: &Model) -> (ElementId, ElementId) {
        m.iter()
            .find_map(|e| match &e.kind {
                ElementKind::Opening(o) if o.kind == OpeningKind::Door => Some((o.host, e.id)),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn detail_components_have_points_a_width_and_a_unit_to_drag() {
        use creabim_core::{DetailItem, View};
        let mut doc = Document::new();
        let section = Element::new(
            "Section",
            ElementKind::View(View::section(DVec2::ZERO, DVec2::new(5.0, 0.0), 3.0, true)),
        );
        let sid = section.id;
        let p = DVec2::new;
        let mut batt = DetailItem::new(
            sid,
            DetailKind::Repeat {
                module: 0.215,
                joint: 0.01,
            },
            vec![p(0.0, 0.0), p(2.0, 0.0)],
        );
        batt.width = 0.1;
        let d = Element::new("Brick", ElementKind::Detail(batt));
        let did = d.id;
        doc.commit(
            creabim_core::Transaction::new("setup")
                .insert(section)
                .insert(d),
        )
        .unwrap();
        let m = doc.model().clone();
        let hs = handles_in(&m, did, None, false);
        let kinds: Vec<HandleKind> = hs.iter().map(|h| h.kind).collect();
        assert!(kinds.contains(&HandleKind::Vertex {
            ring: Ring::Outline,
            i: 1
        }));
        assert!(kinds.contains(&HandleKind::DetailWidth));
        assert!(kinds.contains(&HandleKind::DetailSpacing));
        let get = |k| *hs.iter().find(|h| h.kind == k).unwrap();
        let drag = |h: Handle, by: DVec3| {
            let (orig, joined) = originals(&m, did, h.kind);
            let out = apply(&m, &orig, &joined, h.kind, h.at, h.at + by, 0.0).unwrap();
            match &out[0].kind {
                ElementKind::Detail(d) => d.clone(),
                _ => panic!(),
            }
        };
        // the section looks along +Y: up on its paper is +Z in the model
        let w = drag(get(HandleKind::DetailWidth), DVec3::Z * 0.05);
        assert!((w.width - 0.2).abs() < 1e-9);
        let u = drag(get(HandleKind::DetailSpacing), DVec3::X * 0.1);
        assert!(
            matches!(u.kind, DetailKind::Repeat { module, .. } if (module - 0.315).abs() < 1e-9)
        );
        let end = get(HandleKind::Vertex {
            ring: Ring::Outline,
            i: 1,
        });
        let e = drag(end, DVec3::new(0.5, 0.0, 1.0));
        assert!(e.points[1].distance(p(2.5, 1.0)) < 1e-9);
        // a plant: its crown and its height
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let lid = doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Level(_)))
            .unwrap()
            .id;
        let tree = Element::new(
            "Tree",
            ElementKind::Site(creabim_core::Site::plant(
                lid,
                p(20.0, 20.0),
                creabim_core::PlantForm::Tree,
                4.0,
                6.0,
            )),
        );
        let tid = tree.id;
        doc.commit(creabim_core::Transaction::new("tree").insert(tree))
            .unwrap();
        let m = doc.model().clone();
        let hs = handles(&m, tid, None);
        let canopy = *hs
            .iter()
            .find(|h| h.kind == HandleKind::PlantCanopy)
            .unwrap();
        let (orig, _) = originals(&m, tid, canopy.kind);
        let out = apply(
            &m,
            &orig,
            &[],
            canopy.kind,
            canopy.at,
            canopy.at + DVec3::X * 0.5,
            0.0,
        )
        .unwrap();
        assert!(matches!(
            out[0].kind,
            ElementKind::Site(creabim_core::Site {
                form: creabim_core::SiteForm::Plant { canopy, .. },
                ..
            }) if (canopy - 5.0).abs() < 1e-9
        ));
        assert!(hs.iter().any(|h| h.kind == HandleKind::PlantHeight));
    }

    #[test]
    fn a_callouts_box_resizes_with_its_grips_and_moves_with_its_body() {
        use creabim_core::View;
        let mut doc = Document::new();
        let p = DVec2::new;
        let section = View::section(DVec2::ZERO, DVec2::new(5.0, 0.0), 3.0, true);
        let s = Element::new("Section", ElementKind::View(section.clone()));
        let sid = s.id;
        let d = Element::new(
            "Detail 1",
            ElementKind::View(View::callout(sid, &section, p(1.0, 0.0), p(2.0, 1.0), 10.0)),
        );
        let did = d.id;
        doc.commit(creabim_core::Transaction::new("setup").insert(s).insert(d))
            .unwrap();
        let m = doc.model().clone();
        let hs = handles(&m, did, None);
        // four corners and four edges, none of the section line's
        assert_eq!(hs.len(), 8);
        assert!(hs
            .iter()
            .all(|h| !matches!(h.kind, HandleKind::ViewEnd { .. })));
        let get = |k| *hs.iter().find(|h| h.kind == k).unwrap();
        let drag = |h: Handle, by: DVec3| {
            let (orig, joined) = originals(&m, did, h.kind);
            let out = apply(&m, &orig, &joined, h.kind, h.at, h.at + by, 0.0).unwrap();
            match &out[0].kind {
                ElementKind::View(v) => v.callout.unwrap(),
                _ => panic!(),
            }
        };
        // on a section it is a ring: pushing the top up widens it about its middle
        let c = drag(get(HandleKind::CalloutEdge(2)), DVec3::Z * 0.5);
        assert!(c.lo.distance(p(0.75, 0.0)) < 1e-9 && c.hi.distance(p(2.25, 1.5)) < 1e-9);
        // a corner: the opposite one stays
        let c = drag(get(HandleKind::CalloutCorner(2)), DVec3::new(0.5, 0.0, 0.5));
        assert!(c.lo.distance(p(1.0, 0.0)) < 1e-9 && c.hi.distance(p(2.5, 1.5)) < 1e-9);
        // its body moves the box, not the line its view was cut on
        let moved = transform::transformed(
            &m,
            m.get(did).unwrap(),
            &Xform::Move(DVec3::new(0.25, 0.0, 0.5)),
        )
        .unwrap();
        let ElementKind::View(v) = moved.kind else {
            panic!()
        };
        assert!(v.callout.unwrap().lo.distance(p(1.25, 0.5)) < 1e-9);
        assert_eq!(v.kind, section.kind);
    }

    #[test]
    fn a_line_constraint_takes_the_point_nearest_the_ray() {
        let c = Constraint::Line {
            point: DVec3::ZERO,
            dir: DVec3::Z,
        };
        let r = Ray {
            o: DVec3::new(5.0, 0.0, 2.0),
            d: -DVec3::X,
        };
        assert!(c.hit(r).unwrap().distance(DVec3::new(0.0, 0.0, 2.0)) < 1e-9);
        assert!(!c.usable(DVec3::Z));
    }

    #[test]
    fn dragging_a_door_jamb_widens_it_and_the_head_raises_it() {
        let m = sample_house();
        let (_, door) = wall_with_door(&m);
        let hs = handles(&m, door, None);
        let right = hs
            .iter()
            .find(|h| h.kind == HandleKind::OpeningJamb { right: true })
            .unwrap();
        let ElementKind::Opening(o) = &m.get(door).unwrap().kind else {
            panic!()
        };
        let Constraint::Line { dir, .. } = right.constraint else {
            panic!()
        };
        let (orig, joined) = originals(&m, door, right.kind);
        let out = apply(
            &m,
            &orig,
            &joined,
            right.kind,
            right.at,
            right.at + dir * 0.2,
            0.05,
        )
        .unwrap();
        let ElementKind::Opening(n) = &out[0].kind else {
            panic!()
        };
        assert!((n.width - (o.width + 0.2)).abs() < 1e-9);
        assert!((n.offset - o.offset).abs() < 1e-9);
        let head = hs
            .iter()
            .find(|h| h.kind == HandleKind::OpeningHead)
            .unwrap();
        let out = apply(
            &m,
            &orig,
            &[],
            head.kind,
            head.at,
            head.at + DVec3::Z * 0.1,
            0.05,
        )
        .unwrap();
        let ElementKind::Opening(n) = &out[0].kind else {
            panic!()
        };
        assert!((n.height - (o.height + 0.1)).abs() < 1e-9);
        // far too tall for the wall
        assert!(apply(
            &m,
            &orig,
            &[],
            head.kind,
            head.at,
            head.at + DVec3::Z * 5.0,
            0.05
        )
        .is_err());
    }

    #[test]
    fn stretching_a_wall_end_takes_the_joined_wall_along() {
        let m = sample_house();
        let (id, w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        let kind = HandleKind::WallEnd { end: true };
        let (orig, joined) = originals(&m, id, kind);
        assert!(!joined.is_empty(), "sample walls meet at corners");
        let to = (w.end + w.direction() * 0.5).extend(0.0);
        let out = apply(&m, &orig, &joined, kind, w.end.extend(0.0), to, 0.05).unwrap();
        for &(i, oe) in &joined {
            let ElementKind::Wall(o) = &out[i].kind else {
                panic!()
            };
            let p = if oe { o.end } else { o.start };
            assert!(p.distance(to.truncate()) < 1e-9);
        }
    }

    #[test]
    fn a_wall_end_is_gripped_on_its_location_line_and_may_be_left_unjoined() {
        let m = sample_house();
        let (id, mut w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        w.location = creabim_core::LocationLine::ExteriorFace;
        w.disallow_join[1] = true;
        let mut doc = Document::from_model(m);
        doc.edit(id, "edit", |e| e.kind = ElementKind::Wall(w.clone()))
            .unwrap();
        let m = doc.model().clone();
        let kind = HandleKind::WallEnd { end: true };
        let grip = handles(&m, id, None)
            .into_iter()
            .find(|h| h.kind == kind)
            .unwrap();
        let (_, b) = walls::location_line(&m, &w);
        assert!(
            grip.at.truncate().distance(b) < 1e-9,
            "on the exterior face"
        );
        assert!(b.distance(w.end) > 0.05);
        // the end may not join: nothing comes along
        let (orig, joined) = originals(&m, id, kind);
        assert!(joined.is_empty() && orig.len() == 1);
        let to = (b + w.direction() * 0.5).extend(0.0);
        let out = apply(&m, &orig, &joined, kind, grip.at, to, 0.05).unwrap();
        let ElementKind::Wall(n) = &out[0].kind else {
            panic!()
        };
        let (_, nb) = walls::location_line(&m, n);
        assert!(nb.distance(to.truncate()) < 1e-9);
    }

    #[test]
    fn pulling_a_slab_underside_makes_it_thicker() {
        let m = sample_house();
        let (id, t) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.thickness)),
                _ => None,
            })
            .unwrap();
        let h = handles(&m, id, None)
            .into_iter()
            .find(|h| h.kind == HandleKind::SlabUnderside)
            .unwrap();
        let (orig, _) = originals(&m, id, h.kind);
        let out = apply(&m, &orig, &[], h.kind, h.at, h.at - DVec3::Z * 0.05, 0.05).unwrap();
        let ElementKind::Slab(s) = &out[0].kind else {
            panic!()
        };
        assert!((s.thickness - (t + 0.05)).abs() < 1e-9);
        // the top stayed at the level
        assert!(s.top_offset.abs() < 1e-12);
    }

    #[test]
    fn pulling_a_slab_top_moves_the_top_and_keeps_the_underside() {
        let m = sample_house();
        let (id, t) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.thickness)),
                _ => None,
            })
            .unwrap();
        let hs = handles(&m, id, None);
        let top = hs.iter().find(|h| h.kind == HandleKind::SlabTop).unwrap();
        let under = hs
            .iter()
            .find(|h| h.kind == HandleKind::SlabUnderside)
            .unwrap();
        assert!((top.at.z - under.at.z - t).abs() < 1e-9);
        let (orig, _) = originals(&m, id, top.kind);
        let out = apply(
            &m,
            &orig,
            &[],
            top.kind,
            top.at,
            top.at + DVec3::Z * 0.1,
            0.05,
        )
        .unwrap();
        let ElementKind::Slab(s) = &out[0].kind else {
            panic!()
        };
        assert!((s.thickness - (t + 0.1)).abs() < 1e-9);
        assert!((s.top_offset - 0.1).abs() < 1e-9);
        assert!((s.top_offset - s.thickness + t).abs() < 1e-9);
    }

    #[test]
    fn slab_corners_can_be_added_on_an_edge_and_taken_away() {
        let m = creabim_core::sample::sample_house();
        let (id, n) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.boundary.len())),
                _ => None,
            })
            .unwrap();
        let mut doc = Document::from_model(m);
        use crate::editmode::{add_corner, remove_corners};
        add_corner(&mut doc, id, Ring::Outline, 0, None).unwrap();
        let corners = |doc: &Document| match &doc.model().get(id).unwrap().kind {
            ElementKind::Slab(s) => s.boundary.clone(),
            _ => panic!(),
        };
        let b = corners(&doc);
        assert_eq!(b.len(), n + 1);
        assert!(b[1].distance((b[0] + b[2]) / 2.0) < 1e-9);
        remove_corners(&mut doc, id, &[(Ring::Outline, 1)]).unwrap();
        assert_eq!(corners(&doc).len(), n);
        while corners(&doc).len() > 3 {
            remove_corners(&mut doc, id, &[(Ring::Outline, 0)]).unwrap();
        }
        assert!(remove_corners(&mut doc, id, &[(Ring::Outline, 0)]).is_err());
    }

    #[test]
    fn plan_handles_slide_along_a_section() {
        let m = sample_house();
        let (id, _) = wall_with_door(&m);
        // a section looking north: wall ends slide east and west
        let look = DVec3::Y;
        let end = handles(&m, id, None)
            .into_iter()
            .find(|h| matches!(h.kind, HandleKind::WallEnd { .. }))
            .unwrap();
        assert!(!end.constraint.usable(look));
        let h = in_section(end, look);
        assert!(h.constraint.usable(look));
        let Constraint::Line { dir, .. } = h.constraint else {
            panic!()
        };
        assert!(dir.dot(look).abs() < 1e-9 && dir.z.abs() < 1e-9);
        // a ray straight into the paper lands on the line, at the pointer
        let ray = Ray {
            o: DVec3::new(3.0, -50.0, end.at.z),
            d: look,
        };
        let p = h.constraint.hit(ray).unwrap();
        assert!((p.x - 3.0).abs() < 1e-9);
        // plans keep the handle as it was
        assert_eq!(in_section(end, -DVec3::Z), end);
    }

    fn a_slab(doc: &Document) -> (ElementId, Vec<DVec2>) {
        doc.model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, s.boundary.clone())),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_double_click_on_an_add_corner_grip_changes_nothing() {
        let mut doc = Document::from_model(sample_house());
        let (id, before) = a_slab(&doc);
        let mut sel = GripSel::default();
        sel.reset(Some(id));
        let k = HandleKind::AddVertex {
            ring: Ring::Outline,
            i: 0,
        };
        let p = Pos2::new(100.0, 100.0);
        for t in [0.0, 0.2] {
            tap_grip(&mut doc, id, k, true, false, t, p, &mut sel);
        }
        assert_eq!(a_slab(&doc).1, before);
        assert!(!doc.can_undo(), "no undo step at all");
    }

    #[test]
    fn a_double_click_on_a_corner_takes_it_away_even_after_a_small_move() {
        let mut doc = Document::from_model(sample_house());
        let (id, _) = a_slab(&doc);
        doc.edit(id, "five", |e| {
            if let ElementKind::Slab(s) = &mut e.kind {
                s.split_edge(0);
            }
        })
        .unwrap();
        let n = a_slab(&doc).1.len();
        let mut sel = GripSel::default();
        sel.reset(Some(id));
        let k = HandleKind::Vertex {
            ring: Ring::Outline,
            i: 1,
        };
        // the first click picks it
        tap_grip(
            &mut doc,
            id,
            k,
            true,
            false,
            1.0,
            Pos2::new(50.0, 50.0),
            &mut sel,
        );
        assert!(sel.picked(Ring::Outline, 1));
        assert_eq!(a_slab(&doc).1.len(), n);
        // the second, three pixels off, takes it away
        tap_grip(
            &mut doc,
            id,
            k,
            true,
            false,
            1.3,
            Pos2::new(53.0, 50.0),
            &mut sel,
        );
        assert_eq!(a_slab(&doc).1.len(), n - 1);
        // clicks far apart in time only pick
        tap_grip(
            &mut doc,
            id,
            k,
            true,
            false,
            5.0,
            Pos2::new(50.0, 50.0),
            &mut sel,
        );
        tap_grip(
            &mut doc,
            id,
            k,
            true,
            false,
            6.0,
            Pos2::new(50.0, 50.0),
            &mut sel,
        );
        assert_eq!(a_slab(&doc).1.len(), n - 1);
        // Shift+click adds to the picks, and again takes away
        let other = HandleKind::Vertex {
            ring: Ring::Outline,
            i: 0,
        };
        tap_grip(
            &mut doc,
            id,
            other,
            true,
            true,
            8.0,
            Pos2::new(0.0, 0.0),
            &mut sel,
        );
        assert_eq!(sel.corners.len(), 2);
        tap_grip(
            &mut doc,
            id,
            other,
            true,
            true,
            9.0,
            Pos2::new(0.0, 0.0),
            &mut sel,
        );
        assert_eq!(sel.corners.len(), 1);
        // outside Edit mode a double-click on a corner asks for Edit mode
        let mut out = GripSel::default();
        tap_grip(&mut doc, id, k, false, false, 10.0, Pos2::ZERO, &mut out);
        // the picks are cleared every frame outside Edit mode; the click
        // must survive that
        out.clear();
        tap_grip(&mut doc, id, k, false, false, 10.2, Pos2::ZERO, &mut out);
        assert_eq!(out.request, Some(GripRequest::Enter));
        assert_eq!(a_slab(&doc).1.len(), n - 1);
    }

    #[test]
    fn delete_picked_removes_holes_or_corners_and_never_the_slab() {
        let mut doc = Document::from_model(sample_house());
        let (id, b) = a_slab(&doc);
        let c = b.iter().copied().sum::<DVec2>() / b.len() as f64;
        doc.edit(id, "hole", |e| {
            if let ElementKind::Slab(s) = &mut e.kind {
                s.holes = vec![creabim_core::polygon::rectangle(c, c + DVec2::ONE)];
            }
        })
        .unwrap();
        let mut sel = GripSel::default();
        sel.reset(Some(id));
        assert!(delete_picked(&mut doc, id, &mut sel).is_err());
        sel.pick_hole(0, false);
        delete_picked(&mut doc, id, &mut sel).unwrap();
        let ElementKind::Slab(s) = &doc.model().get(id).unwrap().kind else {
            panic!()
        };
        assert!(s.holes.is_empty());
        // a triangle keeps its corners
        sel.corners = (0..b.len()).map(|i| (Ring::Outline, i)).collect();
        assert!(delete_picked(&mut doc, id, &mut sel).is_err());
        assert!(doc.model().get(id).is_some());
    }

    fn drag_to(
        m: &Model,
        id: ElementId,
        kind: HandleKind,
        to: impl Fn(&Handle) -> DVec3,
    ) -> Element {
        let h = handles_in(m, id, None, false)
            .into_iter()
            .chain(handles_in(m, id, None, true))
            .find(|h| h.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?}"));
        let (orig, joined) = originals(m, id, kind);
        apply(m, &orig, &joined, kind, h.at, to(&h), 0.05).unwrap()[0].clone()
    }

    #[test]
    fn side_bars_sit_at_the_middle_of_their_side_not_on_a_corner() {
        // A4-8: a bar that pushes a side across sits between the element's
        // bottom and top, so in a section it is half-way up the side
        let m = creabim_core::starter::starter_house();
        let scene = creabim_geom::scene::build_scene_of(&m, None);
        let mut checked = 0;
        for e in m.iter() {
            let (lo, hi) = scene
                .iter()
                .filter(|i| i.id == e.id)
                .flat_map(|i| i.mesh.positions.iter().map(|p| p[2] as f64))
                .fold((f64::MAX, f64::MIN), |(a, b), z| (a.min(z), b.max(z)));
            if lo >= hi {
                continue;
            }
            for h in handles_in(&m, e.id, None, false) {
                let Constraint::Line { dir, .. } = h.constraint else {
                    continue;
                };
                // a member's other grips are its real ends (a stair's run);
                // a slab's or roof's edge bars too (MU25)
                let side = matches!(
                    h.kind,
                    HandleKind::WallFace | HandleKind::BeamWidth | HandleKind::ColumnSize { .. }
                ) || (matches!(h.kind, HandleKind::Edge { .. })
                    && matches!(e.kind, ElementKind::Slab(_) | ElementKind::Roof(_)))
                    || (matches!(h.kind, HandleKind::Member { .. })
                        && matches!(e.kind, ElementKind::Footing(_)));
                if !side || dir.z.abs() > 0.1 {
                    continue;
                }
                checked += 1;
                let z = h.at.z;
                assert!(
                    z > lo + 1e-3 && z < hi - 1e-3,
                    "{} {:?} at z {z} on its edge ({lo}..{hi})",
                    e.name,
                    h.kind
                );
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn a_mass_has_grips_on_every_face_corner_and_edge_and_moves_turns_by_them() {
        let mut doc = creabim_core::Document::new();
        let mass = creabim_core::FreeMesh::new(
            None,
            DVec3::new(1.0, 2.0, 0.0),
            creabim_core::editmesh::EditMesh::cuboid(DVec3::new(2.0, 1.0, 3.0)),
        );
        let e = Element::new("Mass", ElementKind::FreeMesh(mass));
        let id = e.id;
        doc.commit(creabim_core::Transaction::new("Mass").insert(e))
            .unwrap();
        let m = doc.model().clone();
        // at rest: the base point and the turn grip only
        let rest = handles_in(&m, id, None, false);
        assert_eq!(rest.len(), 2, "{rest:?}");
        // a clicked face adds its arrow
        assert_eq!(handles_in(&m, id, Some(0), false).len(), 3);
        let hs = handles_in(&m, id, None, true);
        let count = |f: fn(&HandleKind) -> bool| hs.iter().filter(|h| f(&h.kind)).count();
        assert_eq!(
            count(|k| matches!(k, HandleKind::Face(_))),
            6,
            "an arrow on every face"
        );
        assert_eq!(count(|k| matches!(k, HandleKind::MeshVertex(_))), 8);
        assert_eq!(count(|k| matches!(k, HandleKind::MeshEdge(..))), 12);
        for k in [
            HandleKind::MeshMove,
            HandleKind::MeshLift,
            HandleKind::Rotate,
        ] {
            assert_eq!(hs.iter().filter(|h| h.kind == k).count(), 1, "{k:?}");
        }
        // the base point moves the whole mass, to the step
        let out = drag_to(&m, id, HandleKind::MeshMove, |h| {
            h.at + DVec3::new(0.52, -0.31, 0.0)
        });
        let ElementKind::FreeMesh(t) = &out.kind else {
            panic!()
        };
        assert!(
            t.origin.distance(DVec3::new(1.5, 1.7, 0.0)) < 1e-9,
            "{:?}",
            t.origin
        );
        // a corner moves alone
        let out = drag_to(&m, id, HandleKind::MeshVertex(0), |h| h.at + DVec3::X * 0.5);
        let ElementKind::FreeMesh(t) = &out.kind else {
            panic!()
        };
        let ElementKind::FreeMesh(was) = &m.get(id).unwrap().kind else {
            panic!()
        };
        let moved = (0..8)
            .filter(|i| t.mesh.vertices[*i].distance(was.mesh.vertices[*i]) > 1e-9)
            .count();
        assert_eq!(moved, 1);
        // the turn grip turns it about its base point
        let out = drag_to(&m, id, HandleKind::Rotate, |h| {
            let o = DVec2::new(1.0, 2.0);
            (o + (h.at.truncate() - o).perp()).extend(h.at.z)
        });
        let ElementKind::FreeMesh(t) = &out.kind else {
            panic!()
        };
        assert!((t.rotation - 90.0).abs() < 1e-6, "{}", t.rotation);
        assert_eq!(t.origin, DVec3::new(1.0, 2.0, 0.0));
    }

    #[test]
    fn a_drags_pill_shows_only_the_value_that_changes() {
        let m = sample_house();
        let (id, _) = wall_with_door(&m);
        let old = m.get(id).unwrap().clone();
        let mut new = old.clone();
        let ElementKind::Wall(w) = &mut new.kind else {
            panic!()
        };
        let len = w.length();
        let dir = w.direction();
        w.end += dir * 0.6;
        let (text, number) = changing(&old, &new).unwrap();
        assert_eq!(text, format!("Length {} → ", bare(len)));
        assert_eq!(number, format!("{} +600", bare(len + 0.6)));
        // nothing named changed: no pill
        assert!(changing(&old, &old).is_none());
    }

    #[test]
    fn value_pills_are_legible_in_every_theme() {
        for t in crate::theme::Theme::ALL {
            let p = PillLook::of(&t.palette());
            for (what, c) in [("text", p.text), ("number", p.number)] {
                let r = crate::theme::contrast_ratio(c, p.ground);
                assert!(r >= 4.5, "{}: the pill's {what} is {r:.2}:1", t.id());
            }
            assert_ne!(p.ground, p.text);
        }
    }

    #[test]
    fn a_wall_shows_no_floating_slide_grip() {
        // the slide grip is kept for the body drag, but the grips drawn
        // all sit on the wall: none off its faces
        let m = sample_house();
        let (id, _) = wall_with_door(&m);
        let ElementKind::Wall(w) = &m.get(id).unwrap().kind else {
            panic!()
        };
        let n = w.direction().perp();
        for h in handles(&m, id, None) {
            if h.kind == HandleKind::WallSlide {
                continue;
            }
            let off = (h.at.truncate() - w.start).dot(n).abs();
            assert!(
                off <= w.thickness / 2.0 + 1e-6 + walls_offset(&m, w),
                "{:?} sits {off} off the wall",
                h.kind
            );
        }
    }

    #[test]
    fn a_wall_slides_square_to_itself() {
        let m = sample_house();
        let (id, w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        let n = w.direction().perp();
        let out = drag_to(&m, id, HandleKind::WallSlide, |h| {
            h.at + n.extend(0.0) * 0.5
        });
        let ElementKind::Wall(s) = &out.kind else {
            panic!()
        };
        assert!(s.start.distance(w.start + n * 0.5) < 1e-9);
        assert!(s.end.distance(w.end + n * 0.5) < 1e-9);
        assert!((s.thickness - w.thickness).abs() < 1e-12);
    }

    #[test]
    fn a_column_turns_and_changes_width_and_depth() {
        let m = creabim_core::sample::layered_house();
        let Some(id) = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Column(_)))
            .map(|e| e.id)
        else {
            return;
        };
        let ElementKind::Column(c) = &m.get(id).unwrap().kind else {
            panic!()
        };
        // a quarter turn about its point
        let out = drag_to(&m, id, HandleKind::Rotate, |h| {
            let r = h.at.truncate() - c.position;
            (c.position + r.perp()).extend(h.at.z)
        });
        let ElementKind::Column(t) = &out.kind else {
            panic!()
        };
        assert!(((t.rotation - c.rotation).rem_euclid(360.0) - 90.0).abs() < 1e-6);
        let creabim_core::members::Profile::Rectangle { width, .. } = c.profile else {
            return;
        };
        let x = DVec2::from_angle(c.rotation.to_radians());
        let out = drag_to(&m, id, HandleKind::ColumnSize { depth: false }, |h| {
            h.at + x.extend(0.0) * 0.05
        });
        let ElementKind::Column(t) = &out.kind else {
            panic!()
        };
        let creabim_core::members::Profile::Rectangle { width: w2, .. } = t.profile else {
            panic!()
        };
        assert!((w2 - (width + 0.1)).abs() < 1e-9);
        assert!(t.position.distance(c.position) < 1e-12);
    }

    #[test]
    fn an_object_turns_about_its_middle_and_turns_round() {
        let m = creabim_core::starter::starter_house();
        let Some(e) = m
            .iter()
            .find(|e| {
                matches!(e.kind, ElementKind::Object(_))
                    && creabim_core::objects::type_of(&m, e).is_some()
            })
            .cloned()
        else {
            return;
        };
        let mid = pivot_of(&m, &e).unwrap();
        let mut turned = e.clone();
        turn_object(&m, &mut turned, 180.0);
        // the footprint stays where it was, facing the other way
        assert!(pivot_of(&m, &turned).unwrap().distance(mid) < 1e-9);
        let (ElementKind::Object(a), ElementKind::Object(b)) = (&e.kind, &turned.kind) else {
            panic!()
        };
        assert!((a.facing() + b.facing()).length() < 1e-9);
        assert!(handles(&m, e.id, None)
            .iter()
            .any(|h| h.kind == HandleKind::Rotate));
    }

    #[test]
    fn a_door_slides_along_its_wall_at_the_same_width() {
        let m = sample_house();
        let (_, door) = wall_with_door(&m);
        let ElementKind::Opening(o) = &m.get(door).unwrap().kind else {
            panic!()
        };
        let h = handles(&m, door, None)
            .into_iter()
            .find(|h| h.kind == HandleKind::OpeningSlide)
            .unwrap();
        let Constraint::Line { dir, .. } = h.constraint else {
            panic!()
        };
        let out = drag_to(&m, door, HandleKind::OpeningSlide, |h| h.at - dir * 0.1);
        let ElementKind::Opening(n) = &out.kind else {
            panic!()
        };
        assert!((n.offset - (o.offset - 0.1)).abs() < 1e-9);
        assert!((n.width - o.width).abs() < 1e-12);
    }

    /// Snapping is free by default (the owner's review): no grid snap, no
    /// step, no distance marks; drags go exactly where the pointer is.
    #[test]
    fn drags_are_free_by_default() {
        let s = crate::snaps::SnapSettings::default();
        assert!(!s.increment && !s.marks);
        assert_eq!(s.step_len(), 0.0);
        assert_eq!(s.marks_len(), 0.0);
        assert_eq!(s.step_label(), "Off");
    }

    /// A distance mark holds a drag only within 10 px of it, never at the
    /// grab point, and lets go beyond.
    #[test]
    fn distance_marks_hold_only_when_near() {
        // marks 50 mm apart, 40 px apart on screen: 10 px is 12.5 mm
        assert_eq!(super::mark_held(0.112, 0.05, 40.0), Some(0.1));
        assert_eq!(super::mark_held(0.088, 0.05, 40.0), Some(0.1));
        assert_eq!(super::mark_held(0.075, 0.05, 40.0), None);
        assert_eq!(super::mark_held(0.004, 0.05, 40.0), None);
        // the same distances with marks only 10 px apart: always near one
        assert_eq!(super::mark_held(0.077, 0.05, 10.0), Some(0.1));
    }

    /// A grip sliding along a line (a wall's top going up) stops on a face
    /// across its path when the pointer puts it near, and not otherwise.
    #[test]
    fn a_sliding_grip_stops_on_a_face_in_3d() {
        // a side view: x across, z up, 100 px a metre
        let project = |p: DVec3| Some(Pos2::new(p.x as f32 * 100.0, -p.z as f32 * 100.0));
        let ray = |_: Pos2| None;
        let snap = |_: Pos2, _: Option<DVec3>| None;
        let none = |_: Pos2, _: ElementId| None;
        // a slab's soffit at 2.5 m, and an upright face the path runs along
        let soffit = |_: Pos2| Some((DVec3::new(0.3, 0.0, 2.5), DVec3::Z));
        let upright = |_: Pos2| Some((DVec3::new(1.0, 0.0, 2.5), DVec3::X));
        let input = |face: &dyn Fn(Pos2) -> Option<(DVec3, DVec3)>| {
            let (point, dir) = (DVec3::new(0.0, 0.0, 2.0), DVec3::Z);
            let pane = PaneInput {
                ray: &ray,
                project: &project,
                look: DVec3::Y,
                snap: &snap,
                grab_body: &none,
                grab_snap: &none,
                body_normal: DVec3::Z,
                view: None,
                face,
            };
            [2.47, 2.0]
                .map(|z| super::on_face(&pane, Pos2::ZERO, point, dir, DVec3::new(0.0, 0.0, z)))
        };
        // 3 px short of it: on it; 50 px short: free
        let [near, far] = input(&soffit);
        assert!(near.is_some_and(|p| (p.z - 2.5).abs() < 1e-9));
        assert!(far.is_none());
        // a face the path runs along: never
        assert_eq!(input(&upright), [None, None]);
    }
}
