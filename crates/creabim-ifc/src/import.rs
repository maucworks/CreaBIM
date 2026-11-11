//! IFC STEP import (IFC2X3, IFC4, IFC4X3).
//!
//! Storeys become levels. Walls, slabs, doors and windows that follow the
//! simple patterns CreaBIM models natively (a straight wall extruded from a
//! footprint between two parallel faces, a horizontal slab whose top sits on
//! a storey, a door or window filling an opening in such a wall) become
//! native elements, so they stay editable as walls and slabs. Everything
//! else with a body becomes a free mesh that keeps its IFC class, so nothing
//! visible is lost. Element ids are decoded from GlobalIds, so a model that
//! round-trips through IFC keeps its identity.

use crate::classes::product_class;
use crate::shape::RevolvedSweep;
use crate::shape::{Geo, Warnings};
use crate::step_read::{parse, Entity, StepError, StepFile, Value};
use crate::tess::{area2, bounds, merge_coplanar, weld};
use creabim_core::{
    parts::Path, Document, EditMesh, Element, ElementId, ElementKind, FreeMesh, Grid, Hatch, Level,
    LocationLine, Material, Model, Opening, OpeningKind, Part, Slab, Transaction, Wall,
};
use glam::{DMat4, DVec2, DVec3};
use std::collections::{HashMap, HashSet};

mod brep;
mod members;
mod objects;
mod spaces;

pub(crate) use members::MEMBER_PSET;

/// Classes (as STEP writes them) the import may make native elements of.
pub(crate) fn could_be_native(class: &str) -> bool {
    [
        "IFCWALL",
        "IFCSLAB",
        "IFCCOLUMN",
        "IFCBEAM",
        "IFCDOOR",
        "IFCWINDOW",
    ]
    .iter()
    .any(|c| class.to_ascii_uppercase().starts_with(c))
}

/// A model read from a file, with what could not be brought across.
#[derive(Debug)]
pub struct ImportResult {
    pub model: Model,
    /// Human-readable notes about anything skipped or approximated,
    /// grouped by kind with a count.
    pub warnings: Vec<String>,
    /// The same notes element by element, each naming the element's
    /// class, name and GlobalId and saying why, e.g. "IfcWall 'Wall-Ext'
    /// (2XcU...) is a brep whose top is sloped; imported as a mesh".
    pub details: Vec<String>,
    /// The schema named in the file, e.g. `IFC4` or `ifcx_alpha`.
    pub schema: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error(transparent)]
    Step(#[from] StepError),
    #[error("not an IFC file (schema {0:?}, no IfcProject)")]
    NotIfc(String),
    #[error("invalid IFCX file: {0}")]
    Ifcx(String),
    #[error("imported elements are inconsistent: {0}")]
    Model(String),
}

/// Read an IFC STEP file into a CreaBIM model.
pub fn import_ifc(text: &str) -> Result<ImportResult, ImportError> {
    let file = parse(text)?;
    let schema = file.schemas.first().cloned().unwrap_or_default();
    let by_class = file.by_class();
    let has_project = by_class.contains_key("IFCPROJECT");
    if !has_project && !schema.to_ascii_uppercase().starts_with("IFC") {
        return Err(ImportError::NotIfc(schema));
    }
    let mut imp = Importer::new(&file, &by_class);
    if !file.errors.is_empty() {
        imp.g.warnings.add(format!(
            "{} damaged records were skipped; first: {}",
            file.errors.len(),
            file.errors[0]
        ));
    }
    imp.run();
    let mut elements = std::mem::take(&mut imp.out);
    read_tags(&file, &mut elements);
    let mut warnings = std::mem::take(&mut imp.g.warnings);
    let model = build_model(elements, &mut warnings)?;
    let (warnings, details) = warnings.into_parts();
    Ok(ImportResult {
        model,
        warnings,
        details,
        schema,
    })
}

/// Read only the materials of an IFC STEP file: every IfcMaterial, with
/// its category and description, and its colour, transparency and (when
/// given) roughness from its IfcSurfaceStyle, or from the style on the body
/// of something made of it. Names are kept as they are, so importing again
/// updates the same materials.
pub fn import_ifc_materials(text: &str) -> Result<Vec<(String, Material)>, ImportError> {
    let file = parse(text)?;
    let schema = file.schemas.first().cloned().unwrap_or_default();
    let by_class = file.by_class();
    if !by_class.contains_key("IFCPROJECT") && !schema.to_ascii_uppercase().starts_with("IFC") {
        return Err(ImportError::NotIfc(schema));
    }
    let mut imp = Importer::new(&file, &by_class);
    imp.index_relationships();
    Ok(imp.all_materials())
}

/// Insert elements into a fresh model, dropping any that fail validation.
/// Elements must come in dependency order.
pub(crate) fn build_model(
    elements: Vec<Element>,
    warnings: &mut Warnings,
) -> Result<Model, ImportError> {
    let mut tx = Transaction::new("Import");
    let mut ids: HashSet<ElementId> = HashSet::new();
    for e in elements {
        let label = || format!("{} '{}' ({})", e.ifc_class(), e.name, e.id.ifc_guid());
        if let Err(msg) = e.validate() {
            warnings.element(
                "elements that failed validation were skipped",
                format!("{} was skipped: {msg}", label()),
            );
            continue;
        }
        if let Some(missing) = e.references().into_iter().find(|r| !ids.contains(r)) {
            warnings.element(
                "elements depending on something not imported were skipped",
                format!(
                    "{} was skipped: it depends on {missing}, which was not imported",
                    label()
                ),
            );
            continue;
        }
        if !ids.insert(e.id) {
            warnings.add(format!("duplicate id for {:?}", e.name));
            continue;
        }
        tx = tx.insert(e);
    }
    let mut doc = Document::new();
    doc.commit(tx)
        .map_err(|e| ImportError::Model(e.to_string()))?;
    Ok(doc.model().clone())
}

/// Give elements the marks their IFC entities carry as their Tag (the
/// attribute after Representation, on every IfcElement). Tags of bare
/// digits are left out: some programs write their own element numbers there.
fn read_tags(f: &StepFile, elements: &mut [Element]) {
    const SPATIAL: &[&str] = &[
        "IFCPROJECT",
        "IFCSITE",
        "IFCBUILDING",
        "IFCBUILDINGSTOREY",
        "IFCSPACE",
        "IFCSPATIALZONE",
        "IFCEXTERNALSPATIALELEMENT",
    ];
    let mut tags: HashMap<ElementId, String> = HashMap::new();
    for e in f.entities.values() {
        if SPATIAL.contains(&e.class.as_str()) || crate::classes::product_class(&e.class).is_none()
        {
            continue;
        }
        let Some(tag) = e.arg(7).as_str().map(str::trim) else {
            continue;
        };
        if tag.is_empty() || tag.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Some(id) = e.arg(0).as_str().and_then(ElementId::from_ifc_guid) {
            tags.insert(id, tag.to_string());
        }
    }
    for e in elements {
        if let Some(t) = tags.remove(&e.id) {
            e.mark = Some(t);
        }
    }
}

/// Classes with no physical body of their own, or handled separately.
const SKIPPED: &[&str] = &[
    "IfcProject",
    "IfcBuilding",
    "IfcBuildingStorey",
    "IfcSpace",
    "IfcSpatialZone",
    "IfcExternalSpatialElement",
    "IfcFacility",
    "IfcFacilityPart",
    "IfcFacilityPartCommon",
    "IfcBridge",
    "IfcBridgePart",
    "IfcRoad",
    "IfcRoadPart",
    "IfcRailway",
    "IfcRailwayPart",
    "IfcMarineFacility",
    "IfcMarinePart",
    "IfcOpeningElement",
    "IfcOpeningStandardCase",
    "IfcVoidingFeature",
    "IfcAnnotation",
    "IfcGrid",
    "IfcVirtualElement",
    "IfcPort",
    "IfcDistributionPort",
    "IfcAlignment",
    "IfcAlignmentCant",
    "IfcAlignmentHorizontal",
    "IfcAlignmentSegment",
    "IfcAlignmentVertical",
    "IfcReferent",
    "IfcLinearPositioningElement",
    "IfcPositioningElement",
    "IfcLinearElement",
    "IfcProxy",
];

/// A wall that became native, for hosting its doors and windows.
#[derive(Clone, Copy)]
pub(crate) struct HostWall {
    pub id: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
    pub base_z: f64,
    pub top_z: f64,
}

struct Storey {
    id: ElementId,
    elevation: f64,
}

/// A native wall and the holes cut through its body.
type WallPart = (Wall, Vec<brep::Void>);

/// A vertical prism: a plan outline between two heights, in world coordinates.
pub(crate) struct Prism {
    pub footprint: Vec<DVec2>,
    pub z0: f64,
    pub z1: f64,
    /// Holes through it, counter-clockwise.
    pub holes: Vec<Vec<DVec2>>,
    pub clipped: bool,
}

struct Importer<'a> {
    f: &'a StepFile,
    by_class: &'a HashMap<&'a str, Vec<u64>>,
    g: Geo<'a>,
    out: Vec<Element>,
    used_ids: HashSet<ElementId>,
    storeys: HashMap<u64, Storey>,
    /// Levels by elevation, for elements not placed on a storey.
    levels: Vec<(f64, ElementId)>,
    contained: HashMap<u64, u64>,
    parent: HashMap<u64, u64>,
    voids: HashMap<u64, u64>,
    fills: HashMap<u64, u64>,
    /// Hosts with openings, openings with fillings, elements with parts.
    voided: HashSet<u64>,
    filled: HashSet<u64>,
    has_parts: HashSet<u64>,
    materials_of: HashMap<u64, u64>,
    type_of: HashMap<u64, u64>,
    /// Property sets given to each object.
    psets: HashMap<u64, Vec<u64>>,
    /// Imported element types, by the IFC type entity they came from.
    types: HashMap<u64, ElementId>,
    /// Which of those are object types (furniture, fixtures).
    object_types: HashSet<u64>,
    type_elems: Vec<Element>,
    /// Native slabs, by IFC instance: where in `wall_elems` they are.
    slabs: HashMap<u64, usize>,
    styles: HashMap<u64, Surface>,
    material_colors: HashMap<u64, Surface>,
    /// Imported materials by name.
    materials: HashMap<String, ElementId>,
    /// Imported assemblies, by the IfcMaterialLayerSet they came from.
    layer_sets: HashMap<u64, ElementId>,
    assembly_elems: Vec<Element>,
    /// The native walls each IFC wall became (several for a wall along a
    /// polyline axis).
    walls: HashMap<u64, Vec<HostWall>>,
    /// Holes cut through native walls' bodies, by wall, and whether a
    /// door, window or opening of the file fills each.
    wall_voids: HashMap<ElementId, Vec<(brep::Void, bool)>>,
    /// Why an element that could have been native stayed a mesh.
    mesh_reason: HashMap<u64, String>,
    /// Elements written so far, by IFC instance, so nothing is imported twice.
    done: HashSet<u64>,
    // elements are collected per kind, then emitted in dependency order
    material_elems: Vec<Element>,
    level_elems: Vec<Element>,
    grid_elems: Vec<Element>,
    wall_elems: Vec<Element>,
    other_elems: Vec<Element>,
    opening_elems: Vec<Element>,
}

impl<'a> Importer<'a> {
    fn new(f: &'a StepFile, by_class: &'a HashMap<&'a str, Vec<u64>>) -> Self {
        let (len, angle, unit_note) = units(f, by_class);
        let mut g = Geo::new(f, len, angle);
        if let Some(note) = unit_note {
            g.warnings.add(note);
        }
        Self {
            f,
            by_class,
            g,
            out: vec![],
            used_ids: HashSet::new(),
            storeys: HashMap::new(),
            levels: vec![],
            contained: HashMap::new(),
            parent: HashMap::new(),
            voids: HashMap::new(),
            fills: HashMap::new(),
            voided: HashSet::new(),
            filled: HashSet::new(),
            has_parts: HashSet::new(),
            materials_of: HashMap::new(),
            type_of: HashMap::new(),
            psets: HashMap::new(),
            types: HashMap::new(),
            object_types: HashSet::new(),
            type_elems: vec![],
            slabs: HashMap::new(),
            styles: HashMap::new(),
            material_colors: HashMap::new(),
            materials: HashMap::new(),
            layer_sets: HashMap::new(),
            assembly_elems: vec![],
            walls: HashMap::new(),
            wall_voids: HashMap::new(),
            mesh_reason: HashMap::new(),
            done: HashSet::new(),
            material_elems: vec![],
            level_elems: vec![],
            grid_elems: vec![],
            wall_elems: vec![],
            other_elems: vec![],
            opening_elems: vec![],
        }
    }

    fn ids(&self, class: &str) -> &'a [u64] {
        self.by_class.get(class).map(Vec::as_slice).unwrap_or(&[])
    }

    fn run(&mut self) {
        self.index_relationships();
        self.read_storeys();
        self.read_grids();
        self.read_types();
        self.read_object_types();
        for &id in self.ids("IFCCURTAINWALL") {
            self.import_curtain_wall(id);
        }
        for class in ["IFCWALL", "IFCWALLSTANDARDCASE", "IFCWALLELEMENTEDCASE"] {
            for &id in self.ids(class) {
                self.import_wall(id);
            }
        }
        for class in ["IFCSLAB", "IFCSLABSTANDARDCASE", "IFCSLABELEMENTEDCASE"] {
            for &id in self.ids(class) {
                self.import_slab(id);
            }
        }
        self.import_members();
        for class in [
            "IFCDOOR",
            "IFCDOORSTANDARDCASE",
            "IFCWINDOW",
            "IFCWINDOWSTANDARDCASE",
        ] {
            for &id in self.ids(class) {
                self.import_filling(id);
            }
        }
        self.import_bare_openings();
        self.import_wall_voids();
        self.import_objects();
        for &id in self.ids("IFCFOOTING") {
            self.import_footing(id);
        }
        self.import_spaces();
        let mut rest: Vec<u64> = self
            .f
            .entities
            .iter()
            .filter(|(id, e)| {
                !self.done.contains(id)
                    && !e.arg(6).is_null()
                    && product_class(&e.class).is_some_and(|c| !SKIPPED.contains(&c))
                    && !e.class.starts_with("IFCSTRUCTURAL")
                    // a layer piece of a wall, slab or roof read as itself:
                    // its layers come from that element's layer set
                    && !(e.class == "IFCBUILDINGELEMENTPART"
                        && self.parent.get(id).is_some_and(|p| self.done.contains(p)))
            })
            .map(|(id, _)| *id)
            .collect();
        rest.sort_unstable();
        for id in rest {
            if !self.import_revolved_part(id) {
                self.import_free_mesh(id);
            }
        }
        // the rest of the file's layer sets (a roof's, unused ones) come
        // along as assemblies, as its materials do
        let mut sets: Vec<u64> = self.ids("IFCMATERIALLAYERSET").to_vec();
        sets.sort_unstable();
        for set in sets {
            let roof = self.materials_of.iter().any(|(p, m)| {
                *m == set
                    && self
                        .f
                        .get(*p)
                        .is_some_and(|e| e.class.starts_with("IFCROOF"))
            });
            let name = self
                .f
                .get(set)
                .and_then(|e| e.arg(1).as_str())
                .unwrap_or("")
                .to_lowercase();
            let usage = if roof || name.contains("roof") {
                creabim_core::AssemblyUse::Roof
            } else if name.contains("floor") || name.contains("slab") {
                creabim_core::AssemblyUse::Floor
            } else {
                creabim_core::AssemblyUse::Wall
            };
            self.layer_set(set, usage);
        }
        self.out = [
            std::mem::take(&mut self.material_elems),
            std::mem::take(&mut self.assembly_elems),
            std::mem::take(&mut self.type_elems),
            std::mem::take(&mut self.level_elems),
            std::mem::take(&mut self.grid_elems),
            std::mem::take(&mut self.wall_elems),
            std::mem::take(&mut self.other_elems),
            std::mem::take(&mut self.opening_elems),
        ]
        .concat();
        // instances override what differs from their type
        let params: HashMap<ElementId, creabim_core::TypeParams> = self
            .out
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::ElementType(t) => Some((e.id, t.params.clone())),
                _ => None,
            })
            .collect();
        for e in &mut self.out {
            if let Some(r) = &mut e.of_type {
                if let Some(p) = params.get(&r.id) {
                    // a door's or window's frame is only written on its type
                    if let (
                        creabim_core::TypeParams::Door(t) | creabim_core::TypeParams::Window(t),
                        ElementKind::Opening(o),
                    ) = (p, &mut e.kind)
                    {
                        o.frame = t.frame;
                    }
                    r.overrides = creabim_core::types::differences(p, &e.kind);
                }
            }
        }
    }

    /// The single values of a product's (or a type's) property set `name`.
    fn props(&self, id: u64, name: &str) -> crate::params::Props {
        let f = self.f;
        let mut sets: Vec<u64> = self.psets.get(&id).cloned().unwrap_or_default();
        if let Some(e) = f.get(id) {
            if e.class.ends_with("TYPE") || e.class.ends_with("STYLE") {
                sets.extend(e.arg(5).refs());
            }
        }
        let mut out = crate::params::Props::new();
        for s in sets.iter().filter_map(|s| f.get(*s)) {
            if !s.is("IFCPROPERTYSET") || s.arg(2).as_str() != Some(name) {
                continue;
            }
            for p in s.arg(4).refs().filter_map(|p| f.get(p)) {
                if !p.is("IFCPROPERTYSINGLEVALUE") {
                    continue;
                }
                let Some(key) = p.arg(0).as_str() else {
                    continue;
                };
                let v = p.arg(2);
                let number = match v {
                    Value::Typed(t, _) if t.contains("MEASURE") => self.g.length(v),
                    _ => v.as_f64(),
                };
                out.insert(key.to_string(), (number, v.as_str().map(str::to_string)));
            }
        }
        out
    }

    /// The file's element types (IfcWallType, IfcDoorType...), with the
    /// parameters CreaBIM wrote on them and their material or layer set.
    fn read_types(&mut self) {
        let mut ids: Vec<(u64, creabim_core::TypeCategory, bool)> = vec![];
        for (id, e) in &self.f.entities {
            if let Some((c, curtain)) = crate::params::category_of(&e.class) {
                ids.push((*id, c, curtain));
            }
        }
        ids.sort_unstable_by_key(|i| i.0);
        for (id, category, curtain) in ids {
            use creabim_core::types::TypeParams as P;
            let Some(e) = self.f.get(id) else { continue };
            let props = self.props(id, crate::params::TYPE_PSET);
            let mut params = crate::params::type_from(category, curtain, &props);
            let usage = match category {
                creabim_core::TypeCategory::Slab => Some(creabim_core::AssemblyUse::Floor),
                creabim_core::TypeCategory::Roof => Some(creabim_core::AssemblyUse::Roof),
                creabim_core::TypeCategory::Wall if !curtain => {
                    Some(creabim_core::AssemblyUse::Wall)
                }
                _ => None,
            };
            let assembly = usage.and_then(|u| self.assembly_for(id, u)).map(|a| a.0);
            let material = match assembly {
                Some(a) => self.core_material(a),
                None => self.material_for(id),
            };
            match &mut params {
                P::Wall(w) => (w.assembly, w.material) = (assembly, material),
                P::Slab(s) | P::Roof(s) => (s.assembly, s.material) = (assembly, material),
                P::Door(o) | P::Window(o) => o.material = material,
                P::Column(c) => c.material = material,
                P::Beam(b) => b.material = material,
                P::Footing(f) => f.material = material,
                // object types are read by read_object_types
                P::Object(_) => continue,
            }
            let mut t = creabim_core::ElementType::new(params);
            t.description = e.arg(3).as_str().unwrap_or("").trim().to_string();
            let eid = self.new_id(e);
            let name = Self::name(e, "Type");
            self.type_elems.push(Element {
                mark: None,
                id: eid,
                name,
                kind: ElementKind::ElementType(t),
                source: None,
                of_type: None,
            });
            self.types.insert(id, eid);
        }
    }

    /// The imported type of a product, if its IFC type was imported.
    fn type_ref(&self, id: u64) -> Option<creabim_core::TypeRef> {
        let t = self.type_of.get(&id)?;
        Some(creabim_core::TypeRef::new(*self.types.get(t)?))
    }

    /// A curtain wall CreaBIM wrote: its grid from its property set, its
    /// line from its placement. Its mullions and panels are rebuilt from
    /// the grid, so they are not imported themselves. Others are left to
    /// the general import (their parts come in as meshes).
    fn import_curtain_wall(&mut self, id: u64) {
        let props = self.props(id, crate::params::CURTAIN_PSET);
        let (Some(grid), Some(length), Some(height)) = (
            crate::params::curtain_from(&props),
            props.get("Length").and_then(|v| v.0),
            props.get("Height").and_then(|v| v.0),
        ) else {
            return;
        };
        let Some(e) = self.f.get(id) else { return };
        let world = self.g.placement(e.arg(5));
        let start = world.w_axis.truncate().truncate();
        let d = world.x_axis.truncate().truncate().normalize_or_zero();
        if d == DVec2::ZERO || !(length > 1e-3 && height > 1e-3) {
            return;
        }
        let z = world.w_axis.z;
        let (lid, elev) = self.level_for(id, z);
        self.done.insert(id);
        let parts: Vec<u64> = self
            .parent
            .iter()
            .filter(|(_, p)| **p == id)
            .map(|(c, _)| *c)
            .collect();
        let material = parts
            .iter()
            .find(|p| self.f.get(**p).is_some_and(|e| e.is("IFCMEMBER")))
            .copied()
            .and_then(|p| self.material_for(p));
        self.done.extend(parts);
        let mut w = Wall::new(lid, start, start + d * length, grid.mullion_depth, height);
        w.base_offset = z - elev;
        w.material = material;
        w.curtain = Some(grid);
        let eid = self.new_id(e);
        self.walls.entry(id).or_default().push(HostWall {
            id: eid,
            start: w.start,
            end: w.end,
            thickness: w.thickness,
            base_z: z,
            top_z: z + height,
        });
        self.wall_elems.push(Element {
            mark: None,
            source: None,
            of_type: self.type_ref(id),
            id: eid,
            name: Self::name(e, "Curtain wall"),
            kind: ElementKind::Wall(w),
        });
    }

    /// A footing CreaBIM wrote: its values from its property set, its
    /// level from its storey. Others are left to the general import.
    fn import_footing(&mut self, id: u64) {
        let props = self.props(id, crate::params::FOOTING_PSET);
        let Some(e) = self.f.get(id) else { return };
        let (level, _) = self.level_for(id, 0.0);
        let Some((mut f, host)) = crate::params::footing_from(&props, level) else {
            return;
        };
        // the wall or column it is under keeps its id from its GlobalId
        f.host = host.as_deref().and_then(ElementId::from_ifc_guid);
        f.material = self.material_for(id);
        self.done.insert(id);
        let eid = self.new_id(e);
        let name = Self::name(e, f.kind.label());
        self.opening_elems.push(Element {
            mark: None,
            source: None,
            of_type: self.type_ref(id),
            id: eid,
            name,
            kind: ElementKind::Footing(f),
        });
    }

    /// Openings with no door or window in them: bare holes in native
    /// walls, holes in native slabs.
    fn import_bare_openings(&mut self) {
        let filled: HashSet<u64> = self.fills.values().copied().collect();
        let mut ids: Vec<u64> = self
            .ids("IFCOPENINGELEMENT")
            .iter()
            .chain(self.ids("IFCOPENINGSTANDARDCASE"))
            .copied()
            .filter(|o| !filled.contains(o))
            .collect();
        ids.sort_unstable();
        const GROUP: &str = "openings without a door or window were not cut";
        for oid in ids {
            let Some(host) = self.voids.get(&oid).copied() else {
                self.note(oid, GROUP, "voids nothing (no IfcRelVoidsElement)");
                continue;
            };
            let Some(o) = self.f.get(oid) else { continue };
            let world = self.g.placement(o.arg(5));
            let mesh = self.g.product_mesh(o, &world);
            if let Some(wall) = self.host_wall(host, &mesh) {
                let Some(r) = opening_in_wall(&wall, &mesh, None) else {
                    self.note(oid, GROUP, "could not be measured in its wall");
                    continue;
                };
                let (offset, sill, width, height) = self.snap_to_void(&wall, r);
                let eid = self.new_id(o);
                self.opening_elems.push(Element {
                    mark: None,
                    source: None,
                    of_type: None,
                    id: eid,
                    name: Self::name(o, "Opening"),
                    kind: ElementKind::Opening(Opening {
                        host: wall.id,
                        kind: OpeningKind::Hole,
                        offset: offset.max(0.0),
                        sill: sill.max(0.0),
                        width,
                        height,
                        material: None,
                        frame: Default::default(),
                        flip_hand: false,
                        flip_facing: false,
                    }),
                });
            } else if let Some(&i) = self.slabs.get(&host) {
                let Some(p) = prism_of_mesh(mesh) else {
                    self.note(oid, GROUP, "is not an upright prism through its slab");
                    continue;
                };
                if let ElementKind::Slab(s) = &mut self.wall_elems[i].kind {
                    // a B-rep slab may have the hole cut already
                    let c = p.footprint.iter().copied().sum::<DVec2>() / p.footprint.len() as f64;
                    let known = s.holes.iter().any(|h| {
                        crate::tess::inside(c, h)
                            && (area2(h) - area2(&p.footprint)).abs() < 0.01 * area2(h).abs()
                    });
                    if !known {
                        s.holes.push(p.footprint);
                    }
                }
            }
            // openings in other hosts are cut into their meshes, which
            // say so if they can't be
        }
    }

    fn index_relationships(&mut self) {
        let f = self.f;
        let by_class = self.by_class;
        let each = |class: &str| {
            by_class
                .get(class)
                .into_iter()
                .flatten()
                .filter_map(|id| f.get(*id))
        };
        for r in each("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
            if let Some(s) = r.arg(5).as_ref() {
                for e in r.arg(4).refs() {
                    self.contained.insert(e, s);
                }
            }
        }
        for r in each("IFCRELAGGREGATES").chain(each("IFCRELNESTS")) {
            if let Some(p) = r.arg(4).as_ref() {
                for c in r.arg(5).refs() {
                    self.parent.insert(c, p);
                }
            }
        }
        for r in each("IFCRELVOIDSELEMENT") {
            if let (Some(host), Some(o)) = (r.arg(4).as_ref(), r.arg(5).as_ref()) {
                self.voids.insert(o, host);
            }
        }
        for r in each("IFCRELFILLSELEMENT") {
            if let (Some(o), Some(fill)) = (r.arg(4).as_ref(), r.arg(5).as_ref()) {
                self.fills.insert(fill, o);
            }
        }
        for r in each("IFCRELASSOCIATESMATERIAL") {
            if let Some(m) = r.arg(5).as_ref() {
                for o in r.arg(4).refs() {
                    self.materials_of.insert(o, m);
                }
            }
        }
        for r in each("IFCRELDEFINESBYPROPERTIES") {
            if let Some(p) = r.arg(5).as_ref() {
                for o in r.arg(4).refs() {
                    self.psets.entry(o).or_default().push(p);
                }
            }
        }
        for r in each("IFCRELDEFINESBYTYPE") {
            if let Some(t) = r.arg(5).as_ref() {
                for o in r.arg(4).refs() {
                    self.type_of.insert(o, t);
                }
            }
        }
        self.voided = self.voids.values().copied().collect();
        self.filled = self.fills.values().copied().collect();
        self.has_parts = self.parent.values().copied().collect();
        for s in each("IFCSTYLEDITEM") {
            if let (Some(item), Some(c)) = (s.arg(0).as_ref(), style_surface(f, s.arg(1))) {
                self.styles.insert(item, c);
            }
        }
        for mdr in each("IFCMATERIALDEFINITIONREPRESENTATION") {
            let Some(m) = mdr.arg(3).as_ref() else {
                continue;
            };
            let color = mdr
                .arg(2)
                .refs()
                .filter_map(|r| f.get(r))
                .flat_map(|r| r.arg(3).refs().collect::<Vec<_>>())
                .filter_map(|i| f.get(i))
                .find_map(|si| style_surface(f, si.arg(1)));
            if let Some(c) = color {
                self.material_colors.insert(m, c);
            }
        }
    }

    fn new_id(&mut self, e: &Entity) -> ElementId {
        let id = e
            .arg(0)
            .as_str()
            .and_then(ElementId::from_ifc_guid)
            .filter(|id| !self.used_ids.contains(id))
            .unwrap_or_default();
        self.used_ids.insert(id);
        id
    }

    fn name(e: &Entity, fallback: &str) -> String {
        e.arg(2)
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| fallback.to_string())
    }

    fn read_storeys(&mut self) {
        let mut list: Vec<(f64, u64)> = Vec::new();
        for &id in self.ids("IFCBUILDINGSTOREY") {
            let Some(e) = self.f.get(id) else { continue };
            let z = if e.arg(5).is_null() {
                self.g.length(e.arg(9)).unwrap_or(0.0)
            } else {
                self.g.placement(e.arg(5)).w_axis.z
            };
            list.push((z, id));
        }
        list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if list.is_empty() {
            self.g
                .warnings
                .add("the file has no storeys; everything was put on one level at 0");
            let e = Element::new(
                "Level 0",
                ElementKind::Level(Level {
                    elevation: 0.0,
                    height: 3.0,
                }),
            );
            self.levels.push((0.0, e.id));
            self.level_elems.push(e);
            return;
        }
        for (i, &(z, id)) in list.iter().enumerate() {
            let Some(e) = self.f.get(id) else { continue };
            let height = list
                .get(i + 1)
                .map(|n| n.0 - z)
                .filter(|h| *h > 0.01)
                .unwrap_or(3.0);
            let lid = self.new_id(e);
            let mut name = Self::name(e, "");
            if name.is_empty() {
                name = e
                    .arg(7)
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("Level {i}"));
            }
            self.level_elems.push(Element {
                mark: None,
                source: None,
                id: lid,
                name,
                kind: ElementKind::Level(Level {
                    elevation: z,
                    height,
                }),
                of_type: None,
            });
            self.storeys.insert(
                id,
                Storey {
                    id: lid,
                    elevation: z,
                },
            );
            self.levels.push((z, lid));
        }
    }

    fn read_grids(&mut self) {
        for &id in self.ids("IFCGRID") {
            let Some(e) = self.f.get(id) else { continue };
            let world = self.g.placement(e.arg(5));
            // a grid of several axes becomes a grid group holding them
            let count: usize = [7, 8, 9].iter().map(|i| e.arg(*i).as_list().len()).sum();
            let group = (count > 1).then(|| {
                let name = e.arg(2).as_str().unwrap_or("").trim().to_string();
                let g = Element::new(
                    if name.is_empty() { "Grid".into() } else { name },
                    ElementKind::GridGroup(creabim_core::GridGroup::default()),
                );
                let id = g.id;
                self.grid_elems.push(g);
                id
            });
            for axes in [7, 8, 9] {
                for ax in e.arg(axes).as_list() {
                    let Some(a) = self.g.get(ax) else { continue };
                    let Some(pts) = self.g.curve(a.arg(1)) else {
                        continue;
                    };
                    let (Some(p), Some(q)) = (pts.first(), pts.last()) else {
                        continue;
                    };
                    let (p, q) = (world.transform_point3(*p), world.transform_point3(*q));
                    let label = a.arg(0).as_str().unwrap_or("").to_string();
                    self.grid_elems.push(Element::new(
                        format!("Grid {label}"),
                        ElementKind::Grid(Grid {
                            label,
                            start: p.truncate(),
                            end: q.truncate(),
                            group,
                        }),
                    ));
                }
            }
        }
    }

    /// The storey an element sits on: through its spatial container, the
    /// element it is part of, or the container's own parents.
    fn storey_of(&self, id: u64) -> Option<&Storey> {
        let mut cur = id;
        for _ in 0..32 {
            if let Some(s) = self.storeys.get(&cur) {
                return Some(s);
            }
            // a door or window is usually contained, but may only fill an opening
            let next = self
                .contained
                .get(&cur)
                .or_else(|| self.parent.get(&cur))
                .or_else(|| self.fills.get(&cur))
                .or_else(|| self.voids.get(&cur))?;
            cur = *next;
        }
        None
    }

    /// The level with the highest elevation at or below `z`.
    fn level_at(&self, z: f64) -> (ElementId, f64) {
        let mut best = self.levels.first().map(|l| (l.1, l.0)).unwrap_or_default();
        for &(e, id) in &self.levels {
            if e <= z + 1e-3 {
                best = (id, e);
            }
        }
        best
    }

    fn level_for(&self, id: u64, z: f64) -> (ElementId, f64) {
        match self.storey_of(id) {
            Some(s) => (s.id, s.elevation),
            None => self.level_at(z),
        }
    }

    // ---- materials ----

    /// The CreaBIM material for a product, from its own association or its type's.
    fn material_for(&mut self, id: u64) -> Option<ElementId> {
        let assoc = self
            .materials_of
            .get(&id)
            .or_else(|| self.type_of.get(&id).and_then(|t| self.materials_of.get(t)))
            .copied()?;
        let (name, mat_id) = self.material_name(assoc, 0)?;
        if let Some(&m) = self.materials.get(&name) {
            return Some(m);
        }
        let f = self.f;
        let surface = mat_id
            .and_then(|m| self.material_colors.get(&m).copied())
            .or_else(|| self.product_surface(f.get(id)?));
        let material = material_from(f, &name, mat_id, surface);
        let e = Element::new(name.clone(), ElementKind::Material(material));
        let mid = e.id;
        self.material_elems.push(e);
        self.materials.insert(name, mid);
        Some(mid)
    }

    /// The CreaBIM material for an IfcMaterial, made on first use.
    fn material_entity(&mut self, mat: u64) -> Option<ElementId> {
        let f = self.f;
        let e = f.get(mat).filter(|e| e.is("IFCMATERIAL"))?;
        let name = e.arg(0).as_str().map(str::trim).filter(|n| !n.is_empty())?;
        if let Some(&m) = self.materials.get(name) {
            return Some(m);
        }
        let surface = self.material_colors.get(&mat).copied();
        let material = material_from(f, name, Some(mat), surface);
        let e = Element::new(name, ElementKind::Material(material));
        let id = e.id;
        self.material_elems.push(e);
        self.materials.insert(name.to_string(), id);
        Some(id)
    }

    /// The CreaBIM assembly for an IfcMaterialLayerSet of two or more
    /// layers, made on first use: its layers with their materials,
    /// thicknesses and functions. (A set of one layer is just a material.)
    fn layer_set(&mut self, set: u64, usage: creabim_core::AssemblyUse) -> Option<ElementId> {
        use creabim_core::assembly::{Layer, LayerFunction, MIN_LAYER};
        if let Some(&a) = self.layer_sets.get(&set) {
            return Some(a);
        }
        let f = self.f;
        let e = f.get(set).filter(|e| e.is("IFCMATERIALLAYERSET"))?;
        if e.arg(0).refs().count() < 2 {
            return None;
        }
        let mut layers = vec![];
        for l in e.arg(0).refs().filter_map(|l| f.get(l)) {
            if !l.is("IFCMATERIALLAYER") {
                continue;
            }
            let material = l.arg(0).as_ref().and_then(|m| self.material_entity(m));
            let thickness = self.g.length(l.arg(1)).unwrap_or(0.0).max(MIN_LAYER);
            let ventilated = l.arg(2).as_enum() == Some("T");
            let category = l.arg(5).as_str().or(l.arg(3).as_str()).unwrap_or("");
            let function = if material.is_none() && ventilated {
                LayerFunction::AirGap
            } else {
                LayerFunction::from_ifc(category, ventilated)
            };
            layers.push(Layer::new(function, material, thickness));
        }
        if layers.is_empty() {
            return None;
        }
        let base = e
            .arg(1)
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or("Assembly")
            .to_string();
        let mut name = base.clone();
        let mut n = 2;
        while self.assembly_elems.iter().any(|a| a.name == name) {
            name = format!("{base} {n}");
            n += 1;
        }
        let mut a = creabim_core::Assembly::new(usage, layers);
        a.description = e.arg(2).as_str().unwrap_or("").trim().to_string();
        let el = Element::new(name, ElementKind::Assembly(a));
        let id = el.id;
        self.assembly_elems.push(el);
        self.layer_sets.insert(set, id);
        Some(id)
    }

    /// What an imported assembly's core is made of: the element's material
    /// should it lose the assembly.
    fn core_material(&self, assembly: ElementId) -> Option<ElementId> {
        self.assembly_elems.iter().find_map(|e| match &e.kind {
            ElementKind::Assembly(a) if e.id == assembly => a.layers[a.core().0].material,
            _ => None,
        })
    }

    /// A layered product's assembly, and for one placed with a layer set
    /// usage, whether its layers run the usage axis's positive way.
    fn assembly_for(
        &mut self,
        id: u64,
        usage: creabim_core::AssemblyUse,
    ) -> Option<(ElementId, Option<bool>)> {
        let assoc = self
            .materials_of
            .get(&id)
            .or_else(|| self.type_of.get(&id).and_then(|t| self.materials_of.get(t)))
            .copied()?;
        let e = self.f.get(assoc)?;
        let (set, positive) = if e.is("IFCMATERIALLAYERSETUSAGE") {
            (
                e.arg(0).as_ref()?,
                Some(e.arg(2).as_enum() != Some("NEGATIVE")),
            )
        } else if e.is("IFCMATERIALLAYERSET") {
            (assoc, None)
        } else {
            return None;
        };
        Some((self.layer_set(set, usage)?, positive))
    }

    /// The surface style on a product's body, if it has one.
    fn product_surface(&self, e: &Entity) -> Option<Surface> {
        let body = self.g.body(e)?;
        body.items.iter().find_map(|i| self.styles.get(i).copied())
    }

    /// Every IfcMaterial of the file, whether anything uses it or not,
    /// with its look: its own surface style, else the style on the body of
    /// the first product made of it.
    fn all_materials(&self) -> Vec<(String, Material)> {
        let f = self.f;
        // the first product made of each material, for its body's style
        let mut product_of: HashMap<u64, u64> = HashMap::new();
        let mut assocs: Vec<(&u64, &u64)> = self.materials_of.iter().collect();
        assocs.sort();
        for (product, assoc) in assocs {
            if let Some((_, Some(m))) = self.material_name(*assoc, 0) {
                product_of.entry(m).or_insert(*product);
            }
        }
        let mut out: Vec<(String, Material)> = vec![];
        for &id in self.ids("IFCMATERIAL") {
            let Some(e) = f.get(id) else { continue };
            let Some(name) = e.arg(0).as_str().map(str::trim).filter(|n| !n.is_empty()) else {
                continue;
            };
            if out.iter().any(|(n, _)| n == name) {
                continue;
            }
            let surface = self.material_colors.get(&id).copied().or_else(|| {
                let p = f.get(*product_of.get(&id)?)?;
                self.product_surface(p)
            });
            out.push((name.to_string(), material_from(f, name, Some(id), surface)));
        }
        out
    }

    /// A material select's name and the IfcMaterial it is mostly made of.
    fn material_name(&self, id: u64, depth: usize) -> Option<(String, Option<u64>)> {
        let e = self.f.get(id)?;
        if depth > 8 {
            return None;
        }
        let named = |s: &Value| {
            s.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        };
        match e.class.as_str() {
            "IFCMATERIAL" => Some((named(e.arg(0))?, Some(id))),
            "IFCMATERIALLAYERSETUSAGE" => self.material_name(e.arg(0).as_ref()?, depth + 1),
            "IFCMATERIALLAYERSET" => {
                // the thickest layer is what the element is mostly made of
                let main = e
                    .arg(0)
                    .refs()
                    .filter_map(|l| self.f.get(l))
                    .max_by(|a, b| {
                        let t = |l: &Entity| l.arg(1).as_f64().unwrap_or(0.0);
                        t(a).total_cmp(&t(b))
                    })
                    .and_then(|l| l.arg(0).as_ref());
                let main_name = main.and_then(|m| self.material_name(m, depth + 1));
                match named(e.arg(1)) {
                    Some(n) => Some((n, main_name.and_then(|m| m.1))),
                    None => main_name,
                }
            }
            "IFCMATERIALLAYER" | "IFCMATERIALPROFILE" | "IFCMATERIALCONSTITUENT" => {
                let m = if e.is("IFCMATERIALLAYER") {
                    e.arg(0)
                } else {
                    e.arg(2)
                };
                self.material_name(m.as_ref()?, depth + 1)
            }
            "IFCMATERIALLIST" => self.material_name(e.arg(0).refs().next()?, depth + 1),
            "IFCMATERIALPROFILESETUSAGE" => self.material_name(e.arg(0).as_ref()?, depth + 1),
            "IFCMATERIALPROFILESET" | "IFCMATERIALCONSTITUENTSET" => {
                let first = e
                    .arg(2)
                    .refs()
                    .find_map(|p| self.material_name(p, depth + 1));
                match named(e.arg(0)) {
                    Some(n) => Some((n, first.and_then(|m| m.1))),
                    None => first,
                }
            }
            _ => None,
        }
    }

    /// Total thickness of an element's layer set usage, and where its
    /// centre lies on the element's local y axis.
    fn layer_band(&self, id: u64) -> Option<(f64, f64)> {
        let usage = self.f.get(*self.materials_of.get(&id)?)?;
        if !usage.is("IFCMATERIALLAYERSETUSAGE") {
            return None;
        }
        let set = self.g.get(usage.arg(0))?;
        let total: f64 = set
            .arg(0)
            .refs()
            .filter_map(|l| self.f.get(l))
            .filter_map(|l| self.g.length(l.arg(1)))
            .sum();
        if total <= 0.0 {
            return None;
        }
        let sense = if usage.arg(2).as_enum() == Some("NEGATIVE") {
            -1.0
        } else {
            1.0
        };
        let offset = self.g.length(usage.arg(3)).unwrap_or(0.0);
        Some((total, offset + sense * total / 2.0))
    }

    // ---- shape analysis ----

    /// How notes name an element: its class, name and GlobalId.
    pub(super) fn label(&self, id: u64) -> String {
        let Some(e) = self.f.get(id) else {
            return format!("#{id}");
        };
        let class = product_class(&e.class).unwrap_or(&e.class);
        let guid = e.arg(0).as_str().unwrap_or("no GlobalId");
        match e.arg(2).as_str().map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => format!("{class} '{name}' ({guid})"),
            None => format!("{class} ({guid})"),
        }
    }

    /// The imported revolution as a native part: the product's IFC class
    /// and material as the file has them, on the storey under its centre
    /// of mass, with the arc in that storey's frame.
    pub(super) fn swept_part(&mut self, id: u64, e: &Entity, s: RevolvedSweep) -> ElementKind {
        let (level, elev) = self.level_for(id, s.z);
        let above = |p: DVec3| DVec3::new(p.x, p.y, p.z - elev);
        let mut part = Part::sweep(
            level,
            s.profile,
            Path::Arc {
                center: above(s.center),
                axis: s.axis,
                start: above(s.start),
                sweep: s.sweep,
            },
        );
        part.ifc_class = product_class(&e.class)
            .unwrap_or("IfcBuildingElementProxy")
            .to_string();
        part.material = self.material_for(id);
        ElementKind::Part(part)
    }

    /// A product of any other class whose body is one clean revolution:
    /// a native arc sweep keeping its IFC class. `false` when it is no
    /// such product, so the caller meshes it (recording why, when it is
    /// one that stays a mesh).
    fn import_revolved_part(&mut self, id: u64) -> bool {
        let Some(e) = self.f.get(id) else {
            return false;
        };
        if self.written_as_mesh(id) {
            return false;
        }
        let world = self.g.placement(e.arg(5));
        let swept = match self.g.revolved_sweep(e, &world) {
            Some(Ok(s)) => s,
            Some(Err(why)) => {
                if self.g.body(e).is_some() {
                    let body = self.body_kind(e);
                    self.mesh_reason
                        .insert(id, format!("is {body} that {why}; imported as a mesh"));
                }
                return false;
            }
            None => return false,
        };
        self.done.insert(id);
        let kind = self.swept_part(id, e, swept);
        let eid = self.new_id(e);
        self.other_elems.push(Element {
            mark: None,
            source: None,
            of_type: self.type_ref(id),
            id: eid,
            name: Self::name(e, "Sweep"),
            kind,
        });
        true
    }

    /// A note about one element, counted under `group` and listed as the
    /// element's label followed by `why`.
    pub(super) fn note(&mut self, id: u64, group: &str, why: impl std::fmt::Display) {
        let detail = format!("{} {why}", self.label(id));
        self.g.warnings.element(group, detail);
    }

    /// What an element's body is made of, for notes: "an IfcFacetedBrep".
    pub(super) fn body_kind(&self, e: &Entity) -> String {
        let Some(body) = self.g.body(e) else {
            return "a body".into();
        };
        let [item] = body.items[..] else {
            return format!("a body of {} items", body.items.len());
        };
        let class = self.f.get(item).map_or("", |i| i.class.as_str());
        let name = match class {
            "IFCFACETEDBREP" => "IfcFacetedBrep",
            "IFCFACETEDBREPWITHVOIDS" => "IfcFacetedBrepWithVoids",
            "IFCADVANCEDBREP" => "IfcAdvancedBrep",
            "IFCPOLYGONALFACESET" => "IfcPolygonalFaceSet",
            "IFCTRIANGULATEDFACESET" => "IfcTriangulatedFaceSet",
            "IFCEXTRUDEDAREASOLID" => "IfcExtrudedAreaSolid",
            "IFCBOOLEANRESULT" => "IfcBooleanResult",
            "IFCBOOLEANCLIPPINGRESULT" => "IfcBooleanClippingResult",
            "IFCMAPPEDITEM" => "IfcMappedItem",
            "IFCSHELLBASEDSURFACEMODEL" => "IfcShellBasedSurfaceModel",
            "IFCFACEBASEDSURFACEMODEL" => "IfcFaceBasedSurfaceModel",
            "IFCREVOLVEDAREASOLID" => "IfcRevolvedAreaSolid",
            "IFCSWEPTDISKSOLID" => "IfcSweptDiskSolid",
            _ => return "a body".into(),
        };
        format!("an {name}")
    }

    /// The body as a vertical prism, if it is a single extrusion straight
    /// up or down (possibly clipped). Other bodies are read as meshes.
    fn prism(&mut self, e: &Entity, world: &DMat4) -> Option<Prism> {
        let body = self.g.body(e)?;
        let [mut item] = body.items[..] else {
            return None;
        };
        let mut clipped = false;
        for _ in 0..8 {
            let it = self.f.get(item)?;
            if it.is("IFCBOOLEANCLIPPINGRESULT") {
                clipped = true;
                item = it.arg(1).as_ref()?;
            } else {
                break;
            }
        }
        let it = self.f.get(item)?;
        if !it.is("IFCEXTRUDEDAREASOLID") {
            return None;
        }
        let x = self.g.extrusion(item, world)?;
        let normal = x.position.z_axis.truncate();
        let v = x.position.transform_vector3(x.vector);
        if normal.cross(DVec3::Z).length() > 1e-6 || v.truncate().length() > 1e-6 * v.length() {
            return None;
        }
        let [region] = &x.regions[..] else {
            return None;
        };
        let plan = |lp: &[DVec2]| -> Vec<DVec2> {
            lp.iter()
                .map(|p| x.position.transform_point3(p.extend(0.0)).truncate())
                .collect()
        };
        let z = x
            .position
            .transform_point3(region.outer.first()?.extend(0.0))
            .z;
        let mut footprint = plan(&region.outer);
        if area2(&footprint) < 0.0 {
            footprint.reverse();
        }
        let holes = region
            .holes
            .iter()
            .map(|h| {
                let mut h = plan(h);
                if area2(&h) < 0.0 {
                    h.reverse();
                }
                h
            })
            .collect();
        Some(Prism {
            footprint,
            z0: z.min(z + v.z),
            z1: z.max(z + v.z),
            holes,
            clipped,
        })
    }

    /// A wall's axis in world plan coordinates, if it is a straight line.
    fn axis(&mut self, e: &Entity, world: &DMat4) -> Option<(DVec2, DVec2)> {
        let rep = self
            .g
            .reps(e)
            .into_iter()
            .find(|r| r.identifier.eq_ignore_ascii_case("Axis"))?;
        let pts = self.g.curve(&Value::Ref(*rep.items.first()?))?;
        let pts: Vec<DVec2> = pts
            .iter()
            .map(|p| world.transform_point3(*p).truncate())
            .collect();
        let (a, b) = (*pts.first()?, *pts.last()?);
        let d = (b - a).normalize_or_zero();
        if d == DVec2::ZERO || pts.iter().any(|p| (*p - a).perp_dot(d).abs() > 1e-4) {
            return None;
        }
        Some((a, b))
    }

    /// Did CreaBIM write this element as a free mesh?
    pub(super) fn written_as_mesh(&self, id: u64) -> bool {
        let props = self.props(id, MEMBER_PSET);
        props.get("Kind").and_then(|k| k.1.as_deref()) == Some("FreeMesh")
    }

    fn import_wall(&mut self, id: u64) {
        let Some(e) = self.f.get(id) else { return };
        if self.written_as_mesh(id) {
            return self.import_free_mesh(id);
        }
        let world = self.g.placement(e.arg(5));
        let walls = match self.multi_wall(id, e, &world) {
            Some(walls) => walls,
            None => self.native_wall(id, e, &world).map(|w| vec![w]),
        };
        let walls = match walls {
            Ok(w) => w,
            Err(why) => {
                let kind = self.body_kind(e);
                self.mesh_reason
                    .insert(id, format!("is {kind} that {why}; imported as a mesh"));
                self.import_free_mesh(id);
                return;
            }
        };
        self.done.insert(id);
        let parts = walls.len();
        for (i, (w, voids)) in walls.into_iter().enumerate() {
            let part = (parts > 1).then_some(i);
            self.push_wall(id, e, &world, w, voids, part);
        }
    }

    /// One native wall of an IFC wall; `part` numbers the runs of a wall
    /// along a polyline axis, all but the first under new ids.
    fn push_wall(
        &mut self,
        id: u64,
        e: &'a Entity,
        world: &DMat4,
        w: Wall,
        voids: Vec<brep::Void>,
        part: Option<usize>,
    ) {
        let world = *world;
        let lid = w.level;
        let level_elev = self.levels.iter().find(|l| l.1 == lid).map_or(0.0, |l| l.0);
        let layered = self.assembly_for(id, creabim_core::AssemblyUse::Wall);
        // the first (exterior) layer is on the wall's left when the
        // layers run towards its right
        let flip = layered.and_then(|l| l.1).is_some_and(|positive| {
            let y = world.y_axis.truncate().truncate();
            let left = (w.end - w.start).normalize_or_zero().perp();
            positive == (y.dot(left) < 0.0)
        });
        // an axis on a face is that face's location line
        let axis = if part.is_some() {
            None
        } else {
            self.axis(e, &world)
        };
        let location = axis.map_or(LocationLine::Centre, |(a, _)| {
            let off = (a - w.start).dot((w.end - w.start).normalize_or_zero().perp());
            let t = w.thickness / 2.0;
            let ext = if flip { t } else { -t };
            if t > 1e-3 && (off - ext).abs() < 1e-3 {
                LocationLine::ExteriorFace
            } else if t > 1e-3 && (off + ext).abs() < 1e-3 {
                LocationLine::InteriorFace
            } else {
                LocationLine::Centre
            }
        });
        let material = match layered {
            Some((a, _)) => self.core_material(a),
            None => self.material_for(id),
        };
        let eid = match part {
            Some(i) if i > 0 => ElementId::new(),
            _ => self.new_id(e),
        };
        self.walls.entry(id).or_default().push(HostWall {
            id: eid,
            start: w.start,
            end: w.end,
            thickness: w.thickness,
            base_z: level_elev + w.base_offset,
            top_z: level_elev + w.base_offset + w.height,
        });
        if !voids.is_empty() {
            self.wall_voids
                .insert(eid, voids.into_iter().map(|v| (v, false)).collect());
        }
        self.wall_elems.push(Element {
            mark: None,
            source: None,
            of_type: self.type_ref(id),
            id: eid,
            name: match part {
                Some(i) => format!("{} {}", Self::name(e, "Wall"), i + 1),
                None => Self::name(e, "Wall"),
            },
            kind: ElementKind::Wall(Wall {
                material,
                assembly: layered.map(|l| l.0),
                flip,
                location,
                ..w
            }),
        });
    }

    /// Native walls along a wall's axis when it is a polyline of several
    /// runs; `None` for a straight axis or none.
    fn multi_wall(
        &mut self,
        id: u64,
        e: &Entity,
        world: &DMat4,
    ) -> Option<Result<Vec<WallPart>, String>> {
        let pts = self.axis_polyline(e, world)?;
        if pts.len() < 3 || remove_collinear(&pts).len() < 3 {
            return None;
        }
        let mesh = self.g.product_mesh(e, world);
        let shapes = match brep::walls_along(&mesh, &pts) {
            Ok(s) => s,
            Err(why) => return Some(Err(why)),
        };
        let walls = shapes
            .into_iter()
            .map(|w| {
                let (lid, elev) = self.level_for(id, w.z0);
                let mut wall = Wall::new(lid, w.start, w.end, w.thickness, w.z1 - w.z0);
                wall.base_offset = w.z0 - elev;
                (wall, w.voids)
            })
            .collect();
        Some(Ok(walls))
    }

    /// The points of a wall's axis in world plan coordinates.
    fn axis_polyline(&mut self, e: &Entity, world: &DMat4) -> Option<Vec<DVec2>> {
        let rep = self
            .g
            .reps(e)
            .into_iter()
            .find(|r| r.identifier.eq_ignore_ascii_case("Axis"))?;
        let pts = self.g.curve(&Value::Ref(*rep.items.first()?))?;
        Some(
            pts.iter()
                .map(|p| world.transform_point3(*p).truncate())
                .collect(),
        )
    }

    /// A native wall for an IFC wall, with the holes already cut through
    /// its body (a B-rep exported with its doors and windows cut), or why
    /// it can't be one.
    fn native_wall(
        &mut self,
        id: u64,
        e: &Entity,
        world: &DMat4,
    ) -> Result<(Wall, Vec<brep::Void>), String> {
        let axis = self.axis(e, world);
        let extruded = self
            .prism(e, world)
            .filter(|p| p.holes.is_empty())
            .and_then(|p| {
                let w = match wall_from_footprint(&p.footprint, axis) {
                    Some(w) => w,
                    None => self.stepped_wall(id, &p.footprint, axis, world)?,
                };
                Some((w, p))
            });
        let (start, end, thickness, z0, z1, voids) = match extruded {
            Some(((start, end, thickness), p)) => {
                if p.clipped {
                    self.note(
                        id,
                        "walls whose clipping was ignored to keep them native walls",
                        "is clipped (by a roof or a sloped cut); the clipping was ignored to keep it a native wall",
                    );
                }
                (start, end, thickness, p.z0, p.z1, vec![])
            }
            None => {
                let mesh = self.g.product_mesh(e, world);
                let mut w = brep::wall_of_mesh(&mesh, axis)?;
                // run the way the element's placement does
                let x = world.x_axis.truncate().truncate();
                if axis.is_none() && (w.end - w.start).dot(x) < 0.0 {
                    let len = w.start.distance(w.end);
                    std::mem::swap(&mut w.start, &mut w.end);
                    for v in &mut w.voids {
                        (v.u0, v.u1) = (len - v.u1, len - v.u0);
                    }
                }
                (w.start, w.end, w.thickness, w.z0, w.z1, w.voids)
            }
        };
        let (lid, elev) = self.level_for(id, z0);
        let wall = Wall {
            level: lid,
            start,
            end,
            thickness,
            height: z1 - z0,
            base_offset: z0 - elev,
            material: None,
            assembly: None,
            flip: false,
            location: Default::default(),
            top: Default::default(),
            disallow_join: [false; 2],
            attach_top: vec![],
            curtain: None,
            layer_ends: vec![],
            bulge: 0.0,
        };
        Ok((wall, voids))
    }

    /// A wall extruded from a footprint with stepped ends: the layer set
    /// usage still gives its faces.
    fn stepped_wall(
        &self,
        id: u64,
        fp: &[DVec2],
        axis: Option<(DVec2, DVec2)>,
        world: &DMat4,
    ) -> Option<(DVec2, DVec2, f64)> {
        let (a, b) = axis?;
        let (t, centre) = self.layer_band(id)?;
        let n = world.y_axis.truncate().truncate().normalize_or_zero();
        let d = (b - a).normalize_or_zero();
        if n == DVec2::ZERO || d.perp().dot(n).abs() < 0.999 {
            return None;
        }
        let (lo, hi) = (centre - t / 2.0, centre + t / 2.0);
        let inside = fp.iter().all(|p| {
            let s = (*p - a).dot(n);
            s > lo - 1e-3 && s < hi + 1e-3
        });
        inside.then(|| (a + n * centre, b + n * centre, t))
    }

    /// A slab's outline, holes and heights, from its extrusion or its
    /// mesh, or why it is not a flat slab.
    fn slab_shape(&mut self, e: &Entity, world: &DMat4) -> Result<brep::SlabShape, String> {
        if let Some(p) = self.prism(e, world).filter(|p| !p.clipped) {
            return Ok(brep::SlabShape {
                outline: remove_collinear(&p.footprint),
                holes: p.holes,
                z0: p.z0,
                z1: p.z1,
            });
        }
        brep::slab_of_mesh(&self.g.product_mesh(e, world))
    }

    fn import_slab(&mut self, id: u64) {
        let Some(e) = self.f.get(id) else { return };
        if self.written_as_mesh(id) {
            return self.import_free_mesh(id);
        }
        let world = self.g.placement(e.arg(5));
        let p = match self.slab_shape(e, &world) {
            Ok(p) => p,
            Err(why) => {
                let kind = self.body_kind(e);
                self.mesh_reason
                    .insert(id, format!("is {kind} that {why}; imported as a mesh"));
                self.import_free_mesh(id);
                return;
            }
        };
        // a storey at its top, its own storey first; else its own storey
        // or the level under its top, with the top offset from it
        let own = self.storey_of(id).map(|s| (s.id, s.elevation));
        let at_top = |(_, z): &(ElementId, f64)| (z - p.z1).abs() < 0.01;
        let (lid, elev) = own
            .filter(at_top)
            .or_else(|| self.levels.iter().map(|&(z, l)| (l, z)).find(at_top))
            .or(own)
            .unwrap_or_else(|| self.level_at(p.z1));
        let top_offset = if (p.z1 - elev).abs() < 1e-6 {
            0.0
        } else {
            p.z1 - elev
        };
        self.done.insert(id);
        let layered = self.assembly_for(id, creabim_core::AssemblyUse::Floor);
        let material = match layered {
            Some((a, _)) => self.core_material(a),
            None => self.material_for(id),
        };
        let eid = self.new_id(e);
        self.slabs.insert(id, self.wall_elems.len());
        self.wall_elems.push(Element {
            mark: None,
            source: None,
            of_type: self.type_ref(id),
            id: eid,
            name: Self::name(e, "Slab"),
            kind: ElementKind::Slab(Slab {
                layer_edges: vec![],
                level: lid,
                boundary: p.outline,
                thickness: p.z1 - p.z0,
                material,
                top_offset,
                bulges: vec![],
                assembly: layered.map(|l| l.0),
                holes: p.holes,
            }),
        });
    }

    /// Fit a measured opening to the hole cut through its wall's body, if
    /// the wall's B-rep had one there, and mark that hole as filled.
    fn snap_to_void(
        &mut self,
        wall: &HostWall,
        (offset, sill, width, height): (f64, f64, f64, f64),
    ) -> (f64, f64, f64, f64) {
        let Some(voids) = self.wall_voids.get_mut(&wall.id) else {
            return (offset, sill, width, height);
        };
        let (z0, z1) = (wall.base_z + sill, wall.base_z + sill + height);
        let best = voids
            .iter_mut()
            .filter(|(_, used)| !used)
            .map(|(v, used)| (v.covered_by(offset, offset + width, z0, z1), v, used))
            .filter(|(c, _, _)| *c > 0.5)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        match best {
            Some((_, v, used)) => {
                *used = true;
                (v.u0, v.z0 - wall.base_z, v.u1 - v.u0, v.z1 - v.z0)
            }
            None => (offset, sill, width, height),
        }
    }

    /// The holes cut through native walls' bodies that no door, window or
    /// opening of the file fills: bare openings, so the wall keeps them.
    fn import_wall_voids(&mut self) {
        let mut walls: Vec<(ElementId, Vec<brep::Void>)> = self
            .wall_voids
            .iter()
            .map(|(w, v)| (*w, v.iter().filter(|x| !x.1).map(|x| x.0).collect()))
            .collect();
        walls.sort_by_key(|w| w.0);
        for (wid, voids) in walls {
            let Some(host) = self.walls.values().flatten().find(|h| h.id == wid).copied() else {
                continue;
            };
            let name = self
                .wall_elems
                .iter()
                .find(|e| e.id == wid)
                .map_or("Wall".to_string(), |e| e.name.clone());
            for (i, v) in voids.into_iter().enumerate() {
                self.opening_elems.push(Element::new(
                    format!("{name} opening {}", i + 1),
                    ElementKind::Opening(Opening {
                        host: wid,
                        kind: OpeningKind::Hole,
                        offset: v.u0.max(0.0),
                        sill: (v.z0 - host.base_z).max(0.0),
                        width: v.u1 - v.u0,
                        height: v.z1 - v.z0,
                        material: None,
                        frame: Default::default(),
                        flip_hand: false,
                        flip_facing: false,
                    }),
                ));
            }
        }
    }

    /// The native wall a door's or window's body sits in, for one with no
    /// opening relationship: the nearest whose thickness and height hold
    /// the body's centre.
    fn wall_around(&self, mesh: &EditMesh) -> Option<HostWall> {
        let mut walls: Vec<&HostWall> = self.walls.values().flatten().collect();
        walls.sort_by_key(|w| w.id);
        nearest_wall(walls, mesh, 0.05)
    }

    /// Which of the native walls an IFC wall became an opening's body is in.
    fn host_wall(&self, host: u64, mesh: &EditMesh) -> Option<HostWall> {
        match self.walls.get(&host)?.as_slice() {
            [w] => Some(*w),
            walls => nearest_wall(walls.iter().collect(), mesh, 0.5),
        }
    }

    fn import_filling(&mut self, id: u64) {
        let Some(e) = self.f.get(id) else { return };
        if self.written_as_mesh(id) {
            return self.import_free_mesh(id);
        }
        let kind = if e.class.starts_with("IFCDOOR") {
            OpeningKind::Door
        } else {
            OpeningKind::Window
        };
        let world = self.g.placement(e.arg(5));
        let opening = self.fills.get(&id).copied();
        let host = opening.and_then(|o| self.voids.get(&o)).copied();
        let mut why = None;
        // the hole is what the wall sees, so measure the opening element
        let mut mesh = opening
            .and_then(|o| self.f.get(o))
            .map_or_else(EditMesh::default, |o| {
                let ow = self.g.placement(o.arg(5));
                self.g.product_mesh(o, &ow)
            });
        if mesh.faces.is_empty() {
            mesh = self.g.product_mesh(e, &world);
        }
        let filled = match host.map(|h| (h, self.host_wall(h, &mesh))) {
            Some((_, Some(wall))) => {
                let r = opening_in_wall(&wall, &mesh, None);
                if r.is_none() {
                    why = Some("could not be measured in its wall".to_string());
                }
                r.map(|r| (wall, r))
            }
            Some((h, None)) => {
                why = Some(format!(
                    "fills an opening in {}, which is not a native wall",
                    self.label(h)
                ));
                None
            }
            None => None,
        };
        let placed = filled.or_else(|| {
            if why.is_some() {
                return None;
            }
            // no opening relationship: find a wall the door's body sits in
            let mesh = self.g.product_mesh(e, &world);
            let overall = (self.g.length(e.arg(9)), self.g.length(e.arg(8)));
            let Some(wall) = self.wall_around(&mesh) else {
                why = Some("has no opening relationship and sits in no native wall".into());
                return None;
            };
            let r = opening_in_wall(&wall, &mesh, Some(overall))?;
            Some((wall, r))
        });
        let Some((wall, r)) = placed else {
            let why = why.unwrap_or_else(|| "could not be measured in its wall".into());
            self.mesh_reason
                .insert(id, format!("{why}; imported as a mesh"));
            self.import_free_mesh(id);
            return;
        };
        if opening.is_none() {
            let wall_ifc = self
                .walls
                .iter()
                .find(|(_, w)| w.iter().any(|w| w.id == wall.id))
                .map(|(i, _)| *i);
            let host = wall_ifc.map_or_else(|| "a wall".into(), |w| self.label(w));
            self.note(
                id,
                "doors and windows with no opening relationship were placed in the wall they sit in",
                format!("has no IfcRelFillsElement; placed in {host}"),
            );
        }
        let (offset, sill, width, height) = self.snap_to_void(&wall, r);
        if offset < -0.01 || sill < -0.01 {
            self.note(
                id,
                "doors and windows reaching past their wall's start or base were clipped to it",
                "reaches past its wall's start or base; clipped to it",
            );
        }
        self.done.insert(id);
        let material = self.material_for(id);
        let e = self.f.get(id).expect("checked above");
        let eid = self.new_id(e);
        let fallback = if kind == OpeningKind::Door {
            "Door"
        } else {
            "Window"
        };
        self.opening_elems.push(Element {
            mark: None,
            source: None,
            of_type: self.type_ref(id),
            id: eid,
            name: Self::name(e, fallback),
            kind: ElementKind::Opening(Opening {
                host: wall.id,
                kind,
                offset: offset.max(0.0),
                sill: sill.max(0.0),
                width,
                height,
                material,
                frame: Default::default(),
                flip_hand: false,
                flip_facing: false,
            }),
        });
    }

    fn import_free_mesh(&mut self, id: u64) {
        if !self.done.insert(id) {
            return;
        }
        let Some(e) = self.f.get(id) else { return };
        let class = product_class(&e.class).unwrap_or("IfcBuildingElementProxy");
        let world = self.g.placement(e.arg(5));
        let mut mesh = self.g.product_mesh(e, &world);
        if mesh.faces.is_empty() {
            let has_parts = self.has_parts.contains(&id);
            if !has_parts && !e.arg(6).is_null() && class != "IfcSite" {
                self.note(
                    id,
                    "elements with no geometry CreaBIM can read were skipped",
                    "has no geometry CreaBIM can read; skipped",
                );
            }
            return;
        }
        weld(&mut mesh, 1e-6);
        merge_coplanar(&mut mesh);
        let Some((lo, _)) = bounds(mesh.vertices.iter().copied()) else {
            return;
        };
        let (lid, elev) = self.level_for(id, lo.z);
        if let Some(why) = self.mesh_reason.remove(&id) {
            let group = if class.starts_with("IfcWall") {
                "walls that are not straight upright prisms were imported as meshes"
            } else if class.starts_with("IfcSlab") {
                "slabs that are not flat upright prisms were imported as meshes"
            } else if class.starts_with("IfcDoor") || class.starts_with("IfcWindow") {
                "doors and windows not in a native wall were imported as meshes"
            } else {
                "columns and beams that are not straight prisms were imported as meshes"
            };
            self.note(id, group, why);
        }
        if self.voided.contains(&id) {
            self.cut_openings(id, &mut mesh);
        }
        // vertices relative to the element's own placement, as authored
        let at = world.w_axis.truncate();
        for v in &mut mesh.vertices {
            *v -= at;
        }
        let material = self.material_for(id);
        let eid = self.new_id(e);
        self.other_elems.push(Element {
            mark: None,
            source: None,
            id: eid,
            name: Self::name(e, class),
            kind: ElementKind::FreeMesh(FreeMesh {
                ifc_class: class.to_string(),
                material,
                ..FreeMesh::new(
                    Some(lid),
                    DVec3::new(at.x, at.y, at.z - elev),
                    EditMesh {
                        vertices: mesh.vertices,
                        faces: mesh.faces,
                    },
                )
            }),
            of_type: None,
        });
    }

    /// Cut an element's openings (IfcOpeningElement through
    /// IfcRelVoidsElement) out of its mesh, noting any that can't be.
    fn cut_openings(&mut self, id: u64, mesh: &mut EditMesh) {
        use creabim_core::csg::{boolean, BoolOp, Polygon};
        const GROUP: &str = "openings that could not be cut into a mesh";
        let mut openings: Vec<u64> = self
            .voids
            .iter()
            .filter(|(_, h)| **h == id)
            .map(|(o, _)| *o)
            .collect();
        openings.sort_unstable();
        // work near the origin, where the cut is most precise
        let Some((origin, _)) = bounds(mesh.vertices.iter().copied()) else {
            return;
        };
        let polygons = |m: &EditMesh| -> Vec<Polygon> {
            (0..m.faces.len())
                .flat_map(|f| m.face_triangles(f))
                .filter_map(|t| {
                    Polygon::new(t.iter().map(|&v| m.vertices[v as usize] - origin).collect())
                })
                .collect()
        };
        let mut solid = polygons(mesh);
        let mut changed = false;
        for oid in openings {
            let Some(o) = self.f.get(oid) else { continue };
            let ow = self.g.placement(o.arg(5));
            let mut om = self.g.product_mesh(o, &ow);
            if om.faces.is_empty() {
                self.note(
                    oid,
                    GROUP,
                    format!("has no shape to cut into {}", self.label(id)),
                );
                continue;
            }
            weld(&mut om, 1e-6);
            let before = volume(&solid);
            let cut = boolean(solid.clone(), polygons(&om), BoolOp::Difference);
            let after = volume(&cut);
            if cut.is_empty() || after > before + 1e-6 || after < 1e-9 {
                self.note(
                    oid,
                    GROUP,
                    format!("could not be cut into {}", self.label(id)),
                );
                continue;
            }
            // a body exported with its holes already cut loses nothing
            if before - after > 1e-6 {
                solid = cut;
                changed = true;
            }
        }
        if !changed {
            return;
        }
        let mut out = EditMesh::default();
        for p in &solid {
            let base = out.vertices.len() as u32;
            out.vertices.extend(p.points.iter().map(|v| *v + origin));
            out.faces
                .push((0..p.points.len() as u32).map(|i| base + i).collect());
        }
        weld(&mut out, 1e-6);
        merge_coplanar(&mut out);
        *mesh = out;
    }
}

/// The wall nearest a body's centre whose thickness (give or take `slack`)
/// and height hold it.
fn nearest_wall(walls: Vec<&HostWall>, mesh: &EditMesh, slack: f64) -> Option<HostWall> {
    let (lo, hi) = bounds(mesh.vertices.iter().copied())?;
    let c = (lo + hi) / 2.0;
    let across = |w: &HostWall| {
        let d = (w.end - w.start).normalize_or_zero();
        (c.truncate() - w.start).perp_dot(d).abs()
    };
    walls
        .into_iter()
        .filter(|w| {
            let d = (w.end - w.start).normalize_or_zero();
            let u = (c.truncate() - w.start).dot(d);
            across(w) <= w.thickness / 2.0 + slack
                && u >= 0.0
                && u <= w.start.distance(w.end)
                && c.z >= w.base_z - 0.01
                && c.z <= w.top_z + 0.01
        })
        .min_by(|a, b| across(a).total_cmp(&across(b)))
        .copied()
}

/// The volume a set of outward polygons encloses.
fn volume(polygons: &[creabim_core::csg::Polygon]) -> f64 {
    let mut v = 0.0;
    for p in polygons {
        let q = &p.points;
        for i in 1..q.len().saturating_sub(1) {
            v += q[0].dot(q[i].cross(q[i + 1])) / 6.0;
        }
    }
    v
}

/// Recognise a vertical prism in a closed mesh: every vertex at the bottom or
/// the top, and the top faces outlining one loop.
pub(crate) fn prism_of_mesh(mut mesh: EditMesh) -> Option<Prism> {
    weld(&mut mesh, 1e-6);
    let (lo, hi) = bounds(mesh.vertices.iter().copied())?;
    let (z0, z1) = (lo.z, hi.z);
    if z1 - z0 < 1e-4
        || mesh
            .vertices
            .iter()
            .any(|v| (v.z - z0).abs() > 1e-5 && (v.z - z1).abs() > 1e-5)
    {
        return None;
    }
    // the outline of the top faces must be one loop
    let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
    for f in &mesh.faces {
        if f.iter()
            .all(|&v| (mesh.vertices[v as usize].z - z1).abs() <= 1e-5)
        {
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                if edges.remove(&(b, a)).is_none() {
                    *edges.entry((a, b)).or_insert(0) += 1;
                }
            }
        }
    }
    let next: HashMap<u32, u32> = edges.keys().map(|&(a, b)| (a, b)).collect();
    if next.len() != edges.len() || next.len() < 3 {
        return None;
    }
    let start = *next.keys().min()?;
    let mut lp = vec![start];
    let mut cur = start;
    while let Some(&n) = next.get(&cur) {
        if n == start {
            break;
        }
        if lp.len() > next.len() {
            return None;
        }
        lp.push(n);
        cur = n;
    }
    if lp.len() != next.len() {
        return None; // several loops: holes or separate pieces
    }
    let mut plan: Vec<DVec2> = lp
        .iter()
        .map(|&v| mesh.vertices[v as usize].truncate())
        .collect();
    if area2(&plan) < 0.0 {
        plan.reverse();
    }
    let volume = area2(&plan) / 2.0 * (z1 - z0);
    if (mesh.volume().abs() - volume).abs() > 1e-3 * volume.max(1e-6) {
        return None;
    }
    Some(Prism {
        footprint: remove_collinear(&plan),
        z0,
        z1,
        holes: vec![],
        clipped: false,
    })
}

/// A straight wall's centreline and thickness from its plan footprint: all
/// corners must lie on two lines parallel to the wall, whatever its ends do
/// (square, mitred or trimmed at a T). The axis, when given, sets the
/// direction and the ends.
pub(crate) fn wall_from_footprint(
    fp: &[DVec2],
    axis: Option<(DVec2, DVec2)>,
) -> Option<(DVec2, DVec2, f64)> {
    if fp.len() < 4 {
        return None;
    }
    let (origin, d) = match axis {
        Some((a, b)) if a.distance(b) > 1e-6 => (a, (b - a).normalize()),
        _ => {
            let (i, _) = (0..fp.len())
                .map(|i| (i, fp[i].distance(fp[(i + 1) % fp.len()])))
                .max_by(|a, b| a.1.total_cmp(&b.1))?;
            (fp[i], (fp[(i + 1) % fp.len()] - fp[i]).normalize_or_zero())
        }
    };
    wall_from_points(fp, origin, d, axis)
}

/// A straight wall's centreline and thickness from corners that must lie
/// on two lines running `d` (through `origin` or parallel to it).
pub(crate) fn wall_from_points(
    fp: &[DVec2],
    origin: DVec2,
    d: DVec2,
    axis: Option<(DVec2, DVec2)>,
) -> Option<(DVec2, DVec2, f64)> {
    if fp.len() < 4 || d == DVec2::ZERO {
        return None;
    }
    let n = d.perp();
    let s: Vec<f64> = fp.iter().map(|p| (*p - origin).dot(n)).collect();
    let smin = s.iter().copied().fold(f64::INFINITY, f64::min);
    let smax = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let tol = 1e-3;
    let thickness = smax - smin;
    if thickness < 2.0 * tol
        || s.iter()
            .any(|&v| (v - smin).abs() > tol && (v - smax).abs() > tol)
    {
        return None;
    }
    let centre = (smin + smax) / 2.0;
    if let Some((a, b)) = axis.filter(|(a, b)| a.distance(*b) > 1e-6) {
        let shift = n * ((a - origin).dot(n) - centre);
        return Some((a - shift, b - shift, thickness));
    }
    // the centreline ends halfway between the two faces' ends, which is
    // exact for square and mitred ends
    let mut ranges = [(f64::INFINITY, f64::NEG_INFINITY); 2];
    for (p, &sv) in fp.iter().zip(&s) {
        let k = usize::from((sv - smin).abs() > tol);
        let u = (*p - origin).dot(d);
        ranges[k] = (ranges[k].0.min(u), ranges[k].1.max(u));
    }
    let u0 = (ranges[0].0 + ranges[1].0) / 2.0;
    let u1 = (ranges[0].1 + ranges[1].1) / 2.0;
    if u1 - u0 <= 1e-3 {
        return None;
    }
    let base = origin + n * centre;
    Some((base + d * u0, base + d * u1, thickness))
}

/// Offset along the wall, sill above its base, width and height of a hole
/// measured from a mesh. With `overall` (width, height), those sizes win and
/// the opening is centred on the mesh.
pub(crate) fn opening_in_wall(
    wall: &HostWall,
    mesh: &EditMesh,
    overall: Option<(Option<f64>, Option<f64>)>,
) -> Option<(f64, f64, f64, f64)> {
    let d = (wall.end - wall.start).normalize_or_zero();
    if d == DVec2::ZERO {
        return None;
    }
    let mut u = (f64::INFINITY, f64::NEG_INFINITY);
    let mut z = (f64::INFINITY, f64::NEG_INFINITY);
    for v in &mesh.vertices {
        let t = (v.truncate() - wall.start).dot(d);
        u = (u.0.min(t), u.1.max(t));
        z = (z.0.min(v.z), z.1.max(v.z));
    }
    if !(u.1 - u.0 > 1e-3 && z.1 - z.0 > 1e-3) {
        return None;
    }
    let (mut width, mut height) = (u.1 - u.0, z.1 - z.0);
    let (mut offset, sill) = (u.0, z.0 - wall.base_z);
    if let Some((w, h)) = overall {
        if let Some(w) = w.filter(|w| *w > 0.0) {
            offset += (width - w) / 2.0;
            width = w;
        }
        if let Some(h) = h.filter(|h| *h > 0.0) {
            height = h;
        }
    }
    Some((offset, sill, width, height))
}

use brep::remove_collinear;

/// Metres per length unit and radians per angle unit, from the project's units.
fn units(f: &StepFile, by_class: &HashMap<&str, Vec<u64>>) -> (f64, f64, Option<String>) {
    let mut len = None;
    let mut angle = None;
    let project = by_class
        .get("IFCPROJECT")
        .and_then(|v| v.first())
        .and_then(|id| f.get(*id));
    let assignment = project.and_then(|p| f.deref(p.arg(8)));
    for u in assignment.map(|a| a.arg(0).as_list()).unwrap_or(&[]) {
        let Some(e) = f.deref(u) else { continue };
        match (e.arg(1).as_enum(), unit_factor(f, e, 0)) {
            (Some("LENGTHUNIT"), Some(k)) => len = len.or(Some(k)),
            (Some("PLANEANGLEUNIT"), Some(k)) => angle = angle.or(Some(k)),
            _ => {}
        }
    }
    let note = len
        .is_none()
        .then(|| "the file declares no length unit; metres were assumed".to_string());
    (len.unwrap_or(1.0), angle.unwrap_or(1.0), note)
}

/// The size of a unit in SI base units (metres, radians).
fn unit_factor(f: &StepFile, e: &Entity, depth: usize) -> Option<f64> {
    if depth > 8 {
        return None;
    }
    match e.class.as_str() {
        "IFCSIUNIT" => {
            let prefix = match e.arg(2).as_enum() {
                None => 1.0,
                Some("EXA") => 1e18,
                Some("PETA") => 1e15,
                Some("TERA") => 1e12,
                Some("GIGA") => 1e9,
                Some("MEGA") => 1e6,
                Some("KILO") => 1e3,
                Some("HECTO") => 1e2,
                Some("DECA") => 1e1,
                Some("DECI") => 1e-1,
                Some("CENTI") => 1e-2,
                Some("MILLI") => 1e-3,
                Some("MICRO") => 1e-6,
                Some("NANO") => 1e-9,
                Some("PICO") => 1e-12,
                Some(_) => 1.0,
            };
            Some(prefix)
        }
        "IFCCONVERSIONBASEDUNIT" | "IFCCONVERSIONBASEDUNITWITHOFFSET" => {
            let m = f.deref(e.arg(3))?;
            let value = m.arg(0).as_f64()?;
            let base = f
                .deref(m.arg(1))
                .and_then(|u| unit_factor(f, u, depth + 1))
                .unwrap_or(1.0);
            let k = value * base;
            (k.is_finite() && k > 0.0).then_some(k)
        }
        _ => None,
    }
}

/// How a surface style says a surface looks: its colour, and when the
/// style is a full rendering, how see-through, rough and metallic it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Surface {
    pub color: [f32; 3],
    /// 0 opaque .. 1 invisible.
    pub transparency: f32,
    pub roughness: Option<f32>,
    pub metallic: Option<f32>,
}

impl Surface {
    /// The material with this look, and whether the style said anything
    /// about roughness or metal (else the caller guesses from the name).
    pub fn apply(&self, m: &mut Material) -> bool {
        m.color = self.color;
        m.opacity = (1.0 - self.transparency).clamp(0.05, 1.0);
        if let Some(r) = self.roughness {
            m.roughness = r.clamp(0.0, 1.0);
        }
        if let Some(x) = self.metallic {
            m.metallic = x.clamp(0.0, 1.0);
        }
        self.roughness.is_some() || self.metallic.is_some()
    }
}

/// The surface of a list of presentation styles (IfcSurfaceStyleShading
/// or IfcSurfaceStyleRendering, read through style assignments).
///
/// A rendering's `SpecularHighlight` is used when it is an
/// `IfcSpecularRoughness`; `IfcSpecularExponent` is ignored, since some
/// exporters write the same exponent for every material. A `METAL`
/// reflectance method makes the surface metallic.
pub(crate) fn style_surface(f: &StepFile, styles: &Value) -> Option<Surface> {
    for s in styles.as_list() {
        let Some(e) = f.deref(s) else { continue };
        let found = match e.class.as_str() {
            "IFCPRESENTATIONSTYLEASSIGNMENT" => style_surface(f, e.arg(0)),
            "IFCSURFACESTYLE" => e.arg(2).as_list().iter().find_map(|r| {
                let r = f.deref(r)?;
                let rendering = r.class.starts_with("IFCSURFACESTYLERENDERING");
                if !r.class.starts_with("IFCSURFACESTYLESHADING") && !rendering {
                    return None;
                }
                let c = f.deref(r.arg(0))?;
                let ch = |i: usize| c.arg(i).as_f64().map(|v| v.clamp(0.0, 1.0) as f32);
                let mut out = Surface {
                    color: [ch(1)?, ch(2)?, ch(3)?],
                    transparency: r
                        .arg(1)
                        .as_f64()
                        .filter(|t| t.is_finite())
                        .map_or(0.0, |t| t.clamp(0.0, 1.0) as f32),
                    roughness: None,
                    metallic: None,
                };
                if rendering {
                    if let Value::Typed(t, v) = r.arg(7) {
                        if t == "IFCSPECULARROUGHNESS" {
                            out.roughness = v.as_f64().map(|v| v.clamp(0.0, 1.0) as f32);
                        }
                    }
                    match r.arg(8).as_enum() {
                        Some("METAL") => out.metallic = Some(1.0),
                        Some("GLASS" | "MIRROR") => {
                            out.roughness = out.roughness.or(Some(0.05));
                            out.metallic = Some(0.0);
                        }
                        Some("MATT") => out.roughness = out.roughness.or(Some(1.0)),
                        Some("PLASTIC") => out.roughness = out.roughness.or(Some(0.4)),
                        _ => {}
                    }
                }
                Some(out)
            }),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// A CreaBIM material from an IfcMaterial (or a layer set's name and its
/// main IfcMaterial) and the surface style found for it.
fn material_from(
    f: &StepFile,
    name: &str,
    mat_id: Option<u64>,
    surface: Option<Surface>,
) -> Material {
    let (description, category) = mat_id
        .and_then(|m| f.get(m))
        .filter(|m| m.is("IFCMATERIAL"))
        .map(|m| {
            (
                m.arg(1).as_str().unwrap_or(""),
                m.arg(2).as_str().unwrap_or(""),
            )
        })
        .unwrap_or(("", ""));
    let (_, plain) = split_description(description);
    let mut m = Material {
        hatch: hatch_for(name, description, category),
        color: [0.75, 0.75, 0.75],
        category: category.trim().to_lowercase(),
        description: plain,
        ..Default::default()
    };
    let has_surface = surface.is_some_and(|s| s.apply(&mut m));
    creabim_core::library::imported(name, m, has_surface)
}

/// The hatch CreaBIM wrote at the start of a material's description, and
/// the rest of the description.
pub(crate) fn split_description(description: &str) -> (Option<Hatch>, String) {
    let Some(rest) = description.strip_prefix(crate::export::HATCH_PREFIX) else {
        return (None, description.trim().to_string());
    };
    let (label, plain) = rest.split_once(". ").unwrap_or((rest, ""));
    let hatch = Hatch::ALL.into_iter().find(|h| h.label() == label.trim());
    (hatch, plain.trim().to_string())
}

/// The hatch CreaBIM wrote, or a guess from the material's name and category.
pub(crate) fn hatch_for(name: &str, description: &str, category: &str) -> Hatch {
    if let (Some(h), _) = split_description(description) {
        return h;
    }
    let text = format!("{name} {category}").to_lowercase();
    if ["brick", "masonry", "block"]
        .iter()
        .any(|k| text.contains(k))
    {
        Hatch::Brick
    } else if ["concrete", "beton", "cement"]
        .iter()
        .any(|k| text.contains(k))
    {
        Hatch::Concrete
    } else if ["wood", "timber", "oak", "pine", "lumber"]
        .iter()
        .any(|k| text.contains(k))
    {
        Hatch::Timber
    } else if ["insulation", "gypsum", "plaster", "stud"]
        .iter()
        .any(|k| text.contains(k))
    {
        Hatch::Cross
    } else {
        Hatch::Solid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mitred_footprint_gives_the_centreline() {
        // a 10 m wall, 0.3 thick, mitred at both ends as at two L corners
        let fp = [
            DVec2::new(-0.15, 0.15),
            DVec2::new(10.15, 0.15),
            DVec2::new(9.85, -0.15),
            DVec2::new(0.15, -0.15),
        ];
        let (a, b, t) = wall_from_footprint(&fp, None).unwrap();
        assert!((t - 0.3).abs() < 1e-9);
        let (a, b) = if a.x < b.x { (a, b) } else { (b, a) };
        assert!(a.distance(DVec2::ZERO) < 1e-9, "{a}");
        assert!(b.distance(DVec2::new(10.0, 0.0)) < 1e-9, "{b}");
        // an axis on the outer face moves to the centre
        let (a, b, _) =
            wall_from_footprint(&fp, Some((DVec2::new(0.0, 0.15), DVec2::new(10.0, 0.15))))
                .unwrap();
        assert!(a.distance(DVec2::ZERO) < 1e-9 && b.distance(DVec2::new(10.0, 0.0)) < 1e-9);
        // an L-shaped footprint is not a wall
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        assert!(wall_from_footprint(&l, None).is_none());
    }

    #[test]
    fn hatches_round_trip_and_are_guessed() {
        let d = format!("{}Diagonal", crate::export::HATCH_PREFIX);
        assert_eq!(hatch_for("x", &d, ""), Hatch::Diagonal);
        assert_eq!(hatch_for("Concrete C30/37", "", ""), Hatch::Concrete);
        assert_eq!(hatch_for("Paint", "", ""), Hatch::Solid);
    }
}
