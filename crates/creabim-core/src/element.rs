pub use crate::annotation::{DimAxis, DimensionKind, Note, Terminator, TextStyle};
pub use crate::grids::GridGroup;
pub use crate::members::{Beam, Column, Railing, Roof, Stair};
pub use crate::sketch::Sketch;
pub use crate::titleblock::SheetTemplate;
use crate::visibility::ViewHidden;
use crate::{
    editmesh::EditMesh,
    modifiers::{Evaluated, Modifier},
    sketch::Curve,
    ElementId,
};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One object in the building model. Lengths are in metres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: ElementId,
    pub name: String,
    pub kind: ElementKind,
    /// Where the element came from, for elements read from another file
    /// (a Blender object, a FreeCAD object, an IFC entity). Reloading that
    /// file updates the element in place instead of adding a copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// The element type it is an instance of (a wall type, a door type...),
    /// and which of the type's parameters it sets for itself.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub of_type: Option<crate::types::TypeRef>,
    /// The mark typed on it (a door's "D-101"), shown by its tags and
    /// schedules; none takes the number the project's rules give it (see
    /// [`crate::marks`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
}

/// An object in another file that an element mirrors.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Source {
    /// The file's path, as it was opened.
    pub file: String,
    /// The object's identity in that file: its name, or its GlobalId.
    pub object: String,
    /// A fingerprint of the object as last read, so a reload can tell
    /// whether it changed there (and leave edits made here alone if not).
    #[serde(default)]
    pub stamp: u64,
}

impl Element {
    pub fn new(name: impl Into<String>, kind: ElementKind) -> Self {
        Self {
            id: ElementId::new(),
            name: name.into(),
            kind,
            source: None,
            of_type: None,
            mark: None,
        }
    }

    /// The same element, marked as mirroring `object` in `file`.
    pub fn from_source(mut self, file: impl Into<String>, object: impl Into<String>) -> Self {
        self.source = Some(Source {
            file: file.into(),
            object: object.into(),
            stamp: 0,
        });
        self
    }

    /// Elements this element depends on. The model refuses edits that would
    /// leave any of these missing.
    pub fn references(&self) -> Vec<ElementId> {
        match &self.kind {
            ElementKind::Level(_) | ElementKind::Grid(_) | ElementKind::GridGroup(_) => vec![],
            // a top level and attached roofs are soft: when one goes the wall
            // keeps the height it had
            ElementKind::Wall(w) => vec![w.level],
            ElementKind::Slab(s) => vec![s.level],
            // a missing material falls back to the element type's default,
            // so materials are not hard references
            ElementKind::Material(_) => vec![],
            // so are assemblies, and the materials of their layers
            ElementKind::Assembly(_) => vec![],
            ElementKind::Opening(o) => vec![o.host],
            ElementKind::FreeMesh(m) => m.level.into_iter().collect(),
            ElementKind::View(v) => match &v.kind {
                ViewKind::Plan { level, .. } => vec![*level],
                ViewKind::Section { .. }
                | ViewKind::Axonometric { .. }
                | ViewKind::Schedule { .. } => vec![],
            },
            ElementKind::Sheet(_) => vec![],
            // a 3D view's crop boxes go with it; the elements they crop are soft
            ElementKind::CropBox(c) => c.view.into_iter().collect(),
            ElementKind::Viewport(v) => vec![v.sheet, v.view],
            ElementKind::Dimension(d) => std::iter::once(d.view)
                .chain(d.a.host)
                .chain(d.b.host)
                .chain(d.center.and_then(|c| c.host))
                .collect(),
            ElementKind::Tag(t) => vec![t.view, t.host],
            ElementKind::Note(n) => std::iter::once(n.view)
                .chain(n.leader.and_then(|p| p.host))
                .collect(),
            // a missing style falls back to the default look, like materials
            ElementKind::TextStyle(_) => vec![],
            ElementKind::SheetTemplate(_) => vec![],
            // a reference's view is soft: without it, it shows nothing
            ElementKind::Reference(_) => vec![],
            ElementKind::PaperMark(m) => vec![m.sheet],
            ElementKind::Units(_)
            | ElementKind::Project(_)
            | ElementKind::MarkRules(_)
            | ElementKind::StylePack(_)
            | ElementKind::ScriptRegistry(_) => vec![],
            // types are soft too: an element whose type is gone keeps its values
            ElementKind::ElementType(_) => vec![],
            // a shaft's top level is soft: without it the shaft stops at
            // its own level's height
            ElementKind::Shaft(s) => vec![s.level],
            // an object's shape is its type's: without it there is nothing
            // to show, so deleting a type deletes its objects
            ElementKind::Object(o) => std::iter::once(o.level)
                .chain(self.of_type.as_ref().map(|t| t.id))
                .collect(),
            ElementKind::Sketch(s) => s.level.into_iter().collect(),
            // a footing goes with the wall or column it is under
            ElementKind::Footing(f) => std::iter::once(f.level).chain(f.host).collect(),
            // a sweep along a wall goes with the wall
            ElementKind::Part(p) => std::iter::once(p.level).chain(p.host()).collect(),
            ElementKind::Room(r) => vec![r.level],
            ElementKind::RoomSeparator(s) => vec![s.level],
            // a ceiling's room is soft: without it the ceiling has no
            // outline to show until it is given one
            ElementKind::Ceiling(c) => vec![c.level],
            ElementKind::Cut(c) => vec![c.target, c.cutter],
            ElementKind::Join(j) => vec![j.a, j.b],
            ElementKind::Detail(d) => vec![d.view],
            // a script block references whatever its element inputs name
            ElementKind::ScriptObject(s) => s
                .inputs
                .iter()
                .filter_map(|i| match &i.value {
                    crate::script::ParamValue::Element(id) => Some(*id),
                    _ => None,
                })
                .collect(),
            ElementKind::Guide(g) => g.level.into_iter().collect(),
            ElementKind::Railing(r) => std::iter::once(r.level)
                .chain(r.stair.map(|s| s.stair))
                .collect(),
            k => crate::members::level_of(k).into_iter().collect(),
        }
    }

    /// The IFC entity this element exports as.
    pub fn ifc_class(&self) -> &str {
        match &self.kind {
            ElementKind::Level(_) => "IfcBuildingStorey",
            ElementKind::Grid(_) | ElementKind::GridGroup(_) => "IfcGrid",
            ElementKind::Wall(w) if w.curtain.is_some() => "IfcCurtainWall",
            ElementKind::Wall(_) => "IfcWall",
            ElementKind::Slab(_) => "IfcSlab",
            ElementKind::Opening(o) => match o.kind {
                OpeningKind::Door => "IfcDoor",
                OpeningKind::Window => "IfcWindow",
                OpeningKind::Hole => "IfcOpeningElement",
            },
            ElementKind::Shaft(_) => "IfcOpeningElement",
            ElementKind::Object(_) => "IfcFurniture",
            ElementKind::ElementType(t) => t.ifc_class(),
            ElementKind::FreeMesh(m) => &m.ifc_class,
            // Views are documentation, not building elements; they are not exported to IFC.
            ElementKind::View(_) => "View",
            ElementKind::Sheet(_) => "Sheet",
            ElementKind::Viewport(_) => "Viewport",
            ElementKind::Material(_) => "IfcMaterial",
            ElementKind::Assembly(_) => "IfcMaterialLayerSet",
            ElementKind::Dimension(_) => "Dimension",
            ElementKind::ScriptObject(_) => "Script",
            ElementKind::ScriptRegistry(_) => "ScriptRegistry",
            ElementKind::Tag(_) => "Tag",
            ElementKind::Note(_) => "Note",
            ElementKind::TextStyle(_) => "TextStyle",
            ElementKind::SheetTemplate(_) => "SheetTemplate",
            ElementKind::Units(_) => "ProjectUnits",
            ElementKind::Project(_) => "ProjectInfo",
            ElementKind::Reference(_) => "Reference",
            ElementKind::PaperMark(_) => "IfcAnnotation",
            ElementKind::MarkRules(_) => "MarkRules",
            ElementKind::StylePack(_) => "StylePack",
            ElementKind::CropBox(_) => "CropBox",
            ElementKind::Sketch(_) => "Sketch",
            ElementKind::Cut(_) => "Cut",
            // drawing rules and drafting, not building elements
            ElementKind::Join(_) => "Join",
            ElementKind::Detail(_) => "DetailItem",
            ElementKind::Guide(_) => "Guide",
            ElementKind::Column(_) => "IfcColumn",
            ElementKind::Beam(_) => "IfcBeam",
            ElementKind::Roof(_) => "IfcRoof",
            ElementKind::Stair(_) => "IfcStair",
            ElementKind::Railing(_) => "IfcRailing",
            ElementKind::Footing(_) => "IfcFooting",
            ElementKind::Part(p) if p.void => "IfcOpeningElement",
            ElementKind::Part(p) => &p.ifc_class,
            ElementKind::Site(s) => match s.form {
                crate::SiteForm::Terrain { .. } => "IfcGeographicElement",
                crate::SiteForm::Area { .. } => "IfcCivilElement",
                crate::SiteForm::Plant { .. } => "IfcGeographicElement",
            },
            ElementKind::Room(_) => "IfcSpace",
            ElementKind::RoomSeparator(_) => "IfcVirtualElement",
            ElementKind::Ceiling(_) => "IfcCovering",
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let positive = |v: f64, what: &str| {
            if v.is_finite() && v > 0.0 {
                Ok(())
            } else {
                Err(format!("{what} must be positive, got {v}"))
            }
        };
        match &self.kind {
            ElementKind::Level(l) => positive(l.height, "level height"),
            ElementKind::Grid(g) => {
                if g.start.distance(g.end) > 1e-6 {
                    Ok(())
                } else {
                    Err("grid line has zero length".into())
                }
            }
            ElementKind::GridGroup(g) => {
                if g.color.iter().all(|c| (0.0..=1.0).contains(c)) {
                    Ok(())
                } else {
                    Err("grid group colour components must be within 0..1".into())
                }
            }
            ElementKind::Wall(w) => {
                if let Some(c) = &w.curtain {
                    c.validate()?;
                }
                positive(w.thickness, "wall thickness")?;
                positive(w.height, "wall height")?;
                if let WallTop::UpToLevel { offset, .. } = w.top {
                    if !offset.is_finite() {
                        return Err("wall top offset must be a number".into());
                    }
                }
                if w.attach_top.contains(&self.id) {
                    return Err("a wall can't be attached to itself".into());
                }
                positive(w.length(), "wall length")
            }
            ElementKind::Slab(s) => {
                positive(s.thickness, "slab thickness")?;
                if !s.top_offset.is_finite() || s.bulges.iter().any(|b| !b.is_finite()) {
                    return Err("slab top offset and arcs must be finite".into());
                }
                let arcs = s.bulges.iter().any(|b| b.abs() > 1e-9);
                if s.boundary.len() < 3 && !(arcs && s.boundary.len() == 2) {
                    return Err("slab boundary needs at least 3 points, or 2 and an arc".into());
                }
                crate::slab::check_holes(&s.holes)
            }
            ElementKind::Opening(o) => {
                positive(o.width, "opening width")?;
                positive(o.height, "opening height")?;
                if o.sill < 0.0 || o.offset < 0.0 {
                    return Err("opening sill and offset must not be negative".into());
                }
                Ok(())
            }
            ElementKind::FreeMesh(m) => {
                m.mesh.validate()?;
                if !(m.origin.is_finite() && m.rotation.is_finite()) {
                    return Err("mass position must be finite".into());
                }
                if !m.scale.is_finite() || m.scale.abs() < 1e-6 {
                    return Err("mass scale must not be zero".into());
                }
                m.modifiers.iter().try_for_each(Modifier::validate)
            }
            ElementKind::View(v) => {
                positive(v.scale, "view scale")?;
                match &v.kind {
                    ViewKind::Plan { cut_height, .. } => positive(*cut_height, "cut height"),
                    ViewKind::Section {
                        start, end, depth, ..
                    } => {
                        positive(*depth, "view depth")?;
                        positive(start.distance(*end), "section line length")
                    }
                    ViewKind::Axonometric { altitude, .. } => {
                        if altitude.is_finite() && altitude.abs() <= 90.0 {
                            Ok(())
                        } else {
                            Err(format!("view altitude must be within ±90°, got {altitude}"))
                        }
                    }
                    ViewKind::Schedule { .. } => Ok(()),
                }
            }
            ElementKind::Sheet(sh) => {
                positive(sh.width, "sheet width")?;
                positive(sh.height, "sheet height")
            }
            ElementKind::Viewport(v) => {
                if !v.center.is_finite() || !v.target.is_finite() {
                    return Err("viewport position must be finite".into());
                }
                match v.crop {
                    Some([lo, hi]) if !(lo.is_finite() && hi.is_finite() && lo.cmplt(hi).all()) => {
                        Err("viewport crop must have a positive size".into())
                    }
                    _ => Ok(()),
                }
            }
            ElementKind::Material(m) => {
                if !m.color.iter().all(|c| (0.0..=1.0).contains(c)) {
                    Err("material colour components must be within 0..1".into())
                } else if ![m.roughness, m.metallic, m.opacity]
                    .iter()
                    .all(|c| (0.0..=1.0).contains(c))
                {
                    Err("material roughness, metallic and opacity must be within 0..1".into())
                } else {
                    Ok(())
                }
            }
            ElementKind::Dimension(d) => {
                if d.offset.is_finite()
                    && [Some(d.a), Some(d.b), d.center]
                        .iter()
                        .flatten()
                        .all(|p| p.local.is_finite() && p.body.is_none_or(|b| b.is_finite()))
                {
                    Ok(())
                } else {
                    Err("dimension points must be finite".into())
                }
            }
            ElementKind::Tag(t) => {
                if t.offset.is_finite()
                    && t.elbow.is_none_or(|p| p.is_finite())
                    && t.end.is_none_or(|p| p.is_finite())
                {
                    Ok(())
                } else {
                    Err("tag position must be finite".into())
                }
            }
            ElementKind::Note(n) => n.validate(),
            ElementKind::TextStyle(s) => s.validate(),
            ElementKind::SheetTemplate(t) => t.validate(),
            ElementKind::Units(_) | ElementKind::Project(_) => Ok(()),
            ElementKind::PaperMark(m) => {
                if m.at.is_finite() && m.size > 0.0 {
                    Ok(())
                } else {
                    Err("a paper mark needs a place and a size".into())
                }
            }
            ElementKind::Reference(r) => {
                if r.scale.is_finite()
                    && r.scale > 0.0
                    && r.at.is_finite()
                    && r.rotation.is_finite()
                {
                    Ok(())
                } else {
                    Err("a reference needs a finite place and a scale above zero".into())
                }
            }
            ElementKind::MarkRules(r) => r.validate(),
            ElementKind::StylePack(p) => p.validate(),
            ElementKind::CropBox(c) => c.validate(),
            ElementKind::Assembly(a) => a.validate(),
            ElementKind::ElementType(t) => t.validate(),
            ElementKind::Shaft(s) => s.validate(),
            ElementKind::Object(o) => o.validate(),
            ElementKind::Footing(f) => f.validate(),
            ElementKind::Part(p) => p.validate(),
            ElementKind::Site(s) => s.validate(),
            ElementKind::Room(r) => r.validate(),
            ElementKind::RoomSeparator(s) => s.validate(),
            ElementKind::Ceiling(c) => c.validate(),
            ElementKind::Sketch(s) => s.check(),
            ElementKind::Join(j) => {
                if j.a == j.b {
                    Err("an element can't join itself".into())
                } else {
                    Ok(())
                }
            }
            ElementKind::Detail(d) => d.validate(),
            ElementKind::ScriptObject(s) => s.validate(),
            ElementKind::ScriptRegistry(r) => r.validate(),
            ElementKind::Cut(c) => {
                if c.target == c.cutter {
                    Err("an element can't cut itself".into())
                } else {
                    Ok(())
                }
            }
            ElementKind::Guide(g) => {
                if g.a.is_finite() && g.b.is_finite() && g.a.distance(g.b) > 1e-6 {
                    Ok(())
                } else {
                    Err("a guide needs two different points".into())
                }
            }
            k @ (ElementKind::Column(_)
            | ElementKind::Beam(_)
            | ElementKind::Roof(_)
            | ElementKind::Stair(_)
            | ElementKind::Railing(_)) => crate::members::validate(k),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ElementKind {
    Level(Level),
    Grid(Grid),
    Wall(Wall),
    Slab(Slab),
    Opening(Opening),
    FreeMesh(FreeMesh),
    View(View),
    Sheet(Sheet),
    Viewport(Viewport),
    Material(Material),
    Dimension(Dimension),
    Tag(Tag),
    Sketch(Sketch),
    Note(Note),
    TextStyle(TextStyle),
    /// A named title block that sheets can share.
    SheetTemplate(SheetTemplate),
    Cut(Cut),
    Guide(Guide),
    Column(Column),
    Beam(Beam),
    Roof(Roof),
    Stair(Stair),
    Railing(Railing),
    GridGroup(GridGroup),
    /// A named layered build-up that walls, slabs or roofs are made of.
    Assembly(crate::assembly::Assembly),
    /// The project's units: one per project, read through [`crate::Model::units`].
    Units(crate::units::Units),
    /// The project's own information (at most one).
    Project(crate::project::ProjectInfo),
    /// Something traced over in views (Trace & Reference v2, MU19).
    Reference(crate::reference::Reference),
    /// A north arrow or scale bar on a sheet (MU22).
    PaperMark(crate::papermark::PaperMark),
    /// The project's mark numbering rules: at most one per project, read
    /// through [`crate::marks::rules`].
    MarkRules(crate::marks::MarkRules),
    /// A style pack: object styles, patterns, pens and view templates.
    /// The one marked default is what the project draws with, read through
    /// [`crate::styles::active_pack`].
    StylePack(Box<crate::styles::StylePack>),
    /// A box that crops a 3D view (Revit's section box).
    CropBox(crate::cropbox::CropBox),
    /// A named set of shared parameters (a wall type, a door type...) that
    /// elements are instances of: editing it changes all of them.
    ElementType(crate::types::ElementType),
    /// A vertical hole through every slab and roof between two levels.
    Shaft(crate::slab::Shaft),
    /// Furniture or a fixture: an instance of an object type, which holds
    /// its shape.
    Object(crate::objects::Object),
    /// A strip footing under a wall or a pad footing under a column.
    Footing(crate::footing::Footing),
    /// An extrusion or a sweep of a profile, solid or a void that cuts
    /// what it meets.
    Part(crate::parts::Part),
    /// Terrain, hard landscape or a plant.
    Site(crate::site::Site),
    /// A room (IfcSpace): the space the walls close off round a point, or
    /// an outline drawn for it.
    Room(crate::room::Room),
    /// A line that closes rooms off where there is no wall.
    RoomSeparator(crate::room::Separator),
    /// A ceiling (IfcCovering CEILING) over a room or an outline.
    Ceiling(crate::room::Ceiling),
    /// How two elements join in drawings, set by hand for that junction.
    Join(crate::joins::JoinRule),
    /// A detail component drawn in one view: a flashing, a membrane...
    Detail(crate::detail::DetailItem),
    /// A script block: source plus engine, inputs and outputs. The block
    /// itself draws nothing; its outputs are ordinary elements.
    ScriptObject(crate::script::ScriptObject),
    /// The script registry: the project's link table between script
    /// blocks and disc modules. One per model; blocks name an entry.
    ScriptRegistry(crate::script::ScriptRegistry),
}

impl ElementKind {
    /// The variant name, as it appears in saved files (`"Wall"`, `"Level"`...).
    pub fn name(&self) -> &'static str {
        match self {
            ElementKind::Level(_) => "Level",
            ElementKind::Grid(_) => "Grid",
            ElementKind::Wall(_) => "Wall",
            ElementKind::Slab(_) => "Slab",
            ElementKind::Opening(_) => "Opening",
            ElementKind::FreeMesh(_) => "FreeMesh",
            ElementKind::View(_) => "View",
            ElementKind::Sheet(_) => "Sheet",
            ElementKind::Viewport(_) => "Viewport",
            ElementKind::Material(_) => "Material",
            ElementKind::Dimension(_) => "Dimension",
            ElementKind::ScriptObject(_) => "Script",
            ElementKind::ScriptRegistry(_) => "ScriptRegistry",
            ElementKind::Tag(_) => "Tag",
            ElementKind::Sketch(_) => "Sketch",
            ElementKind::Cut(_) => "Cut",
            ElementKind::Guide(_) => "Guide",
            ElementKind::Column(_) => "Column",
            ElementKind::Beam(_) => "Beam",
            ElementKind::Roof(_) => "Roof",
            ElementKind::Stair(_) => "Stair",
            ElementKind::Railing(_) => "Railing",
            ElementKind::Note(_) => "Note",
            ElementKind::TextStyle(_) => "TextStyle",
            ElementKind::SheetTemplate(_) => "SheetTemplate",
            ElementKind::GridGroup(_) => "GridGroup",
            ElementKind::Assembly(_) => "Assembly",
            ElementKind::Units(_) => "Units",
            ElementKind::Project(_) => "Project information",
            ElementKind::Reference(_) => "Reference",
            ElementKind::PaperMark(m) => match m.kind {
                crate::papermark::PaperMarkKind::NorthArrow { .. } => "North arrow",
                crate::papermark::PaperMarkKind::ScaleBar { .. } => "Scale bar",
                crate::papermark::PaperMarkKind::Cloud { .. } => "Revision cloud",
                crate::papermark::PaperMarkKind::RevisionTable => "Revisions table",
                crate::papermark::PaperMarkKind::Picture { .. } => "Picture",
                crate::papermark::PaperMarkKind::Vector { .. } => "Vector picture",
            },
            ElementKind::MarkRules(_) => "MarkRules",
            ElementKind::StylePack(_) => "StylePack",
            ElementKind::CropBox(_) => "CropBox",
            ElementKind::ElementType(_) => "ElementType",
            ElementKind::Shaft(_) => "Shaft",
            ElementKind::Object(_) => "Object",
            ElementKind::Footing(_) => "Footing",
            ElementKind::Part(_) => "Part",
            ElementKind::Site(_) => "Site",
            ElementKind::Room(_) => "Room",
            ElementKind::RoomSeparator(_) => "RoomSeparator",
            ElementKind::Ceiling(_) => "Ceiling",
            ElementKind::Join(_) => "Join",
            ElementKind::Detail(_) => "Detail",
        }
    }
}

/// Pattern drawn on cut faces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hatch {
    /// Outline only.
    Empty,
    /// Plain grey fill (poché).
    #[default]
    Solid,
    /// 45° lines.
    Diagonal,
    /// Crossed 45° lines.
    Cross,
    /// Pairs of 45° lines, the usual masonry hatch.
    Brick,
    /// Stipple with sparse 45° lines.
    Concrete,
    /// Lines along the cut, like grain.
    Timber,
}

impl Hatch {
    pub const ALL: [Hatch; 7] = [
        Hatch::Empty,
        Hatch::Solid,
        Hatch::Diagonal,
        Hatch::Cross,
        Hatch::Brick,
        Hatch::Concrete,
        Hatch::Timber,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Hatch::Empty => "Empty",
            Hatch::Solid => "Solid",
            Hatch::Diagonal => "Diagonal",
            Hatch::Cross => "Cross",
            Hatch::Brick => "Brick",
            Hatch::Concrete => "Concrete",
            Hatch::Timber => "Timber",
        }
    }
}

/// What an element is made of: how it looks in 3D and how its cut faces are hatched.
///
/// Everything after `color` was added later and is optional in files, so
/// projects saved before it open unchanged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub hatch: Hatch,
    /// sRGB colour, 0..1, as picked in a colour chooser.
    pub color: [f32; 3],
    /// What kind of material this is, as IFC's `IfcMaterial.Category`
    /// (`concrete`, `brick`, `wood`, `steel`, `glass`...). Empty when unknown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// Free text, as `IfcMaterial.Description`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// 0 is mirror-smooth, 1 is fully matte (glTF's roughness).
    #[serde(
        default = "default_roughness",
        skip_serializing_if = "is_default_roughness"
    )]
    pub roughness: f32,
    /// 0 for dielectrics (almost everything), 1 for bare metal.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub metallic: f32,
    /// 1 is opaque; glass is around 0.3.
    #[serde(default = "one_f32", skip_serializing_if = "is_one")]
    pub opacity: f32,
    /// A hatch pattern of the project's style pack, by name, for its cut
    /// faces; none uses the built-in pattern `hatch` names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Its price for take-off, per m², m³, m or piece (see
    /// [`crate::takeoff`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<crate::takeoff::Rate>,
}

/// The roughness of a material that doesn't say: matte, like most
/// building materials.
pub const DEFAULT_ROUGHNESS: f32 = 0.8;

fn default_roughness() -> f32 {
    DEFAULT_ROUGHNESS
}
fn is_default_roughness(v: &f32) -> bool {
    *v == DEFAULT_ROUGHNESS
}
fn one_f32() -> f32 {
    1.0
}
fn is_one(v: &f32) -> bool {
    *v == 1.0
}
fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

impl Default for Material {
    fn default() -> Self {
        Self {
            hatch: Hatch::Solid,
            color: [0.8, 0.8, 0.8],
            category: String::new(),
            description: String::new(),
            roughness: DEFAULT_ROUGHNESS,
            metallic: 0.0,
            opacity: 1.0,
            pattern: None,
            rate: None,
        }
    }
}

impl Material {
    /// A plain material with a hatch and a colour.
    pub fn new(hatch: Hatch, color: [f32; 3]) -> Self {
        Self {
            hatch,
            color,
            ..Default::default()
        }
    }

    /// Roughness, metallic and opacity, as the 3D view takes them.
    pub fn surface(&self) -> [f32; 3] {
        [self.roughness, self.metallic, self.opacity]
    }

    /// The same material with every number in its range (for values read
    /// from other files): non-finite values take their defaults.
    pub fn clamped(mut self) -> Self {
        let unit = |v: f32, d: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
        self.color = self.color.map(|c| unit(c, 0.8));
        self.roughness = unit(self.roughness, DEFAULT_ROUGHNESS);
        self.metallic = unit(self.metallic, 0.0);
        self.opacity = unit(self.opacity, 1.0).max(0.05);
        self
    }
}

/// One end of a dimension. With a host, `local` is in the host's own
/// frame, so the dimension follows the host when it moves:
/// - wall: (distance along the wall, offset across it), measured from the
///   wall's end instead of its start when `from_end` is set;
/// - door or window: (distance from its left edge, offset across the wall);
/// - level: (drawing x, height above the level).
///
/// Any other host (a mass, slab, grid, sketch or guide, or a wall seen in a
/// section) keeps the point in `body` instead, in the host's own 3D axes,
/// so it follows the host when it moves or turns.
///
/// Without a host, `local` is a point in the view's drawing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostPoint {
    pub host: Option<ElementId>,
    pub local: DVec2,
    #[serde(default)]
    pub from_end: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<DVec3>,
}

impl HostPoint {
    pub fn free(p: DVec2) -> Self {
        Self {
            host: None,
            local: p,
            from_end: false,
            body: None,
        }
    }
}

/// A boolean between two elements, like IFC's IfcRelVoidsElement: `target`
/// is shown as `target op cutter` and the cutter itself is hidden (drawn as
/// a ghost in 3D, left out of drawings and exports). Deleting either
/// element deletes the cut.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cut {
    pub target: ElementId,
    pub cutter: ElementId,
    pub op: crate::csg::BoolOp,
}

/// A construction line, endless through `a` and `b`, to snap to. Guides are
/// drawn dashed in plans (on their level) and in 3D, never exported.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub level: Option<ElementId>,
    pub a: DVec3,
    pub b: DVec3,
}

/// A dimension in one view. A linear one (the default, and the only kind
/// older files have) measures from `a` to `b`, its line `offset` drawing
/// units to the left of a → b. The other kinds measure about `center`; see
/// [`DimensionKind`] for what `a`, `b` and `offset` mean for each.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub view: ElementId,
    pub a: HostPoint,
    pub b: HostPoint,
    pub offset: f64,
    #[serde(default, skip_serializing_if = "DimensionKind::is_linear")]
    pub kind: DimensionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<HostPoint>,
    /// A [`TextStyle`] element; none is the default look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ElementId>,
    /// Overrides the style's colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    /// A linear dimension can measure across or up the drawing only.
    #[serde(default, skip_serializing_if = "DimAxis::is_aligned")]
    pub axis: DimAxis,
    /// What a spot level reads (see [`crate::annotation::spot_text`]);
    /// none is just the elevation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Locked with the padlock: the spacing (metres) it keeps between its
    /// two hosts. Moving one moves the other (see [`crate::holds`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<f64>,
    /// Its own number format over its kind's in the style pack (A5-2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<crate::units::NumberFormat>,
}

impl Dimension {
    /// A linear dimension in the default style.
    pub fn linear(view: ElementId, a: HostPoint, b: HostPoint, offset: f64) -> Self {
        Self {
            view,
            a,
            b,
            offset,
            kind: DimensionKind::Linear,
            center: None,
            style: None,
            color: None,
            axis: DimAxis::Aligned,
            pattern: None,
            held: None,
            format: None,
        }
    }

    /// A spot level of the point `a`, written at `b`.
    pub fn spot(view: ElementId, a: HostPoint, b: HostPoint, pattern: Option<String>) -> Self {
        Self {
            kind: DimensionKind::Spot,
            pattern,
            ..Self::linear(view, a, b, 0.0)
        }
    }

    /// A dimension of `kind` about `center`, in the default style.
    pub fn about(
        kind: DimensionKind,
        view: ElementId,
        center: HostPoint,
        a: HostPoint,
        b: HostPoint,
        offset: f64,
    ) -> Self {
        Self {
            kind,
            center: Some(center),
            ..Self::linear(view, a, b, offset)
        }
    }
}

/// A label in one view showing a property of `host`, such as a door's mark.
/// It sits at `offset` from the host's centre, so it moves with the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub view: ElementId,
    pub host: ElementId,
    pub offset: DVec2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ElementId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    /// For a room's tag, what it shows, in order; none shows the name,
    /// number and area.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<crate::room::RoomField>,
    /// A bend in its leader, from the host's centre like `offset`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elbow: Option<DVec2>,
    /// Where its leader ends, from the host's centre; none is the centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<DVec2>,
    /// The mark at the end of its leader; none is its style's leader end,
    /// or a plain line when the style sets none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader_end: Option<Terminator>,
    /// A material tag (Annotate step 5): it reads its host's material, or
    /// its layers' build-up one line each, outside to inside, instead of
    /// its mark.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub material: bool,
}

impl Tag {
    pub fn new(view: ElementId, host: ElementId, offset: DVec2) -> Self {
        Self {
            view,
            host,
            offset,
            style: None,
            color: None,
            fields: Vec::new(),
            elbow: None,
            end: None,
            leader_end: None,
            material: false,
        }
    }
}

/// A storey. Walls and slabs hang off a level, so moving the level moves them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub elevation: f64,
    /// Default floor-to-floor height, used for new walls on this level.
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    /// The label after its group's prefix (see [`crate::grids`]).
    pub label: String,
    pub start: DVec2,
    pub end: DVec2,
    /// The grid system it belongs to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<ElementId>,
}

/// A wall defined by its centreline on a level: straight, or an arc.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub level: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
    pub height: f64,
    /// Offset of the wall base above its level.
    pub base_offset: f64,
    #[serde(default)]
    pub material: Option<ElementId>,
    /// The layered assembly the wall is built of. With one, the wall's
    /// thickness is the assembly's total and `material` is not drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<ElementId>,
    /// The wall's exterior face is on its left (start → end) instead of
    /// its right: where an assembly's first layer goes, which face the
    /// location line names, and which side doors open away from.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// The line the wall is drawn along and kept on (Revit's location
    /// line). `start` and `end` are always the centreline; this says which
    /// line stays put when the wall gets thicker, changes assembly or flips.
    #[serde(default, skip_serializing_if = "LocationLine::is_centre")]
    pub location: LocationLine,
    /// Where the top is: `height` above the base, or tied to a level (then
    /// `height` follows the levels; see [`crate::walls`]).
    #[serde(default, skip_serializing_if = "WallTop::is_unconnected")]
    pub top: WallTop,
    /// Joins disallowed at the start and at the end: that end butts into
    /// what it meets instead of joining, and walls joined there no longer
    /// follow it.
    #[serde(default, skip_serializing_if = "no_flags")]
    pub disallow_join: [bool; 2],
    /// Roofs and slabs the top is attached to: where one is overhead, the
    /// top follows its underside instead of stopping at `height`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attach_top: Vec<ElementId>,
    /// A curtain wall: a grid of mullions holding glass or solid panels
    /// instead of a solid wall. Its thickness is the mullions' depth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curtain: Option<crate::curtain::CurtainGrid>,
    /// Layer ends moved by hand (Layers, in Edit mode): each runs its
    /// layer on past, or stops it short of, where the joins put that end.
    /// Kept when the wall moves, stretches, turns or changes type; a layer
    /// the type no longer has drops its edit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layer_ends: Vec<LayerEnd>,
    /// A curved wall (MU40): the arc from `start` to `end` bulging this
    /// much, as a slab edge's (the tangent of a quarter of its angle,
    /// positive to the right of start → end); 0 is straight. A full
    /// circle is two half-circle walls.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub bulge: f64,
}

/// One layer's ends moved by hand: how far layer `layer` (counted from the
/// exterior) runs past where the joins put its start and its end, in
/// metres along the wall. Negative stops it short.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayerEnd {
    pub layer: usize,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    /// How far it runs down past the wall's base (A7-10): the outer leaf
    /// and insulation down past a slab's edge. Negative stops it short.
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub bottom: f64,
    /// How far it runs up past the wall's top.
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub top: f64,
}

fn is_zero_f(v: &f64) -> bool {
    *v == 0.0
}

impl LayerEnd {
    pub fn is_zero(&self) -> bool {
        [self.start, self.end, self.bottom, self.top]
            .iter()
            .all(|v| v.abs() < 1e-9)
    }
}

/// One layer's edge moved by hand on a slab or a roof: how far edge
/// `edge` of layer `layer` (counted from the top) is pushed out past the
/// element's own edge, in metres in plan, square to it. Negative pulls it
/// in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayerEdge {
    pub layer: usize,
    pub edge: usize,
    pub by: f64,
}

/// How far each of `n` edges of layer `layer` is pushed out by `edits`.
pub fn layer_edge_offsets(edits: &[LayerEdge], layer: usize, n: usize) -> Vec<f64> {
    let mut out = vec![0.0; n];
    for e in edits.iter().filter(|e| e.layer == layer && e.edge < n) {
        out[e.edge] = e.by;
    }
    out
}

/// Set how far edge `edge` of layer `layer` is pushed out; an edit back
/// at zero is dropped.
pub fn set_layer_edge(edits: &mut Vec<LayerEdge>, layer: usize, edge: usize, by: f64) {
    match edits
        .iter_mut()
        .find(|e| e.layer == layer && e.edge == edge)
    {
        Some(e) => e.by = by,
        None => edits.push(LayerEdge { layer, edge, by }),
    }
    edits.retain(|e| e.by.abs() > 1e-9);
    edits.sort_by_key(|e| (e.layer, e.edge));
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn no_flags(v: &[bool; 2]) -> bool {
    !v[0] && !v[1]
}

/// Which line of a wall is its location line. The exterior is on the
/// wall's right unless it is flipped; a wall without layers is all core.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocationLine {
    #[default]
    Centre,
    CoreCentre,
    ExteriorFace,
    InteriorFace,
    CoreExterior,
    CoreInterior,
    /// The line between layer `k - 1` and layer `k`, counted from the
    /// exterior: 0 is the exterior face, the layer count the interior
    /// face (3.7, *Draw by this line*).
    Boundary(u8),
}

impl LocationLine {
    pub const ALL: [LocationLine; 6] = [
        LocationLine::Centre,
        LocationLine::CoreCentre,
        LocationLine::ExteriorFace,
        LocationLine::InteriorFace,
        LocationLine::CoreExterior,
        LocationLine::CoreInterior,
    ];

    pub fn is_centre(&self) -> bool {
        *self == LocationLine::Centre
    }

    pub fn label(self) -> &'static str {
        match self {
            LocationLine::Centre => "Wall centre",
            LocationLine::CoreCentre => "Core centre",
            LocationLine::ExteriorFace => "Exterior face",
            LocationLine::InteriorFace => "Interior face",
            LocationLine::CoreExterior => "Core exterior face",
            LocationLine::CoreInterior => "Core interior face",
            LocationLine::Boundary(_) => "A layer boundary",
        }
    }
}

/// What a wall's top is tied to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum WallTop {
    /// `height` above the base, whatever the levels do.
    #[default]
    Unconnected,
    /// `offset` above (below, when negative) a level. A level that is gone
    /// leaves the wall at the height it had.
    UpToLevel { level: ElementId, offset: f64 },
}

impl WallTop {
    pub fn is_unconnected(&self) -> bool {
        *self == WallTop::Unconnected
    }
}

impl Wall {
    /// A plain wall on `level` along the centreline `start` → `end`, its
    /// base on the level, its top unconnected.
    pub fn new(level: ElementId, start: DVec2, end: DVec2, thickness: f64, height: f64) -> Self {
        Self {
            level,
            start,
            end,
            thickness,
            height,
            base_offset: 0.0,
            material: None,
            assembly: None,
            flip: false,
            location: LocationLine::Centre,
            top: WallTop::Unconnected,
            disallow_join: [false; 2],
            attach_top: vec![],
            curtain: None,
            layer_ends: vec![],
            bulge: 0.0,
        }
    }

    /// Its centreline: a line, or an arc when it bulges.
    pub fn curve(&self) -> crate::sketch::Curve {
        crate::sketch::Curve::arc(self.start, self.end, self.bulge)
    }

    /// Is it curved?
    pub fn is_arc(&self) -> bool {
        self.bulge.abs() > 1e-9
    }

    /// The point `u` along its centreline and `w` across it (left of the
    /// way it runs positive): on an arc, `u` is measured round it and `w`
    /// toward or away from its centre.
    pub fn place(&self, u: f64, w: f64) -> DVec2 {
        match self.curve() {
            crate::sketch::Curve::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let a = start + sweep.signum() * u / radius;
                let r = radius - sweep.signum() * w;
                center + DVec2::from_angle(a) * r
            }
            crate::sketch::Curve::Line(a, b) => {
                let d = (b - a).normalize_or_zero();
                a + d * u + d.perp() * w
            }
        }
    }

    /// How far along its centreline `p` is (clamped to its ends), and how
    /// far `p` is from that point: round the arc for a curved wall.
    pub fn along(&self, p: DVec2) -> (f64, f64) {
        let u = match self.curve() {
            crate::sketch::Curve::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let tau = std::f64::consts::TAU;
                let a = (p - center).y.atan2((p - center).x);
                let rel = ((a - start) * sweep.signum()).rem_euclid(tau);
                if rel <= sweep.abs() {
                    rel * radius
                } else if rel - sweep.abs() < tau - rel {
                    sweep.abs() * radius
                } else {
                    0.0
                }
            }
            crate::sketch::Curve::Line(a, b) => {
                let d = (b - a).normalize_or_zero();
                (p - a).dot(d).clamp(0.0, a.distance(b))
            }
        };
        (u, p.distance(self.place(u, 0.0)))
    }

    /// A point of one of its ends as its joins put it: `u` along it (which
    /// a join moved off the end, either way) and `w` across, on the line
    /// along its tangent at that end, the line its joins were worked out
    /// on. The same as [`Wall::place`] for a straight wall.
    pub fn place_end(&self, at_end: bool, u: f64, w: f64) -> DVec2 {
        let e = if at_end { self.length() } else { 0.0 };
        self.place(e, w) + self.tangent_at(e) * (u - e)
    }

    /// The way it runs at `u` along it.
    pub fn tangent_at(&self, u: f64) -> DVec2 {
        match self.curve() {
            crate::sketch::Curve::Arc {
                start,
                sweep,
                radius,
                ..
            } => {
                let a = start + sweep.signum() * u / radius;
                DVec2::from_angle(a).perp() * sweep.signum()
            }
            crate::sketch::Curve::Line(..) => self.direction(),
        }
    }

    /// Where along it a curved wall bends: steps of at most
    /// [`crate::sketch::ARC_STEP`], ends excluded; none for a straight one.
    pub fn bends(&self) -> Vec<f64> {
        match self.curve() {
            crate::sketch::Curve::Arc { sweep, .. } => {
                let n = ((sweep.abs() / crate::sketch::ARC_STEP - 1e-9).ceil() as usize).max(2);
                let len = self.length();
                (1..n).map(|i| len * i as f64 / n as f64).collect()
            }
            crate::sketch::Curve::Line(..) => vec![],
        }
    }

    /// How far layer `layer` runs past its start and its end, as edited
    /// by hand: `[start, end]`, zero when untouched.
    pub fn layer_ext(&self, layer: usize) -> [f64; 2] {
        self.layer_ends
            .iter()
            .find(|l| l.layer == layer)
            .map_or([0.0; 2], |l| [l.start, l.end])
    }

    /// Set how far layer `layer` runs past one end (`end` false for the
    /// start); an edit back at zero is dropped.
    pub fn set_layer_ext(&mut self, layer: usize, end: bool, by: f64) {
        match self.layer_ends.iter_mut().find(|l| l.layer == layer) {
            Some(l) => {
                if end {
                    l.end = by
                } else {
                    l.start = by
                }
            }
            None => self.layer_ends.push(LayerEnd {
                layer,
                start: if end { 0.0 } else { by },
                end: if end { by } else { 0.0 },
                ..Default::default()
            }),
        }
        self.layer_ends.retain(|l| !l.is_zero());
        self.layer_ends.sort_by_key(|l| l.layer);
    }

    /// How far layer `layer` runs past the wall's base and top, as edited
    /// by hand: `[bottom, top]`, zero when untouched (A7-10).
    pub fn layer_vext(&self, layer: usize) -> [f64; 2] {
        self.layer_ends
            .iter()
            .find(|l| l.layer == layer)
            .map_or([0.0; 2], |l| [l.bottom, l.top])
    }

    /// Set how far layer `layer` runs past the top (`top`) or below the
    /// base; an edit back at zero is dropped.
    pub fn set_layer_vext(&mut self, layer: usize, top: bool, by: f64) {
        match self.layer_ends.iter_mut().find(|l| l.layer == layer) {
            Some(l) => {
                if top {
                    l.top = by
                } else {
                    l.bottom = by
                }
            }
            None => self.layer_ends.push(LayerEnd {
                layer,
                bottom: if top { 0.0 } else { by },
                top: if top { by } else { 0.0 },
                ..Default::default()
            }),
        }
        self.layer_ends.retain(|l| !l.is_zero());
        self.layer_ends.sort_by_key(|l| l.layer);
    }

    pub fn length(&self) -> f64 {
        match self.curve() {
            crate::sketch::Curve::Arc { radius, sweep, .. } => radius * sweep.abs(),
            crate::sketch::Curve::Line(a, b) => a.distance(b),
        }
    }

    /// Unit vector along the wall, from start to end.
    pub fn direction(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }
}

/// A horizontal slab: its outline pushed `thickness` down from its top
/// face, which sits `top_offset` above its level's elevation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slab {
    pub level: ElementId,
    /// Corners of the outline in plan, counter-clockwise.
    pub boundary: Vec<DVec2>,
    pub thickness: f64,
    #[serde(default)]
    pub material: Option<ElementId>,
    /// Height of the top face above the level; negative for below.
    #[serde(default)]
    pub top_offset: f64,
    /// How each edge (corner `i` to corner `i + 1`, the last back to the
    /// first) bends: a bulge as in sketches, 0 for straight. Edges past the
    /// end of the list are straight, so older slabs have none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bulges: Vec<f64>,
    /// The layered assembly the slab is built of, layers listed from the
    /// top down. With one, the slab's thickness is the assembly's total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<ElementId>,
    /// Holes cut through it (a stair well, a duct), each a polygon in plan
    /// inside the outline. Shafts cut more; see [`crate::slab::holes_of`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<DVec2>>,
    /// Layer edges moved by hand (see [`LayerEdge`]); none for most.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layer_edges: Vec<LayerEdge>,
}

impl Slab {
    /// A straight-edged slab with its top at its level.
    pub fn new(level: ElementId, boundary: Vec<DVec2>, thickness: f64) -> Self {
        Self {
            level,
            boundary,
            thickness,
            material: None,
            top_offset: 0.0,
            bulges: vec![],
            assembly: None,
            holes: vec![],
            layer_edges: vec![],
        }
    }

    /// The corners of layer `layer`'s outline: the slab's, with any of its
    /// edges moved by hand pushed out or pulled in.
    pub fn layer_boundary(&self, layer: usize) -> Vec<DVec2> {
        let by = layer_edge_offsets(&self.layer_edges, layer, self.boundary.len());
        crate::members::offset_edges(&self.boundary, &by)
    }

    /// Layer `layer`'s outline, arcs traced, as [`Slab::outline`].
    pub fn layer_outline(&self, layer: usize) -> Vec<DVec2> {
        if !self.layer_edges.iter().any(|e| e.layer == layer) {
            return self.outline();
        }
        Slab {
            boundary: self.layer_boundary(layer),
            ..self.clone()
        }
        .outline()
    }

    /// The bulge of edge `i`; 0 for a straight one.
    pub fn bulge(&self, i: usize) -> f64 {
        self.bulges.get(i).copied().unwrap_or(0.0)
    }

    pub fn has_arcs(&self) -> bool {
        self.bulges.iter().any(|b| b.abs() > 1e-9)
    }

    /// Edge `i`'s exact shape.
    pub fn edge(&self, i: usize) -> Curve {
        let n = self.boundary.len();
        let (a, b) = (self.boundary[i % n], self.boundary[(i + 1) % n]);
        Curve::arc(a, b, self.bulge(i))
    }

    /// The outline as a polygon, arcs in pieces of at most
    /// [`crate::sketch::ARC_STEP`]. Without arcs it is `boundary`.
    pub fn outline(&self) -> Vec<DVec2> {
        if !self.has_arcs() {
            return self.boundary.clone();
        }
        let mut out = Vec::new();
        for i in 0..self.boundary.len() {
            let pts = self.edge(i).points();
            out.extend(&pts[..pts.len() - 1]);
        }
        out
    }

    /// Plan area of the outline, arcs included, less its own holes.
    pub fn area(&self) -> f64 {
        let holes: f64 = self
            .holes
            .iter()
            .map(|h| crate::polygon::signed_area2(h).abs() / 2.0)
            .sum();
        (crate::polygon::signed_area2(&self.outline()).abs() / 2.0 - holes).max(0.0)
    }

    /// Height of the top face, given the level's elevation.
    pub fn top(&self, elevation: f64) -> f64 {
        elevation + self.top_offset
    }

    /// Change the thickness, keeping the top face where it is, or with
    /// `keep_bottom` the underside (the top moves instead).
    pub fn set_thickness(&mut self, thickness: f64, keep_bottom: bool) {
        if keep_bottom {
            self.top_offset += thickness - self.thickness;
        }
        self.thickness = thickness;
    }

    /// Run the outline the other way round, arcs and all.
    pub fn reverse(&mut self) {
        let n = self.boundary.len();
        if self.has_arcs() {
            // edge j of the reversed outline is edge n - 2 - j run backwards
            self.bulges = (0..n).map(|j| -self.bulge((2 * n - 2 - j) % n)).collect();
        }
        self.boundary.reverse();
    }

    /// Make the outline counter-clockwise, as slabs keep it.
    pub fn make_ccw(&mut self) {
        if crate::polygon::signed_area2(&self.outline()) < 0.0 {
            self.reverse();
        }
    }

    /// Add a corner in the middle of edge `i` (on the arc, for an arc).
    pub fn split_edge(&mut self, i: usize) {
        let n = self.boundary.len();
        if i >= n {
            return;
        }
        let mid = self.edge(i).at(0.5);
        let b = self.bulge(i);
        self.boundary.insert(i + 1, mid);
        if self.has_arcs() {
            self.bulges.resize(n, 0.0);
            // each half turns through half the angle
            let half = (b.atan() / 2.0).tan();
            self.bulges[i] = half;
            self.bulges.insert(i + 1, half);
        }
    }

    /// Take corner `i` away; the edges either side of it become one
    /// straight edge.
    pub fn remove_corner(&mut self, i: usize) {
        let n = self.boundary.len();
        if i >= n {
            return;
        }
        self.boundary.remove(i);
        if self.has_arcs() {
            self.bulges.resize(n, 0.0);
            self.bulges.remove(i);
            let before = (i + n - 2) % (n - 1);
            self.bulges[before] = 0.0;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpeningKind {
    Door,
    Window,
    /// A bare rectangular hole with nothing in it.
    Hole,
}

/// A door or window cut into a host wall, or a bare hole.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Opening {
    pub host: ElementId,
    pub kind: OpeningKind,
    /// Distance from the wall start to the opening's left edge.
    pub offset: f64,
    /// Height of the opening's bottom edge above the wall base.
    pub sill: f64,
    pub width: f64,
    pub height: f64,
    /// What the door or window is made of; none draws it in its default colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
    /// Its frame, glazing bars and leaf.
    #[serde(default, skip_serializing_if = "OpeningFrame::is_default")]
    pub frame: OpeningFrame,
    /// A door hinged on its other jamb: the one at the wall's end side
    /// instead of its start side.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_hand: bool,
    /// A door opening to the wall's exterior instead of its interior; a
    /// window with its outside face (and its sill) to the wall's interior.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_facing: bool,
}

impl Opening {
    /// Does the door open to (the window face out of) the wall's left,
    /// looking from its start to its end? Doors open into the interior and
    /// windows face the exterior unless flipped; the exterior is the
    /// wall's right unless the wall is flipped, so flipping a wall turns
    /// its doors and windows round with it.
    pub fn faces_left(&self, wall: &Wall) -> bool {
        let interior_left = !wall.flip;
        let left = match self.kind {
            OpeningKind::Door => interior_left,
            OpeningKind::Window | OpeningKind::Hole => !interior_left,
        };
        left != self.flip_facing
    }
}

/// How a door or window is built inside its opening: a frame round the
/// edge, a window's glass split into panes by glazing bars, a door's leaf.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpeningFrame {
    /// Face width of the frame members, seen from the front.
    pub width: f64,
    /// Depth of the frame through the wall (at most the wall's thickness).
    pub depth: f64,
    /// Window panes side by side and one above the other, split by bars.
    pub panes_across: u32,
    pub panes_up: u32,
}

impl Default for OpeningFrame {
    fn default() -> Self {
        Self {
            width: 0.06,
            depth: 0.1,
            panes_across: 1,
            panes_up: 1,
        }
    }
}

impl OpeningFrame {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The frame kept to what fits an opening `w` × `h` in a wall `t` thick:
    /// members no wider than a third of the opening, at least one pane.
    pub fn fitted(&self, w: f64, h: f64, t: f64) -> Self {
        let fw = self.width.clamp(0.01, (w.min(h) / 3.0).max(0.01));
        Self {
            width: fw,
            depth: self.depth.clamp(0.02, t.max(0.02)),
            panes_across: self.panes_across.clamp(1, 12),
            panes_up: self.panes_up.clamp(1, 12),
        }
    }
}

/// A freely modelled object, edited Blender-style, that still carries BIM
/// meaning through its IFC class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FreeMesh {
    pub level: Option<ElementId>,
    pub origin: DVec3,
    pub mesh: EditMesh,
    pub ifc_class: String,
    #[serde(default)]
    pub material: Option<ElementId>,
    /// Non-destructive operations applied to `mesh`, top to bottom.
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    /// Turn about the vertical axis through `origin`, in degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Turns about the x and y axes through `origin`, in degrees, applied
    /// before `rotation`: XYZ Euler, as Blender's (MU24).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub rot_x: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub rot_y: f64,
    /// Uniform size factor about `origin`.
    #[serde(default = "one")]
    pub scale: f64,
}

fn one() -> f64 {
    1.0
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

impl FreeMesh {
    /// A mesh as modelled, with no modifiers, turn or scaling.
    pub fn new(level: Option<ElementId>, origin: DVec3, mesh: EditMesh) -> Self {
        Self {
            level,
            origin,
            mesh,
            ifc_class: "IfcBuildingElementProxy".into(),
            material: None,
            modifiers: vec![],
            rotation: 0.0,
            rot_x: 0.0,
            rot_y: 0.0,
            scale: 1.0,
        }
    }

    /// Its turn as a rotation: about x, then y, then z (XYZ Euler).
    pub fn turn(&self) -> glam::DQuat {
        glam::DQuat::from_euler(
            glam::EulerRot::ZYX,
            self.rotation.to_radians(),
            self.rot_y.to_radians(),
            self.rot_x.to_radians(),
        )
    }

    /// From the mesh's own axes to the model's, about `origin`.
    pub fn to_model(&self, v: DVec3) -> DVec3 {
        self.turn() * (v * self.scale)
    }

    /// A model-axes offset in the mesh's own axes (for moving vertices).
    pub fn to_local(&self, d: DVec3) -> DVec3 {
        (self.turn().inverse() * d) / self.scale
    }

    /// The shape as shown: modifiers applied, turned and scaled, relative to
    /// `origin`. `source` gives each face's face in `mesh`, for editing.
    pub fn evaluated(&self) -> Evaluated {
        let mut e = crate::modifiers::evaluate(&self.mesh, &self.modifiers);
        if self.rotation != 0.0 || self.rot_x != 0.0 || self.rot_y != 0.0 || self.scale != 1.0 {
            for v in &mut e.mesh.vertices {
                *v = self.to_model(*v);
            }
            if self.scale < 0.0 {
                for f in &mut e.mesh.faces {
                    f.reverse();
                }
            }
        }
        e
    }
}

/// A plane anywhere in the model to sketch on: an origin and two
/// perpendicular unit axes. Its normal is `x × y`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workplane {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
}

impl Workplane {
    /// A horizontal plane at height `z`.
    pub fn ground(z: f64) -> Self {
        Self {
            origin: DVec3::new(0.0, 0.0, z),
            x: DVec3::X,
            y: DVec3::Y,
        }
    }

    /// The plane through `origin` facing `normal`. Its x axis is level
    /// (horizontal) whenever the plane is not, so sketches on walls stay square.
    pub fn facing(origin: DVec3, normal: DVec3) -> Self {
        let n = normal.normalize_or(DVec3::Z);
        let x = if n.z.abs() > 0.999 {
            DVec3::X * n.z.signum()
        } else {
            DVec3::Z.cross(n).normalize()
        };
        Self {
            origin,
            x,
            y: n.cross(x),
        }
    }

    pub fn normal(&self) -> DVec3 {
        self.x.cross(self.y)
    }

    pub fn to_model(&self, p: DVec2) -> DVec3 {
        self.origin + self.x * p.x + self.y * p.y
    }

    pub fn to_plane(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.x), d.dot(self.y))
    }

    /// Where a ray meets the plane, if it does in front of its origin.
    pub fn intersect(&self, origin: DVec3, dir: DVec3) -> Option<DVec3> {
        let n = self.normal();
        let denom = dir.dot(n);
        if denom.abs() < 1e-9 {
            return None;
        }
        let t = (self.origin - origin).dot(n) / denom;
        (t > 0.0).then(|| origin + dir * t)
    }
}

/// A drawing of the model. Views live in the model like any element, so they
/// are saved, undone and (later) placed on sheets. Their drawings are always
/// generated from the current model, never stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub kind: ViewKind,
    /// Drawing scale denominator: 100 means 1:100.
    pub scale: f64,
    #[serde(default)]
    pub style: ViewStyle,
    /// Categories, elements and grid groups this view leaves out.
    #[serde(default, skip_serializing_if = "ViewHidden::is_empty")]
    pub hidden: ViewHidden,
    /// Another level drawn faintly under a plan, to trace and snap to.
    #[serde(default, skip_serializing_if = "Underlay::is_none")]
    pub underlay: Underlay,
    /// Where the user put the view in its group of the project browser, by
    /// dragging it; `None` keeps it in the automatic order (plans by level).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_order: Option<u32>,
    /// Draw what is hidden inside other elements (a beam in a wall, a stud
    /// wall in a cavity) with dashed hidden lines.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub show_hidden: bool,
    /// Draw those hidden elements in their own material colours, washed
    /// over what hides them, as well as dashed: for detailing what is
    /// inside a wall or a slab.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden_in_colour: bool,
    /// Wash each room in a colour of its own (by its department, else its
    /// name): a colour-fill plan, or rooms shaded in a section.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub room_colours: bool,
    /// A plan seen looking up from the cut (a reflected ceiling plan): the
    /// ceilings over the level and what hangs from them, with walls cut but
    /// without the furniture and floors below.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ceiling_plan: bool,
    /// A plan's line weights, below-cut fill and view depth.
    #[serde(
        default,
        skip_serializing_if = "crate::graphics::PlanGraphics::is_default"
    )]
    pub graphics: crate::graphics::PlanGraphics,
    /// A detail view called out from another view: it shows the part of
    /// that view inside the callout's box, at its own scale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<Callout>,
    /// Draw room tags in this view: unset, on except in detail and callout
    /// views, where a tag at 1:10 is a scribble (A5-7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_tags: Option<bool>,
    /// How its title reads where it is placed on a sheet (MU27).
    #[serde(default, skip_serializing_if = "boxed_title_is_default")]
    pub sheet_title: Box<SheetTitle>,
    /// How a section or elevation shows each level's head: turned off, or
    /// dragged away from where it would sit. Levels not listed show their
    /// heads where the automatic layout puts them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub level_heads: BTreeMap<ElementId, LevelHead>,
    /// Its view template, graphic overrides and depth cues, over the
    /// project's style pack.
    #[serde(default, skip_serializing_if = "crate::styles::boxed_look_is_default")]
    pub look: Box<crate::styles::ViewLook>,
}

/// Where a detail view comes from: the box `lo`..`hi` (drawing
/// coordinates, lower-left and upper-right) drawn on `parent`. The detail
/// is drawn from its parent's plan or section, so it follows the parent's
/// line and cut; the parent shows the box with the detail's number. The
/// parent is soft: without it the detail keeps the view it had.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Callout {
    pub parent: ElementId,
    pub lo: DVec2,
    pub hi: DVec2,
}

impl Callout {
    /// The box, with its corners in order whichever way it was drawn.
    pub fn new(parent: ElementId, a: DVec2, b: DVec2) -> Self {
        Self {
            parent,
            lo: a.min(b),
            hi: a.max(b),
        }
    }
}

/// One level's head in one section or elevation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LevelHead {
    /// Leave the head (name and elevation) out of this view; its datum line stays.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Where it was dragged to, from the end of its datum line, in drawing
    /// units; none lets the automatic layout place it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<DVec2>,
}

impl LevelHead {
    pub fn is_default(&self) -> bool {
        *self == LevelHead::default()
    }
}

/// What a plan shows faintly under its own level (Revit's underlay).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Underlay {
    #[default]
    None,
    /// The next level down.
    Below,
    /// The next level up.
    Above,
    /// A chosen level. Not a hard reference: deleting the level just turns
    /// the underlay off.
    Level(ElementId),
}

impl Underlay {
    pub fn is_none(&self) -> bool {
        *self == Underlay::None
    }

    /// The level this underlay shows for a plan of `level`, if any.
    pub fn level(&self, model: &crate::Model, level: ElementId) -> Option<ElementId> {
        let here = model.level(level)?.elevation;
        let mut others: Vec<(ElementId, f64)> = model
            .levels()
            .filter(|(e, _)| e.id != level)
            .map(|(e, l)| (e.id, l.elevation))
            .collect();
        others.sort_by(|a, b| a.1.total_cmp(&b.1));
        match self {
            Underlay::None => None,
            Underlay::Below => others.iter().rev().find(|o| o.1 <= here).map(|o| o.0),
            Underlay::Above => others.iter().find(|o| o.1 >= here).map(|o| o.0),
            Underlay::Level(id) => (*id != level && model.level(*id).is_some()).then_some(*id),
        }
    }
}

/// How a section, elevation or 3D view draws what is beyond the cut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewStyle {
    /// Visible edges only.
    #[default]
    HiddenLine,
    /// Visible edges over surfaces shaded in their material colours.
    Shaded,
    /// Every edge, the hidden ones too: to see what is inside and behind,
    /// like a shaft running through the floors.
    Wireframe,
}

impl ViewStyle {
    pub const ALL: [ViewStyle; 3] = [
        ViewStyle::HiddenLine,
        ViewStyle::Shaded,
        ViewStyle::Wireframe,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewStyle::HiddenLine => "Hidden line",
            ViewStyle::Shaded => "Shaded with edges",
            ViewStyle::Wireframe => "Wireframe",
        }
    }
}

/// What a schedule lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleCategory {
    Doors,
    Windows,
    Walls,
    Slabs,
    /// Columns and beams.
    Structure,
    /// Placed objects that are furniture in IFC (IfcFurniture,
    /// IfcSystemFurnitureElement), counted by type and level.
    Furniture,
    /// Every other placed object (sanitary ware, appliances, electrical,
    /// lighting, plumbing, heating and ventilation), counted by type and
    /// level.
    Fixtures,
    /// Rooms, with their areas totalled for each level.
    Rooms,
    /// The take-off: what each material adds up to (area, volume, length,
    /// pieces), priced by its rate; grouped, it goes level by level.
    Materials,
}

impl ScheduleCategory {
    pub const ALL: [ScheduleCategory; 9] = [
        ScheduleCategory::Doors,
        ScheduleCategory::Windows,
        ScheduleCategory::Walls,
        ScheduleCategory::Slabs,
        ScheduleCategory::Structure,
        ScheduleCategory::Furniture,
        ScheduleCategory::Fixtures,
        ScheduleCategory::Rooms,
        ScheduleCategory::Materials,
    ];

    /// The name of a new schedule of it: "Door Schedule".
    pub fn schedule_name(self) -> String {
        let one = match self {
            ScheduleCategory::Doors => "Door",
            ScheduleCategory::Windows => "Window",
            ScheduleCategory::Walls => "Wall",
            ScheduleCategory::Slabs => "Slab",
            ScheduleCategory::Structure => "Column and beam",
            ScheduleCategory::Furniture => "Furniture",
            ScheduleCategory::Fixtures => "Fixture",
            ScheduleCategory::Rooms => "Room",
            ScheduleCategory::Materials => return "Take-off".into(),
        };
        format!("{one} Schedule")
    }

    /// Does it list placed objects, counted by type?
    pub fn is_objects(self) -> bool {
        matches!(
            self,
            ScheduleCategory::Furniture | ScheduleCategory::Fixtures
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            ScheduleCategory::Doors => "Doors",
            ScheduleCategory::Windows => "Windows",
            ScheduleCategory::Walls => "Walls",
            ScheduleCategory::Slabs => "Slabs",
            ScheduleCategory::Structure => "Columns and beams",
            ScheduleCategory::Furniture => "Furniture",
            ScheduleCategory::Fixtures => "Fixtures",
            ScheduleCategory::Rooms => "Rooms",
            ScheduleCategory::Materials => "Take-off",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ViewKind {
    /// A floor plan of one level, cut horizontally at `cut_height` above it.
    Plan { level: ElementId, cut_height: f64 },
    /// A vertical section along the line `start` → `end`, looking to the left
    /// of that direction (a line drawn west to east looks north) for `depth`
    /// metres. An elevation is the same view with `cut` off: nothing is cut,
    /// everything in front of the viewer is drawn in projection.
    Section {
        start: DVec2,
        end: DVec2,
        depth: f64,
        cut: bool,
    },
    /// A parallel-projection 3D view of the whole model, seen from compass
    /// bearing `azimuth` (degrees, 0 = from the south, counter-clockwise)
    /// and `altitude` degrees above the horizon.
    Axonometric { azimuth: f64, altitude: f64 },
    /// A table of every element in a category, read live from the model.
    /// It is drawn at paper size, whatever the scale.
    Schedule {
        category: ScheduleCategory,
        /// Rows grouped by element type, under a heading per type.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        by_type: bool,
    },
}

impl View {
    pub fn plan(level: ElementId) -> Self {
        Self {
            kind: ViewKind::Plan {
                level,
                cut_height: 1.2,
            },
            scale: 100.0,
            style: ViewStyle::HiddenLine,
            hidden: ViewHidden::default(),
            underlay: Underlay::None,
            browser_order: None,
            show_hidden: false,
            hidden_in_colour: false,
            room_colours: false,
            ceiling_plan: false,
            graphics: Default::default(),
            callout: None,
            room_tags: None,
            sheet_title: Box::default(),
            level_heads: BTreeMap::new(),
            look: Default::default(),
        }
    }

    pub fn section(start: DVec2, end: DVec2, depth: f64, cut: bool) -> Self {
        Self {
            kind: ViewKind::Section {
                start,
                end,
                depth,
                cut,
            },
            scale: 100.0,
            style: ViewStyle::HiddenLine,
            hidden: ViewHidden::default(),
            underlay: Underlay::None,
            browser_order: None,
            show_hidden: false,
            hidden_in_colour: false,
            room_colours: false,
            ceiling_plan: false,
            graphics: Default::default(),
            callout: None,
            room_tags: None,
            sheet_title: Box::default(),
            level_heads: BTreeMap::new(),
            look: Default::default(),
        }
    }
}

impl View {
    pub fn axonometric(azimuth: f64, altitude: f64) -> Self {
        Self {
            kind: ViewKind::Axonometric { azimuth, altitude },
            scale: 100.0,
            style: ViewStyle::Shaded,
            hidden: ViewHidden::default(),
            underlay: Underlay::None,
            browser_order: None,
            show_hidden: false,
            hidden_in_colour: false,
            room_colours: false,
            ceiling_plan: false,
            graphics: Default::default(),
            callout: None,
            room_tags: None,
            sheet_title: Box::default(),
            level_heads: BTreeMap::new(),
            look: Default::default(),
        }
    }

    pub fn schedule(category: ScheduleCategory) -> Self {
        Self {
            kind: ViewKind::Schedule {
                category,
                by_type: false,
            },
            scale: 1000.0,
            style: ViewStyle::HiddenLine,
            hidden: ViewHidden::default(),
            underlay: Underlay::None,
            browser_order: None,
            show_hidden: false,
            hidden_in_colour: false,
            room_colours: false,
            ceiling_plan: false,
            graphics: Default::default(),
            callout: None,
            room_tags: None,
            sheet_title: Box::default(),
            level_heads: BTreeMap::new(),
            look: Default::default(),
        }
    }

    /// A detail view of the box `a`..`b` on `parent` (a plan or a
    /// section), at `scale`: a copy of the parent's view, cropped.
    /// Whether room tags draw here (A5-7): off by default in a detail or
    /// callout view, else on.
    pub fn shows_room_tags(&self) -> bool {
        self.room_tags.unwrap_or(self.callout.is_none())
    }

    pub fn callout(parent_id: ElementId, parent: &View, a: DVec2, b: DVec2, scale: f64) -> Self {
        Self {
            kind: parent.kind.clone(),
            scale,
            style: ViewStyle::HiddenLine,
            hidden: parent.hidden.clone(),
            underlay: Underlay::None,
            browser_order: None,
            show_hidden: false,
            hidden_in_colour: false,
            room_colours: false,
            ceiling_plan: parent.ceiling_plan,
            graphics: parent.graphics.clone(),
            callout: Some(Callout::new(parent_id, a, b)),
            room_tags: None,
            sheet_title: Box::default(),
            level_heads: BTreeMap::new(),
            look: Default::default(),
        }
    }

    /// The scale its drawing is placed on paper at. Schedules are drawn in
    /// paper millimetres, so they are placed 1:1000 (a drawing metre is a millimetre).
    pub fn paper_scale(&self) -> f64 {
        match self.kind {
            ViewKind::Schedule { .. } => 1000.0,
            _ => self.scale,
        }
    }
}

/// A drawing sheet. Its name is the sheet title; sizes are in millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    /// Sheet number, e.g. "A101".
    pub number: String,
    pub width: f64,
    pub height: f64,
    /// The [`SheetTemplate`] its title block comes from; none draws the
    /// built-in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<ElementId>,
    /// Values for its template's own fields, by name.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub fields: std::collections::BTreeMap<String, String>,
    /// Guide lines to place things by, as Inkscape's (MU26): never
    /// printed. `true` for an up-and-down guide at that many mm across,
    /// `false` for one across at that many mm up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<(bool, f64)>,
}

impl Sheet {
    /// ISO A-series sizes, landscape.
    pub const SIZES: &[(&str, f64, f64)] = &[
        ("A0", 1189.0, 841.0),
        ("A1", 841.0, 594.0),
        ("A2", 594.0, 420.0),
        ("A3", 420.0, 297.0),
        ("A4", 297.0, 210.0),
    ];

    pub fn a(number: impl Into<String>, size: &str) -> Self {
        let (_, width, height) = Self::SIZES
            .iter()
            .find(|(n, ..)| *n == size)
            .copied()
            .unwrap_or(Self::SIZES[3]);
        Self {
            number: number.into(),
            width,
            height,
            template: None,
            fields: Default::default(),
            guides: vec![],
        }
    }

    /// The paper size's name, if it is a standard one.
    pub fn size_name(&self) -> Option<&'static str> {
        Self::SIZES
            .iter()
            .find(|(_, w, h)| (w - self.width).abs() < 0.5 && (h - self.height).abs() < 0.5)
            .map(|(n, ..)| *n)
    }
}

/// A view placed on a sheet. The view's drawing is generated live, so the
/// sheet always shows the current model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub sheet: ElementId,
    pub view: ElementId,
    /// Where `target` sits on the sheet, mm from the sheet's lower-left corner.
    pub center: DVec2,
    /// The drawing point (model metres) pinned to `center`, so the drawing
    /// stays put on the sheet as the model changes.
    pub target: DVec2,
    /// Only the part of the drawing inside this rectangle (drawing
    /// coordinates, lower-left and upper-right) is shown.
    #[serde(default)]
    pub crop: Option<[DVec2; 2]>,
}

/// A view's title on a sheet: shown or not, its scale shown or not, moved
/// from where it sits under the drawing, its size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SheetTitle {
    pub hidden: bool,
    pub show_scale: bool,
    pub show_number: bool,
    /// From its place under the drawing's lower left, paper mm.
    pub offset: DVec2,
    /// The title's letter height, paper mm.
    pub size: f64,
    /// Its own words; empty, the view's name.
    pub text: String,
    /// The text style its words take (font, weight, colour); none, plain
    /// sans (MU28). Its size stays `size`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<crate::ElementId>,
}

impl Default for SheetTitle {
    fn default() -> Self {
        Self {
            hidden: false,
            show_scale: true,
            show_number: true,
            offset: DVec2::ZERO,
            size: 3.5,
            text: String::new(),
            style: None,
        }
    }
}

impl SheetTitle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// For serde: a view's title settings are left out while they are the default.
#[allow(clippy::borrowed_box)]
fn boxed_title_is_default(t: &Box<SheetTitle>) -> bool {
    t.is_default()
}

/// IFC classes a free mesh may be classified as.
pub const FREE_MESH_CLASSES: &[&str] = &[
    "IfcBuildingElementProxy",
    "IfcWall",
    "IfcSlab",
    "IfcRoof",
    "IfcColumn",
    "IfcBeam",
    "IfcCovering",
    "IfcFurniture",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slab_layer_edge_moves_that_layer_only() {
        let mut s = Slab::new(
            ElementId::new(),
            vec![
                DVec2::ZERO,
                DVec2::new(5.0, 0.0),
                DVec2::new(5.0, 4.0),
                DVec2::new(0.0, 4.0),
            ],
            0.3,
        );
        set_layer_edge(&mut s.layer_edges, 1, 2, 0.05);
        assert_eq!(s.layer_outline(0), s.outline());
        let out = s.layer_outline(1);
        assert!(out.iter().any(|p| (p.y - 4.05).abs() < 1e-9), "{out:?}");
        assert_eq!(
            layer_edge_offsets(&s.layer_edges, 1, 4),
            [0.0, 0.0, 0.05, 0.0]
        );
        set_layer_edge(&mut s.layer_edges, 1, 2, 0.0);
        assert!(s.layer_edges.is_empty());
    }

    #[test]
    fn layer_ends_are_kept_sorted_dropped_at_zero_and_left_out_of_files_untouched() {
        let mut w = Wall::new(ElementId::new(), DVec2::ZERO, DVec2::X * 4.0, 0.3, 3.0);
        w.set_layer_ext(2, true, 0.02);
        w.set_layer_ext(0, false, -0.05);
        assert_eq!(w.layer_ext(2), [0.0, 0.02]);
        assert_eq!(w.layer_ext(0), [-0.05, 0.0]);
        assert_eq!(w.layer_ext(1), [0.0; 2]);
        assert_eq!(
            w.layer_ends.iter().map(|l| l.layer).collect::<Vec<_>>(),
            [0, 2]
        );
        let back: Wall = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(back.layer_ends, w.layer_ends);
        w.set_layer_ext(2, true, 0.0);
        w.set_layer_ext(0, false, 0.0);
        assert!(w.layer_ends.is_empty());
        assert!(!serde_json::to_string(&w).unwrap().contains("layer_ends"));
    }

    #[test]
    fn dimension_points_from_older_files_still_read() {
        let old: HostPoint =
            serde_json::from_str(r#"{"host":null,"local":[1.0,2.0],"from_end":false}"#).unwrap();
        assert_eq!(old, HostPoint::free(DVec2::new(1.0, 2.0)));
        // a point on a body round-trips, a free point writes no body
        let on = HostPoint {
            host: Some(ElementId::new()),
            local: DVec2::ZERO,
            from_end: false,
            body: Some(DVec3::new(1.0, -1.0, 3.0)),
        };
        let text = serde_json::to_string(&on).unwrap();
        assert_eq!(serde_json::from_str::<HostPoint>(&text).unwrap(), on);
        let free = serde_json::to_string(&HostPoint::free(DVec2::ONE)).unwrap();
        assert!(!free.contains("body"));
    }

    #[test]
    fn views_from_older_files_keep_the_automatic_browser_order() {
        let mut v = View::schedule(ScheduleCategory::ALL[0]);
        let text = serde_json::to_string(&v).unwrap();
        assert!(
            !text.contains("browser_order"),
            "nothing written when automatic"
        );
        assert_eq!(
            serde_json::from_str::<View>(&text).unwrap().browser_order,
            None
        );
        v.browser_order = Some(3);
        let text = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<View>(&text).unwrap(), v);
    }

    #[test]
    fn openings_from_older_files_get_a_default_frame() {
        let host = serde_json::to_string(&ElementId::new()).unwrap();
        let old = format!(
            r#"{{"host":{host},"kind":"Window","offset":1.0,"sill":0.9,"width":1.2,"height":1.3}}"#
        );
        let o: Opening = serde_json::from_str(&old).unwrap();
        assert_eq!(o.frame, OpeningFrame::default());
        // a default frame writes nothing; a changed one round-trips
        assert!(!serde_json::to_string(&o).unwrap().contains("frame"));
        let mut o = o;
        o.frame.panes_across = 3;
        let text = serde_json::to_string(&o).unwrap();
        assert_eq!(serde_json::from_str::<Opening>(&text).unwrap(), o);
        // a frame missing some fields fills the rest in
        let partial = old.replace("}", r#","frame":{"width":0.08}}"#);
        let o: Opening = serde_json::from_str(&partial).unwrap();
        assert_eq!(o.frame.width, 0.08);
        assert_eq!(o.frame.depth, OpeningFrame::default().depth);
    }

    #[test]
    fn room_tags_are_off_by_default_only_in_callouts() {
        let plan = View::schedule(ScheduleCategory::ALL[0]);
        assert!(plan.shows_room_tags());
        let mut detail = plan.clone();
        detail.callout = Some(Callout::new(ElementId::new(), DVec2::ZERO, DVec2::ONE));
        assert!(!detail.shows_room_tags());
        detail.room_tags = Some(true);
        assert!(detail.shows_room_tags());
    }

    #[test]
    fn views_layers_and_junctions_from_older_files_still_read() {
        // a view saved before callouts reads as no callout, and writes none
        let v = View::schedule(ScheduleCategory::ALL[0]);
        let text = serde_json::to_string(&v).unwrap();
        assert!(!text.contains("callout"));
        assert_eq!(serde_json::from_str::<View>(&text).unwrap().callout, None);
        // a layer saved before join priorities takes its function's
        let l: crate::assembly::Layer =
            serde_json::from_str(r#"{"function":"Structure","thickness":0.2}"#).unwrap();
        assert_eq!(l.priority, None);
        assert_eq!(l.join_priority(), 1);
        // a junction written with only its two elements joins automatically
        let (a, b) = (ElementId::new(), ElementId::new());
        let text = format!(
            r#"{{"a":{},"b":{}}}"#,
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        let r: crate::joins::JoinRule = serde_json::from_str(&text).unwrap();
        assert_eq!(r, crate::joins::JoinRule::new(a, b));
        assert!(r.cleanup && r.is_default());
    }
}

#[cfg(test)]
mod mesh_turn_tests {
    use super::*;

    #[test]
    fn a_mesh_turns_about_x_then_y_then_z_and_back() {
        // MU24: Blender's XYZ Euler, so a shape drawn on a wall can turn
        // about the wall's own axes
        let mut m = FreeMesh::new(None, DVec3::ZERO, crate::editmesh::EditMesh::default());
        m.rot_x = 90.0;
        let up = m.to_model(DVec3::Z);
        assert!(up.distance(DVec3::new(0.0, -1.0, 0.0)) < 1e-9, "{up}");
        m.rot_y = 30.0;
        m.rotation = 45.0;
        m.scale = 2.0;
        let v = DVec3::new(0.3, -1.2, 0.7);
        assert!(m.to_local(m.to_model(v)).distance(v) < 1e-9);
    }
}
