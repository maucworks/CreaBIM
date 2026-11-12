use crate::{triangulate::triangulate, wall::wall_mesh, TriMesh};
use creabim_core::{assembly, csg, BoolOp, Element, ElementId, ElementKind, Model, Opening};
use glam::{DVec2, DVec3};

/// Roughness, metallic and opacity of an element without a material.
pub const PLAIN_SURFACE: [f32; 3] = [creabim_core::element::DEFAULT_ROUGHNESS, 0.0, 1.0];

/// One element's renderable geometry.
#[derive(Clone, Debug)]
pub struct SceneItem {
    pub id: ElementId,
    pub mesh: TriMesh,
    pub color: [f32; 3],
    /// Roughness, metallic and opacity, from the element's material.
    pub surface: [f32; 3],
    /// A cutter: drawn as a see-through outline in 3D, left out of
    /// drawings and exports, but still clickable.
    pub ghost: bool,
    /// For one layer of a layered wall, slab or roof: which layer. A
    /// layered element is one item per layer, all with its id.
    pub layer: Option<LayerPart>,
    /// A look of its own over the element's material, by name: a
    /// window's glass, a door's handle.
    pub finish: Option<&'static str>,
}

/// Which layer of its element's assembly a scene item is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPart {
    pub index: usize,
    pub material: Option<ElementId>,
    pub function: creabim_core::LayerFunction,
    pub core: bool,
    /// Its join priority: lower runs through higher where elements meet.
    pub priority: u8,
}

impl LayerPart {
    pub fn of(b: &assembly::Band) -> Self {
        Self {
            index: b.index,
            material: b.material,
            function: b.function,
            core: b.core,
            priority: b.priority,
        }
    }
}

/// Default colours for layers with no material, by function.
fn layer_color(f: creabim_core::LayerFunction, plain: [f32; 3]) -> [f32; 3] {
    use creabim_core::LayerFunction as F;
    match f {
        F::Insulation => [0.93, 0.86, 0.45],
        F::Membrane => [0.25, 0.27, 0.30],
        F::Finish => [0.92, 0.91, 0.88],
        _ => plain,
    }
}

/// The solid of each layer of a layered element, in model coordinates,
/// air gaps left out: what an export writes as the element's parts.
/// Empty for an element that isn't layered.
pub fn layer_parts(
    model: &Model,
    e: &Element,
    joins: &std::collections::HashMap<ElementId, creabim_core::WallEnds>,
) -> Vec<(LayerPart, TriMesh)> {
    layer_items(model, e, joins, [0.8; 3])
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_lining())
        .filter_map(|i| Some((i.layer?, i.mesh)))
        .collect()
}

/// The solid of each layer of a layered element, with its look; air gaps
/// are left empty. `None` for an element that isn't layered.
fn layer_items(
    model: &Model,
    e: &Element,
    joins: &std::collections::HashMap<ElementId, creabim_core::WallEnds>,
    plain: [f32; 3],
) -> Option<Vec<SceneItem>> {
    let (_, a) = assembly::assembly_of(model, e)?;
    let mut linings: Vec<SceneItem> = Vec::new();
    let parts: Vec<(assembly::Band, TriMesh)> = match &e.kind {
        ElementKind::Wall(w) => {
            let level = model.level(w.level)?;
            // its doors and windows, and the curtain walls set in it
            let owned = creabim_core::curtain::wall_openings(model, e.id);
            let openings: Vec<&Opening> = owned.iter().collect();
            let ends = joins.get(&e.id).cloned().unwrap_or_default();
            let top = creabim_core::walls::top_profile(model, w);
            linings = reveal_linings(
                model,
                e.id,
                w,
                &openings,
                &assembly::wall_bands(a, w),
                level.elevation,
                plain,
            );
            assembly::wall_bands(a, w)
                .into_iter()
                .map(|b| {
                    let m = crate::wall::wall_layer_mesh_v(
                        w,
                        level.elevation,
                        &openings,
                        &ends,
                        top.as_ref(),
                        (b.s0, b.s1),
                        b.core,
                        b.ext,
                        b.vext,
                    );
                    (b, m)
                })
                .collect()
        }
        ElementKind::Slab(s) => {
            let level = model.level(s.level)?;
            let top = s.top(level.elevation);
            let holes = creabim_core::slab::holes_of(model, e);
            assembly::depth_bands(a, s.thickness)
                .into_iter()
                .map(|b| {
                    let outline = s.layer_outline(b.index);
                    let m = slab_mesh_with_holes(&outline, &holes, top - b.s0, b.s1 - b.s0);
                    (b, m)
                })
                .collect()
        }
        ElementKind::Roof(r) => {
            let base = r.base(model);
            // layers are square to the slope; upright they are thicker
            let k = r.vertical_thickness() / r.thickness.max(1e-9);
            let tv = r.vertical_thickness();
            let holes = creabim_core::slab::holes_of(model, e);
            assembly::depth_bands(a, r.thickness)
                .into_iter()
                .map(|b| {
                    let m = crate::members::roof_layer_mesh(
                        r,
                        base,
                        tv - b.s1 * k,
                        tv - b.s0 * k,
                        Some(b.index),
                    );
                    (b, crate::members::cut_holes(m, &holes))
                })
                .collect()
        }
        _ => return None,
    };
    Some(
        parts
            .into_iter()
            .filter(|(b, _)| b.function != creabim_core::LayerFunction::AirGap)
            .map(|(b, mesh)| {
                let look = b.material.and_then(|m| match &model.get(m)?.kind {
                    ElementKind::Material(m) => Some((m.color, m.surface())),
                    _ => None,
                });
                let (color, surface) =
                    look.unwrap_or((layer_color(b.function, plain), PLAIN_SURFACE));
                SceneItem {
                    id: e.id,
                    mesh,
                    color,
                    surface,
                    ghost: false,
                    layer: Some(LayerPart::of(&b)),
                    finish: None,
                }
            })
            .chain(linings)
            .collect(),
    )
}

/// The [`SceneItem::finish`] of a reveal lining.
pub const REVEAL: &str = "Reveal";

/// How thick a reveal lining is: thin enough to leave the opening its size.
const LINING: f64 = 0.004;

impl SceneItem {
    /// A reveal lining, seen in 3D only: drawings and exports leave it out.
    pub fn is_lining(&self) -> bool {
        self.finish == Some(REVEAL)
    }
}

/// The reveals of a layered wall's doors and windows lined, for the 3D
/// view: each layer's part of the jambs, head and sill in the finish on
/// its side of the wall (render outside, plaster inside), so an opening
/// doesn't show the insulation or the membrane cut open.
fn reveal_linings(
    model: &Model,
    id: ElementId,
    w: &creabim_core::Wall,
    openings: &[&Opening],
    bands: &[assembly::Band],
    level_z: f64,
    plain: [f32; 3],
) -> Vec<SceneItem> {
    use creabim_core::LayerFunction as F;
    let look = |b: &assembly::Band| {
        b.material
            .and_then(|m| match &model.get(m)?.kind {
                ElementKind::Material(m) => Some((m.color, m.surface())),
                _ => None,
            })
            .unwrap_or((layer_color(b.function, plain), PLAIN_SURFACE))
    };
    let finishes: Vec<&assembly::Band> = bands.iter().filter(|b| b.function == F::Finish).collect();
    let d = w.direction();
    let n = DVec2::new(-d.y, d.x);
    let z0 = level_z + w.base_offset;
    let at = |u: f64, v: f64, s: f64| (w.start + d * u + n * s).extend(z0 + v);
    let mut out = Vec::new();
    for o in openings {
        if o.kind == creabim_core::OpeningKind::Hole {
            continue;
        }
        let Some(c) = crate::wall::Cut::of(o, w) else {
            continue;
        };
        for b in bands.iter().filter(|b| b.function != F::AirGap) {
            // a finish shows itself; the rest take the finish nearest them
            let mid = (b.s0 + b.s1) / 2.0;
            let from = if b.function == F::Finish {
                Some(b)
            } else {
                finishes.iter().copied().min_by(|x, y| {
                    ((x.s0 + x.s1) / 2.0 - mid)
                        .abs()
                        .total_cmp(&((y.s0 + y.s1) / 2.0 - mid).abs())
                })
            };
            let (color, surface) = from.map_or((plain, PLAIN_SURFACE), look);
            let mut mesh = TriMesh::default();
            let mut slab = |u0: f64, u1: f64, v0: f64, v1: f64| {
                let p = |i: usize| {
                    at(
                        if i & 1 == 0 { u0 } else { u1 },
                        if i & 2 == 0 { v0 } else { v1 },
                        if i & 4 == 0 { b.s0 } else { b.s1 },
                    )
                };
                cuboid(&mut mesh, [p(0), p(1), p(2), p(3), p(4), p(5), p(6), p(7)]);
            };
            slab(c.u0, c.u0 + LINING, c.v0, c.v1);
            slab(c.u1 - LINING, c.u1, c.v0, c.v1);
            slab(c.u0, c.u1, c.v1 - LINING, c.v1);
            if c.v0 > 1e-6 {
                slab(c.u0, c.u1, c.v0, c.v0 + LINING);
            }
            // one item per finish, for all the wall's openings
            match out.iter_mut().find(|i: &&mut SceneItem| i.color == color) {
                Some(i) => i.mesh.append(&mesh),
                None => out.push(SceneItem {
                    id,
                    mesh,
                    color,
                    surface,
                    ghost: false,
                    layer: None,
                    finish: Some(REVEAL),
                }),
            }
        }
    }
    out
}

/// A six-sided box from its corners, corner `i` having bit 0 for the far
/// end along the first axis, bit 1 along the second and bit 2 along the
/// third.
fn cuboid(m: &mut TriMesh, c: [DVec3; 8]) {
    let centre = c.iter().copied().sum::<DVec3>() / 8.0;
    for face in [
        [0, 1, 3, 2],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 3, 7, 5],
    ] {
        let q = face.map(|i| c[i]);
        let mid = q.iter().copied().sum::<DVec3>() / 4.0;
        let normal = (q[1] - q[0]).cross(q[3] - q[0]);
        let normal = if normal.dot(mid - centre) < 0.0 {
            -normal
        } else {
            normal
        };
        m.add_quad(q, normal);
    }
}

/// The lines of every sketch, in model coordinates: (sketch, from, to).
pub fn sketch_lines(model: &Model) -> Vec<(ElementId, DVec3, DVec3)> {
    let mut out = Vec::new();
    for e in model.iter() {
        if let ElementKind::Sketch(s) = &e.kind {
            let z = s
                .level
                .and_then(|l| model.level(l))
                .map_or(0.0, |l| l.elevation);
            for (a, b) in s.segments() {
                out.push((e.id, a + DVec3::Z * z, b + DVec3::Z * z));
            }
        }
    }
    out
}

/// Build viewport geometry for every element with a body.
pub fn build_scene(model: &Model) -> Vec<SceneItem> {
    build_scene_of(model, None)
}

/// [`build_scene`] for `only` these elements (with what cuts them, which
/// the caller includes): a plan slicing a few elements needn't mesh the
/// whole model (30-40 ms for the starter house, every drag frame).
pub fn build_scene_of(
    model: &Model,
    only: Option<&std::collections::HashSet<ElementId>>,
) -> Vec<SceneItem> {
    let mut out = build_scene_unjoined(model, only);
    apply_joins(model, &mut out);
    out
}

/// The scene for exchange (IFC, IFCX): each element's own body, its
/// explicit cuts applied but not the wall and slab joins, so a slab
/// stays the plain extrusion other tools read and re-import as a slab.
pub fn build_scene_for_exchange(model: &Model) -> Vec<SceneItem> {
    build_scene_unjoined(model, None)
}

fn build_scene_unjoined(
    model: &Model,
    only: Option<&std::collections::HashSet<ElementId>>,
) -> Vec<SceneItem> {
    let mut out = Vec::new();
    let joins = creabim_core::wall_joins(model);
    for e in model.iter() {
        if only.is_some_and(|o| !o.contains(&e.id)) {
            continue;
        }
        let plain = match &e.kind {
            ElementKind::Wall(_) => [0.86, 0.84, 0.80],
            ElementKind::Slab(_) => [0.70, 0.70, 0.72],
            _ => [0.66, 0.34, 0.26],
        };
        if let ElementKind::Wall(
            w @ creabim_core::Wall {
                curtain: Some(_), ..
            },
        ) = &e.kind
        {
            out.extend(curtain_items(model, e, w));
            continue;
        }
        if let Some(layers) = layer_items(model, e, &joins, plain) {
            out.extend(layers);
            continue;
        }
        let (mesh, color) = match &e.kind {
            ElementKind::Wall(w) => {
                let Some(level) = model.level(w.level) else {
                    continue;
                };
                // its doors and windows, and the curtain walls set in it
                let owned = creabim_core::curtain::wall_openings(model, e.id);
                let openings: Vec<&Opening> = owned.iter().collect();
                let ends = joins.get(&e.id).cloned().unwrap_or_default();
                let top = creabim_core::walls::top_profile(model, w);
                (
                    wall_mesh(w, level.elevation, &openings, &ends, top.as_ref()),
                    [0.86, 0.84, 0.80],
                )
            }
            ElementKind::Slab(s) => {
                let Some(level) = model.level(s.level) else {
                    continue;
                };
                let holes = creabim_core::slab::holes_of(model, e);
                (
                    slab_mesh_with_holes(&s.outline(), &holes, s.top(level.elevation), s.thickness),
                    [0.70, 0.70, 0.72],
                )
            }
            ElementKind::Shaft(s) => {
                // a see-through stand-in, to see and pick the shaft by
                let (z0, z1) = s.span(model);
                out.push(SceneItem {
                    id: e.id,
                    mesh: slab_mesh(&s.boundary, z1, z1 - z0),
                    color: [0.85, 0.45, 0.25],
                    surface: PLAIN_SURFACE,
                    ghost: true,
                    layer: None,
                    finish: None,
                });
                continue;
            }
            ElementKind::Ceiling(c) => {
                let Some(outline) = c.outline(model) else {
                    continue;
                };
                let (_, top) = c.span(model);
                (slab_mesh(&outline, top, c.thickness), [0.95, 0.95, 0.93])
            }
            ElementKind::Opening(o) => {
                let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
                    continue;
                };
                let Some(level) = model.level(w.level) else {
                    continue;
                };
                out.extend(opening_items(model, e, o, w, level.elevation));
                // a bare hole has nothing in it: a see-through box shows
                // it in 3D and lets it be clicked (MU18)
                if o.kind == creabim_core::OpeningKind::Hole {
                    if let Some(mesh) = crate::opening::cut_box_mesh(o, w, level.elevation) {
                        out.push(SceneItem {
                            id: e.id,
                            mesh,
                            color: [0.85, 0.45, 0.25],
                            surface: PLAIN_SURFACE,
                            ghost: true,
                            layer: None,
                            finish: None,
                        });
                    }
                }
                continue;
            }
            ElementKind::Part(_) | ElementKind::Site(_) => {
                out.extend(crate::parts::scene_items(model, e));
                continue;
            }
            ElementKind::Object(_) => {
                // one item per part, each in its own material
                for (mesh, material) in creabim_core::objects::model_parts(model, e) {
                    let look = material.and_then(|m| match &model.get(m)?.kind {
                        ElementKind::Material(m) => Some((m.color, m.surface())),
                        _ => None,
                    });
                    let (color, surface) = look.unwrap_or(([0.8, 0.78, 0.74], PLAIN_SURFACE));
                    out.push(SceneItem {
                        id: e.id,
                        mesh: edit_mesh_to_tri(&mesh, DVec3::ZERO),
                        color,
                        surface,
                        ghost: false,
                        layer: None,
                        finish: None,
                    });
                }
                continue;
            }
            ElementKind::FreeMesh(f) => {
                let z = f
                    .level
                    .and_then(|l| model.level(l))
                    .map_or(0.0, |l| l.elevation);
                (
                    edit_mesh_to_tri(&f.evaluated().mesh, f.origin + DVec3::Z * z),
                    [0.62, 0.75, 0.90],
                )
            }
            ElementKind::Level(_)
            | ElementKind::Grid(_)
            | ElementKind::View(_)
            | ElementKind::Sheet(_)
            | ElementKind::Viewport(_)
            | ElementKind::Material(_)
            | ElementKind::Assembly(_)
            | ElementKind::Dimension(_)
            | ElementKind::Tag(_)
            | ElementKind::Note(_)
            | ElementKind::TextStyle(_)
            | ElementKind::SheetTemplate(_)
            | ElementKind::Units(_)
            | ElementKind::Project(_)
            | ElementKind::Reference(_)
            | ElementKind::PaperMark(_)
            | ElementKind::MarkRules(_)
            | ElementKind::StylePack(_)
            | ElementKind::ElementType(_)
            | ElementKind::Sketch(_)
            | ElementKind::Cut(_)
            | ElementKind::Guide(_)
            | ElementKind::CropBox(_)
            | ElementKind::GridGroup(_)
            | ElementKind::Room(_)
            | ElementKind::RoomSeparator(_)
            | ElementKind::Join(_)
            | ElementKind::ScriptObject(_)
            | ElementKind::Detail(_) => continue,
            ElementKind::Column(_)
            | ElementKind::Beam(_)
            | ElementKind::Roof(_)
            | ElementKind::Stair(_)
            | ElementKind::Railing(_)
            | ElementKind::Footing(_) => match crate::members::member_mesh(model, e) {
                Some(m) => m,
                None => continue,
            },
        };
        // a material's look wins over the element type's default
        let (color, surface) = creabim_core::material_of(model, e)
            .map_or((color, PLAIN_SURFACE), |(_, m)| (m.color, m.surface()));
        out.push(SceneItem {
            id: e.id,
            mesh,
            color,
            surface,
            ghost: false,
            layer: None,
            finish: None,
        });
    }
    apply_cuts(model, &mut out);
    out
}

/// The scene as drawings show it: cutters left out.
pub fn drawn_scene(model: &Model) -> Vec<SceneItem> {
    shared_scene(model)
        .iter()
        .filter(|i| !i.ghost && !i.is_lining())
        .cloned()
        .collect()
}

/// How many scenes [`shared_scene`] keeps: the model as it is, and a few
/// others (a view's filtered copy, the state before an edit).
const SHARED_SCENES: usize = 4;

static SHARED: std::sync::Mutex<Vec<(u64, std::sync::Arc<Vec<SceneItem>>)>> =
    std::sync::Mutex::new(Vec::new());

/// The shared scene of `model`'s revision if it is built already, without
/// waiting: `None` while it isn't, or while another thread holds the
/// cache (building one).
pub fn shared_scene_ready(model: &Model) -> Option<std::sync::Arc<Vec<SceneItem>>> {
    let rev = model.revision();
    let cache = SHARED.try_lock().ok()?;
    cache
        .iter()
        .find(|(r, _)| *r == rev && rev != 0)
        .map(|(_, s)| s.clone())
}

/// [`build_scene`] built once per model revision and shared: the 3D view,
/// every section, elevation and 3D drawing of the same revision use the
/// one scene instead of each building its own. Revisions are unique
/// across models (see [`Model::revision`]); a model at revision 0 is
/// built afresh every time.
pub fn shared_scene(model: &Model) -> std::sync::Arc<Vec<SceneItem>> {
    let rev = model.revision();
    if rev == 0 {
        return std::sync::Arc::new(build_scene(model));
    }
    // held while building, so two threads after the same revision build it once
    let mut cache = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(i) = cache.iter().position(|(r, _)| *r == rev) {
        let hit = cache.remove(i);
        let scene = hit.1.clone();
        cache.push(hit);
        return scene;
    }
    let scene = std::sync::Arc::new(build_scene(model));
    if cache.len() >= SHARED_SCENES {
        cache.remove(0);
    }
    cache.push((rev, scene.clone()));
    scene
}

/// One item per element, a layered element's layers in one mesh with its
/// core's look: for exports that take one shape per element.
pub fn merged(items: Vec<SceneItem>) -> Vec<SceneItem> {
    let mut out: Vec<SceneItem> = Vec::with_capacity(items.len());
    let mut at: std::collections::HashMap<ElementId, usize> = std::collections::HashMap::new();
    for item in items.into_iter().filter(|i| !i.is_lining()) {
        match at.get(&item.id) {
            Some(&i) => {
                let o = &mut out[i];
                o.mesh.append(&item.mesh);
                // the first core layer gives the look
                if item.layer.is_some_and(|l| l.core) && !o.layer.is_some_and(|l| l.core) {
                    (o.color, o.surface, o.layer) = (item.color, item.surface, item.layer);
                }
            }
            None => {
                at.insert(item.id, out.len());
                out.push(item);
            }
        }
    }
    for o in &mut out {
        o.layer = None;
    }
    out
}

/// [`build_scene`] with one item per element.
pub fn element_scene(model: &Model) -> Vec<SceneItem> {
    merged(build_scene(model))
}

/// [`drawn_scene`] with one item per element.
pub fn drawn_elements(model: &Model) -> Vec<SceneItem> {
    merged(drawn_scene(model))
}

/// Every cut in the model, in order: (target, cutter, operation).
pub fn cuts(model: &Model) -> Vec<(ElementId, ElementId, BoolOp)> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Cut(c) => Some((c.target, c.cutter, c.op)),
            _ => None,
        })
        .collect()
}

/// [`cuts`], and what each void cuts: everything that is cut in the model.
pub fn all_cuts(model: &Model) -> Vec<(ElementId, ElementId, BoolOp)> {
    let mut out = cuts(model);
    out.extend(
        creabim_core::parts::void_cuts(model)
            .into_iter()
            .map(|(t, v)| (t, v, BoolOp::Difference)),
    );
    out
}

/// Is `id` used to cut something? Cutters are hidden from drawings.
pub fn is_cutter(model: &Model, id: ElementId) -> bool {
    model
        .iter()
        .any(|e| matches!(&e.kind, ElementKind::Cut(c) if c.cutter == id))
}

/// Apply every cut to the meshes of its target, using the cutters' own
/// shapes (a cutter that is itself cut cuts with its uncut shape, so cuts
/// can never loop), and turn the cutters into ghosts.
fn apply_cuts(model: &Model, items: &mut [SceneItem]) {
    let cuts = all_cuts(model);
    if cuts.is_empty() {
        return;
    }
    // a layered cutter cuts with all its layers
    let mut original: std::collections::HashMap<ElementId, TriMesh> =
        std::collections::HashMap::new();
    for (_, c, _) in &cuts {
        if original.contains_key(c) {
            continue;
        }
        let mut tool = TriMesh::default();
        for i in items.iter().filter(|i| i.id == *c) {
            tool.append(&i.mesh);
        }
        if !tool.is_empty() {
            original.insert(*c, tool);
        }
    }
    for (target, cutter, op) in &cuts {
        let Some(tool) = original.get(cutter) else {
            continue;
        };
        for item in items.iter_mut().filter(|i| i.id == *target) {
            item.mesh = boolean_mesh(&item.mesh, tool, *op);
        }
    }
    for item in items.iter_mut() {
        if original.contains_key(&item.id) {
            item.ghost = true;
        }
    }
}

/// Where a wall and a slab overlap in 3D, the one the drawings let run
/// through cuts the other (A4-6), so 3D, sections and plans agree: by the
/// junction's own rule if it has one, else layer by layer (the lower join
/// priority runs through: cores through finishes), then walls through
/// slabs. As strong, they are left as they are. Each result is kept, keyed
/// by the two shapes, so editing one wall redoes only its own joins.
fn apply_joins(model: &Model, items: &mut [SceneItem]) {
    let rules = creabim_core::joins::rules(model);
    let is = |id: ElementId, wall: bool| match model.get(id).map(|e| &e.kind) {
        Some(ElementKind::Wall(w)) => wall && w.curtain.is_none(),
        Some(ElementKind::Slab(_)) => !wall,
        _ => false,
    };
    let strength = |i: &SceneItem| {
        (
            std::cmp::Reverse(i.layer.map_or(1, |l| l.priority)),
            model
                .get(i.id)
                .map_or(0, |e| creabim_core::joins::category_rank(&e.kind)),
        )
    };
    let boxes: Vec<Option<(glam::Vec3, glam::Vec3)>> = items
        .iter()
        .map(|i| if i.ghost { None } else { i.mesh.bounds() })
        .collect();
    let overlap = |a: (glam::Vec3, glam::Vec3), b: (glam::Vec3, glam::Vec3)| {
        let lo = a.0.max(b.0);
        let hi = a.1.min(b.1);
        (hi - lo).min_element() > 1e-4
    };
    let walls: Vec<usize> = (0..items.len())
        .filter(|k| boxes[*k].is_some() && is(items[*k].id, true))
        .collect();
    let slabs: Vec<usize> = (0..items.len())
        .filter(|k| boxes[*k].is_some() && is(items[*k].id, false))
        .collect();
    // (target, cutter) pairs, in item indices
    let mut pairs: Vec<(usize, usize)> = vec![];
    for &w in &walls {
        for &sl in &slabs {
            let (Some(bw), Some(bs)) = (boxes[w], boxes[sl]) else {
                continue;
            };
            if !overlap(bw, bs) {
                continue;
            }
            let (a, b) = (&items[w], &items[sl]);
            let rule = rules.iter().find(|r| r.joins(a.id, b.id));
            if rule.is_some_and(|r| !r.cleanup) {
                continue;
            }
            let wall_wins = match rule.and_then(|r| r.wins(a.id, b.id)) {
                Some(w) => w,
                None => {
                    let (sa, sb) = (strength(a), strength(b));
                    if sa == sb {
                        continue;
                    }
                    sa > sb
                }
            };
            pairs.push(if wall_wins { (sl, w) } else { (w, sl) });
        }
    }
    if pairs.is_empty() {
        return;
    }
    // the cutters' shapes before any join cuts them
    let originals: Vec<TriMesh> = items.iter().map(|i| i.mesh.clone()).collect();
    for (t, c) in pairs {
        let key = (mesh_key(&items[t].mesh), mesh_key(&originals[c]));
        let done = JOIN_CACHE.with(|m| m.borrow().get(&key).cloned());
        let mesh = match done {
            Some(m) => m,
            None => {
                let m = boolean_mesh(&items[t].mesh, &originals[c], BoolOp::Difference);
                JOIN_CACHE.with(|cache| {
                    let mut cache = cache.borrow_mut();
                    if cache.len() > 4000 {
                        cache.clear();
                    }
                    cache.insert(key, m.clone());
                });
                m
            }
        };
        items[t].mesh = mesh;
    }
}

thread_local! {
    static JOIN_CACHE: std::cell::RefCell<std::collections::HashMap<(u64, u64), TriMesh>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// A fingerprint of a mesh's shape.
fn mesh_key(m: &TriMesh) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    m.indices.hash(&mut h);
    for p in &m.positions {
        for v in p {
            v.to_bits().hash(&mut h);
        }
    }
    h.finish()
}

/// `a op b` for two closed triangle meshes.
pub fn boolean_mesh(a: &TriMesh, b: &TriMesh, op: BoolOp) -> TriMesh {
    let polys = |m: &TriMesh| -> Vec<csg::Polygon> {
        m.indices
            .chunks_exact(3)
            .filter_map(|t| {
                let p = |i: u32| {
                    let v = m.positions[i as usize];
                    DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
                };
                csg::Polygon::new(vec![p(t[0]), p(t[1]), p(t[2])])
            })
            .collect()
    };
    let result = csg::boolean(polys(a), polys(b), op);
    let mut out = TriMesh::default();
    for p in &result {
        let n = (p.points[1] - p.points[0])
            .cross(p.points[2] - p.points[0])
            .normalize_or_zero();
        let fan: Vec<[usize; 3]> = (1..p.points.len() - 1).map(|i| [0, i, i + 1]).collect();
        // the fan's winding already follows the polygon's
        out.add_polygon(&p.points, &fan, newell(&p.points).normalize_or(n));
    }
    out
}

fn newell(p: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        n += DVec3::new(
            (a.y - b.y) * (a.z + b.z),
            (a.z - b.z) * (a.x + b.x),
            (a.x - b.x) * (a.y + b.y),
        );
    }
    n
}

/// A door's or window's parts, each in its own colour: the frame and leaf
/// in the opening's material (or a painted default), glass see-through.
/// An element can so have several items, all with its id.
fn opening_items(
    model: &Model,
    e: &creabim_core::Element,
    o: &Opening,
    w: &creabim_core::Wall,
    level_z: f64,
) -> Vec<SceneItem> {
    use crate::opening::Part;
    use creabim_core::OpeningKind;
    let material = creabim_core::material_of(model, e).map(|(_, m)| (m.color, m.surface()));
    crate::opening::opening_meshes(o, w, level_z)
        .into_iter()
        .map(|(part, mesh)| {
            let (color, surface) = match (part, o.kind, material) {
                (Part::Glass, ..) => (GLASS_COLOR, GLASS_SURFACE),
                (Part::Handle, ..) => ([0.72, 0.73, 0.75], [0.3, 1.0, 1.0]),
                (_, _, Some(m)) => m,
                (Part::Frame, OpeningKind::Window | OpeningKind::Hole, None) => {
                    ([0.93, 0.93, 0.91], PLAIN_SURFACE)
                }
                (Part::Frame, OpeningKind::Door, None) => ([0.50, 0.35, 0.22], PLAIN_SURFACE),
                (Part::Leaf, _, None) => ([0.62, 0.45, 0.30], PLAIN_SURFACE),
            };
            SceneItem {
                id: e.id,
                mesh,
                color,
                surface,
                ghost: false,
                layer: None,
                finish: match part {
                    Part::Glass => Some("Glass"),
                    Part::Handle => Some("Metal"),
                    Part::Frame | Part::Leaf => None,
                },
            }
        })
        .collect()
}

/// Glass in doors and windows: pale blue, smooth and mostly see-through.
pub const GLASS_COLOR: [f32; 3] = [0.62, 0.78, 0.88];
pub const GLASS_SURFACE: [f32; 3] = [0.05, 0.0, 0.3];

/// Each element's whole mesh: the items of one element (a window's frame
/// and glass) merged.
pub fn element_meshes(scene: &[SceneItem]) -> std::collections::HashMap<ElementId, TriMesh> {
    let mut out: std::collections::HashMap<ElementId, TriMesh> = Default::default();
    for i in scene.iter().filter(|i| !i.is_lining()) {
        out.entry(i.id).or_default().append(&i.mesh);
    }
    out
}

/// A curtain wall's mullions and panels, each kind in its own look: the
/// mullions and solid panels in the wall's material (or anodised
/// aluminium and a pale grey), glass see-through.
fn curtain_items(model: &Model, e: &Element, w: &creabim_core::Wall) -> Vec<SceneItem> {
    use crate::curtain::CurtainPart;
    let (Some(level), Some(grid)) = (model.level(w.level), &w.curtain) else {
        return vec![];
    };
    let openings: Vec<&Opening> = model
        .dependents(e.id)
        .into_iter()
        .filter_map(|d| match &model.get(d)?.kind {
            ElementKind::Opening(o) => Some(o),
            _ => None,
        })
        .collect();
    let material = creabim_core::material_of(model, e).map(|(_, m)| (m.color, m.surface()));
    crate::curtain::curtain_meshes(w, grid, &openings, level.elevation)
        .into_iter()
        .map(|(part, mesh)| {
            let (color, surface) = match (part, material) {
                (CurtainPart::Glass, _) => (GLASS_COLOR, GLASS_SURFACE),
                (_, Some(m)) => m,
                (CurtainPart::Mullion, None) => ([0.62, 0.64, 0.67], [0.35, 0.9, 1.0]),
                (CurtainPart::Panel, None) => ([0.80, 0.80, 0.78], PLAIN_SURFACE),
            };
            SceneItem {
                id: e.id,
                mesh,
                color,
                surface,
                ghost: false,
                layer: None,
                finish: match part {
                    CurtainPart::Glass => Some("Glass"),
                    CurtainPart::Mullion => Some("Metal"),
                    CurtainPart::Panel => None,
                },
            }
        })
        .collect()
}

/// An extruded slab with holes through it: top face at `top_z`,
/// `thickness` downwards, each hole (a polygon inside the outline) cut
/// straight through.
pub fn slab_mesh_with_holes(
    boundary: &[DVec2],
    holes: &[Vec<DVec2>],
    top_z: f64,
    thickness: f64,
) -> TriMesh {
    if holes.is_empty() {
        return slab_mesh(boundary, top_z, thickness);
    }
    let face = creabim_core::polygon::bridged(boundary, holes);
    let mut m = TriMesh::default();
    let tris = triangulate(&face);
    let top: Vec<DVec3> = face.iter().map(|p| p.extend(top_z)).collect();
    let bottom: Vec<DVec3> = face.iter().map(|p| p.extend(top_z - thickness)).collect();
    m.add_polygon(&top, &tris, DVec3::Z);
    m.add_polygon(&bottom, &tris, -DVec3::Z);
    // the outline counter-clockwise and the holes clockwise: material is
    // on the left of every edge, so the sides face right
    let mut loops = vec![boundary.to_vec()];
    loops.extend(holes.iter().cloned());
    for (k, lp) in loops.iter_mut().enumerate() {
        let ccw = crate::triangulate::signed_area2(lp) > 0.0;
        if ccw != (k == 0) {
            lp.reverse();
        }
        let n = lp.len();
        for i in 0..n {
            let (a, b) = (lp[i], lp[(i + 1) % n]);
            let e = (b - a).normalize_or_zero();
            m.add_quad(
                [
                    a.extend(top_z - thickness),
                    b.extend(top_z - thickness),
                    b.extend(top_z),
                    a.extend(top_z),
                ],
                DVec3::new(e.y, -e.x, 0.0),
            );
        }
    }
    m
}

/// Extruded slab: top face at `top_z`, `thickness` downwards.
pub fn slab_mesh(boundary: &[DVec2], top_z: f64, thickness: f64) -> TriMesh {
    let mut m = TriMesh::default();
    let tris = triangulate(boundary);
    let top: Vec<DVec3> = boundary.iter().map(|p| p.extend(top_z)).collect();
    let bottom: Vec<DVec3> = boundary
        .iter()
        .map(|p| p.extend(top_z - thickness))
        .collect();
    m.add_polygon(&top, &tris, DVec3::Z);
    m.add_polygon(&bottom, &tris, -DVec3::Z);
    let ccw = crate::triangulate::signed_area2(boundary) > 0.0;
    for i in 0..boundary.len() {
        let (a, b) = (boundary[i], boundary[(i + 1) % boundary.len()]);
        let e = (b - a).normalize_or_zero();
        let out = if ccw {
            DVec2::new(e.y, -e.x)
        } else {
            DVec2::new(-e.y, e.x)
        };
        m.add_quad(
            [
                bottom[i],
                bottom[(i + 1) % boundary.len()],
                top[(i + 1) % boundary.len()],
                top[i],
            ],
            out.extend(0.0),
        );
    }
    m
}

/// Flat-shaded triangles from a polygon mesh, offset to `origin`, face by
/// face (so triangle k still maps back to its face).
///
/// Coincident corners are welded: corners closer than a micron become one
/// position, and faces lying in one plane share the vertex itself. A
/// corner on a crease is still one vertex per face direction, so every
/// face keeps its own flat normal, but they all sit at exactly the same
/// point, so the mesh is closed wherever the edit mesh is (see
/// [`TriMesh::is_closed`]).
pub fn edit_mesh_to_tri(mesh: &creabim_core::EditMesh, origin: DVec3) -> TriMesh {
    use std::collections::HashMap;
    const TOL: f64 = 1e-6;
    let key = |p: DVec3| {
        (
            (p.x / TOL).round() as i64,
            (p.y / TOL).round() as i64,
            (p.z / TOL).round() as i64,
        )
    };
    let mut first: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let corner: Vec<usize> = (0..mesh.vertices.len())
        .map(|v| *first.entry(key(mesh.vertices[v])).or_insert(v))
        .collect();
    let mut m = TriMesh::default();
    let mut shared: HashMap<(usize, [i64; 3]), u32> = HashMap::new();
    for f in 0..mesh.faces.len() {
        let face = &mesh.faces[f];
        let n = mesh.face_normal(f);
        let nk = (n * 1e5).round().as_i64vec3().to_array();
        let mut index = |k: usize| -> (u32, DVec3) {
            let c = corner[face[k] as usize];
            let p = mesh.vertices[c] + origin;
            let i = *shared.entry((c, nk)).or_insert_with(|| {
                m.positions.push(p.as_vec3().to_array());
                m.normals.push(n.as_vec3().to_array());
                (m.positions.len() - 1) as u32
            });
            (i, p)
        };
        let tris: Vec<[(u32, DVec3); 3]> = mesh
            .face_corners(f)
            .into_iter()
            .map(|t| t.map(&mut index))
            .collect();
        for [(a, pa), (b, pb), (c, pc)] in tris {
            // keep the winding consistent with the normal
            if (pb - pa).cross(pc - pa).dot(n) >= 0.0 {
                m.indices.extend([a, b, c]);
            } else {
                m.indices.extend([a, c, b]);
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partition_held_to_the_ceilings_has_no_sliver_at_its_ends() {
        // MU20: FF Spine runs a few centimetres past the ceiling's edge into
        // the wall it joins; its top used to rise there to its own height
        let m = creabim_core::starter::starter_house();
        let id = m.iter().find(|e| e.name == "FF Spine").unwrap().id;
        let top = build_scene_of(&m, None)
            .iter()
            .filter(|i| i.id == id)
            .filter_map(|i| i.mesh.bounds())
            .map(|b| b.1.z)
            .fold(f32::MIN, f32::max);
        assert!((top - 5.5).abs() < 1e-3, "its top reaches {top}");
    }

    #[test]
    fn a_slab_edge_in_a_wall_is_cut_by_it_in_3d_as_in_the_drawings() {
        // A4-6: a floor slab whose edge runs into the wall below it, with
        // no junction rule: the wall runs through, so nothing of the slab
        // is left inside the wall in 3D, as in plans and sections
        use creabim_core::{Element, Level, Slab, Transaction, Wall};
        let l0 = Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let l1 = Element::new(
            "L1",
            ElementKind::Level(Level {
                elevation: 3.0,
                height: 3.0,
            }),
        );
        let wall = Element::new(
            "Wall",
            ElementKind::Wall(Wall::new(
                l0.id,
                DVec2::ZERO,
                DVec2::new(6.0, 0.0),
                0.3,
                3.0,
            )),
        );
        let slab = Element::new(
            "Slab",
            ElementKind::Slab(Slab::new(
                l1.id,
                vec![
                    DVec2::new(0.0, 0.0),
                    DVec2::new(6.0, 0.0),
                    DVec2::new(6.0, 4.0),
                    DVec2::new(0.0, 4.0),
                ],
                0.25,
            )),
        );
        let (wid, sid) = (wall.id, slab.id);
        let mut doc = creabim_core::document::Document::new();
        doc.commit(
            Transaction::new("setup")
                .insert(l0)
                .insert(l1)
                .insert(wall)
                .insert(slab),
        )
        .unwrap();
        let items = build_scene_of(doc.model(), None);
        let (lo, hi) = items
            .iter()
            .filter(|i| i.id == wid)
            .filter_map(|i| i.mesh.bounds())
            .next()
            .unwrap();
        let slab_mesh = &items.iter().find(|i| i.id == sid).unwrap().mesh;
        let (slo, _) = slab_mesh.bounds().unwrap();
        // its edge now stops at the wall's face, not its middle
        assert!(
            (slo.y - hi.y).abs() < 1e-3,
            "slab starts at y {} (wall face {})",
            slo.y,
            hi.y
        );
        for t in slab_mesh.indices.chunks_exact(3) {
            let c = t
                .iter()
                .map(|k| glam::Vec3::from(slab_mesh.positions[*k as usize]))
                .sum::<glam::Vec3>()
                / 3.0;
            let inside = (c - lo).min_element() > 0.002 && (hi - c).min_element() > 0.002;
            assert!(
                !inside,
                "a slab face at {c} is inside the wall ({lo}..{hi})"
            );
        }
    }

    #[test]
    fn slab_volume_matches_area_times_thickness() {
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let m = slab_mesh(&l, 0.0, 0.25);
        assert!((m.volume() - 12.0 * 0.25).abs() < 1e-5);
        let mut cw = l.to_vec();
        cw.reverse();
        assert!((slab_mesh(&cw, 0.0, 0.25).volume() - 3.0).abs() < 1e-5);
    }

    #[test]
    fn a_slab_top_offset_lifts_its_mesh_and_arcs_round_it() {
        let mut model = creabim_core::sample::sample_house();
        let (id, elevation, t) = model
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => Some((e.id, model.level(s.level)?.elevation, s.thickness)),
                _ => None,
            })
            .unwrap();
        let mut e = model.get(id).unwrap().clone();
        let ElementKind::Slab(s) = &mut e.kind else {
            panic!()
        };
        // a 4 x 2 m slab whose far side is a half circle, its top 0.6 m up,
        // far from any wall (walls cut what of a slab runs into them, A4-6)
        let o = DVec2::splat(100.0);
        s.boundary = vec![
            o + DVec2::new(0.0, 0.0),
            o + DVec2::new(4.0, 0.0),
            o + DVec2::new(4.0, 2.0),
            o + DVec2::new(0.0, 2.0),
        ];
        s.bulges = vec![0.0, 1.0, 0.0, 0.0];
        s.top_offset = 0.6;
        creabim_core::Transaction::new("edit")
            .replace(model.get(id).unwrap().clone(), e)
            .apply(&mut model)
            .unwrap();
        let scene = build_scene(&model);
        let item = scene.iter().find(|i| i.id == id).unwrap();
        let (lo, hi) = item.mesh.bounds().unwrap();
        assert!((hi.z as f64 - (elevation + 0.6)).abs() < 1e-5);
        assert!((lo.z as f64 - (elevation + 0.6 - t)).abs() < 1e-5);
        // the arc bulges a metre past the straight side
        assert!((hi.x - 105.0).abs() < 1e-4);
        let area = 8.0 + std::f64::consts::PI / 2.0;
        assert!((item.mesh.volume() - area * t).abs() < 0.01);
    }

    #[test]
    fn a_slab_with_a_hole_loses_its_volume() {
        let outer = creabim_core::polygon::rectangle(DVec2::ZERO, DVec2::new(4.0, 4.0));
        let hole = creabim_core::polygon::rectangle(DVec2::new(1.0, 1.0), DVec2::new(2.0, 3.0));
        let m = slab_mesh_with_holes(&outer, std::slice::from_ref(&hole), 0.0, 0.25);
        assert!((m.volume() - 14.0 * 0.25).abs() < 1e-5, "{}", m.volume());
        // the plan cut at mid-depth has the hole in it
        let cut = crate::slice::slice(&m, -0.1);
        assert_eq!(cut.len(), 1);
        let area = crate::triangulate::signed_area2(&cut[0]) / 2.0;
        assert!((area - 14.0).abs() < 1e-5, "{area}");
    }

    #[test]
    fn edit_mesh_corners_are_welded_and_faces_stay_flat() {
        // a box whose faces each carry their own copies of the corners, as
        // an imported mesh does
        let cube = creabim_core::EditMesh::cuboid(DVec3::new(2.0, 2.0, 2.0));
        let mut split = creabim_core::EditMesh::default();
        for f in &cube.faces {
            let base = split.vertices.len() as u32;
            split
                .vertices
                .extend(f.iter().map(|&v| cube.vertices[v as usize]));
            split
                .faces
                .push((0..f.len() as u32).map(|i| base + i).collect());
        }
        let m = edit_mesh_to_tri(&split, DVec3::ZERO);
        assert!(m.is_closed());
        // eight corners, three face directions each
        assert_eq!(m.positions.len(), 24);
        let mut corners: Vec<[u32; 3]> = m.positions.iter().map(|p| p.map(f32::to_bits)).collect();
        corners.sort_unstable();
        corners.dedup();
        assert_eq!(corners.len(), 8);
        for t in m.indices.chunks(3) {
            let n = |i: u32| m.normals[i as usize];
            assert!(n(t[0]) == n(t[1]) && n(t[1]) == n(t[2]), "flat faces");
        }
        // a face split in two shares the corners along the split
        let mut halves = cube.clone();
        let top = halves
            .faces
            .iter()
            .position(|f| f.iter().all(|&v| cube.vertices[v as usize].z > 1.0))
            .unwrap();
        let q = halves.faces.remove(top);
        halves.faces.push(vec![q[0], q[1], q[2]]);
        halves.faces.push(vec![q[0], q[2], q[3]]);
        let m = edit_mesh_to_tri(&halves, DVec3::ZERO);
        assert_eq!(m.positions.len(), 24);
        assert!(m.is_closed());
        // and the plan cut of the closed box is one loop
        let cut = crate::slice::slice(&m, 1.0);
        assert_eq!(cut.len(), 1);
    }

    #[test]
    fn edit_mesh_volume_is_preserved() {
        let mut e = creabim_core::EditMesh::cuboid(DVec3::new(2.0, 2.0, 2.0));
        e.extrude_face(1, 1.0);
        let m = edit_mesh_to_tri(&e, DVec3::new(10.0, 0.0, 0.0));
        assert!((m.volume() - 12.0).abs() < 1e-4);
    }

    #[test]
    fn window_reveals_show_the_finish_not_the_insulation() {
        let m = creabim_core::starter::starter_house();
        let scene = build_scene(&m);
        let linings: Vec<&SceneItem> = scene.iter().filter(|i| i.is_lining()).collect();
        assert!(!linings.is_empty());
        let insulation = layer_color(creabim_core::LayerFunction::Insulation, [0.0; 3]);
        let insulation_materials: Vec<[f32; 3]> = scene
            .iter()
            .filter(|i| {
                i.layer
                    .is_some_and(|l| l.function == creabim_core::LayerFunction::Insulation)
            })
            .map(|i| i.color)
            .collect();
        for l in &linings {
            assert_ne!(l.color, insulation);
            assert!(!insulation_materials.contains(&l.color), "{:?}", l.color);
            // thin, closed boxes
            assert!(l.mesh.volume() > 0.0);
        }
        // drawings and exports leave them out
        assert!(!drawn_scene(&m).iter().any(|i| i.is_lining()));
        let walls: f64 = element_meshes(&scene).values().map(|m| m.volume()).sum();
        let without: Vec<SceneItem> = scene.iter().filter(|i| !i.is_lining()).cloned().collect();
        let plain: f64 = element_meshes(&without).values().map(|m| m.volume()).sum();
        assert!((walls - plain).abs() < 1e-9);
    }
}
