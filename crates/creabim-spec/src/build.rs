//! A model description into a model. Every id the file uses is checked
//! against the kind of element it must name (a door's wall must be a
//! wall), every element against what the model accepts, and each problem
//! is reported at its path in the file. An element with a problem is left
//! out and what refers to it says nothing more, so one mistake gives one
//! message.

use crate::format as f;
use crate::json::{suggest, Path, Problem, Spec, J};
use creabim_core::{
    self as core, parts, site, types, CatalogueObject, Document, Element, ElementId, ElementKind,
    Model, Transaction,
};
use glam::{DVec2, DVec3};
use std::collections::{HashMap, HashSet};

/// A built model, and what was worth saying about the file on the way.
pub struct Built {
    pub model: Model,
    /// Things that built but look wrong (a window taller than its wall).
    pub warnings: Vec<Problem>,
}

/// What an id names, so a reference to the wrong kind is caught.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    Material,
    Assembly,
    Type(f::TypeCategory),
    Level,
    GridGroup,
    Grid,
    Wall,
    Opening,
    Slab,
    Roof,
    Column,
    Beam,
    Stair,
    Railing,
    Part,
    Site,
    Object,
    Room,
    View,
    Sheet,
}

impl Tag {
    fn word(self) -> String {
        match self {
            Tag::Material => "a material".into(),
            Tag::Assembly => "an assembly".into(),
            Tag::Type(c) => format!("a {} type", c.key()),
            Tag::Level => "a level".into(),
            Tag::GridGroup => "a grid group".into(),
            Tag::Grid => "a grid".into(),
            Tag::Wall => "a wall".into(),
            Tag::Opening => "an opening".into(),
            Tag::Slab => "a slab".into(),
            Tag::Roof => "a roof".into(),
            Tag::Column => "a column".into(),
            Tag::Beam => "a beam".into(),
            Tag::Stair => "a stair".into(),
            Tag::Railing => "a railing".into(),
            Tag::Part => "a part".into(),
            Tag::Site => "a site element".into(),
            Tag::Object => "an object".into(),
            Tag::Room => "a room".into(),
            Tag::View => "a view".into(),
            Tag::Sheet => "a sheet".into(),
        }
    }
}

struct Ctx {
    /// Metres in one of the file's units.
    k: f64,
    ids: HashMap<String, (ElementId, Tag, Path)>,
    /// Ids whose element had a problem: what refers to them is quiet.
    broken: HashSet<String>,
    errors: Vec<Problem>,
    warnings: Vec<Problem>,
    doc: Document,
}

impl Ctx {
    fn l(&self, v: f64) -> f64 {
        v * self.k
    }
    fn p(&self, v: DVec2) -> DVec2 {
        v * self.k
    }
    fn p3(&self, v: DVec3) -> DVec3 {
        v * self.k
    }
    fn pts(&self, v: &[DVec2]) -> Vec<DVec2> {
        v.iter().map(|p| self.p(*p)).collect()
    }
    fn model(&self) -> &Model {
        self.doc.model()
    }
    fn err(&mut self, at: &Path, msg: impl Into<String>) {
        self.errors.push(Problem::new(at, msg));
    }

    /// Claim `id` for an element at `at`.
    fn claim(&mut self, id: &str, tag: Tag, at: &Path) {
        if id.is_empty() {
            self.err(&at.key("id"), "an id can't be empty");
            return;
        }
        if let Some((_, _, first)) = self.ids.get(id) {
            let first = first.clone();
            self.err(
                &at.key("id"),
                format!("the id \"{id}\" is already used by {first}"),
            );
            return;
        }
        self.ids
            .insert(id.to_string(), (ElementId::new(), tag, at.clone()));
    }

    /// The element `id` was claimed for, if it is one of `want`.
    fn get(&mut self, id: &str, want: &[Tag], at: &Path) -> Option<ElementId> {
        if self.broken.contains(id) {
            return None;
        }
        let words = || {
            want.iter()
                .map(|t| t.word())
                .collect::<Vec<_>>()
                .join(" or ")
        };
        match self.ids.get(id) {
            Some((eid, tag, _)) if want.contains(tag) => Some(*eid),
            Some((_, tag, _)) => {
                let msg = format!("\"{id}\" is {}, not {}", tag.word(), words());
                self.err(at, msg);
                None
            }
            None => {
                let known: Vec<&str> = self
                    .ids
                    .iter()
                    .filter(|(_, (_, t, _))| want.contains(t))
                    .map(|(k, _)| k.as_str())
                    .collect();
                let hint = suggest(id, &known)
                    .map(|s| format!("; did you mean \"{s}\"?"))
                    .unwrap_or_default();
                self.err(
                    at,
                    format!("there is no {} with the id \"{id}\"{hint}", {
                        let w = words();
                        w.split_once(' ').map(|(_, r)| r.to_string()).unwrap_or(w)
                    }),
                );
                None
            }
        }
    }

    fn opt(&mut self, id: &Option<String>, want: &[Tag], at: &Path) -> Option<Option<ElementId>> {
        match id {
            None => Some(None),
            Some(i) => self.get(i, want, at).map(Some),
        }
    }

    fn eid(&self, id: &str) -> ElementId {
        self.ids[id].0
    }

    /// Put `e` (claimed as `id`, described at `at`) into the model.
    /// Did the element at `at` claim `id`? Not when another had it first
    /// (already reported).
    fn owns(&self, id: &str, at: &Path) -> bool {
        self.ids.get(id).is_some_and(|(_, _, p)| p == at)
    }

    fn insert(&mut self, id: &str, mut e: Element, at: &Path) {
        if !self.owns(id, at) {
            return;
        }
        e.id = self.eid(id);
        self.commit(id, Transaction::new("Build").insert(e), at);
    }

    fn commit(&mut self, id: &str, tx: Transaction, at: &Path) {
        for op in &tx.ops {
            if let core::Op::Insert(e) = op {
                if let Err(m) = e.validate() {
                    self.err(at, m);
                    self.broken.insert(id.to_string());
                    return;
                }
            }
        }
        if let Err(e) = self.doc.commit(tx) {
            self.err(at, e.to_string());
            self.broken.insert(id.to_string());
        }
    }

    /// Note that the element `id` had a problem.
    fn fail(&mut self, id: &str) {
        self.broken.insert(id.to_string());
    }
}

/// The keys of `v` as written that `allowed` doesn't list: fields given
/// that its kind doesn't use.
fn stray(v: &impl Spec, allowed: &[&str]) -> Vec<String> {
    match v.write() {
        J::Obj(o) => o
            .into_iter()
            .map(|(k, _)| k)
            .filter(|k| !allowed.contains(&k.as_str()))
            .collect(),
        _ => vec![],
    }
}

fn name_of(name: &Option<String>, id: &str) -> String {
    name.clone().unwrap_or_else(|| id.to_string())
}

/// Turn a model description into a model.
pub fn build(spec: &f::Model) -> Result<Built, Vec<Problem>> {
    let mut c = Ctx {
        k: spec.units.metres(),
        ids: HashMap::new(),
        broken: HashSet::new(),
        errors: vec![],
        warnings: vec![],
        doc: Document::new(),
    };
    let root = Path::root();
    claim_all(&mut c, spec, &root);

    if let Some(u) = &spec.display_units {
        let length = match u.length {
            f::DisplayLength::Millimetres => core::LengthUnit::Millimetres,
            f::DisplayLength::Centimetres => core::LengthUnit::Centimetres,
            f::DisplayLength::Metres => core::LengthUnit::Metres,
            f::DisplayLength::FeetInches => core::LengthUnit::FeetInches,
            f::DisplayLength::Feet => core::LengthUnit::Feet,
            f::DisplayLength::Inches => core::LengthUnit::Inches,
        };
        let units = core::Units {
            length,
            precision: u.precision.min(9) as u8,
        };
        c.commit(
            "",
            Transaction::new("Build").insert(Element::new("Units", ElementKind::Units(units))),
            &root.key("display_units"),
        );
    }

    for (i, m) in spec.materials.iter().enumerate() {
        material(&mut c, m, &root.key("materials").index(i));
    }
    for (i, a) in spec.assemblies.iter().enumerate() {
        assembly(&mut c, a, &root.key("assemblies").index(i));
    }
    for (i, t) in spec.types.iter().enumerate() {
        element_type(&mut c, t, &root.key("types").index(i));
    }
    for (i, l) in spec.levels.iter().enumerate() {
        let at = root.key("levels").index(i);
        let level = core::Level {
            elevation: c.l(l.elevation),
            height: c.l(l.height),
        };
        c.insert(
            &l.id,
            Element::new(name_of(&l.name, &l.id), ElementKind::Level(level)),
            &at,
        );
    }
    for (i, g) in spec.grid_groups.iter().enumerate() {
        let at = root.key("grid_groups").index(i);
        let label = |s: f::GridLabels| match s {
            f::GridLabels::Numbers => core::grids::LabelStyle::Numbers,
            f::GridLabels::Letters => core::grids::LabelStyle::Letters,
        };
        let mut group = core::GridGroup {
            prefix: g.prefix.clone(),
            vertical: label(g.vertical),
            horizontal: label(g.horizontal),
            ..Default::default()
        };
        if let Some(col) = g.colour {
            group.color = col.0;
        }
        c.insert(
            &g.id,
            Element::new(name_of(&g.name, &g.id), ElementKind::GridGroup(group)),
            &at,
        );
    }
    for (i, g) in spec.grids.iter().enumerate() {
        let at = root.key("grids").index(i);
        let Some(group) = c.opt(&g.group, &[Tag::GridGroup], &at.key("group")) else {
            c.fail(&g.id);
            continue;
        };
        let grid = core::Grid {
            label: g.label.clone().unwrap_or_else(|| g.id.clone()),
            start: c.p(g.start),
            end: c.p(g.end),
            group,
        };
        let name = g
            .name
            .clone()
            .unwrap_or_else(|| format!("Grid {}", grid.label));
        c.insert(&g.id, Element::new(name, ElementKind::Grid(grid)), &at);
    }
    for (i, w) in spec.walls.iter().enumerate() {
        wall(&mut c, w, &root.key("walls").index(i));
    }
    for (i, s) in spec.slabs.iter().enumerate() {
        slab(&mut c, s, &root.key("slabs").index(i));
    }
    for (i, r) in spec.roofs.iter().enumerate() {
        roof(&mut c, r, &root.key("roofs").index(i));
    }
    for (i, o) in spec.openings.iter().enumerate() {
        opening(&mut c, o, &root.key("openings").index(i));
    }
    for (i, col) in spec.columns.iter().enumerate() {
        column(&mut c, col, &root.key("columns").index(i));
    }
    for (i, b) in spec.beams.iter().enumerate() {
        beam(&mut c, b, &root.key("beams").index(i));
    }
    for (i, s) in spec.stairs.iter().enumerate() {
        stair(&mut c, s, &root.key("stairs").index(i));
    }
    for (i, r) in spec.railings.iter().enumerate() {
        railing(&mut c, r, &root.key("railings").index(i));
    }
    for (i, p) in spec.parts.iter().enumerate() {
        part(&mut c, p, &root.key("parts").index(i));
    }
    for (i, s) in spec.site.iter().enumerate() {
        site_element(&mut c, s, &root.key("site").index(i));
    }
    let catalogue = core::catalogue::starters();
    for (i, o) in spec.objects.iter().enumerate() {
        object(&mut c, o, &catalogue, &root.key("objects").index(i));
    }
    for (i, r) in spec.rooms.iter().enumerate() {
        room(&mut c, r, &root.key("rooms").index(i));
    }
    for (i, v) in spec.views.iter().enumerate() {
        view(&mut c, v, &root.key("views").index(i));
    }
    for (i, s) in spec.sheets.iter().enumerate() {
        sheet(&mut c, s, &root.key("sheets").index(i));
    }
    for (i, t) in spec.tags.iter().enumerate() {
        tag(&mut c, t, &root.key("tags").index(i));
    }
    for (i, n) in spec.notes.iter().enumerate() {
        note(&mut c, n, &root.key("notes").index(i));
    }

    if !c.errors.is_empty() {
        return Err(c.errors);
    }
    let (model, _, _) = c.doc.into_parts();
    Ok(Built {
        model,
        warnings: c.warnings,
    })
}

const TAGGABLE: &[Tag] = &[
    Tag::Wall,
    Tag::Opening,
    Tag::Slab,
    Tag::Roof,
    Tag::Column,
    Tag::Beam,
    Tag::Stair,
    Tag::Railing,
    Tag::Part,
    Tag::Site,
    Tag::Object,
    Tag::Room,
];

fn tag(c: &mut Ctx, t: &f::Tag, at: &Path) {
    let refs = (
        c.get(&t.view, &[Tag::View], &at.key("view")),
        c.get(&t.element, TAGGABLE, &at.key("element")),
    );
    let (Some(view), Some(host)) = refs else {
        return;
    };
    let mut tag = core::Tag::new(view, host, c.p(t.offset));
    tag.elbow = t.elbow.map(|p| c.p(p));
    tag.end = t.end.map(|p| c.p(p));
    c.commit(
        "",
        Transaction::new("Build").insert(Element::new("Tag", ElementKind::Tag(tag))),
        at,
    );
}

fn note(c: &mut Ctx, n: &f::Note, at: &Path) {
    let Some(view) = c.get(&n.view, &[Tag::View, Tag::Sheet], &at.key("view")) else {
        return;
    };
    // on a sheet, paper millimetres; in a view, drawing units
    let on_sheet = matches!(c.ids.get(&n.view), Some((_, Tag::Sheet, _)));
    let p = |c: &Ctx, v: DVec2| if on_sheet { v } else { c.p(v) };
    let mut note = core::Note::new(view, p(c, n.at), n.text.clone());
    note.leader = n.leader.map(|l| core::HostPoint::free(p(c, l)));
    note.elbow = n.elbow.map(|e| p(c, e));
    note.color = n.colour.map(|col| col.0);
    c.commit(
        "",
        Transaction::new("Build").insert(Element::new("Note", ElementKind::Note(note))),
        at,
    );
}

/// Claim every id up front, so elements may refer to ones listed after
/// them (a wall attached to a roof).
fn claim_all(c: &mut Ctx, s: &f::Model, root: &Path) {
    macro_rules! claim {
        ($list:ident, $tag:expr) => {
            for (i, x) in s.$list.iter().enumerate() {
                c.claim(&x.id, $tag, &root.key(stringify!($list)).index(i));
            }
        };
    }
    claim!(materials, Tag::Material);
    claim!(assemblies, Tag::Assembly);
    for (i, t) in s.types.iter().enumerate() {
        c.claim(&t.id, Tag::Type(t.category), &root.key("types").index(i));
    }
    claim!(levels, Tag::Level);
    claim!(grid_groups, Tag::GridGroup);
    claim!(grids, Tag::Grid);
    claim!(walls, Tag::Wall);
    claim!(openings, Tag::Opening);
    claim!(slabs, Tag::Slab);
    claim!(roofs, Tag::Roof);
    claim!(columns, Tag::Column);
    claim!(beams, Tag::Beam);
    claim!(stairs, Tag::Stair);
    claim!(railings, Tag::Railing);
    claim!(parts, Tag::Part);
    claim!(site, Tag::Site);
    claim!(objects, Tag::Object);
    claim!(rooms, Tag::Room);
    claim!(views, Tag::View);
    claim!(sheets, Tag::Sheet);
}

fn hatch(h: f::Hatch) -> core::Hatch {
    match h {
        f::Hatch::Empty => core::Hatch::Empty,
        f::Hatch::Solid => core::Hatch::Solid,
        f::Hatch::Diagonal => core::Hatch::Diagonal,
        f::Hatch::Cross => core::Hatch::Cross,
        f::Hatch::Brick => core::Hatch::Brick,
        f::Hatch::Concrete => core::Hatch::Concrete,
        f::Hatch::Timber => core::Hatch::Timber,
    }
}

fn material(c: &mut Ctx, m: &f::Material, at: &Path) {
    let mut mat = match &m.library {
        None => core::Material::default(),
        Some(name) => match core::library::LIBRARY.iter().find(|e| e.name == name) {
            Some(e) => e.material(),
            None => {
                let names: Vec<&str> = core::library::LIBRARY.iter().map(|e| e.name).collect();
                let hint = suggest(name, &names)
                    .map(|s| format!("; did you mean \"{s}\"?"))
                    .unwrap_or_default();
                c.err(
                    &at.key("library"),
                    format!("no library material is called \"{name}\"{hint}"),
                );
                c.fail(&m.id);
                return;
            }
        },
    };
    if let Some(h) = m.hatch {
        mat.hatch = hatch(h);
    }
    if let Some(col) = m.colour {
        mat.color = col.0;
    }
    if let Some(s) = &m.category {
        mat.category = s.clone();
    }
    if let Some(s) = &m.description {
        mat.description = s.clone();
    }
    for (v, slot, key) in [
        (m.roughness, &mut mat.roughness, "roughness"),
        (m.metallic, &mut mat.metallic, "metallic"),
        (m.opacity, &mut mat.opacity, "opacity"),
    ] {
        if let Some(v) = v {
            if !(0.0..=1.0).contains(&v) {
                c.errors.push(Problem::new(
                    &at.key(key),
                    format!("must be from 0 to 1, got {v}"),
                ));
            }
            *slot = v as f32;
        }
    }
    if let Some(amount) = m.rate {
        if !(amount.is_finite() && amount >= 0.0) {
            c.errors.push(Problem::new(
                &at.key("rate"),
                format!("must be 0 or more, got {amount}"),
            ));
        }
        let per = match m.rate_per {
            None | Some(f::RatePer::SquareMetre) => core::takeoff::RateUnit::Area,
            Some(f::RatePer::CubicMetre) => core::takeoff::RateUnit::Volume,
            Some(f::RatePer::Metre) => core::takeoff::RateUnit::Length,
            Some(f::RatePer::Each) => core::takeoff::RateUnit::Each,
        };
        mat.rate = Some(core::takeoff::Rate { amount, per });
    }
    let name = m
        .name
        .clone()
        .or_else(|| m.library.clone())
        .unwrap_or_else(|| m.id.clone());
    c.insert(&m.id, Element::new(name, ElementKind::Material(mat)), at);
}

fn assembly(c: &mut Ctx, a: &f::Assembly, at: &Path) {
    let mut layers = vec![];
    let mut ok = true;
    for (i, l) in a.layers.iter().enumerate() {
        let lp = at.key("layers").index(i);
        let Some(material) = c.opt(&l.material, &[Tag::Material], &lp.key("material")) else {
            ok = false;
            continue;
        };
        layers.push(core::Layer {
            function: match l.function {
                f::LayerFunction::Structure => core::LayerFunction::Structure,
                f::LayerFunction::Substrate => core::LayerFunction::Substrate,
                f::LayerFunction::Insulation => core::LayerFunction::Insulation,
                f::LayerFunction::AirGap => core::LayerFunction::AirGap,
                f::LayerFunction::Membrane => core::LayerFunction::Membrane,
                f::LayerFunction::Finish => core::LayerFunction::Finish,
            },
            material,
            thickness: c.l(l.thickness),
            priority: match l.priority {
                Some(p @ 1..=5) => Some(p as u8),
                Some(p) => {
                    c.err(&lp.key("priority"), format!("must be from 1 to 5, got {p}"));
                    ok = false;
                    None
                }
                None => None,
            },
        });
    }
    if !ok {
        return c.fail(&a.id);
    }
    let asm = core::Assembly {
        usage: match a.usage {
            f::AssemblyUsage::Wall => core::AssemblyUse::Wall,
            f::AssemblyUsage::Floor => core::AssemblyUse::Floor,
            f::AssemblyUsage::Roof => core::AssemblyUse::Roof,
        },
        layers,
        description: a.description.clone().unwrap_or_default(),
    };
    c.insert(
        &a.id,
        Element::new(name_of(&a.name, &a.id), ElementKind::Assembly(asm)),
        at,
    );
}

fn location(l: f::Location, boundary: Option<u32>) -> core::LocationLine {
    match l {
        f::Location::Centre => core::LocationLine::Centre,
        f::Location::CoreCentre => core::LocationLine::CoreCentre,
        f::Location::ExteriorFace => core::LocationLine::ExteriorFace,
        f::Location::InteriorFace => core::LocationLine::InteriorFace,
        f::Location::CoreExterior => core::LocationLine::CoreExterior,
        f::Location::CoreInterior => core::LocationLine::CoreInterior,
        f::Location::LayerBoundary => {
            core::LocationLine::Boundary(boundary.unwrap_or(0).min(255) as u8)
        }
    }
}

fn frame(c: &Ctx, fr: &f::Frame) -> core::OpeningFrame {
    core::OpeningFrame {
        width: c.l(fr.width),
        depth: c.l(fr.depth),
        panes_across: fr.panes_across,
        panes_up: fr.panes_up,
    }
}

/// The total of assembly `id`'s layers, once it is in the model.
fn assembly_total(c: &Ctx, id: Option<ElementId>) -> Option<f64> {
    match c.model().get(id?).map(|e| &e.kind) {
        Some(ElementKind::Assembly(a)) if a.total() > 1e-9 => Some(a.total()),
        _ => None,
    }
}

/// Report the fields given in `v` that its kind doesn't use.
fn check_stray(c: &mut Ctx, v: &impl Spec, allowed: &[&str], kind: &str, at: &Path) -> bool {
    let extra = stray(v, allowed);
    for k in &extra {
        c.err(&at.key(k), format!("`{k}` is not used by {kind}"));
    }
    extra.is_empty()
}

fn element_type(c: &mut Ctx, t: &f::Type, at: &Path) {
    use f::TypeCategory as C;
    const COMMON: &[&str] = &["id", "name", "category", "description", "material"];
    let own: &[&str] = match t.category {
        C::Wall => &["thickness", "assembly", "location", "height", "screen"],
        C::Door | C::Window => &["width", "height", "sill", "frame"],
        C::Slab | C::Roof => &["thickness", "assembly"],
        C::Column => &["width", "depth", "diameter"],
        C::Beam => &["width", "depth"],
    };
    let allowed: Vec<&str> = COMMON.iter().chain(own).copied().collect();
    let kind = format!("a {} type", t.category.key());
    let mut ok = check_stray(c, t, &allowed, &kind, at);
    let Some(material) = c.opt(&t.material, &[Tag::Material], &at.key("material")) else {
        return c.fail(&t.id);
    };
    let use_as = match t.category {
        C::Roof => core::AssemblyUse::Roof,
        C::Slab => core::AssemblyUse::Floor,
        _ => core::AssemblyUse::Wall,
    };
    let _ = use_as;
    let Some(assembly) = c.opt(&t.assembly, &[Tag::Assembly], &at.key("assembly")) else {
        return c.fail(&t.id);
    };
    let mut need = |c: &mut Ctx, v: Option<f64>, key: &str| -> f64 {
        match v {
            Some(v) => c.l(v),
            None => {
                c.err(at, format!("{kind} needs `{key}`"));
                ok = false;
                0.0
            }
        }
    };
    let thick = |c: &mut Ctx, need: &mut dyn FnMut(&mut Ctx, Option<f64>, &str) -> f64| {
        match assembly_total(c, assembly) {
            Some(total) if t.thickness.is_none() => total,
            _ => need(c, t.thickness, "thickness (or an assembly)"),
        }
    };
    let category = match t.category {
        C::Wall => core::TypeCategory::Wall,
        C::Door => core::TypeCategory::Door,
        C::Window => core::TypeCategory::Window,
        C::Slab => core::TypeCategory::Slab,
        C::Roof => core::TypeCategory::Roof,
        C::Column => core::TypeCategory::Column,
        C::Beam => core::TypeCategory::Beam,
    };
    let mut params = types::default_params(category, false);
    match &mut params {
        core::TypeParams::Wall(w) => {
            w.thickness = thick(c, &mut need);
            w.assembly = assembly;
            w.material = material;
            w.location = t
                .location
                .map(|l| location(l, t.boundary))
                .unwrap_or_default();
            w.height = t.height.map(|h| c.l(h));
            w.curtain = None;
            w.screen = t.screen.as_ref().map(|s| {
                Box::new(core::ScreenPattern {
                    unit: c.p(s.unit),
                    holes: s.holes.iter().map(|r| [c.p(r.min), c.p(r.max)]).collect(),
                    frame: c.l(s.frame),
                    stagger: s.stagger,
                })
            });
        }
        core::TypeParams::Door(o) | core::TypeParams::Window(o) => {
            o.width = need(c, t.width, "width");
            o.height = need(c, t.height, "height");
            o.sill = t.sill.map(|s| c.l(s)).unwrap_or(match t.category {
                C::Window => 0.9,
                _ => 0.0,
            });
            o.frame = t.frame.as_ref().map(|fr| frame(c, fr)).unwrap_or_default();
            o.material = material;
        }
        core::TypeParams::Slab(s) | core::TypeParams::Roof(s) => {
            s.thickness = thick(c, &mut need);
            s.assembly = assembly;
            s.material = material;
        }
        core::TypeParams::Column(col) => {
            match profile(c, t.width, t.depth, t.diameter, at) {
                Some(p) => col.profile = p,
                None => ok = false,
            }
            col.material = material;
        }
        core::TypeParams::Beam(b) => {
            b.width = need(c, t.width, "width");
            b.depth = need(c, t.depth, "depth");
            b.material = material;
        }
        _ => unreachable!("the format has no other categories"),
    }
    if !ok {
        return c.fail(&t.id);
    }
    let mut ty = core::ElementType::new(params);
    ty.description = t.description.clone().unwrap_or_default();
    c.insert(
        &t.id,
        Element::new(name_of(&t.name, &t.id), ElementKind::ElementType(ty)),
        at,
    );
}

/// A column profile from its width and depth, or its diameter.
fn profile(
    c: &mut Ctx,
    width: Option<f64>,
    depth: Option<f64>,
    diameter: Option<f64>,
    at: &Path,
) -> Option<core::Profile> {
    match (width, depth, diameter) {
        (Some(w), Some(d), None) => Some(core::Profile::Rectangle {
            width: c.l(w),
            depth: c.l(d),
        }),
        (None, None, Some(d)) => Some(core::Profile::Round { diameter: c.l(d) }),
        (_, _, Some(_)) => {
            c.err(at, "give `width` and `depth`, or `diameter`, not both");
            None
        }
        _ => {
            c.err(
                at,
                "needs `width` and `depth` (a rectangle) or `diameter` (round)",
            );
            None
        }
    }
}

/// Stamp `e` with the type `id`, if one is given. False after a problem.
fn stamp(
    c: &mut Ctx,
    e: &mut Element,
    id: &Option<String>,
    want: f::TypeCategory,
    at: &Path,
) -> bool {
    let Some(tid) = id else { return true };
    let Some(eid) = c.get(tid, &[Tag::Type(want)], &at.key("type")) else {
        return false;
    };
    if !types::stamp(c.model(), e, eid) {
        c.err(
            &at.key("type"),
            format!("\"{tid}\" doesn't fit this element"),
        );
        return false;
    }
    true
}

fn level_height(c: &Ctx, level: ElementId) -> (f64, f64) {
    c.model()
        .level(level)
        .map(|l| (l.elevation, l.height))
        .unwrap_or((0.0, 3.0))
}

fn wall(c: &mut Ctx, w: &f::Wall, at: &Path) {
    let Some(level) = c.get(&w.level, &[Tag::Level], &at.key("level")) else {
        return c.fail(&w.id);
    };
    let refs = (
        c.opt(&w.top_level, &[Tag::Level], &at.key("top_level")),
        c.opt(&w.material, &[Tag::Material], &at.key("material")),
        c.opt(&w.assembly, &[Tag::Assembly], &at.key("assembly")),
    );
    let (Some(top), Some(material), Some(assembly)) = refs else {
        return c.fail(&w.id);
    };
    let mut attach = vec![];
    for (i, a) in w.attach_top.iter().enumerate() {
        match c.get(a, &[Tag::Roof, Tag::Slab], &at.key("attach_top").index(i)) {
            Some(id) => attach.push(id),
            None => return c.fail(&w.id),
        }
    }
    let (elevation, storey) = level_height(c, level);
    let mut wall = core::Wall::new(level, c.p(w.start), c.p(w.end), 0.2, storey);
    wall.base_offset = c.l(w.base_offset);
    let mut e = Element::new(name_of(&w.name, &w.id), ElementKind::Wall(wall));
    if !stamp(c, &mut e, &w.r#type, f::TypeCategory::Wall, at) {
        return c.fail(&w.id);
    }
    let typed = w.r#type.is_some();
    if !typed && w.thickness.is_none() && w.assembly.is_none() {
        c.err(at, "a wall needs `thickness`, an `assembly` or a `type`");
        return c.fail(&w.id);
    }
    let ElementKind::Wall(wall) = &mut e.kind else {
        unreachable!()
    };
    if let Some(t) = w.thickness {
        wall.thickness = c.k * t;
    }
    if w.material.is_some() {
        wall.material = material;
    }
    if let Some(l) = w.location {
        wall.location = location(l, w.boundary);
    }
    wall.flip = w.flip;
    wall.attach_top = attach;
    if let Some(t) = top {
        let top_elevation = c.model().level(t).map(|l| l.elevation).unwrap_or(0.0);
        wall.top = core::WallTop::UpToLevel {
            level: t,
            offset: c.l(w.top_offset),
        };
        wall.height = top_elevation + c.l(w.top_offset) - elevation - wall.base_offset;
    } else if w.top_offset != 0.0 {
        c.err(&at.key("top_offset"), "`top_offset` needs a `top_level`");
    }
    if let Some(h) = w.height {
        if top.is_some() {
            c.err(&at.key("height"), "give `height` or `top_level`, not both");
        }
        wall.height = c.l(h);
    }
    if w.assembly.is_some() {
        core::assembly::set_assembly(c.model(), &mut e, assembly);
    }
    e.mark = w.mark.clone();
    c.insert(&w.id, e, at);
}

fn opening(c: &mut Ctx, o: &f::Opening, at: &Path) {
    let Some(host) = c.get(&o.wall, &[Tag::Wall], &at.key("wall")) else {
        return c.fail(&o.id);
    };
    let Some(material) = c.opt(&o.material, &[Tag::Material], &at.key("material")) else {
        return c.fail(&o.id);
    };
    let Some(ElementKind::Wall(w)) = c.model().get(host).map(|e| e.kind.clone()) else {
        return c.fail(&o.id);
    };
    let kind = match o.kind {
        f::OpeningKind::Door => core::OpeningKind::Door,
        f::OpeningKind::Window => core::OpeningKind::Window,
        f::OpeningKind::Hole => core::OpeningKind::Hole,
    };
    let opening = core::Opening {
        host,
        kind,
        offset: 0.0,
        sill: if kind == core::OpeningKind::Window {
            0.9
        } else {
            0.0
        },
        width: 0.9,
        height: 2.1,
        material: None,
        frame: Default::default(),
        flip_hand: o.flip_hand,
        flip_facing: o.flip_facing,
    };
    let mut e = Element::new(name_of(&o.name, &o.id), ElementKind::Opening(opening));
    let typed = match o.kind {
        f::OpeningKind::Door => stamp(c, &mut e, &o.r#type, f::TypeCategory::Door, at),
        f::OpeningKind::Window => stamp(c, &mut e, &o.r#type, f::TypeCategory::Window, at),
        f::OpeningKind::Hole => {
            if o.r#type.is_some() {
                c.err(&at.key("type"), "a hole has no type");
                false
            } else {
                true
            }
        }
    };
    if !typed {
        return c.fail(&o.id);
    }
    if o.r#type.is_none() {
        for (v, key) in [(o.width, "width"), (o.height, "height")] {
            if v.is_none() {
                c.err(at, format!("an opening needs `{key}` or a `type`"));
                return c.fail(&o.id);
            }
        }
    }
    let ElementKind::Opening(op) = &mut e.kind else {
        unreachable!()
    };
    if let Some(v) = o.width {
        op.width = c.k * v;
    }
    if let Some(v) = o.height {
        op.height = c.k * v;
    }
    if let Some(v) = o.sill {
        op.sill = c.k * v;
    }
    if let Some(fr) = &o.frame {
        op.frame = frame(c, fr);
    }
    if o.material.is_some() {
        op.material = material;
    }
    op.offset = match (o.offset, o.centre) {
        (Some(v), None) => c.k * v,
        (None, Some(m)) => c.k * m - op.width / 2.0,
        (Some(_), Some(_)) => {
            c.err(at, "give `offset` or `centre`, not both");
            return c.fail(&o.id);
        }
        (None, None) => {
            c.err(
                at,
                "an opening needs `offset` (to its near edge) or `centre`",
            );
            return c.fail(&o.id);
        }
    };
    let len = w.length();
    if op.offset < -1e-9 || op.offset + op.width > len + 1e-6 {
        c.err(
            at,
            format!(
                "it runs from {:.3} to {:.3} m along its wall, which is {:.3} m long",
                op.offset,
                op.offset + op.width,
                len
            ),
        );
        return c.fail(&o.id);
    }
    op.offset = op.offset.max(0.0);
    if op.sill + op.height > w.height + 1e-6 {
        c.warnings.push(Problem::new(
            at,
            format!(
                "its top ({:.3} m) is above its wall's ({:.3} m)",
                op.sill + op.height,
                w.height
            ),
        ));
    }
    e.mark = o.mark.clone();
    c.insert(&o.id, e, at);
}

fn slab(c: &mut Ctx, s: &f::Slab, at: &Path) {
    let refs = (
        c.get(&s.level, &[Tag::Level], &at.key("level")),
        c.opt(&s.material, &[Tag::Material], &at.key("material")),
        c.opt(&s.assembly, &[Tag::Assembly], &at.key("assembly")),
    );
    let (Some(level), Some(material), Some(assembly)) = refs else {
        return c.fail(&s.id);
    };
    let mut slab = core::Slab::new(level, c.pts(&s.outline), 0.2);
    slab.bulges = s.bulges.clone();
    slab.holes = s.holes.iter().map(|h| c.pts(h)).collect();
    slab.make_ccw();
    let mut e = Element::new(name_of(&s.name, &s.id), ElementKind::Slab(slab));
    if !stamp(c, &mut e, &s.r#type, f::TypeCategory::Slab, at) {
        return c.fail(&s.id);
    }
    if s.r#type.is_none() && s.thickness.is_none() && s.assembly.is_none() {
        c.err(at, "a slab needs `thickness`, an `assembly` or a `type`");
        return c.fail(&s.id);
    }
    let ElementKind::Slab(slab) = &mut e.kind else {
        unreachable!()
    };
    if let Some(t) = s.thickness {
        slab.thickness = c.k * t;
    }
    slab.top_offset = c.k * s.top_offset;
    if s.material.is_some() {
        slab.material = material;
    }
    if s.assembly.is_some() {
        core::assembly::set_assembly(c.model(), &mut e, assembly);
    }
    c.insert(&s.id, e, at);
}

fn roof(c: &mut Ctx, r: &f::Roof, at: &Path) {
    let refs = (
        c.get(&r.level, &[Tag::Level], &at.key("level")),
        c.opt(&r.material, &[Tag::Material], &at.key("material")),
        c.opt(&r.assembly, &[Tag::Assembly], &at.key("assembly")),
    );
    let (Some(level), Some(material), Some(assembly)) = refs else {
        return c.fail(&r.id);
    };
    let mut boundary = c.pts(&r.outline);
    let n = boundary.len();
    let mut edge = r.edge as usize;
    if n >= 3 && edge >= n {
        c.err(
            &at.key("edge"),
            format!("the outline has {n} edges, numbered from 0"),
        );
        return c.fail(&r.id);
    }
    // kept counter-clockwise; the edge keeps naming the same side
    if n >= 3 && core::polygon::signed_area2(&boundary) < 0.0 {
        boundary.reverse();
        edge = (2 * n - 2 - edge) % n;
    }
    let shape = match r.shape {
        f::RoofShape::Flat => core::RoofShape::Flat,
        f::RoofShape::Shed => core::RoofShape::Shed,
        f::RoofShape::Gable => core::RoofShape::Gable,
        f::RoofShape::Hip => core::RoofShape::Hip,
    };
    let pitch = match (r.pitch, shape) {
        (Some(p), _) => p,
        (None, core::RoofShape::Flat) => 0.0,
        (None, _) => {
            c.err(at, format!("a {} roof needs `pitch`", r.shape.key()));
            return c.fail(&r.id);
        }
    };
    let roof = core::Roof {
        layer_edges: vec![],
        level,
        boundary,
        shape,
        pitch,
        overhang: c.l(r.overhang),
        thickness: r.thickness.map(|t| c.l(t)).unwrap_or(0.2),
        base_offset: c.l(r.base_offset),
        edge,
        material: None,
        assembly: None,
        holes: r.holes.iter().map(|h| c.pts(h)).collect(),
    };
    let mut e = Element::new(name_of(&r.name, &r.id), ElementKind::Roof(roof));
    if !stamp(c, &mut e, &r.r#type, f::TypeCategory::Roof, at) {
        return c.fail(&r.id);
    }
    if r.r#type.is_none() && r.thickness.is_none() && r.assembly.is_none() {
        c.err(at, "a roof needs `thickness`, an `assembly` or a `type`");
        return c.fail(&r.id);
    }
    let ElementKind::Roof(roof) = &mut e.kind else {
        unreachable!()
    };
    if let Some(t) = r.thickness {
        roof.thickness = c.k * t;
    }
    if r.material.is_some() {
        roof.material = material;
    }
    if r.assembly.is_some() {
        core::assembly::set_assembly(c.model(), &mut e, assembly);
    }
    c.insert(&r.id, e, at);
}

fn column(c: &mut Ctx, col: &f::Column, at: &Path) {
    let refs = (
        c.get(&col.level, &[Tag::Level], &at.key("level")),
        c.opt(&col.top_level, &[Tag::Level], &at.key("top_level")),
        c.opt(&col.material, &[Tag::Material], &at.key("material")),
    );
    let (Some(level), Some(top), Some(material)) = refs else {
        return c.fail(&col.id);
    };
    let column = core::Column {
        level,
        position: c.p(col.at),
        profile: core::Profile::Rectangle {
            width: 0.3,
            depth: 0.3,
        },
        rotation: col.rotation,
        base_offset: c.l(col.base_offset),
        height: col.height.map(|h| c.l(h)),
        material: None,
        section: None,
        top: match top {
            Some(t) => core::WallTop::UpToLevel {
                level: t,
                offset: c.l(col.top_offset),
            },
            None => core::WallTop::Unconnected,
        },
    };
    let mut e = Element::new(name_of(&col.name, &col.id), ElementKind::Column(column));
    if !stamp(c, &mut e, &col.r#type, f::TypeCategory::Column, at) {
        return c.fail(&col.id);
    }
    let given = col.width.is_some() || col.depth.is_some() || col.diameter.is_some();
    let profile = if given || col.r#type.is_none() {
        match profile(c, col.width, col.depth, col.diameter, at) {
            Some(p) => Some(p),
            None => return c.fail(&col.id),
        }
    } else {
        None
    };
    let ElementKind::Column(column) = &mut e.kind else {
        unreachable!()
    };
    if let Some(p) = profile {
        column.profile = p;
    }
    if col.material.is_some() {
        column.material = material;
    }
    e.mark = col.mark.clone();
    c.insert(&col.id, e, at);
}

fn beam(c: &mut Ctx, b: &f::Beam, at: &Path) {
    let refs = (
        c.get(&b.level, &[Tag::Level], &at.key("level")),
        c.opt(&b.material, &[Tag::Material], &at.key("material")),
    );
    let (Some(level), Some(material)) = refs else {
        return c.fail(&b.id);
    };
    let beam = core::Beam {
        level,
        start: c.p(b.start),
        end: c.p(b.end),
        width: 0.2,
        depth: 0.4,
        top_offset: c.l(b.top_offset),
        material: None,
        section: None,
        rise: [0.0, 0.0],
        up: None,
        extend: [0.0, 0.0],
        cuts: [None, None],
    };
    let mut e = Element::new(name_of(&b.name, &b.id), ElementKind::Beam(beam));
    if !stamp(c, &mut e, &b.r#type, f::TypeCategory::Beam, at) {
        return c.fail(&b.id);
    }
    if b.r#type.is_none() && (b.width.is_none() || b.depth.is_none()) {
        c.err(at, "a beam needs `width` and `depth`, or a `type`");
        return c.fail(&b.id);
    }
    let ElementKind::Beam(beam) = &mut e.kind else {
        unreachable!()
    };
    if let Some(v) = b.width {
        beam.width = c.k * v;
    }
    if let Some(v) = b.depth {
        beam.depth = c.k * v;
    }
    if b.material.is_some() {
        beam.material = material;
    }
    e.mark = b.mark.clone();
    c.insert(&b.id, e, at);
}

fn stair(c: &mut Ctx, s: &f::Stair, at: &Path) {
    let refs = (
        c.get(&s.level, &[Tag::Level], &at.key("level")),
        c.opt(&s.material, &[Tag::Material], &at.key("material")),
    );
    let (Some(level), Some(material)) = refs else {
        return c.fail(&s.id);
    };
    let stair = core::Stair {
        level,
        start: c.p(s.start),
        direction: s.direction,
        width: c.l(s.width),
        max_riser: c.l(s.max_riser),
        tread: c.l(s.tread),
        height: s.height.map(|h| c.l(h)),
        base_offset: c.l(s.base_offset),
        waist: c.l(s.waist),
        landing: s.landing.as_ref().map(|l| core::Landing {
            after: l.after,
            left: l.turn == f::Side::Left,
        }),
        material,
    };
    c.insert(
        &s.id,
        Element::new(name_of(&s.name, &s.id), ElementKind::Stair(stair)),
        at,
    );
}

fn railing(c: &mut Ctx, r: &f::Railing, at: &Path) {
    let refs = (
        c.get(&r.level, &[Tag::Level], &at.key("level")),
        c.opt(&r.stair, &[Tag::Stair], &at.key("stair")),
        c.opt(&r.material, &[Tag::Material], &at.key("material")),
    );
    let (Some(level), Some(stair), Some(material)) = refs else {
        return c.fail(&r.id);
    };
    let stair = match (stair, r.side) {
        (Some(s), Some(side)) => Some(core::StairSide {
            stair: s,
            left: side == f::Side::Left,
        }),
        (Some(_), None) => {
            c.err(
                at,
                "a railing on a stair needs `side`: \"left\" or \"right\"",
            );
            return c.fail(&r.id);
        }
        (None, Some(_)) => {
            c.err(&at.key("side"), "`side` needs a `stair`");
            return c.fail(&r.id);
        }
        (None, None) => {
            if r.path.len() < 2 {
                c.err(
                    at,
                    "a railing needs a `path` of at least 2 points, or a `stair`",
                );
                return c.fail(&r.id);
            }
            None
        }
    };
    let railing = core::Railing {
        level,
        path: r.path.iter().map(|p| c.p3(*p)).collect(),
        height: c.l(r.height),
        post_spacing: c.l(r.post_spacing),
        stair,
        material,
        rail_profile: None,
    };
    c.insert(
        &r.id,
        Element::new(name_of(&r.name, &r.id), ElementKind::Railing(railing)),
        at,
    );
}

fn part(c: &mut Ctx, p: &f::Part, at: &Path) {
    let refs = (
        c.get(&p.level, &[Tag::Level], &at.key("level")),
        c.opt(&p.wall, &[Tag::Wall], &at.key("wall")),
        c.opt(&p.material, &[Tag::Material], &at.key("material")),
    );
    let (Some(level), Some(wall), Some(material)) = refs else {
        return c.fail(&p.id);
    };
    const COMMON: &[&str] = &["id", "name", "level", "void", "material", "ifc_class"];
    let extrusion = !p.outline.is_empty();
    let own: &[&str] = if extrusion {
        &["outline", "holes", "base", "height", "plane"]
    } else if p.wall.is_some() {
        &["profile", "wall", "edge", "offset", "extend"]
    } else if p.arc.is_some() {
        &["profile", "arc"]
    } else {
        &["profile", "path", "closed", "face_normal"]
    };
    let allowed: Vec<&str> = COMMON.iter().chain(own).copied().collect();
    let kind = if extrusion { "an extrusion" } else { "a sweep" };
    if !check_stray(c, p, &allowed, kind, at) {
        return c.fail(&p.id);
    }
    let mut part = if extrusion {
        let Some(h) = p.height else {
            c.err(at, "an extrusion needs `height`");
            return c.fail(&p.id);
        };
        let mut part = parts::Part::extrusion(level, c.pts(&p.outline), c.l(p.base), c.l(h));
        if let parts::Form::Extrusion { holes, plane, .. } = &mut part.form {
            *holes = p.holes.iter().map(|h| c.pts(h)).collect();
            if let Some(pl) = &p.plane {
                let (x, y) = (pl.x.normalize_or_zero(), pl.y.normalize_or_zero());
                if x == glam::DVec3::ZERO || y == glam::DVec3::ZERO || x.dot(y).abs() > 1e-6 {
                    c.err(&at.key("plane"), "a plane needs two square axes");
                    return c.fail(&p.id);
                }
                *plane = Some(creabim_core::Workplane {
                    origin: c.p3(pl.origin),
                    x,
                    y,
                });
            }
        }
        part
    } else if p.profile.is_empty() {
        c.err(
            at,
            "a part needs an `outline` (an extrusion) or a `profile` (a sweep)",
        );
        return c.fail(&p.id);
    } else {
        let path = match wall {
            Some(w) => parts::Path::Wall {
                wall: w,
                edge: match p.edge {
                    f::WallEdge::Top => parts::WallEdge::Top,
                    f::WallEdge::Base => parts::WallEdge::Base,
                },
                offset: c.l(p.offset),
                extend: p.extend.map(|x| c.p(x).to_array()).unwrap_or([0.0; 2]),
            },
            None if p.arc.is_some() => {
                let a = p.arc.as_ref().unwrap();
                let at = at.key("arc");
                let axis = a.axis.try_normalize();
                if axis.is_none() {
                    c.err(&at.key("axis"), "a turn axis needs a direction");
                    return c.fail(&p.id);
                }
                let center = c.p3(a.center);
                let start = c.p3(a.start);
                let axis = axis.unwrap();
                let out = start - center;
                let radial = out - axis * out.dot(axis);
                if radial.length() < 1e-9 {
                    c.err(&at.key("start"), "an arc's start must be off its axis");
                    return c.fail(&p.id);
                }
                if !(a.sweep.abs() > 1e-9 && a.sweep.abs() <= 360.0 + 1e-9) {
                    c.err(
                        &at.key("sweep"),
                        "an arc's sweep must be within a full turn (360)",
                    );
                    return c.fail(&p.id);
                }
                parts::Path::Arc {
                    center,
                    axis,
                    start,
                    sweep: a.sweep.to_radians(),
                }
            }
            None => {
                if p.path.len() < 2 {
                    c.err(
                        at,
                        "a sweep needs a `path` of at least 2 points, an `arc`, or a `wall`",
                    );
                    return c.fail(&p.id);
                }
                let normal = p.face_normal.and_then(|n| n.try_normalize());
                if p.face_normal.is_some() && normal.is_none() {
                    c.err(&at.key("face_normal"), "a face normal needs a direction");
                    return c.fail(&p.id);
                }
                parts::Path::Points {
                    points: p.path.iter().map(|x| c.p3(*x)).collect(),
                    closed: p.closed,
                    normal,
                }
            }
        };
        parts::Part::sweep(level, c.pts(&p.profile), path)
    };
    part.void = p.void;
    part.material = material;
    if let Some(class) = &p.ifc_class {
        part.ifc_class = class.clone();
    }
    let name = p.name.clone().unwrap_or_else(|| p.id.clone());
    c.insert(&p.id, Element::new(name, ElementKind::Part(part)), at);
}

fn site_element(c: &mut Ctx, s: &f::SiteElement, at: &Path) {
    let refs = (
        c.get(&s.level, &[Tag::Level], &at.key("level")),
        c.opt(&s.material, &[Tag::Material], &at.key("material")),
    );
    let (Some(level), Some(material)) = refs else {
        return c.fail(&s.id);
    };
    const COMMON: &[&str] = &["id", "name", "kind", "level", "material"];
    let own: &[&str] = match s.kind {
        f::SiteKind::Terrain => &["outline", "top", "thickness", "points", "contour"],
        f::SiteKind::Area => &["outline", "holes", "top", "thickness", "pattern"],
        f::SiteKind::Plant => &["at", "base", "plant", "canopy", "height"],
    };
    let allowed: Vec<&str> = COMMON.iter().chain(own).copied().collect();
    let kind = format!("a site {}", s.kind.key());
    if !check_stray(c, s, &allowed, &kind, at) {
        return c.fail(&s.id);
    }
    let missing = |c: &mut Ctx, v: Option<f64>, key: &str| -> Option<f64> {
        if v.is_none() {
            c.err(at, format!("{kind} needs `{key}`"));
        }
        v.map(|v| c.l(v))
    };
    let form = match s.kind {
        f::SiteKind::Terrain | f::SiteKind::Area => {
            let Some(thickness) = missing(c, s.thickness, "thickness") else {
                return c.fail(&s.id);
            };
            let mut boundary = c.pts(&s.outline);
            if core::polygon::signed_area2(&boundary) < 0.0 {
                boundary.reverse();
            }
            if s.kind == f::SiteKind::Terrain {
                site::SiteForm::Terrain {
                    boundary,
                    top: c.l(s.top),
                    thickness,
                    points: s.points.iter().map(|p| c.p3(*p)).collect(),
                    contour: c.l(s.contour),
                }
            } else {
                let pattern = match &s.pattern {
                    None => site::Pattern::None,
                    Some(p) => match pattern(c, p, &at.key("pattern")) {
                        Some(p) => p,
                        None => return c.fail(&s.id),
                    },
                };
                site::SiteForm::Area {
                    boundary,
                    holes: s.holes.iter().map(|h| c.pts(h)).collect(),
                    top: c.l(s.top),
                    thickness,
                    pattern,
                }
            }
        }
        f::SiteKind::Plant => {
            let (Some(position), Some(canopy), Some(height)) = (
                s.at,
                missing(c, s.canopy, "canopy"),
                missing(c, s.height, "height"),
            ) else {
                if s.at.is_none() {
                    c.err(at, "a plant needs `at`");
                }
                return c.fail(&s.id);
            };
            site::SiteForm::Plant {
                position: c.p(position),
                base: c.l(s.base),
                form: match s.plant {
                    f::PlantForm::Tree => core::PlantForm::Tree,
                    f::PlantForm::Conifer => core::PlantForm::Conifer,
                    f::PlantForm::Palm => core::PlantForm::Palm,
                    f::PlantForm::Shrub => core::PlantForm::Shrub,
                },
                canopy,
                height,
            }
        }
    };
    let el = core::Site {
        level,
        form,
        material,
    };
    let name = s.name.clone().unwrap_or_else(|| s.id.clone());
    c.insert(&s.id, Element::new(name, ElementKind::Site(el)), at);
}

fn pattern(c: &mut Ctx, p: &f::SitePattern, at: &Path) -> Option<site::Pattern> {
    let own: &[&str] = match p.kind {
        f::PatternKind::None => &["kind"],
        f::PatternKind::Units => &["kind", "size", "stagger", "angle"],
        f::PatternKind::Lines => &["kind", "spacing", "angle"],
        f::PatternKind::Stipple => &["kind", "spacing"],
    };
    let kind = format!("a {} pattern", p.kind.key());
    if !check_stray(c, p, own, &kind, at) {
        return None;
    }
    let spacing = |c: &mut Ctx| match p.spacing {
        Some(s) => Some(c.l(s)),
        None => {
            c.err(at, format!("{kind} needs `spacing`"));
            None
        }
    };
    Some(match p.kind {
        f::PatternKind::None => site::Pattern::None,
        f::PatternKind::Units => match p.size {
            Some(size) => site::Pattern::Units {
                size: c.p(size),
                stagger: p.stagger,
                angle: p.angle,
            },
            None => {
                c.err(at, "a units pattern needs `size`");
                return None;
            }
        },
        f::PatternKind::Lines => site::Pattern::Lines {
            spacing: spacing(c)?,
            angle: p.angle,
        },
        f::PatternKind::Stipple => site::Pattern::Stipple {
            spacing: spacing(c)?,
        },
    })
}

fn object(c: &mut Ctx, o: &f::Object, catalogue: &[CatalogueObject], at: &Path) {
    let Some(level) = c.get(&o.level, &[Tag::Level], &at.key("level")) else {
        return c.fail(&o.id);
    };
    let found = catalogue.iter().find(|x| x.key == o.catalogue).or_else(|| {
        catalogue
            .iter()
            .find(|x| x.key == format!("starter/{}", o.catalogue))
    });
    let Some(cat) = found else {
        let keys: Vec<&str> = catalogue.iter().map(|x| x.key.as_str()).collect();
        let short: Vec<&str> = keys
            .iter()
            .map(|k| k.trim_start_matches("starter/"))
            .collect();
        let hint = suggest(o.catalogue.trim_start_matches("starter/"), &short)
            .map(|s| format!("; did you mean \"starter/{s}\"?"))
            .unwrap_or_else(|| "; `creabim catalogue` lists them".into());
        c.err(
            &at.key("catalogue"),
            format!("no catalogue object \"{}\"{hint}", o.catalogue),
        );
        return c.fail(&o.id);
    };
    let at_plan = c.p(o.at);
    let position = match o.anchor {
        f::Anchor::Origin => at_plan,
        f::Anchor::Centre => core::objects::centred(&cat.object, at_plan, o.rotation),
    };
    let (mut tx, _) = core::objects::place(c.model(), cat, level, position, o.rotation);
    if !c.owns(&o.id, at) {
        return;
    }
    let eid = c.eid(&o.id);
    let k = c.k;
    for op in &mut tx.ops {
        if let core::Op::Insert(e) = op {
            if let ElementKind::Object(obj) = &mut e.kind {
                e.id = eid;
                if let Some(n) = &o.name {
                    e.name = n.clone();
                }
                e.mark = o.mark.clone();
                if let Some(off) = o.offset {
                    obj.offset = off * k;
                }
            }
        }
    }
    c.commit(&o.id, tx, at);
}

fn room(c: &mut Ctx, r: &f::Room, at: &Path) {
    let Some(level) = c.get(&r.level, &[Tag::Level], &at.key("level")) else {
        return c.fail(&r.id);
    };
    let mut boundary = c.pts(&r.outline);
    if boundary.len() >= 3 && core::polygon::signed_area2(&boundary) < 0.0 {
        boundary.reverse();
    }
    let room = core::Room {
        level,
        at: c.p(r.at),
        boundary,
        number: r.number.clone(),
        floor_finish: r.floor_finish.clone(),
        wall_finish: r.wall_finish.clone(),
        ceiling_finish: r.ceiling_finish.clone(),
        occupancy: r.occupancy.clone(),
        department: r.department.clone(),
        height: r.height.map(|h| c.l(h)),
    };
    c.insert(
        &r.id,
        Element::new(name_of(&r.name, &r.id), ElementKind::Room(room)),
        at,
    );
}

fn view(c: &mut Ctx, v: &f::View, at: &Path) {
    use f::ViewKind as K;
    const COMMON: &[&str] = &["id", "name", "kind", "scale", "style", "callout"];
    let own: &[&str] = match v.kind {
        K::Plan => &["level", "cut_height", "ceiling_plan", "room_colours"],
        K::Section => &["start", "end", "depth", "room_colours"],
        K::Elevation => &["start", "end", "depth"],
        K::ThreeD => &["azimuth", "altitude"],
        K::Schedule => &["category", "by_type"],
    };
    let allowed: Vec<&str> = COMMON.iter().chain(own).copied().collect();
    let article = if v.kind == K::Elevation { "an" } else { "a" };
    let kind = format!("{article} {} view", v.kind.key());
    if !check_stray(c, v, &allowed, &kind, at) {
        return c.fail(&v.id);
    }
    let mut view = match v.kind {
        K::Plan => {
            let Some(l) = &v.level else {
                c.err(at, "a plan needs `level`");
                return c.fail(&v.id);
            };
            let Some(level) = c.get(l, &[Tag::Level], &at.key("level")) else {
                return c.fail(&v.id);
            };
            let mut view = core::View::plan(level);
            view.kind = core::ViewKind::Plan {
                level,
                cut_height: c.l(v.cut_height),
            };
            view.ceiling_plan = v.ceiling_plan;
            view
        }
        K::Section | K::Elevation => {
            let (Some(start), Some(end), Some(depth)) = (v.start, v.end, v.depth) else {
                c.err(at, format!("{kind} needs `start`, `end` and `depth`"));
                return c.fail(&v.id);
            };
            core::View::section(c.p(start), c.p(end), c.l(depth), v.kind == K::Section)
        }
        K::ThreeD => core::View::axonometric(v.azimuth, v.altitude),
        K::Schedule => {
            let Some(cat) = v.category else {
                c.err(at, "a schedule needs `category`");
                return c.fail(&v.id);
            };
            let category = match cat {
                f::ScheduleCategory::Doors => core::ScheduleCategory::Doors,
                f::ScheduleCategory::Windows => core::ScheduleCategory::Windows,
                f::ScheduleCategory::Walls => core::ScheduleCategory::Walls,
                f::ScheduleCategory::Slabs => core::ScheduleCategory::Slabs,
                f::ScheduleCategory::Structure => core::ScheduleCategory::Structure,
                f::ScheduleCategory::Furniture => core::ScheduleCategory::Furniture,
                f::ScheduleCategory::Fixtures => core::ScheduleCategory::Fixtures,
                f::ScheduleCategory::Rooms => core::ScheduleCategory::Rooms,
                f::ScheduleCategory::Materials => core::ScheduleCategory::Materials,
            };
            let mut view = core::View::schedule(category);
            view.kind = core::ViewKind::Schedule {
                category,
                by_type: v.by_type,
            };
            view
        }
    };
    if v.kind != K::Schedule {
        view.scale = v.scale;
    }
    if let Some(s) = v.style {
        view.style = match s {
            f::ViewStyle::HiddenLine => core::ViewStyle::HiddenLine,
            f::ViewStyle::Shaded => core::ViewStyle::Shaded,
            f::ViewStyle::Wireframe => core::ViewStyle::Wireframe,
        };
    }
    view.room_colours = v.room_colours;
    if let Some(co) = &v.callout {
        let Some(parent) = c.get(&co.parent, &[Tag::View], &at.key("callout").key("parent")) else {
            return c.fail(&v.id);
        };
        view.callout = Some(core::element::Callout::new(
            parent,
            c.p(co.min),
            c.p(co.max),
        ));
    }
    c.insert(
        &v.id,
        Element::new(name_of(&v.name, &v.id), ElementKind::View(view)),
        at,
    );
}

fn sheet(c: &mut Ctx, s: &f::Sheet, at: &Path) {
    if !c.owns(&s.id, at) {
        return;
    }
    let mut sheet = match (s.size, s.width, s.height) {
        (Some(size), None, None) => core::Sheet::a(s.id.clone(), size.key()),
        (None, Some(w), Some(h)) => {
            let mut sheet = core::Sheet::a(s.id.clone(), "A3");
            sheet.width = w;
            sheet.height = h;
            sheet
        }
        (None, None, None) => {
            c.err(
                at,
                "a sheet needs `size` (\"A1\"...) or `width` and `height` in mm",
            );
            return c.fail(&s.id);
        }
        _ => {
            c.err(
                at,
                "give a sheet `size`, or `width` and `height`, not some of each",
            );
            return c.fail(&s.id);
        }
    };
    sheet.fields = s.fields.clone();
    let sheet_id = c.eid(&s.id);
    c.insert(
        &s.id,
        Element::new(name_of(&s.name, &s.id), ElementKind::Sheet(sheet)),
        at,
    );
    if c.broken.contains(&s.id) {
        return;
    }
    for (i, vp) in s.viewports.iter().enumerate() {
        let vat = at.key("viewports").index(i);
        let Some(view) = c.get(&vp.view, &[Tag::View], &vat.key("view")) else {
            continue;
        };
        let target = match (vp.target, &vp.crop) {
            (Some(t), _) => c.p(t),
            (None, Some(r)) => c.p((r.min + r.max) / 2.0),
            (None, None) => creabim_geom::section::view_drawing(c.model(), view)
                .and_then(|d| d.bounds())
                .map(|(lo, hi)| (lo + hi) / 2.0)
                .unwrap_or(DVec2::ZERO),
        };
        let viewport = core::Viewport {
            sheet: sheet_id,
            view,
            center: vp.at,
            target,
            crop: vp.crop.as_ref().map(|r| [c.p(r.min), c.p(r.max)]),
        };
        c.commit(
            &s.id,
            Transaction::new("Build")
                .insert(Element::new("Viewport", ElementKind::Viewport(viewport))),
            &vat,
        );
    }
}
