//! The 2D plan view: draws the generated plan of a level and hosts the
//! drawing tools. Edits made here are ordinary model edits, so the 3D view
//! and any other open view update at once.

use crate::boxsel;
use crate::commands::Command;
use crate::handles::{self, PaneInput};
use crate::hoverpick::{self, Pane};
use crate::modal::{self, Flow, Modal, Mode, Outcome, PaneMap, Space, Stage};
use crate::outline::Next;
use crate::selection::Selection;
use crate::sketch::{level_below, Click, SketchDraft};
use crate::snaps::{self, SnapKind, Snapper};
use creabim_core::Workplane;
use creabim_core::{
    Dimension, Document, EditMesh, Element, ElementId, ElementKind, FreeMesh, HostPoint, Opening,
    OpeningKind, Slab, Tag, Transaction, View, ViewKind, Wall,
};
use creabim_geom::annotate::{self, dimension_geometry, tag_geometry};
use creabim_geom::annotation::TextLook;
use creabim_geom::SceneItem;
use creabim_geom::{paper, plan::Pen, section::Drawing, triangulate::triangulate, PlanItem};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use glam::{DVec2, DVec3};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Wall,
    Door,
    Window,
    /// A bare opening in a wall: a hole with no door or window.
    Opening,
    Slab,
    /// A vertical hole through the slabs and roofs between two levels.
    Shaft,
    Mass,
    Section,
    Elevation,
    Dimension,
    Tag,
    Angle,
    Radius,
    Diameter,
    ArcLength,
    Note,
    /// A spot level or a level marker: the height of a point.
    SpotLevel,
    Sketch,
    Guide,
    Grid,
    Column,
    Beam,
    Roof,
    Stair,
    Railing,
    /// A strip footing under a wall or a pad footing under a column.
    Footing,
    Level,
    Align,
    /// A piece of furniture or a fixture from the catalogue.
    Object,
    /// A plan outline pulled up into a solid.
    Extrusion,
    /// A profile carried along a wall or a path: copings, plinths, steps.
    Sweep,
    /// A solid that cuts what it meets.
    Void,
    /// The ground: a surface over an outline.
    Terrain,
    /// Paving, gravel, a driveway: hard landscape with a pattern.
    Paving,
    /// Trees and shrubs.
    Planting,
    /// A room: click inside walls, or draw its outline.
    Room,
    /// A line that splits a room in two where there is no wall.
    RoomSeparator,
    /// A ceiling under a room, or drawn.
    Ceiling,
    /// A box on a plan or section that makes a larger-scale detail view.
    Callout,
    /// A detail component: flashing, membrane, insulation, blocking...
    Detail,
}

impl Tool {
    pub const ALL: [Tool; 41] = [
        Tool::Select,
        Tool::Wall,
        Tool::Door,
        Tool::Window,
        Tool::Opening,
        Tool::Slab,
        Tool::Shaft,
        Tool::Mass,
        Tool::Column,
        Tool::Beam,
        Tool::Roof,
        Tool::Stair,
        Tool::Railing,
        Tool::Footing,
        Tool::Object,
        Tool::Extrusion,
        Tool::Sweep,
        Tool::Void,
        Tool::Terrain,
        Tool::Paving,
        Tool::Planting,
        Tool::Room,
        Tool::RoomSeparator,
        Tool::Ceiling,
        Tool::Section,
        Tool::Elevation,
        Tool::Callout,
        Tool::Dimension,
        Tool::Tag,
        Tool::Angle,
        Tool::Radius,
        Tool::Diameter,
        Tool::ArcLength,
        Tool::Note,
        Tool::Detail,
        Tool::SpotLevel,
        Tool::Sketch,
        Tool::Guide,
        Tool::Grid,
        Tool::Level,
        Tool::Align,
    ];

    /// Dimensions, tags and notes: tools that annotate a view.
    pub fn is_annotation(self) -> bool {
        matches!(self, Tool::Dimension | Tool::Tag) || crate::annotate::is_tool(self)
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Wall => "Wall",
            Tool::Door => "Door",
            Tool::Window => "Window",
            Tool::Opening => "Wall opening",
            Tool::Slab => "Slab",
            Tool::Shaft => "Shaft",
            Tool::Mass => "Mass",
            Tool::Section => "Section",
            Tool::Elevation => "Elevation",
            Tool::Dimension => "Dimension",
            Tool::Tag => "Tag",
            Tool::Angle => "Angle",
            Tool::Radius => "Radius",
            Tool::Diameter => "Diameter",
            Tool::ArcLength => "Arc length",
            Tool::Note => "Note",
            Tool::SpotLevel => "Spot level",
            Tool::Sketch => "Sketch",
            Tool::Guide => "Guide",
            Tool::Grid => "Grid",
            Tool::Column => "Column",
            Tool::Beam => "Beam",
            Tool::Roof => "Roof",
            Tool::Stair => "Stair",
            Tool::Railing => "Railing",
            Tool::Footing => "Footing",
            Tool::Level => "Level",
            Tool::Align => "Align",
            Tool::Object => "Object",
            Tool::Extrusion => "Extrusion",
            Tool::Sweep => "Sweep",
            Tool::Void => "Void",
            Tool::Terrain => "Terrain",
            Tool::Paving => "Hard landscape",
            Tool::Planting => "Planting",
            Tool::Room => "Room",
            Tool::RoomSeparator => "Room separator",
            Tool::Ceiling => "Ceiling",
            Tool::Callout => "Callout",
            Tool::Detail => "Detail component",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Click an element to select it. Drag a selected element to move it. M (Move) moves it by a base point; over a view, G moves it with the pointer, R rotates, S scales, Shift+D duplicates.",
            Tool::Wall => "Click to start, click again to place each wall. Esc or right-click ends the chain. Shift keeps it orthogonal.",
            Tool::Door => "Click on a wall to place a door.",
            Tool::Window => "Click on a wall to place a window.",
            Tool::Opening => "Click on a wall to cut a bare rectangular opening in it: no door or window, just the hole. Set its size and sill in Properties.",
            Tool::Shaft => "Draw the shaft's outline like a slab (1–4: Polygon, Rectangle, Fill room, From sketch). It runs from this level to the one above, cutting every slab and roof in between.",
            Tool::Slab => "Click the slab's corners, then the first again or Enter. A draws arcs, L lines; Esc or right-click takes back a point, right-click again leaves. 1–4: Polygon, Rectangle, Fill room, From sketch.",
            Tool::Mass => "Click to place a freeform mass you can then edit like a mesh.",
            Tool::Section => "Click two points for the section line. It looks to the left of the direction you draw.",
            Tool::Elevation => "Click two points for the elevation line, outside the building. It looks to the left of the direction you draw.",
            Tool::Dimension => "Click two points to measure, then click where the dimension line goes. Points snap to wall corners, door and window edges and levels, and stay attached to them.",
            Tool::Tag => "Click a door, window, wall, slab, roof, column, beam, stair, railing, room or ceiling, then click where its tag goes.",
            Tool::Angle => "Click two lines (wall faces, edges) for the angle between them, or click the corner and a point on each arm. Then click where the arc goes.",
            Tool::Radius => "Click an arc or circle, or three points on one. Then click where the dimension line goes.",
            Tool::Diameter => "Click an arc or circle, or three points on one. Then click where the dimension line goes.",
            Tool::ArcLength => "Click an arc, or its start, a point on it and its end. Then click where the dimension line goes.",
            Tool::SpotLevel => "Click a point on a floor, slab or roof in a section or elevation, or on a slab in a plan (elsewhere in a plan it marks the plan's level), then click where its text goes. Its text pattern is in Properties.",
            Tool::Note => "Click what the note points at, then where its text goes (turn Leader off to just click). Type its text in Properties. Works on sheets too.",
            Tool::Sketch => "Click the corners of a sketch on this level. Click the first corner to close it, Enter or right-click to leave it open, Esc to cancel. L, A and C draw lines, arcs and circles. Then E extrudes it.",
            Tool::Guide => "Click two points for a guide through them. Or click an edge, then click where its parallel guide goes. Everything snaps to guides.",
            Tool::Grid => "Click two points for a grid line. It goes in the active grid system (top bar) and takes its next label. Shift keeps it straight.",
            Tool::Column => "Click to place a column. It runs up to the next level; set its size in the top bar or Properties.",
            Tool::Beam => "Click the beam's start, then its end; keep clicking to chain beams. Its top sits at the top of the storey. Esc or right-click ends.",
            Tool::Roof => "Draw the roof's outline like a slab (1–4: Polygon, Rectangle, Fill room, From sketch). Pick Flat, Mono-pitch, Gable or Hip in the bar first.",
            Tool::Stair => "Click where the stair starts, then click the way it climbs. It rises to the next level in equal risers.",
            Tool::Railing => "Click inside a stair to rail both its sides, or click the points of a path; click the last point again to finish.",
            Tool::Footing => "Showing footings while the tool is on. Strip: click a wall to put a strip footing under it, or click two points for one on its own. Pad: click a column to put a pad under it, or click anywhere for one on its own. Keep clicking for more, or drag a box over walls or columns for all of them at once. Pick Strip or Pad, and the size, in the top bar.",
            Tool::Level => "In a section or elevation, click where the level line starts (its height), then where it ends; its floor plan is made with it. In other views the Levels window opens to add, rename and move levels.",
            Tool::Align => "Click a reference line (a wall face, slab edge, guide or grid line), then the edge of what to align: it turns parallel and moves flush. In Extend mode a wall, beam or grid line end stretches to the reference instead; in Stretch mode the face moves and the rest stays. Centre lines (dashed purple) can be picked too, both ways. In 3D, click a face, edge, corner, centre line or grid line, then a face.",
            Tool::Object => "Click to place the object picked in the Catalogue window. Sinks, WCs, kitchen units, beds and sofas back onto the nearest wall; Space turns the next one a quarter.",
            Tool::Extrusion => "Draw the outline like a slab (1–4: Polygon, Rectangle, Fill room, From sketch). It is pulled up from its base by its height, set in the bar: a chimney, a hearth, a pier. In the 3D view, draw it on any face: it stands square out of that face.",
            Tool::Sweep => "Click a wall to run the profile along its top (coping) or base (plinth); it follows the wall. Or click the points of a path, and the last one again to finish. Pick the profile and its sizes in the bar.",
            Tool::Void => "Draw the void's outline like a slab. It cuts every wall, slab, roof, site surface and extrusion it meets, between its base and height: a firebox, a hole through a floor. In the 3D view, draw it on any face, a sloped roof or a wall: it cuts square into that face, as deep as Depth on a face.",
            Tool::Terrain => "Draw the ground's outline like a slab. It starts flat at the level; add spot heights in Properties to shape it. Paving drawn inside it sits flush.",
            Tool::Paving => "Draw paving, gravel, a driveway or decking like a slab. Pick what in the bar; each has its own pattern and material.",
            Tool::Planting => "Click to plant a tree, conifer, palm or shrub. Pick the form, canopy and height in the bar.",
            Tool::Room => "Click inside walls for a room that follows them (3, Fill room), or draw its outline (1 Polygon, 2 Rectangle, 4 From sketch) where no walls close it off. Its name, number and finishes are in Properties.",
            Tool::RoomSeparator => "Click two points for a line that splits a room in two where there is no wall, such as between a kitchen and a dining room.",
            Tool::Callout => "On a plan or section, click two corners of a box round what to detail: it becomes a detail view at the scale in the top bar, numbered in a bubble on this view.",
            Tool::Detail => "Pick what to draw in the top bar, then click its points; Enter, a double-click or a right-click finishes (the first point again closes a region). Esc keeps what is drawn. Detail components show in this view only.",
            Tool::Ceiling => "Click inside a room for a ceiling over it that follows its walls (3, Fill room), or draw its outline (1 Polygon, 2 Rectangle, 4 From sketch). Set its height and thickness in the top bar or Properties.",
        }
    }

    /// Tools that draw an outline the way the Slab tool does.
    pub fn draws_outline(self) -> bool {
        matches!(
            self,
            Tool::Slab
                | Tool::Roof
                | Tool::Shaft
                | Tool::Extrusion
                | Tool::Void
                | Tool::Terrain
                | Tool::Paving
                | Tool::Room
                | Tool::Ceiling
        )
    }

    /// The hint for this tool in a section or elevation.
    fn section_hint(self) -> &'static str {
        match self {
            Tool::Select => "Click to select, Shift+click to add, drag on empty paper for a box. M moves by a base point, G with the pointer, R tilts, S scales, Shift+D duplicates.",
            Tool::Wall => "Click two points: the wall runs along the section line, as high as you draw it.",
            Tool::Slab => "Click two corners of the slab's edge. Its top snaps to the nearest level; it runs back as far as the walls.",
            Tool::Section => "Click where the new section cuts, then click on the side it should look towards.",
            Tool::Elevation => "Click where the new elevation stands, then click on the side it should look towards.",
            Tool::Dimension => "Click two points to measure, then click where the dimension line goes. Points snap to levels.",
            Tool::Guide => "Click two points for a guide on the section plane. It shows here, in 3D and in any section on the same plane.",
            Tool::Level => "Click where the new level line starts (it sets the height; it snaps to model points), then where it ends. Its floor plan is made with it.",
            Tool::Sketch => "Click the corners of a sketch on the section plane. Click the first corner to close it, Enter or right-click to leave it open. L, A and C draw lines, arcs and circles. Then E extrudes it.",
            t => t.hint(),
        }
    }
}

/// What kind of drawing the 2D pane is showing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaneKind {
    /// A plan of a level: every drawing tool works here.
    Plan { level: ElementId },
    /// A section or elevation along `start` → `end`. Walls, doors, windows
    /// and masses can be drawn here too; drawing x runs along the line and
    /// y is height.
    Section { start: DVec2, end: DVec2 },
    /// A parallel 3D view: drag to orbit, click to select.
    Axonometric,
    /// A schedule drawn as a table: click a row to select its element.
    Schedule,
}

impl PaneKind {
    /// World plan position of a section drawing point on the section plane.
    fn section_point(start: DVec2, end: DVec2, p: DVec2) -> DVec2 {
        start + (end - start).normalize_or_zero() * p.x
    }
}

/// How a pane is shown: on its own, or inside a viewport on a sheet.
#[derive(Clone, Copy, Debug)]
pub struct PaneStyle {
    /// Background fill; `None` leaves what is behind (the sheet) visible.
    pub background: Option<Color32>,
    /// Metric grid and tool hint.
    pub decorations: bool,
    /// Fixed pan and zoom (centre, pixels per metre) instead of the view's own.
    pub camera: Option<(DVec2, f32)>,
    /// Drawing scale denominator, for hatch spacing and annotation sizes.
    pub scale: f64,
}

impl PaneStyle {
    /// Drawing units per paper millimetre.
    pub fn mm(&self) -> f64 {
        self.scale / 1000.0
    }
}

/// Which part of a selected section line is being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Start,
    End,
    Line,
}

pub struct PlanState {
    /// Pan and zoom per view: centre in drawing units, pixels per metre.
    pub cameras: HashMap<ElementId, (DVec2, f32)>,
    /// While Divide is open: where it will divide (model lines; true: plans
    /// show it), and each part's size at its middle (MU38).
    pub divide_marks: Vec<(DVec3, DVec3, bool)>,
    pub divide_labels: Vec<(DVec3, String)>,
    /// The element a click would place, from the view the pointer is in
    /// (its view, or None for 3D): set as it is drawn there, and shown a
    /// frame later in every other view, so placing a mass in 3D shows it
    /// in the plans too (MU39).
    /// Keyed by the pane (two panes may show the same view; the 3D view
    /// is `usize::MAX`).
    pub ghost_now: Option<(Option<usize>, crate::ghost::Ghost)>,
    pub ghost_shared: Option<(Option<usize>, crate::ghost::Ghost)>,
    /// The pane being drawn now, while the areas draw theirs.
    pub pane_now: Option<usize>,
    /// Divide is making guides: its marks show as guides do (MU39).
    pub divide_guides: bool,
    /// While a point is asked for (a radial array's centre): the next
    /// click in a plan gives it, in model x and y (MU39).
    pub point_pick: bool,
    pub point_picked: Option<DVec2>,
    /// While a point is asked for: where the pointer is, and (when
    /// `point_lines`) the drawing's straight line under it, which a click
    /// takes instead of a point (Split's cutting line, MU39).
    pub point_hover: Option<DVec2>,
    /// The first point of a line being picked (Split's), for tracking.
    pub point_from: Option<DVec2>,
    pub point_lines: bool,
    pub point_line_picked: Option<(DVec2, DVec2)>,
    /// Lines and see-through planes a modifier being set shows in the
    /// views (its centre, its mirror plane, its cuts), in model space.
    pub mod_guides: crate::modguides::Guides,
    /// A view to show at its drawing scale on the next frame: 1 paper mm
    /// on 1 screen mm (A5-7, the view's More menu).
    pub zoom_to_scale: Option<ElementId>,
    /// Sheets show their rulers and guides, and snap to them (MU27).
    pub sheet_guides: bool,
    /// A note double-clicked on a sheet, to open in its editor (MU27).
    pub edit_note: Option<ElementId>,
    /// A second area showing the same view pans and zooms on its own
    /// (A4-4): the area being drawn when it does, and those cameras.
    pub area: Option<usize>,
    pub area_cameras: HashMap<(usize, ElementId), (DVec2, f32)>,
    /// Areas kept in step with the first area showing their view.
    pub linked_areas: std::collections::HashSet<usize>,
    /// What the pane under the pointer does, for the status bar.
    pub hint: Option<&'static str>,
    pub tool: Tool,
    /// First click of a two-click tool (wall start, slab corner).
    pub pending: Option<DVec2>,
    pub wall_thickness: f64,
    /// The line new walls are drawn along, and which side their exterior
    /// is on (the right of the direction drawn, or flipped its left).
    pub wall_location: creabim_core::LocationLine,
    pub wall_flip: bool,
    /// What the Wall tool draws (MU40): 0 straight, 1 an arc (start, end,
    /// then a point it passes through), 2 a circle (centre, then radius).
    pub wall_shape: u8,
    /// An arc wall's end, once clicked: the next click bends it.
    pub wall_arc_end: Option<DVec2>,
    /// Section line handle being dragged, and the line when the drag began.
    drag: Option<(Handle, DVec2, DVec2, DVec2)>,
    /// Dimension being drawn: its points so far, attached and as drawn.
    dim: Vec<(HostPoint, DVec2)>,
    /// Element picked for a tag, waiting for the tag's position.
    tag_host: Option<ElementId>,
    /// Tag or dimension being dragged: where the drag began and its offset then.
    anno_drag: Option<(ElementId, DVec2, DVec2)>,
    /// The area whose drag holds `anno_drag`, `level_drag` or `drag`:
    /// another area, not dragged, must not end it (MU32: a dimension moved
    /// a step per drag).
    drag_owner: Option<egui::Id>,
    /// Angles, radii, arc lengths and notes being placed or dragged.
    pub anno: crate::annotate::Draft,
    /// A keyboard move, rotate or scale, in whichever pane started it.
    pub modal: Option<Modal>,
    /// A sketch being drawn, in whichever pane started it.
    pub sketch: Option<SketchDraft>,
    /// A handle or selected element being dragged in a plan or section.
    pub(crate) grip: Option<handles::Drag>,
    /// The element open in Edit mode (see [`crate::editmode`]): all its
    /// grips show, in every view.
    pub edit: Option<ElementId>,
    /// The corners and holes picked in Edit mode, in every view.
    pub grips: handles::GripSel,
    /// Snaps for each 2D view: built from its drawing without the listed
    /// elements, at a model revision.
    snaps: HashMap<ElementId, (u64, Vec<ElementId>, Snapper)>,
    /// A message for the status bar, taken by the app each frame.
    pub status: Option<String>,
    /// The edge a guide is drawn parallel to, picked by its first click.
    pub guide_edge: Option<(DVec2, DVec2)>,
    /// A slab outline being drawn, or a slab outline or wall path being
    /// redrawn.
    pub outline: Option<crate::outline::Outline>,
    /// How the Slab tool draws.
    pub slab_mode: crate::slabtool::SlabMode,
    /// The next side of a slab outline is an arc.
    pub slab_arc: bool,
    /// A new slab thickness keeps the underside where it is (the top
    /// moves) instead of the top.
    pub slab_keep_bottom: bool,
    /// Where a box selection began, on screen.
    /// A box being dragged: where it started, and the area dragging it
    /// (two areas may show one view; only that one ends it, MU21).
    pub boxsel: Option<(Pos2, egui::Id)>,
    /// What rotate and scale turn about.
    pub pivot: modal::Pivot,
    /// What the Move tool last worked on: whole elements or a face.
    pub move_target: modal::Target,
    /// The pivot cursor, placed with Shift+right-click.
    pub cursor: Option<DVec3>,
    /// A move, rotate or scale picked from a button or menu, started by the
    /// next view the pointer is over: (mode, copy first).
    pub armed: Option<(Mode, bool)>,
    /// With [`Mode::Grip`] armed: the grip it drives (from the edge palette).
    pub armed_grip: Option<crate::handles::HandleKind>,
    /// With [`Mode::Offset`] armed: change the original, no copy (the edge
    /// palette's *Offset all*).
    pub in_place: bool,
    /// Commands picked from a view's right-click menu, run by the app, with
    /// the model point the menu was opened at.
    pub commands: Vec<(Command, Option<DVec3>)>,
    /// Where the right-click menu was opened, in the model.
    pub menu_at: Option<DVec3>,
    /// Menus are drawn by the web interface: a right-click asks for one at
    /// this point on screen instead of opening egui's.
    pub web_menus: bool,
    pub menu_request: Option<egui::Pos2>,
    /// Which snaps are on, in every pane and the 3D view.
    pub snap: snaps::SnapSettings,
    /// The grid system the Grid tool puts new lines in.
    pub grid_group: Option<ElementId>,
    /// A level being dragged in a section or elevation, and how far above
    /// the pointer its line was when the drag began.
    level_drag: Option<(ElementId, f64)>,
    /// Sizes for new columns, beams, roofs, stairs and railings, and a
    /// railing path being clicked.
    pub members: crate::memberstool::MemberSettings,
    /// Sizes for new extrusions, sweeps, voids and site elements, and a
    /// sweep path being clicked.
    pub parts: crate::partstool::PartSettings,
    /// The Level tool was used where level lines can't be drawn: the app
    /// opens the Levels window instead.
    pub want_levels: bool,
    /// What the Object tool places.
    pub objects: crate::objecttool::ObjectSettings,
    /// The Object tool was used with nothing picked: the app opens the
    /// Catalogue window.
    pub want_catalogue: bool,
    /// Walls or columns a box dragged with the Footing tool took: the app
    /// puts footings under them all, in one step.
    pub want_footings: Vec<ElementId>,
    /// A box selection just made: the app offers Keep only… while the
    /// selection is still what the box took.
    pub boxed: bool,
    /// Elements shown as if selected without being so (a type's instances,
    /// from its Show in model), and whether the rest is drawn faint.
    pub spotlight: Vec<ElementId>,
    pub spotlight_ghost: bool,
    /// The points clicked so far for a chain of walls (or beams, grid
    /// lines...), on a level, for alignment tracking.
    pub chain: (Option<ElementId>, Vec<DVec2>),
    /// Points the pointer rested on while drawing in a view, tracked like
    /// the shape's own points, and the point it is resting on (since when).
    rested: (Option<ElementId>, Vec<DVec2>),
    rest: Option<(DVec2, f64)>,
    /// Snap references in each view: points and edges the pointer rested
    /// on, that temporary guide lines run from. They stay while you work,
    /// until rested on again, cleared (Esc with nothing to cancel), or kept.
    pub refs: HashMap<ElementId, Vec<snaps::Reference>>,
    /// When each view's references were last picked or followed: they go
    /// after [`REF_FADE`] seconds unused, so none linger until Esc.
    refs_used: HashMap<ElementId, f64>,
    /// What the pointer is resting on for a reference: since when, and
    /// whether that rest has already picked (or dropped) it.
    ref_rest: Option<(snaps::Reference, f64, bool)>,
    /// Angled guides every this many degrees through references and the
    /// start point; 0 for none.
    pub guide_step: f64,
    /// The temporary guide under the pointer in the pane it is over, in
    /// that pane's drawing: what K keeps as a guide.
    pub hover_guide: Option<(ElementId, snaps::TempGuide)>,
    /// The Dimension tool's axis; none picks it from where the line goes.
    pub dim_axis: Option<creabim_core::DimAxis>,
    /// Continuous dimensions (MU23 board): each click after the first
    /// dimension adds the next on the same line.
    pub dim_chain: bool,
    /// The line a chain runs on: its axis and a point on it.
    pub chain_line: Option<(creabim_core::DimAxis, DVec2)>,
    /// Rows of dimensions land on steps of this many paper mm from what
    /// they measure (0: free).
    pub dim_step_mm: f64,
    /// The spacing typed for that, kept while it is off.
    pub dim_step_size: f64,
    /// The axis a dimension is held to while Shift is down.
    dim_lock: Option<creabim_core::DimAxis>,
    /// The Align tool's picks so far.
    pub align: crate::aligntool::AlignState,
    /// What the pointer is over before a click, and what else is under it.
    pub hover: Option<crate::hoverpick::HoverPick>,
    /// The drawing views seen in X-ray while working: what holds others
    /// see-through, what is inside them drawn and picked first.
    pub xray: HashSet<ElementId>,
    /// Each view's model as its view rays pick from, with the revision it
    /// was built at.
    pick_scenes: HashMap<ElementId, (u64, Arc<Vec<SceneItem>>)>,
    /// Each 3D view's frame and snaps for the Align tool, with the
    /// revision they were made at.
    axo_snaps: HashMap<ElementId, (u64, Arc<(creabim_geom::section::AxoFrame, Snapper)>)>,
    /// Each plan's and section's centre lines for the Align tool, with the
    /// revision they were made at.
    centre_lines: HashMap<ElementId, (u64, Arc<Vec<crate::aligntool::CentreLine>>)>,
    /// The type each tool draws new elements as.
    pub types: HashMap<creabim_core::TypeCategory, ElementId>,
    /// What the Detail and Callout tools draw, and the Detail tool's
    /// points so far.
    pub details: crate::detailtool::DetailSettings,
    /// A view made in a pane that the app should open: a new detail view.
    pub want_view: Option<ElementId>,
    /// The Snap menu's Custom step being typed.
    pub step_prompt: Option<String>,
    /// The tracker's typed length and angle while a line is drawn (2.10).
    pub tracker: crate::tracker::Tracker,
    /// The temporary dimension being typed into (2.10 part 3).
    pub temp: crate::tempdims::TempState,
    /// The host a host tool's click goes on, as the pointer shows it, and
    /// how many times Tab has moved past the first (A3-2).
    /// What each view shows traced under it (MU18): the viewer's, never
    /// saved in the project.
    /// What the project's references show, read (Trace & Reference v2).
    pub ref_cache: crate::trace::RefCache,
    /// Calibrate: the reference being scaled and the points picked on
    /// it so far; with two, `calibrated` asks for the distance.
    pub calibrate: Option<(ElementId, Vec<DVec2>)>,
    /// The tool options' Close button was pressed (A5-3).
    pub close_outline: bool,
    /// Align (MU23): the reference, and the points picked so far (two on
    /// it, then two on what it lines up with).
    pub align_ref: Option<(ElementId, Vec<DVec2>)>,
    /// Align's four points, picked, for the app to apply.
    pub aligned: Option<(ElementId, [DVec2; 4])>,
    /// Picture or DXF files dropped on a view, and where (MU23).
    pub dropped: Vec<(ElementId, DVec2, std::path::PathBuf)>,
    pub calibrated: Option<(ElementId, DVec2, DVec2)>,
    /// Temporary dimensions muted (MU21): none shown, none clicked.
    pub temp_dims_off: bool,
    /// Sections show the floor plans' ranges: each plan's cut and view
    /// depth as a band, dragged by their grips (MU34 A1).
    pub plan_ranges: bool,
    /// A plan's range line being dragged in a section: the plan, its cut
    /// (else its view depth), its level's height.
    range_drag: Option<(ElementId, RangeGrip, f64)>,
    /// Each section's plan-range band ends, in its drawing (MU36).
    range_ends: HashMap<ElementId, (f64, f64)>,
    /// A plan range line clicked, its height being typed: the plan, its
    /// cut (else its depth), its level, where.
    range_edit: Option<(ElementId, bool, f64, Pos2)>,
    /// The plan whose range a section reads out last (MU35).
    range_focus: Option<ElementId>,
    pub host_pick: Option<ElementId>,
    pub host_cycle: usize,
    /// A footing was just placed on a host: the status bar offers the
    /// Foundation plan.
    pub footing_placed: bool,
    /// What a Fill room click would fill, under the pointer.
    pub fill_look: Option<crate::fillpreview::Cached>,
}

impl PlanState {
    /// `e`, made an instance of the type its tool draws (if there is one
    /// and it fits).
    pub fn typed(&self, model: &creabim_core::Model, mut e: Element) -> Element {
        let t = creabim_core::TypeCategory::of(&e.kind).and_then(|c| self.types.get(&c));
        if let Some(t) = t {
            creabim_core::types::stamp(model, &mut e, *t);
        }
        e
    }
}

impl Default for PlanState {
    fn default() -> Self {
        Self {
            cameras: HashMap::new(),
            divide_marks: vec![],
            divide_labels: vec![],
            divide_guides: false,
            ghost_now: None,
            ghost_shared: None,
            pane_now: None,
            point_pick: false,
            point_picked: None,
            point_hover: None,
            point_from: None,
            point_lines: false,
            point_line_picked: None,
            mod_guides: Default::default(),
            zoom_to_scale: None,
            edit_note: None,
            sheet_guides: false,
            area: None,
            area_cameras: HashMap::new(),
            linked_areas: Default::default(),
            hint: None,
            tool: Tool::Select,
            pending: None,
            wall_thickness: 0.2,
            wall_location: Default::default(),
            wall_flip: false,
            wall_shape: 0,
            wall_arc_end: None,
            drag: None,
            dim: vec![],
            tag_host: None,
            anno_drag: None,
            drag_owner: None,
            anno: Default::default(),
            modal: None,
            sketch: None,
            grip: None,
            edit: None,
            grips: Default::default(),
            snaps: HashMap::new(),
            status: None,
            guide_edge: None,
            outline: None,
            slab_mode: Default::default(),
            slab_arc: false,
            slab_keep_bottom: false,
            boxsel: None,
            pivot: modal::Pivot::default(),
            move_target: modal::Target::default(),
            cursor: None,
            armed: None,
            armed_grip: None,
            in_place: false,
            commands: vec![],
            menu_at: None,
            web_menus: false,
            menu_request: None,
            snap: snaps::SnapSettings::default(),
            grid_group: None,
            level_drag: None,
            members: Default::default(),
            parts: Default::default(),
            want_levels: false,
            objects: Default::default(),
            want_catalogue: false,
            want_footings: vec![],
            refs_used: HashMap::new(),
            boxed: false,
            spotlight: vec![],
            spotlight_ghost: false,
            chain: (None, vec![]),
            rested: (None, vec![]),
            rest: None,
            refs: HashMap::new(),
            ref_rest: None,
            guide_step: 45.0,
            hover_guide: None,
            dim_axis: None,
            dim_chain: false,
            chain_line: None,
            dim_step_mm: 0.0,
            dim_step_size: 7.0,
            dim_lock: None,
            align: Default::default(),
            hover: None,
            xray: HashSet::new(),
            pick_scenes: HashMap::new(),
            axo_snaps: HashMap::new(),
            centre_lines: HashMap::new(),
            types: HashMap::new(),
            details: Default::default(),
            want_view: None,
            step_prompt: None,
            temp: Default::default(),
            tracker: Default::default(),
            ref_cache: Default::default(),
            calibrate: None,
            close_outline: false,
            align_ref: None,
            aligned: None,
            dropped: vec![],
            calibrated: None,
            temp_dims_off: false,
            plan_ranges: false,
            range_drag: None,
            range_ends: HashMap::new(),
            range_edit: None,
            range_focus: None,
            host_pick: None,
            host_cycle: 0,
            footing_placed: false,
            fill_look: None,
        }
    }
}

impl PlanState {
    /// Rebuild every view's snaps on their next draw (a snap setting
    /// that changes what is snapped to).
    pub fn forget_snaps(&mut self) {
        self.snaps.clear();
    }

    /// Drop any half-finished tool input.
    pub fn cancel(&mut self) {
        self.pending = None;
        self.dim.clear();
        self.chain_line = None;
        self.anno.clear();
        self.tag_host = None;
        self.sketch = None;
        self.guide_edge = None;
        self.outline = None;
        self.members.path.clear();
        self.parts.path.clear();
        self.chain.1.clear();
        self.rested.1.clear();
        self.align.clear();
        self.details.path.clear();
        self.temp = Default::default();
    }

    /// Is a tool part way through, with clicks to undo?
    pub fn has_input(&self) -> bool {
        self.pending.is_some()
            || !self.dim.is_empty()
            || self.anno.has_input()
            || self.tag_host.is_some()
            || self.sketch.is_some()
            || self.guide_edge.is_some()
            || self.outline.is_some()
            || self.armed.is_some()
            || !self.members.path.is_empty()
            || self.align.has_input()
            || !self.details.path.is_empty()
    }

    /// Drop every snap reference, and the guides they make.
    pub fn clear_references(&mut self) -> usize {
        let n = self.refs.values().map(Vec::len).sum();
        self.refs.clear();
        self.ref_rest = None;
        n
    }

    /// Esc or right-click: drop what the tool has so far, or else leave the
    /// tool for Select. False when there was nothing to step back from.
    pub fn step_back(&mut self) -> bool {
        if self.has_input() {
            self.cancel();
            self.armed = None;
            self.status = Some("Cancelled. Right-click or Esc again to leave the tool.".into());
            true
        } else if self.clear_references() > 0 {
            self.status = Some("Guide lines cleared. Esc again leaves the tool.".into());
            true
        } else if self.edit.is_some() && !self.grips.is_empty() {
            self.grips.clear();
            self.status = Some("Nothing picked. Tab finishes editing.".into());
            true
        } else if self.edit.is_some() {
            // Esc never leaves Edit mode, as Blender's: Tab does (MU39)
            self.status = Some("Still editing: Tab finishes.".into());
            true
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
            self.status = Some("Back to Select.".into());
            true
        } else {
            false
        }
    }

    /// Is a tool waiting for more clicks?
    /// Is a grip being dragged in a plan or section?
    pub fn gripping(&self) -> bool {
        self.grip.is_some()
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
            || !self.dim.is_empty()
            || self.anno.has_input()
            || self.tag_host.is_some()
            || self.sketch.is_some()
            || self.modal.is_some()
            || !self.details.path.is_empty()
            || self.temp.editing.is_some()
    }
}

pub struct Xf {
    pub rect: Rect,
    pub center: DVec2,
    pub scale: f32,
}

impl Xf {
    pub fn to_screen(&self, p: DVec2) -> Pos2 {
        let d = (p - self.center).as_vec2() * self.scale;
        self.rect.center() + Vec2::new(d.x, -d.y)
    }

    pub fn to_world(&self, p: Pos2) -> DVec2 {
        let d = (p - self.rect.center()) / self.scale;
        self.center + DVec2::new(d.x as f64, -d.y as f64)
    }
}

/// Show a view's drawing in `rect`. Returns a status message when an edit fails.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    doc: &mut Document,
    view: ElementId,
    kind: PaneKind,
    plan: &Drawing,
    state: &mut PlanState,
    selection: &mut Selection,
    style: PaneStyle,
) -> Option<String> {
    // while a point is asked for, a click gives it and selects nothing
    let picking = state.point_pick && matches!(kind, PaneKind::Plan { .. });
    let sense = if picking {
        Sense::drag()
    } else {
        Sense::click_and_drag()
    };
    let response = ui.interact(rect, ui.id().with(("view", view)), sense);
    let painter = ui.painter_at(rect);
    if let Some(bg) = style.background {
        painter.rect_filled(rect, 0.0, bg);
    }

    let (mut center, mut scale) = match style.camera {
        Some(c) => c,
        None => {
            let shared = *state.cameras.entry(view).or_insert_with(|| fit(plan, rect));
            match state.area {
                // its own, starting where the first area is
                Some(a) => *state.area_cameras.entry((a, view)).or_insert(shared),
                None => shared,
            }
        }
    };
    if style.camera.is_none() && state.zoom_to_scale == Some(view) {
        state.zoom_to_scale = None;
        if let Some(ElementKind::View(v)) = doc.model().get(view).map(|e| &e.kind) {
            // a metre is 1000/scale paper mm; a screen mm is 96/25.4 points
            scale = (1000.0 / v.scale.max(1.0) * 96.0 / 25.4) as f32;
        }
    }
    let tool = match (kind, state.tool) {
        (PaneKind::Axonometric, Tool::Tag) => Tool::Tag,
        (PaneKind::Axonometric, Tool::Note) => Tool::Note,
        (PaneKind::Axonometric, Tool::Align) => Tool::Align,
        (PaneKind::Axonometric | PaneKind::Schedule, _) => Tool::Select,
        (_, t) => t,
    };
    let mm = style.mm();

    // orbit a 3D view by dragging it
    if kind == PaneKind::Axonometric && response.dragged_by(egui::PointerButton::Primary) {
        let d = response.drag_delta();
        let r = doc.edit_merged(view, "Orbit 3D view", |e| {
            if let ElementKind::View(View {
                kind: ViewKind::Axonometric { azimuth, altitude },
                ..
            }) = &mut e.kind
            {
                *azimuth = (*azimuth - d.x as f64 * 0.4).rem_euclid(360.0);
                *altitude = (*altitude + d.y as f64 * 0.4).clamp(-89.0, 89.0);
            }
        });
        if let Err(e) = r {
            return Some(e.to_string());
        }
    }

    // pan and zoom; Ctrl + middle drag zooms about where it started (MU39)
    let ctrl_mid =
        response.dragged_by(egui::PointerButton::Middle) && ui.input(|i| i.modifiers.command);
    if ctrl_mid {
        let k = (1.0 - response.drag_delta().y * 0.01).clamp(0.5, 2.0);
        if let Some(at) = ui.input(|i| i.pointer.press_origin()) {
            let xf = Xf {
                rect,
                center,
                scale,
            };
            let before = xf.to_world(at);
            scale = (scale * k).clamp(2.0, 2000.0);
            let xf = Xf {
                rect,
                center,
                scale,
            };
            center += before - xf.to_world(at);
        }
    } else if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        let d = response.drag_delta() / scale;
        center -= DVec2::new(d.x as f64, -d.y as f64);
    }
    if response.hovered() {
        // a touchpad's two fingers pan; a wheel and a pinch zoom (MU24)
        let (pan, scroll) = wheel(ui);
        center -= DVec2::new(pan.x as f64, -pan.y as f64) / scale as f64;
        if scroll != 0.0 {
            if let Some(hover) = response.hover_pos() {
                let xf = Xf {
                    rect,
                    center,
                    scale,
                };
                let before = xf.to_world(hover);
                scale = (scale * (1.0 + scroll * 0.002)).clamp(2.0, 2000.0);
                let xf = Xf {
                    rect,
                    center,
                    scale,
                };
                center += before - xf.to_world(hover);
            }
        }
    }
    // zoom and pan buttons, for a mouse without a wheel button or a
    // trackpad (MU18): zoom in, zoom out, fit, and a hand dragged to pan
    if style.decorations && style.camera.is_none() {
        // the selection, framed as the drawing is (MU24)
        let picked = creabim_geom::PlanView {
            items: plan
                .items
                .iter()
                .filter(|i| selection.contains(i.id()))
                .cloned()
                .collect(),
        };
        let fit_sel = (!picked.items.is_empty()).then(|| fit(&picked, rect));
        if let Some((c, k)) = nav_column(ui, rect, view, || fit(plan, rect), fit_sel, center, scale)
        {
            center = c;
            scale = k;
        }
    }
    if style.camera.is_none() {
        match state.area {
            Some(a) => {
                state.area_cameras.insert((a, view), (center, scale));
            }
            None => {
                state.cameras.insert(view, (center, scale));
            }
        }
    }
    let xf = Xf {
        rect,
        center,
        scale,
    };

    if style.decorations && kind != PaneKind::Schedule {
        let (grid, axes) = plan_look();
        if grid {
            draw_metric_grid(&painter, &xf);
        }
        // the origin in a plan, faint: where the building is anchored, red
        // along X and green along Y as in 3D (MU40)
        if axes && matches!(kind, PaneKind::Plan { .. }) {
            draw_origin_axes(&painter, &xf);
        }
    }
    // a point asked for: a cross follows the pointer, a click gives it
    if picking {
        if let Some(h) = response.hover_pos() {
            let w = xf.to_world(h);
            let step = crate::snaps::current_step();
            // the drawing tools' snaps (MU39): corners, middles, where
            // guides and lines cross, along a guide; else the grid step
            let cfg = state.snap.live(ui.input(|i| i.modifiers.alt));
            let from = state.point_from.map(|f| f.extend(0.0));
            let snapped = state.snaps.get(&view).and_then(|(_, _, sn)| {
                sn.snap(
                    &cfg,
                    h,
                    &|p: DVec3| Some(xf.to_screen(p.truncate())),
                    from,
                    Some(w.extend(0.0)),
                )
            });
            if let Some(sn) = snapped {
                let top = ui
                    .ctx()
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        ui.id().with("picksnap"),
                    ))
                    .with_clip_rect(rect);
                crate::snaps::draw(
                    &top,
                    sn,
                    xf.to_screen(sn.p.truncate()),
                    from.map(|f| xf.to_screen(f.truncate())),
                );
            }
            let corner = snapped
                .filter(|s| s.kind.is_point())
                .map(|s| s.p.truncate());
            let along = snapped
                .filter(|s| !s.kind.is_point())
                .map(|s| s.p.truncate());
            let w = match corner.or(if state.point_lines { None } else { along }) {
                Some(c) => c,
                None if step > 0.0 => (w / step).round() * step,
                None => w,
            };
            state.point_hover = Some(w);
            // a straight line of the drawing under the pointer: a guide, a
            // wall's face, an edge
            let line = if state.point_lines && corner.is_none() {
                let reach = 6.0 / scale as f64;
                let to_seg = |a: DVec2, b: DVec2| {
                    let ab = b - a;
                    let t = ((w - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                    (a + ab * t).distance(w)
                };
                plan.items
                    .iter()
                    .flat_map(|it| match it {
                        PlanItem::Line { a, b, .. } => vec![(*a, *b)],
                        PlanItem::Polygon { points, .. } => (0..points.len())
                            .map(|i| (points[i], points[(i + 1) % points.len()]))
                            .collect(),
                        _ => vec![],
                    })
                    .filter(|(a, b)| a.distance(*b) > 1e-6 && to_seg(*a, *b) < reach)
                    .min_by(|p, q| to_seg(p.0, p.1).total_cmp(&to_seg(q.0, q.1)))
            } else {
                None
            };
            if let Some((a, b)) = line {
                let top = ui
                    .ctx()
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        ui.id().with("pickline"),
                    ))
                    .with_clip_rect(rect);
                top.line_segment(
                    [xf.to_screen(a), xf.to_screen(b)],
                    Stroke::new(3.0, SPLIT_RED),
                );
            }
            let c = xf.to_screen(w);
            let top = ui
                .ctx()
                .layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    ui.id().with("pick"),
                ))
                .with_clip_rect(rect);
            let s = Stroke::new(1.5, accent_selected());
            top.line_segment([c - egui::vec2(12.0, 0.0), c + egui::vec2(12.0, 0.0)], s);
            top.line_segment([c - egui::vec2(0.0, 12.0), c + egui::vec2(0.0, 12.0)], s);
            top.circle_stroke(c, 5.0, s);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if ui.input(|i| i.pointer.primary_clicked()) {
                match line {
                    Some(l) => state.point_line_picked = Some(l),
                    None => state.point_picked = Some(w),
                }
                if !state.point_lines {
                    state.point_pick = false;
                }
            }
        }
        if ui.input(|i| i.pointer.secondary_clicked() || i.key_pressed(egui::Key::Escape)) {
            state.point_pick = false;
        }
    }
    // the references shown in this view, under the drawing (MU19)
    if style.decorations {
        crate::trace::draw_view(
            &painter,
            doc.model(),
            &mut state.ref_cache,
            view,
            false,
            &|p| xf.to_screen(p),
        );
    }
    // the slabs of a plan's level: a light fill under the rest, stronger
    // where the pointer is, so they can be seen and clicked
    let slabs = match kind {
        PaneKind::Plan { level } => level_slabs(doc.model(), level, plan),
        _ => vec![],
    };
    // pre-selection: what a click here would take, and what else is
    // under the pointer (Tab cycles them)
    let pane = Pane::View(view);
    if !response.dragged() {
        // (the Align tool takes faces in a 3D view, so Tab reaches them)
        let reading = (matches!(tool, Tool::Select | Tool::Tag)
            || (tool == Tool::Align && kind == PaneKind::Axonometric))
            && state.modal.is_none()
            && state.boxsel.is_none()
            // in Edit mode Tab is Layers', in a drag the typed length's
            && state.edit.is_none()
            && state.grip.is_none();
        let pointer = response.hover_pos().filter(|_| reading);
        let tol = 6.0 / xf.scale as f64;
        let rev = doc.model().revision();
        let mut hover = state.hover.take();
        let said = hoverpick::track(ui.ctx(), &mut hover, pane, pointer, rev, doc.model(), |p| {
            under_pointer(state, doc, view, kind, plan, xf.to_world(p), tol, mm)
        });
        state.hover = hover;
        if said.is_some() {
            state.status = said;
        }
    }
    let hovered = hoverpick::highlighted(&state.hover)
        .filter(|id| !selection.contains(*id) && !response.dragged());
    let hovered_slab = hovered.filter(|id| slabs.iter().any(|(s, _)| s == id));
    for (id, points) in &slabs {
        let fill = if selection.contains(*id) {
            SLAB_SELECTED
        } else if hovered_slab == Some(*id) {
            SLAB_HOVER
        } else {
            SLAB_FILL
        };
        fill_polygon(&painter, &xf, points, fill);
    }
    let xray = state.xray.contains(&view);
    // a type's instances shown in the model: drawn as selected, the rest
    // faint when asked
    let ghost = state.spotlight_ghost && !state.spotlight.is_empty();
    let mut faint = painter.clone();
    faint.set_opacity(0.22);
    // the view's graphic overrides, as on its sheets (MU27)
    let overrides = view_overrides(doc.model(), view);
    for item in &plan.items {
        let lit = state.spotlight.contains(&item.id());
        let selected = selection.contains(item.id()) || lit;
        if ghost && !selected {
            draw_item(&faint, &xf, item, false, mm);
        } else if xray {
            draw_item_xray(&painter, &xf, item, selected, mm, doc.model());
        } else if let Some(o) = overrides.get(&item.id()) {
            let mut p = painter.clone();
            if o.fade > 0.0 && !selected {
                p.set_opacity(1.0 - o.fade);
            }
            draw_item_with(&p, &xf, item, selected, mm, o);
        } else {
            draw_item(&painter, &xf, item, selected, mm);
        }
    }
    // the references drawn over the model, then the outline of each one
    // selected or hovered, which is where it is picked (MU19)
    if style.decorations {
        crate::trace::draw_view(
            &painter,
            doc.model(),
            &mut state.ref_cache,
            view,
            true,
            &|p| xf.to_screen(p),
        );
        for (id, r) in creabim_core::reference::in_view(doc.model(), view) {
            let lit = selection.contains(id);
            if !(lit || hovered == Some(id)) {
                continue;
            }
            if let Some(c) = r.outline() {
                let mut pts: Vec<Pos2> = c.iter().map(|p| xf.to_screen(*p)).collect();
                pts.push(pts[0]);
                let ink = if lit {
                    accent_selected()
                } else {
                    accent_selected().gamma_multiply(0.6)
                };
                painter.extend(egui::Shape::dashed_line(
                    &pts,
                    Stroke::new(1.5, ink),
                    6.0,
                    4.0,
                ));
            }
        }
    }
    // a dimension that holds its spacing shows a small closed padlock by
    // its value, on screen only: paper prints it as any dimension. Left
    // alone it is a faint ghost; selected, or with an end selected, it
    // shows strongly
    for item in &plan.items {
        if let PlanItem::Dimension {
            id,
            a,
            b,
            offset,
            witness,
            text,
            look,
            ..
        } = item
        {
            let current = match doc.model().get(*id).map(|e| &e.kind) {
                Some(ElementKind::Dimension(d)) if d.held.is_some() => Some(
                    selection.contains(*id)
                        || [d.a.host, d.b.host]
                            .into_iter()
                            .flatten()
                            .any(|h| selection.contains(h)),
                ),
                _ => None,
            };
            if let Some(current) = current {
                let g = dimension_geometry(*a, *b, *offset, *witness, look, mm);
                let px = crate::annotate::text_px(look, mm, &xf);
                let half = paper::text_width(text, look.size) * mm * xf.scale as f64 / 2.0;
                let along = DVec2::from_angle(g.angle);
                let at = xf.to_screen(g.text_at);
                let dir = Vec2::new(along.x as f32, -along.y as f32);
                let spot = at + dir * (half as f32 + px * 0.9);
                crate::tempdims::held_mark(&painter, spot, px.clamp(9.0, 16.0), current);
            }
        }
    }
    // footings: an ordinary plan doesn't show them, but while the Footing
    // tool is on it shows its level's, and a selected one always shows
    // (A3-1)
    if let PaneKind::Plan { level } = kind {
        draw_footings(
            &painter,
            &xf,
            doc.model(),
            level,
            tool == Tool::Footing,
            selection,
        );
    }
    // 3D crop boxes in plans and sections (A4-5): the selected one, and
    // every one whose frame is shown, dashed in the crop colour
    if style.decorations {
        for (id, lo, hi) in crop_rects(doc.model(), view) {
            let selected = selection.contains(id);
            let shown = matches!(
                doc.model().get(id).map(|e| &e.kind),
                Some(ElementKind::CropBox(c)) if !c.frame_hidden
            );
            if !(selected || shown) {
                continue;
            }
            let c = Color32::from_rgb(217, 115, 64);
            let w = if selected { 1.8 } else { 1.0 };
            let corners = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y), lo];
            let pts: Vec<Pos2> = corners.iter().map(|p| xf.to_screen(*p)).collect();
            for seg in pts.windows(2) {
                painter.extend(egui::Shape::dashed_line(seg, Stroke::new(w, c), 6.0, 4.0));
            }
        }
    }
    // a selected mark is traced in the accent, as a hover traces it
    for item in plan.items.iter().filter(|it| is_mark(it)) {
        if selection.contains(item.id()) && !ghost {
            trace(&painter, &xf, item, accent_selected(), mm);
        }
    }
    if let Some(id) = hovered {
        draw_hover(&painter, &xf, &plan.items, id, mm);
    }
    if let Some(id) = hoverpick::badge(ui.ctx(), &mut state.hover, pane, doc.model()) {
        if ui.input(|i| i.modifiers.shift) {
            selection.toggle(id);
        } else {
            selection.set(Some(id));
        }
        state.status = Some(format!(
            "{} selected.",
            hoverpick::describe(doc.model(), id)
        ));
    }
    if let Some((_, points)) = slabs.iter().find(|(id, _)| Some(*id) == hovered_slab) {
        // its edge over the walls that sit on it
        let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
        painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0, SLAB_EDGE)));
    }
    crate::annotate::draw_grips(&painter, &xf, doc.model(), &plan.items, selection, mm);
    crate::sketch::draw_marks_in_pane(&painter, doc.model(), selection.active, kind, &xf);
    // the centre lines the Align tool can take here
    let centre_lines = match (tool, kind) {
        (Tool::Align, PaneKind::Plan { .. } | PaneKind::Section { .. }) => {
            pane_centre_lines(state, doc, view, kind, plan)
        }
        _ => Arc::new(vec![]),
    };
    if tool == Tool::Align && kind != PaneKind::Axonometric {
        let model = doc.model();
        let hover = response.hover_pos().and_then(|p| {
            crate::aligntool::hover_edge(
                model,
                &state.align,
                &plan.items,
                &centre_lines,
                xf.to_world(p),
                6.0 / xf.scale as f64,
            )
        });
        crate::aligntool::draw_picks(&painter, &xf, &state.align, view, hover, &centre_lines);
    }

    let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
    // holding Alt picks free of snaps
    let snap_cfg = state.snap.live(alt);
    // the Align tool in a 3D view works on the model's faces
    let align_under = match (kind, tool) {
        (PaneKind::Axonometric, Tool::Align) => {
            let fs = axo_frame_snaps(state, doc, view);
            let under = match (&fs, response.hover_pos()) {
                (Some(fs), Some(p)) => axo_align_under(state, doc, view, fs, &xf, p, &snap_cfg),
                _ => Default::default(),
            };
            if let Some(fs) = &fs {
                let project = |m: DVec3| Some(xf.to_screen(fs.0.project(m).truncate()));
                crate::align3d::draw(&painter, &project, doc.model(), &state.align, &under);
            }
            Some(under)
        }
        _ => None,
    };
    // snaps: the selection and what joins it are left out while selecting,
    // since dragging them must not snap to themselves
    let moving = state
        .modal
        .as_ref()
        .filter(|m| m.pane == Some(view))
        .and_then(|m| m.snap_exclude(doc));
    let exclude = match (moving, tool) {
        (Some(ids), _) => ids,
        (None, Tool::Select) => selection
            .active
            .map(|id| joined_with(doc, id))
            .unwrap_or_default(),
        _ => vec![],
    };
    if matches!(kind, PaneKind::Plan { .. } | PaneKind::Section { .. }) {
        refresh_snaps(state, doc.model(), view, plan, &exclude);
    }
    let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
    // a line starts from the tool's first click, or the sketch's last corner
    let sketch_last = state
        .sketch
        .as_ref()
        .filter(|d| d.pane == Some(view))
        .and_then(|d| Some(d.plane.to_model(*d.points.last()?)));
    let from_pane = |m: DVec3| match kind {
        PaneKind::Section { start, end } => model_to_pane(Space::Section { start, end }, m),
        _ => m.truncate(),
    };
    let last = state
        .outline
        .as_ref()
        .and_then(|o| o.points.last().copied());
    let drawing_from = state
        .pending
        .or(sketch_last.map(from_pane))
        .or(last)
        .map(|a| a.extend(0.0));
    // temporary guide lines: from the points and edges rested on in the
    // drawing tools, and through where the line being drawn starts
    let guided = matches!(kind, PaneKind::Plan { .. } | PaneKind::Section { .. })
        && tool != Tool::Select
        && snap_cfg.allows(SnapKind::OnTempGuide);
    let guide_from = drawing_from.or(state.dim.first().map(|d| d.1.extend(0.0)));
    let guides = if guided {
        let refs = state.refs.get(&view).map_or(&[][..], |r| r.as_slice());
        let _p = crate::app::Perf::new("plan snaps: guides");
        snaps::temp_guides(refs, guide_from, state.guide_step)
    } else {
        vec![]
    };
    // the points of the shape being drawn here, for the next to line up with
    let track = {
        let shape = shape_points(state, view, kind, &from_pane);
        // (with guide lines on, points rested on make guides instead)
        let rested: Vec<DVec3> = match (&state.rested, drawing_from) {
            ((Some(v), pts), Some(_)) if *v == view && !guided => {
                pts.iter().map(|p| p.extend(0.0)).collect()
            }
            _ => vec![],
        };
        let shape: Vec<DVec3> = shape.iter().map(|p| p.extend(0.0)).collect();
        snaps::track_points(&shape, &rested)
    };
    // a host tool's host under the pointer wins over snaps and guides
    // before the first click; Tab goes through overlapping hosts (A3-2)
    let host_hits: Vec<ElementId> = match (kind, response.hover_pos()) {
        (PaneKind::Plan { level }, Some(p)) if crate::memberstool::hosted(tool) => {
            let _p = crate::app::Perf::new("plan snaps: hosts");
            crate::memberstool::hosts_at(doc.model(), level, tool, state, xf.to_world(p))
        }
        _ => vec![],
    };
    let host = (!host_hits.is_empty()).then(|| host_hits[state.host_cycle % host_hits.len()]);
    if response.hovered() {
        state.host_pick = host;
        if host_hits.len() < 2 {
            state.host_cycle = 0;
        }
    }
    let guides = if host.is_some() { vec![] } else { guides };
    let tracked = response
        .hover_pos()
        .filter(|_| host.is_none())
        .and_then(|p| {
            let _p = crate::app::Perf::new("plan snaps: guided");
            let snapper = &state.snaps.get(&view)?.2;
            snapper.snap_guided(
                &snap_cfg,
                p,
                &pane_project,
                drawing_from,
                &track,
                &guides,
                Some(xf.to_world(p).extend(0.0)),
            )
        });
    let tool_snap = tracked.as_ref().map(|t| t.snap);
    let tool_align = tracked.as_ref().and_then(|t| t.align);
    let now = ui.input(|i| i.time);
    if guided {
        // rest on a point or edge (or press Q over it) to pick it as a
        // reference, and again to drop it
        let under = response.hover_pos().and_then(|p| {
            let snapper = &state.snaps.get(&view)?.2;
            let s = snapper.snap(&snap_cfg, p, &pane_project, None, None)?;
            snapper.reference_at(&s)
        });
        let q = response.hovered()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Q) && !i.modifiers.command);
        click_is_not_a_rest(state, tool, response.clicked(), under, now);
        let under = under.filter(|_| host.is_none());
        if let Some(msg) = rest_reference(state, view, under, now, q) {
            state.status = Some(msg);
        }
        // the app draws no frames while nothing moves (A6), so a pointer
        // resting still would never be seen to rest: ask for one frame
        // just after the dwell (MU21, guide lines stopped appearing)
        if matches!(state.ref_rest, Some((_, _, false))) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(REST_DWELL + 0.05));
        }
        // guide lines last while they are used: a snap following one, or
        // a shape being drawn from them; otherwise they go after a while
        let following = tracked.as_ref().is_some_and(|t| !t.guides.is_empty());
        fade_references(state, view, now, following || drawing_from.is_some());
        if state.refs.get(&view).is_some_and(|r| !r.is_empty()) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(REF_FADE / 4.0));
        }
    } else {
        // a point snapped to and rested on while drawing is tracked too
        rest_on(state, view, tool_snap, drawing_from.is_some(), now);
        // a frame after the dwell, as nothing repaints while the pointer
        // rests (A6, MU21)
        if state.rest.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(450));
        }
    }
    if guided {
        // the references, and their guides faintly; those a snap follows
        // are drawn again with it
        if let Some(refs) = state.refs.get(&view).filter(|_| host.is_none()) {
            snaps::draw_references(&painter, &pane_project, refs);
        }
        snaps::draw_guides(&painter, &pane_project, &guides, &[]);
        if response.hovered() {
            let on = tracked.as_ref().and_then(|t| t.guides.first().copied());
            state.hover_guide = on.and_then(|i| guides.get(i)).map(|g| (view, *g));
        }
        // K keeps the guide line under the pointer
        if response.hovered()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::K) && !i.modifiers.command)
        {
            let g = state.hover_guide.filter(|h| h.0 == view).map(|h| h.1);
            state.status = Some(match keep_guide(doc, kind, g) {
                Ok(msg) => msg,
                Err(e) => format!("Couldn't do that: {e}"),
            });
        }
    }
    let pointer_world = response.hover_pos().map(|p| match kind {
        PaneKind::Plan { .. } | PaneKind::Section { .. } if !shift => {
            tool_snap.map_or(xf.to_world(p), |s| s.p.truncate())
        }
        PaneKind::Plan { level } => snap(
            doc,
            level,
            &xf,
            xf.to_world(p),
            state.pending,
            shift,
            &[],
            &snap_cfg,
        ),
        PaneKind::Axonometric | PaneKind::Schedule => xf.to_world(p),
        PaneKind::Section { .. } => {
            let mut p = xf.to_world(p);
            if let (true, Some(a)) = (shift, state.pending) {
                let d = p - a;
                p = if d.x.abs() >= d.y.abs() {
                    DVec2::new(p.x, a.y)
                } else {
                    DVec2::new(a.x, p.y)
                };
            }
            if snap_cfg.allows(SnapKind::Grid) && snap_cfg.step_len() > 0.0 {
                let s = snap_cfg.step_len();
                (p / s).round() * s
            } else {
                p
            }
        }
    });

    // the tracker: a typed length and angle take over from the pointer
    // while a line is being drawn, and Enter places it (2.10)
    let line_tool = matches!(
        tool,
        Tool::Wall
            | Tool::Section
            | Tool::Elevation
            | Tool::Grid
            | Tool::Beam
            | Tool::Railing
            | Tool::Footing
            | Tool::RoomSeparator
    ) && matches!(kind, PaneKind::Plan { .. });
    let mut tracker_enter = false;
    let hover_world = match (state.pending, pointer_world, line_tool) {
        (Some(a), Some(b), true) if response.hovered() => {
            if state.tracker.keys(ui.ctx()) == crate::tracker::Keys::Enter {
                tracker_enter = true;
            }
            Some(state.tracker.end(a, b))
        }
        _ => {
            if state.pending.is_none() && !state.tracker.fields.iter().all(String::is_empty) {
                state.tracker.clear();
            }
            pointer_world
        }
    };

    // the Slab tool's modes, and Line / Arc while clicking a slab outline
    if let PaneKind::Plan { level } = kind {
        let slab_outline = state
            .outline
            .as_ref()
            .is_some_and(|o| o.level == level && !o.is_wall());
        if tool.draws_outline() || slab_outline {
            let modes = tool.draws_outline();
            if let Some(msg) =
                crate::slabtool::controls(ui, rect, Some(view), modes, doc, state, selection)
            {
                state.status = Some(msg);
            }
        }
    }

    // drawing a slab, or redrawing a slab or wall, on this plan's level
    if let (Some(o), PaneKind::Plan { level }) = (&state.outline, kind) {
        if o.level == level {
            let o = o.clone();
            let project = |p: DVec3| Some(xf.to_screen(p.truncate()));
            if let (Some(t), false) = (&tracked, shift) {
                let from = o.points.last().map(|a| xf.to_screen(*a));
                snaps::draw_guides(&painter, &project, &guides, &t.guides);
                snaps::draw_guided(&painter, &project, t, &guides, from);
            }
            o.draw(&painter, &project, hover_world);
            // keys go to the pane under the pointer, as another pane may
            // show the same outline
            let here = response.hovered();
            let typing = ui.ctx().egui_wants_keyboard_input();
            let (enter, escape, backspace) = ui.input(|i| {
                (
                    here && (i.key_pressed(egui::Key::Enter)
                        || !typing && !i.modifiers.any() && i.key_pressed(egui::Key::C)),
                    here && i.key_pressed(egui::Key::Escape),
                    here && !typing && i.key_pressed(egui::Key::Backspace),
                )
            });
            let click = match (response.clicked(), hover_world, response.hover_pos()) {
                (true, Some(p), Some(h)) => Some((p, o.near_first(h, &project))),
                _ => None,
            };
            let right = response.secondary_clicked() && !shift;
            let double = response.double_clicked();
            let enter = enter || std::mem::take(&mut state.close_outline);
            let arc = state.slab_arc;
            let next = state
                .outline
                .as_mut()?
                .input_more(enter, escape, right, click, arc, double, backspace);
            match next {
                Next::Drawing(hint) => state.status = Some(hint),
                Next::Cancel(msg) => {
                    state.outline = None;
                    state.status = Some(msg.into());
                }
                Next::Finish(closed) => {
                    let o = state.outline.take()?;
                    return match o.finish(doc, closed) {
                        Ok((msg, made)) => {
                            if let Some(id) = made {
                                selection.only(id);
                            }
                            state.status = Some(msg);
                            None
                        }
                        Err(e) => Some(e.to_string()),
                    };
                }
            }
            return None;
        }
    }

    // pointer edits: keyboard move, rotate and scale; sketches; grips
    let space = match kind {
        PaneKind::Plan { .. } => Some(Space::Plan),
        PaneKind::Section { start, end } => Some(Space::Section { start, end }),
        PaneKind::Axonometric | PaneKind::Schedule => None,
    };
    // temporary dimensions of the one element selected: click a value
    // and type, or click its padlock (2.10 part 3)
    // a level's in a section or elevation (A5-2)
    let level_picked = selection.active.is_some_and(|id| {
        doc.model()
            .get(id)
            .is_some_and(|e| matches!(e.kind, ElementKind::Level(_)))
            || crate::tempdims::dim_levels(doc.model(), id).is_some()
    });
    if response.clicked() {
        state.temp.click = response.interact_pointer_pos().map(|p| xf.to_world(p));
    }
    let temp_level = match kind {
        PaneKind::Plan { level } if !level_picked => Some(level),
        PaneKind::Section { .. } if level_picked => Some(view),
        _ => None,
    };
    if let (Some(level), Tool::Select, false) = (temp_level, tool, state.temp_dims_off) {
        let quiet = state.modal.is_some()
            || state.grip.is_some()
            || state.edit.is_some()
            || state.anno_drag.is_some()
            || state.boxsel.is_some()
            || response.dragged();
        if let Some(r) = crate::tempdims::interact(
            ui,
            doc,
            view,
            level,
            &xf,
            &painter,
            &response,
            &mut state.temp,
            selection,
            &mut state.status,
            &mut state.hint,
            quiet,
        ) {
            return r;
        }
    }
    if let Some(space) = space {
        let to_model = |pos: Pos2, _axis: Option<creabim_core::Axis>, pivot: DVec3| {
            Some(pane_to_model(space, xf.to_world(pos), pivot.z))
        };
        let project = |p: DVec3| Some(xf.to_screen(model_to_pane(space, p)));
        // the element another view's pointer would place, here too
        if let Some((from, g)) = &state.ghost_shared {
            if *from != state.pane_now {
                crate::ghost::draw(&painter, &project, g, accent_selected());
            }
        }
        // what the modifier being set shows: its centre, plane or cuts
        crate::modguides::draw(
            &painter,
            &state.mod_guides,
            state.mod_guides.ink.unwrap_or(accent_selected()),
            matches!(space, Space::Plan),
            &project,
        );
        // Divide's marks: ticks across in plans, lines standing in sections
        for (a, b, plan) in &state.divide_marks {
            if *plan != matches!(space, Space::Plan) {
                continue;
            }
            let (pa, pb) = (
                xf.to_screen(model_to_pane(space, *a)),
                xf.to_screen(model_to_pane(space, *b)),
            );
            if pa.distance(pb) > 1.0 {
                if state.divide_guides {
                    crate::snaps::dashed_clipped(
                        &painter,
                        &[pa, pb],
                        Stroke::new(1.5, GUIDE_COLOR),
                        8.0,
                        5.0,
                    );
                } else {
                    painter.line_segment([pa, pb], Stroke::new(2.0, accent_selected()));
                }
            }
        }
        let mut taken = vec![];
        for (at, text) in &state.divide_labels {
            let plan_label =
                (at.z - state.divide_marks.first().map_or(at.z, |m| m.0.z)).abs() < 1e-6;
            if matches!(space, Space::Plan) && !plan_label {
                continue;
            }
            divide_label(
                &painter,
                xf.to_screen(model_to_pane(space, *at)),
                text,
                accent_selected(),
                &mut taken,
            );
        }
        // the pane's snaps, lent out while a move or rotate runs here
        let lent = std::mem::take(&mut state.snaps);
        let plane_z = match kind {
            PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
            _ => 0.0,
        };
        let snap_fn = |pos: Pos2, from: Option<DVec3>| {
            let snapper = &lent.get(&view)?.2;
            let to_pane = |p: DVec3| model_to_pane(space, p).extend(0.0);
            let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
            let free = Some(xf.to_world(pos).extend(0.0));
            let s = snapper.snap(&snap_cfg, pos, &pane_project, from.map(to_pane), free)?;
            let p = pane_to_model(space, s.p.truncate(), from.map_or(plane_z, |f| f.z));
            Some((p, snaps::Snap { p, ..s }))
        };
        // Move on faces: the edge under the pointer stands for the face
        // through it, square to the paper
        let pushable: std::collections::HashSet<ElementId> = if state.modal.is_some() {
            let model = doc.model();
            plan.items
                .iter()
                .map(|i| i.id())
                .filter(|id| {
                    model
                        .get(*id)
                        .is_some_and(|e| creabim_core::pushpull::can_push(&e.kind))
                })
                .collect()
        } else {
            Default::default()
        };
        // the selection's own faces win over those lying on them
        let chosen = selection.all();
        let pick_face = |pos: Pos2| {
            let takes = |id: ElementId| {
                pushable
                    .contains(&id)
                    .then_some(u8::from(chosen.contains(&id)))
            };
            let px = 1.0 / xf.scale as f64;
            crate::paneface::face_at(
                &plan.items,
                space,
                xf.to_world(pos),
                6.0 * px,
                plane_z,
                5.0 * px,
                &takes,
            )
            .map(|f| f.pick)
        };
        let map = PaneMap {
            to_model: &to_model,
            project: &project,
            look: match space {
                Space::Section { start, end } => {
                    (end - start).normalize_or_zero().perp().extend(0.0)
                }
                _ => -DVec3::Z,
            },
            snap: &snap_fn,
            pick_face: Some(&pick_face),
        };
        let hover_now = state.hover.clone();
        let pick = |p: Pos2| {
            hover_now
                .as_ref()
                .and_then(|h| h.at(pane, p))
                .or_else(|| hit_test(&plan.items, xf.to_world(p), 6.0 / xf.scale as f64, mm))
        };
        let edits = keyboard_edits(
            ui, doc, view, space, &painter, &response, state, selection, &map, &pick,
        );
        state.snaps = lent;
        if let Some(r) = edits {
            return r;
        }
        if tool == Tool::Sketch {
            let level_z = match kind {
                PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
                _ => 0.0,
            };
            let hover = hover_world.map(|w| pane_to_model(space, w, level_z));
            // a point snapped to something, or with increments off, is kept as is
            let exact = !snap_cfg.allows(SnapKind::Grid)
                || (!shift && tool_snap.is_some_and(|s| s.kind != SnapKind::Grid));
            if let Some(r) = sketch_tool(
                ui, doc, view, space, &xf, &painter, &response, state, selection, hover, shift,
                exact,
            ) {
                return r;
            }
        }
        if tool == Tool::Select {
            let level_z = match kind {
                PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
                _ => 0.0,
            };
            let look = match space {
                Space::Section { start, end } => {
                    (end - start).normalize_or_zero().perp().extend(0.0)
                }
                _ => -DVec3::Z,
            };
            let snapper = &state.snaps[&view].2;
            let ray = |pos: Pos2| {
                let p = pane_to_model(space, xf.to_world(pos), level_z);
                Some(handles::Ray {
                    o: p - look * 1000.0,
                    d: look,
                })
            };
            let snap = |pos: Pos2, from: Option<DVec3>| {
                let to_pane = |p: DVec3| model_to_pane(space, p).extend(0.0);
                let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
                let free = Some(xf.to_world(pos).extend(0.0));
                let s = snapper.snap(&snap_cfg, pos, &pane_project, from.map(to_pane), free)?;
                let z = from.map_or(level_z, |f| f.z);
                let p = pane_to_model(space, s.p.truncate(), z);
                Some((p, snaps::Snap { p, ..s }))
            };
            // what is under others (picked with Tab, or in X-ray) can be
            // dragged by its body too
            let hover_now = state.hover.clone();
            let grab_body = |pos: Pos2, id: ElementId| {
                let w = xf.to_world(pos);
                let on = hit_all(&plan.items, w, 6.0 / xf.scale as f64, mm).contains(&id)
                    || hover_now.as_ref().and_then(|h| h.at(pane, pos)) == Some(id);
                on.then(|| pane_to_model(space, w, level_z))
            };
            let grab_snap = |pos: Pos2, id: ElementId| {
                let only: Vec<PlanItem> = plan
                    .items
                    .iter()
                    .filter(|i| i.id() == id)
                    .cloned()
                    .collect();
                let own = Snapper::from_drawing(&only, &[]);
                let pane_project = |p: DVec3| Some(xf.to_screen(p.truncate()));
                let s = own.snap(&snap_cfg.points_only(), pos, &pane_project, None, None)?;
                Some(pane_to_model(space, s.p.truncate(), level_z))
            };
            let input = PaneInput {
                ray: &ray,
                project: &project,
                look,
                snap: &snap,
                grab_body: &grab_body,
                grab_snap: &grab_snap,
                body_normal: if matches!(space, Space::Section { .. }) {
                    look
                } else {
                    DVec3::Z
                },
                view: Some(view),
                face: &|_| None,
            };
            let mut status = None;
            let edit = state.edit.is_some() && state.edit == selection.active;
            let busy = handles::interact(
                doc,
                &painter,
                &response,
                selection.active,
                None,
                edit,
                &mut state.grip,
                &mut state.grips,
                &input,
                &mut status,
            );
            if status.is_some() {
                state.status = status;
            }
            if busy {
                return None;
            }
        }
    }

    // a level line or head in a section drags the level up and down; after
    // the selection's grips, so a wall's top grip lying on a level line
    // moves the wall's top, not the level
    // the floor plans' ranges, drawn and dragged by their grips (MU34 A1)
    if let (PaneKind::Section { .. }, true) = (kind, state.plan_ranges) {
        if let Some(r) = plan_ranges(
            doc,
            &painter,
            &xf,
            &response,
            state,
            selection,
            tool == Tool::Select,
            view,
            plan,
        ) {
            return r;
        }
    }
    if let (PaneKind::Section { .. }, Tool::Select) = (kind, tool) {
        let snapped = tool_snap.filter(|s| !shift && s.kind != SnapKind::Grid);
        if let Some(r) = drag_level(doc, plan, &xf, &response, state, selection, mm, snapped) {
            return r;
        }
    }
    // a selected section line in a plan can be dragged by its ends or middle
    if let (PaneKind::Plan { .. }, Tool::Select) = (kind, tool) {
        if let Some(r) = drag_section_line(doc, &painter, &xf, &response, state, selection.active) {
            return r;
        }
    }
    // a tag, dimension or note can be dragged into place, once selected:
    // a drag that starts on one that isn't draws a box instead (MU20)
    let press_on_selected = response.drag_started_by(egui::PointerButton::Primary)
        && ui.input(|i| i.pointer.press_origin()).is_some_and(|p0| {
            hit_all(&plan.items, xf.to_world(p0), 6.0 / scale as f64, mm)
                .into_iter()
                .any(|id| selection.contains(id))
        });
    let annotation_drag = state.anno.grab.is_some()
        || !response.drag_started_by(egui::PointerButton::Primary)
        || press_on_selected;
    if tool == Tool::Select && annotation_drag {
        if let Some(r) = drag_annotation(doc, view, plan, &xf, &response, state, selection, mm) {
            return r;
        }
        let view_def = match doc.model().get(view).map(|e| &e.kind) {
            Some(ElementKind::View(v)) => Some(v.clone()),
            _ => None,
        };
        let leader_drag = matches!(state.anno.grab, Some(crate::annotate::Grab::Leader(_)));
        let snapped = match (&view_def, response.interact_pointer_pos(), leader_drag) {
            (Some(v), Some(p), true) => {
                let snapper = state.snaps.get(&view).map(|s| &s.2);
                let (hp, at, _) = dim_snap(doc, v, kind, snapper, &snap_cfg, &xf, p);
                Some((hp, at))
            }
            _ => None,
        };
        if let Some(r) = crate::annotate::drag(
            doc,
            Some(view),
            view_def.as_ref(),
            &plan.items,
            &xf,
            &response,
            &mut state.anno,
            selection,
            mm,
            snapped,
        ) {
            return r;
        }
    }

    // drag on empty paper to select with a box; with the Footing tool, a
    // box anywhere on a plan puts footings under the walls (Strip) or the
    // columns (Pad) it takes, as Revit's multiple selection does
    let footing_box =
        tool == Tool::Footing && matches!(kind, PaneKind::Plan { .. }) && state.pending.is_none();
    if (tool == Tool::Select && matches!(kind, PaneKind::Plan { .. } | PaneKind::Section { .. }))
        || footing_box
    {
        if response.drag_started_by(egui::PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin());
            if let Some(p0) = origin {
                // as in Revit: a drag starts a box unless it starts on
                // something selected (that drag moves it); grid lines,
                // dimensions and the rest under the press don't stop it
                // (MU19)
                let on_selected = hit_all(&plan.items, xf.to_world(p0), 6.0 / scale as f64, mm)
                    .into_iter()
                    .any(|id| selection.contains(id));
                if !on_selected || footing_box {
                    state.boxsel = Some((p0, response.id));
                    boxsel::trace(p0, true);
                }
            }
        }
        if let Some((p0, _)) = state.boxsel.filter(|b| b.1 == response.id) {
            let now = ui.input(|i| i.pointer.latest_pos()).unwrap_or(p0);
            boxsel::trace(now, false);
            boxsel::draw(&painter, p0, now);
            let ended = response.drag_stopped() || !response.dragged();
            {
                let parts = plan.items.iter().filter_map(|it| {
                    if creabim_geom::plan::is_underlay(it) {
                        return None;
                    }
                    let (lo, hi) = it.bounds()?;
                    Some((
                        it.id(),
                        Rect::from_two_pos(xf.to_screen(lo), xf.to_screen(hi)),
                    ))
                });
                let ids = boxsel::pick(p0, now, parts)
                    .into_iter()
                    .filter(|id| box_selectable(doc, *id));
                if !ended {
                    // what it would take, counted as it is drawn (MU39)
                    if !footing_box {
                        boxsel::draw_count(&painter, now, ids.count());
                    }
                    return None;
                }
                if footing_box {
                    let pad = crate::memberstool::footing_kind(doc.model(), state)
                        == creabim_core::FootingKind::Pad;
                    let hosts: Vec<ElementId> = ids
                        .filter(|id| match doc.model().get(*id).map(|e| &e.kind) {
                            Some(ElementKind::Wall(w)) => !pad && w.curtain.is_none(),
                            Some(ElementKind::Column(_)) => pad,
                            _ => false,
                        })
                        .collect();
                    if hosts.is_empty() {
                        state.status = Some(if pad {
                            "The box took no columns: drag it over the columns to put pads under."
                        } else {
                            "The box took no walls: drag it over the walls to put strips under."
                        }
                        .into());
                    }
                    state.want_footings = hosts;
                } else {
                    let ctrl = ui.input(|i| i.modifiers.command);
                    boxsel::apply(selection, ids.collect(), boxsel::combine_with(shift, ctrl));
                    state.boxed = true;
                }
                state.boxsel = None;
            }
            return None;
        }
    }

    // dimension and tag tools in progress
    let view_def = match doc.model().get(view).map(|e| &e.kind) {
        Some(ElementKind::View(v)) => Some(v.clone()),
        _ => None,
    };
    let dim_snapper = state.snaps.get(&view).map(|s| &s.2);
    let dim_guided = match (tool, &view_def, response.hover_pos()) {
        (t, Some(v), Some(p)) if t == Tool::Dimension || crate::annotate::is_tool(t) => {
            Some(dim_snap_guided(
                doc,
                v,
                kind,
                dim_snapper,
                &snap_cfg,
                &xf,
                p,
                &guides,
                state.dim.last().map(|d| d.1.extend(0.0)),
                &snaps::track_points(
                    &state
                        .dim
                        .iter()
                        .map(|d| d.1.extend(0.0))
                        .collect::<Vec<_>>(),
                    &[],
                ),
            ))
        }
        _ => None,
    };
    let dim_hit = dim_guided.as_ref().map(|(h, at, k, _)| (*h, *at, *k));
    let accent = crate::theme::interaction();
    if let Some((_, at, k, used)) = &dim_guided {
        let c = xf.to_screen(*at);
        match k {
            Some(k) if *k != SnapKind::Grid => {
                let from = state.dim.first().map(|d| xf.to_screen(d.1));
                let hit = snaps::Guided {
                    snap: snaps::Snap {
                        p: at.extend(0.0),
                        kind: *k,
                        host: None,
                    },
                    align: None,
                    guides: used.clone(),
                };
                snaps::draw_guides(&painter, &pane_project, &guides, used);
                snaps::draw_guided(&painter, &pane_project, &hit, &guides, from);
                if response.hovered() && !used.is_empty() {
                    state.hover_guide = guides.get(used[0]).map(|g| (view, *g));
                }
            }
            _ => {
                painter.circle_stroke(c, 5.0, Stroke::new(1.5, accent));
            }
        }
    }
    let dim_hover = dim_hit.map(|(h, at, _)| (h, at));
    // the way a linear dimension measures: as set, held with Shift, or
    // picked from where its line is going
    let dim_axis = match (state.dim.as_slice(), response.hover_pos()) {
        ([(_, a), (_, b)], Some(h)) => {
            let picked = state
                .dim_axis
                .unwrap_or_else(|| creabim_core::DimAxis::pick(*a, *b, xf.to_world(h)));
            if shift {
                *state.dim_lock.get_or_insert(picked)
            } else {
                state.dim_lock = None;
                picked
            }
        }
        _ => {
            state.dim_lock = None;
            state.dim_axis.unwrap_or_default()
        }
    };
    let dim_look = annotate::look_of(doc.model(), state.anno.dim_style, None);
    if crate::annotate::is_tool(tool) {
        let raw = response.hover_pos().map(|p| xf.to_world(p));
        crate::annotate::preview(
            &painter,
            &xf,
            tool,
            &state.anno,
            dim_hover.map(|h| h.1),
            raw,
            &plan.items,
            6.0 / scale as f64,
            mm,
        );
    }
    match (state.dim.as_slice(), dim_hover, response.hover_pos()) {
        ([(_, a)], Some((_, b)), _) if a.distance(b) > 1e-6 => {
            preview_dimension(&painter, &xf, *a, b, dim_axis, 0.0, &dim_look, mm, accent);
        }
        ([(_, a), (_, b)], _, Some(h)) => {
            let off = dim_offset(dim_axis, *a, *b, xf.to_world(h));
            preview_dimension(&painter, &xf, *a, *b, dim_axis, off, &dim_look, mm, accent);
            let what = if state.dim_lock.is_some() {
                format!("{} (held by Shift)", dim_axis.label())
            } else if state.dim_axis.is_some() {
                dim_axis.label().to_string()
            } else {
                format!("{} (Shift holds it)", dim_axis.label())
            };
            painter.text(
                h + Vec2::new(14.0, 16.0),
                egui::Align2::LEFT_TOP,
                what,
                egui::FontId::proportional(12.0),
                accent,
            );
        }
        _ => {}
    }
    if let (Tool::Tag, Some(host), Some(h)) = (tool, state.tag_host, response.hover_pos()) {
        if let Some(v) = &view_def {
            if let Some(proj) = annotate::Projector::of(doc.model(), v) {
                if let Some(c) = annotate::centre(doc.model(), host) {
                    let (text, round) = annotate::tag_text(doc.model(), host);
                    let item = PlanItem::Tag {
                        id: host,
                        anchor: proj.project(c),
                        at: xf.to_world(h),
                        elbow: None,
                        text,
                        round,
                        look: annotate::look_of(doc.model(), None, None),
                    };
                    draw_item(&painter, &xf, &item, true, mm);
                }
            }
        }
    }

    // the element a click would place, as a ghost (A7-4)
    if let (PaneKind::Plan { level }, Some(h), true) = (kind, hover_world, response.hovered()) {
        if crate::ghost::shows(tool) {
            // the interaction colour, as the 3D view's ghost (MU39)
            let accent = accent_selected();
            match crate::ghost::cached(doc.model(), level, tool, h, state) {
                Some(g) => {
                    crate::ghost::draw(&painter, &|p| Some(xf.to_screen(p.truncate())), &g, accent);
                    state.ghost_now = Some((state.pane_now, g));
                }
                None if matches!(tool, Tool::Door | Tool::Window | Tool::Opening) => {
                    if let Some(at) = response.hover_pos() {
                        let look =
                            crate::handles::PillLook::of(&crate::theme::Palette::of(ui.ctx()));
                        crate::handles::plain_pill(
                            &painter,
                            at + Vec2::new(16.0, 18.0),
                            "Move onto a wall",
                            &look,
                        );
                    }
                }
                None => {}
            }
        }
    }
    // the object about to be placed, under the pointer
    if let (Tool::Object, PaneKind::Plan { level }, Some(h)) = (tool, kind, hover_world) {
        if ui.input(|i| !i.modifiers.command && i.key_pressed(egui::Key::Space))
            && !ui.ctx().egui_wants_keyboard_input()
        {
            state.objects.rotation = (state.objects.rotation + 90.0).rem_euclid(360.0);
        }
        crate::objecttool::preview(
            &painter,
            &|p| Some(xf.to_screen(p.truncate())),
            doc.model(),
            level,
            h,
            state,
            false,
        );
    }
    // a callout's box or a detail component being drawn, and Enter to
    // finish the component
    if matches!(tool, Tool::Callout | Tool::Detail) {
        crate::detailtool::preview(
            doc.model(),
            &painter,
            &xf,
            tool,
            state,
            view,
            hover_world,
            mm,
        );
        let enter = response.hovered()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !ui.ctx().egui_wants_keyboard_input();
        // a double-click finishes too (its first click put the last point)
        let space = response.hovered()
            && ui.input(|i| !i.modifiers.command && i.key_pressed(egui::Key::Space))
            && !ui.ctx().egui_wants_keyboard_input();
        if tool == Tool::Detail
            && space
            && state.details.kind == creabim_core::DetailKind::Membrane
            && state.details.view == Some(view)
            && !state.details.path.is_empty()
        {
            state.details.swap = !state.details.swap;
            state.status = Some("The membrane goes on the other side.".into());
        }
        let double = response.double_clicked() && crate::detailtool::finishable(state);
        if tool == Tool::Detail && (enter || double) && state.details.view == Some(view) {
            if let Err(e) = crate::detailtool::finish(doc, state, selection) {
                return Some(e.to_string());
            }
            // the double-click's second click is not a new path's first
            if double {
                return None;
            }
        }
    }
    // a sweep traced: its band (the profile's width either side of the
    // path) as a ghost, and its section at the pointer, so a wrong profile
    // shows at once (MU37)
    if let (Tool::Sweep, PaneKind::Plan { .. }, Some(b)) = (tool, kind, hover_world) {
        let (prof, name) = state.parts.sweep_profile(doc.model(), 0.2);
        if !prof.is_empty() {
            let lo = prof.iter().map(|p| p.x).fold(f64::MAX, f64::min);
            let hi = prof.iter().map(|p| p.x).fold(f64::MIN, f64::max);
            let mut path = state.parts.path.clone();
            path.push(b);
            let ghost = crate::theme::interaction();
            for w in path.windows(2) {
                let d = (w[1] - w[0]).normalize_or_zero();
                if d == DVec2::ZERO {
                    continue;
                }
                let n = d.perp();
                let band = [w[0] + n * lo, w[1] + n * lo, w[1] + n * hi, w[0] + n * hi];
                let pts: Vec<Pos2> = band.iter().map(|p| xf.to_screen(*p)).collect();
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    ghost.gamma_multiply(0.15),
                    Stroke::new(1.0, ghost.gamma_multiply(0.7)),
                ));
            }
            // the section itself, small, by the pointer
            let span = prof
                .iter()
                .fold((f64::MAX, f64::MIN, f64::MAX, f64::MIN), |a, p| {
                    (a.0.min(p.x), a.1.max(p.x), a.2.min(p.y), a.3.max(p.y))
                });
            let size = (span.1 - span.0).max(span.3 - span.2).max(1e-6);
            let k = 42.0 / size;
            let at = xf.to_screen(b) + Vec2::new(26.0, 26.0);
            let box_r = Rect::from_min_size(at, Vec2::splat(50.0));
            let pal = crate::theme::Palette::of(ui.ctx());
            painter.rect(
                box_r,
                6.0,
                pal.panel,
                Stroke::new(1.0, pal.border),
                egui::StrokeKind::Inside,
            );
            let mid = DVec2::new((span.0 + span.1) / 2.0, (span.2 + span.3) / 2.0);
            let sec: Vec<Pos2> = prof
                .iter()
                .map(|p| {
                    box_r.center()
                        + Vec2::new(((p.x - mid.x) * k) as f32, (-(p.y - mid.y) * k) as f32)
                })
                .collect();
            painter.add(egui::Shape::closed_line(sec, Stroke::new(1.2, pal.text)));
            painter.text(
                box_r.center_bottom() + Vec2::new(0.0, 3.0),
                egui::Align2::CENTER_TOP,
                name,
                egui::FontId::proportional(10.5),
                pal.muted,
            );
        }
    }
    // preview of the tool in progress
    if let (Some(a), Some(b)) = (state.pending, hover_world) {
        let accent = Stroke::new(1.5, crate::theme::interaction());
        match tool {
            Tool::Level => {
                let end = DVec2::new(b.x, a.y);
                painter.extend(egui::Shape::dashed_line(
                    &[xf.to_screen(a), xf.to_screen(end)],
                    accent,
                    10.0,
                    4.0,
                ));
                painter.text(
                    xf.to_screen(end) + Vec2::new(8.0, -6.0),
                    egui::Align2::LEFT_BOTTOM,
                    crate::units::elevation(a.y),
                    egui::FontId::proportional(13.0),
                    accent.color,
                );
            }
            Tool::Wall
            | Tool::Section
            | Tool::Elevation
            | Tool::Grid
            | Tool::Beam
            | Tool::Stair
            | Tool::Railing
            | Tool::Footing
            | Tool::RoomSeparator => {
                let level = match kind {
                    PaneKind::Plan { level } => Some(level),
                    _ => None,
                };
                crate::memberstool::preview(
                    &painter,
                    &|p| Some(xf.to_screen(p.truncate())),
                    doc.model(),
                    level,
                    tool,
                    state,
                    a.extend(0.0),
                    b.extend(0.0),
                );
                // an arc, circle or rectangle of walls being drawn: its
                // centrelines and both faces (MU40); no straight line to
                // the pointer, which read as a straight wall
                let shape = (tool == Tool::Wall)
                    .then(|| wall_shape_preview(state, a, b))
                    .flatten();
                if shape.is_none() {
                    painter.line_segment([xf.to_screen(a), xf.to_screen(b)], accent);
                } else if state.wall_shape == 2 {
                    // a circle's radius, faint
                    painter.extend(egui::Shape::dashed_line(
                        &[xf.to_screen(a), xf.to_screen(b)],
                        Stroke::new(1.0, accent.color.gamma_multiply(0.6)),
                        6.0,
                        4.0,
                    ));
                }
                if let Some(lines) = shape {
                    let t = state.wall_thickness / 2.0;
                    for pts in lines {
                        painter.add(egui::Shape::line(
                            pts.iter().map(|p| xf.to_screen(*p)).collect(),
                            Stroke::new(2.0, accent.color),
                        ));
                        for side in [-t, t] {
                            painter.add(egui::Shape::line(
                                offset_line(&pts, side)
                                    .iter()
                                    .map(|p| xf.to_screen(*p))
                                    .collect(),
                                Stroke::new(1.0, accent.color),
                            ));
                        }
                    }
                } else if tool == Tool::Wall && matches!(kind, PaneKind::Plan { .. }) {
                    // the wall's faces about the line it is drawn along
                    if let Some(faces) = wall_preview(doc.model(), state, a, b) {
                        let thin = Stroke::new(1.0, accent.color);
                        for (p, q) in faces {
                            painter.line_segment([xf.to_screen(p), xf.to_screen(q)], thin);
                        }
                    }
                }
                let len = a.distance(b);
                let text = match (tool, state.wall_shape) {
                    (Tool::Wall, 2) => format!("R {}", crate::units::len(len)),
                    (Tool::Wall, 3) => format!(
                        "{} × {}",
                        crate::units::len((b.x - a.x).abs()),
                        crate::units::len((b.y - a.y).abs())
                    ),
                    (Tool::Wall, 1) => match state.wall_arc_end.and_then(|e| {
                        bulge_through(a, b, e).map(|k| creabim_core::sketch::Curve::arc(a, e, k))
                    }) {
                        Some(c) => format!(
                            "Arc {}",
                            crate::units::len(
                                c.points()
                                    .windows(2)
                                    .map(|w| w[0].distance(w[1]))
                                    .sum::<f64>()
                            )
                        ),
                        None => crate::units::len(len),
                    },
                    _ => crate::units::len(len),
                };
                painter.text(
                    xf.to_screen((a + b) / 2.0) + Vec2::new(0.0, -12.0),
                    egui::Align2::CENTER_BOTTOM,
                    text,
                    egui::FontId::proportional(13.0),
                    accent.color,
                );
            }
            t if t.draws_outline() => {
                painter.rect_stroke(
                    Rect::from_two_pos(xf.to_screen(a), xf.to_screen(b)),
                    0.0,
                    accent,
                    egui::StrokeKind::Middle,
                );
            }
            Tool::Guide => {
                let (through, dir, text) = match state.guide_edge {
                    Some((e0, e1)) => {
                        let d = (e1 - e0).normalize_or_zero();
                        let off = (b - e0).perp_dot(d).abs();
                        (b, d, crate::units::len(off))
                    }
                    None => {
                        let d = (b - a).normalize_or_zero();
                        let deg = d.y.atan2(d.x).to_degrees().rem_euclid(180.0);
                        (a, d, format!("{deg:.1}°"))
                    }
                };
                let reach = rect.width().max(rect.height()) as f64 / xf.scale as f64;
                painter.extend(egui::Shape::dashed_line(
                    &[
                        xf.to_screen(through - dir * reach),
                        xf.to_screen(through + dir * reach),
                    ],
                    accent,
                    10.0,
                    5.0,
                ));
                painter.text(
                    xf.to_screen(b) + Vec2::new(12.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    text,
                    egui::FontId::proportional(13.0),
                    accent.color,
                );
            }
            _ => {}
        }
    }
    if let (Some(s), PaneKind::Plan { .. } | PaneKind::Section { .. }, false) =
        (tool_snap, kind, shift)
    {
        if tool != Tool::Select && !tool.is_annotation() {
            let from = state
                .pending
                .or(sketch_last.map(from_pane))
                .map(|a| xf.to_screen(a));
            let at = xf.to_screen(s.p.truncate());
            match &tracked {
                Some(t) if !t.guides.is_empty() => {
                    snaps::draw_guides(&painter, &pane_project, &guides, &t.guides);
                    snaps::draw_guided(&painter, &pane_project, t, &guides, from);
                }
                _ => snaps::draw_tracked(&painter, &pane_project, s, tool_align, at, from),
            }
        }
    }
    if let Some(p) = hover_world {
        if tool != Tool::Select && !tool.is_annotation() {
            painter.circle_stroke(
                xf.to_screen(p),
                4.0,
                Stroke::new(1.0, crate::theme::interaction()),
            );
        }
    }

    // Shift+right-click places the pivot cursor
    if let (Some(space), true, true, Some(p)) = (
        space,
        response.secondary_clicked(),
        shift,
        response.interact_pointer_pos(),
    ) {
        let level_z = match kind {
            PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
            _ => 0.0,
        };
        let w = hover_world.unwrap_or_else(|| xf.to_world(p));
        let c = pane_to_model(space, w, level_z);
        state.cursor = Some(c);
        state.pivot = modal::Pivot::Cursor;
        state.status = Some(format!(
            "Pivot cursor at {:.2}, {:.2}, {:.2}. Rotate and scale now turn about it.",
            c.x, c.y, c.z
        ));
        return None;
    }
    if let (Some(space), Some(c)) = (space, state.cursor) {
        draw_cursor(&painter, xf.to_screen(model_to_pane(space, c)));
    }
    let right = response.secondary_clicked() && !shift;
    let esc =
        ui.input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input();
    // a detail path that can be made is finished, never dropped
    if tool == Tool::Detail
        && (esc || right)
        && state.details.view == Some(view)
        && crate::detailtool::keep(doc, state, selection)
    {
        return None;
    }
    if esc {
        state.step_back();
    }
    let menu = right && !state.step_back() && tool == Tool::Select;
    if menu {
        // right-click on something not yet selected selects it first
        if let Some(p) = response.interact_pointer_pos() {
            let raw = xf.to_world(p);
            let hit = state
                .hover
                .as_ref()
                .and_then(|h| h.at(pane, p))
                .or_else(|| hit_test(&plan.items, raw, 6.0 / scale as f64, mm))
                .or_else(|| ray_pick(doc, view, kind, raw));
            if let Some(id) = hit.filter(|id| !selection.contains(*id)) {
                selection.only(id);
            }
            let level_z = match kind {
                PaneKind::Plan { level } => doc.model().level(level).map_or(0.0, |l| l.elevation),
                _ => 0.0,
            };
            let w = hover_world.unwrap_or(raw);
            state.menu_at = space.map(|space| pane_to_model(space, w, level_z));
        }
    }
    if tool == Tool::Select && state.modal.is_none() && state.web_menus {
        if menu {
            state.menu_request = response.interact_pointer_pos();
        }
    } else if tool == Tool::Select && state.modal.is_none() {
        let n = selection.len();
        let at = state.menu_at;
        response.context_menu(|ui| {
            if let Some(c) = crate::commands::context_menu(ui, n) {
                match c.modal() {
                    Some(m) => state.armed = Some(m),
                    None => state.commands.push((c, at)),
                }
            }
        });
    }
    if let (PaneKind::Section { .. }, Some(a), Some(b)) = (kind, state.pending, hover_world) {
        if matches!(tool, Tool::Section | Tool::Elevation) {
            preview_cross_section(&painter, &xf, a, b, accent);
        }
    }

    // Fill room: what a click would fill, or where the walls are open
    if let (PaneKind::Plan { level }, Some(p), Some(at), true) = (
        kind,
        hover_world,
        response.hover_pos(),
        crate::fillpreview::on(state),
    ) {
        let look = crate::fillpreview::cached(&mut state.fill_look, doc, level, tool, p).clone();
        let units = doc.model().units();
        crate::fillpreview::draw(&painter, &xf, &look, at, &units, tool);
    }
    // the host a click would go on, and what it would make there
    if let (Some(h), Some(at)) = (host, response.hover_pos()) {
        if hovered != Some(h) {
            draw_hover(&painter, &xf, &plan.items, h, mm);
        }
        let mut text = crate::memberstool::host_preview(doc.model(), state, tool, h);
        if host_hits.len() > 1 {
            let i = state.host_cycle % host_hits.len();
            text += &format!("   {}/{} · Tab", i + 1, host_hits.len());
        }
        let look = crate::handles::PillLook::of(&crate::theme::Palette::of(ui.ctx()));
        crate::handles::plain_pill(&painter, at + Vec2::new(16.0, 18.0), &text, &look);
    }
    // the tracker's pill by the pointer
    if let (true, Some(a), Some(b), Some(at)) =
        (line_tool, state.pending, hover_world, response.hover_pos())
    {
        let look = crate::handles::PillLook::of(&crate::theme::Palette::of(ui.ctx()));
        crate::tracker::draw(
            &painter,
            at,
            &state.tracker,
            a.distance(b),
            crate::tracker::angle_of(a, b),
            &snap_cfg.step_label(),
            &look,
        );
    }

    let mut error = None;
    if let (true, Some(p), PaneKind::Plan { level }) = (tracker_enter, hover_world, kind) {
        state.tracker.clear();
        if let Err(e) = plan_tool(doc, level, tool, p, state, selection) {
            error = Some(e.to_string());
        }
    }
    // Calibrate: two clicks on the reference, then its distance (MU19);
    // right-click or Esc stops it (MU23)
    if state.calibrate.is_some()
        && (response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)))
    {
        state.calibrate = None;
        state.status = Some("Calibrate cancelled.".into());
        return None;
    }
    // files dropped on the view: pictures and DXF drawings become
    // references here, at the pointer (MU23)
    let drops: Vec<std::path::PathBuf> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    });
    if !drops.is_empty() && response.contains_pointer() {
        let at = hover_world.unwrap_or(xf.center);
        for p in drops {
            state.dropped.push((view, at, p));
        }
    }
    // Align: two clicks on the reference, two on the model (MU23)
    if state.align_ref.is_some()
        && (response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)))
    {
        state.align_ref = None;
        state.status = Some("Align cancelled.".into());
        return None;
    }
    if let Some((rid, pts)) = state.align_ref.as_mut() {
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        let ink = |i: usize| {
            if i < 2 {
                accent_selected()
            } else {
                GUIDE_COLOR
            }
        };
        for (i, q) in pts.iter().enumerate() {
            painter.circle_stroke(xf.to_screen(*q), 5.0, Stroke::new(1.5, ink(i)));
        }
        if pts.len() == 2 {
            painter.line_segment(
                [xf.to_screen(pts[0]), xf.to_screen(pts[1])],
                Stroke::new(1.5, accent_selected()),
            );
        }
        if let Some(h) = hover_world {
            painter.circle_stroke(xf.to_screen(h), 4.0, Stroke::new(1.0, ink(pts.len())));
            if pts.len() % 2 == 1 {
                let a = pts[pts.len() - 1];
                painter.line_segment(
                    [xf.to_screen(a), xf.to_screen(h)],
                    Stroke::new(1.0, ink(pts.len())),
                );
            }
        }
        if response.clicked() {
            if let Some(q) = hover_world {
                pts.push(q);
            }
            state.status = Some(
                match pts.len() {
                    1 => "Align: a second point on the reference, along the line to lay down.",
                    2 => {
                        "Align: now the first point of the line it lies along (the model, a guide)."
                    }
                    3 => "Align: the second point of that line.",
                    _ => "",
                }
                .into(),
            );
            if pts.len() >= 4 {
                let (id, p) = (*rid, pts.clone());
                state.align_ref = None;
                state.aligned = Some((id, [p[0], p[1], p[2], p[3]]));
            }
        }
        return None;
    }
    if let Some((rid, pts)) = state.calibrate.as_mut() {
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        // where the next click goes, before the first too (MU23)
        if let (Some(h), true) = (hover_world, pts.is_empty()) {
            painter.circle_stroke(xf.to_screen(h), 4.0, Stroke::new(1.0, accent_selected()));
        }
        for q in pts.iter() {
            painter.circle_stroke(xf.to_screen(*q), 5.0, Stroke::new(1.5, accent_selected()));
        }
        if let (Some(a), Some(h)) = (pts.first(), hover_world) {
            painter.line_segment(
                [xf.to_screen(*a), xf.to_screen(h)],
                Stroke::new(1.0, accent_selected()),
            );
        }
        if response.clicked() {
            if let Some(q) = hover_world {
                pts.push(q);
            }
            if pts.len() >= 2 {
                state.calibrated = Some((*rid, pts[0], pts[1]));
                state.calibrate = None;
            } else {
                state.status =
                    Some("Calibrate: click a second point a known distance from the first.".into());
            }
        }
        return None;
    }
    if response.clicked() {
        state.tracker.clear();
        let p = hover_world?;
        let at = response.interact_pointer_pos().unwrap_or_default();
        let raw = xf.to_world(at);
        // what is pre-selected there (Tab may have moved past the first);
        // else linework first (edges, symbols), then what the view ray hits
        let hover_pick = state.hover.as_mut().and_then(|h| {
            h.list = false;
            h.at(pane, at)
        });
        let picked = || {
            hover_pick
                .or_else(|| hit_test(&plan.items, raw, 6.0 / scale as f64, mm))
                .or_else(|| ray_pick(doc, view, kind, raw))
        };
        let result = match (kind, tool) {
            (_, Tool::Select) => {
                let under = picked();
                // a double-click on the selected element opens it in Edit mode
                if response.double_clicked()
                    && !shift
                    && under.is_some()
                    && under == selection.active
                {
                    if let Some(id) = under {
                        // a callout, section or elevation mark opens its view
                        let is_view = matches!(
                            doc.model().get(id).map(|e| &e.kind),
                            Some(ElementKind::View(_))
                        );
                        if is_view && id != view {
                            state.want_view = Some(id);
                        } else {
                            state.status =
                                Some(crate::editmode::enter(doc.model(), id, &mut state.edit));
                        }
                    }
                    return None;
                }
                match (under, shift) {
                    (Some(id), true) => selection.toggle(id),
                    (None, true) => {}
                    (id, false) => selection.set(id),
                }
                Ok(())
            }
            (_, Tool::Dimension) => match (dim_hover, &view_def) {
                (Some(hp), Some(_)) => {
                    let lines: Vec<(DVec2, DVec2, f64)> = plan
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            PlanItem::Dimension { a, b, offset, .. } => Some((*a, *b, *offset)),
                            _ => None,
                        })
                        .collect();
                    let tol = 8.0 / xf.scale as f64;
                    dimension_tool(doc, view, hp, raw, dim_axis, state, selection, &lines, tol)
                }
                _ => Ok(()),
            },
            (PaneKind::Axonometric, Tool::Align) => {
                let under = align_under.unwrap_or_default();
                state.status = Some(crate::align3d::click(
                    doc,
                    &mut state.align,
                    &under,
                    selection,
                ));
                Ok(())
            }
            (_, Tool::Align) => crate::aligntool::click(
                doc,
                view,
                kind,
                &plan.items,
                &centre_lines,
                raw,
                tool_snap.filter(|_| !shift),
                6.0 / scale as f64,
                mm,
                state,
                selection,
            ),
            (_, Tool::Tag) => tag_tool(doc, view, &view_def, picked(), raw, state, selection),
            (PaneKind::Plan { .. } | PaneKind::Section { .. }, Tool::Callout) => {
                // the new callout is selected where it was drawn; its view
                // opens from Properties or with a double-click
                crate::detailtool::callout_click(doc, view, p, state, selection).map(|_| ())
            }
            (PaneKind::Plan { .. } | PaneKind::Section { .. }, Tool::Detail) => {
                crate::detailtool::detail_click(doc, view, p, raw, state, selection)
            }
            (_, t) if crate::annotate::is_tool(t) => match &view_def {
                Some(v) => crate::annotate::click(
                    doc,
                    view,
                    v,
                    t,
                    dim_hit,
                    raw,
                    &plan.items,
                    6.0 / scale as f64,
                    &mut state.anno,
                    selection,
                )
                .map(|msg| {
                    if !msg.is_empty() {
                        state.status = Some(msg);
                    }
                }),
                None => Ok(()),
            },
            (PaneKind::Section { start, end }, _) => {
                section_tool(doc, start, end, tool, p, picked(), state, selection)
            }
            (PaneKind::Plan { level }, tool) => {
                if tool == Tool::Guide && state.pending.is_none() {
                    state.guide_edge = tool_snap
                        .filter(|s| {
                            !shift
                                && matches!(
                                    s.kind,
                                    snaps::SnapKind::OnEdge
                                        | snaps::SnapKind::Guide
                                        | snaps::SnapKind::Midpoint
                                )
                        })
                        .and_then(|s| state.snaps.get(&view)?.2.edge_at(s.p))
                        .map(|(a, b)| (a.truncate(), b.truncate()));
                }
                plan_tool(doc, level, tool, p, state, selection)
            }
            (PaneKind::Axonometric | PaneKind::Schedule, _) => Ok(()),
        };
        if let Err(e) = result {
            error = Some(e.to_string());
        }
    }

    // what the pane under the pointer does is said in the status bar,
    // never over the drawing
    if style.decorations && ui.rect_contains_pointer(rect) {
        state.hint = Some(
            match (kind, tool) {
                (PaneKind::Plan { .. }, Tool::Select) => {
                    "Click to select, Shift+click to add, drag on empty paper for a box. Drag grips to stretch; Enter or a double-click opens Edit mode to add and remove corners. M move, G grab, R rotate, S scale, Shift+D duplicate."
                }
                (PaneKind::Plan { .. }, t) => t.hint(),
                (PaneKind::Section { .. }, t) => t.section_hint(),
                (PaneKind::Axonometric, Tool::Tag) => Tool::Tag.hint(),
                (PaneKind::Axonometric, Tool::Note) => Tool::Note.hint(),
                (PaneKind::Axonometric, Tool::Align) => {
                    "Align in 3D: click a face, edge, corner, centre line or grid line for the reference (empty space takes the plane of the level nearest the ground), then a face to align, stretch or extend to it. Tab picks what is behind. Esc lets go."
                }
                (PaneKind::Axonometric, _) => {
                    "Drag to orbit, click to select. The Tag tool works here too. Right-drag pans, the wheel zooms."
                }
                (PaneKind::Schedule, _) => {
                    "Click a row to select its element."
                }
            },
        );
    }
    error
}

/// Underlays (another level traced under a plan) are drawn in this colour.
pub const UNDERLAY_COLOR: Color32 = Color32::from_rgb(178, 184, 196);

/// Drag a level's line or head in a section or elevation: the level (and
/// everything on it) follows the pointer up and down, onto model points it
/// snaps to, else to the nearest centimetre (half inch). Returns `None`
/// when the pointer isn't dragging a level.
#[allow(clippy::too_many_arguments)]
fn drag_level(
    doc: &mut Document,
    plan: &Drawing,
    xf: &Xf,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &mut Selection,
    mm: f64,
    snapped: Option<snaps::Snap>,
) -> Option<Option<String>> {
    if response.drag_started_by(egui::PointerButton::Primary) {
        let press = response.ctx.input(|i| i.pointer.press_origin())?;
        // the grip on a selected level's head moves the head instead
        let on_head = plan.items.iter().any(|it| match it {
            PlanItem::Annotation { id, item } => {
                selection.contains(*id)
                    && matches!(item.shape, creabim_geom::annotation::Shape::Level { at, .. } if xf.to_screen(at).distance(press) < 9.0)
            }
            _ => false,
        });
        if on_head {
            return None;
        }
        let w = xf.to_world(press);
        let id = hit_test(&plan.items, w, 6.0 / xf.scale as f64, mm)?;
        let l = doc.model().level(id)?;
        state.level_drag = Some((id, l.elevation - w.y));
        state.drag_owner = Some(response.id);
        selection.only(id);
    }
    if state.level_drag.is_some() && state.drag_owner != Some(response.id) {
        return None;
    }
    let (id, off) = state.level_drag?;
    if response.drag_stopped() || !response.dragged() {
        state.level_drag = None;
        return Some(None);
    }
    let p = response.interact_pointer_pos()?;
    let step = if crate::units::get().is_imperial() {
        0.0127
    } else {
        0.01
    };
    let z = match snapped {
        Some(s) => s.p.y,
        None => ((xf.to_world(p).y + off) / step).round() * step,
    };
    let name = doc
        .model()
        .get(id)
        .map(|e| e.name.clone())
        .unwrap_or_default();
    let r = doc.edit_merged(id, "Move level", |e| {
        if let ElementKind::Level(l) = &mut e.kind {
            l.elevation = z;
        }
    });
    state.status = Some(format!(
        "{name} at {}. Everything on it moves with it.",
        crate::units::elevation(z)
    ));
    Some(r.err().map(|e| e.to_string()))
}

/// Guides are drawn in this colour, in every pane.
/// Split's cutting line and plane (MU39).
pub const SPLIT_RED: Color32 = Color32::from_rgb(222, 60, 60);

pub const GUIDE_COLOR: Color32 = Color32::from_rgb(190, 60, 170);

/// The part of the screen segment ab inside `r`.
pub fn clip_to_rect(a: Pos2, b: Pos2, r: Rect) -> Option<(Pos2, Pos2)> {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    let d = b - a;
    for (p, q) in [
        (-d.x, a.x - r.min.x),
        (d.x, r.max.x - a.x),
        (-d.y, a.y - r.min.y),
        (d.y, r.max.y - a.y),
    ] {
        if p.abs() < 1e-9 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 < t1).then(|| (a + d * t0, a + d * t1))
}

/// A click with a drawing tool in a plan (or on a level in the 3D view).
/// The points of a chain of clicks (walls, beams) are kept for alignment
/// tracking while it goes on.
pub fn plan_tool(
    doc: &mut Document,
    level: ElementId,
    tool: Tool,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    let started = state.pending.is_none();
    let r = plan_click(doc, level, tool, p, state, selection);
    match state.pending {
        Some(q) => {
            if started || state.chain.0 != Some(level) {
                state.chain = (Some(level), vec![]);
            }
            if state.chain.1.last() != Some(&q) {
                state.chain.1.push(q);
            }
        }
        None => state.chain.1.clear(),
    }
    r
}

/// A wall as the Wall tool draws it between two clicks: along its location
/// line, the exterior to the right of the direction drawn unless flipped.
pub fn new_wall(
    model: &creabim_core::Model,
    level: ElementId,
    state: &PlanState,
    a: DVec2,
    b: DVec2,
) -> Wall {
    let height = model.level(level).map_or(3.0, |l| l.height);
    let mut w = Wall::new(level, a, b, state.wall_thickness, height);
    w.location = state.wall_location;
    // the wall type in use sets its thickness, location line and grid
    let typed = state.typed(model, Element::new("Wall", ElementKind::Wall(w.clone())));
    if let ElementKind::Wall(tw) = typed.kind {
        w = tw;
    }
    w.flip = state.wall_flip;
    creabim_core::walls::set_location_line(model, &mut w, a, b);
    w
}

/// The centrelines the Wall tool's shape would make from its clicks so
/// far (`a`, the arc's end if clicked) to the pointer `b`, for its ghost in
/// plans and in 3D (MU40): an arc, a circle, a rectangle's four sides.
/// None for a straight wall, which draws its own.
pub fn wall_shape_preview(state: &PlanState, a: DVec2, b: DVec2) -> Option<Vec<Vec<DVec2>>> {
    match (state.wall_shape, state.wall_arc_end) {
        // the arc's end not yet clicked: already an arc (a quarter turn),
        // so it never reads as a straight wall; the next click bends it
        (1, None) if a.distance(b) > 1e-6 => Some(vec![creabim_core::sketch::Curve::arc(
            a,
            b,
            -(std::f64::consts::PI / 8.0).tan(),
        )
        .points()]),
        (1, Some(e)) => Some(vec![match bulge_through(a, b, e) {
            Some(k) => creabim_core::sketch::Curve::arc(a, e, k).points(),
            None => vec![a, e],
        }]),
        (2, _) if a.distance(b) > 1e-6 => Some(vec![(0..=72)
            .map(|i| a + DVec2::from_angle(i as f64 / 72.0 * std::f64::consts::TAU) * a.distance(b))
            .collect()]),
        (3, _) => Some(vec![vec![
            a,
            DVec2::new(b.x, a.y),
            b,
            DVec2::new(a.x, b.y),
            a,
        ]]),
        _ => None,
    }
}

/// The bulge of the arc from `a` through `t` to `b` (as [`Wall::bulge`]);
/// none when the three are in a line.
/// `pts` moved `d` to its left, corner by corner (a ghost's faces).
pub fn offset_line(pts: &[DVec2], d: f64) -> Vec<DVec2> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let before = (i > 0).then(|| (pts[i] - pts[i - 1]).normalize_or_zero());
            let after = (i + 1 < n).then(|| (pts[i + 1] - pts[i]).normalize_or_zero());
            let dir = match (before, after) {
                (Some(a), Some(b)) => (a + b).normalize_or(b),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                _ => DVec2::X,
            };
            pts[i] + dir.perp() * d
        })
        .collect()
}

pub fn bulge_through(a: DVec2, t: DVec2, b: DVec2) -> Option<f64> {
    let d = 2.0 * (a.x * (t.y - b.y) + t.x * (b.y - a.y) + b.x * (a.y - t.y));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |p: DVec2| p.length_squared();
    let c = DVec2::new(
        (sq(a) * (t.y - b.y) + sq(t) * (b.y - a.y) + sq(b) * (a.y - t.y)) / d,
        (sq(a) * (b.x - t.x) + sq(t) * (a.x - b.x) + sq(b) * (t.x - a.x)) / d,
    );
    let tau = std::f64::consts::TAU;
    let ang = |p: DVec2| (p - c).y.atan2((p - c).x);
    let norm = |x: f64| x.rem_euclid(tau);
    let (a0, at, ae) = (ang(a), ang(t), ang(b));
    let sweep = if norm(at - a0) < norm(ae - a0) {
        norm(ae - a0)
    } else {
        -norm(a0 - ae)
    };
    // Curve::arc's bulge: tan(sweep / 4), anticlockwise positive
    Some((sweep / 4.0).tan())
}

/// The faces of the wall being drawn from `a` to `b`, and where its
/// exterior is, for the preview.
pub fn wall_preview(
    model: &creabim_core::Model,
    state: &PlanState,
    a: DVec2,
    b: DVec2,
) -> Option<[(DVec2, DVec2); 3]> {
    if a.distance(b) < 1e-6 {
        return None;
    }
    let w = new_wall(model, ElementId::new(), state, a, b);
    let n = w.direction().perp() * (w.thickness / 2.0);
    let out = if w.flip { n } else { -n };
    let mid = (w.start + w.end) / 2.0;
    Some([
        (w.start + n, w.end + n),
        (w.start - n, w.end - n),
        // a tick from the middle out of the exterior face
        (mid + out, mid + out * 2.5),
    ])
}

fn plan_click(
    doc: &mut Document,
    level: ElementId,
    tool: Tool,
    p: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    match tool {
        Tool::Select | Tool::Align => Ok(()),
        Tool::Level => {
            state.status = Some(
                "Level lines are drawn in a section or elevation. Here, add, rename and move levels in the Levels window.".into(),
            );
            state.want_levels = true;
            Ok(())
        }
        Tool::Section | Tool::Elevation => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if a.distance(p) > 0.2 => {
                add_section_view(doc, a, p, tool == Tool::Section, selection)
            }
            Some(_) => Ok(()),
        },
        // a circle: its centre, then a point on it; two half walls (MU40)
        Tool::Wall if state.wall_shape == 2 => match state.pending.take() {
            None => {
                state.pending = Some(p);
                state.status = Some("Circle wall: click a point on it (its radius).".into());
                Ok(())
            }
            Some(c) if c.distance(p) > 0.1 => {
                let a = p;
                let b = c - (p - c);
                let mut w1 = new_wall(doc.model(), level, state, a, b);
                let mut w2 = new_wall(doc.model(), level, state, b, a);
                for (w, s, e) in [(&mut w1, a, b), (&mut w2, b, a)] {
                    // on its centreline: a curve's location line is its middle
                    w.start = s;
                    w.end = e;
                    w.bulge = 1.0;
                }
                let e1 = state.typed(doc.model(), Element::new("Wall", ElementKind::Wall(w1)));
                let e2 = state.typed(doc.model(), Element::new("Wall", ElementKind::Wall(w2)));
                selection.only(e1.id);
                state.status = Some(format!(
                    "Circle wall, radius {}: two half walls. Click a centre for another.",
                    crate::units::len(c.distance(p))
                ));
                doc.commit(Transaction::new("Draw circle wall").insert(e1).insert(e2))
            }
            Some(_) => Ok(()),
        },
        // a rectangle of walls: two opposite corners (MU40)
        Tool::Wall if state.wall_shape == 3 => match state.pending.take() {
            None => {
                state.pending = Some(p);
                state.status = Some("Rectangle of walls: click the opposite corner.".into());
                Ok(())
            }
            Some(a) if (p - a).abs().min_element() > 0.1 => {
                let c = [a, DVec2::new(p.x, a.y), p, DVec2::new(a.x, p.y)];
                let mut tx = Transaction::new("Draw rectangle of walls");
                let mut first = None;
                for i in 0..4 {
                    let w = new_wall(doc.model(), level, state, c[i], c[(i + 1) % 4]);
                    let e = state.typed(doc.model(), Element::new("Wall", ElementKind::Wall(w)));
                    first.get_or_insert(e.id);
                    tx = tx.insert(e);
                }
                if let Some(id) = first {
                    selection.only(id);
                }
                state.status =
                    Some("Four walls, joined at the corners. Click a corner for another.".into());
                doc.commit(tx)
            }
            Some(_) => Ok(()),
        },
        // an arc: its start, its end, then a point it passes through
        Tool::Wall if state.wall_shape == 1 => match (state.pending, state.wall_arc_end) {
            (None, _) => {
                state.pending = Some(p);
                state.wall_arc_end = None;
                state.status = Some("Arc wall: click its end.".into());
                Ok(())
            }
            (Some(a), None) if a.distance(p) > 0.05 => {
                state.wall_arc_end = Some(p);
                state.status = Some("Arc wall: click a point it passes through.".into());
                Ok(())
            }
            (Some(a), Some(b)) => {
                let Some(bulge) = bulge_through(a, p, b) else {
                    state.status =
                        Some("That point is on the straight line: pick one off it.".into());
                    return Ok(());
                };
                let mut w = new_wall(doc.model(), level, state, a, b);
                w.start = a;
                w.end = b;
                w.bulge = bulge;
                let e = state.typed(doc.model(), Element::new("Wall", ElementKind::Wall(w)));
                selection.only(e.id);
                // the next arc starts where this one ends
                state.pending = Some(b);
                state.wall_arc_end = None;
                state.status = Some("Arc wall placed. Click the next one's end, or Esc.".into());
                doc.commit(Transaction::new("Draw arc wall").insert(e))
            }
            _ => Ok(()),
        },
        Tool::Wall => match state.pending {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if a.distance(p) > 0.05 => {
                let w = new_wall(doc.model(), level, state, a, p);
                let e = state.typed(doc.model(), Element::new("Wall", ElementKind::Wall(w)));
                selection.only(e.id);
                state.pending = Some(p);
                doc.commit(Transaction::new("Draw wall").insert(e))
            }
            Some(_) => Ok(()),
        },
        Tool::Door | Tool::Window | Tool::Opening => {
            place_opening(doc, level, p, tool, state, selection)
        }
        Tool::Dimension
        | Tool::Tag
        | Tool::Angle
        | Tool::Radius
        | Tool::Diameter
        | Tool::ArcLength
        | Tool::Note
        | Tool::SpotLevel
        | Tool::Sketch
        | Tool::Callout
        | Tool::Detail => Ok(()),
        // a library profile placed at the click, pulled up (MU36)
        Tool::Extrusion if state.parts.extrusion_library.is_some() => {
            let name = state.parts.extrusion_library.clone().unwrap_or_default();
            let Some(outline) = creabim_core::profiles::find(doc.model(), &name) else {
                state.status = Some(format!("There is no profile {name} any more."));
                return Ok(());
            };
            let s = &state.parts;
            let mut part = creabim_core::parts::Part::extrusion(
                level,
                outline.iter().map(|q| *q + p).collect(),
                s.extrusion_base,
                s.extrusion_height,
            );
            part.profile = Some(name.clone());
            let e = Element::new(name.clone(), ElementKind::Part(part));
            let id = e.id;
            doc.commit(creabim_core::Transaction::new("Place profile").insert(e))?;
            selection.only(id);
            state.status = Some(format!(
                "{name} placed, pulled up {}. Click again for another.",
                crate::units::len(s.extrusion_height)
            ));
            Ok(())
        }
        Tool::Slab
        | Tool::Roof
        | Tool::Shaft
        | Tool::Extrusion
        | Tool::Void
        | Tool::Terrain
        | Tool::Paving
        | Tool::Room
        | Tool::Ceiling => crate::slabtool::click(doc, level, p, state, selection),
        Tool::Sweep | Tool::Planting => {
            crate::partstool::click(doc, level, tool, p, state, selection)
        }
        Tool::Column | Tool::Beam | Tool::Stair | Tool::Railing | Tool::Footing => {
            crate::memberstool::click(doc, level, tool, p, state, selection)
        }
        Tool::Object => crate::objecttool::click(doc, level, p, state, selection),
        Tool::RoomSeparator => crate::roomtool::separator_click(doc, level, p, state, selection),
        Tool::Guide => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) => {
                let (ga, gb) = match state.guide_edge.take() {
                    Some((e0, e1)) => (p, p + (e1 - e0)),
                    None if a.distance(p) > 0.01 => (a, p),
                    None => return Ok(()),
                };
                let z = doc.model().level(level).map_or(0.0, |l| l.elevation);
                let e = Element::new(
                    "Guide",
                    ElementKind::Guide(creabim_core::Guide {
                        level: Some(level),
                        a: ga.extend(z),
                        b: gb.extend(z),
                    }),
                );
                doc.commit(Transaction::new("Add guide").insert(e))
            }
        },
        Tool::Grid => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if a.distance(p) > 0.1 => {
                let e = creabim_core::grids::new_grid(doc.model(), state.grid_group, a, p);
                state.status = Some(format!("Added {}. Click again for the next line.", e.name));
                selection.only(e.id);
                doc.commit(Transaction::new("Add grid line").insert(e))
            }
            Some(a) => {
                state.pending = Some(a);
                Ok(())
            }
        },
        Tool::Mass => {
            let e = Element::new(
                "Mass",
                ElementKind::FreeMesh(FreeMesh::new(
                    Some(level),
                    DVec3::new(p.x, p.y, 0.0),
                    EditMesh::cuboid(DVec3::new(2.0, 2.0, 3.0)),
                )),
            );
            selection.only(e.id);
            doc.commit(Transaction::new("Add mass").insert(e))
        }
    }
}

/// The centre lines of what plan or section `view` draws, seen edge on
/// (see `crate::centres`), kept until the model changes.
fn pane_centre_lines(
    state: &mut PlanState,
    doc: &Document,
    view: ElementId,
    kind: PaneKind,
    plan: &Drawing,
) -> Arc<Vec<crate::aligntool::CentreLine>> {
    let rev = doc.model().revision();
    if let Some((r, lines)) = state.centre_lines.get(&view) {
        if *r == rev {
            return lines.clone();
        }
    }
    let (space, look) = match kind {
        PaneKind::Plan { .. } => (Space::Plan, -DVec3::Z),
        PaneKind::Section { start, end } => (
            Space::Section { start, end },
            (end - start).normalize_or_zero().perp().extend(0.0),
        ),
        _ => return Arc::new(vec![]),
    };
    let drawn: HashSet<ElementId> = plan.items.iter().map(|i| i.id()).collect();
    let scene = cached_pick_scene(state, doc, view);
    let to_pane = |p: DVec3| model_to_pane(space, p);
    let lines: Vec<crate::aligntool::CentreLine> =
        crate::centres::all(doc.model(), &scene, &|id| drawn.contains(&id))
            .iter()
            .filter_map(|c| {
                let (a, b) = crate::centres::edge_on(c, look, &to_pane)?;
                Some((c.id, a, b))
            })
            .collect();
    let lines = Arc::new(lines);
    state.centre_lines.insert(view, (rev, lines.clone()));
    lines
}

/// A 3D view's frame and the snaps of what it shows, kept until the model
/// changes.
fn axo_frame_snaps(
    state: &mut PlanState,
    doc: &Document,
    view: ElementId,
) -> Option<Arc<(creabim_geom::section::AxoFrame, Snapper)>> {
    let rev = doc.model().revision();
    if let Some((r, fs)) = state.axo_snaps.get(&view) {
        if *r == rev {
            return Some(fs.clone());
        }
    }
    let Some(ElementKind::View(View {
        kind: ViewKind::Axonometric { azimuth, altitude },
        hidden,
        ..
    })) = doc.model().get(view).map(|e| &e.kind)
    else {
        return None;
    };
    let frame = creabim_geom::section::AxoFrame::new(doc.model(), *azimuth, *altitude);
    let hidden = hidden.hidden_set(doc.model());
    let scene = cached_pick_scene(state, doc, view);
    let mut snapper = Snapper::from_scene(&scene, &[]);
    snapper.add_grids(doc.model(), &hidden);
    let fs = Arc::new((frame, snapper));
    state.axo_snaps.insert(view, (rev, fs.clone()));
    Some(fs)
}

/// What the Align tool takes under pointer `p` of 3D view `view`: the
/// face of what the pre-selection reads there (Tab may have moved it), an
/// object snap, and the lowest level for empty space.
fn axo_align_under(
    state: &mut PlanState,
    doc: &Document,
    view: ElementId,
    fs: &(creabim_geom::section::AxoFrame, Snapper),
    xf: &Xf,
    p: Pos2,
    cfg: &snaps::SnapSettings,
) -> crate::align3d::Under {
    let model = doc.model();
    // the level nearest the ground
    let level = model
        .levels()
        .map(|(e, l)| (e.id, l.elevation))
        .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
    let scene = cached_pick_scene(state, doc, view);
    let (frame, snapper) = (&fs.0, &fs.1);
    let (o, d) = frame.ray(xf.to_world(p));
    let chosen = state.hover.as_ref().and_then(|h| h.at(Pane::View(view), p));
    let face = crate::align3d::face_pick(model, &scene, o, d, chosen);
    let project = |m: DVec3| Some(xf.to_screen(frame.project(m).truncate()));
    let free = face.as_ref().map(|f| f.face.point);
    let snap = snapper
        .snap(cfg, p, &project, None, free)
        .filter(|s| s.kind != SnapKind::Grid);
    let edge = snap.and_then(|s| snapper.edge_at(s.p));
    let (centres, centre) =
        crate::align3d::centres_under(model, &scene, face.as_ref(), p, &project);
    let centre = centre.filter(|_| crate::align3d::takes_centres(&state.align));
    crate::align3d::Under {
        face,
        snap,
        edge,
        level,
        centres,
        centre,
    }
}

/// The element a section or 3D view shows at drawing point `p`, found by
/// casting the viewing ray through the model.
fn ray_pick(doc: &Document, view: ElementId, kind: PaneKind, p: DVec2) -> Option<ElementId> {
    let scene = pick_scene(doc, view);
    ray_pick_all(doc.model(), &scene, view, kind, p, None)
        .first()
        .copied()
}

/// The model view `view` picks from: what it leaves out is not in it.
fn pick_scene(doc: &Document, view: ElementId) -> Vec<SceneItem> {
    // what the view leaves out can't be clicked in it, and isn't in the
    // model its drawing was made from
    let (hidden, axo) = match doc.model().get(view).map(|e| &e.kind) {
        Some(ElementKind::View(v)) => (
            v.hidden.hidden_set(doc.model()),
            matches!(v.kind, creabim_core::ViewKind::Axonometric { .. }),
        ),
        _ => Default::default(),
    };
    let model = creabim_core::visibility::without(doc.model(), &hidden);
    let mut scene = creabim_geom::build_scene(&model);
    scene.retain(|s| !hidden.contains(&s.id));
    // a 3D view's crop boxes cut its drawing, and what they cut away
    // can't be clicked either
    if axo {
        let boxes = creabim_geom::cropbox::active(doc.model(), Some(view));
        creabim_geom::cropbox::crop_scene(doc.model(), &mut scene, &boxes);
    }
    scene
}

/// [`pick_scene`], kept until the model changes.
fn cached_pick_scene(
    state: &mut PlanState,
    doc: &Document,
    view: ElementId,
) -> Arc<Vec<SceneItem>> {
    let rev = doc.model().revision();
    match state.pick_scenes.get(&view) {
        Some((r, s)) if *r == rev => s.clone(),
        _ => {
            let s = Arc::new(pick_scene(doc, view));
            state.pick_scenes.insert(view, (rev, s.clone()));
            s
        }
    }
}

/// Every element the viewing ray through drawing point `p` passes, nearest
/// first. In a plan the ray looks down through the level, and only what
/// the plan draws (`drawn`) counts.
fn ray_pick_all(
    model: &creabim_core::Model,
    scene: &[SceneItem],
    view: ElementId,
    kind: PaneKind,
    p: DVec2,
    drawn: Option<&HashSet<ElementId>>,
) -> Vec<ElementId> {
    let (o, d) = match kind {
        PaneKind::Plan { level } => {
            // a click inside a floor picks its slab
            let slabs: Vec<(ElementId, Vec<DVec2>)> = model
                .iter()
                .filter_map(|e| match &e.kind {
                    ElementKind::Slab(s) if s.level == level => Some((e.id, s.outline())),
                    _ => None,
                })
                .collect();
            let slab = slab_at(&slabs, p);
            let Some(drawn) = drawn else {
                return slab.into_iter().collect();
            };
            let z = model.level(level).map_or(0.0, |l| l.elevation);
            let o = DVec3::new(p.x, p.y, z + 100.0);
            let mut out: Vec<ElementId> =
                creabim_geom::pick::pick_all(scene, o.as_vec3(), -glam::Vec3::Z)
                    .into_iter()
                    .filter(|h| drawn.contains(&h.id) && o.z - h.t as f64 > z - 1.0)
                    .map(|h| h.id)
                    .collect();
            if let Some(s) = slab.filter(|s| !out.contains(s)) {
                out.push(s);
            }
            return out;
        }
        PaneKind::Schedule => return vec![],
        PaneKind::Section { start, end } => {
            let look = (end - start).normalize_or_zero().perp();
            let xy = PaneKind::section_point(start, end, p);
            (DVec3::new(xy.x, xy.y, p.y), look.extend(0.0))
        }
        PaneKind::Axonometric => {
            let Some(ElementKind::View(View {
                kind: ViewKind::Axonometric { azimuth, altitude },
                ..
            })) = model.get(view).map(|e| &e.kind)
            else {
                return vec![];
            };
            creabim_geom::section::AxoFrame::new(model, *azimuth, *altitude).ray(p)
        }
    };
    creabim_geom::pick::pick_all(scene, o.as_vec3(), d.as_vec3())
        .into_iter()
        .map(|h| h.id)
        .collect()
}

/// What holds others and turns see-through in X-ray: walls, slabs,
/// roofs and masses.
pub fn is_xray_host(kind: &ElementKind) -> bool {
    match kind {
        ElementKind::Wall(_) | ElementKind::Slab(_) | ElementKind::Roof(_) => true,
        ElementKind::FreeMesh(m) => !matches!(
            m.ifc_class.as_str(),
            "IfcBeam" | "IfcColumn" | "IfcMember" | "IfcStair" | "IfcRailing"
        ),
        _ => false,
    }
}

/// With X-ray on, what is inside comes before what holds it: the beam in
/// the wall is what a click takes first.
pub fn xray_order(model: &creabim_core::Model, under: &mut [ElementId]) {
    under.sort_by_key(|id| model.get(*id).is_some_and(|e| is_xray_host(&e.kind)));
}

/// Everything under drawing point `p` of a view, in the order a click
/// takes it: its linework first, then what the view ray passes.
#[allow(clippy::too_many_arguments)]
fn under_pointer(
    state: &mut PlanState,
    doc: &Document,
    view: ElementId,
    kind: PaneKind,
    plan: &Drawing,
    p: DVec2,
    tol: f64,
    mm: f64,
) -> Vec<ElementId> {
    let mut under = hit_all(&plan.items, p, tol, mm);
    // a crop box's dashed outline picks it (A4-5)
    for (id, lo, hi) in crop_rects(doc.model(), view) {
        let c = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
        if (0..4).any(|i| segment_distance(p, c[i], c[(i + 1) % 4]) < tol) && !under.contains(&id) {
            under.push(id);
        }
    }
    // a reference's outline picks it (MU19), after what is drawn
    for (id, r) in creabim_core::reference::in_view(doc.model(), view) {
        if r.hidden || under.contains(&id) {
            continue;
        }
        if let Some(c) = r.outline() {
            if (0..4).any(|i| segment_distance(p, c[i], c[(i + 1) % 4]) < tol) {
                under.push(id);
            }
        }
    }
    let scene = cached_pick_scene(state, doc, view);
    let drawn: HashSet<ElementId> = match kind {
        PaneKind::Plan { .. } => plan.items.iter().map(|i| i.id()).collect(),
        _ => HashSet::new(),
    };
    for id in ray_pick_all(doc.model(), &scene, view, kind, p, Some(&drawn)) {
        if !under.contains(&id) {
            under.push(id);
        }
    }
    // underlays and guides drawn from elsewhere are not picked here
    under.retain(|id| doc.model().get(*id).is_some());
    if state.xray.contains(&view) {
        xray_order(doc.model(), &mut under);
    }
    under
}

/// A click with a drawing tool in a section or elevation. `p` is the
/// drawing point (along the line, height); `picked` is what is under it.
#[allow(clippy::too_many_arguments)]
fn section_tool(
    doc: &mut Document,
    start: DVec2,
    end: DVec2,
    tool: Tool,
    p: DVec2,
    picked: Option<ElementId>,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    let at = |p: DVec2| PaneKind::section_point(start, end, p);
    match tool {
        Tool::Level => match state.pending.take() {
            None => {
                state.pending = Some(p);
                state.status = Some(format!(
                    "New level at {}. Click where its line ends.",
                    crate::units::elevation(p.y)
                ));
                Ok(())
            }
            Some(a) => {
                let model = doc.model();
                let z = a.y;
                if model.levels().any(|(_, l)| (l.elevation - z).abs() < 1e-4) {
                    state.status = Some("There is a level at that height already.".into());
                    return Ok(());
                }
                // up to the next level, or as tall as the one below
                let list = creabim_core::levels::sorted(model);
                let height = list
                    .iter()
                    .find(|(_, _, l)| l.elevation > z + 1e-6)
                    .map(|(_, _, l)| l.elevation - z)
                    .or_else(|| {
                        list.iter()
                            .rev()
                            .find(|(_, _, l)| l.elevation < z)
                            .map(|(_, _, l)| l.height)
                    })
                    .unwrap_or(3.0)
                    .max(0.5);
                let name = creabim_core::levels::next_name(model);
                let (mut tx, id, _) = creabim_core::levels::add(model, &name, z, height, true);
                tx.label = "Draw level".into();
                selection.only(id);
                state.status = Some(format!(
                    "Added {name} at {} with its floor plan. Drag its line to move it.",
                    crate::units::elevation(z)
                ));
                doc.commit(tx)
            }
        },
        Tool::Wall => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) => {
                let (x0, x1) = (a.x.min(p.x), a.x.max(p.x));
                let (z0, z1) = (a.y.min(p.y), a.y.max(p.y));
                if x1 - x0 < 0.05 {
                    return Ok(());
                }
                let Some((level, elevation)) = level_below(doc, z0) else {
                    return Ok(());
                };
                let height = if z1 - z0 < 0.1 {
                    doc.model().level(level).map_or(3.0, |l| l.height)
                } else {
                    z1 - z0
                };
                // set just behind the section plane, so its face is what you drew
                let behind =
                    (end - start).normalize_or_zero().perp() * (state.wall_thickness / 2.0 + 0.01);
                let e = Element::new(
                    "Wall",
                    ElementKind::Wall(Wall {
                        level,
                        start: at(DVec2::new(x0, 0.0)) + behind,
                        end: at(DVec2::new(x1, 0.0)) + behind,
                        thickness: state.wall_thickness,
                        height,
                        base_offset: z0 - elevation,
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
                    }),
                );
                // its type's thickness, but its drawn height
                let mut e = state.typed(doc.model(), e);
                if let ElementKind::Wall(w) = &mut e.kind {
                    w.height = height;
                }
                selection.only(e.id);
                doc.commit(Transaction::new("Draw wall in section").insert(e))
            }
        },
        Tool::Door | Tool::Window | Tool::Opening => {
            // the wall under the cursor hosts the opening (or the wall of
            // the door or window under it)
            let Some(mut host) = picked else {
                return Ok(());
            };
            if let Some(ElementKind::Opening(o)) = doc.model().get(host).map(|e| &e.kind) {
                host = o.host;
            }
            let Some(ElementKind::Wall(w)) = doc.model().get(host).map(|e| e.kind.clone()) else {
                return Ok(());
            };
            let look = (end - start).normalize_or_zero().perp();
            let d = w.direction();
            if d.perp_dot(look).abs() < 1e-6 {
                return Ok(()); // seen edge-on
            }
            // where the viewing ray through the click meets the wall's centreline
            let o = at(p);
            let t = (w.start - o).perp_dot(d) / look.perp_dot(d);
            let u = (o + look * t - w.start).dot(d);
            let base = doc.model().level(w.level).map_or(0.0, |l| l.elevation) + w.base_offset;
            let (kind, name) = match tool {
                Tool::Door => (OpeningKind::Door, "Door"),
                Tool::Opening => (OpeningKind::Hole, "Wall opening"),
                _ => (OpeningKind::Window, "Window"),
            };
            let mut e = state.typed(
                doc.model(),
                Element::new(
                    name,
                    ElementKind::Opening(Opening {
                        host,
                        kind,
                        offset: 0.0,
                        sill: 0.0,
                        width: if kind == OpeningKind::Door { 0.9 } else { 1.2 },
                        height: match kind {
                            OpeningKind::Door => 2.1,
                            OpeningKind::Window => 1.3,
                            OpeningKind::Hole => 1.0,
                        },
                        material: None,
                        frame: Default::default(),
                        flip_hand: false,
                        flip_facing: false,
                    }),
                ),
            );
            let ElementKind::Opening(op) = &mut e.kind else {
                return Ok(());
            };
            if op.width > w.length() {
                return Ok(());
            }
            // where it was clicked: along the wall, and up it but for doors
            op.offset = (u - op.width / 2.0).clamp(0.0, w.length() - op.width);
            if kind != OpeningKind::Door {
                op.sill =
                    (p.y - base - op.height / 2.0).clamp(0.0, (w.height - op.height).max(0.0));
            }
            selection.only(e.id);
            doc.commit(
                Transaction::new(format!("Place {} in section", name.to_lowercase())).insert(e),
            )
        }
        Tool::Mass => {
            let Some((level, elevation)) = level_below(doc, p.y) else {
                return Ok(());
            };
            let xy = at(p);
            let e = Element::new(
                "Mass",
                ElementKind::FreeMesh(FreeMesh::new(
                    Some(level),
                    DVec3::new(xy.x, xy.y, p.y - elevation),
                    EditMesh::cuboid(DVec3::new(2.0, 2.0, 3.0)),
                )),
            );
            selection.only(e.id);
            doc.commit(Transaction::new("Add mass").insert(e))
        }
        Tool::Slab => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) => {
                let (x0, x1) = (a.x.min(p.x), a.x.max(p.x));
                if x1 - x0 < 0.1 {
                    return Ok(());
                }
                // the top snaps to the nearest level; the drawn height is the thickness
                let top = a.y.max(p.y);
                let Some((level, _)) = doc
                    .model()
                    .levels()
                    .map(|(e, l)| (e.id, l.elevation))
                    .min_by(|x, y| (x.1 - top).abs().total_cmp(&(y.1 - top).abs()))
                else {
                    return Ok(());
                };
                let dy = (a.y - p.y).abs();
                let thickness = if dy > 0.05 { dy } else { 0.25 };
                let (d0, d1) = depth_of_walls(doc, level, start, end, x0, x1);
                let look = (end - start).normalize_or_zero().perp();
                let corner = |x: f64, d: f64| at(DVec2::new(x, 0.0)) + look * d;
                let mut boundary = vec![
                    corner(x0, d0),
                    corner(x1, d0),
                    corner(x1, d1),
                    corner(x0, d1),
                ];
                if signed_area(&boundary) < 0.0 {
                    boundary.reverse();
                }
                let e = Element::new(
                    "Slab",
                    ElementKind::Slab(Slab {
                        layer_edges: vec![],
                        level,
                        boundary,
                        thickness,
                        material: None,
                        top_offset: 0.0,
                        bulges: vec![],
                        assembly: None,
                        holes: vec![],
                    }),
                );
                // the slab type in use, unless its thickness was drawn
                let e = if dy > 0.05 {
                    e
                } else {
                    state.typed(doc.model(), e)
                };
                selection.only(e.id);
                doc.commit(Transaction::new("Draw slab in section").insert(e))
            }
        },
        Tool::Guide => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if a.distance(p) > 0.01 => {
                let e = Element::new(
                    "Guide",
                    ElementKind::Guide(creabim_core::Guide {
                        level: None,
                        a: at(a).extend(a.y),
                        b: at(p).extend(p.y),
                    }),
                );
                doc.commit(Transaction::new("Add guide").insert(e))
            }
            Some(_) => Ok(()),
        },
        Tool::Section | Tool::Elevation => match state.pending.take() {
            None => {
                state.pending = Some(p);
                Ok(())
            }
            Some(a) if (p.x - a.x).abs() > 0.05 => {
                // a new view square to this one, through the first click,
                // looking towards the side of the second
                let look = (end - start).normalize_or_zero().perp();
                let base = at(DVec2::new(a.x, 0.0));
                let (near, far) = model_depth(doc, start, end);
                let (s, e) = if p.x > a.x {
                    (base + look * far, base + look * near)
                } else {
                    (base + look * near, base + look * far)
                };
                add_section_view(doc, s, e, tool == Tool::Section, selection)
            }
            Some(_) => Ok(()),
        },
        _ => Ok(()),
    }
}

fn signed_area(p: &[DVec2]) -> f64 {
    (0..p.len())
        .map(|i| p[i].perp_dot(p[(i + 1) % p.len()]))
        .sum::<f64>()
        / 2.0
}

/// How far back from a section line the walls of `level` reach, within the
/// drawing range `x0..x1`: (nearest, furthest). A slab drawn in a section
/// runs back this far.
fn depth_of_walls(
    doc: &Document,
    level: ElementId,
    start: DVec2,
    end: DVec2,
    x0: f64,
    x1: f64,
) -> (f64, f64) {
    let along = (end - start).normalize_or_zero();
    let look = along.perp();
    let mut range: Option<(f64, f64)> = None;
    for e in doc.model().iter() {
        let ElementKind::Wall(w) = &e.kind else {
            continue;
        };
        if w.level != level {
            continue;
        }
        let pts = [w.start, w.end].map(|q| ((q - start).dot(along), (q - start).dot(look)));
        let (u0, u1) = (pts[0].0.min(pts[1].0), pts[0].0.max(pts[1].0));
        if u1 < x0 - 0.5 || u0 > x1 + 0.5 {
            continue;
        }
        for (_, d) in pts {
            range = Some(range.map_or((d, d), |(lo, hi)| (lo.min(d), hi.max(d))));
        }
    }
    match range {
        Some((lo, hi)) if hi - lo > 0.1 => (lo, hi),
        _ => (0.0, 5.0),
    }
}

/// The range of the model in front of a section line, with room to spare:
/// how long a section drawn across it should be.
fn model_depth(doc: &Document, start: DVec2, end: DVec2) -> (f64, f64) {
    let look = (end - start).normalize_or_zero().perp();
    let scene = (*creabim_geom::shared_scene(doc.model())).clone();
    let (lo, hi) = scene
        .iter()
        .flat_map(|s| s.mesh.positions.iter())
        .map(|p| (DVec2::new(p[0] as f64, p[1] as f64) - start).dot(look))
        .fold((f64::MAX, f64::MIN), |(a, b), d| (a.min(d), b.max(d)));
    if lo > hi {
        (-2.0, 10.0)
    } else {
        (lo - 2.0, hi + 2.0)
    }
}

/// Add a section or elevation view along `a` → `b`, named in sequence.
fn add_section_view(
    doc: &mut Document,
    a: DVec2,
    b: DVec2,
    cut: bool,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    let n = doc
        .model()
        .iter()
        .filter(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Section { cut: c, .. } if c == cut)))
        .count();
    let name = if cut {
        format!("Section {}", section_letter(n))
    } else {
        format!("Elevation {}", n + 1)
    };
    let e = Element::new(
        name.clone(),
        ElementKind::View(View::section(a, b, 30.0, cut)),
    );
    selection.only(e.id);
    doc.commit(Transaction::new(format!("Add {name}")).insert(e))
}

/// Preview of a section or elevation being placed in a section: a vertical
/// line at the first click with an arrow to the side it will look.
fn preview_cross_section(painter: &egui::Painter, xf: &Xf, a: DVec2, b: DVec2, accent: Color32) {
    let lo = xf.to_world(xf.rect.left_bottom());
    let hi = xf.to_world(xf.rect.right_top());
    let stroke = Stroke::new(1.5, accent);
    painter.line_segment(
        [
            xf.to_screen(DVec2::new(a.x, lo.y)),
            xf.to_screen(DVec2::new(a.x, hi.y)),
        ],
        stroke,
    );
    let side = if b.x > a.x { 1.0 } else { -1.0 };
    let tip = xf.to_screen(DVec2::new(a.x, b.y)) + Vec2::new(side * 24.0, 0.0);
    let root = xf.to_screen(DVec2::new(a.x, b.y));
    painter.arrow(root, tip - root, stroke);
}

/// From a pane's drawing point to the model: plans are level at `z`;
/// sections stand on their line.
fn pane_to_model(space: Space, w: DVec2, z: f64) -> DVec3 {
    match space {
        Space::Section { start, end } => {
            (start + (end - start).normalize_or_zero() * w.x).extend(w.y)
        }
        Space::Plan | Space::Model => w.extend(z),
    }
}

/// A part's size while Divide is open: small, on a paper chip.
/// None is drawn over one already in `taken`: zoomed out, sizes thin out
/// rather than pile up.
pub fn divide_label(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    ink: Color32,
    taken: &mut Vec<egui::Rect>,
) {
    let font = egui::FontId::proportional(11.0);
    let g = painter.layout_no_wrap(text.to_string(), font, ink);
    let r = egui::Rect::from_center_size(at, g.size() + egui::vec2(8.0, 4.0));
    if taken.iter().any(|t| t.expand(2.0).intersects(r)) {
        return;
    }
    taken.push(r);
    painter.rect(
        r,
        4.0,
        Color32::from_black_alpha(170),
        Stroke::new(1.0, ink.gamma_multiply(0.6)),
        egui::StrokeKind::Middle,
    );
    painter.galley(r.min + egui::vec2(4.0, 2.0), g, ink);
}

fn model_to_pane(space: Space, p: DVec3) -> DVec2 {
    match space {
        Space::Section { start, end } => DVec2::new(
            (p.truncate() - start).dot((end - start).normalize_or_zero()),
            p.z,
        ),
        Space::Plan | Space::Model => p.truncate(),
    }
}

/// The Move and Rotate tools, and G, R and S (move, rotate and scale) and
/// Shift+D (copy) while the pointer is over this pane. Returns `Some` while
/// one runs here. `pick` finds the element under the pointer, for
/// Shift+click while the base point is picked.
#[allow(clippy::too_many_arguments)]
fn keyboard_edits(
    ui: &egui::Ui,
    doc: &mut Document,
    view: ElementId,
    space: Space,
    painter: &egui::Painter,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &mut Selection,
    map: &PaneMap,
    pick: &dyn Fn(Pos2) -> Option<ElementId>,
) -> Option<Option<String>> {
    let shift = ui.input(|i| i.modifiers.shift);
    // the area that runs this view's move: the one that took it, else the
    // one under the pointer takes it now
    let mine = |m: &Modal| {
        m.pane == Some(view)
            && match m.owner {
                Some(o) => o == response.id,
                None => response.hovered(),
            }
    };
    // a tool's move still waiting for its base point follows the pointer
    // into another area (MU29: crossing a view on the way left it stuck
    // there): it is put back and armed again for this one
    let follows = state.modal.as_ref().is_some_and(|m| {
        m.flow == Flow::Tool && m.stage() == Stage::Base && response.hovered() && !mine(m)
    });
    if follows {
        if let Some(mut old) = state.modal.take() {
            let again = (old.mode, old.duplicated);
            old.discard(doc);
            state.armed = Some(again);
        }
    }
    if let Some(m) = state.modal.as_mut().filter(|m| mine(m)) {
        m.owner = Some(response.id);
        // Shift+click while the base point is picked adds or takes away
        if m.stage() == Stage::Base && shift && response.clicked() {
            if let Some(id) = response.interact_pointer_pos().and_then(pick) {
                selection.toggle(id);
                let grab = response.hover_pos().unwrap_or_default();
                state.status = Some(restart_modal(
                    doc,
                    &mut state.modal,
                    &selection.all(),
                    grab,
                    state.pivot,
                    state.cursor,
                ));
            }
            return Some(None);
        }
        m.panel(ui.ctx(), response.rect);
        let click = response.clicked();
        let right = response.secondary_clicked() && !shift;
        match m.update(
            doc,
            ui.ctx(),
            painter,
            response.hover_pos(),
            click,
            right,
            map,
        ) {
            Outcome::Running(s) => state.status = Some(s),
            Outcome::Done(s) | Outcome::Cancelled(s) => {
                state.status = Some(s);
                state.modal = None;
            }
        }
        if let Some(m) = &state.modal {
            state.move_target = m.works_on;
        }
        return Some(None);
    }
    if state.modal.is_some() || !response.hovered() || ui.ctx().egui_wants_keyboard_input() {
        return None;
    }
    selection.active?;
    let keys = start_keys(ui.ctx());
    let flow = if keys.is_some() {
        Flow::Keys
    } else {
        Flow::Tool
    };
    let mode = keys.or(state.armed)?;
    // Shift+D on views copies them at once, as the Duplicate button does
    if mode == (Mode::Move, true) {
        if let Some(r) = modal::duplicate_views(doc, &selection.all()) {
            state.armed = None;
            state.status = Some(match r {
                Ok(copies) => {
                    let said = modal::views_duplicated(doc, &copies);
                    selection.extend(copies, false);
                    said
                }
                Err(e) => e,
            });
            return Some(None);
        }
    }
    if keys.is_none() && only_paper(doc, &selection.all()) {
        return None;
    }
    let grab = response.hover_pos()?;
    state.armed = None;
    let extras = (state.armed_grip.take(), std::mem::take(&mut state.in_place));
    state.cancel();
    let ids = selection.all();
    match start_modal(
        doc,
        mode,
        &ids,
        None,
        space,
        Some(view),
        grab,
        state.pivot,
        state.cursor,
        flow,
        state.move_target,
        None,
    )
    .and_then(|(m, copies)| Ok((with_extras(doc, m, extras)?, copies)))
    {
        Ok((m, copies)) => {
            if copies != ids {
                selection.extend(copies, false);
            }
            state.status = Some(start_hint(&m));
            state.modal = Some(m);
        }
        Err(e) => state.status = Some(e),
    }
    Some(None)
}

/// Is everything selected written on a sheet (notes, marks)? Then only a
/// sheet runs its Move: a plan or the 3D view the pointer crosses on the
/// way must not take it (MU30).
pub fn only_paper(doc: &Document, ids: &[ElementId]) -> bool {
    let model = doc.model();
    let sheet = |v: ElementId| matches!(model.get(v).map(|e| &e.kind), Some(ElementKind::Sheet(_)));
    !ids.is_empty()
        && ids.iter().all(|id| match model.get(*id).map(|e| &e.kind) {
            Some(ElementKind::PaperMark(_)) => true,
            Some(ElementKind::Note(n)) => sheet(n.view),
            _ => false,
        })
}

/// What the edge palette asked of an armed edit: the grip [`Mode::Grip`]
/// drives, and whether an offset changes the original.
pub fn with_extras(
    doc: &mut Document,
    mut m: Modal,
    (grip, in_place): (Option<crate::handles::HandleKind>, bool),
) -> Result<Modal, String> {
    if let Some(k) = grip {
        m = m.with_grip(doc, k)?;
    }
    if in_place {
        m.set_copy(doc, false);
    }
    Ok(m)
}

/// What to do first, as a running edit begins.
pub fn start_hint(m: &Modal) -> String {
    if m.duplicated && m.mode == Mode::Move {
        return match m.stage() {
            Stage::Base => "Copy made: click a base point, then where it goes. Typed distances work; Esc or a right-click takes the copy away.".into(),
            _ => "Copy made: move the pointer and click where it goes. Esc or a right-click takes the copy away.".into(),
        };
    }
    match (m.stage(), m.mode) {
        (Stage::Base, Mode::Rotate) => "Rotate: click the centre to turn about.".into(),
        (Stage::Base, _) => {
            "Move: click the base point, the point you move by. It snaps; Shift+click adds or takes away elements.".into()
        }
        (_, mode) => format!(
            "{mode:?}: move the pointer, or type a value. X, Y or Z keeps to an axis."
        ),
    }
}

/// Start the running Move or Rotate tool again on a new selection, as it
/// was changed with Shift+click before the base point. Returns a message.
pub fn restart_modal(
    doc: &mut Document,
    modal: &mut Option<Modal>,
    ids: &[ElementId],
    grab: Pos2,
    pivot: modal::Pivot,
    cursor: Option<DVec3>,
) -> String {
    let Some(old) = modal.take() else {
        return String::new();
    };
    let mut old = old;
    old.undo_edit(doc);
    if ids.is_empty() {
        return "Nothing selected: click what to move.".into();
    }
    match Modal::start(
        doc,
        old.mode,
        ids,
        None,
        old.space,
        old.pane,
        grab,
        pivot,
        cursor,
        old.plane(),
    ) {
        Ok(m) => {
            let n = ids.len();
            *modal = Some(m.with_flow(Flow::Tool).with_works_on(old.works_on));
            format!(
                "{n} selected. Shift+click adds or takes away more; click the base point to go on."
            )
        }
        Err(e) => e,
    }
}

/// Which keyboard edit the keys pressed this frame start, if any:
/// (mode, duplicate first).
pub fn start_keys(ctx: &egui::Context) -> Option<(Mode, bool)> {
    let sketching = crate::sketch::sketching(ctx);
    ctx.input(|i| {
        if i.modifiers.command || i.modifiers.alt {
            return None;
        }
        let shift = i.modifiers.shift;
        if shift && i.key_pressed(egui::Key::D) {
            Some((Mode::Move, true))
        } else if shift {
            None
        } else if i.key_pressed(egui::Key::G) {
            Some((Mode::Move, false))
        } else if i.key_pressed(egui::Key::R) && !sketching {
            Some((Mode::Rotate, false))
        } else if i.key_pressed(egui::Key::S) {
            Some((Mode::Scale, false))
        } else {
            None
        }
    })
}

/// Start a keyboard edit on the selection, copying it first for Shift+D.
/// Returns the edit and the elements it works on (the copies, if made).
#[allow(clippy::too_many_arguments)]
pub fn start_modal(
    doc: &mut Document,
    (mode, duplicate): (Mode, bool),
    ids: &[ElementId],
    face: Option<usize>,
    space: Space,
    pane: Option<ElementId>,
    grab: Pos2,
    pivot: modal::Pivot,
    cursor: Option<DVec3>,
    flow: Flow,
    works_on: modal::Target,
    plane: Option<creabim_core::Workplane>,
) -> Result<(Modal, Vec<ElementId>), String> {
    let targets = if duplicate {
        modal::duplicate(doc, ids)?
    } else {
        ids.to_vec()
    };
    // a selected mass face is what G moves; the Move tool picks its face
    // itself when it works on faces
    let face = face.filter(|_| !duplicate && flow == Flow::Keys);
    match Modal::start(
        doc, mode, &targets, face, space, pane, grab, pivot, cursor, plane,
    ) {
        Ok(mut m) => {
            m.duplicated = duplicate;
            // a mass's picked vertices in Edit mode are what moves (MU39)
            if let ([one], false) = (targets.as_slice(), duplicate) {
                if let Some(vs) = crate::handles::mesh_pick(*one) {
                    m.set_verts(doc, *one, vs);
                }
            }
            Ok((m.with_flow(flow).with_works_on(works_on), targets))
        }
        Err(e) => {
            if duplicate {
                doc.discard_last();
            }
            Err(e)
        }
    }
}

/// Drawing a sketch in a plan or section. Returns `Some` when it used the
/// frame's input.
#[allow(clippy::too_many_arguments)]
fn sketch_tool(
    ui: &egui::Ui,
    doc: &mut Document,
    view: ElementId,
    space: Space,
    xf: &Xf,
    painter: &egui::Painter,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &mut Selection,
    hover: Option<DVec3>,
    shift: bool,
    exact: bool,
) -> Option<Option<String>> {
    let project = |p: DVec3| Some(xf.to_screen(model_to_pane(space, p)));
    // a sketch being drawn in another pane is left alone
    let mine = state.sketch.as_ref().is_some_and(|d| d.pane == Some(view));
    let draft = state.sketch.as_mut().filter(|d| d.pane == Some(view));
    if let Some(hint) = crate::sketch::shape_controls(ui, response.rect, Some(view), draft) {
        state.status = Some(hint);
    }
    let tol = 10.0 / xf.scale as f64;
    let on_plane = match (&state.sketch, hover) {
        (Some(d), Some(m)) if mine => Some(d.snap(d.plane.to_plane(m), tol, shift, !exact)),
        _ => None,
    };
    if let (Some(d), true) = (&state.sketch, mine) {
        d.draw(painter, &project, on_plane);
    }
    if !mine && !response.clicked() {
        return None;
    }
    let (enter, escape) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });
    let finish = |doc: &mut Document, state: &mut PlanState, selection: &mut Selection, closed| {
        let d = state.sketch.take()?;
        match d.finish(doc, closed) {
            Ok(Some(id)) => {
                selection.only(id);
                state.status = Some(
                    "Sketch added. Press E to extrude it into a mass, or G to move it.".into(),
                );
                Some(None)
            }
            Ok(None) => Some(None),
            Err(e) => Some(Some(e.to_string())),
        }
    };
    if escape && state.sketch.is_some() {
        state.sketch = None;
        return Some(None);
    }
    if enter || (response.secondary_clicked() && state.sketch.is_some()) {
        return finish(doc, state, selection, false);
    }
    if !response.clicked() {
        return None;
    }
    let m = hover?;
    if !mine {
        state.sketch = None;
    }
    match &mut state.sketch {
        None => {
            let plane = match space {
                Space::Section { start, end } => Workplane {
                    origin: m,
                    x: (end - start).normalize_or_zero().extend(0.0),
                    y: DVec3::Z,
                },
                Space::Plan | Space::Model => Workplane {
                    origin: m,
                    ..Workplane::ground(m.z)
                },
            };
            let draft = SketchDraft::new(plane, Some(view));
            let shape = crate::sketch::shape(ui.ctx());
            state.status = Some(crate::sketch::hint(shape, Some(&draft)));
            state.sketch = Some(draft);
            Some(None)
        }
        Some(d) => match d.click(on_plane?) {
            Some(Click::Closed) => finish(doc, state, selection, true),
            Some(Click::Added) => {
                state.status = Some(crate::sketch::hint(d.shape, Some(d)));
                Some(None)
            }
            None => Some(None),
        },
    }
}

/// The element and the walls whose ends meet it: what a drag on it may
/// change, so it must not snap to itself.
pub fn joined_with(doc: &Document, id: ElementId) -> Vec<ElementId> {
    let mut out = vec![id];
    if let Some(ElementKind::Wall(w)) = doc.model().get(id).map(|e| &e.kind) {
        for e in doc.model().iter() {
            if let ElementKind::Wall(o) = &e.kind {
                let touch = [o.start, o.end]
                    .iter()
                    .any(|p| p.distance(w.start) < 1e-3 || p.distance(w.end) < 1e-3);
                if touch && e.id != id {
                    out.push(e.id);
                }
            }
        }
    }
    out
}

/// Rebuild a view's snaps when the model changed, but not during a drag,
/// which changes the model every frame.
fn refresh_snaps(
    state: &mut PlanState,
    model: &creabim_core::Model,
    view: ElementId,
    plan: &Drawing,
    exclude: &[ElementId],
) {
    let revision = model.revision();
    let dragging = state.grip.is_some()
        || state
            .modal
            .as_ref()
            .is_some_and(|m| m.pane == Some(view) && m.freezes_snaps());
    // built once per model change with everything in it; what is left out
    // (the selection) is only hidden, so selecting builds nothing (MU39)
    let fresh = state
        .snaps
        .get(&view)
        .is_some_and(|(r, _, _)| *r == revision || dragging);
    if !fresh {
        let _p = crate::app::Perf::new("plan snaps: build");
        let mut s = Snapper::from_drawing(&plan.items, &[]);
        s.add_underlay_centres(model, view);
        add_reference_snaps(&mut s, state, model, view);
        state.snaps.insert(view, (revision, vec![], s));
    }
    if let Some((_, ex, s)) = state.snaps.get_mut(&view) {
        if ex != exclude {
            *ex = exclude.to_vec();
            s.hide(exclude);
        }
    }
}

/// The lines of the references shown in `view` (another view's linework,
/// a DXF drawing) snap as an underlay's do: ends, middles and along them
/// (MU32). A picture has no lines to snap to.
fn add_reference_snaps(
    s: &mut Snapper,
    state: &mut PlanState,
    model: &creabim_core::Model,
    view: ElementId,
) {
    // a huge drawing is cut short rather than slowing every snap
    const MOST: usize = 20_000;
    let mut n = 0;
    for e in model.iter() {
        let ElementKind::Reference(r) = &e.kind else {
            continue;
        };
        if r.hidden || !r.shown_in.contains(&view) {
            continue;
        }
        let crate::trace::Shown::Lines(lines) = state.ref_cache.get(model, e.id, r) else {
            continue;
        };
        for l in lines.iter() {
            for w in l.windows(2) {
                if n >= MOST {
                    return;
                }
                s.add_segment(
                    r.place(w[0]).extend(0.0),
                    r.place(w[1]).extend(0.0),
                    crate::snaps::Source::Underlay,
                    Some(e.id),
                );
                n += 1;
            }
        }
    }
}

/// The floor plans' ranges in a section (MU34 A1, MU36): a band per plan
/// from its view depth up to its cut, as wide as the section's range
/// (its own ends, dragged out or in, kept per section); the cut a solid
/// line, the depth dashed. One plan is read at a time: the one dragged,
/// selected or under the pointer shows its band, name and grips; the
/// others a faint cut line. A line is dragged up and down by its grip,
/// or clicked to type its height. Returns `Some` while dragging.
#[allow(clippy::too_many_arguments)]
fn plan_ranges(
    doc: &mut Document,
    painter: &egui::Painter,
    xf: &Xf,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &Selection,
    grips: bool,
    section: ElementId,
    drawing: &Drawing,
) -> Option<Option<String>> {
    let model = doc.model();
    let mut seen = std::collections::HashSet::new();
    let mut ranges: Vec<(ElementId, f64, f64, f64, String)> = vec![];
    for e in model.iter() {
        let ElementKind::View(v) = &e.kind else {
            continue;
        };
        let ViewKind::Plan { level, cut_height } = v.kind else {
            continue;
        };
        if v.ceiling_plan || !seen.insert(level) {
            continue;
        }
        let Some(l) = model.level(level) else {
            continue;
        };
        let below = model
            .levels()
            .map(|(_, x)| x.elevation)
            .filter(|z| *z < l.elevation - 1e-6)
            .fold(None, |m: Option<f64>, z| Some(m.map_or(z, |m| m.max(z))));
        let depth = v.graphics.depth(l.elevation, below);
        ranges.push((
            e.id,
            l.elevation,
            l.elevation + cut_height,
            l.elevation - depth,
            e.name.clone(),
        ));
    }
    let accent = crate::theme::interaction();
    // the band's ends, in the drawing: its own, else the drawing's width
    let (ex0, ex1) = *state.range_ends.entry(section).or_insert_with(|| {
        drawing
            .bounds()
            .map_or((-5.0, 5.0), |(lo, hi)| (lo.x - 1.0, hi.x + 1.0))
    });
    let sx = |x: f64| xf.to_screen(DVec2::new(x, 0.0)).x;
    let y_of = |z: f64| xf.to_screen(DVec2::new(0.0, z)).y;
    let (x0, x1) = (sx(ex0), sx(ex1));
    let pointer = response.hover_pos();
    let dragged = state.range_drag.map(|d| d.0);
    let near_line = pointer
        .filter(|p| p.x >= x0 - 8.0 && p.x <= x1 + 8.0)
        .and_then(|p| {
            ranges
                .iter()
                .flat_map(|r| {
                    [
                        (r.0, (y_of(r.2) - p.y).abs()),
                        (r.0, (y_of(r.3) - p.y).abs() + 2.0),
                    ]
                })
                .filter(|(_, d)| *d < 8.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id)
        });
    // the last one read stays only while the pointer is inside its band
    // or by its end handles; else none is lit (MU36: quiet until asked)
    let kept = state.range_focus.filter(|f| {
        state.range_edit.is_some()
            || pointer.is_some_and(|p| {
                ranges.iter().find(|r| r.0 == *f).is_some_and(|r| {
                    let (a, b) = (y_of(r.2), y_of(r.3));
                    p.y >= a.min(b) - 10.0
                        && p.y <= a.max(b) + 10.0
                        && p.x >= x0 - 14.0
                        && p.x <= x1 + 14.0
                })
            })
    });
    let focus = dragged
        .or_else(|| ranges.iter().find(|r| selection.contains(r.0)).map(|r| r.0))
        .or(near_line)
        .or(kept);
    state.range_focus = focus;
    let mut over: Option<(ElementId, RangeGrip, f64)> = None;
    let mut order: Vec<&(ElementId, f64, f64, f64, String)> = ranges.iter().collect();
    order.sort_by_key(|r| Some(r.0) == focus);
    let font = egui::FontId::proportional(11.0);
    for (id, z, cut, deep, name) in order {
        let on = Some(*id) == focus;
        let (yc, yd) = (y_of(*cut), y_of(*deep));
        let ink = if on {
            accent
        } else {
            accent.gamma_multiply(0.45)
        };
        if on {
            let band = Rect::from_min_max(egui::pos2(x0, yc.min(yd)), egui::pos2(x1, yc.max(yd)));
            painter.rect_filled(band, 0.0, accent.gamma_multiply(0.12));
            painter.add(egui::Shape::dashed_line(
                &[egui::pos2(x0, yd), egui::pos2(x1, yd)],
                Stroke::new(1.2, accent),
                5.0,
                3.0,
            ));
            painter.text(
                egui::pos2(x1 - 6.0, yd + 3.0),
                egui::Align2::RIGHT_TOP,
                format!("view depth {}", crate::units::len(z - deep)),
                font.clone(),
                accent,
            );
            painter.text(
                egui::pos2(x1 - 6.0, yc - 3.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("{name} · cut {}", crate::units::len(cut - z)),
                font.clone(),
                accent,
            );
        }
        painter.line_segment(
            [egui::pos2(x0, yc), egui::pos2(x1, yc)],
            Stroke::new(if on { 1.8 } else { 1.0 }, ink),
        );
        if !grips {
            continue;
        }
        let mut gs = vec![(yc, RangeGrip::Cut)];
        if on {
            gs.push((yd, RangeGrip::Depth));
        }
        for (y, g) in gs {
            let at = egui::pos2(x1, y);
            let hot = pointer.is_some_and(|p| p.distance(at) < 7.0);
            if on || hot {
                painter.circle(
                    at,
                    if hot { 5.5 } else { 4.5 },
                    Color32::WHITE,
                    Stroke::new(1.5, ink),
                );
            }
            if hot {
                over = Some((*id, g, *z));
            }
        }
    }
    // the band's own ends, dragged out or in like a section's
    if grips && focus.is_some() {
        let (ya, yb) = ranges
            .iter()
            .find(|r| Some(r.0) == focus)
            .map_or((0.0, 0.0), |r| (y_of(r.2), y_of(r.3)));
        let ym = (ya + yb) / 2.0;
        for (x, g) in [(x0, RangeGrip::Left), (x1, RangeGrip::Right)] {
            let at = egui::pos2(x, ym);
            let r = Rect::from_center_size(at, Vec2::new(7.0, 14.0));
            let hot = pointer.is_some_and(|p| r.expand(3.0).contains(p));
            painter.rect(
                r,
                2.0,
                if hot { accent } else { Color32::WHITE },
                Stroke::new(1.4, accent),
                egui::StrokeKind::Middle,
            );
            if hot {
                over = Some((focus.unwrap_or(section), g, 0.0));
            }
        }
    }
    if over.is_some() {
        painter.ctx().set_cursor_icon(match over.map(|o| o.1) {
            Some(RangeGrip::Left | RangeGrip::Right) => egui::CursorIcon::ResizeHorizontal,
            _ => egui::CursorIcon::ResizeVertical,
        });
    }
    // a click on a line: its height typed (MU36: precise values)
    // a cut line, or either grip; a depth line alone lies on the level
    // below's line, which a click there selects
    if response.clicked() && state.range_drag.is_none() {
        let hit = match over {
            Some((id, RangeGrip::Cut, _)) => Some((id, true)),
            Some((id, RangeGrip::Depth, _)) => Some((id, false)),
            _ => pointer.filter(|p| p.x >= x0 && p.x <= x1).and_then(|p| {
                ranges
                    .iter()
                    .find(|r| (y_of(r.2) - p.y).abs() < 5.0)
                    .map(|r| (r.0, true))
            }),
        };
        if let (Some((f, is_cut)), Some(p)) = (hit, pointer) {
            let z = ranges.iter().find(|r| r.0 == f).map_or(0.0, |r| r.1);
            state.range_edit = Some((f, is_cut, z, p));
            return Some(None);
        }
    }
    if let Some((id, is_cut, z, at)) = state.range_edit {
        let cur = ranges
            .iter()
            .find(|r| r.0 == id)
            .map(|r| if is_cut { r.2 - r.1 } else { r.1 - r.3 });
        let mut v = cur.unwrap_or(0.0);
        let mut close = false;
        egui::Area::new(egui::Id::new(("range.edit", section)))
            .order(egui::Order::Foreground)
            .fixed_pos(at + Vec2::new(12.0, -14.0))
            .show(painter.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(if is_cut { "Cut height" } else { "View depth" });
                        let r = ui.add(crate::units::drag(&mut v).range(0.0..=100.0));
                        r.request_focus();
                        if ui.input(|i| {
                            i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape)
                        }) {
                            close = true;
                        }
                    });
                });
            });
        if cur.is_some_and(|c| (c - v).abs() > 1e-9) {
            let r = doc.edit_merged(
                id,
                if is_cut {
                    "Plan cut height"
                } else {
                    "Plan view depth"
                },
                |e| {
                    if let ElementKind::View(view) = &mut e.kind {
                        if is_cut {
                            if let ViewKind::Plan { cut_height, .. } = &mut view.kind {
                                *cut_height = v.max(0.1);
                            }
                        } else {
                            view.graphics.range.view_depth = Some(v.max(0.0));
                        }
                    }
                },
            );
            let _ = z;
            if let Err(e) = r {
                return Some(Some(e.to_string()));
            }
        }
        // a click elsewhere, Enter or Esc closes it
        if close || (response.clicked() && near_line.is_none()) {
            state.range_edit = None;
        }
    }
    if response.drag_started_by(egui::PointerButton::Primary) && over.is_some() {
        state.range_drag = over;
        state.range_edit = None;
        state.drag_owner = Some(response.id);
    }
    let (id, g, z) = state.range_drag?;
    if state.drag_owner != Some(response.id) {
        return None;
    }
    if response.drag_stopped() || !response.dragged() {
        state.range_drag = None;
        return Some(None);
    }
    let w = xf.to_world(response.interact_pointer_pos()?);
    match g {
        RangeGrip::Left | RangeGrip::Right => {
            let ends = state.range_ends.entry(section).or_insert((ex0, ex1));
            if g == RangeGrip::Left {
                ends.0 = w.x.min(ends.1 - 0.5);
            } else {
                ends.1 = w.x.max(ends.0 + 0.5);
            }
            Some(None)
        }
        RangeGrip::Cut | RangeGrip::Depth => {
            let y = (w.y * 100.0).round() / 100.0;
            let is_cut = g == RangeGrip::Cut;
            let r = doc.edit_merged(
                id,
                if is_cut {
                    "Plan cut height"
                } else {
                    "Plan view depth"
                },
                |e| {
                    if let ElementKind::View(v) = &mut e.kind {
                        if is_cut {
                            if let ViewKind::Plan { cut_height, .. } = &mut v.kind {
                                *cut_height = (y - z).max(0.1);
                            }
                        } else {
                            v.graphics.range.view_depth = Some((z - y).max(0.0));
                        }
                    }
                },
            );
            state.status = Some(if is_cut {
                format!("Cut height {}", crate::units::len((y - z).max(0.1)))
            } else {
                format!("View depth {}", crate::units::len((z - y).max(0.0)))
            });
            Some(r.err().map(|e| e.to_string()))
        }
    }
}

/// What a grip of a plan's range in a section moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RangeGrip {
    Cut,
    Depth,
    Left,
    Right,
}

/// Handles on a selected section or elevation line. Returns `Some` while a
/// handle is being dragged (the pane skips its other input then).
fn drag_section_line(
    doc: &mut Document,
    painter: &egui::Painter,
    xf: &Xf,
    response: &egui::Response,
    state: &mut PlanState,
    selection: Option<ElementId>,
) -> Option<Option<String>> {
    let id = selection?;
    // a detail view's box has grips of its own, not its parent's line
    let Some(ElementKind::View(View {
        kind: ViewKind::Section { start, end, .. },
        callout: None,
        ..
    })) = doc.model().get(id).map(|e| &e.kind)
    else {
        state.drag = None;
        return None;
    };
    let (start, end) = (*start, *end);
    let handles = [
        (Handle::Start, start),
        (Handle::End, end),
        (Handle::Line, (start + end) / 2.0),
    ];
    let accent = crate::theme::interaction();
    for (h, p) in handles {
        let c = xf.to_screen(p);
        if h == Handle::Line {
            painter.circle(c, 5.0, Color32::WHITE, Stroke::new(1.5, accent));
        } else {
            painter.rect(
                Rect::from_center_size(c, Vec2::splat(9.0)),
                0.0,
                Color32::WHITE,
                Stroke::new(1.5, accent),
                egui::StrokeKind::Middle,
            );
        }
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        // where the button went down, not where the drag was recognised
        let p = painter
            .ctx()
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())?;
        state.drag = handles
            .iter()
            .find(|(_, q)| xf.to_screen(*q).distance(p) < 10.0)
            .map(|(h, _)| (*h, xf.to_world(p), start, end));
        state.drag_owner = Some(response.id);
    }
    if state.drag.is_some() && state.drag_owner != Some(response.id) {
        return None;
    }
    let (h, grab, s0, e0) = state.drag?;
    if response.drag_stopped() || !response.dragged() {
        state.drag = None;
        return Some(None);
    }
    let p = xf.to_world(response.interact_pointer_pos()?);
    let snap = |v: DVec2| (v / 0.05).round() * 0.05;
    let (s1, e1) = match h {
        Handle::Start => (snap(p), e0),
        Handle::End => (s0, snap(p)),
        Handle::Line => {
            let d = snap(p - grab);
            (s0 + d, e0 + d)
        }
    };
    if s1.distance(e1) < 0.2 {
        return Some(None);
    }
    let r = doc.edit_merged(id, "Move section line", |e| {
        if let ElementKind::View(View {
            kind: ViewKind::Section { start, end, .. },
            ..
        }) = &mut e.kind
        {
            (*start, *end) = (s1, e1);
        }
    });
    Some(r.err().map(|e| e.to_string()))
}

/// This frame's wheel input, split as touchpads expect (MU24, MU25): a
/// two-finger scroll pans, returned as screen points the view follows; a
/// mouse wheel and a pinch zoom, returned as a zoom amount in the units of
/// `smooth_scroll_delta.y`.
///
/// Windows reports a precision touchpad's scroll in lines, as a wheel's,
/// so the unit alone can't tell them apart: a wheel notch is a whole line
/// straight up or down, a touchpad sends fractions of lines and sideways
/// steps. Once a gesture shows itself as a touchpad's, the rest of it
/// (within 0.4 s) pans too.
pub(crate) fn wheel(ui: &egui::Ui) -> (egui::Vec2, f32) {
    let id = egui::Id::new("touchpad-gesture");
    let now = ui.input(|i| i.time);
    let last: f64 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(-10.0);
    let mut seen = None;
    let out = ui.input(|i| {
        let mut pan = egui::Vec2::ZERO;
        let mut zoom = 0.0;
        for e in &i.raw.events {
            if let egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = e
            {
                if modifiers.ctrl || modifiers.command {
                    // a pinch and Ctrl+wheel are in zoom_delta already
                    continue;
                }
                let whole = |v: f32| (v - v.round()).abs() < 1e-3;
                let touchpad = match unit {
                    egui::MouseWheelUnit::Point => true,
                    egui::MouseWheelUnit::Line => {
                        delta.x.abs() > 1e-3 || !whole(delta.y) || now - last < 0.4
                    }
                    egui::MouseWheelUnit::Page => false,
                };
                if touchpad {
                    seen = Some(now);
                    pan += match unit {
                        egui::MouseWheelUnit::Point => *delta,
                        _ => *delta * 40.0,
                    };
                } else {
                    zoom += match unit {
                        egui::MouseWheelUnit::Page => delta.y * 400.0,
                        _ => delta.y * 40.0,
                    };
                }
            }
        }
        let pinch = i.zoom_delta();
        if (pinch - 1.0).abs() > 1e-4 {
            zoom += (pinch - 1.0) * 500.0;
        }
        (pan, zoom)
    });
    if let Some(t) = seen {
        if t - last > 0.4 {
            crate::actlog::line("wheel: a touchpad scroll, panning");
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, t));
    }
    out
}

/// Centre and zoom that fit a drawing in the pane.
/// The column of zoom and pan buttons at a drawing area's top right; the
/// camera it asks for, if any.
pub(crate) fn nav_column(
    ui: &egui::Ui,
    rect: Rect,
    view: ElementId,
    fit_to: impl Fn() -> (DVec2, f32),
    fit_sel: Option<(DVec2, f32)>,
    center: DVec2,
    scale: f32,
) -> Option<(DVec2, f32)> {
    let (out, look) = nav_input(ui, rect, view, fit_to, fit_sel, center, scale);
    if let Some(look) = look {
        nav_draw(ui, &look);
    }
    out
}

/// How the zoom column looks this frame, to draw it once what lies
/// under it is drawn (a sheet's paper, MU25).
pub(crate) struct NavLook {
    rect: Rect,
    column: Rect,
    fade: f32,
    buttons: Vec<(Rect, &'static str, bool)>,
}

/// The zoom column's clicks and drags: the camera asked for, and how to
/// draw it with [`nav_draw`].
pub(crate) fn nav_input(
    ui: &egui::Ui,
    rect: Rect,
    view: ElementId,
    fit_to: impl Fn() -> (DVec2, f32),
    fit_sel: Option<(DVec2, f32)>,
    center: DVec2,
    scale: f32,
) -> (Option<(DVec2, f32)>, Option<NavLook>) {
    let size = 28.0;
    let x = rect.right() - size - 10.0;
    let y0 = rect.top() + 10.0;
    // a short area cuts the column off at its edge rather than taking it
    // away (MU40, as Blender's)
    if rect.width() < 200.0 || rect.height() < size + 20.0 {
        return (None, None);
    }
    let column = Rect::from_min_size(
        egui::pos2(x - 3.0, y0 - 3.0),
        egui::vec2(size + 6.0, 5.0 * size + 6.0),
    )
    .intersect(rect);
    let near = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|h| column.expand(40.0).contains(h));
    let fade = if near { 1.0 } else { 0.45 };
    let mut out = None;
    let mut look = NavLook {
        rect,
        column,
        fade,
        buttons: vec![],
    };
    let buttons = [
        ("zoom-in", "Zoom in: click a step, or drag up and down"),
        ("zoom-out", "Zoom out: click a step, or drag up and down"),
        ("frame-all", "Fit the drawing"),
        ("frame-selection", "Frame the selection"),
        ("hand", "Drag to pan"),
    ];
    for (i, (icon, tip)) in buttons.iter().enumerate() {
        let r = Rect::from_min_size(egui::pos2(x, y0 + i as f32 * size), egui::vec2(size, size));
        // one cut off by a short area is not there to click
        if r.bottom() > rect.bottom() {
            continue;
        }
        let resp = ui
            .interact(
                r,
                ui.id().with(("nav2d", view, i)),
                egui::Sense::click_and_drag(),
            )
            .on_hover_text(*tip);
        look.buttons.push((r, icon, resp.hovered()));
        let zoom = |by: f32| {
            // plans count points per metre, sheets per millimetre (MU25)
            let k = (scale * by).clamp(0.05, 5000.0);
            Some((center, k))
        };
        match i {
            // a click steps; a drag up or down zooms smoothly, as the 3D
            // view's zoom button does (MU20)
            0 | 1 if resp.dragged() => {
                let d = resp.drag_delta().y;
                out = zoom((1.0 - d * 0.01).clamp(0.5, 2.0));
            }
            0 if resp.clicked() => out = zoom(1.25),
            1 if resp.clicked() => out = zoom(1.0 / 1.25),
            2 if resp.clicked() => out = Some(fit_to()),
            3 if resp.clicked() => out = fit_sel,
            4 if resp.dragged() => {
                let d = resp.drag_delta() / scale;
                out = Some((center - DVec2::new(d.x as f64, -d.y as f64), scale));
            }
            _ => {}
        }
    }
    (out, Some(look))
}

/// Draw the zoom column [`nav_input`] read.
pub(crate) fn nav_draw(ui: &egui::Ui, look: &NavLook) {
    let p = crate::theme::Palette::of(ui.ctx());
    let fade = look.fade;
    // on the pane's own layer, so windows and pop-ups cover it (MU23);
    // dark paper is told to leave it alone, as it turns the drawing over
    // but never the controls (MU20)
    let painter = ui.painter().clone().with_clip_rect(look.rect);
    let first = painter.add(egui::Shape::Noop);
    painter.rect(
        look.column,
        8.0,
        // all but solid, faded or not: the drawing's lines no longer run
        // over its buttons (MU39); only its border and icons fade
        p.panel.gamma_multiply(0.94),
        egui::Stroke::new(1.0, p.border.gamma_multiply(fade)),
        egui::StrokeKind::Inside,
    );
    for (r, icon, hovered) in &look.buttons {
        if *hovered {
            painter.rect_filled(r.shrink(2.0), 6.0, p.accent.gamma_multiply(0.15));
        }
        crate::svgicon::paint(&painter, r.shrink(6.0), icon, p.text.gamma_multiply(fade));
    }
    let last = painter.add(egui::Shape::Noop);
    keep_colours(ui.ctx(), painter.layer_id(), first, last);
}

/// Shapes `first..=last` on `layer` keep their colours when dark paper
/// turns the drawing over (the zoom column's), this frame.
pub(crate) fn keep_colours(
    ctx: &egui::Context,
    layer: egui::LayerId,
    first: egui::layers::ShapeIdx,
    last: egui::layers::ShapeIdx,
) {
    let id = egui::Id::new("paper.keep");
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let list = d.get_temp_mut_or_default::<(u64, Vec<(egui::LayerId, usize, usize)>)>(id);
        if list.0 != pass {
            *list = (pass, vec![]);
        }
        list.1.push((layer, first.0, last.0));
    });
}

/// The shapes on `layer` dark paper leaves alone this frame.
pub(crate) fn kept_colours(ctx: &egui::Context, layer: egui::LayerId) -> Vec<(usize, usize)> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| {
        d.get_temp::<(u64, Vec<(egui::LayerId, usize, usize)>)>(egui::Id::new("paper.keep"))
    })
    .filter(|l| l.0 == pass)
    .map(|l| {
        l.1.into_iter()
            .filter(|r| r.0 == layer)
            .map(|r| (r.1, r.2))
            .collect()
    })
    .unwrap_or_default()
}

/// Each 3D crop box as a rectangle on `view`'s drawing: its footprint
/// in a plan, its extent across a section.
fn crop_rects(model: &creabim_core::Model, view: ElementId) -> Vec<(ElementId, DVec2, DVec2)> {
    let Some(frame) = creabim_core::DrawingFrame::of(model, view) else {
        return vec![];
    };
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::CropBox(c) => {
                let mut lo = DVec2::splat(f64::MAX);
                let mut hi = DVec2::splat(f64::MIN);
                for i in 0..8 {
                    let p = DVec3::new(
                        if i & 1 == 0 { c.min.x } else { c.max.x },
                        if i & 2 == 0 { c.min.y } else { c.max.y },
                        if i & 4 == 0 { c.min.z } else { c.max.z },
                    );
                    let d = frame.to_drawing(p);
                    lo = lo.min(d);
                    hi = hi.max(d);
                }
                Some((e.id, lo, hi))
            }
            _ => None,
        })
        .collect()
}

fn fit(d: &Drawing, rect: Rect) -> (DVec2, f32) {
    match d.bounds() {
        Some((lo, hi)) => {
            let size = (hi - lo).max(DVec2::splat(1.0));
            let scale = (rect.width() as f64 / size.x).min(rect.height() as f64 / size.y) * 0.85;
            ((lo + hi) / 2.0, scale.clamp(2.0, 2000.0) as f32)
        }
        None => (DVec2::ZERO, 45.0),
    }
}

/// A, B, … Z, AA, AB, …
fn section_letter(n: usize) -> String {
    let mut n = n;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break s;
        }
        n = n / 26 - 1;
    }
}

static PLAN_GRID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static PLAN_AXES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// The settings' grid behind drawings and X/Y lines in plans, for this
/// frame.
pub fn set_plan_look(grid: bool, axes: bool) {
    use std::sync::atomic::Ordering::Relaxed;
    PLAN_GRID.store(grid, Relaxed);
    PLAN_AXES.store(axes, Relaxed);
}

fn plan_look() -> (bool, bool) {
    use std::sync::atomic::Ordering::Relaxed;
    (PLAN_GRID.load(Relaxed), PLAN_AXES.load(Relaxed))
}

/// The model's X axis (red) and Y axis (green) through the origin, toned
/// well down so they place the building without competing with it.
fn draw_origin_axes(painter: &egui::Painter, xf: &Xf) {
    let r = xf.rect;
    let o = xf.to_screen(DVec2::ZERO);
    if (r.top()..=r.bottom()).contains(&o.y) {
        painter.line_segment(
            [egui::pos2(r.left(), o.y), egui::pos2(r.right(), o.y)],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(214, 72, 72, 70)),
        );
    }
    if (r.left()..=r.right()).contains(&o.x) {
        painter.line_segment(
            [egui::pos2(o.x, r.top()), egui::pos2(o.x, r.bottom())],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(70, 160, 80, 70)),
        );
    }
}

fn draw_metric_grid(painter: &egui::Painter, xf: &Xf) {
    let step = if xf.scale > 30.0 {
        1.0
    } else if xf.scale > 6.0 {
        5.0
    } else {
        25.0
    };
    let lo = xf.to_world(xf.rect.left_bottom());
    let hi = xf.to_world(xf.rect.right_top());
    let stroke = Stroke::new(1.0, Color32::from_gray(236));
    let mut x = (lo.x / step).floor() * step;
    while x <= hi.x {
        painter.line_segment(
            [
                xf.to_screen(DVec2::new(x, lo.y)),
                xf.to_screen(DVec2::new(x, hi.y)),
            ],
            stroke,
        );
        x += step;
    }
    let mut y = (lo.y / step).floor() * step;
    while y <= hi.y {
        painter.line_segment(
            [
                xf.to_screen(DVec2::new(lo.x, y)),
                xf.to_screen(DVec2::new(hi.x, y)),
            ],
            stroke,
        );
        y += step;
    }
}

/// Draw one drawing item. `mm` is drawing units per paper millimetre, which
/// sizes hatching and annotations.
/// A view's graphic overrides for one element, as the working view draws
/// them (MU27: they only showed on sheets and in PDF).
#[derive(Clone, Copy, Debug)]
pub struct Ovr {
    /// 0 drawn fully, 1 gone: halftone and transparency.
    pub fade: f32,
    pub colour: Option<Color32>,
    pub hatch_off: bool,
    /// Projection and cut pens, as factors of the view's own.
    pub proj: f32,
    pub cut: f32,
    pub dashed: bool,
}

impl Default for Ovr {
    fn default() -> Self {
        Self {
            fade: 0.0,
            colour: None,
            hatch_off: false,
            proj: 1.0,
            cut: 1.0,
            dashed: false,
        }
    }
}

/// The overrides view `view` sets, element by element: its categories'
/// first, then its own elements'. Empty when it sets none.
pub fn view_overrides(model: &creabim_core::Model, view: ElementId) -> HashMap<ElementId, Ovr> {
    use creabim_core::styles::Target;
    let mut out = HashMap::new();
    let Some(ElementKind::View(v)) = model.get(view).map(|e| &e.kind) else {
        return out;
    };
    if v.look.overrides.is_empty() {
        return out;
    }
    let w = creabim_core::styles::view_weights(model, v);
    let cutters = creabim_core::visibility::cutters(model);
    for e in model.iter() {
        let Some(cat) = creabim_core::visibility::Category::of(e, &cutters) else {
            continue;
        };
        let mut list: Vec<&creabim_core::styles::Override> = v
            .look
            .overrides
            .iter()
            .filter(|o| match o.target {
                Target::Category(c) => c == cat,
                Target::Element(id) => id == e.id,
            })
            .collect();
        if list.is_empty() {
            continue;
        }
        list.sort_by_key(|o| matches!(o.target, Target::Element(_)));
        let mut r = Ovr::default();
        for o in list {
            if let Some(x) = o.weight {
                r.proj = (x.mm(&w) / w.projection.max(1e-6)) as f32;
            }
            if let Some(x) = o.cut_weight {
                r.cut = (x.mm(&w) / w.cut.max(1e-6)) as f32;
            }
            if let Some(c) = o.colour {
                let [a, b, d] = c.0;
                r.colour = Some(Color32::from_rgb(
                    (a * 255.0) as u8,
                    (b * 255.0) as u8,
                    (d * 255.0) as u8,
                ));
            }
            if let Some(pat) = &o.pattern {
                r.dashed = !matches!(pat, creabim_core::styles::LinePattern::Solid);
            }
            if o.halftone {
                r.fade = 1.0 - (1.0 - r.fade) * 0.5;
            }
            if let Some(t) = o.transparency {
                r.fade = 1.0 - (1.0 - r.fade) * (1.0 - t);
            }
            if let Some(h) = o.hatch {
                r.hatch_off = !h;
            }
        }
        r.proj = r.proj.clamp(0.2, 6.0);
        r.cut = r.cut.clamp(0.2, 6.0);
        out.insert(e.id, r);
    }
    out
}

pub fn draw_item(painter: &egui::Painter, xf: &Xf, item: &PlanItem, selected: bool, mm: f64) {
    draw_item_with(painter, xf, item, selected, mm, &Ovr::default());
}

/// [`draw_item`] with a view's overrides for the element.
pub fn draw_item_with(
    painter: &egui::Painter,
    xf: &Xf,
    item: &PlanItem,
    selected: bool,
    mm: f64,
    o: &Ovr,
) {
    let highlight = crate::theme::interaction();
    let ink = if selected {
        highlight
    } else {
        o.colour.unwrap_or(Color32::BLACK)
    };
    let grey = |g: u8| o.colour.unwrap_or(Color32::from_gray(g));
    let hatch = paper::HATCH_SPACING * mm;
    // annotation text is a fixed size on paper, so it scales with zoom
    let text_px = |size: f64| (size * mm * xf.scale as f64) as f32;
    match item {
        PlanItem::Fill {
            points,
            hatch: pattern,
            rgb,
            ..
        } => {
            let gray = (paper::hatch_fill(*pattern) * 255.0) as u8;
            let fill = match (selected, rgb) {
                (true, _) => Color32::from_rgb(250, 205, 160),
                // Colour and hatch: the material's colour under it (MU32)
                (false, Some([r, g, b])) => {
                    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
                }
                (false, None) => Color32::from_gray(gray),
            };
            fill_polygon(painter, xf, points, fill);
            // skip hatching once it would be too dense to read, or when
            // the view turns it off
            if hatch * xf.scale as f64 > 3.0 && !o.hatch_off {
                let stroke = Stroke::new(0.6, grey(70));
                for (a, b) in creabim_geom::plan::hatch_lines(*pattern, points, hatch) {
                    painter.line_segment([xf.to_screen(a), xf.to_screen(b)], stroke);
                }
            }
        }
        PlanItem::Polygon { pen, points, .. } => {
            let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
            match pen {
                Pen::Cut => {
                    let fill = if selected {
                        Color32::from_rgb(250, 205, 160)
                    } else {
                        Color32::from_gray(150)
                    };
                    // any outline, a curved wall's too (MU40): filled as a
                    // mesh, then its edge
                    if pts.len() > 4 {
                        fill_polygon(painter, xf, points, fill);
                        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.6 * o.cut, ink)));
                    } else {
                        painter.add(egui::Shape::convex_polygon(
                            pts,
                            fill,
                            Stroke::new(1.6 * o.cut, ink),
                        ));
                    }
                }
                _ => {
                    let (w, c) = if selected {
                        (2.0, highlight)
                    } else {
                        (1.0 * o.proj, grey(120))
                    };
                    painter.add(egui::Shape::closed_line(pts, Stroke::new(w, c)));
                }
            }
        }
        PlanItem::Line { pen, a, b, .. } => {
            let (a, b) = (xf.to_screen(*a), xf.to_screen(*b));
            match pen {
                Pen::Guide => {
                    if let Some((a, b)) = clip_to_rect(a, b, painter.clip_rect()) {
                        let (w, c) = if selected {
                            (2.0, highlight)
                        } else {
                            (1.0, GUIDE_COLOR)
                        };
                        painter.extend(egui::Shape::dashed_line(
                            &[a, b],
                            Stroke::new(w, c),
                            6.0,
                            4.0,
                        ));
                    }
                }
                Pen::Grid => {
                    let (w, c) = if selected {
                        (2.0, highlight)
                    } else {
                        (1.0, Color32::from_rgb(120, 140, 170))
                    };
                    painter.extend(egui::Shape::dashed_line(
                        &[a, b],
                        Stroke::new(w, c),
                        12.0,
                        4.0,
                    ));
                }
                Pen::Underlay => {
                    painter.line_segment([a, b], Stroke::new(1.0, UNDERLAY_COLOR));
                }
                Pen::Projection => {
                    let (w, c) = if selected {
                        (2.0, highlight)
                    } else {
                        (1.0 * o.proj, grey(120))
                    };
                    if o.dashed {
                        painter.extend(egui::Shape::dashed_line(
                            &[a, b],
                            Stroke::new(w, c),
                            6.0,
                            4.0,
                        ));
                    } else {
                        painter.line_segment([a, b], Stroke::new(w, c));
                    }
                }
                Pen::Cut => {
                    if o.dashed {
                        painter.extend(egui::Shape::dashed_line(
                            &[a, b],
                            Stroke::new(1.8 * o.cut, ink),
                            6.0,
                            4.0,
                        ));
                    } else {
                        painter.line_segment([a, b], Stroke::new(1.8 * o.cut, ink));
                    }
                }
                Pen::Hidden => {
                    let (w, c) = if selected {
                        (2.0, highlight)
                    } else {
                        (1.0, Color32::from_gray(110))
                    };
                    painter.extend(egui::Shape::dashed_line(
                        &[a, b],
                        Stroke::new(w, c),
                        6.0,
                        4.0,
                    ));
                }
                _ => {
                    painter.line_segment([a, b], Stroke::new(1.0, ink));
                }
            }
        }
        PlanItem::Arc {
            center,
            radius,
            start,
            end,
            pen,
            ..
        } => {
            let ink = if *pen == Pen::Underlay {
                UNDERLAY_COLOR
            } else {
                ink
            };
            let n = 24;
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let t = start + (end - start) * i as f64 / n as f64;
                    xf.to_screen(*center + DVec2::from_angle(t) * *radius)
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.0, ink)));
        }
        PlanItem::Label { at, text, .. } => {
            let p = xf.to_screen(*at);
            let blue = Color32::from_rgb(120, 140, 170);
            // short tags (grids, section marks) sit in a bubble; longer
            // labels such as level datums are plain text
            if creabim_geom::plan::is_bubble(text) {
                let r = 10.0 * creabim_geom::plan::bubble_scale(text) as f32;
                painter.circle(p, r, Color32::WHITE, Stroke::new(1.0, blue));
            }
            painter.text(
                p,
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(12.0),
                blue,
            );
        }
        PlanItem::Text {
            at,
            text,
            size,
            align,
            rgb,
            bold,
            ..
        } => {
            let px = text_px(*size);
            if px >= 4.0 {
                let align = match align {
                    paper::Anchor::Start => egui::Align2::LEFT_CENTER,
                    paper::Anchor::Middle => egui::Align2::CENTER_CENTER,
                    paper::Anchor::End => egui::Align2::RIGHT_CENTER,
                };
                let colour = match rgb {
                    Some([r, g, b]) => {
                        Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
                    }
                    None => ink,
                };
                let at = xf.to_screen(*at);
                let font = egui::FontId::proportional(px);
                painter.text(at, align, text, font.clone(), colour);
                if *bold {
                    // a second pass a hair to the right reads as semibold
                    painter.text(at + Vec2::new(px / 28.0, 0.0), align, text, font, colour);
                }
            }
        }
        PlanItem::Dimension {
            a,
            b,
            offset,
            text,
            look,
            witness,
            breaks,
            ..
        } => {
            let ink = crate::annotate::ink(look, selected, highlight);
            let d = (*a, *b, *offset, *witness);
            draw_dimension(painter, xf, d, breaks, text, look, mm, ink);
        }
        PlanItem::Annotation { item, .. } => {
            let ink = crate::annotate::ink(&item.look, selected, highlight);
            crate::annotate::paint(painter, xf, item, ink, mm);
        }
        PlanItem::Tag {
            anchor,
            at,
            elbow,
            text,
            round,
            look,
            ..
        } => {
            let g = tag_geometry(*anchor, *at, *elbow, text, *round, look, mm);
            let pts: Vec<Pos2> = g.outline.iter().map(|p| xf.to_screen(*p)).collect();
            let ink = crate::annotate::ink(look, selected, highlight);
            if !g.leader.is_empty() {
                let line: Vec<Pos2> = g.leader.iter().map(|p| xf.to_screen(*p)).collect();
                painter.add(egui::Shape::line(line, Stroke::new(1.0, ink)));
                crate::annotate::paint_ends(painter, xf, &g.ends, ink);
            }
            if g.closed {
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    Color32::WHITE,
                    Stroke::new(1.2, ink),
                ));
            } else if pts.len() > 1 {
                painter.add(egui::Shape::line(pts, Stroke::new(1.2, ink)));
            }
            let px = crate::annotate::text_px(look, mm, xf);
            crate::annotate::text(
                painter,
                xf.to_screen(*at),
                text,
                px,
                ink,
                0.0,
                paper::Anchor::Middle,
                look,
            );
        }
        PlanItem::Tint { points, rgb, .. } => {
            let c = if selected { [0.98, 0.70, 0.45] } else { *rgb };
            let a = (paper::TINT_ALPHA * 255.0) as u8;
            let fill = crate::annotate::color32(c).gamma_multiply_u8(a);
            fill_polygon(painter, xf, points, fill);
        }
        // a room shows only in its view's colours, and when it is selected
        PlanItem::Room { points, rgb, .. } => {
            if let Some(rgb) = rgb {
                let a = (paper::TINT_ALPHA * 255.0) as u8;
                let fill = crate::annotate::color32(*rgb).gamma_multiply_u8(a);
                fill_polygon(painter, xf, points, fill);
            }
            if selected {
                let wash = Color32::from_rgb(250, 178, 115).gamma_multiply(0.25);
                fill_polygon(painter, xf, points, wash);
                let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
                painter.add(egui::Shape::closed_line(
                    pts,
                    Stroke::new(1.5, Color32::from_rgb(230, 120, 40)),
                ));
            }
        }
        PlanItem::Shade { points, rgb, .. } => {
            let c = if selected { [0.98, 0.85, 0.70] } else { *rgb };
            fill_polygon(painter, xf, points, crate::annotate::color32(c));
        }
        PlanItem::Image {
            raster,
            lo,
            hi,
            clip,
            ..
        } => {
            let rect = Rect::from_two_pos(xf.to_screen(*lo), xf.to_screen(*hi));
            let clip = clip.map(|(a, b)| Rect::from_two_pos(xf.to_screen(a), xf.to_screen(b)));
            crate::textures::paint(painter, rect, clip, raster);
        }
    }
}

/// An item drawn in X-ray: what holds others (walls, slabs, roofs,
/// masses) faint and see-through, and what is hidden inside them drawn
/// solid instead of dashed, as if nothing were in front.
fn draw_item_xray(
    painter: &egui::Painter,
    xf: &Xf,
    item: &PlanItem,
    selected: bool,
    mm: f64,
    model: &creabim_core::Model,
) {
    let host = || model.get(item.id()).is_some_and(|e| is_xray_host(&e.kind));
    match item {
        PlanItem::Fill { points, .. } if !selected && host() => {
            fill_polygon(
                painter,
                xf,
                points,
                Color32::from_gray(150).gamma_multiply(0.18),
            );
        }
        PlanItem::Polygon {
            pen: Pen::Cut,
            points,
            ..
        } if !selected && host() => {
            // filled as it is: a curved wall's band is not convex
            fill_polygon(
                painter,
                xf,
                points,
                Color32::from_gray(150).gamma_multiply(0.18),
            );
            let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
            painter.add(egui::Shape::closed_line(
                pts,
                Stroke::new(1.2, Color32::from_gray(90)),
            ));
        }
        PlanItem::Line {
            pen: Pen::Cut,
            a,
            b,
            ..
        } if !selected && host() => {
            painter.line_segment(
                [xf.to_screen(*a), xf.to_screen(*b)],
                Stroke::new(1.2, Color32::from_gray(90)),
            );
        }
        PlanItem::Line {
            pen: Pen::Hidden,
            a,
            b,
            ..
        } => {
            let c = if selected {
                crate::theme::interaction()
            } else {
                Color32::from_gray(25)
            };
            painter.line_segment([xf.to_screen(*a), xf.to_screen(*b)], Stroke::new(1.5, c));
        }
        _ => draw_item(painter, xf, item, selected, mm),
    }
}

/// Footings over a plan of `level`, dashed with a light tint in the
/// accent: all of the level's while the Footing tool is on (`working`),
/// and any selected one at all times, even below the view's range.
pub fn draw_footings(
    painter: &egui::Painter,
    xf: &Xf,
    model: &creabim_core::Model,
    level: ElementId,
    working: bool,
    selection: &Selection,
) {
    let accent = accent_selected();
    let strips = creabim_core::footing::strip_outlines(model);
    for e in model.iter() {
        let ElementKind::Footing(f) = &e.kind else {
            continue;
        };
        let selected = selection.contains(e.id);
        if !(selected || working && f.plan_level(model) == level) {
            continue;
        }
        let outlines = match f.kind {
            creabim_core::FootingKind::Strip => strips.get(&e.id).cloned().unwrap_or_default(),
            creabim_core::FootingKind::Pad => vec![f.pad_outline(model)],
        };
        let tint = accent.gamma_multiply(if selected { 0.22 } else { 0.12 });
        let stroke = Stroke::new(if selected { 1.8 } else { 1.2 }, accent);
        for o in outlines.iter().filter(|o| o.len() > 2) {
            fill_polygon(painter, xf, o, tint);
            let mut pts: Vec<Pos2> = o.iter().map(|p| xf.to_screen(*p)).collect();
            pts.push(pts[0]);
            painter.extend(egui::Shape::dashed_line(&pts, stroke, 6.0, 4.0));
        }
    }
}

/// The selection's colour in a pane: what selected linework is drawn in.
pub fn accent_selected() -> Color32 {
    crate::theme::interaction()
}

/// How much heavier a traced mark is than the mark itself (A3-5).
pub const TRACE: f32 = 1.5;

/// The pre-selection highlight of element `id`: its linework traced and
/// its fills washed in the hover colour, over the drawing. Marks, text,
/// tags and dimensions are traced themselves, never boxed.
pub fn draw_hover(painter: &egui::Painter, xf: &Xf, items: &[PlanItem], id: ElementId, mm: f64) {
    let line = Stroke::new(2.5, hoverpick::HOVER.gamma_multiply(0.75));
    let wash = hoverpick::HOVER.gamma_multiply(0.16);
    for it in items.iter().filter(|it| it.id() == id) {
        if is_mark(it) {
            trace(painter, xf, it, hoverpick::HOVER, mm);
            continue;
        }
        match it {
            PlanItem::Fill { points, .. }
            | PlanItem::Tint { points, .. }
            | PlanItem::Shade { points, .. } => {
                fill_polygon(painter, xf, points, wash);
            }
            PlanItem::Room { points, .. } => {
                fill_polygon(painter, xf, points, wash);
                let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
                painter.add(egui::Shape::closed_line(pts, line));
            }
            PlanItem::Polygon { points, .. } => {
                let pts: Vec<Pos2> = points.iter().map(|p| xf.to_screen(*p)).collect();
                painter.add(egui::Shape::closed_line(pts, line));
            }
            PlanItem::Line { pen, a, b, .. } => {
                let (a, b) = (xf.to_screen(*a), xf.to_screen(*b));
                // guides and grid lines run far: just the part in view
                if let Some((a, b)) = match pen {
                    Pen::Guide | Pen::Grid => clip_to_rect(a, b, painter.clip_rect()),
                    _ => Some((a, b)),
                } {
                    painter.line_segment([a, b], line);
                }
            }
            PlanItem::Arc {
                center,
                radius,
                start,
                end,
                ..
            } => {
                painter.add(egui::Shape::line(
                    arc_points(xf, *center, *radius, *start, *end),
                    line,
                ));
            }
            _ => {}
        }
    }
}

/// Is `item` part of a mark drawn on paper (a view mark's bubble, wedge
/// and line, a text, tag, label or dimension), which a hover or a
/// selection traces rather than washes?
pub fn is_mark(item: &PlanItem) -> bool {
    match item {
        PlanItem::Label { .. }
        | PlanItem::Text { .. }
        | PlanItem::Dimension { .. }
        | PlanItem::Tag { .. }
        | PlanItem::Annotation { .. } => true,
        PlanItem::Line { pen, .. } | PlanItem::Arc { pen, .. } => *pen == Pen::Symbol,
        // a view mark's solid wedge
        PlanItem::Shade { rgb, .. } => rgb.iter().sum::<f32>() < 1.0,
        _ => false,
    }
}

fn arc_points(xf: &Xf, center: DVec2, radius: f64, start: f64, end: f64) -> Vec<Pos2> {
    (0..=24)
        .map(|i| {
            let t = start + (end - start) * i as f64 / 24.0;
            xf.to_screen(center + DVec2::from_angle(t) * radius)
        })
        .collect()
}

/// One mark drawn over itself in `color`, its strokes [`TRACE`] times
/// their weight: what hovering it or selecting it shows.
pub fn trace(painter: &egui::Painter, xf: &Xf, item: &PlanItem, color: Color32, mm: f64) {
    let stroke = Stroke::new(TRACE, color);
    match item {
        PlanItem::Line { a, b, .. } => {
            painter.line_segment([xf.to_screen(*a), xf.to_screen(*b)], stroke);
        }
        PlanItem::Arc {
            center,
            radius,
            start,
            end,
            ..
        } => {
            painter.add(egui::Shape::line(
                arc_points(xf, *center, *radius, *start, *end),
                stroke,
            ));
        }
        PlanItem::Shade { points, .. } => fill_polygon(painter, xf, points, color),
        PlanItem::Label { at, text, .. } => {
            let p = xf.to_screen(*at);
            if creabim_geom::plan::is_bubble(text) {
                let r = 10.0 * creabim_geom::plan::bubble_scale(text) as f32;
                painter.circle_stroke(p, r, stroke);
            }
            painter.text(
                p,
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(12.0),
                color,
            );
        }
        PlanItem::Text {
            at,
            text,
            size,
            align,
            ..
        } => {
            let px = (size * mm * xf.scale as f64) as f32;
            if px >= 4.0 {
                let look = TextLook {
                    bold: true,
                    ..TextLook::default()
                };
                crate::annotate::text(
                    painter,
                    xf.to_screen(*at),
                    text,
                    px,
                    color,
                    0.0,
                    *align,
                    &look,
                );
            }
        }
        PlanItem::Dimension {
            a,
            b,
            offset,
            text,
            look,
            witness,
            breaks,
            ..
        } => {
            let d = (*a, *b, *offset, *witness);
            draw_dimension_weighted(painter, xf, d, breaks, text, look, mm, color, TRACE);
        }
        PlanItem::Annotation { item, .. } => {
            crate::annotate::paint_weighted(painter, xf, item, color, mm, TRACE);
        }
        PlanItem::Tag {
            anchor,
            at,
            elbow,
            text,
            round,
            look,
            ..
        } => {
            let g = tag_geometry(*anchor, *at, *elbow, text, *round, look, mm);
            if !g.leader.is_empty() {
                let line: Vec<Pos2> = g.leader.iter().map(|p| xf.to_screen(*p)).collect();
                painter.add(egui::Shape::line(line, stroke));
                crate::annotate::paint_ends_weighted(painter, xf, &g.ends, color, TRACE);
            }
            let pts: Vec<Pos2> = g.outline.iter().map(|p| xf.to_screen(*p)).collect();
            let heavy = Stroke::new(1.2 * TRACE, color);
            if g.closed {
                painter.add(egui::Shape::closed_line(pts, heavy));
            } else if pts.len() > 1 {
                painter.add(egui::Shape::line(pts, heavy));
            }
            let px = crate::annotate::text_px(look, mm, xf);
            crate::annotate::text(
                painter,
                xf.to_screen(*at),
                text,
                px,
                color,
                0.0,
                paper::Anchor::Middle,
                look,
            );
        }
        _ => {}
    }
}

/// A dimension: witness lines, the line with its ticks, and its value.
#[allow(clippy::too_many_arguments)]
fn draw_dimension(
    painter: &egui::Painter,
    xf: &Xf,
    (a, b, offset, witness): (DVec2, DVec2, f64, Option<[DVec2; 2]>),
    breaks: &[[f64; 2]],
    text: &str,
    look: &TextLook,
    mm: f64,
    ink: Color32,
) {
    draw_dimension_weighted(
        painter,
        xf,
        (a, b, offset, witness),
        breaks,
        text,
        look,
        mm,
        ink,
        1.0,
    );
}

/// A dimension with its strokes `w` times their weight.
#[allow(clippy::too_many_arguments)]
fn draw_dimension_weighted(
    painter: &egui::Painter,
    xf: &Xf,
    (a, b, offset, witness): (DVec2, DVec2, f64, Option<[DVec2; 2]>),
    breaks: &[[f64; 2]],
    text: &str,
    look: &TextLook,
    mm: f64,
    ink: Color32,
    w: f32,
) {
    let mut g = dimension_geometry(a, b, offset, witness, look, mm);
    g.break_line(breaks);
    for (p, q) in g.lines {
        painter.line_segment([xf.to_screen(p), xf.to_screen(q)], Stroke::new(w, ink));
    }
    crate::annotate::paint_ends_weighted(painter, xf, &g.ends, ink, w);
    let px = crate::annotate::text_px(look, mm, xf);
    crate::annotate::text(
        painter,
        xf.to_screen(g.text_at),
        text,
        px,
        ink,
        g.angle,
        paper::Anchor::Middle,
        look,
    );
}

/// A dimension being drawn, measuring along `axis`.
#[allow(clippy::too_many_arguments)]
fn preview_dimension(
    painter: &egui::Painter,
    xf: &Xf,
    a: DVec2,
    b: DVec2,
    axis: creabim_core::DimAxis,
    offset: f64,
    look: &TextLook,
    mm: f64,
    accent: Color32,
) {
    let Some((b2, witness)) = annotate::squared(axis, a, b) else {
        return;
    };
    let text = annotate::dimension_text(&crate::units::get(), a, b2);
    draw_dimension(
        painter,
        xf,
        (a, b2, offset, witness),
        &[],
        &text,
        look,
        mm,
        accent,
    );
}

/// The point a dimension click lands on, what it attaches to, and the snap
/// that found it. Wall corners and door or window edges come first, since
/// those keep the most useful attachment; then any snap in the pane's
/// drawing: corners, edges and centres of masses, slabs, columns, beams,
/// sketches, and grid and level lines. A point on an element follows it.
fn dim_snap(
    doc: &Document,
    view: &View,
    kind: PaneKind,
    snapper: Option<&Snapper>,
    cfg: &snaps::SnapSettings,
    xf: &Xf,
    cursor: Pos2,
) -> (HostPoint, DVec2, Option<SnapKind>) {
    let (hp, at, k, _) = dim_snap_guided(doc, view, kind, snapper, cfg, xf, cursor, &[], None, &[]);
    (hp, at, k)
}

/// The line offset of a dimension from `a` to `b` measuring along `axis`
/// whose line goes through `at`.
fn dim_offset(axis: creabim_core::DimAxis, a: DVec2, b: DVec2, at: DVec2) -> f64 {
    let u = axis.direction(a, b).unwrap_or((b - a).normalize_or_zero());
    (at - a).dot(u.perp())
}

/// [`dim_snap`] with temporary guides too: where they cross each other
/// and the drawing's edges. Also returns the guides the point is on.
#[allow(clippy::too_many_arguments)]
fn dim_snap_guided(
    doc: &Document,
    view: &View,
    kind: PaneKind,
    snapper: Option<&Snapper>,
    cfg: &snaps::SnapSettings,
    xf: &Xf,
    cursor: Pos2,
    guides: &[snaps::TempGuide],
    from: Option<DVec3>,
    track: &[(DVec3, snaps::Through)],
) -> (HostPoint, DVec2, Option<SnapKind>, Vec<usize>) {
    let model = doc.model();
    let p = xf.to_world(cursor);
    let tol = 10.0 / xf.scale as f64;
    let mut best: Option<(f64, ElementId, DVec2)> = None;
    let mut consider = |host: ElementId, q: DVec2| {
        let d = q.distance(p);
        if d < tol && best.is_none_or(|(bd, ..)| d < bd) {
            best = Some((d, host, q));
        }
    };
    if let (PaneKind::Plan { level }, true) = (kind, cfg.allows(SnapKind::Endpoint)) {
        let joins = creabim_core::wall_joins(model);
        for e in model.iter() {
            let ElementKind::Wall(w) = &e.kind else {
                continue;
            };
            if w.level != level {
                continue;
            }
            let ends = joins.get(&e.id).cloned().unwrap_or_default();
            for q in ends.footprint(w) {
                consider(e.id, q);
            }
            for q in [w.start, w.end] {
                consider(e.id, q);
            }
            let (d, n, t) = (w.direction(), w.direction().perp(), w.thickness / 2.0);
            for oid in model.dependents(e.id) {
                let Some(ElementKind::Opening(o)) = model.get(oid).map(|e| &e.kind) else {
                    continue;
                };
                for u in [o.offset, o.offset + o.width] {
                    for s in [-t, t] {
                        consider(
                            oid,
                            if w.is_arc() {
                                w.place(u, s)
                            } else {
                                w.start + d * u + n * s
                            },
                        );
                    }
                }
            }
        }
    }
    let project = |q: DVec3| Some(xf.to_screen(q.truncate()));
    let hit = snapper.and_then(|s| {
        // from the point clicked last, as the drawing tools snap from
        // theirs: on its axes, square to it, lined up with the points
        // picked (A7-11)
        s.snap_guided(
            cfg,
            cursor,
            &project,
            from,
            track,
            guides,
            Some(p.extend(0.0)),
        )
    });
    let (host, at, kind, used) = match (best, hit) {
        // a nearer corner of something else beats the wall's
        (Some((d, host, q)), h)
            if h.as_ref().is_none_or(|h| {
                !h.snap.kind.is_point() || h.snap.p.truncate().distance(p) >= d
            }) =>
        {
            (Some(host), q, Some(SnapKind::Endpoint), vec![])
        }
        (_, Some(h)) => (
            h.snap.host,
            h.snap.p.truncate(),
            Some(h.snap.kind),
            h.guides,
        ),
        (_, None) => (None, p, None, vec![]),
    };
    (annotate::attach(model, view, host, at), at, kind, used)
}

/// Keep a temporary guide line as a guide element: in a plan on its
/// level, in a section on the section's plane.
fn keep_guide(
    doc: &mut Document,
    kind: PaneKind,
    guide: Option<snaps::TempGuide>,
) -> Result<String, creabim_core::ModelError> {
    let Some(g) = guide else {
        return Ok("Point at a guide line first, then press K to keep it.".into());
    };
    let snaps::GuideShape::Line { p, d } = g.shape else {
        return Ok("Only straight guide lines can be kept; circles stay temporary.".into());
    };
    let (level, a, b) = match kind {
        PaneKind::Plan { level } => {
            let z = doc.model().level(level).map_or(0.0, |l| l.elevation);
            (Some(level), p.extend(z), (p + d).extend(z))
        }
        PaneKind::Section { start, end } => {
            let space = Space::Section { start, end };
            (
                None,
                pane_to_model(space, p, 0.0),
                pane_to_model(space, p + d, 0.0),
            )
        }
        _ => return Ok(String::new()),
    };
    let e = Element::new(
        format!("Guide ({})", g.kind.label().to_lowercase()),
        ElementKind::Guide(creabim_core::Guide { level, a, b }),
    );
    doc.commit(Transaction::new("Keep guide line").insert(e))?;
    Ok(format!(
        "Kept the {} guide line. It stays until you delete it.",
        g.kind.label().to_lowercase()
    ))
}

/// A click with the Dimension tool: first point, second point, then where
/// the line goes. Continuous: then each click adds the next on that line.
/// The line snaps onto another dimension's line within `tol`, and to the
/// spacing steps (MU23 board).
#[allow(clippy::too_many_arguments)]
fn dimension_tool(
    doc: &mut Document,
    view: ElementId,
    hit: (HostPoint, DVec2),
    raw: DVec2,
    axis: creabim_core::DimAxis,
    state: &mut PlanState,
    selection: &mut Selection,
    others: &[(DVec2, DVec2, f64)],
    tol: f64,
) -> Result<(), creabim_core::ModelError> {
    // a chain: the next point, at once, on the chain's line
    if let (Some((axis, on_line)), 1) = (state.chain_line, state.dim.len()) {
        let (a, pa) = state.dim[0];
        if hit.1.distance(pa) < 1e-3 {
            return Ok(());
        }
        let (b, pb) = hit;
        // the line stays where it is, whichever way the chain runs
        let offset = dim_offset(axis, pa, pb, on_line);
        let mut d = Dimension::linear(view, a, b, offset);
        d.axis = axis;
        let e = state
            .anno
            .styled(Element::new("Dimension", ElementKind::Dimension(d)));
        selection.only(e.id);
        state.dim = vec![hit];
        return doc.commit(Transaction::new("Add dimension").insert(e));
    }
    match state.dim.len() {
        0 => {
            state.dim.push(hit);
            Ok(())
        }
        1 => {
            if hit.1.distance(state.dim[0].1) > 1e-3 {
                state.dim.push(hit);
            }
            Ok(())
        }
        _ => {
            let (a, pa) = state.dim[0];
            let (b, pb) = state.dim[1];
            state.dim.clear();
            // a horizontal or vertical one needs its points apart that way
            let axis = if axis.direction(pa, pb).is_some() {
                axis
            } else {
                creabim_core::DimAxis::Aligned
            };
            let mut offset = dim_offset(axis, pa, pb, raw);
            let u = axis
                .direction(pa, pb)
                .unwrap_or((pb - pa).normalize_or_zero());
            let n = u.perp();
            // onto another dimension's line, parallel and near (MU23)
            let mine = pa.dot(n) + offset;
            let mut aligned = false;
            for (oa, ob, oo) in others {
                let ou = (*ob - *oa).normalize_or_zero();
                if ou.perp_dot(u).abs() > 1e-3 {
                    continue;
                }
                let theirs = oa.dot(n) + *oo * ou.perp().dot(n);
                if (theirs - mine).abs() < tol {
                    offset = theirs - pa.dot(n);
                    aligned = true;
                    break;
                }
            }
            // else on spacing steps from what it measures
            if !aligned && state.dim_step_mm > 0.0 {
                let scale = match doc.model().get(view).map(|e| &e.kind) {
                    Some(ElementKind::View(v)) => v.scale,
                    _ => 50.0,
                };
                let step = state.dim_step_mm / 1000.0 * scale;
                offset = (offset / step).round() * step;
                if offset.abs() < 1e-9 {
                    offset = step.copysign(dim_offset(axis, pa, pb, raw));
                }
            }
            let mut d = Dimension::linear(view, a, b, offset);
            d.axis = axis;
            let e = state
                .anno
                .styled(Element::new("Dimension", ElementKind::Dimension(d)));
            selection.only(e.id);
            if state.dim_chain {
                // the chain goes on from the second point, on this line
                state.chain_line = Some((axis, pa + n * offset));
                state.dim = vec![(b, pb)];
                state.status = Some(
                    "Continuous: click the next point; Esc or right-click ends the chain.".into(),
                );
            }
            doc.commit(Transaction::new("Add dimension").insert(e))
        }
    }
}

/// A click with the Tag tool: pick what to tag, then where the tag goes.
fn tag_tool(
    doc: &mut Document,
    view: ElementId,
    v: &Option<View>,
    picked: Option<ElementId>,
    raw: DVec2,
    state: &mut PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    let Some(v) = v else { return Ok(()) };
    match state.tag_host.take() {
        None => {
            let taggable = picked.filter(|id| is_taggable(doc.model(), *id));
            if picked.is_some() && taggable.is_none() {
                state.status = Some(
                    "That can't take a tag: pick a door, window, wall, slab, roof, column, beam, stair, railing, furniture or a fixture."
                        .into(),
                );
            }
            state.tag_host = taggable;
            Ok(())
        }
        Some(host) => {
            let (Some(proj), Some(c)) = (
                annotate::Projector::of(doc.model(), v),
                annotate::centre(doc.model(), host),
            ) else {
                return Ok(());
            };
            let mut t = Tag::new(view, host, raw - proj.project(c));
            t.material = state.anno.tag_material;
            let e = state.anno.styled(Element::new(
                if t.material { "Material tag" } else { "Tag" },
                ElementKind::Tag(t),
            ));
            selection.only(e.id);
            doc.commit(Transaction::new("Add tag").insert(e))
        }
    }
}

/// Can the Tag tool tag element `id`? Building elements can; views,
/// annotation and the like can't.
pub fn is_taggable(model: &creabim_core::Model, id: ElementId) -> bool {
    matches!(
        model.get(id).map(|e| &e.kind),
        Some(
            ElementKind::Opening(_)
                | ElementKind::Wall(_)
                | ElementKind::Slab(_)
                | ElementKind::FreeMesh(_)
                | ElementKind::Roof(_)
                | ElementKind::Column(_)
                | ElementKind::Beam(_)
                | ElementKind::Stair(_)
                | ElementKind::Railing(_)
                | ElementKind::Object(_)
                | ElementKind::Room(_)
                | ElementKind::Ceiling(_)
        )
    ) && annotate::centre(model, id).is_some()
}

/// Tag every services fixture (electrical, lighting, plumbing, heating and
/// ventilation) on a plan's level that has no tag there yet, with its
/// type's mark: in front of what stands against a wall, beside what stands
/// free or hangs from the ceiling. Furniture and sanitary ware are left
/// for the Tag tool, so a furnished plan doesn't drown in tags.
/// Returns how many were added.
pub fn tag_fixtures(
    doc: &mut Document,
    view: ElementId,
) -> Result<usize, creabim_core::ModelError> {
    let model = doc.model();
    let Some(ElementKind::View(v)) = model.get(view).map(|e| &e.kind) else {
        return Ok(0);
    };
    let ViewKind::Plan { level, .. } = v.kind else {
        return Ok(0);
    };
    let tagged: Vec<ElementId> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Tag(t) if t.view == view => Some(t.host),
            _ => None,
        })
        .collect();
    let scale = v.scale / 100.0;
    let mut tx = Transaction::new("Tag fixtures");
    let mut n = 0;
    for e in model.iter() {
        let ElementKind::Object(o) = &e.kind else {
            continue;
        };
        if o.level != level || tagged.contains(&e.id) {
            continue;
        }
        let Some(t) = creabim_core::objects::type_of(model, e) else {
            continue;
        };
        if !t.category.is_services() {
            continue;
        }
        let offset = match t.mount {
            creabim_core::objects::Mount::Wall => o.facing() * (t.size().y / 2.0 + 0.45 * scale),
            _ => {
                DVec2::new(1.0, 1.0).normalize()
                    * (t.size().truncate().length() / 2.0 + 0.3 * scale)
            }
        };
        tx = tx.insert(Element::new(
            "Tag",
            ElementKind::Tag(Tag::new(view, e.id, offset)),
        ));
        n += 1;
    }
    if n > 0 {
        doc.commit(tx)?;
    }
    Ok(n)
}

/// Tag every door and window in a view that has no tag there yet, just
/// outside its wall. Returns how many were added.
pub fn tag_openings(
    doc: &mut Document,
    view: ElementId,
) -> Result<usize, creabim_core::ModelError> {
    let model = doc.model();
    let Some(ElementKind::View(v)) = model.get(view).map(|e| &e.kind) else {
        return Ok(0);
    };
    let level = match v.kind {
        ViewKind::Plan { level, .. } => Some(level),
        _ => None,
    };
    let Some(proj) = annotate::Projector::of(model, v) else {
        return Ok(0);
    };
    let tagged: Vec<ElementId> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Tag(t) if t.view == view => Some(t.host),
            _ => None,
        })
        .collect();
    // the drawing's middle, to tell outside from inside
    let middle = creabim_geom::section::view_drawing(model, view)
        .and_then(|d| d.bounds())
        .map_or(DVec2::ZERO, |(lo, hi)| (lo + hi) / 2.0);
    let scale = v.scale / 100.0;
    let mut tx = Transaction::new("Tag doors and windows");
    let mut n = 0;
    for e in model.iter() {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        if tagged.contains(&e.id) {
            continue;
        }
        let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) else {
            continue;
        };
        if level.is_some_and(|l| l != w.level) {
            continue;
        }
        let Some(c) = annotate::centre(model, e.id) else {
            continue;
        };
        let anchor = proj.project(c);
        let offset = match level {
            Some(_) => {
                let n = w.direction().perp();
                let out = if (anchor - middle).dot(n) > 0.0 {
                    n
                } else {
                    -n
                };
                out * 0.9 * scale
            }
            // in sections and 3D views, above the opening
            None => DVec2::new(0.0, (o.height / 2.0 + 0.5) * scale.max(0.5)),
        };
        tx = tx.insert(Element::new(
            "Tag",
            ElementKind::Tag(Tag::new(view, e.id, offset)),
        ));
        n += 1;
    }
    doc.commit(tx)?;
    Ok(n)
}

/// Drag a tag (moves it) or a dimension (moves its line) in the Select
/// tool. Returns `Some` while a drag is in progress.
#[allow(clippy::too_many_arguments)]
fn drag_annotation(
    doc: &mut Document,
    view: ElementId,
    drawing: &Drawing,
    xf: &Xf,
    response: &egui::Response,
    state: &mut PlanState,
    selection: &mut Selection,
    mm: f64,
) -> Option<Option<String>> {
    if response.drag_started_by(egui::PointerButton::Primary) {
        let p = response
            .ctx
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())?;
        let w = xf.to_world(p);
        let hit = drawing.items.iter().rev().find_map(|it| match it {
            PlanItem::Tag { id, at, .. }
                if at.distance(w) < 3.0 * mm.max(6.0 / xf.scale as f64 / 3.0) =>
            {
                Some(*id)
            }
            PlanItem::Dimension {
                id, a, b, offset, ..
            } => {
                let n = (*b - *a).normalize_or_zero().perp() * *offset;
                (segment_distance(w, *a + n, *b + n) < 6.0 / xf.scale as f64).then_some(*id)
            }
            // a room's tag, drawn as its lines of text
            PlanItem::Annotation { id, item }
                if matches!(
                    doc.model().get(*id).map(|e| &e.kind),
                    Some(ElementKind::Tag(_))
                ) =>
            {
                let pts = item.extent();
                let first = *pts.first()?;
                let (lo, hi) = pts
                    .iter()
                    .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                let pad = DVec2::splat(mm);
                (w.cmpge(lo - pad).all() && w.cmple(hi + pad).all()).then_some(*id)
            }
            _ => None,
        });
        state.anno_drag = hit.and_then(|id| {
            let start = match &doc.model().get(id)?.kind {
                ElementKind::Tag(t) if t.view == view => t.offset,
                ElementKind::Dimension(d) if d.view == view => DVec2::new(d.offset, 0.0),
                _ => return None,
            };
            Some((id, w, start))
        });
        if let Some((id, ..)) = state.anno_drag {
            selection.only(id);
            state.drag_owner = Some(response.id);
        }
    }
    if state.anno_drag.is_some() && state.drag_owner != Some(response.id) {
        return None;
    }
    let (id, grab, start) = state.anno_drag?;
    if response.drag_stopped() || !response.dragged() {
        state.anno_drag = None;
        return Some(None);
    }
    let p = xf.to_world(response.interact_pointer_pos()?);
    let r = match doc.model().get(id).map(|e| e.kind.clone()) {
        Some(ElementKind::Tag(_)) => doc.edit_merged(id, "Move tag", |e| {
            if let ElementKind::Tag(t) = &mut e.kind {
                t.offset = start + p - grab;
            }
        }),
        Some(ElementKind::Dimension(_)) => {
            // move the line square to what it measures
            let item = drawing.items.iter().find_map(|it| match it {
                PlanItem::Dimension { id: i, a, b, .. } if *i == id => Some((*a, *b)),
                _ => None,
            });
            let Some((a, b)) = item else {
                return Some(None);
            };
            let n = (b - a).normalize_or_zero().perp();
            let offset = start.x + (p - grab).dot(n);
            doc.edit_merged(id, "Move dimension line", |e| {
                if let ElementKind::Dimension(d) = &mut e.kind {
                    d.offset = offset;
                }
            })
        }
        _ => Ok(()),
    };
    Some(r.err().map(|e| e.to_string()))
}

/// Snap to wall endpoints within 10 px, else to a 5 cm grid, as far as
/// `cfg` allows. With `ortho`, keep the segment from `from` horizontal or
/// vertical.
#[allow(clippy::too_many_arguments)]
fn snap(
    doc: &Document,
    level: ElementId,
    xf: &Xf,
    p: DVec2,
    from: Option<DVec2>,
    ortho: bool,
    exclude: &[ElementId],
    cfg: &snaps::SnapSettings,
) -> DVec2 {
    let mut p = p;
    if let (true, Some(a)) = (ortho, from) {
        let d = p - a;
        p = if d.x.abs() >= d.y.abs() {
            DVec2::new(p.x, a.y)
        } else {
            DVec2::new(a.x, p.y)
        };
    }
    let tol = 10.0 / xf.scale as f64;
    let mut best: Option<(f64, DVec2)> = None;
    for e in doc.model().iter() {
        if let ElementKind::Wall(w) = &e.kind {
            if w.level != level || exclude.contains(&e.id) || !cfg.allows(SnapKind::Endpoint) {
                continue;
            }
            for q in [w.start, w.end] {
                let d = q.distance(p);
                if d < tol && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, q));
                }
            }
        }
    }
    best.map(|(_, q)| q).unwrap_or_else(|| {
        if cfg.allows(SnapKind::Grid) {
            (p / 0.05).round() * 0.05
        } else {
            p
        }
    })
}

/// Fill a possibly concave polygon.
/// A slab's fill in its level's plan, under the pointer, and selected.
const SLAB_FILL: Color32 = Color32::from_rgba_unmultiplied_const(70, 110, 170, 30);
const SLAB_HOVER: Color32 = Color32::from_rgba_unmultiplied_const(60, 110, 200, 70);
const SLAB_SELECTED: Color32 = Color32::from_rgba_unmultiplied_const(240, 140, 50, 60);
/// The edge of the slab under the pointer.
const SLAB_EDGE: Color32 = Color32::from_rgb(60, 110, 200);

/// The slabs of `level` the plan's drawing shows in projection (not cut
/// through), with their outlines.
fn level_slabs(
    model: &creabim_core::Model,
    level: ElementId,
    plan: &Drawing,
) -> Vec<(ElementId, Vec<DVec2>)> {
    plan.items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Polygon {
                id,
                pen: Pen::Projection,
                points,
            } if matches!(
                model.get(*id).map(|e| &e.kind),
                Some(ElementKind::Slab(s)) if s.level == level
            ) =>
            {
                Some((*id, points.clone()))
            }
            _ => None,
        })
        .collect()
}

/// The slab whose outline holds `p`; the smallest when they overlap.
fn slab_at(slabs: &[(ElementId, Vec<DVec2>)], p: DVec2) -> Option<ElementId> {
    slabs
        .iter()
        .filter(|(_, pts)| point_in_polygon(p, pts))
        .min_by(|a, b| signed_area(&a.1).abs().total_cmp(&signed_area(&b.1).abs()))
        .map(|(id, _)| *id)
}

pub fn fill_polygon(painter: &egui::Painter, xf: &Xf, points: &[DVec2], fill: Color32) {
    let mut mesh = egui::Mesh::default();
    for p in points {
        mesh.colored_vertex(xf.to_screen(*p), fill);
    }
    for [a, b, c] in triangulate(points) {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The pivot cursor: a red and white ring with a cross, as in Blender.
pub fn draw_cursor(painter: &egui::Painter, at: Pos2) {
    let red = Color32::from_rgb(220, 50, 50);
    painter.circle_stroke(at, 7.0, Stroke::new(2.0, Color32::WHITE));
    painter.add(egui::Shape::dashed_line(
        &[
            at + Vec2::new(7.0, 0.0),
            at + Vec2::new(0.0, 7.0),
            at + Vec2::new(-7.0, 0.0),
            at + Vec2::new(0.0, -7.0),
            at + Vec2::new(7.0, 0.0),
        ],
        Stroke::new(2.0, red),
        4.0,
        4.0,
    ));
    for d in [Vec2::X, Vec2::Y, -Vec2::X, -Vec2::Y] {
        painter.line_segment(
            [at + d * 9.0, at + d * 14.0],
            Stroke::new(1.0, Color32::BLACK),
        );
    }
}

/// Can a box take this element? Views and levels are left to clicks, so a
/// box round the building doesn't pick up its section marks.
pub fn box_selectable(doc: &Document, id: ElementId) -> bool {
    doc.model().get(id).is_some_and(|e| {
        !matches!(
            e.kind,
            ElementKind::View(_)
                | ElementKind::Level(_)
                | ElementKind::Sheet(_)
                | ElementKind::Viewport(_)
        )
    })
}

pub fn hit_test(items: &[PlanItem], p: DVec2, tol: f64, mm: f64) -> Option<ElementId> {
    hit_all(items, p, tol, mm).first().copied()
}

/// Every element whose linework is at `p`, in the order a click takes
/// them: annotations, then symbols (doors and windows over the walls they
/// sit in), then fills (a tinted element over the wall it is inside),
/// then lines; each once.
pub fn hit_all(items: &[PlanItem], p: DVec2, tol: f64, mm: f64) -> Vec<ElementId> {
    let mut out: Vec<ElementId> = Vec::new();
    let mut found = |id: ElementId| {
        if !out.contains(&id) {
            out.push(id);
        }
    };
    // symbols (doors, windows) win over the walls they sit in
    let near_line = |a: DVec2, b: DVec2| segment_distance(p, a, b) < tol;
    // annotations sit on top of everything
    for it in items.iter().rev() {
        match it {
            PlanItem::Tag { id, at, .. } if p.distance(*at) < (3.0 * mm).max(tol) => found(*id),
            PlanItem::Annotation { id, item } if item.hit(p, tol, mm) => found(*id),
            PlanItem::Dimension {
                id, a, b, offset, ..
            } => {
                let n = (*b - *a).normalize_or_zero().perp() * *offset;
                let text = (*a + *b) / 2.0 + n;
                if near_line(*a + n, *b + n) || p.distance(text) < (3.0 * mm).max(tol) {
                    found(*id);
                }
            }
            PlanItem::Text {
                id,
                at,
                text,
                size,
                align,
                ..
            } if *id != ElementId::NIL => {
                // schedule rows: anywhere along the row's height
                let w = paper::text_width(text, *size) * mm;
                let x0 = match align {
                    paper::Anchor::Start => at.x,
                    paper::Anchor::Middle => at.x - w / 2.0,
                    paper::Anchor::End => at.x - w,
                };
                if (p.y - at.y).abs() < size * mm && p.x >= x0 - tol && p.x <= x0 + w + tol {
                    found(*id);
                }
            }
            _ => {}
        }
    }
    for it in items.iter().rev() {
        match it {
            PlanItem::Line {
                id,
                pen: Pen::Symbol,
                a,
                b,
            } if near_line(*a, *b) => found(*id),
            PlanItem::Arc {
                id,
                center,
                radius,
                pen,
                ..
            } if !matches!(pen, Pen::Underlay | Pen::Hidden)
                && (p.distance(*center) - radius).abs() < tol =>
            {
                found(*id)
            }
            // view marks and grid ends: their arrow or their bubble
            PlanItem::Polygon {
                id,
                pen: Pen::Symbol,
                points,
            } if point_in_polygon(p, points) => found(*id),
            PlanItem::Label { id, at, text }
                if text.chars().count() <= 3 && p.distance(*at) < tol * 1.2 =>
            {
                found(*id)
            }
            _ => {}
        }
    }
    for it in items.iter().rev() {
        match it {
            PlanItem::Fill { id, points, .. }
            | PlanItem::Tint { id, points, .. }
            | PlanItem::Shade { id, points, .. }
                if point_in_polygon(p, points) =>
            {
                found(*id)
            }
            _ => {}
        }
    }
    for it in items.iter().rev() {
        match it {
            PlanItem::Line { id, a, b, pen }
                if !matches!(pen, Pen::Underlay | Pen::Hidden) && near_line(*a, *b) =>
            {
                found(*id)
            }
            PlanItem::Polygon { id, points, pen } if *pen != Pen::Hidden => {
                let n = points.len();
                if (0..n).any(|i| near_line(points[i], points[(i + 1) % n])) {
                    found(*id);
                }
            }
            _ => {}
        }
    }
    // what is in the cut wins over what is drawn dashed below or above it:
    // a click in a door's opening takes the door, not the footing under it
    let mut boxes: Vec<(ElementId, DVec2, DVec2)> = vec![];
    for it in items {
        let (id, pts): (ElementId, Vec<DVec2>) = match it {
            PlanItem::Line {
                id,
                pen: Pen::Symbol,
                a,
                b,
            } => (*id, vec![*a, *b]),
            PlanItem::Arc {
                id,
                pen: Pen::Symbol,
                center,
                radius,
                start,
                end,
            } => (
                *id,
                (0..=8)
                    .map(|i| {
                        let t = start + (end - start) * i as f64 / 8.0;
                        *center + DVec2::from_angle(t) * *radius
                    })
                    .collect(),
            ),
            _ => continue,
        };
        let lo = pts.iter().fold(DVec2::splat(f64::MAX), |m, q| m.min(*q));
        let hi = pts.iter().fold(DVec2::splat(f64::MIN), |m, q| m.max(*q));
        match boxes.iter_mut().find(|b| b.0 == id) {
            Some(b) => {
                b.1 = b.1.min(lo);
                b.2 = b.2.max(hi);
            }
            None => boxes.push((id, lo, hi)),
        }
    }
    let below = |id: &ElementId| {
        items.iter().all(|it| match it {
            PlanItem::Line { id: i, pen, .. }
            | PlanItem::Arc { id: i, pen, .. }
            | PlanItem::Polygon { id: i, pen, .. } => i != id || *pen == Pen::Hidden,
            _ => true,
        })
    };
    let mut hidden: Vec<ElementId> = vec![];
    for it in items.iter().rev() {
        let hit = match it {
            PlanItem::Line {
                id,
                pen: Pen::Hidden,
                a,
                b,
            } if near_line(*a, *b) => Some(*id),
            PlanItem::Arc {
                id,
                pen: Pen::Hidden,
                center,
                radius,
                ..
            } if (p.distance(*center) - radius).abs() < tol => Some(*id),
            PlanItem::Polygon {
                id,
                pen: Pen::Hidden,
                points,
            } if {
                let n = points.len();
                (0..n).any(|i| near_line(points[i], points[(i + 1) % n]))
            } =>
            {
                Some(*id)
            }
            _ => None,
        };
        if let Some(id) = hit {
            if !hidden.contains(&id) {
                hidden.push(id);
            }
        }
    }
    if hidden.iter().any(below) {
        for (id, lo, hi) in &boxes {
            let (lo, hi) = (*lo - DVec2::splat(tol), *hi + DVec2::splat(tol));
            if p.cmpge(lo).all() && p.cmple(hi).all() {
                found(*id);
            }
        }
    }
    for id in hidden {
        found(id);
    }
    // rooms last: a click inside one that finds nothing else picks it
    for it in items.iter().rev() {
        if let PlanItem::Room { id, points, .. } = it {
            if point_in_polygon(p, points) {
                found(*id);
            }
        }
    }
    out
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

fn place_opening(
    doc: &mut Document,
    level: ElementId,
    p: DVec2,
    tool: Tool,
    state: &PlanState,
    selection: &mut Selection,
) -> Result<(), creabim_core::ModelError> {
    let mut best: Option<(f64, ElementId, f64, f64)> = None;
    // the wall the pointer showed wins (Tab may have moved past the first)
    let shown = state
        .host_pick
        .filter(|h| crate::memberstool::walls_at(doc.model(), level, p).contains(h));
    for e in doc.model().iter() {
        let ElementKind::Wall(w) = &e.kind else {
            continue;
        };
        if w.level != level || shown.is_some_and(|h| h != e.id) {
            continue;
        }
        // round the arc on a curved wall
        let (u, d) = w.along(p);
        if d <= w.thickness.max(0.3) && best.is_none_or(|b| d < b.0) {
            best = Some((d, e.id, u, w.length()));
        }
    }
    let Some((_, host, u, len)) = best else {
        return Ok(());
    };
    // the sizes in the bar (a type, when picked, sets its own)
    let m = &state.members;
    let (kind, [width, height, sill], name) = match tool {
        Tool::Door => (OpeningKind::Door, m.door, "Door"),
        Tool::Opening => (OpeningKind::Hole, m.hole, "Wall opening"),
        _ => (OpeningKind::Window, m.window, "Window"),
    };
    let mut e = state.typed(
        doc.model(),
        Element::new(
            name,
            ElementKind::Opening(Opening {
                host,
                kind,
                offset: 0.0,
                sill,
                width,
                height,
                material: None,
                frame: Default::default(),
                flip_hand: false,
                flip_facing: false,
            }),
        ),
    );
    // centred on the click, as wide as its type makes it
    if let ElementKind::Opening(o) = &mut e.kind {
        if o.width > len {
            return Ok(());
        }
        o.offset = (u - o.width / 2.0).clamp(0.0, len - o.width);
        // clear of a corner (MU23)
        creabim_core::walls::clamp_opening(doc.model(), o);
    }
    selection.only(e.id);
    doc.commit(Transaction::new(format!("Place {}", name.to_lowercase())).insert(e))
}

/// The pane behaviour for a view.
pub fn pane_kind(v: &View) -> PaneKind {
    match v.kind {
        ViewKind::Plan { level, .. } => PaneKind::Plan { level },
        ViewKind::Section { start, end, .. } => PaneKind::Section { start, end },
        ViewKind::Axonometric { .. } => PaneKind::Axonometric,
        ViewKind::Schedule { .. } => PaneKind::Schedule,
    }
}

/// The points of the shape being drawn in a view, in its drawing
/// coordinates: a slab outline, a sketch, a railing path or a chain of
/// walls. Alignment tracking lines the next point up with them.
fn shape_points(
    state: &PlanState,
    view: ElementId,
    kind: PaneKind,
    from_pane: &dyn Fn(DVec3) -> DVec2,
) -> Vec<DVec2> {
    let level = match kind {
        PaneKind::Plan { level } => Some(level),
        _ => None,
    };
    if let Some(o) = state.outline.as_ref().filter(|o| Some(o.level) == level) {
        return o.points.clone();
    }
    if let Some(d) = state.sketch.as_ref().filter(|d| d.pane == Some(view)) {
        return d
            .points
            .iter()
            .map(|p| from_pane(d.plane.to_model(*p)))
            .collect();
    }
    if level.is_some() && !state.members.path.is_empty() {
        return state.members.path.clone();
    }
    match (&state.chain, state.pending) {
        ((Some(l), pts), Some(_)) if Some(*l) == level => pts.clone(),
        _ => vec![],
    }
}

/// Rest on a point or edge for a moment (or press Q over it) and it becomes
/// a snap reference in this view, its guide lines showing; rest on one
/// again and it goes. Returns a status message when one is picked or
/// dropped.
/// An Align click is not a rest: the pointer left still on the edge it
/// clicked doesn't make guides there, so the message for the click stays.
fn click_is_not_a_rest(
    state: &mut PlanState,
    tool: Tool,
    clicked: bool,
    under: Option<snaps::Reference>,
    now: f64,
) {
    if let (Tool::Align, true, Some(r)) = (tool, clicked, under) {
        state.ref_rest = Some((r, now, true));
    }
}

fn rest_reference(
    state: &mut PlanState,
    view: ElementId,
    under: Option<snaps::Reference>,
    now: f64,
    key: bool,
) -> Option<String> {
    const DWELL: f64 = REST_DWELL;
    let Some(r) = under else {
        state.ref_rest = None;
        return None;
    };
    let toggle = match state.ref_rest {
        Some((q, since, done)) if q.same(&r) => {
            let due = !done && (key || now - since > DWELL);
            state.ref_rest = Some((q, since, done || due));
            due
        }
        _ => {
            state.ref_rest = Some((r, now, key));
            key
        }
    };
    if !toggle {
        return None;
    }
    let refs = state.refs.entry(view).or_default();
    let what = match r {
        snaps::Reference::Point { .. } => "point",
        snaps::Reference::Edge { arc: Some(_), .. } => "arc",
        snaps::Reference::Edge { .. } => "edge",
    };
    if let Some(i) = refs.iter().position(|q| q.same(&r)) {
        refs.remove(i);
        return Some(format!("Dropped the {what}'s guide lines."));
    }
    refs.push(r);
    if refs.len() > snaps::MAX_REFERENCES {
        refs.remove(0);
    }
    state.refs_used.insert(view, now);
    Some(format!(
        "Guide lines from this {what}: where they cross snaps. They go when not used for a few seconds; K keeps one."
    ))
}

/// How long the pointer rests on a point or edge before it becomes a
/// snap reference, as ArchiCAD's hover delay.
pub const REST_DWELL: f64 = 0.6;

/// Seconds a view's guide lines last with nothing using them.
pub const REF_FADE: f64 = 6.0;

/// Drop a view's references once nothing has used them for [`REF_FADE`].
fn fade_references(state: &mut PlanState, view: ElementId, now: f64, used: bool) {
    if state.refs.get(&view).is_none_or(|r| r.is_empty()) {
        state.refs_used.remove(&view);
        return;
    }
    let last = state.refs_used.entry(view).or_insert(now);
    if used {
        *last = now;
    } else if now - *last > REF_FADE {
        state.refs.remove(&view);
        state.refs_used.remove(&view);
    }
}

/// While drawing, a point snapped to and rested on for a moment joins the
/// points tracked (the last few of them), as in SketchUp.
fn rest_on(
    state: &mut PlanState,
    view: ElementId,
    snap: Option<snaps::Snap>,
    drawing: bool,
    now: f64,
) {
    const DWELL: f64 = 0.4;
    const KEEP: usize = 4;
    if !drawing {
        state.rested.1.clear();
        state.rest = None;
        return;
    }
    if state.rested.0 != Some(view) {
        state.rested = (Some(view), vec![]);
    }
    let Some(p) = snap.filter(|s| s.kind.is_point()).map(|s| s.p.truncate()) else {
        state.rest = None;
        return;
    };
    match state.rest {
        Some((q, since)) if q.distance(p) < 1e-9 => {
            let pts = &mut state.rested.1;
            if now - since > DWELL && !pts.iter().any(|r| r.distance(p) < 1e-9) {
                pts.push(p);
                if pts.len() > KEEP {
                    pts.remove(0);
                }
            }
        }
        _ => state.rest = Some((p, now)),
    }
}

#[cfg(test)]
mod dimension_chain_tests {
    use super::*;

    #[test]
    fn a_continuous_chain_stays_on_one_line_and_rows_snap_together() {
        // MU23 board: continuous dimensions, and lines onto other lines
        let mut doc = creabim_core::Document::from_model(creabim_core::starter::starter_house());
        let view = doc
            .model()
            .iter()
            .find(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, creabim_core::ViewKind::Plan { .. })))
            .unwrap()
            .id;
        let mut state = PlanState {
            dim_chain: true,
            ..Default::default()
        };
        let mut sel = Selection::default();
        let h = |x: f64| (HostPoint::free(DVec2::new(x, 0.0)), DVec2::new(x, 0.0));
        let ax = creabim_core::DimAxis::Horizontal;
        let mut click =
            |doc: &mut creabim_core::Document, state: &mut PlanState, x: f64, raw: DVec2| {
                dimension_tool(doc, view, h(x), raw, ax, state, &mut sel, &[], 0.1).unwrap();
            };
        click(&mut doc, &mut state, 0.0, DVec2::ZERO);
        click(&mut doc, &mut state, 2.0, DVec2::ZERO);
        click(&mut doc, &mut state, 1.0, DVec2::new(1.0, 1.5)); // the line, 1.5 up
        click(&mut doc, &mut state, 5.0, DVec2::ZERO); // the chain goes on
        click(&mut doc, &mut state, 6.5, DVec2::ZERO);
        let dims: Vec<(f64, f64)> = doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Dimension(d) if d.view == view => Some((d.a.local.x, d.offset)),
                _ => None,
            })
            .collect();
        // the three just made are the last ones
        let made: Vec<&(f64, f64)> = dims.iter().rev().take(3).collect();
        assert_eq!(
            made.iter().map(|d| d.0).collect::<Vec<_>>(),
            vec![5.0, 2.0, 0.0],
            "{dims:?}"
        );
        for (_, off) in made {
            assert!((off - 1.5).abs() < 1e-9, "on one line: {dims:?}");
        }
        // a new row placed near another dimension's line lands on it
        state.cancel();
        state.dim_chain = false;
        let others = [(DVec2::new(0.0, 0.0), DVec2::new(2.0, 0.0), 3.0)];
        dimension_tool(
            &mut doc,
            view,
            h(10.0),
            DVec2::ZERO,
            ax,
            &mut state,
            &mut sel,
            &others,
            0.2,
        )
        .unwrap();
        dimension_tool(
            &mut doc,
            view,
            h(12.0),
            DVec2::ZERO,
            ax,
            &mut state,
            &mut sel,
            &others,
            0.2,
        )
        .unwrap();
        dimension_tool(
            &mut doc,
            view,
            h(11.0),
            DVec2::new(11.0, 3.1),
            ax,
            &mut state,
            &mut sel,
            &others,
            0.2,
        )
        .unwrap();
        let last = doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Dimension(d)
                    if d.view == view && (d.a.local.x - 10.0).abs() < 1e-9 =>
                {
                    Some(d.offset)
                }
                _ => None,
            })
            .last()
            .unwrap();
        assert!((last - 3.0).abs() < 1e-9, "{last}");
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn an_arc_wall_passes_through_the_point_clicked() {
        let (a, b) = (DVec2::new(3.0, 0.0), DVec2::new(-3.0, 0.0));
        for t in [
            DVec2::new(0.0, 3.0),
            DVec2::new(0.0, -3.0),
            DVec2::new(2.0, 1.0),
        ] {
            let k = bulge_through(a, t, b).unwrap();
            let c = creabim_core::sketch::Curve::arc(a, b, k);
            let pts = c.points();
            let near = pts.iter().map(|p| p.distance(t)).fold(f64::MAX, f64::min);
            // within a step of the arc's own drawing
            assert!(near < 0.25, "{t}: {near}");
        }
        assert!(bulge_through(a, DVec2::ZERO, b).is_none());
    }

    /// Two areas showing the same plan, side by side, share the drag; a
    /// drag started in one goes on every frame until the button is let go
    /// (the owner's MU16 report: in plan the drag only moved at the press
    /// and the release, because the other area ended it after one frame).
    #[test]
    fn a_drag_in_one_of_two_plan_areas_follows_the_pointer() {
        use eframe::egui::{Pos2, Rect};
        crate::units::set(creabim_core::Units::metric());
        let mut doc = creabim_core::Document::from_model(creabim_core::starter::starter_house());
        let (view, level) = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::View(v) if e.name == "Ground Floor plan" => match v.kind {
                    ViewKind::Plan { level, .. } => Some((e.id, level)),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let wall = creabim_core::Element::new(
            "Probe",
            ElementKind::Wall(creabim_core::Wall::new(
                level,
                DVec2::new(30.0, 0.0),
                DVec2::new(34.0, 0.0),
                0.2,
                3.0,
            )),
        );
        let id = wall.id;
        doc.commit(creabim_core::Transaction::new("probe").insert(wall))
            .unwrap();
        let ctx = egui::Context::default();
        let mut state = PlanState::default();
        state.cameras.insert(view, (DVec2::new(32.0, 1.5), 100.0));
        let mut sel = crate::selection::Selection::default();
        sel.only(id);
        let left = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0));
        let right = Rect::from_min_size(Pos2::new(1000.0, 0.0), egui::vec2(1000.0, 800.0));
        let xf = Xf {
            rect: left,
            center: DVec2::new(32.0, 1.5),
            scale: 100.0,
        };
        let end = xf.to_screen(DVec2::new(34.0, 0.0));
        let mut t = 0.0;
        let mut frame = |events: Vec<egui::Event>,
                         doc: &mut creabim_core::Document,
                         state: &mut PlanState,
                         sel: &mut crate::selection::Selection| {
            t += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(2000.0, 800.0))),
                events,
                time: Some(t),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                for (i, rect) in [left, right].into_iter().enumerate() {
                    ui.push_id(("area", i), |ui| {
                        let d = creabim_geom::section::view_drawing_with(doc.model(), view, false)
                            .unwrap();
                        let style = PaneStyle {
                            background: None,
                            decorations: false,
                            camera: None,
                            scale: 50.0,
                        };
                        show(
                            ui,
                            rect,
                            doc,
                            view,
                            PaneKind::Plan { level },
                            &d,
                            state,
                            sel,
                            style,
                        );
                    });
                }
            });
            out.textures_delta.clear();
        };
        let length = |doc: &creabim_core::Document| match &doc.model().get(id).unwrap().kind {
            ElementKind::Wall(w) => w.length(),
            _ => 0.0,
        };
        frame(
            vec![egui::Event::PointerMoved(end)],
            &mut doc,
            &mut state,
            &mut sel,
        );
        frame(
            vec![egui::Event::PointerMoved(end)],
            &mut doc,
            &mut state,
            &mut sel,
        );
        frame(
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            &mut doc,
            &mut state,
            &mut sel,
        );
        let mut seen = std::collections::BTreeSet::new();
        for dx in 1..=40 {
            let p = end + egui::vec2(dx as f32, 0.0);
            frame(
                vec![egui::Event::PointerMoved(p)],
                &mut doc,
                &mut state,
                &mut sel,
            );
            seen.insert((length(&doc) * 1e4).round() as i64);
        }
        // free (grid snap off by default): a new length nearly every pixel
        assert!(
            seen.len() > 30,
            "the wall took only {} lengths over 40 px",
            seen.len()
        );
        assert!(
            (length(&doc) - 4.4).abs() < 0.02,
            "ended at {}",
            length(&doc)
        );
    }

    /// A grip sliding along its line stops where an edge crosses its path:
    /// a wall's end dragged up to another wall's face stops on the face
    /// (the owner's MU16 report: a wall's base wouldn't stop on a slab's
    /// top in a section; only corners and middles caught a sliding grip).
    #[test]
    fn a_sliding_grip_stops_on_an_edge_across_its_path() {
        use eframe::egui::{Pos2, Rect};
        crate::units::set(creabim_core::Units::metric());
        let mut doc = creabim_core::Document::from_model(creabim_core::starter::starter_house());
        let (view, level) = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::View(v) if e.name == "Ground Floor plan" => match v.kind {
                    ViewKind::Plan { level, .. } => Some((e.id, level)),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let wall = |a: DVec2, b: DVec2| {
            creabim_core::Element::new(
                "Probe",
                ElementKind::Wall(creabim_core::Wall::new(level, a, b, 0.2, 3.0)),
            )
        };
        let probe = wall(DVec2::new(30.0, 0.0), DVec2::new(34.0, 0.0));
        let id = probe.id;
        // a long parallel wall, its near face at y = 0.9, its corners far
        let across = wall(DVec2::new(20.0, 1.0), DVec2::new(45.0, 1.0));
        doc.commit(
            creabim_core::Transaction::new("probe")
                .insert(probe)
                .insert(across),
        )
        .unwrap();
        let ctx = egui::Context::default();
        let mut state = PlanState::default();
        state.cameras.insert(view, (DVec2::new(33.0, 0.5), 100.0));
        let mut sel = crate::selection::Selection::default();
        sel.only(id);
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0));
        let xf = Xf {
            rect,
            center: DVec2::new(33.0, 0.5),
            scale: 100.0,
        };
        let mut t = 0.0;
        let mut frame = |events: Vec<egui::Event>,
                         doc: &mut creabim_core::Document,
                         state: &mut PlanState,
                         sel: &mut crate::selection::Selection| {
            t += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(rect),
                events,
                time: Some(t),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                let d = creabim_geom::section::view_drawing_with(doc.model(), view, false).unwrap();
                let style = PaneStyle {
                    background: None,
                    decorations: false,
                    camera: None,
                    scale: 50.0,
                };
                show(
                    ui,
                    rect,
                    doc,
                    view,
                    PaneKind::Plan { level },
                    &d,
                    state,
                    sel,
                    style,
                );
            });
            out.textures_delta.clear();
        };
        // the probe's face (y = 0.1) slides square to the wall
        let face = xf.to_screen(DVec2::new(32.0, 0.1));
        frame(
            vec![egui::Event::PointerMoved(face)],
            &mut doc,
            &mut state,
            &mut sel,
        );
        frame(
            vec![egui::Event::PointerMoved(face)],
            &mut doc,
            &mut state,
            &mut sel,
        );
        frame(
            vec![egui::Event::PointerButton {
                pos: face,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            &mut doc,
            &mut state,
            &mut sel,
        );
        // to 2.7 px past the other wall's face at y = 0.9
        let target = xf.to_screen(DVec2::new(32.0, 0.927));
        for k in 1..=10 {
            let p = face + (target - face) * (k as f32 / 10.0);
            frame(
                vec![egui::Event::PointerMoved(p)],
                &mut doc,
                &mut state,
                &mut sel,
            );
        }
        let (start, end, t) = match &doc.model().get(id).unwrap().kind {
            ElementKind::Wall(w) => (w.start, w.end, w.thickness),
            _ => unreachable!(),
        };
        // the dragged face, whichever way the wall grew, is on y = 0.9
        let mid = (start + end) / 2.0;
        let face_y = mid.y + t / 2.0;
        assert!(
            (face_y - 0.9).abs() < 1e-6,
            "the face is at y = {face_y} (thickness {t}), not on the other wall's face at 0.9"
        );
    }

    #[test]
    fn guide_lines_go_when_nothing_uses_them() {
        let view = ElementId::new();
        let edge = snaps::Reference::Edge {
            a: DVec3::ZERO,
            b: DVec3::X,
            arc: None,
        };
        let mut state = PlanState::default();
        rest_reference(&mut state, view, Some(edge), 0.0, false);
        assert!(rest_reference(&mut state, view, Some(edge), 1.0, false).is_some());
        fade_references(&mut state, view, 2.0, false);
        assert_eq!(state.refs[&view].len(), 1);
        // a snap following them keeps them
        fade_references(&mut state, view, 6.5, true);
        fade_references(&mut state, view, 10.0, false);
        assert_eq!(state.refs[&view].len(), 1);
        fade_references(&mut state, view, 6.5 + REF_FADE + 0.1, false);
        assert!(!state.refs.contains_key(&view));
    }

    #[test]
    fn resting_after_an_align_click_keeps_its_message() {
        let view = ElementId::new();
        let edge = snaps::Reference::Edge {
            a: DVec3::ZERO,
            b: DVec3::X,
            arc: None,
        };
        // resting on an edge makes guides from it, with a message
        let mut state = PlanState::default();
        assert!(rest_reference(&mut state, view, Some(edge), 0.0, false).is_none());
        assert!(rest_reference(&mut state, view, Some(edge), 1.0, false).is_some());
        // but not right after the Align tool clicked it
        let mut state = PlanState::default();
        click_is_not_a_rest(&mut state, Tool::Align, true, Some(edge), 0.0);
        assert!(rest_reference(&mut state, view, Some(edge), 0.0, false).is_none());
        assert!(rest_reference(&mut state, view, Some(edge), 5.0, false).is_none());
        assert!(state.refs.get(&view).is_none_or(|r| r.is_empty()));
        // other tools still rest where they clicked
        let mut state = PlanState::default();
        click_is_not_a_rest(&mut state, Tool::Wall, true, Some(edge), 0.0);
        assert!(rest_reference(&mut state, view, Some(edge), 0.0, false).is_none());
        assert!(rest_reference(&mut state, view, Some(edge), 1.0, false).is_some());
    }

    use super::*;

    /// Drive a plan pane with pointer events, frame by frame.
    fn drive_box(kind_section: bool, decorations: bool, two: bool) -> usize {
        let m = creabim_core::starter::starter_house();
        let (view, kind) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::View(v) if !kind_section && e.name == "Ground Floor plan" => {
                    Some((e.id, pane_kind(v)))
                }
                ElementKind::View(v) if kind_section && e.name == "Section A-A" => {
                    Some((e.id, pane_kind(v)))
                }
                _ => None,
            })
            .unwrap();
        let mut doc = creabim_core::Document::from_model(m);
        let ctx = egui::Context::default();
        let mut state = PlanState::default();
        let mut sel = crate::selection::Selection::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 900.0));
        let d = creabim_geom::section::view_drawing_with(doc.model(), view, false).unwrap();
        let mut t = 0.0;
        let mut frame = |events: Vec<egui::Event>,
                         doc: &mut creabim_core::Document,
                         state: &mut PlanState,
                         sel: &mut crate::selection::Selection| {
            t += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(2400.0, 900.0))),
                events,
                time: Some(t),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                let style = PaneStyle {
                    background: None,
                    decorations,
                    camera: None,
                    scale: 50.0,
                };
                ui.push_id(("area", 0), |ui| {
                    show(ui, rect, doc, view, kind, &d, state, sel, style)
                });
                // a second area showing the same view, beside it (MU21)
                if two {
                    let other = rect.translate(egui::vec2(rect.width(), 0.0));
                    ui.push_id(("area", 1), |ui| {
                        show(ui, other, doc, view, kind, &d, state, sel, style)
                    });
                }
            });
            out.textures_delta.clear();
        };
        // over the middle of the drawing, then a drag of 300 x 200 px
        let a = Pos2::new(700.0, 550.0);
        let b = Pos2::new(400.0, 350.0);
        for e in [
            vec![egui::Event::PointerMoved(a)],
            vec![egui::Event::PointerMoved(a)],
            vec![egui::Event::PointerButton {
                pos: a,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        ] {
            frame(e, &mut doc, &mut state, &mut sel);
        }
        for k in 1..=10 {
            let p = a + (b - a) * (k as f32 / 10.0);
            frame(
                vec![egui::Event::PointerMoved(p)],
                &mut doc,
                &mut state,
                &mut sel,
            );
        }
        frame(
            vec![egui::Event::PointerButton {
                pos: b,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
            &mut doc,
            &mut state,
            &mut sel,
        );
        frame(
            vec![egui::Event::PointerMoved(b)],
            &mut doc,
            &mut state,
            &mut sel,
        );
        sel.len()
    }

    #[test]
    fn a_left_drag_on_a_plan_with_nothing_selected_selects_with_a_box() {
        assert!(
            drive_box(false, false, false) > 0,
            "the box took nothing in the plan"
        );
        assert!(drive_box(false, true, false) > 0, "with decorations");
        assert!(drive_box(true, true, false) > 0, "in a section");
        // two areas showing the same plan: the other one used to end the
        // box the moment it started (MU21)
        assert!(
            drive_box(false, true, true) > 0,
            "with a second area on the same plan"
        );
    }

    #[test]
    fn a_crop_box_shows_as_its_footprint_in_a_plan_and_its_extent_in_a_section() {
        let m = creabim_core::sample::sample_house();
        let views: Vec<(ElementId, bool)> = m
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::View(v) => match v.kind {
                    creabim_core::ViewKind::Plan { .. } => Some((e.id, true)),
                    creabim_core::ViewKind::Section { .. } => Some((e.id, false)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let crop = creabim_core::CropBox {
            view: None,
            min: DVec3::new(1.0, 2.0, 0.5),
            max: DVec3::new(4.0, 6.0, 3.0),
            scope: Default::default(),
            enabled: true,
            frame_hidden: false,
            except: vec![],
            outside: Default::default(),
        };
        let e = creabim_core::Element::new("Crop", ElementKind::CropBox(crop));
        let id = e.id;
        let mut doc = Document::from_model(m);
        doc.commit(Transaction::new("crop").insert(e)).unwrap();
        let plan = views.iter().find(|v| v.1).unwrap().0;
        let r = crop_rects(doc.model(), plan);
        let (_, lo, hi) = r.iter().find(|r| r.0 == id).unwrap();
        assert_eq!((*lo, *hi), (DVec2::new(1.0, 2.0), DVec2::new(4.0, 6.0)));
        let section = views.iter().find(|v| !v.1).unwrap().0;
        let (_, lo, hi) = crop_rects(doc.model(), section)
            .into_iter()
            .find(|r| r.0 == id)
            .unwrap();
        assert!(
            (lo.y - 0.5).abs() < 1e-9 && (hi.y - 3.0).abs() < 1e-9,
            "{lo} {hi}"
        );
    }
    use creabim_core::{EditMesh, FreeMesh};

    #[test]
    fn a_view_marks_parts_are_traced_not_boxed() {
        // every part of a bubble with its wedge is traced by a hover
        let id = ElementId::new();
        let items =
            creabim_geom::markers::bubble(id, DVec2::ZERO, "A", "A-201", Some(DVec2::X), 0.05);
        assert!(items.len() >= 4);
        for it in &items {
            assert!(is_mark(it), "{it:?} would be washed, not traced");
        }
        // a wall's cut linework and fill are not marks
        let wall = PlanItem::Line {
            id,
            pen: Pen::Cut,
            a: DVec2::ZERO,
            b: DVec2::X,
        };
        assert!(!is_mark(&wall));
    }

    #[test]
    fn a_click_in_a_doors_opening_takes_the_door_not_the_footing_below() {
        let (door, footing) = (ElementId::new(), ElementId::new());
        let items = vec![
            PlanItem::Line {
                id: footing,
                pen: Pen::Hidden,
                a: DVec2::new(-3.0, 0.0),
                b: DVec2::new(3.0, 0.0),
            },
            PlanItem::Line {
                id: door,
                pen: Pen::Symbol,
                a: DVec2::new(0.0, 0.0),
                b: DVec2::new(0.0, 0.9),
            },
            PlanItem::Arc {
                id: door,
                pen: Pen::Symbol,
                center: DVec2::ZERO,
                radius: 0.9,
                start: 0.0,
                end: std::f64::consts::FRAC_PI_2,
            },
        ];
        let hits = hit_all(&items, DVec2::new(0.45, 0.0), 0.05, 0.05);
        assert_eq!(hits, [door, footing]);
        // away from the door the footing is still picked
        assert_eq!(
            hit_test(&items, DVec2::new(2.0, 0.0), 0.05, 0.05),
            Some(footing)
        );
    }

    #[test]
    fn tools_draw_new_elements_as_their_type() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let (tx, ids) = creabim_core::types::add_starters(doc.model());
        doc.commit(tx).unwrap();
        let level = doc.model().levels().next().unwrap().0.id;
        let storefront = ids[5];
        let window = ids[9];
        let mut state = PlanState::default();
        state
            .types
            .insert(creabim_core::TypeCategory::Wall, storefront);
        state
            .types
            .insert(creabim_core::TypeCategory::Window, window);
        let mut sel = Selection::default();
        let (a, b) = (DVec2::new(0.0, -3.0), DVec2::new(6.0, -3.0));
        plan_click(&mut doc, level, Tool::Wall, a, &mut state, &mut sel).unwrap();
        plan_click(&mut doc, level, Tool::Wall, b, &mut state, &mut sel).unwrap();
        let wall = sel.active.unwrap();
        let e = doc.model().get(wall).unwrap();
        assert_eq!(e.of_type.as_ref().map(|r| r.id), Some(storefront));
        let ElementKind::Wall(w) = &e.kind else {
            panic!()
        };
        assert!(w.curtain.is_some(), "drawn as a curtain wall");
        // a window clicked into a plain wall takes its type's size
        let mut sel = Selection::default();
        plan_click(
            &mut doc,
            level,
            Tool::Window,
            DVec2::new(5.0, 6.0),
            &mut state,
            &mut sel,
        )
        .unwrap();
        let e = doc.model().get(sel.active.unwrap()).unwrap();
        assert_eq!(e.of_type.as_ref().map(|r| r.id), Some(window));
        // and the opening tool cuts a bare hole, with no type
        let mut sel = Selection::default();
        plan_click(
            &mut doc,
            level,
            Tool::Opening,
            DVec2::new(10.0, 4.5),
            &mut state,
            &mut sel,
        )
        .unwrap();
        let e = doc.model().get(sel.active.unwrap()).unwrap();
        assert!(matches!(&e.kind, ElementKind::Opening(o) if o.kind == OpeningKind::Hole));
        assert!(e.of_type.is_none());
    }

    /// A dimension's second point locks onto the first point's
    /// horizontal and vertical, as a wall's end does (A7-11).
    #[test]
    fn a_dimension_snaps_on_the_axes_of_its_first_point() {
        let doc = Document::from_model(creabim_core::sample::sample_house());
        let v = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::View(v) if matches!(v.kind, ViewKind::Plan { .. }) => Some(v.clone()),
                _ => None,
            })
            .unwrap();
        // far from the house, so nothing else is under the pointer
        let first = DVec2::new(40.0, 40.0);
        let xf = Xf {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(1000.0)),
            center: first,
            scale: 100.0,
        };
        let snapper = Snapper::from_drawing(&[], &[]);
        let cfg = snaps::SnapSettings::default();
        for (aim, want) in [
            (DVec2::new(43.0, 40.05), DVec2::new(43.0, 40.0)),
            (DVec2::new(39.96, 37.0), DVec2::new(40.0, 37.0)),
        ] {
            let (_, at, kind, _) = dim_snap_guided(
                &doc,
                &v,
                pane_kind(&v),
                Some(&snapper),
                &cfg,
                &xf,
                xf.to_screen(aim),
                &[],
                Some(first.extend(0.0)),
                &[],
            );
            assert!(
                at.distance(want) < 1e-6,
                "aimed at {aim}, got {at} ({kind:?}), wanted {want}"
            );
        }
    }

    #[test]
    fn a_dimension_attaches_to_a_mass_corner_and_follows_it() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let (view, v) = doc
            .model()
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::View(v) if matches!(v.kind, ViewKind::Plan { .. }) => {
                    Some((e.id, v.clone()))
                }
                _ => None,
            })
            .unwrap();
        let ViewKind::Plan { level, .. } = v.kind else {
            unreachable!()
        };
        let mass = Element::new(
            "Mass",
            ElementKind::FreeMesh(FreeMesh::new(
                Some(level),
                DVec3::new(20.0, 5.0, 0.0),
                EditMesh::cuboid(DVec3::new(2.0, 4.0, 3.0)),
            )),
        );
        let id = mass.id;
        doc.commit(Transaction::new("Add mass").insert(mass))
            .unwrap();
        let drawing = creabim_geom::section::view_drawing(doc.model(), view).unwrap();
        let snapper = Snapper::from_drawing(&drawing.items, &[]);
        let xf = Xf {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(1000.0)),
            center: DVec2::new(20.0, 5.0),
            scale: 100.0,
        };
        let cursor = xf.to_screen(DVec2::new(21.03, 6.98));
        let cfg = snaps::SnapSettings::default();
        let (hp, at, kind) = dim_snap(&doc, &v, pane_kind(&v), Some(&snapper), &cfg, &xf, cursor);
        assert_eq!(kind, Some(SnapKind::Endpoint));
        assert!(at.distance(DVec2::new(21.0, 7.0)) < 1e-9);
        assert_eq!(hp.host, Some(id));
        // move the mass: the point goes with it
        doc.edit(id, "Move mass", |e| {
            if let ElementKind::FreeMesh(m) = &mut e.kind {
                m.origin += DVec3::new(1.5, -1.0, 0.0);
            }
        })
        .unwrap();
        let now = annotate::resolve(doc.model(), &v, &hp).unwrap();
        assert!(now.distance(DVec2::new(22.5, 6.0)) < 1e-9, "{now}");
        // with snapping off it is a free point
        let off = cfg.live(true);
        let (hp, ..) = dim_snap(&doc, &v, pane_kind(&v), Some(&snapper), &off, &xf, cursor);
        assert_eq!(hp.host, None);
    }

    #[test]
    fn the_beam_inside_a_wall_is_under_the_pointer_in_a_section_and_first_in_x_ray() {
        let (model, beam) = creabim_core::sample::sample_with_buried_beam();
        let doc = Document::from_model(model);
        let sec = doc
            .model()
            .iter()
            .find(|e| e.name == "Section A")
            .unwrap()
            .id;
        let wall = doc
            .model()
            .iter()
            .find(|e| e.name == "GF Wall 1")
            .unwrap()
            .id;
        let Some(ElementKind::View(v)) = doc.model().get(sec).map(|e| &e.kind) else {
            panic!("Section A is a view")
        };
        let kind = pane_kind(v);
        let drawing = creabim_geom::section::view_drawing(doc.model(), sec).unwrap();
        let mut state = PlanState::default();
        // Section A runs north from y = -5: the wall is 5 m along it; the
        // beam sits 2.3 to 2.6 m up inside it
        let p = DVec2::new(5.0, 2.45);
        let under = under_pointer(&mut state, &doc, sec, kind, &drawing, p, 0.02, 0.1);
        assert_eq!(under.first(), Some(&wall), "a plain click takes the wall");
        assert!(
            under.contains(&beam),
            "the beam is found under it: {under:?}"
        );
        // what a click takes without Tab is what it always took
        assert_eq!(
            hit_test(&drawing.items, p, 0.02, 0.1).or_else(|| ray_pick(&doc, sec, kind, p)),
            Some(wall)
        );
        state.xray.insert(sec);
        let xray = creabim_geom::section::view_drawing_with(doc.model(), sec, true).unwrap();
        let under = under_pointer(&mut state, &doc, sec, kind, &xray, p, 0.02, 0.1);
        assert_eq!(under.first(), Some(&beam), "in X-ray the beam comes first");
        assert!(under.contains(&wall));
    }

    #[test]
    fn what_a_3d_views_crop_box_cuts_away_cannot_be_picked_in_it() {
        let mut doc = Document::from_model(creabim_core::sample::sample_house());
        let wall = doc
            .model()
            .iter()
            .find(|e| e.name == "GF Wall 1")
            .unwrap()
            .id;
        let axo = Element::new(
            "Axo",
            ElementKind::View(creabim_core::View::axonometric(45.0, 30.0)),
        );
        let view = axo.id;
        doc.commit(Transaction::new("view").insert(axo)).unwrap();
        assert!(pick_scene(&doc, view).iter().any(|s| s.id == wall));
        // a box over the north half of the house: the south wall (y = 0)
        // is cut away
        let b = creabim_core::CropBox::new(
            Some(view),
            DVec3::new(-1.0, 3.0, -1.0),
            DVec3::new(11.0, 7.0, 7.0),
        );
        doc.commit(Transaction::new("crop").insert(Element::new("Box", ElementKind::CropBox(b))))
            .unwrap();
        let scene = pick_scene(&doc, view);
        assert!(
            !scene.iter().any(|s| s.id == wall),
            "cut away, not pickable"
        );
        assert!(!scene.is_empty(), "what is inside the box still is");
    }

    #[test]
    fn in_an_elevation_the_slab_behind_a_wall_is_under_the_pointer_too() {
        let doc = Document::from_model(creabim_core::sample::sample_house());
        let named = |n: &str| doc.model().iter().find(|e| e.name == n).unwrap().id;
        let (elev, wall, slab) = (
            named("South Elevation S"),
            named("GF Wall 1"),
            named("FF Slab"),
        );
        let Some(ElementKind::View(v)) = doc.model().get(elev).map(|e| &e.kind) else {
            panic!("the elevation is a view")
        };
        let kind = pane_kind(v);
        let drawing = creabim_geom::section::view_drawing(doc.model(), elev).unwrap();
        let mut state = PlanState::default();
        // the elevation runs east from x = -2: 7 m along is the middle of
        // the south wall; the first floor slab is 2.75 to 3 m up behind it
        let p = DVec2::new(7.0, 2.9);
        let under = under_pointer(&mut state, &doc, elev, kind, &drawing, p, 0.02, 0.1);
        assert_eq!(under.first(), Some(&wall), "a plain click takes the wall");
        assert!(under.contains(&slab), "Tab reaches the slab: {under:?}");
        // and in X-ray the slab, though it holds others too, is still there
        state.xray.insert(elev);
        let xray = creabim_geom::section::view_drawing_with(doc.model(), elev, true).unwrap();
        let under = under_pointer(&mut state, &doc, elev, kind, &xray, p, 0.02, 0.1);
        assert!(under.contains(&slab) && under.contains(&wall));
    }

    #[test]
    fn tag_fixtures_tags_services_fixtures_once_and_leaves_furniture() {
        let mut doc = Document::from_model(creabim_core::sample::furnished_house());
        let plan = doc
            .model()
            .iter()
            .find(|e| e.name == "Ground Floor plan")
            .unwrap()
            .id;
        let n = tag_fixtures(&mut doc, plan).unwrap();
        assert!(
            n > 10,
            "the ground floor has its sockets, switches and lights"
        );
        let model = doc.model();
        for e in model.iter() {
            let ElementKind::Tag(t) = &e.kind else {
                continue;
            };
            if t.view != plan {
                continue;
            }
            let host = model.get(t.host).unwrap();
            if let Some(ty) = creabim_core::objects::type_of(model, host) {
                assert!(ty.category.is_services(), "{} is not a fixture", host.name);
            }
        }
        // a second run finds nothing left to tag
        assert_eq!(tag_fixtures(&mut doc, plan).unwrap(), 0);
    }
}
