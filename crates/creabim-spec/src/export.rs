//! A model back into the format, for the elements the format covers. Ids
//! are made from element names (made unique where names repeat), so the
//! file reads like one written by hand. What the format can't describe
//! yet is left out and listed, so nothing goes missing silently.

use crate::format as f;
use crate::json::Colour;
use creabim_core::{
    self as core, parts, site, types::Param, Element, ElementId, ElementKind, Model,
};
use std::collections::{BTreeMap, HashMap, HashSet};

/// A model description, and what of the model it leaves out.
pub struct Exported {
    pub spec: f::Model,
    /// One line for each kind of thing left out: "3 Tag (tags are not in
    /// the format yet)".
    pub skipped: Vec<String>,
}

/// Why elements of a kind are not in the format.
fn not_in_format(kind: &str) -> &'static str {
    match kind {
        "Tag" | "Dimension" | "Note" => "annotation is not in the format yet",
        "TextStyle" | "SheetTemplate" | "MarkRules" => "styles are not in the format yet",
        "Sketch" | "Guide" => "drafting is not in the format yet",
        "FreeMesh" => "free meshes are not in the format yet",
        "Footing" | "Shaft" | "Ceiling" | "RoomSeparator" => "not in the format yet",
        "Join" => "join rules are not in the format yet",
        "Detail" => "detail components are not in the format yet",
        "CropBox" | "Cut" => "not in the format yet",
        _ => "not in the format yet",
    }
}

struct Ids {
    of: HashMap<ElementId, String>,
    used: HashSet<String>,
}

impl Ids {
    fn give(&mut self, e: ElementId, base: &str) {
        let base = base.trim();
        let base = if base.is_empty() { "element" } else { base };
        let mut id = base.to_string();
        let mut n = 2;
        while self.used.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        self.used.insert(id.clone());
        self.of.insert(e, id);
    }
    fn get(&self, e: ElementId) -> Option<String> {
        self.of.get(&e).cloned()
    }
    fn of(&self, e: Option<ElementId>) -> Option<String> {
        e.and_then(|e| self.get(e))
    }
}

/// The name, when it says more than the id.
fn name(e: &Element, id: &str) -> Option<String> {
    (e.name != id).then(|| e.name.clone())
}

/// Is the element's own value of `p` written (it has no type, or sets
/// `p` for itself)?
fn own(e: &Element, p: Param) -> bool {
    e.of_type.as_ref().is_none_or(|t| t.overrides.contains(&p))
}

fn some_if(v: f64, keep: bool) -> Option<f64> {
    keep.then_some(v)
}

/// Can the format describe this element as it is?
fn covered(model: &Model, e: &Element) -> bool {
    match &e.kind {
        ElementKind::Wall(w) => w.curtain.is_none(),
        ElementKind::ElementType(t) if matches!(t.params, core::TypeParams::Object(_)) => {
            object_type_key(e).is_some()
        }
        ElementKind::ElementType(t) => {
            matches!(
                &t.params,
                core::TypeParams::Wall(w) if w.curtain.is_none()
            ) || matches!(
                t.params,
                core::TypeParams::Door(_)
                    | core::TypeParams::Window(_)
                    | core::TypeParams::Slab(_)
                    | core::TypeParams::Roof(_)
                    | core::TypeParams::Column(_)
                    | core::TypeParams::Beam(_)
            )
        }
        ElementKind::Object(_) => catalogue_key(model, e).is_some(),
        // a tag's room fields, style and leader end are left out
        ElementKind::Tag(_) => true,
        // a note's leader only when it points at a place, not an element
        ElementKind::Note(n) => n.leader.is_none_or(|l| l.host.is_none()),
        ElementKind::Material(_)
        | ElementKind::Assembly(_)
        | ElementKind::Level(_)
        | ElementKind::GridGroup(_)
        | ElementKind::Grid(_)
        | ElementKind::Opening(_)
        | ElementKind::Slab(_)
        | ElementKind::Roof(_)
        | ElementKind::Column(_)
        | ElementKind::Beam(_)
        | ElementKind::Stair(_)
        | ElementKind::Railing(_)
        | ElementKind::Part(_)
        | ElementKind::Site(_)
        | ElementKind::Room(_)
        | ElementKind::View(_)
        | ElementKind::Sheet(_)
        | ElementKind::Viewport(_)
        | ElementKind::Units(_)
        | ElementKind::Project(_) => true,
        _ => false,
    }
}

/// The starter catalogue key an object type was made from: such a type
/// is not written, its objects name the key and building brings it back.
fn object_type_key(e: &Element) -> Option<String> {
    let ElementKind::ElementType(core::ElementType {
        params: core::TypeParams::Object(t),
        ..
    }) = &e.kind
    else {
        return None;
    };
    let key = t.catalogue.clone()?;
    core::catalogue::starter(&key).map(|_| key)
}

/// The starter catalogue key an object was placed from.
fn catalogue_key(model: &Model, e: &Element) -> Option<String> {
    let t = core::objects::type_of(model, e)?;
    let key = t.catalogue.clone()?;
    core::catalogue::starter(&key).map(|_| key)
}

/// Describe `model` in the format.
pub fn export(model: &Model) -> Exported {
    let mut skipped: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut skip = |kind: &str, why: &str| {
        let s = skipped
            .entry(kind.to_string())
            .or_insert((0, why.to_string()));
        s.0 += 1;
    };
    // what is covered, and what everything it refers to being covered too
    let mut keep: HashSet<ElementId> = model
        .iter()
        .filter(|e| covered(model, e))
        .map(|e| e.id)
        .collect();
    loop {
        let before = keep.len();
        let drop: Vec<ElementId> = model
            .iter()
            .filter(|e| keep.contains(&e.id))
            .filter(|e| e.references().iter().any(|r| !keep.contains(r)))
            .map(|e| e.id)
            .collect();
        for d in drop {
            keep.remove(&d);
        }
        if keep.len() == before {
            break;
        }
    }
    let mut ids = Ids {
        of: HashMap::new(),
        used: HashSet::new(),
    };
    for e in model.iter() {
        if !keep.contains(&e.id) {
            let kind = match &e.kind {
                ElementKind::Wall(w) if w.curtain.is_some() => "curtain wall",
                ElementKind::ElementType(_) if covered(model, e) => "type",
                ElementKind::ElementType(t) => match t.params {
                    core::TypeParams::Object(_) => "object type",
                    core::TypeParams::Footing(_) => "footing type",
                    _ => "curtain wall type",
                },
                ElementKind::Object(_) => "object",
                k => k.name(),
            };
            let why = if covered(model, e) {
                "it refers to something the format leaves out"
            } else if kind.starts_with("object") {
                "only objects from the starter catalogue are in the format yet"
            } else {
                not_in_format(e.kind.name())
            };
            skip(kind, why);
            continue;
        }
        match &e.kind {
            ElementKind::Viewport(_)
            | ElementKind::Units(_)
            | ElementKind::Project(_)
            | ElementKind::Tag(_)
            | ElementKind::Note(_) => {}
            _ if object_type_key(e).is_some() => {}
            ElementKind::Sheet(s) => ids.give(e.id, &s.number),
            ElementKind::Grid(g) => ids.give(e.id, &g.label),
            _ => ids.give(e.id, &e.name),
        }
    }

    let mut spec = f::Model {
        schema: None,
        version: f::VERSION,
        units: f::LengthIn::Metres,
        display_units: None,
        materials: vec![],
        assemblies: vec![],
        types: vec![],
        levels: vec![],
        grid_groups: vec![],
        grids: vec![],
        walls: vec![],
        openings: vec![],
        slabs: vec![],
        roofs: vec![],
        columns: vec![],
        beams: vec![],
        stairs: vec![],
        railings: vec![],
        parts: vec![],
        site: vec![],
        objects: vec![],
        rooms: vec![],
        views: vec![],
        sheets: vec![],
        tags: vec![],
        notes: vec![],
    };
    let mut lost_notes = 0;
    let mut lost_view_settings = 0;
    let mut lost_templates = 0;
    let mut lost_joins = 0;
    for e in model.iter().filter(|e| keep.contains(&e.id)) {
        let id = ids.get(e.id).unwrap_or_default();
        match &e.kind {
            ElementKind::Units(u) => {
                spec.display_units = Some(f::DisplayUnits {
                    length: match u.length {
                        core::LengthUnit::Millimetres => f::DisplayLength::Millimetres,
                        core::LengthUnit::Centimetres => f::DisplayLength::Centimetres,
                        core::LengthUnit::Metres => f::DisplayLength::Metres,
                        core::LengthUnit::FeetInches => f::DisplayLength::FeetInches,
                        core::LengthUnit::Feet => f::DisplayLength::Feet,
                        core::LengthUnit::Inches => f::DisplayLength::Inches,
                    },
                    precision: u32::from(u.precision),
                })
            }
            ElementKind::Material(m) => spec.materials.push(material(e, &id, m)),
            ElementKind::Assembly(a) => spec.assemblies.push(f::Assembly {
                name: name(e, &id),
                id,
                usage: match a.usage {
                    core::AssemblyUse::Wall => f::AssemblyUsage::Wall,
                    core::AssemblyUse::Floor => f::AssemblyUsage::Floor,
                    core::AssemblyUse::Roof => f::AssemblyUsage::Roof,
                },
                layers: a
                    .layers
                    .iter()
                    .map(|l| f::Layer {
                        function: match l.function {
                            core::LayerFunction::Structure => f::LayerFunction::Structure,
                            core::LayerFunction::Substrate => f::LayerFunction::Substrate,
                            core::LayerFunction::Insulation => f::LayerFunction::Insulation,
                            core::LayerFunction::AirGap => f::LayerFunction::AirGap,
                            core::LayerFunction::Membrane => f::LayerFunction::Membrane,
                            core::LayerFunction::Finish => f::LayerFunction::Finish,
                        },
                        material: ids.of(l.material),
                        thickness: l.thickness,
                        priority: l.priority.map(u32::from),
                    })
                    .collect(),
                description: (!a.description.is_empty()).then(|| a.description.clone()),
            }),
            ElementKind::ElementType(_) if object_type_key(e).is_some() => {}
            ElementKind::ElementType(t) => spec.types.push(element_type(e, &id, t, &ids)),
            ElementKind::Level(l) => spec.levels.push(f::Level {
                name: name(e, &id),
                id,
                elevation: l.elevation,
                height: l.height,
            }),
            ElementKind::GridGroup(g) => {
                let label = |s: core::grids::LabelStyle| match s {
                    core::grids::LabelStyle::Numbers => f::GridLabels::Numbers,
                    core::grids::LabelStyle::Letters => f::GridLabels::Letters,
                };
                let default = core::GridGroup::default();
                spec.grid_groups.push(f::GridGroup {
                    name: name(e, &id),
                    id,
                    prefix: g.prefix.clone(),
                    vertical: label(g.vertical),
                    horizontal: label(g.horizontal),
                    colour: (g.color != default.color).then_some(Colour(g.color)),
                })
            }
            ElementKind::Grid(g) => spec.grids.push(f::Grid {
                name: (e.name != format!("Grid {}", g.label)).then(|| e.name.clone()),
                label: (g.label != id).then(|| g.label.clone()),
                id,
                start: g.start,
                end: g.end,
                group: ids.of(g.group),
            }),
            ElementKind::Wall(w) => {
                lost_joins += usize::from(w.disallow_join != [false; 2]);
                spec.walls.push(wall(model, e, &id, w, &ids));
            }
            ElementKind::Opening(o) => spec.openings.push(opening(e, &id, o, &ids)),
            ElementKind::Slab(s) => spec.slabs.push(f::Slab {
                name: name(e, &id),
                id,
                level: ids.get(s.level).unwrap_or_default(),
                outline: s.boundary.clone(),
                bulges: if s.has_arcs() {
                    s.bulges.clone()
                } else {
                    vec![]
                },
                holes: s.holes.clone(),
                thickness: some_if(s.thickness, thickness_written(model, e)),
                top_offset: s.top_offset,
                r#type: e.of_type.as_ref().and_then(|t| ids.get(t.id)),
                material: ids.of(s.material).filter(|_| own(e, Param::Material)),
                assembly: ids.of(s.assembly).filter(|_| own(e, Param::Assembly)),
            }),
            ElementKind::Roof(r) => spec.roofs.push(f::Roof {
                name: name(e, &id),
                id,
                level: ids.get(r.level).unwrap_or_default(),
                outline: r.boundary.clone(),
                shape: match r.shape {
                    core::RoofShape::Flat => f::RoofShape::Flat,
                    core::RoofShape::Shed => f::RoofShape::Shed,
                    core::RoofShape::Gable => f::RoofShape::Gable,
                    core::RoofShape::Hip => f::RoofShape::Hip,
                },
                pitch: (r.shape != core::RoofShape::Flat || r.pitch != 0.0).then_some(r.pitch),
                overhang: r.overhang,
                thickness: some_if(r.thickness, thickness_written(model, e)),
                base_offset: r.base_offset,
                edge: r.edge as u32,
                r#type: e.of_type.as_ref().and_then(|t| ids.get(t.id)),
                material: ids.of(r.material).filter(|_| own(e, Param::Material)),
                assembly: ids.of(r.assembly).filter(|_| own(e, Param::Assembly)),
                holes: r.holes.clone(),
            }),
            ElementKind::Column(c) => {
                let (width, depth, diameter) = match c.profile {
                    _ if !own(e, Param::Profile) => (None, None, None),
                    core::Profile::Rectangle { width, depth } => (Some(width), Some(depth), None),
                    core::Profile::Round { diameter } => (None, None, Some(diameter)),
                };
                let (top_level, top_offset) = match c.top {
                    core::WallTop::UpToLevel { level, offset } => (ids.get(level), offset),
                    core::WallTop::Unconnected => (None, 0.0),
                };
                spec.columns.push(f::Column {
                    name: name(e, &id),
                    mark: e.mark.clone(),
                    id,
                    level: ids.get(c.level).unwrap_or_default(),
                    at: c.position,
                    r#type: e.of_type.as_ref().and_then(|t| ids.get(t.id)),
                    width,
                    depth,
                    diameter,
                    rotation: c.rotation,
                    base_offset: c.base_offset,
                    height: c.height,
                    top_offset: if top_level.is_some() { top_offset } else { 0.0 },
                    top_level,
                    material: ids.of(c.material).filter(|_| own(e, Param::Material)),
                })
            }
            ElementKind::Beam(b) => spec.beams.push(f::Beam {
                name: name(e, &id),
                mark: e.mark.clone(),
                id,
                level: ids.get(b.level).unwrap_or_default(),
                start: b.start,
                end: b.end,
                r#type: e.of_type.as_ref().and_then(|t| ids.get(t.id)),
                width: some_if(b.width, own(e, Param::Width)),
                depth: some_if(b.depth, own(e, Param::Depth)),
                top_offset: b.top_offset,
                material: ids.of(b.material).filter(|_| own(e, Param::Material)),
            }),
            ElementKind::Stair(s) => spec.stairs.push(f::Stair {
                name: name(e, &id),
                id,
                level: ids.get(s.level).unwrap_or_default(),
                start: s.start,
                direction: s.direction,
                width: s.width,
                max_riser: s.max_riser,
                tread: s.tread,
                height: s.height,
                base_offset: s.base_offset,
                waist: s.waist,
                landing: s.landing.map(|l| f::Landing {
                    after: l.after,
                    turn: if l.left {
                        f::Side::Left
                    } else {
                        f::Side::Right
                    },
                }),
                material: ids.of(s.material),
            }),
            ElementKind::Railing(r) => spec.railings.push(f::Railing {
                name: name(e, &id),
                id,
                level: ids.get(r.level).unwrap_or_default(),
                path: if r.stair.is_some() {
                    vec![]
                } else {
                    r.path.clone()
                },
                height: r.height,
                post_spacing: r.post_spacing,
                stair: r.stair.and_then(|s| ids.get(s.stair)),
                side: r.stair.map(|s| {
                    if s.left {
                        f::Side::Left
                    } else {
                        f::Side::Right
                    }
                }),
                material: ids.of(r.material),
            }),
            ElementKind::Part(p) => spec.parts.push(part(e, &id, p, &ids)),
            ElementKind::Site(s) => spec.site.push(site_element(e, &id, s, &ids)),
            ElementKind::Object(o) => {
                let key = catalogue_key(model, e).unwrap_or_default();
                let base = core::catalogue::starter(&key).map(|c| c.object.offset);
                let type_name = model
                    .get(e.of_type.as_ref().map(|t| t.id).unwrap_or_default())
                    .map(|t| t.name.clone());
                spec.objects.push(f::Object {
                    name: match &type_name {
                        // named as the type, as a placed object is
                        Some(n) if *n == e.name => None,
                        _ => Some(e.name.clone()),
                    }
                    .filter(|n| *n != id),
                    mark: e.mark.clone(),
                    id,
                    level: ids.get(o.level).unwrap_or_default(),
                    catalogue: key,
                    at: o.position,
                    anchor: f::Anchor::Origin,
                    rotation: o.rotation,
                    offset: (Some(o.offset) != base).then_some(o.offset),
                })
            }
            ElementKind::Room(r) => spec.rooms.push(f::Room {
                name: name(e, &id),
                id,
                number: r.number.clone(),
                level: ids.get(r.level).unwrap_or_default(),
                at: r.at,
                outline: r.boundary.clone(),
                height: r.height,
                floor_finish: r.floor_finish.clone(),
                wall_finish: r.wall_finish.clone(),
                ceiling_finish: r.ceiling_finish.clone(),
                occupancy: r.occupancy.clone(),
                department: r.department.clone(),
            }),
            ElementKind::View(v) => {
                let plain = core::View::plan(ElementId::NIL);
                let lost = !v.hidden.is_empty()
                    || !v.underlay.is_none()
                    || v.show_hidden
                    || v.hidden_in_colour
                    || v.graphics != plain.graphics
                    || !v.level_heads.is_empty();
                lost_view_settings += usize::from(lost);
                spec.views.push(view(e, &id, v, &ids));
            }
            ElementKind::Sheet(s) => {
                lost_templates += usize::from(s.template.is_some());
                let size = s.size_name().and_then(|n| match n {
                    "A0" => Some(f::PaperSize::A0),
                    "A1" => Some(f::PaperSize::A1),
                    "A2" => Some(f::PaperSize::A2),
                    "A3" => Some(f::PaperSize::A3),
                    "A4" => Some(f::PaperSize::A4),
                    _ => None,
                });
                let viewports = model
                    .iter()
                    .filter(|v| keep.contains(&v.id))
                    .filter_map(|v| match &v.kind {
                        ElementKind::Viewport(vp) if vp.sheet == e.id => Some(f::Viewport {
                            view: ids.get(vp.view).unwrap_or_default(),
                            at: vp.center,
                            target: Some(vp.target),
                            crop: vp.crop.map(|[min, max]| f::Rect { min, max }),
                        }),
                        _ => None,
                    })
                    .collect();
                spec.sheets.push(f::Sheet {
                    name: name(e, &id),
                    id,
                    size,
                    width: size.is_none().then_some(s.width),
                    height: size.is_none().then_some(s.height),
                    fields: s.fields.clone(),
                    viewports,
                })
            }
            ElementKind::Tag(t) => {
                lost_notes += usize::from(t.style.is_some() || !t.fields.is_empty());
                spec.tags.push(f::Tag {
                    view: ids.get(t.view).unwrap_or_default(),
                    element: ids.get(t.host).unwrap_or_default(),
                    offset: t.offset,
                    elbow: t.elbow,
                    end: t.end,
                })
            }
            ElementKind::Note(n) => {
                lost_notes += usize::from(n.style.is_some());
                spec.notes.push(f::Note {
                    view: ids.get(n.view).unwrap_or_default(),
                    at: n.at,
                    text: n.text.clone(),
                    leader: n.leader.map(|l| l.local),
                    elbow: n.elbow,
                    colour: n.color.map(Colour),
                })
            }
            ElementKind::Viewport(_) => {}
            _ => {}
        }
    }
    let mut skipped: Vec<String> = skipped
        .into_iter()
        .map(|(kind, (n, why))| format!("{n} {kind} ({why})"))
        .collect();
    if lost_view_settings > 0 {
        skipped.push(format!(
            "the visibility, graphics, underlay or level heads of {lost_view_settings} views \
             (not in the format yet)"
        ));
    }
    if lost_templates > 0 {
        skipped.push(format!(
            "the title block template of {lost_templates} sheets (they get the built-in one)"
        ));
    }
    if lost_notes > 0 {
        skipped.push(format!(
            "the text styles and room fields of {lost_notes} tags and notes \
             (not in the format yet)"
        ));
    }
    if lost_joins > 0 {
        skipped.push(format!(
            "disallowed joins at the ends of {lost_joins} walls (not in the format yet)"
        ));
    }
    Exported { spec, skipped }
}

fn material(e: &Element, id: &str, m: &core::Material) -> f::Material {
    let d = core::Material::default();
    let hatch = match m.hatch {
        core::Hatch::Empty => f::Hatch::Empty,
        core::Hatch::Solid => f::Hatch::Solid,
        core::Hatch::Diagonal => f::Hatch::Diagonal,
        core::Hatch::Cross => f::Hatch::Cross,
        core::Hatch::Brick => f::Hatch::Brick,
        core::Hatch::Concrete => f::Hatch::Concrete,
        core::Hatch::Timber => f::Hatch::Timber,
    };
    let text = |s: &String| (!s.is_empty()).then(|| s.clone());
    let unit = |v: f32, d: f32| (v != d).then_some(short(v));
    f::Material {
        id: id.into(),
        name: name(e, id),
        library: None,
        hatch: Some(hatch),
        colour: Some(Colour(m.color)),
        category: text(&m.category),
        description: text(&m.description),
        roughness: unit(m.roughness, d.roughness),
        metallic: unit(m.metallic, d.metallic),
        opacity: unit(m.opacity, d.opacity),
        rate: m.rate.map(|r| r.amount),
        rate_per: m.rate.map(|r| match r.per {
            core::takeoff::RateUnit::Area => f::RatePer::SquareMetre,
            core::takeoff::RateUnit::Volume => f::RatePer::CubicMetre,
            core::takeoff::RateUnit::Length => f::RatePer::Metre,
            core::takeoff::RateUnit::Each => f::RatePer::Each,
        }),
    }
}

/// An f32 as the f64 its shortest form reads as: 0.95, not 0.949999988.
pub(crate) fn short(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(f64::from(v))
}

fn location(l: core::LocationLine) -> f::Location {
    match l {
        core::LocationLine::Centre => f::Location::Centre,
        core::LocationLine::CoreCentre => f::Location::CoreCentre,
        core::LocationLine::ExteriorFace => f::Location::ExteriorFace,
        core::LocationLine::InteriorFace => f::Location::InteriorFace,
        core::LocationLine::CoreExterior => f::Location::CoreExterior,
        core::LocationLine::CoreInterior => f::Location::CoreInterior,
        core::LocationLine::Boundary(_) => f::Location::LayerBoundary,
    }
}

fn boundary(l: core::LocationLine) -> Option<u32> {
    match l {
        core::LocationLine::Boundary(k) => Some(u32::from(k)),
        _ => None,
    }
}

fn frame(fr: &core::OpeningFrame) -> f::Frame {
    f::Frame {
        width: fr.width,
        depth: fr.depth,
        panes_across: fr.panes_across,
        panes_up: fr.panes_up,
    }
}

fn element_type(e: &Element, id: &str, t: &core::ElementType, ids: &Ids) -> f::Type {
    let mut out = f::Type {
        id: id.into(),
        name: name(e, id),
        category: f::TypeCategory::Wall,
        description: (!t.description.is_empty()).then(|| t.description.clone()),
        thickness: None,
        assembly: None,
        material: ids.of(t.material()),
        location: None,
        boundary: None,
        height: None,
        width: None,
        sill: None,
        frame: None,
        depth: None,
        diameter: None,
        screen: None,
    };
    match &t.params {
        core::TypeParams::Wall(w) => {
            out.thickness = Some(w.thickness);
            out.assembly = ids.of(w.assembly);
            out.location = (!w.location.is_centre()).then(|| location(w.location));
            out.boundary = boundary(w.location);
            out.height = w.height;
            out.screen = w.screen.as_ref().map(|s| f::Screen {
                unit: s.unit,
                holes: s
                    .holes
                    .iter()
                    .map(|[min, max]| f::Rect {
                        min: *min,
                        max: *max,
                    })
                    .collect(),
                frame: s.frame,
                stagger: s.stagger,
            });
        }
        core::TypeParams::Door(o) | core::TypeParams::Window(o) => {
            let window = matches!(t.params, core::TypeParams::Window(_));
            out.category = if window {
                f::TypeCategory::Window
            } else {
                f::TypeCategory::Door
            };
            out.width = Some(o.width);
            out.height = Some(o.height);
            let default_sill = if window { 0.9 } else { 0.0 };
            out.sill = (o.sill != default_sill).then_some(o.sill);
            out.frame = (!o.frame.is_default()).then(|| frame(&o.frame));
        }
        core::TypeParams::Slab(s) | core::TypeParams::Roof(s) => {
            out.category = if matches!(t.params, core::TypeParams::Roof(_)) {
                f::TypeCategory::Roof
            } else {
                f::TypeCategory::Slab
            };
            out.thickness = Some(s.thickness);
            out.assembly = ids.of(s.assembly);
        }
        core::TypeParams::Column(c) => {
            out.category = f::TypeCategory::Column;
            match c.profile {
                core::Profile::Rectangle { width, depth } => {
                    out.width = Some(width);
                    out.depth = Some(depth);
                }
                core::Profile::Round { diameter } => out.diameter = Some(diameter),
            }
        }
        core::TypeParams::Beam(b) => {
            out.category = f::TypeCategory::Beam;
            out.width = Some(b.width);
            out.depth = Some(b.depth);
        }
        _ => {}
    }
    out
}

/// Is a wall, slab or roof's thickness its own to write? A layered one
/// is as thick as its layers, and a typed one as its type unless it
/// sets its own.
fn thickness_written(model: &Model, e: &Element) -> bool {
    let layered = core::assembly_of(model, e).is_some();
    if e.of_type.is_some() {
        own(e, Param::Thickness) && !(layered && !own(e, Param::Assembly))
    } else {
        !layered
    }
}

fn wall(model: &Model, e: &Element, id: &str, w: &core::Wall, ids: &Ids) -> f::Wall {
    let typed = e.of_type.is_some();
    let (top_level, top_offset) = match w.top {
        core::WallTop::UpToLevel { level, offset } if ids.get(level).is_some() => {
            (ids.get(level), offset)
        }
        _ => (None, 0.0),
    };
    f::Wall {
        id: id.into(),
        name: name(e, id),
        mark: e.mark.clone(),
        level: ids.get(w.level).unwrap_or_default(),
        start: w.start,
        end: w.end,
        r#type: e.of_type.as_ref().and_then(|t| ids.get(t.id)),
        thickness: some_if(w.thickness, thickness_written(model, e)),
        height: top_level.is_none().then_some(w.height),
        base_offset: w.base_offset,
        top_offset: if top_level.is_some() { top_offset } else { 0.0 },
        top_level,
        material: ids.of(w.material).filter(|_| own(e, Param::Material)),
        assembly: ids.of(w.assembly).filter(|_| own(e, Param::Assembly)),
        location: (own(e, Param::Location) && !(w.location.is_centre() && !typed))
            .then(|| location(w.location)),
        boundary: boundary(w.location).filter(|_| own(e, Param::Location)),
        flip: w.flip,
        attach_top: w.attach_top.iter().filter_map(|a| ids.get(*a)).collect(),
    }
}

fn opening(e: &Element, id: &str, o: &core::Opening, ids: &Ids) -> f::Opening {
    let kind = match o.kind {
        core::OpeningKind::Door => f::OpeningKind::Door,
        core::OpeningKind::Window => f::OpeningKind::Window,
        core::OpeningKind::Hole => f::OpeningKind::Hole,
    };
    let typed = e.of_type.is_some();
    let default_sill = if o.kind == core::OpeningKind::Window {
        0.9
    } else {
        0.0
    };
    f::Opening {
        id: id.into(),
        name: name(e, id),
        mark: e.mark.clone(),
        kind,
        wall: ids.get(o.host).unwrap_or_default(),
        offset: Some(o.offset),
        centre: None,
        r#type: e.of_type.as_ref().and_then(|t| ids.get(t.id)),
        width: some_if(o.width, own(e, Param::Width)),
        height: some_if(o.height, own(e, Param::Height)),
        sill: some_if(
            o.sill,
            own(e, Param::Sill) && (typed || o.sill != default_sill),
        ),
        material: ids.of(o.material).filter(|_| own(e, Param::Material)),
        frame: (own(e, Param::Frame) && (typed || !o.frame.is_default())).then(|| frame(&o.frame)),
        flip_hand: o.flip_hand,
        flip_facing: o.flip_facing,
    }
}

fn part(e: &Element, id: &str, p: &parts::Part, ids: &Ids) -> f::Part {
    let mut out = f::Part {
        id: id.into(),
        name: name(e, id),
        level: ids.get(p.level).unwrap_or_default(),
        outline: vec![],
        holes: vec![],
        base: 0.0,
        height: None,
        plane: None,
        profile: vec![],
        path: vec![],
        closed: false,
        face_normal: None,
        arc: None,
        wall: None,
        edge: f::WallEdge::Top,
        offset: 0.0,
        extend: None,
        void: p.void,
        material: ids.of(p.material),
        ifc_class: (p.ifc_class != "IfcBuildingElementProxy").then(|| p.ifc_class.clone()),
    };
    match &p.form {
        parts::Form::Extrusion {
            outline,
            holes,
            base,
            height,
            plane,
        } => {
            out.plane = plane.map(|pl| f::Plane {
                origin: pl.origin,
                x: pl.x,
                y: pl.y,
            });
            out.outline = outline.clone();
            out.holes = holes.clone();
            out.base = *base;
            out.height = Some(*height);
        }
        parts::Form::Sweep { profile, path } => {
            out.profile = profile.clone();
            match path {
                parts::Path::Points {
                    points,
                    closed,
                    normal,
                } => {
                    out.path = points.clone();
                    out.closed = *closed;
                    out.face_normal = *normal;
                }
                parts::Path::Arc {
                    center,
                    axis,
                    start,
                    sweep,
                } => {
                    out.arc = Some(f::ArcPath {
                        center: *center,
                        axis: *axis,
                        start: *start,
                        sweep: sweep.to_degrees(),
                    });
                }
                parts::Path::Wall {
                    wall,
                    edge,
                    offset,
                    extend,
                } => {
                    out.wall = ids.get(*wall);
                    out.edge = match edge {
                        parts::WallEdge::Top => f::WallEdge::Top,
                        parts::WallEdge::Base => f::WallEdge::Base,
                    };
                    out.offset = *offset;
                    out.extend = (*extend != [0.0; 2]).then(|| glam::DVec2::from_array(*extend));
                }
            }
        }
    }
    out
}

fn site_element(e: &Element, id: &str, s: &site::Site, ids: &Ids) -> f::SiteElement {
    let mut out = f::SiteElement {
        id: id.into(),
        name: name(e, id),
        kind: f::SiteKind::Terrain,
        level: ids.get(s.level).unwrap_or_default(),
        outline: vec![],
        holes: vec![],
        top: 0.0,
        thickness: None,
        points: vec![],
        contour: 0.0,
        pattern: None,
        at: None,
        base: 0.0,
        plant: f::PlantForm::Tree,
        canopy: None,
        height: None,
        material: ids.of(s.material),
    };
    match &s.form {
        site::SiteForm::Terrain {
            boundary,
            top,
            thickness,
            points,
            contour,
        } => {
            out.outline = boundary.clone();
            out.top = *top;
            out.thickness = Some(*thickness);
            out.points = points.clone();
            out.contour = *contour;
        }
        site::SiteForm::Area {
            boundary,
            holes,
            top,
            thickness,
            pattern,
        } => {
            out.kind = f::SiteKind::Area;
            out.outline = boundary.clone();
            out.holes = holes.clone();
            out.top = *top;
            out.thickness = Some(*thickness);
            let p = |kind| f::SitePattern {
                kind,
                size: None,
                stagger: false,
                angle: 0.0,
                spacing: None,
            };
            out.pattern = match *pattern {
                site::Pattern::None => None,
                site::Pattern::Units {
                    size,
                    stagger,
                    angle,
                } => Some(f::SitePattern {
                    size: Some(size),
                    stagger,
                    angle,
                    ..p(f::PatternKind::Units)
                }),
                site::Pattern::Lines { spacing, angle } => Some(f::SitePattern {
                    spacing: Some(spacing),
                    angle,
                    ..p(f::PatternKind::Lines)
                }),
                site::Pattern::Stipple { spacing } => Some(f::SitePattern {
                    spacing: Some(spacing),
                    ..p(f::PatternKind::Stipple)
                }),
            };
        }
        site::SiteForm::Plant {
            position,
            base,
            form,
            canopy,
            height,
        } => {
            out.kind = f::SiteKind::Plant;
            out.at = Some(*position);
            out.base = *base;
            out.plant = match form {
                core::PlantForm::Tree => f::PlantForm::Tree,
                core::PlantForm::Conifer => f::PlantForm::Conifer,
                core::PlantForm::Palm => f::PlantForm::Palm,
                core::PlantForm::Shrub => f::PlantForm::Shrub,
            };
            out.canopy = Some(*canopy);
            out.height = Some(*height);
        }
    }
    out
}

fn view(e: &Element, id: &str, v: &core::View, ids: &Ids) -> f::View {
    let mut out = f::View {
        id: id.into(),
        name: name(e, id),
        kind: f::ViewKind::Plan,
        scale: v.scale,
        style: None,
        level: None,
        cut_height: 1.2,
        ceiling_plan: v.ceiling_plan,
        room_colours: v.room_colours,
        start: None,
        end: None,
        depth: None,
        azimuth: 30.0,
        altitude: 30.0,
        category: None,
        by_type: false,
        callout: v.callout.and_then(|co| {
            Some(f::Callout {
                parent: ids.get(co.parent)?,
                min: co.lo,
                max: co.hi,
            })
        }),
    };
    let mut default_style = core::ViewStyle::HiddenLine;
    match &v.kind {
        core::ViewKind::Plan { level, cut_height } => {
            out.level = ids.get(*level);
            out.cut_height = *cut_height;
        }
        core::ViewKind::Section {
            start,
            end,
            depth,
            cut,
        } => {
            out.kind = if *cut {
                f::ViewKind::Section
            } else {
                f::ViewKind::Elevation
            };
            out.start = Some(*start);
            out.end = Some(*end);
            out.depth = Some(*depth);
        }
        core::ViewKind::Axonometric { azimuth, altitude } => {
            out.kind = f::ViewKind::ThreeD;
            out.azimuth = *azimuth;
            out.altitude = *altitude;
            default_style = core::ViewStyle::Shaded;
        }
        core::ViewKind::Schedule { category, by_type } => {
            out.kind = f::ViewKind::Schedule;
            out.scale = 100.0;
            out.by_type = *by_type;
            out.category = Some(match category {
                core::ScheduleCategory::Doors => f::ScheduleCategory::Doors,
                core::ScheduleCategory::Windows => f::ScheduleCategory::Windows,
                core::ScheduleCategory::Walls => f::ScheduleCategory::Walls,
                core::ScheduleCategory::Slabs => f::ScheduleCategory::Slabs,
                core::ScheduleCategory::Structure => f::ScheduleCategory::Structure,
                core::ScheduleCategory::Furniture => f::ScheduleCategory::Furniture,
                core::ScheduleCategory::Fixtures => f::ScheduleCategory::Fixtures,
                core::ScheduleCategory::Rooms => f::ScheduleCategory::Rooms,
                core::ScheduleCategory::Materials => f::ScheduleCategory::Materials,
            });
        }
    }
    if v.style != default_style {
        out.style = Some(match v.style {
            core::ViewStyle::HiddenLine => f::ViewStyle::HiddenLine,
            core::ViewStyle::Shaded => f::ViewStyle::Shaded,
            core::ViewStyle::Wireframe => f::ViewStyle::Wireframe,
        });
    }
    out
}
