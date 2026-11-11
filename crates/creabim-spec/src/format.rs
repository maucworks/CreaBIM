//! The format itself: every object a file may hold, field by field. The
//! doc comments here are the reference in `docs/json-format.md` and the
//! descriptions in `docs/creabim-model.schema.json`, so they are written
//! for someone writing a file, not for someone reading the code.

use crate::json::Colour;
use crate::{spec_enum, spec_struct};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

/// The newest version of the format this CreaBIM reads and writes.
pub const VERSION: u32 = 1;

spec_struct! {
    /// A whole model: the file's top level. Every list may be left out.
    /// Ids are chosen by you, are unique across the whole file, and are
    /// what elements use to refer to each other (a door names its wall's
    /// id, a plan names its level's id).
    pub struct Model {
        /// Where an editor finds this schema; ignored by CreaBIM.
        schema as "$schema": Option<String>,
        /// The format version the file is written to. This CreaBIM reads 1.
        version: u32,
        /// The unit every length in the file is in, except the sizes and
        /// positions on sheets, which are always paper millimetres.
        /// Angles are always degrees.
        units: LengthIn = LengthIn::Metres,
        /// How the project shows and takes lengths (the model is metres
        /// whatever this says).
        display_units: Option<DisplayUnits>,
        materials: Vec<Material>,
        /// Layered build-ups for walls, floors and roofs.
        assemblies: Vec<Assembly>,
        /// Element types: wall types, door and window types, slab, roof,
        /// column and beam types.
        types: Vec<Type>,
        levels: Vec<Level>,
        grid_groups: Vec<GridGroup>,
        grids: Vec<Grid>,
        walls: Vec<Wall>,
        /// Doors, windows and bare holes, each in a wall.
        openings: Vec<Opening>,
        slabs: Vec<Slab>,
        roofs: Vec<Roof>,
        columns: Vec<Column>,
        beams: Vec<Beam>,
        stairs: Vec<Stair>,
        railings: Vec<Railing>,
        /// Extrusions and sweeps, solid or voids that cut what they meet.
        parts: Vec<Part>,
        /// Terrain, hard landscape and planting.
        site: Vec<SiteElement>,
        /// Furniture and fixtures from the catalogue.
        objects: Vec<Object>,
        rooms: Vec<Room>,
        views: Vec<View>,
        sheets: Vec<Sheet>,
        /// Tags of doors, windows, rooms and other elements, in views.
        tags: Vec<Tag>,
        /// Text written in a view or on a sheet.
        notes: Vec<Note>,
    }
}

spec_enum! {
    /// A unit lengths are written in.
    pub enum LengthIn {
        Metres = "m",
        Millimetres = "mm",
        Centimetres = "cm",
        Feet = "ft",
        Inches = "in",
    }
}

impl LengthIn {
    /// Metres in one of it.
    pub fn metres(self) -> f64 {
        match self {
            LengthIn::Metres => 1.0,
            LengthIn::Millimetres => 0.001,
            LengthIn::Centimetres => 0.01,
            LengthIn::Feet => 0.3048,
            LengthIn::Inches => 0.0254,
        }
    }
}

spec_struct! {
    /// The project's display units.
    pub struct DisplayUnits {
        length: DisplayLength,
        /// Decimal places; for feet and inches, the smallest fraction of an
        /// inch as a power of two (3 is 1/8").
        precision: u32 = 0,
    }
}

spec_enum! {
    /// How lengths are shown.
    pub enum DisplayLength {
        Millimetres = "mm",
        Centimetres = "cm",
        Metres = "m",
        /// Feet and fractional inches: 3'-6 1/2".
        FeetInches = "ft-in",
        /// Decimal feet.
        Feet = "ft",
        /// Fractional inches.
        Inches = "in",
    }
}

spec_struct! {
    /// What elements are made of: their colour in 3D and the hatch on
    /// their cut faces. Start from a library material by its name, or give
    /// the values yourself; any value given overrides the library's.
    pub struct Material {
        id: String,
        /// Shown in the app; the id when left out.
        name: Option<String>,
        /// A material of CreaBIM's library, by name ("Concrete, cast in
        /// place"), to start from.
        library: Option<String>,
        hatch: Option<Hatch>,
        colour: Option<Colour>,
        /// IFC's material category: concrete, brick, wood, steel, glass...
        category: Option<String>,
        description: Option<String>,
        /// 0 mirror-smooth to 1 fully matte.
        roughness: Option<f64>,
        /// 0 for almost everything, 1 for bare metal.
        metallic: Option<f64>,
        /// 1 opaque; glass is about 0.3.
        opacity: Option<f64>,
        /// Its price for the take-off, in the project's currency, for each
        /// `rate_per`.
        rate: Option<f64>,
        /// What `rate` prices: square metres (the default), cubic metres,
        /// metres or pieces.
        rate_per: Option<RatePer>,
    }
}

spec_enum! {
    /// What a material's rate is priced per.
    pub enum RatePer {
        SquareMetre = "m2",
        CubicMetre = "m3",
        Metre = "m",
        Each = "each",
    }
}

spec_enum! {
    /// The pattern on a material's cut faces.
    pub enum Hatch {
        Empty = "empty",
        Solid = "solid",
        Diagonal = "diagonal",
        Cross = "cross",
        Brick = "brick",
        Concrete = "concrete",
        Timber = "timber",
    }
}

spec_struct! {
    /// A layered build-up. Walls list layers from the exterior face in;
    /// floors and roofs from the top down. An element built of one is as
    /// thick as its layers.
    pub struct Assembly {
        id: String,
        name: Option<String>,
        usage: AssemblyUsage = AssemblyUsage::Wall,
        layers: Vec<Layer>,
        description: Option<String>,
    }
}

spec_enum! {
    /// What an assembly is for.
    pub enum AssemblyUsage {
        Wall = "wall",
        Floor = "floor",
        Roof = "roof",
    }
}

spec_struct! {
    /// One layer of an assembly.
    pub struct Layer {
        function: LayerFunction = LayerFunction::Structure,
        /// A material's id; none for an air gap.
        material: Option<String>,
        thickness: f64,
        /// Which layer wins where walls join: 1 cuts through 5. Left out,
        /// it follows the layer's function (structure 1, finish 5).
        priority: Option<u32>,
    }
}

spec_enum! {
    /// What a layer does. The core runs from the first structure layer
    /// to the last.
    pub enum LayerFunction {
        Structure = "structure",
        Substrate = "substrate",
        Insulation = "insulation",
        AirGap = "air-gap",
        Membrane = "membrane",
        Finish = "finish",
    }
}

spec_struct! {
    /// An element type: shared values its instances take, so editing the
    /// type in the app changes them all. An instance names it in its
    /// `type` field and may still set any of those values itself.
    ///
    /// Which fields a category uses: wall: `thickness` (or `assembly`),
    /// `material`, `location`, `height`, `screen`; door and window:
    /// `width`, `height`, `sill`, `frame`, `material`; slab and roof:
    /// `thickness` (or `assembly`), `material`; column: `width` and
    /// `depth`, or `diameter`, and `material`; beam: `width`, `depth`,
    /// `material`.
    pub struct Type {
        id: String,
        name: Option<String>,
        category: TypeCategory,
        description: Option<String>,
        thickness: Option<f64>,
        /// An assembly's id.
        assembly: Option<String>,
        /// A material's id.
        material: Option<String>,
        /// A wall type's location line.
        location: Option<Location>,
        /// With location `layer-boundary`: which boundary, counted from
        /// the exterior face (0) to the interior face (the layer count).
        boundary: Option<u32>,
        /// A wall type's height for new walls; a door or window's height.
        height: Option<f64>,
        width: Option<f64>,
        sill: Option<f64>,
        frame: Option<Frame>,
        depth: Option<f64>,
        diameter: Option<f64>,
        /// Makes a wall type a screen wall: a repeating unit with holes.
        screen: Option<Screen>,
    }
}

spec_enum! {
    /// What a type is for.
    pub enum TypeCategory {
        Wall = "wall",
        Door = "door",
        Window = "window",
        Slab = "slab",
        Roof = "roof",
        Column = "column",
        Beam = "beam",
    }
}

spec_struct! {
    /// The repeating unit of a screen wall (breeze block, louvres,
    /// lattice).
    pub struct Screen {
        /// The unit's size, joints included: along the wall and up.
        unit: DVec2,
        /// The open rectangles of one unit, from its lower left corner.
        holes: Vec<Rect>,
        /// A solid border this wide round the screen.
        frame: f64 = 0.0,
        /// Every other course shifted half a unit along.
        stagger: bool = false,
    }
}

spec_struct! {
    /// A rectangle by its lower left and upper right corners.
    pub struct Rect {
        min: DVec2,
        max: DVec2,
    }
}

spec_enum! {
    /// The line a wall is drawn along and kept on when it gets thicker.
    /// The exterior is on the wall's right, looking from start to end,
    /// unless the wall is flipped.
    pub enum Location {
        Centre = "centre",
        CoreCentre = "core-centre",
        ExteriorFace = "exterior-face",
        InteriorFace = "interior-face",
        CoreExterior = "core-exterior",
        CoreInterior = "core-interior",
        /// One of the boundaries between layers, named by `boundary`.
        LayerBoundary = "layer-boundary",
    }
}

spec_struct! {
    /// How a door or window is built inside its opening.
    pub struct Frame {
        /// Face width of the frame members.
        width: f64 = 0.06,
        /// Depth of the frame through the wall.
        depth: f64 = 0.1,
        /// Window panes side by side.
        panes_across: u32 = 1,
        /// Window panes one above the other.
        panes_up: u32 = 1,
    }
}

spec_struct! {
    /// A storey. Elements hang off a level, so moving it moves them.
    pub struct Level {
        id: String,
        name: Option<String>,
        /// Height of its floor above the project's zero.
        elevation: f64,
        /// Floor-to-floor height: the height walls on it take unless
        /// told otherwise.
        height: f64 = 3.0,
    }
}

spec_struct! {
    /// A grid system: how its lines are labelled and coloured.
    pub struct GridGroup {
        id: String,
        name: Option<String>,
        /// Put before every label: "A-" gives A-1, A-2...
        prefix: String = String::new(),
        /// How lines running north-south are labelled, west to east.
        vertical: GridLabels = GridLabels::Letters,
        /// How lines running east-west are labelled, south to north.
        horizontal: GridLabels = GridLabels::Numbers,
        colour: Option<Colour>,
    }
}

spec_enum! {
    /// Grid labels.
    pub enum GridLabels {
        Numbers = "numbers",
        /// A, B, C... (I and O are skipped), then AA, AB...
        Letters = "letters",
    }
}

spec_struct! {
    /// A grid line.
    pub struct Grid {
        id: String,
        name: Option<String>,
        /// Its label after its group's prefix; the id when left out.
        label: Option<String>,
        start: DVec2,
        end: DVec2,
        /// A grid group's id.
        group: Option<String>,
    }
}

spec_struct! {
    /// A straight wall along its centreline on a level.
    pub struct Wall {
        id: String,
        name: Option<String>,
        /// Its mark, shown by tags and schedules; none numbers it
        /// automatically.
        mark: Option<String>,
        /// A level's id.
        level: String,
        start: DVec2,
        end: DVec2,
        /// A wall type's id: the wall takes its thickness, assembly,
        /// material, location line and height.
        r#type: Option<String>,
        /// Needed unless the wall has a type or an assembly.
        thickness: Option<f64>,
        /// Height above its base. Left out: up to `top_level` when given,
        /// else the type's height, else its level's height.
        height: Option<f64>,
        /// Its base above its level.
        base_offset: f64 = 0.0,
        /// A level's id: the top stays `top_offset` above that level as
        /// levels move.
        top_level: Option<String>,
        top_offset: f64 = 0.0,
        material: Option<String>,
        assembly: Option<String>,
        location: Option<Location>,
        /// With location `layer-boundary`: which boundary, counted from
        /// the exterior face (0) to the interior face (the layer count).
        boundary: Option<u32>,
        /// The exterior face on the left instead of the right.
        flip: bool = false,
        /// Roofs and slabs (ids) the top follows the underside of.
        attach_top: Vec<String>,
    }
}

spec_struct! {
    /// A door, window or bare hole in a wall, placed by its distance
    /// along the wall from the wall's start.
    pub struct Opening {
        id: String,
        name: Option<String>,
        mark: Option<String>,
        kind: OpeningKind,
        /// The host wall's id.
        wall: String,
        /// From the wall's start to the opening's near edge. Give this
        /// or `centre`.
        offset: Option<f64>,
        /// From the wall's start to the opening's middle.
        centre: Option<f64>,
        /// A door or window type's id: the opening takes its width,
        /// height, sill, frame and material.
        r#type: Option<String>,
        /// Needed unless the opening has a type.
        width: Option<f64>,
        /// Needed unless the opening has a type.
        height: Option<f64>,
        /// Its bottom above the wall's base. Left out: 0 for doors and
        /// holes, 0.9 m for windows (or the type's).
        sill: Option<f64>,
        material: Option<String>,
        frame: Option<Frame>,
        /// A door hinged on the jamb at the wall's end side.
        flip_hand: bool = false,
        /// A door opening to the wall's exterior; a window facing its
        /// interior.
        flip_facing: bool = false,
    }
}

spec_enum! {
    /// What an opening is.
    pub enum OpeningKind {
        Door = "door",
        Window = "window",
        Hole = "hole",
    }
}

spec_struct! {
    /// A floor slab: its outline pushed down from its top face.
    pub struct Slab {
        id: String,
        name: Option<String>,
        level: String,
        /// The corners in plan, either way round.
        outline: Vec<DVec2>,
        /// How each edge bends (corner i to i + 1): the tangent of a
        /// quarter of the arc's angle, positive bulging left. 0 or left
        /// out is straight.
        bulges: Vec<f64>,
        /// Holes through it, each an outline inside it.
        holes: Vec<Vec<DVec2>>,
        /// Needed unless the slab has a type or an assembly.
        thickness: Option<f64>,
        /// Its top face above its level; negative for below.
        top_offset: f64 = 0.0,
        r#type: Option<String>,
        material: Option<String>,
        assembly: Option<String>,
    }
}

spec_struct! {
    /// A roof over a wall-plate outline.
    pub struct Roof {
        id: String,
        name: Option<String>,
        level: String,
        /// Where the slopes start, in plan.
        outline: Vec<DVec2>,
        shape: RoofShape = RoofShape::Hip,
        /// Slope in degrees. Needed unless the roof is flat.
        pitch: Option<f64>,
        /// How far it reaches past the outline, in plan.
        overhang: f64 = 0.0,
        /// Square to the slope. Needed unless the roof has a type or an
        /// assembly.
        thickness: Option<f64>,
        base_offset: f64 = 0.0,
        /// The outline edge a shed roof rises from, or a gable roof's
        /// ridge runs along (edge i runs from corner i to i + 1).
        edge: u32 = 0,
        r#type: Option<String>,
        material: Option<String>,
        assembly: Option<String>,
        /// Holes cut upright through it (roof lights, a chimney).
        holes: Vec<Vec<DVec2>>,
    }
}

spec_enum! {
    /// A roof's shape.
    pub enum RoofShape {
        Flat = "flat",
        /// One slope, rising from the `edge`.
        Shed = "shed",
        /// Two slopes meeting at a ridge, with upright gable ends.
        Gable = "gable",
        /// A slope from every edge.
        Hip = "hip",
    }
}

spec_struct! {
    /// A column: a rectangular (`width` × `depth`) or round (`diameter`)
    /// profile standing on a level.
    pub struct Column {
        id: String,
        name: Option<String>,
        mark: Option<String>,
        level: String,
        /// The profile's centre in plan.
        at: DVec2,
        r#type: Option<String>,
        width: Option<f64>,
        depth: Option<f64>,
        diameter: Option<f64>,
        /// Turn of the profile, degrees anticlockwise.
        rotation: f64 = 0.0,
        base_offset: f64 = 0.0,
        /// A fixed height. Left out: up to `top_level`, else to the next
        /// level up.
        height: Option<f64>,
        top_level: Option<String>,
        top_offset: f64 = 0.0,
        material: Option<String>,
    }
}

spec_struct! {
    /// A beam, its top `top_offset` below the top of its storey.
    pub struct Beam {
        id: String,
        name: Option<String>,
        mark: Option<String>,
        level: String,
        start: DVec2,
        end: DVec2,
        r#type: Option<String>,
        width: Option<f64>,
        /// Height of the section.
        depth: Option<f64>,
        top_offset: f64 = 0.0,
        material: Option<String>,
    }
}

spec_struct! {
    /// A stair: one flight, or two with a landing between.
    pub struct Stair {
        id: String,
        name: Option<String>,
        level: String,
        /// The middle of the first riser, in plan.
        start: DVec2,
        /// Which way the first flight climbs, degrees anticlockwise from x.
        direction: f64 = 0.0,
        width: f64,
        /// The tallest riser allowed; the risers share the height equally.
        max_riser: f64 = 0.18,
        /// The depth of each tread.
        tread: f64 = 0.28,
        /// A fixed rise. Left out: up to the next level.
        height: Option<f64>,
        base_offset: f64 = 0.0,
        /// Thickness of the sloping slab under the steps.
        waist: f64 = 0.15,
        landing: Option<Landing>,
        material: Option<String>,
    }
}

spec_struct! {
    /// A landing that turns a stair.
    pub struct Landing {
        /// The number of risers before it.
        after: u32,
        turn: Side,
    }
}

spec_enum! {
    /// Left or right.
    pub enum Side {
        Left = "left",
        Right = "right",
    }
}

spec_struct! {
    /// A railing along a path, or along one side of a stair.
    pub struct Railing {
        id: String,
        name: Option<String>,
        level: String,
        /// Points of its path, heights above the level. Not used when it
        /// follows a stair.
        path: Vec<DVec3>,
        height: f64 = 1.0,
        post_spacing: f64 = 1.0,
        /// A stair's id: the railing follows its `side`.
        stair: Option<String>,
        side: Option<Side>,
        material: Option<String>,
    }
}

spec_struct! {
    /// An extrusion of a plan outline, or a sweep of a profile along a
    /// path. A void cuts what it meets (a firebox in a chimney, a hole
    /// through a slab) instead of being solid.
    pub struct Part {
        id: String,
        name: Option<String>,
        level: String,
        /// An extrusion's outline in plan.
        outline: Vec<DVec2>,
        /// An extrusion's holes.
        holes: Vec<Vec<DVec2>>,
        /// An extrusion's bottom above the level.
        base: f64 = 0.0,
        /// An extrusion's height.
        height: Option<f64>,
        /// An extrusion drawn on a face: its outline lies on this plane, and
        /// `base` and `height` run along the plane's normal (`x` × `y`).
        plane: Option<Plane>,
        /// A sweep's section: `x` across the path (positive to its left),
        /// `y` up from it.
        profile: Vec<DVec2>,
        /// A sweep's path through points, `z` above the level.
        path: Vec<DVec3>,
        /// The path runs from its last point back to its first.
        closed: bool = false,
        /// A path drawn on a face: the face's outward normal, which the
        /// profile's `y` runs along instead of up.
        face_normal: Option<DVec3>,
        /// A sweep round an arc: the profile rides the arc, its `y` along
        /// the turn axis. A full turn (sweep 360) closes on itself.
        arc: Option<ArcPath>,
        /// A sweep along a wall's id, at its top or base: it follows the
        /// wall (a coping, a plinth).
        wall: Option<String>,
        edge: WallEdge = WallEdge::Top,
        /// Lifts a wall path off its edge.
        offset: f64 = 0.0,
        /// Runs a wall path on past the wall's start and end.
        extend: Option<DVec2>,
        void: bool = false,
        material: Option<String>,
        /// The IFC class it exports as.
        ifc_class: Option<String>,
    }
}

spec_struct! {
    /// A plane anywhere in the model: its origin (height above the level)
    /// and two square unit axes along it.
    pub struct Plane {
        origin: DVec3,
        x: DVec3,
        y: DVec3,
    }
}

spec_struct! {
    /// A sweep's arc: a point on the turn axis, the turn direction, where
    /// the arc begins and how far it turns (degrees, positive by the
    /// right-hand rule about the axis). A full turn closes on itself. The
    /// profile rides the arc: its `y` runs along `axis`, its `x` to the
    /// path's left.
    pub struct ArcPath {
        center: DVec3,
        axis: DVec3,
        start: DVec3,
        sweep: f64,
    }
}

spec_enum! {
    /// A wall's top or base.
    pub enum WallEdge {
        Top = "top",
        Base = "base",
    }
}

spec_struct! {
    /// A site element: terrain, a hard landscape area or a plant.
    pub struct SiteElement {
        id: String,
        name: Option<String>,
        kind: SiteKind,
        level: String,
        /// Terrain and area: the outline.
        outline: Vec<DVec2>,
        /// An area's holes.
        holes: Vec<Vec<DVec2>>,
        /// Terrain and area: the top above the level.
        top: f64 = 0.0,
        /// Terrain and area: how deep.
        thickness: Option<f64>,
        /// Terrain: spot heights `[x, y, z]`, `z` above the level.
        points: Vec<DVec3>,
        /// Terrain: a contour every so much height; 0 draws none.
        contour: f64 = 0.0,
        /// An area's surface pattern.
        pattern: Option<SitePattern>,
        /// A plant's position.
        at: Option<DVec2>,
        /// A plant's base above the level.
        base: f64 = 0.0,
        plant: PlantForm = PlantForm::Tree,
        /// A plant's crown across.
        canopy: Option<f64>,
        /// A plant's height.
        height: Option<f64>,
        material: Option<String>,
    }
}

spec_enum! {
    /// What a site element is.
    pub enum SiteKind {
        Terrain = "terrain",
        Area = "area",
        Plant = "plant",
    }
}

spec_struct! {
    /// An area's surface: units (`size`, `stagger`, `angle`), lines
    /// (`spacing`, `angle`) or stipple (`spacing`).
    pub struct SitePattern {
        kind: PatternKind,
        size: Option<DVec2>,
        stagger: bool = false,
        angle: f64 = 0.0,
        spacing: Option<f64>,
    }
}

spec_enum! {
    /// A surface pattern.
    pub enum PatternKind {
        None = "none",
        /// Flags, blocks, setts.
        Units = "units",
        /// Boards, decking.
        Lines = "lines",
        /// Gravel, sand.
        Stipple = "stipple",
    }
}

spec_enum! {
    /// A plant's form.
    pub enum PlantForm {
        Tree = "tree",
        Conifer = "conifer",
        Palm = "palm",
        Shrub = "shrub",
    }
}

spec_struct! {
    /// Furniture or a fixture from CreaBIM's catalogue.
    pub struct Object {
        id: String,
        name: Option<String>,
        mark: Option<String>,
        level: String,
        /// The catalogue object's key: "starter/sofa-3" (`creabim
        /// catalogue` lists them; "starter/" may be left out).
        catalogue: String,
        /// Where it goes in plan: its origin (the middle of its back
        /// edge), or its middle with `anchor` "centre".
        at: DVec2,
        anchor: Anchor = Anchor::Origin,
        /// Turn, degrees anticlockwise. At 0 its back is towards +y.
        rotation: f64 = 0.0,
        /// Its floor above the level (for one hung from the ceiling, its
        /// top below the ceiling). Left out: the catalogue's.
        offset: Option<f64>,
    }
}

spec_enum! {
    /// Which point of an object `at` gives.
    pub enum Anchor {
        /// The middle of its back edge.
        Origin = "origin",
        /// The middle of its footprint.
        Centre = "centre",
    }
}

spec_struct! {
    /// A room: the space the walls close off round `at`, or an outline
    /// drawn for it.
    pub struct Room {
        id: String,
        name: Option<String>,
        number: String = String::new(),
        level: String,
        /// A point inside it, where its tag sits.
        at: DVec2,
        /// An outline of its own; left out, the walls round `at` bound it.
        outline: Vec<DVec2>,
        /// Its top above its level. Left out: to the ceiling, or the
        /// level's height.
        height: Option<f64>,
        floor_finish: String = String::new(),
        wall_finish: String = String::new(),
        ceiling_finish: String = String::new(),
        occupancy: String = String::new(),
        department: String = String::new(),
    }
}

spec_struct! {
    /// A view: a plan of a level, a section or elevation along a line, a
    /// 3D view, or a schedule. Which fields it uses depends on its
    /// `kind`: plan: `level`, `cut_height`, `ceiling_plan`; section and
    /// elevation: `start`, `end`, `depth`; 3d: `azimuth`, `altitude`;
    /// schedule: `category`, `by_type`.
    pub struct View {
        id: String,
        /// Its title; the id when left out.
        name: Option<String>,
        kind: ViewKind,
        /// The scale's denominator: 100 is 1:100.
        scale: f64 = 100.0,
        /// Left out: shaded for 3D views, hidden line for the rest.
        style: Option<ViewStyle>,
        /// A plan's level.
        level: Option<String>,
        /// A plan's cut above its level.
        cut_height: f64 = 1.2,
        /// A reflected ceiling plan.
        ceiling_plan: bool = false,
        /// Each room washed in a colour of its own.
        room_colours: bool = false,
        /// A section or elevation's line. It looks to the left of start →
        /// end: a line drawn west to east looks north.
        start: Option<DVec2>,
        end: Option<DVec2>,
        /// How far a section or elevation looks.
        depth: Option<f64>,
        /// A 3D view's compass bearing: 0 from the south, anticlockwise.
        azimuth: f64 = 30.0,
        /// A 3D view's angle above the horizon.
        altitude: f64 = 30.0,
        /// What a schedule lists.
        category: Option<ScheduleCategory>,
        /// A schedule's rows grouped by type.
        by_type: bool = false,
        /// Makes it a callout of another view: an enlarged part of it.
        /// Give it the same kind and line (or level) as its parent.
        callout: Option<Callout>,
    }
}

spec_struct! {
    /// The box of a parent view a callout enlarges, in that view's
    /// drawing coordinates.
    pub struct Callout {
        /// The parent view's id.
        parent: String,
        min: DVec2,
        max: DVec2,
    }
}

spec_enum! {
    /// What a view is.
    pub enum ViewKind {
        Plan = "plan",
        /// Cut along its line.
        Section = "section",
        /// Seen from its line, nothing cut.
        Elevation = "elevation",
        /// A parallel projection of the whole model.
        ThreeD = "3d",
        Schedule = "schedule",
    }
}

spec_enum! {
    /// How a view draws.
    pub enum ViewStyle {
        HiddenLine = "hidden-line",
        Shaded = "shaded",
        Wireframe = "wireframe",
    }
}

spec_enum! {
    /// What a schedule lists.
    pub enum ScheduleCategory {
        Doors = "doors",
        Windows = "windows",
        Walls = "walls",
        Slabs = "slabs",
        /// Columns and beams.
        Structure = "structure",
        Furniture = "furniture",
        Fixtures = "fixtures",
        Rooms = "rooms",
        /// The take-off by material.
        Materials = "materials",
    }
}

spec_struct! {
    /// A drawing sheet, with views placed on it. Its id is its number.
    pub struct Sheet {
        /// Its number: "A101".
        id: String,
        /// Its title; the number when left out.
        name: Option<String>,
        /// An ISO paper size, landscape. Give this or `width` and `height`.
        size: Option<PaperSize>,
        /// In millimetres.
        width: Option<f64>,
        height: Option<f64>,
        /// Title block fields by name.
        fields: BTreeMap<String, String>,
        viewports: Vec<Viewport>,
    }
}

spec_enum! {
    /// ISO paper sizes, landscape.
    pub enum PaperSize {
        A0 = "A0",
        A1 = "A1",
        A2 = "A2",
        A3 = "A3",
        A4 = "A4",
    }
}

spec_struct! {
    /// A tag in a view, showing an element's mark (a door's, a window's)
    /// or a room's name, number and area. It moves with its element.
    pub struct Tag {
        /// A view's id.
        view: String,
        /// The id of the element it tags.
        element: String,
        /// Where it sits, from the element's middle, in drawing units.
        offset: DVec2 = DVec2::ZERO,
        /// A bend in its leader, from the element's middle.
        elbow: Option<DVec2>,
        /// Where its leader ends, from the element's middle; left out, at
        /// the middle.
        end: Option<DVec2>,
    }
}

spec_struct! {
    /// Text in a view (drawing coordinates, in the file's units) or on a
    /// sheet (paper millimetres).
    pub struct Note {
        /// A view's id, or a sheet's.
        view: String,
        /// Where its first line starts: its left end, mid-height.
        at: DVec2,
        /// Line breaks (`\n`) start new lines.
        text: String,
        /// A leader from the text to this point, in the same units as `at`.
        leader: Option<DVec2>,
        /// A bend in the leader.
        elbow: Option<DVec2>,
        colour: Option<Colour>,
    }
}

spec_struct! {
    /// A view placed on a sheet.
    pub struct Viewport {
        /// A view's id.
        view: String,
        /// Where on the sheet, in millimetres from its lower left corner.
        at: DVec2,
        /// The drawing point (in the file's units, as the drawing's x
        /// and y) put at `at`. Left out: the middle of the crop, or of the
        /// whole drawing.
        target: Option<DVec2>,
        /// Only this part of the drawing is shown (drawing coordinates).
        crop: Option<Rect>,
    }
}
