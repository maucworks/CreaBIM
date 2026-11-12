use crate::camera::OrbitCamera;
use crate::commands::Command;
use crate::editmode;
use crate::handles::{self, PaneInput, Ray};
use crate::hoverpick;
use crate::icons::{self, Icon};
use crate::modal::{Outcome, PaneMap, Space};
use crate::planview::{self, PaneStyle, PlanState, Tool};
use crate::render3d::{RenderMode, Vertex, Viewport3d};
use crate::selection::Selection;
use crate::sheetview::{self, SheetState};
use crate::sketch::{Click, SketchDraft};
use crate::snaps::{self, SnapKind, Snapper};
use creabim_core::{
    element::FREE_MESH_CLASSES, sample::sample_house, Axis, BoolOp, Cut, Document, Element,
    ElementId, ElementKind, Hatch, Material, Model, ScheduleCategory, Sheet, Transaction, Underlay,
    View, ViewKind, ViewStyle, Workplane,
};
use creabim_geom::sheet::{self as sheets, PlacedView, SheetInfo};
use creabim_geom::{pick, plan::Pen, section::Drawing, PlanItem, SceneItem};
use eframe::egui::{self, Color32, Rect, Sense, Vec2};
use eframe::egui_wgpu;
use glam::{DVec2, DVec3, Vec2 as GVec2, Vec3 as GVec3};
use std::collections::HashMap;

#[path = "actionbar.rs"]
mod actionbar;
#[path = "appmenu.rs"]
pub(crate) mod appmenu;
#[path = "areasui.rs"]
mod areasui;
#[path = "assemblies.rs"]
mod assemblies;
#[path = "cropui.rs"]
mod cropui;
#[path = "divideui.rs"]
mod divideui;
#[path = "drafting.rs"]
mod drafting;
#[path = "editui.rs"]
mod editui;
#[path = "gridvis.rs"]
mod gridvis;
#[path = "helpui.rs"]
pub(crate) mod helpui;
#[path = "importui.rs"]
mod importui;
#[path = "joinsui.rs"]
mod joinsui;
#[path = "levelcopy.rs"]
mod levelcopy;
#[path = "levelsui.rs"]
mod levelsui;
#[path = "marksui.rs"]
mod marksui;
#[path = "mastersui.rs"]
mod mastersui;
#[path = "materials.rs"]
mod materials;
#[path = "objectsui.rs"]
mod objectsui;
#[path = "profilesui.rs"]
mod profilesui;
#[path = "reportui.rs"]
mod reportui;
#[path = "selectsimilar.rs"]
mod selectsimilar;
#[path = "shell.rs"]
pub(crate) mod shell;
#[path = "stylesui.rs"]
mod stylesui;
#[path = "typepreview.rs"]
mod typepreview;
#[path = "typesui.rs"]
mod typesui;
#[path = "wallsui.rs"]
mod wallsui;

/// What the 3D geometry was last built from; rebuild when any of it changes.
/// ((model revision, hidden revision), selection, selected face, drawing
/// shown in 3D, active view)
/// How a workplane is being picked in the 3D view.
/// Split while it runs (MU39): what it cuts, the first point of a line
/// being drawn, then the cutting line.
#[derive(Clone, Debug)]
struct SplitRun {
    targets: Vec<ElementId>,
    a: Option<DVec2>,
    line: Option<creabim_core::split::Line>,
}

#[derive(Clone, Debug, PartialEq)]
enum PlanePick {
    /// Click a face.
    Face,
    /// Click three points; these are the ones so far.
    Points(Vec<DVec3>),
}

type GeometryKey = (
    (u64, u64),
    Selection,
    Option<usize>,
    bool,
    Option<ElementId>,
);

/// Where the 2D plan is shown relative to the 3D view.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlanPlacement {
    /// Side by side with the 3D view.
    Docked,
    /// In its own OS window, e.g. on a second monitor.
    Window,
    /// Inset inside the 3D workspace.
    Overlay,
    Hidden,
}

pub struct CreaBimApp {
    doc: Document,
    selection: Selection,
    /// Selected face when a free mesh is selected (mesh edit mode).
    face: Option<usize>,
    /// What the 2D pane shows: a view (plan, section, elevation, 3D) or a sheet.
    active_view: Option<ElementId>,
    sheet: SheetState,
    /// Each area's own sheet state (MU29: two areas on sheets shared one,
    /// so one cancelled the other's drags and their cameras moved together).
    sheets: HashMap<usize, SheetState>,
    /// The area being drawn now.
    pub(super) area_now: usize,
    /// Generated drawings by view, with the model revision they were made at.
    drawings: HashMap<ElementId, (u64, Drawing)>,
    /// Drawings and the 3D scene being built off the UI thread.
    builds: crate::builds::Builds,
    /// Script blocks recomputed off the UI thread.
    scripts: crate::scriptrun::ScriptRunner,
    /// Each view's looks on sheets, with the model revision they are for.
    looks: HashMap<ElementId, (u64, std::sync::Arc<creabim_geom::looks::Looks>)>,
    /// The same for views seen in X-ray, which their sheets don't follow.
    xray_drawings: HashMap<ElementId, (u64, Drawing)>,
    /// The theme the 3D view's wires were last drawn for (MU31).
    dark_3d: bool,
    /// The Divide window (MU34 B2).
    divide: divideui::DivideUi,
    /// The profile library window (MU27 block 5).
    profiles: profilesui::ProfilesUi,
    masters: mastersui::MastersUi,
    log_ui: reportui::LogUi,
    /// Report a problem asked for: the themed window is open; whether to
    /// put the project in.
    report_ask: Option<bool>,
    /// Closing with unsaved changes: Save, Don't save or Cancel is asked.
    close_ask: bool,
    /// The close was settled (saved, or the changes let go): let it through.
    close_ok: bool,
    /// What the area in front showed last frame: a Masters or Profile view
    /// coming forward brings its own Properties tab (MU40).
    prop_front: Option<crate::areas::Shows>,
    camera: OrbitCamera,
    cube: crate::viewcube::ViewCube,
    /// Aspect ratio of the 3D view as last drawn, for framing.
    aspect: f32,
    viewport: Option<Viewport3d>,
    geometry_key: Option<GeometryKey>,
    /// How far Inset draws a face inside its edges.
    inset_by: f64,
    /// Split while it runs: what it cuts, the first point of a line being
    /// drawn, the cutting line once there is one (MU39).
    split: Option<SplitRun>,
    /// The selection when a point was first asked for: kept while points
    /// are picked, so a click in a view never changes it (MU39).
    pick_selection: Option<Selection>,
    /// Bumped at each rebuild of the model's 3D buffers.
    geometry_gen: u64,
    /// What the selection highlight was last built for.
    selection_key: Option<(u64, Selection, Option<usize>)>,
    /// Drawings and the 3D view on dark paper this frame (A4-3).
    paper_is_dark: bool,
    /// The Properties tab showing (MU19); it stays when the selection
    /// changes, as Blender's does.
    prop_tab: PropTab,
    /// The tool and whether something was selected, last frame: a change
    /// can pick the tab (MU19).
    prop_seen: (Tool, bool),
    /// Sheets as they print, on white, whatever the paper (MU21).
    print_preview: bool,
    /// The Export sheets dialog (MU22), and its choices this session.
    sheets_export_open: bool,
    sheets_export: SheetsExport,
    /// A reference to take away, asked from its Properties (MU23).
    remove_ref: Option<ElementId>,
    /// Copy to view…: the reference, and the view its copy goes to (MU38).
    copy_ref: Option<(ElementId, ElementId)>,
    /// Shift+A's Add menu, open, with what is typed in it (MU24).
    add_menu: Option<String>,
    scene: Vec<SceneItem>,
    plan: PlanState,
    placement: PlanPlacement,
    plan_in_3d: bool,
    /// The view areas and the workspace (brief 02, 2.6).
    areas: areasui::AreaState,
    /// The focused area's header controls that didn't fit, for its More menu.
    areas_over: Vec<serde_json::Value>,
    /// A handle or element being dragged in the 3D view.
    handle_drag: Option<handles::Drag>,
    /// B pressed in 3D: the next drag draws a selection box; and where it began.
    box_armed: bool,
    box_3d: Option<egui::Pos2>,
    /// Snaps for the 3D view: built from the scene without the listed
    /// elements, at a model revision.
    snaps_3d: Option<(u64, Vec<ElementId>, Snapper)>,
    /// First click of a two-click tool in the 3D view, on a level.
    pending_3d: Option<(DVec2, ElementId)>,
    /// A beam drawn on a face (MU40): its first point, the face's plane
    /// (a point and its outward normal).
    beam_face: Option<(DVec3, DVec3, DVec3)>,
    /// Points rested on while drawing in 3D, that the next point lines up
    /// with, and the one being rested on now (MU24: as in plans).
    rested_3d: (Vec<DVec3>, Option<(DVec3, f64)>),
    /// Where the 3D right-click menu was opened, and the face there.
    menu_at_3d: Option<DVec3>,
    menu_plane_3d: Option<Workplane>,
    /// A workplane set on a face, through three points or off a level, with
    /// its name. The 3D sketch and guide tools draw on it instead of the
    /// level, and the grid lies on it.
    workplane: Option<(Workplane, String)>,
    /// Picking a workplane in the 3D view.
    plane_pick: Option<PlanePick>,
    /// The modifier card last worked in: its guides show in the views,
    /// and Pick sets its centre (MU39).
    mod_focus: Option<(ElementId, usize)>,
    /// First point of a guide being drawn on the workplane.
    guide_from: Option<DVec3>,
    extrude_distance: f64,
    path: String,
    /// The model's revision when it was last opened or saved: anything
    /// else is unsaved (A7-8).
    saved_revision: Option<u64>,
    /// The window title last set, so it is only sent when it changes.
    title: String,
    /// The status and tool last written to the activity log.
    logged: (String, Option<Tool>),
    status: String,
    /// Linked files (Blender...) and when they were last read.
    watch: crate::files::Watch,
    /// Reload linked files as soon as they are saved.
    live_reload: bool,
    /// Elements hidden from the 3D view and the 2D panes (not from sheets
    /// or exports), and a count of changes to them.
    hidden: std::collections::HashSet<ElementId>,
    hidden_rev: u64,
    /// What the 3D view leaves out (categories, grid groups, elements).
    /// Its categories are kept in the settings, the rest for the session.
    vis3d: creabim_core::ViewHidden,
    /// What Ctrl+C copied, for pasting when the system clipboard has none.
    clipboard: Vec<Element>,
    recent: crate::files::Recent,
    /// The web interface round the views.
    shell: shell::Shell,
    /// The material browser.
    mat: materials::MaterialsUi,
    /// The assembly editor.
    asm: assemblies::AssembliesUi,
    /// The Types window.
    types: typesui::TypesUi,
    /// A type opened on its own, and the type Show in model lights.
    type_preview: typepreview::TypePreview,
    /// The Catalogue window and the user's own catalogue.
    catalogue: objectsui::CatalogueUi,
    /// The Levels manager and the Project units dialog.
    levels: levelsui::LevelsUi,
    marks: marksui::MarksUi,
    gstyles: stylesui::StylesUi,
    /// The Copy to levels and Move to level windows.
    level_copy: levelcopy::LevelCopyUi,
    /// What the last IFC import brought across, and what it couldn't.
    import_report: Option<importui::ImportReport>,
    /// The annotation styles and sheet templates windows.
    drafting: drafting::DraftingUi,
    /// Orbit about the selection rather than the middle of the view.
    orbit_selection: bool,
    /// Where the 3D view was drawn last frame, for its navigation buttons.
    rect_3d: Option<Rect>,
    /// Where the 2D pane was drawn last frame, and whether the pointer was
    /// last over it rather than the 3D view: what Alt+Z turns X-ray on in.
    rect_2d: Option<Rect>,
    last_in_2d: bool,
    /// What the status bar said after the last export, and the folder it
    /// went to: while the message shows, the bar offers to open the folder.
    last_export: Option<String>,
    /// What a box selection took, while it is still the selection: the
    /// status bar offers Keep only… for it.
    boxed: Option<Vec<ElementId>>,
    last_export_dir: Option<std::path::PathBuf>,
    /// The crop box whose elements clicks in the 3D view are picking.
    crop_pick: Option<ElementId>,
    /// The element outside a crop box the status bar last spoke of.
    outside_said: Option<ElementId>,
    /// The 3D extents the crop ghosts were measured from: the model's
    /// revision, the elements asked for, and their boxes.
    #[allow(clippy::type_complexity)]
    outside_bounds: Option<(
        u64,
        Vec<ElementId>,
        std::collections::HashMap<ElementId, (DVec3, DVec3)>,
    )>,
    /// The Settings window is open.
    settings_open: bool,
    /// The Project information window (A7-5).
    project_open: bool,
    /// A note open in its own big editor window (MU26).
    note_editor: Option<ElementId>,
    /// A schedule export asked from its header ("csv" or "xlsx").
    schedule_export: Option<String>,
    /// The model revision last checked for overlapping walls.
    overlap_seen: u64,
    /// Guides to copy from a sheet to every other one, next frame.
    copy_guides: Option<(ElementId, Vec<(bool, f64)>)>,
    /// The project information being edited, until Apply.
    project_draft: Option<creabim_core::ProjectInfo>,
    /// Export > IFC: the window asking how walls and slabs go out (MU20).
    ifc_dialog: bool,
    /// The New project window, with the classic panels.
    new_project_classic: bool,
}

/// How much of the accent a selected element takes in 3D (A7-2).
const SELECTION_TINT: f32 = 0.35;

impl CreaBimApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = crate::files::Settings::load();
        let theme = crate::theme::Theme::from_id(&settings.theme).unwrap_or_default();
        crate::handles::set_grip_style(settings.grips);
        crate::modal::set_bar_by_pointer(settings.edit_bar == "pointer");
        crate::handles::set_edit_display(settings.edit_edge_width, settings.edit_vertex_size);
        shell::style(&cc.egui_ctx, theme);
        let viewport = cc
            .wgpu_render_state
            .as_ref()
            .map(|rs| Viewport3d::new(&rs.device));
        let mut app = Self::blank(viewport);
        crate::actlog::start();
        // the graphics card, for problem reports (MU40)
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            let i = rs.adapter.get_info();
            let gpu = format!(
                "{} ({:?}, driver {} {})",
                i.name, i.backend, i.driver, i.driver_info
            );
            crate::actlog::line(format!("graphics: {gpu}"));
            reportui::set_gpu(gpu);
        }
        crate::actlog::line(format!("system: {}", reportui::os_version()));
        app.builds.set_context(cc.egui_ctx.clone());
        app.shell.theme = theme;
        app.vis3d.categories = settings.hidden_3d.clone();
        // grid snap always starts off; its spacing and the distance marks
        // are kept
        app.plan.snap.increment = false;
        app.plan.temp_dims_off = settings.temp_dims_off;
        app.plan.snap.marks = settings.distance_marks;
        if settings.snap_spacing_um > 0 {
            app.plan.snap.step_um = settings.snap_spacing_um;
        }
        app.areas.workspace = settings.workspace.clone();
        if let Some(root) = settings.areas.clone() {
            app.areas.root = root;
        }
        app.shell.settings = settings;
        app.load_model(creabim_core::starter::starter_house());
        app.status = STARTER_LOADED.into();
        // `creabim-app house.creabim` opens that file instead
        if let Some(path) = launch_file(std::env::args().skip(1)) {
            app.open_path(&path);
        }
        app
    }

    /// The app round an empty document, with default settings and no
    /// window: [`Self::new`] fills it in, and tests use it as it is.
    fn blank(viewport: Option<Viewport3d>) -> Self {
        Self {
            doc: Document::new(),
            selection: Selection::default(),
            face: None,
            active_view: None,
            sheet: SheetState::default(),
            sheets: HashMap::new(),
            area_now: 0,
            drawings: HashMap::new(),
            builds: Default::default(),
            scripts: Default::default(),
            looks: HashMap::new(),
            xray_drawings: HashMap::new(),
            dark_3d: true,
            divide: Default::default(),
            profiles: Default::default(),
            masters: Default::default(),
            log_ui: Default::default(),
            report_ask: None,
            close_ask: false,
            close_ok: false,
            prop_front: None,
            camera: OrbitCamera::default(),
            cube: Default::default(),
            aspect: 0.65,
            viewport,
            geometry_key: None,
            geometry_gen: 0,
            pick_selection: None,
            inset_by: 0.1,
            split: None,
            selection_key: None,
            paper_is_dark: false,
            prop_tab: PropTab::Element,
            prop_seen: (Tool::Select, false),
            print_preview: false,
            sheets_export_open: false,
            sheets_export: SheetsExport::default(),
            remove_ref: None,
            copy_ref: None,
            add_menu: None,
            scene: vec![],
            plan: PlanState::default(),
            placement: PlanPlacement::Docked,
            plan_in_3d: false,
            areas: Default::default(),
            areas_over: vec![],
            handle_drag: None,
            box_armed: false,
            box_3d: None,
            snaps_3d: None,
            pending_3d: None,
            beam_face: None,
            rested_3d: (vec![], None),
            menu_at_3d: None,
            menu_plane_3d: None,
            guide_from: None,
            workplane: None,
            plane_pick: None,
            mod_focus: None,
            extrude_distance: 1.0,
            path: "project.creabim".into(),
            saved_revision: None,
            title: String::new(),
            logged: (String::new(), None),
            watch: Default::default(),
            live_reload: true,
            hidden: Default::default(),
            hidden_rev: 0,
            vis3d: Default::default(),
            clipboard: vec![],
            recent: crate::files::Recent::load(),
            shell: Default::default(),
            mat: Default::default(),
            asm: Default::default(),
            types: Default::default(),
            type_preview: Default::default(),
            catalogue: Default::default(),
            levels: Default::default(),
            settings_open: false,
            project_open: false,
            note_editor: None,
            copy_guides: None,
            overlap_seen: 0,
            schedule_export: None,
            project_draft: None,
            ifc_dialog: false,
            new_project_classic: false,
            marks: Default::default(),
            gstyles: Default::default(),
            level_copy: Default::default(),
            import_report: None,
            drafting: Default::default(),
            orbit_selection: false,
            rect_3d: None,
            rect_2d: None,
            last_in_2d: false,
            last_export: None,
            boxed: None,
            last_export_dir: None,
            crop_pick: None,
            outside_said: None,
            outside_bounds: None,
            status: "Loaded the sample house. Orbit with the middle button (or Alt + left drag), pan with right drag, zoom with the wheel; a left drag selects with a box.".into(),
        }
    }

    fn load_model(&mut self, model: Model) {
        self.doc = Document::from_model(with_plan_views(model));
        self.selection.clear();
        self.hidden.clear();
        self.hidden_rev += 1;
        self.face = None;
        self.crop_pick = None;
        self.active_view = None;
        self.sheet = SheetState::default();
        self.sheets.clear();
        self.drawings.clear();
        self.looks.clear();
        self.xray_drawings.clear();
        self.plan.xray.clear();
        self.plan.hover = None;
        self.geometry_key = None;
        self.drop_stale_selection();
        self.frame_all();
    }

    /// Open a project file, or import and link any other file.
    fn open_path(&mut self, path: &str) {
        let lower = path.to_ascii_lowercase();
        // material cards and glTF files bring materials into this project
        if materials::is_material_file(path) {
            self.import_materials(&[path.to_string()]);
            return;
        }
        if lower.ends_with(".ifc") || lower.ends_with(".ifcx") {
            let r = std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|t| {
                    if lower.ends_with(".ifcx") {
                        creabim_ifc::import_ifcx(&t)
                    } else {
                        creabim_ifc::import_ifc(&t)
                    }
                    .map_err(|e| e.to_string())
                });
            match r {
                Ok(imported) => {
                    let n = imported.model.len();
                    let warnings = imported.warnings.len();
                    let report = importui::ImportReport::new(&file_label(path), &imported);
                    for w in &imported.warnings {
                        eprintln!("IFC import: {w}");
                    }
                    for d in &imported.details {
                        eprintln!("IFC import:   {d}");
                    }
                    self.import_report = Some(report);
                    self.load_model(imported.model);
                    self.watch = Default::default();
                    self.recent.add(path);
                    self.path = with_extension(path, "creabim");
                    self.status = format!(
                        "Opened {} ({}): {n} elements{}. Save keeps it as {}.",
                        file_label(path),
                        imported.schema,
                        if warnings > 0 {
                            format!(", {warnings} notes (File › Import report)")
                        } else {
                            String::new()
                        },
                        file_label(&self.path)
                    );
                }
                Err(e) => self.status = format!("Couldn't open {path}: {e}"),
            }
            return;
        }
        if !crate::files::is_project(path) {
            self.link_file(path);
            return;
        }
        match creabim_store::load(path) {
            Ok(doc) => {
                let model = doc.model().clone();
                self.load_model(model.clone());
                // keep the saved undo history when no views had to be added
                if self.doc.model() == &model {
                    self.doc = doc;
                }
                self.watch = Default::default();
                self.recent.add(path);
                self.path = path.to_string();
                self.saved_revision = Some(self.doc.model().revision());
                self.status = format!("Opened {path}");
                // copies a Duplicate left on their originals (MU24)
                let stacked = creabim_core::checks::stacked_copies(self.doc.model()).len();
                if stacked > 0 {
                    self.status = format!(
                        "Opened {path}. {stacked} element{} sit exactly on a copy of themselves: Manage › Copies on originals removes them.",
                        if stacked == 1 { "" } else { "s" }
                    );
                }
            }
            Err(e) => self.status = format!("Couldn't open {path}: {e}"),
        }
    }

    /// Has the model changed since it was opened or saved?
    pub(crate) fn unsaved(&self) -> bool {
        self.saved_revision != Some(self.doc.model().revision())
    }

    fn save(&mut self) {
        if self.path.to_ascii_lowercase().ends_with(".json") {
            let r = std::fs::write(&self.path, self.doc.model().to_json());
            if r.is_ok() {
                self.saved_revision = Some(self.doc.model().revision());
            }
            self.report(r, format!("Saved {}", self.path));
            return;
        }
        if !self.path.to_ascii_lowercase().ends_with(".creabim") {
            self.path = with_extension(&self.path, "creabim");
        }
        let r = creabim_store::save(&self.path, &self.doc);
        if r.is_ok() {
            let path = self.path.clone();
            self.recent.add(&path);
            self.saved_revision = Some(self.doc.model().revision());
        }
        self.report(r, format!("Saved {}", self.path));
    }

    fn open_dialog(&mut self) {
        if let Some(path) = crate::files::pick_open() {
            self.open_path(&path);
        }
    }

    fn save_as(&mut self) {
        let start = with_extension(&self.path, "creabim");
        if let Some(path) = crate::files::pick_save(&start, "Save the project as", "creabim") {
            self.path = path;
            self.save();
        }
    }

    /// Bring a file's objects in as linked elements.
    fn link_file(&mut self, path: &str) {
        let full = std::fs::canonicalize(path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.to_string());
        if crate::files::linked_files(self.doc.model()).contains(&full) {
            self.reload(&full);
            return;
        }
        match crate::files::read_incoming(&full) {
            Ok(incoming) => {
                let (tx, report) = creabim_exchange::sync(self.doc.model(), &full, &incoming);
                let r = self.doc.commit(tx);
                self.watch.mark(&full);
                self.drawings.clear();
                self.frame_all();
                self.report(
                    r,
                    format!(
                        "Linked {}: {}. Pick an object and use Assign as to make it a wall, slab, door or window.",
                        file_label(&full),
                        report.summary()
                    ),
                );
            }
            Err(e) => self.status = format!("Couldn't read {path}: {e}"),
        }
    }

    /// Read a linked file again and update the elements it made.
    fn reload(&mut self, file: &str) {
        match crate::files::read_incoming(file) {
            Ok(incoming) => {
                let (tx, report) = creabim_exchange::sync(self.doc.model(), file, &incoming);
                self.watch.mark(file);
                if tx.ops.is_empty() {
                    self.status = format!("{} is up to date.", file_label(file));
                    return;
                }
                let r = self.doc.commit(tx);
                self.drop_stale_selection();
                self.report(
                    r,
                    format!("Reloaded {}: {}.", file_label(file), report.summary()),
                );
            }
            Err(e) => self.status = format!("Couldn't reload {file}: {e}"),
        }
    }

    /// "Assign as": give a mesh a building meaning, or change its class.
    fn assign_panel(&mut self, ui: &mut egui::Ui, id: ElementId) {
        let Some(e) = self.doc.model().get(id) else {
            return;
        };
        if !matches!(
            e.kind,
            ElementKind::FreeMesh(_)
                | ElementKind::Wall(_)
                | ElementKind::Slab(_)
                | ElementKind::Opening(_)
        ) {
            return;
        }
        if let Some(s) = &e.source {
            ui.label(
                egui::RichText::new(format!("From {}: {}", file_label(&s.file), s.object)).weak(),
            );
        }
        let is_mesh = matches!(e.kind, ElementKind::FreeMesh(_));
        let mut pick = None;
        egui::ComboBox::from_id_salt("assign_as")
            .icon(crate::props::combo_icon)
            .selected_text("Assign as…")
            .show_ui(ui, |ui| {
                for a in creabim_exchange::Assign::menu() {
                    if matches!(a, creabim_exchange::Assign::Class(_)) && !is_mesh {
                        continue;
                    }
                    if ui.selectable_label(false, a.label()).clicked() {
                        pick = Some(a);
                    }
                }
            });
        if is_mesh {
            ui.label(
                egui::RichText::new(
                    "Make this shape a wall, slab, door or window sized to fit it, or give it a class.",
                )
                .weak(),
            );
        }
        if let Some(a) = pick {
            let before = e.clone();
            match creabim_exchange::assign(self.doc.model(), id, &a) {
                Ok(after) => {
                    let r = self.doc.commit(
                        Transaction::new(format!("Assign as {}", a.label())).replace(before, after),
                    );
                    self.report(
                        r,
                        format!("Now a {}. Its handles resize it.", a.label().to_lowercase()),
                    );
                }
                Err(why) => self.status = why,
            }
        }
    }

    /// Sheets in number order, as (id, "A101 Floor Plans").
    pub(super) fn sheet_list(&self) -> Vec<(ElementId, String)> {
        sheets::sheets(self.doc.model())
            .into_iter()
            .map(|(id, s)| {
                let name = self.doc.model().get(id).map_or("", |e| e.name.as_str());
                (id, format!("{} {name}", s.number))
            })
            .collect()
    }

    pub(super) fn is_sheet(&self, id: ElementId) -> bool {
        sheets::sheet_of(self.doc.model(), id).is_some()
    }

    fn sheet_info(&self) -> SheetInfo {
        SheetInfo {
            project: stem(&self.path),
            date: sheets::today(),
        }
    }

    /// A sheet's views with their (cached) drawings.
    fn placed_views(&mut self, sheet: ElementId) -> Vec<PlacedView> {
        let ids: Vec<ElementId> = self
            .doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Viewport(vp) if vp.sheet == sheet => Some(vp.view),
                _ => None,
            })
            .collect();
        for id in ids {
            self.drawing(id);
        }
        let cache = &self.drawings;
        let model = self.doc.model();
        let rev = model.drawing_revision();
        let looks = &mut self.looks;
        sheets::placed_views_with(
            model,
            sheet,
            &mut |v| cache.get(&v).map(|(_, d)| d.clone()),
            &mut |id, v| match looks.get(&id) {
                Some((r, l)) if *r == rev => l.clone(),
                _ => {
                    let l = std::sync::Arc::new(creabim_geom::looks::Looks::for_view(model, id, v));
                    looks.insert(id, (rev, l.clone()));
                    l
                }
            },
        )
    }

    /// Every view in the model, plans first, in model order.
    fn views(&self) -> Vec<(ElementId, String, bool)> {
        // in the project browser's order: plans first, by level
        let model = self.doc.model();
        crate::browser::view_groups(model)
            .into_iter()
            .flat_map(|(g, ids)| {
                ids.into_iter().filter_map(move |id| {
                    let e = model.get(id)?;
                    Some((id, e.name.clone(), g == crate::browser::ViewGroup::Plans))
                })
            })
            .collect()
    }

    fn view(&self, id: ElementId) -> Option<&View> {
        match self.doc.model().get(id).map(|e| &e.kind) {
            Some(ElementKind::View(v)) => Some(v),
            _ => None,
        }
    }

    /// The drawing of a view, regenerated only when the model has changed.
    fn drawing(&mut self, id: ElementId) -> Option<Drawing> {
        let revision = self.doc.model().drawing_revision();
        let xray = self.plan.xray.contains(&id);
        let cache = if xray {
            &mut self.xray_drawings
        } else {
            &mut self.drawings
        };
        if let Some((r, d)) = cache.get(&id) {
            if *r == revision {
                return Some(d.clone());
            }
        }
        let t = std::time::Instant::now();
        let d = creabim_geom::section::view_drawing_with(self.doc.model(), id, xray)?;
        cache.insert(id, (revision, d.clone()));
        self.builds
            .record((id, xray), t.elapsed().as_secs_f64() * 1000.0);
        Some(d)
    }

    /// The drawing of a view to show now: as [`Self::drawing`] when it is
    /// up to date or cheap to build, else the last one finished while a
    /// worker builds the new one (see [`crate::builds`]). Exports and
    /// anything measured from a drawing use [`Self::drawing`].
    fn drawing_live(&mut self, id: ElementId) -> Option<Drawing> {
        let _pf = Perf::new("drawing_live: take finished");
        let finished = self.builds.finished();
        drop(_pf);
        for b in finished {
            let cache = if b.key.1 {
                &mut self.xray_drawings
            } else {
                &mut self.drawings
            };
            match b.drawing {
                // a drawing older than the one held (built on the spot meanwhile) is dropped
                Some(d) if cache.get(&b.key.0).is_none_or(|(r, _)| *r < b.revision) => {
                    cache.insert(b.key.0, (b.revision, d));
                }
                _ => {}
            }
        }
        let revision = self.doc.model().drawing_revision();
        let xray = self.plan.xray.contains(&id);
        let key = (id, xray);
        let held = if xray {
            self.xray_drawings.get(&id)
        } else {
            self.drawings.get(&id)
        };
        match held {
            Some((r, d)) if *r == revision => Some(d.clone()),
            // the view being worked in, and every plan while a plan grip
            // is dragged, are built on the spot: their lines must follow a
            // drag frame by frame. A plan (about 15 ms) beside a 3D drag
            // goes to a worker, which kept 3D drags at 60-80 ms a frame
            Some((_, d))
                if self.builds.enabled()
                    && !self.builds.cheap(key)
                    // a drag in 3D: every drawing, the one in front too
                    && (self.handle_drag.is_some()
                        || (Some(id) != self.active_view
                            && !(self.plan.grip.is_some()
                                && matches!(
                                    self.view(id).map(|v| &v.kind),
                                    Some(ViewKind::Plan { .. })
                                )))) =>
            {
                let d = d.clone();
                self.builds.start(self.doc.model(), key);
                Some(d)
            }
            _ => {
                let _pb = Perf::new("drawing_live: build here");
                self.drawing(id)
            }
        }
    }

    /// A grip is being dragged, in 3D or in a drawing.
    pub(crate) fn dragging(&self) -> bool {
        self.handle_drag.is_some() || self.plan.grip.is_some()
    }

    /// The box round the selected elements in 3D.
    fn selection_bounds(&self) -> Option<(GVec3, GVec3)> {
        self.scene
            .iter()
            .filter(|s| self.selection.contains(s.id))
            .filter_map(|s| s.mesh.bounds())
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    }

    /// Zoom and turn to the selection, keeping the direction looked from.
    fn frame_selection(&mut self) {
        match self.selection_bounds() {
            Some((lo, hi)) => {
                self.camera.frame(lo, hi, self.aspect);
                self.status = "Framed the selection.".into();
            }
            None if !self.selection.is_empty() => {
                self.status = "The selection isn't shown in this 3D view: its category is hidden here (voids and openings usually are). Show it from the view's visibility to frame it.".into()
            }
            None => self.status = "Select something to frame it.".into(),
        }
    }

    /// Orbit the 3D view by a pointer drag, about the selection when asked.
    fn orbit_3d(&mut self, d: Vec2) {
        if self.cube.grabbing() {
            // the drag started on the view cube, which turns the view itself
            return;
        }
        let d = GVec2::new(d.x, d.y);
        match self.selection_bounds().filter(|_| self.orbit_selection) {
            Some((lo, hi)) => self.camera.orbit_around((lo + hi) / 2.0, d),
            None => self.camera.orbit(d),
        }
    }

    /// Frame all in one pane only: the drawing (Home over it) or the 3D
    /// view (Home over it, or its toolbar), leaving the other where it is.
    fn frame_pane(&mut self, in_2d: bool) {
        if in_2d && self.rect_2d.is_some() {
            if let Some(v) = self.active_view {
                self.plan.cameras.remove(&v);
                self.plan.area_cameras.remove(&(self.areas.focus, v));
            }
            self.status = "Framed the drawing.".into();
        } else {
            self.frame_3d();
            self.status = "Framed the 3D view.".into();
        }
    }

    /// The view cube in the 3D view's corner: a click turns the view, a
    /// drag on it orbits, and a drag on its zoom or pan button zooms or
    /// pans.
    /// The references in 3D (MU32): each where its views show it, on the
    /// plan's level or standing on the section's plane, faint as in the
    /// views. A picture shows its frame.
    /// Where Divide will cut, while its window is open (MU36): a mark at
    /// each division on the selected element's length or height.
    fn draw_divide_preview(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) {
        let accent = crate::planview::accent_selected();
        for (a, b, _) in &self.plan.divide_marks {
            if let (Some(pa), Some(pb)) = (project(*a), project(*b)) {
                if self.plan.divide_guides {
                    let ink = egui::Stroke::new(1.5, crate::planview::GUIDE_COLOR);
                    crate::snaps::dashed_clipped(painter, &[pa, pb], ink, 8.0, 5.0);
                } else {
                    painter.line_segment([pa, pb], egui::Stroke::new(2.0, accent));
                }
            }
        }
        let mut taken = vec![];
        for (at, text) in &self.plan.divide_labels {
            if let Some(q) = project(*at) {
                crate::planview::divide_label(painter, q, text, accent, &mut taken);
            }
        }
        crate::modguides::draw(painter, &self.plan.mod_guides, accent, false, project);
        // what a plan's pointer would place, here in 3D too
        if let Some((from, g)) = self
            .plan
            .ghost_shared
            .as_ref()
            .filter(|(from, _)| *from != Some(usize::MAX))
        {
            let _ = from;
            crate::ghost::draw(painter, project, g, crate::theme::interaction());
        }
    }

    /// The guides of the modifier card last worked in, while its mass is
    /// the one selected and the modifiers tab is open (MU39).
    fn modifier_guides(&mut self) -> crate::modguides::Guides {
        let Some((id, i)) = self.mod_focus else {
            return Default::default();
        };
        if self.selection.active != Some(id) || self.prop_tab != PropTab::Modifiers {
            self.mod_focus = None;
            return Default::default();
        }
        let model = self.doc.model();
        let Some(ElementKind::FreeMesh(m)) = model.get(id).map(|e| &e.kind) else {
            return Default::default();
        };
        let Some(md) = m.modifiers.get(i).filter(|md| md.enabled) else {
            return Default::default();
        };
        let z = m
            .level
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation);
        let base = m.origin + DVec3::Z * z;
        crate::modguides::of(m, &md.kind, &|v| base + m.to_model(v))
    }

    /// Start Split on the selection (MU39): next, a line to cut along.
    pub(super) fn start_split(&mut self) {
        let targets = self.selection.all();
        if targets.is_empty() {
            self.status =
                "Split: select what to cut first (walls, slabs, parts, sketches, masses).".into();
            return;
        }
        self.split = Some(SplitRun {
            targets,
            a: None,
            line: None,
        });
        self.plan.point_pick = true;
        self.plan.point_lines = true;
        self.status = "Split: click a guide, a line or an edge to cut along, or click two points. Esc leaves.".into();
    }

    /// Split's clicks as they come, and what it shows meanwhile.
    fn split_steps(&mut self, ctx: &egui::Context) {
        self.plan.point_from = None;
        let Some(mut run) = self.split.take() else {
            return;
        };
        let picked = self.plan.point_picked.take();
        let line = self.plan.point_line_picked.take();
        // Esc or a right-click in a view ends it (a click taken this frame
        // is not a leaving)
        let left = !self.plan.point_pick && picked.is_none() && line.is_none();
        if left || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.plan.point_pick = false;
            self.plan.point_lines = false;
            self.plan.mod_guides = Default::default();
            self.status = "Split left.".into();
            return;
        }
        self.plan.point_pick = true;
        use creabim_core::split::Line;
        match (run.line, run.a, line, picked) {
            (None, _, Some((a, b)), _) => {
                run.line = Some(Line { a, b });
                run.a = None;
            }
            (None, None, None, Some(p)) => {
                run.a = Some(p);
                self.status = "Split: click the line's second point.".into();
            }
            (None, Some(a), None, Some(p)) if p.distance(a) > 1e-6 => {
                run.line = Some(Line { a, b: p });
                run.a = None;
            }
            (Some(cut), _, _, Some(p)) => {
                self.do_split(&run, cut, p);
                return;
            }
            _ => {}
        }
        if run.line.is_some() {
            self.plan.point_lines = false;
            self.status = "Split: click the side to keep selected. Esc leaves.".into();
        }
        self.plan.point_from = run.a;
        self.plan.mod_guides = self.split_guides(&run);
        self.split = Some(run);
    }

    /// The cutting plane in red, or the line being drawn.
    fn split_guides(&self, run: &SplitRun) -> crate::modguides::Guides {
        let mut g = crate::modguides::Guides {
            ink: Some(crate::planview::SPLIT_RED),
            ..Default::default()
        };
        // as high and as far as what it cuts, and a little more
        let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        for item in self.scene.iter().filter(|i| run.targets.contains(&i.id)) {
            if let Some((a, b)) = item.mesh.bounds() {
                lo = lo.min(a.as_dvec3());
                hi = hi.max(b.as_dvec3());
            }
        }
        if lo.x > hi.x {
            let z = self.work_level().map_or(0.0, |(_, z)| z);
            lo = DVec3::new(-5.0, -5.0, z);
            hi = DVec3::new(5.0, 5.0, z + 3.0);
        }
        let (z0, z1) = (lo.z - 0.3, hi.z + 0.3);
        let reach = (hi - lo).truncate().length() / 2.0 + 1.0;
        let mid = ((lo + hi) / 2.0).truncate();
        match (run.line, run.a, self.plan.point_hover) {
            (Some(l), _, _) => {
                let d = (l.b - l.a).normalize_or_zero();
                let c = l.a + d * d.dot(mid - l.a);
                let (p, q) = (c - d * reach, c + d * reach);
                g.planes
                    .push([p.extend(z0), q.extend(z0), q.extend(z1), p.extend(z1)]);
                g.lines.push((p.extend(z0), q.extend(z0)));
            }
            (None, Some(a), Some(h)) => {
                g.lines
                    .push((a.extend(z0.max(lo.z)), h.extend(z0.max(lo.z))));
                g.points.push(a.extend(lo.z));
            }
            _ => {}
        }
        g
    }

    /// Cut, then keep selected what lies on the side clicked.
    fn do_split(&mut self, run: &SplitRun, cut: creabim_core::split::Line, side_at: DVec2) {
        self.plan.point_pick = false;
        self.plan.point_lines = false;
        self.plan.mod_guides = Default::default();
        match creabim_core::split::split(self.doc.model(), &run.targets, cut) {
            Ok(s) => {
                let mut pieces: Vec<ElementId> = run.targets.clone();
                for op in &s.tx.ops {
                    if let creabim_core::Op::Insert(e) = op {
                        pieces.push(e.id);
                    }
                }
                let label = s.tx.label.clone();
                if let Err(e) = self.doc.commit(s.tx) {
                    self.status = format!("Couldn't split: {e}");
                    return;
                }
                let want = cut.side(side_at) > 0.0;
                let model = self.doc.model();
                let kept: Vec<ElementId> = pieces
                    .into_iter()
                    .filter(|id| {
                        creabim_core::transform::pivot(model, *id)
                            .is_some_and(|p| (cut.side(p.truncate()) > 0.0) == want)
                    })
                    .collect();
                self.selection.clear();
                for id in &kept {
                    self.selection.toggle(*id);
                }
                self.face = None;
                self.status = format!(
                    "{label}: the {} on the side you clicked selected. Ctrl+Z to undo.{}",
                    if kept.len() == 1 {
                        "piece".to_string()
                    } else {
                        format!("{} pieces", kept.len())
                    },
                    if s.skipped.is_empty() {
                        String::new()
                    } else {
                        format!(" Not cut: {}.", s.skipped.join("; "))
                    }
                );
            }
            Err(e) => self.status = e,
        }
    }

    /// A centre picked in a plan for the radial array being set.
    fn take_picked_centre(&mut self) {
        let Some(w) = self.plan.point_picked.take() else {
            return;
        };
        let Some((id, i)) = self.mod_focus else {
            return;
        };
        let model = self.doc.model();
        let Some(ElementKind::FreeMesh(m)) = model.get(id).map(|e| &e.kind) else {
            return;
        };
        // the model point in the mesh's own axes (its turn and scale undone)
        let local = m.to_local((w - m.origin.truncate()).extend(0.0));
        let r = self.doc.edit(id, "Set radial centre", |e| {
            if let ElementKind::FreeMesh(m) = &mut e.kind {
                if let Some(creabim_core::Modifier {
                    kind: creabim_core::ModifierKind::RadialArray { center, .. },
                    ..
                }) = m.modifiers.get_mut(i)
                {
                    *center = local.truncate();
                }
            }
        });
        self.report(
            r,
            format!(
                "Centre set at {}, {}.",
                crate::units::len(local.x),
                crate::units::len(local.y)
            ),
        );
    }

    /// The references shown in 3D: not hidden, and not kept to their views.
    fn refs_in_3d(&self) -> Vec<(ElementId, creabim_core::Reference)> {
        self.doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Reference(r)
                    if r.shows_in_3d()
                        && (if self.camera.ortho {
                            r.in_orthographic
                        } else {
                            r.in_perspective
                        })
                        && !self.hidden.contains(&e.id) =>
                {
                    // its own strength in 3D, when it has one
                    let mut r = r.clone();
                    r.opacity = r.opacity_3d.unwrap_or(r.opacity);
                    Some((e.id, r))
                }
                _ => None,
            })
            .collect()
    }

    /// The reference whose line (or a picture's frame) is within 6 px of
    /// `p` in 3D.
    fn reference_near_3d(
        &mut self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) -> Option<ElementId> {
        let near = |a: egui::Pos2, b: egui::Pos2| {
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
            (a + ab * t).distance(p) < 6.0
        };
        for (id, r) in self.refs_in_3d() {
            let model = self.doc.model();
            let world = |q: DVec2| crate::handles::ref_world(model, &r, q);
            let polys: Vec<Vec<DVec2>> = match self.plan.ref_cache.get(model, id, &r) {
                crate::trace::Shown::Lines(l) => l
                    .iter()
                    .map(|l| l.iter().map(|q| r.place(*q)).collect())
                    .collect(),
                _ => r
                    .outline()
                    .map(|c| vec![c.iter().chain(c.first()).copied().collect()])
                    .unwrap_or_default(),
            };
            for l in polys {
                let pts: Vec<egui::Pos2> = l.iter().filter_map(|q| project(world(*q))).collect();
                if pts.windows(2).any(|w| near(w[0], w[1])) {
                    return Some(id);
                }
            }
        }
        None
    }

    fn draw_references_3d(
        &mut self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) {
        let refs = self.refs_in_3d();
        // a picked one is outlined as the plans outline it: dashed, in the
        // selection orange, on its own plane (MU38)
        let picked: Vec<Vec<egui::Pos2>> = refs
            .iter()
            .filter(|(id, _)| self.selection.contains(*id))
            .filter_map(|(_, r)| {
                let c = r.outline()?;
                let model = self.doc.model();
                let pts: Vec<egui::Pos2> = c
                    .iter()
                    .chain(c.first())
                    .filter_map(|p| project(crate::handles::ref_world(model, r, *p)))
                    .collect();
                (pts.len() == 5).then_some(pts)
            })
            .collect();
        self.draw_reference_lines_3d(painter, project, refs);
        for pts in picked {
            crate::snaps::dashed_clipped(
                painter,
                &pts,
                egui::Stroke::new(1.5, crate::planview::accent_selected()),
                6.0,
                4.0,
            );
        }
    }

    fn draw_reference_lines_3d(
        &mut self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        refs: Vec<(ElementId, creabim_core::Reference)>,
    ) {
        // a huge drawing is cut short rather than slowing the 3D view
        let mut budget = 30_000usize;
        for (id, r) in refs {
            let model = self.doc.model();
            let world = |p: DVec2| crate::handles::ref_world(model, &r, p);
            let ink = crate::trace::colour(&r)
                .unwrap_or(Color32::from_gray(150))
                .gamma_multiply(r.opacity.clamp(0.15, 1.0));
            let stroke = egui::Stroke::new(1.0, ink);
            match self.plan.ref_cache.get(model, id, &r) {
                crate::trace::Shown::Lines(lines) => {
                    for l in lines.iter() {
                        if budget == 0 {
                            return;
                        }
                        let pts: Vec<egui::Pos2> = l
                            .iter()
                            .filter_map(|p| project(world(r.place(*p))))
                            .collect();
                        budget = budget.saturating_sub(pts.len());
                        if pts.len() >= 2 {
                            painter.add(egui::Shape::line(pts, stroke));
                        }
                    }
                }
                // a picture: its pixels on its plane, in a grid of small
                // pieces so its perspective holds (MU34)
                crate::trace::Shown::Picture(raster) => {
                    let Some([lo, hi]) = r.bounds else { continue };
                    let tex = crate::textures::texture(painter.ctx(), &raster);
                    let tint = crate::trace::colour(&r)
                        .unwrap_or(Color32::WHITE)
                        .gamma_multiply(r.opacity.clamp(0.0, 1.0));
                    const N: usize = 12;
                    let mut mesh = egui::Mesh::with_texture(tex);
                    let mut ok = true;
                    for j in 0..=N {
                        for i in 0..=N {
                            let (fu, fv) = (i as f64 / N as f64, j as f64 / N as f64);
                            let q =
                                DVec2::new(lo.x + (hi.x - lo.x) * fu, lo.y + (hi.y - lo.y) * fv);
                            let Some(pos) = project(world(r.place(q))) else {
                                ok = false;
                                break;
                            };
                            mesh.vertices.push(egui::epaint::Vertex {
                                pos,
                                uv: egui::pos2(fu as f32, 1.0 - fv as f32),
                                color: tint,
                            });
                        }
                    }
                    if ok {
                        for j in 0..N as u32 {
                            for i in 0..N as u32 {
                                let a = j * (N as u32 + 1) + i;
                                let (b, c, d) = (a + 1, a + N as u32 + 2, a + N as u32 + 1);
                                mesh.indices.extend([a, b, c, a, c, d]);
                            }
                        }
                        painter.add(egui::Shape::mesh(mesh));
                    }
                }
                _ => {
                    if let Some(c) = r.outline() {
                        let pts: Vec<egui::Pos2> = c
                            .iter()
                            .chain(c.first())
                            .filter_map(|p| project(world(*p)))
                            .collect();
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0, ink)));
                    }
                }
            }
        }
    }

    fn view_cube(&mut self, ui: &mut egui::Ui, rect: Rect) {
        use crate::viewcube::{said, target_of, Nav};
        let accent = crate::theme::Palette::of(ui.ctx()).accent;
        let (pick, nav) = self
            .cube
            .ui(ui, rect, &self.camera, accent, self.camera.ortho);
        match nav {
            Some(Nav::Orbit(d)) => self.camera.orbit(GVec2::new(d.x, d.y)),
            // the view moves with the pointer, as a right-button drag does
            Some(Nav::Pan(d)) => self.camera.pan(GVec2::new(d.x, d.y), rect.height()),
            // up zooms in
            Some(Nav::Zoom(d)) => self.camera.zoom(-d.y / 150.0),
            None => {}
        }
        let Some(pick) = pick else {
            return;
        };
        match pick {
            crate::viewcube::Pick::FitAll => {
                self.frame_3d();
                self.status = "Framed everything.".into();
                ui.ctx().request_repaint();
                return;
            }
            crate::viewcube::Pick::FitSel => {
                if self.selection.is_empty() {
                    self.status = "Select something to frame first.".into();
                } else {
                    self.frame_selection();
                    self.status = "Framed the selection.".into();
                }
                ui.ctx().request_repaint();
                return;
            }
            _ => {}
        }
        if pick == crate::viewcube::Pick::Projection {
            self.camera.ortho = !self.camera.ortho;
            self.status = if self.camera.ortho {
                "Orthographic: parallel lines stay parallel.".into()
            } else {
                "Perspective.".into()
            };
            ui.ctx().request_repaint();
            return;
        }
        let now = ui.input(|i| i.time);
        let to = match target_of(pick, &self.camera) {
            Some(to) => to,
            None => {
                let (yaw, pitch) = (self.camera.yaw, self.camera.pitch);
                self.frame_3d();
                (self.camera.yaw, self.camera.pitch) = (yaw, pitch);
                let mut iso = self.camera.clone();
                iso.look(crate::camera::Look::Iso);
                (iso.yaw, iso.pitch)
            }
        };
        self.cube.turn_to(&self.camera, to, now);
        self.status = said(pick);
        ui.ctx().request_repaint();
    }

    /// Fit the 3D view round the whole model.
    fn frame_3d(&mut self) {
        // TEMP-DIAG: synchronous full-scene build on the UI thread.
        eprintln!("[diag] 3d: frame_3d start");
        let t = std::time::Instant::now();
        let scene = (*creabim_geom::shared_scene(self.doc.model())).clone();
        eprintln!(
            "[diag] 3d: frame_3d scene ready in {:.0} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
        let bounds = scene
            .iter()
            .filter_map(|s| s.mesh.bounds())
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
        if let Some((lo, hi)) = bounds {
            self.camera.frame(lo, hi, self.aspect);
        }
    }

    /// Every view framed afresh, as when a project opens.
    fn frame_all(&mut self) {
        self.frame_3d();
        self.plan.cameras.clear();
    }

    fn report(&mut self, r: Result<(), impl ToString>, ok: impl Into<String>) {
        self.status = match r {
            Ok(()) => ok.into(),
            Err(e) => format!("Couldn't do that: {}", e.to_string()),
        };
    }

    /// Run an edit picked from a key, a button or a menu. `at` is where a
    /// right-click menu was opened.
    fn run_command(&mut self, ctx: &egui::Context, c: Command, at: Option<DVec3>) {
        // a Masters view in front: Move, Rotate, Duplicate and Delete act
        // on what is picked on the master (MU40)
        if self.masters_command(c) {
            return;
        }
        if let Some(m) = c.modal() {
            self.arm(c, m);
            return;
        }
        match c {
            Command::Copy | Command::Cut => {
                if self.copy_selected() {
                    let n = self.clipboard.len();
                    ctx.copy_text(self.clipboard_text());
                    if c == Command::Cut {
                        self.delete_selected();
                        self.status = format!("Cut {n} elements. Ctrl+V pastes them back.");
                    } else {
                        self.status = format!("Copied {n} elements. Ctrl+V pastes them.");
                    }
                }
            }
            Command::Paste => {
                if self.clipboard.is_empty() {
                    self.status = "Nothing copied yet.".into();
                } else {
                    self.paste(self.clipboard.clone());
                }
            }
            // in Edit mode Delete takes the picked corners away, never the
            // element being edited
            Command::Delete if self.plan.edit.is_some() => self.delete_grips(),
            Command::Delete => self.delete_selected(),
            Command::Hide => self.hide_selected(),
            Command::Isolate => self.isolate_selected(),
            Command::ShowAll => self.show_all(),
            Command::SelectAll => self.select_all(),
            Command::SelectNone => {
                self.selection.clear();
                self.face = None;
            }
            Command::Undo => {
                let label = self.doc.undo_label().map(str::to_owned);
                if self.doc.undo() {
                    self.status = format!("Undid: {}", label.unwrap_or_default());
                    self.drop_stale_selection();
                }
            }
            Command::Redo => {
                let label = self.doc.redo_label().map(str::to_owned);
                if self.doc.redo() {
                    self.status = format!("Redid: {}", label.unwrap_or_default());
                    self.drop_stale_selection();
                }
            }
            Command::CursorHere => {
                if let Some(p) = at {
                    self.plan.cursor = Some(p);
                    self.plan.pivot = crate::modal::Pivot::Cursor;
                    self.status = format!(
                        "Pivot cursor at {:.2}, {:.2}, {:.2}. Rotate and scale now turn about it.",
                        p.x, p.y, p.z
                    );
                }
            }
            Command::WorkplaneHere => {
                // from a plan: a level plane through the point
                if let Some(p) = at {
                    let plane = Workplane {
                        origin: p,
                        ..Workplane::ground(p.z)
                    };
                    self.set_workplane(plane, format!("level at {}", crate::units::elevation(p.z)));
                }
            }
            Command::Move
            | Command::Rotate
            | Command::Scale
            | Command::Mirror
            | Command::Offset
            | Command::Duplicate => {}
        }
    }

    /// Get Move, Rotate, Scale or Duplicate ready: the view the pointer
    /// goes into starts it. With nothing selected, Move and Rotate first
    /// ask what to work on.
    fn arm(&mut self, c: Command, m: (crate::modal::Mode, bool)) {
        use crate::modal::Mode;
        let picks_first = matches!(m, (Mode::Move | Mode::Rotate, false));
        if self.selection.is_empty() && !picks_first {
            self.status = "Select something first.".into();
            return;
        }
        if let Some(old) = self.plan.modal.as_mut() {
            old.discard(&mut self.doc);
            self.plan.modal = None;
        }
        self.plan.cancel();
        self.plan.tool = Tool::Select;
        self.plan.armed = Some(m);
        self.status = match (m.0, self.selection.is_some()) {
            (Mode::Move, false) => {
                "Move: click what to move (Shift+click adds more), then click the base point. Esc cancels.".into()
            }
            (Mode::Rotate, false) => {
                "Rotate: click what to rotate (Shift+click adds more), then click the centre. Esc cancels.".into()
            }
            (Mode::Move, true) => {
                "Move: click the base point in a view, then where it goes. Esc steps back.".into()
            }
            (Mode::Rotate, true) => {
                "Rotate: click the centre in a view, then a reference direction, then the new direction.".into()
            }
            (Mode::Mirror, _) => {
                "Mirror: click the first point of the line to mirror across, then the second. In 3D it lies on the workplane (a face, or the level). A mirrored copy is made; Ctrl keeps only the mirrored one.".into()
            }
            (Mode::Offset, _) => {
                "Offset: point at a view and move the pointer to the side to offset to, or type a distance; click to place a copy. Ctrl moves the original instead.".into()
            }
            _ if c == Command::Duplicate => {
                "Duplicate: point at a view and click a base point; the copy follows to where you click next. Esc takes the copy away.".into()
            }
            _ => format!(
                "{}: point at a view and move the pointer, then click to finish. Right-click cancels.",
                c.label()
            ),
        };
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // a length typed into a grip's drag takes the keys
        if ctx.egui_wants_keyboard_input() || self.plan.grips.typing {
            return;
        }
        // over a Profile view its own keys win (M, R, F, D, Delete…): the
        // model's would act on the model behind it (MU37: Shift+D started a
        // dimension); undo and redo still work
        if self.profile_view_hovered(ctx) || self.masters_view_hovered(ctx) {
            let (undo, redo) = ctx.input(|i| {
                let cmd = i.modifiers.command;
                (
                    cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                    cmd && (i.key_pressed(egui::Key::Y)
                        || (i.modifiers.shift && i.key_pressed(egui::Key::Z))),
                )
            });
            if undo {
                self.run_command(ctx, Command::Undo, None);
            }
            if redo {
                self.run_command(ctx, Command::Redo, None);
            }
            return;
        }
        // Alt+S: grid snap on or off, as in ArchiCAD
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::S)) {
            let s = &mut self.plan.snap;
            s.increment = !s.increment;
            self.status = if s.increment {
                format!(
                    "Grid snap on ({}): points land on its grid. Alt+S turns it off.",
                    s.spacing_label()
                )
            } else {
                "Grid snap off: points and drags go exactly where the pointer is.".into()
            };
        }
        let editing = self.plan.edit.is_some();
        let (undo, redo, delete, extrude) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                cmd && (i.key_pressed(egui::Key::Y)
                    || (i.modifiers.shift && i.key_pressed(egui::Key::Z))),
                // in Edit mode Backspace removes picked corners too
                i.key_pressed(egui::Key::Delete)
                    || (editing && i.key_pressed(egui::Key::Backspace)),
                !cmd && i.key_pressed(egui::Key::E),
            )
        });
        // F1: the guide's page about what you are doing
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.run_action(ctx, "help");
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F3)) {
            self.plan.snap.on = !self.plan.snap.on;
            self.status = format!(
                "Snaps {}. F3 turns them {}.",
                if self.plan.snap.on { "on" } else { "off" },
                if self.plan.snap.on { "off" } else { "on" },
            );
        }
        // Shift+G: guides on and off everywhere; Alt+Z: X-ray in 3D
        let (guides_key, xray_key) = ctx.input(|i| {
            let m = i.modifiers;
            (
                m.shift && !m.command && !m.alt && i.key_pressed(egui::Key::G),
                m.alt && !m.command && !m.shift && i.key_pressed(egui::Key::Z),
            )
        });
        if guides_key && self.plan.modal.is_none() {
            self.set_guides_hidden(!self.shell.settings.guides_hidden);
        }
        // which pane Alt+Z works in: the one the pointer is (or was last) over
        if let Some(p) = ctx.pointer_hover_pos() {
            if self.rect_2d.is_some_and(|r| r.contains(p)) {
                self.last_in_2d = true;
            } else if self.rect_3d.is_some_and(|r| r.contains(p)) {
                self.last_in_2d = false;
            }
        }
        if xray_key {
            match self.xray_view_target() {
                Some(view) => self.toggle_view_xray(view),
                None => self.set_xray(!self.shell.settings.xray),
            }
        }
        // Ctrl+[ folds the browser, Ctrl+] Properties (MU27: T and N, as
        // Blender's, are Tag and Note here)
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::OpenBracket)) {
            self.shell.settings.left_folded = !self.shell.settings.left_folded;
            self.shell.settings.store();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::CloseBracket)) {
            self.shell.settings.right_folded = !self.shell.settings.right_folded;
            self.shell.settings.store();
        }
        // Tab: Object and Edit mode, as Blender (MU27); 1, 2, 3 in Edit mode:
        // points, edges, faces
        {
            let free = self.plan.tool == Tool::Select
                && self.plan.modal.is_none()
                && !self.plan.busy()
                && !ctx.egui_wants_keyboard_input();
            let layered = self
                .plan
                .edit
                .is_some_and(|id| handles::has_layers(self.doc.model(), id));
            // Tab still cycles what lies under the pointer when there are
            // several things there (MU25); otherwise it switches the mode
            let cycling = self.plan.hover.as_ref().is_some_and(|h| h.under.len() >= 2);
            // in Edit mode Tab always leaves it (a layered element's layers
            // are Shift+Tab now, MU39)
            let _ = layered;
            if free
                && (self.plan.edit.is_some() || !cycling)
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
            {
                self.toggle_edit_mode();
            }
            if free && self.plan.edit.is_some() {
                for (k, key) in [
                    (1u8, egui::Key::Num1),
                    (2, egui::Key::Num2),
                    (3, egui::Key::Num3),
                ] {
                    if ctx.input(|i| i.modifiers.is_none() && i.key_pressed(key)) {
                        handles::set_pick(if handles::pick() == k { 0 } else { k });
                        self.status = match handles::pick() {
                            1 => "Edit mode: points.".into(),
                            2 => "Edit mode: edges.".into(),
                            3 => "Edit mode: faces.".into(),
                            _ => "Edit mode: everything.".into(),
                        };
                    }
                }
            }
        }
        // Enter opens the selected element in Edit mode; only Tab (or the
        // mode menu) closes it, as Blender's (MU39)
        let enter = ctx.input(|i| i.modifiers.is_none() && i.key_pressed(egui::Key::Enter));
        if enter
            && self.plan.edit.is_none()
            && self.plan.tool == Tool::Select
            && self.plan.modal.is_none()
            && !self.plan.busy()
            && self.plan.outline.is_none()
        {
            self.toggle_edit_mode();
        }
        // the Annotate board's keys (MU24): D Dimension, T Tag, N Note;
        // Shift+A the Add menu, with nothing selected (with a selection it
        // keeps selecting all of its type); PgUp / PgDn the previous and
        // next sheet, or view of the same kind
        if !ctx.egui_wants_keyboard_input() && self.plan.modal.is_none() && self.add_menu.is_none()
        {
            let (d, t, n, add, prev, next) = ctx.input(|i| {
                let m = i.modifiers;
                let plain = !m.command && !m.alt && !m.shift;
                (
                    plain && i.key_pressed(egui::Key::D),
                    plain && i.key_pressed(egui::Key::T),
                    plain && i.key_pressed(egui::Key::N),
                    m.shift && !m.command && !m.alt && i.key_pressed(egui::Key::A),
                    plain && i.key_pressed(egui::Key::PageUp),
                    plain && i.key_pressed(egui::Key::PageDown),
                )
            });
            let tag_all = ctx.input(|i| {
                i.modifiers.shift
                    && !i.modifiers.command
                    && !i.modifiers.alt
                    && i.key_pressed(egui::Key::T)
            });
            if tag_all {
                self.run_action(ctx, "tag.all");
            }
            let empty = self.selection.is_empty();
            // (with a selection, D toggles its grips' live dimensions)
            if d && empty {
                self.pick_tool(ctx, Tool::Dimension);
            }
            if t {
                self.pick_tool(ctx, Tool::Tag);
            }
            if n {
                self.pick_tool(ctx, Tool::Note);
            }
            if add && empty {
                self.add_menu = Some(String::new());
            }
            if prev || next {
                self.step_view(if next { 1 } else { -1 });
            }
        }
        self.add_menu_window(ctx);
        // Ctrl+P: this sheet as a PDF; Ctrl+Shift+P: Export sheets (MU22)
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::P,
            )
        }) {
            self.sheets_export_open = true;
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
            if self.active_view.is_some_and(|id| self.is_sheet(id)) {
                self.export_sheets_now(true);
            } else {
                self.sheets_export_open = true;
            }
        }
        // Alt+F2 shows or hides the active view's references (MU18/MU19)
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::F2)) {
            if let Some(v) = self.active_view {
                self.toggle_references(v);
            }
        }
        // Tab in Edit mode on a layered element: its layers' ends, and back
        if let Some(id) = self.plan.edit {
            let free = self.handle_drag.is_none()
                && !self.plan.gripping()
                && !self.plan.grips.typing
                && !ctx.egui_wants_keyboard_input();
            if free && handles::has_layers(self.doc.model(), id) {
                let tab = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
                if tab {
                    let on = !handles::layers();
                    handles::set_layers(on);
                    self.status = if on {
                        match self.doc.model().get(id).map(|e| &e.kind) {
                            Some(ElementKind::Wall(_)) => "Layers: drag a layer's end to run it on or hold it back. Shift+Tab: back to the element's grips.",
                            _ => "Layers: drag a layer's edge to push it out past the edge or pull it in. Shift+Tab: back to the element's grips.",
                        }
                        .into()
                    } else {
                        editmode::enter(self.doc.model(), id, &mut self.plan.edit)
                    };
                }
            }
        }
        // M: the Move tool
        let m_key = ctx.input(|i| {
            !i.modifiers.command
                && !i.modifiers.alt
                && !i.modifiers.shift
                && i.key_pressed(egui::Key::M)
        });
        if m_key && self.plan.modal.is_none() && !self.plan.busy() {
            self.run_command(ctx, Command::Move, None);
        }
        // Space flips: the wall being drawn, else the selected walls, doors
        // and windows; Shift+Space a door's hand
        let flip = ctx.input(|i| {
            // Ctrl+Space shows an area alone instead
            let ctrl = i.events.iter().any(|e| {
                matches!(e, egui::Event::Key { key: egui::Key::Space, modifiers, .. } if modifiers.command)
            });
            (!ctrl && !i.modifiers.command && !i.modifiers.alt && i.key_pressed(egui::Key::Space))
                .then_some(i.modifiers.shift)
        });
        if let Some(hand) = flip.filter(|_| self.plan.modal.is_none()) {
            if self.plan.tool == Tool::Wall && !hand {
                self.wall_action("wall.flipnew", "");
            } else if !self.plan.busy() {
                self.flip_selected(hand);
            }
        }
        let (settings, shortcuts) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                cmd && i.key_pressed(egui::Key::Comma),
                cmd && i.key_pressed(egui::Key::Slash),
            )
        });
        if settings {
            self.settings_open = !self.settings_open;
        }
        if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::N)) {
            self.open_new_project(ctx);
        }
        if shortcuts {
            self.open_help("shortcuts");
        }
        let (palette, next_theme, home, frame_sel) = ctx.input(|i| {
            let (cmd, shift) = (i.modifiers.command, i.modifiers.shift);
            (
                cmd && i.key_pressed(egui::Key::K),
                cmd && shift && i.key_pressed(egui::Key::T),
                i.key_pressed(egui::Key::Home),
                !cmd && i.key_pressed(egui::Key::Period),
            )
        });
        if palette && self.shell.web.is_some() {
            self.open_palette(ctx);
        }
        if next_theme {
            let t = self.shell.theme.next();
            self.set_theme(ctx, t);
        }
        if self.plan.modal.is_none() {
            if home {
                self.frame_pane(self.last_in_2d);
            }
            if frame_sel {
                self.frame_selection();
            }
        }
        // in a text field, Ctrl+A and the letter keys are the field's own
        let typing = ctx.egui_wants_keyboard_input();
        let (all, none, hide, isolate, show, events) = ctx.input(|i| {
            let (cmd, alt, shift) = (i.modifiers.command, i.modifiers.alt, i.modifiers.shift);
            let h = !typing && i.key_pressed(egui::Key::H);
            let a = !typing && i.key_pressed(egui::Key::A);
            (
                cmd && !shift && a,
                alt && a,
                h && !cmd && !alt && !shift,
                h && shift && !cmd && !alt,
                h && alt,
                i.events.clone(),
            )
        });
        let (open, save) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                cmd && i.key_pressed(egui::Key::O),
                cmd && i.key_pressed(egui::Key::S),
            )
        });
        if open {
            self.open_dialog();
        }
        if save {
            self.save();
        }
        if all {
            self.select_all();
        }
        // Shift+A: all of the selection's type in the view; Ctrl+Shift+A
        // in the project
        let similar = ctx.input(|i| {
            let m = i.modifiers;
            (!typing && m.shift && !m.alt && i.key_pressed(egui::Key::A)).then_some(m.command)
        });
        if let Some(project) =
            similar.filter(|_| self.plan.tool == Tool::Select && !self.selection.is_empty())
        {
            self.shell.context_3d = !self.last_in_2d;
            self.select_similar(if project { "type.project" } else { "type.view" });
        }
        if none {
            self.selection.clear();
            self.face = None;
        }
        if hide {
            self.hide_selected();
        }
        if isolate {
            self.isolate_selected();
        }
        if show {
            self.show_all();
        }
        for e in &events {
            match e {
                egui::Event::Copy => self.run_command(ctx, Command::Copy, None),
                egui::Event::Cut => self.run_command(ctx, Command::Cut, None),
                egui::Event::Paste(text) => self.paste_text(text),
                _ => {}
            }
        }
        if undo {
            self.run_command(ctx, Command::Undo, None);
        }
        if redo {
            self.run_command(ctx, Command::Redo, None);
        }
        if delete {
            self.run_command(ctx, Command::Delete, None);
        }
        if extrude {
            let sketch = self
                .selection
                .active
                .and_then(|id| self.doc.model().get(id))
                .is_some_and(|e| matches!(e.kind, ElementKind::Sketch(_)));
            if sketch {
                self.extrude_sketch();
            } else {
                self.extrude_face();
            }
        }
        self.drop_stale_selection();
    }

    fn drop_stale_selection(&mut self) {
        let before = self.selection.active;
        let model = self.doc.model();
        self.selection.retain(|id| model.get(id).is_some());
        if self.selection.active != before {
            self.face = None;
        }
        if self
            .active_view
            .is_none_or(|id| self.view(id).is_none() && !self.is_sheet(id))
        {
            self.active_view = self.views().first().map(|v| v.0);
        }
    }

    fn delete_selected(&mut self) {
        let ids = self.selection.all();
        let Some(&last) = ids.last() else {
            return;
        };
        // a level asks first what happens to what is on it
        if let Some(level) = ids.iter().find(|id| self.doc.model().level(**id).is_some()) {
            self.ask_delete_level(*level);
            return;
        }
        let name = match ids.len() {
            1 => self
                .doc
                .model()
                .get(last)
                .map(|e| e.name.clone())
                .unwrap_or_default(),
            n => format!("{n} elements"),
        };
        let tx =
            Transaction::new(format!("Delete {name}")).remove_all_cascade(self.doc.model(), &ids);
        let r = self.doc.commit(tx);
        self.report(r, format!("Deleted {name} and anything attached to it."));
        self.drop_stale_selection();
    }

    /// Everything that can be picked: building elements, sketches, guides,
    /// annotations, but not levels, views, sheets or materials.
    /// Select what is not selected, and drop what is (MU39).
    pub(super) fn select_invert(&mut self) {
        let was = self.selection.clone();
        let ids: Vec<ElementId> = self
            .selectable_ids()
            .into_iter()
            .filter(|id| !was.contains(*id))
            .collect();
        self.selection.extend(ids, false);
        self.face = None;
        self.status = format!("Inverted: {} selected.", self.selection.len());
    }

    /// Add what is joined to the selection: walls meeting it end to end,
    /// and the doors, windows and holes set in it (MU39).
    pub(super) fn select_linked(&mut self) {
        let model = self.doc.model();
        let mut add = vec![];
        for id in self.selection.all() {
            add.extend(planview::joined_with(&self.doc, id));
            for e in model.iter() {
                if let ElementKind::Opening(o) = &e.kind {
                    if o.host == id {
                        add.push(e.id);
                    }
                }
            }
        }
        let before = self.selection.len();
        self.selection.extend(add, true);
        self.status = format!(
            "Linked: {} added, {} selected.",
            self.selection.len() - before,
            self.selection.len()
        );
    }

    /// Keep about half of the selection, picked at random (MU39).
    pub(super) fn select_random(&mut self) {
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64)
            | 1;
        let keep: Vec<ElementId> = self
            .selection
            .all()
            .into_iter()
            .filter(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed.is_multiple_of(2)
            })
            .collect();
        self.selection.extend(keep, false);
        self.status = format!("Random: {} kept.", self.selection.len());
    }

    /// Select every element of one kind (walls, doors…), as Blender's
    /// Select All by Type (MU39).
    pub(super) fn select_kind(&mut self, kind: &str) {
        let model = self.doc.model();
        let ids: Vec<ElementId> = self
            .selectable_ids()
            .into_iter()
            .filter(|id| model.get(*id).is_some_and(|e| kind_word(&e.kind) == kind))
            .collect();
        self.selection.extend(ids, false);
        self.face = None;
        self.status = format!("{} selected.", self.selection.len());
    }

    /// Everything Select all takes.
    pub(super) fn selectable_ids(&self) -> Vec<ElementId> {
        self.doc
            .model()
            .iter()
            .filter(|e| {
                !self.hidden.contains(&e.id)
                    && !matches!(
                        e.kind,
                        ElementKind::Level(_)
                            | ElementKind::View(_)
                            | ElementKind::Sheet(_)
                            | ElementKind::Viewport(_)
                            | ElementKind::Material(_)
                            | ElementKind::Units(_)
                            | ElementKind::Project(_)
                            | ElementKind::SheetTemplate(_)
                            | ElementKind::CropBox(_)
                    )
            })
            .map(|e| e.id)
            .collect()
    }

    fn select_all(&mut self) {
        let ids = self.selectable_ids();
        self.selection.extend(ids, false);
        self.face = None;
        self.status = format!("{} selected.", self.selection.len());
    }

    fn copy_selected(&mut self) -> bool {
        let ids = self.selection.all();
        if ids.is_empty() {
            self.status = "Select something to copy first.".into();
            return false;
        }
        let model = self.doc.model();
        self.clipboard = creabim_core::transform::copy_set(model, &ids)
            .into_iter()
            .filter_map(|id| model.get(id).cloned())
            .collect();
        true
    }

    /// The clipboard as text, so a paste can reach another CreaBIM window.
    fn clipboard_text(&self) -> String {
        serde_json::json!({ "creabim_elements": self.clipboard }).to_string()
    }

    /// Paste copies of `elements` in place, as one undo step.
    fn paste(&mut self, elements: Vec<Element>) {
        if elements.is_empty() {
            self.status = "Nothing to paste: copy something first (Ctrl+C).".into();
            return;
        }
        let (copies, dropped) = creabim_core::transform::fresh_copies(self.doc.model(), &elements);
        if copies.is_empty() {
            self.status = "Nothing could be pasted here: what was copied needs elements this project doesn't have.".into();
            return;
        }
        let ids: Vec<ElementId> = copies.iter().map(|c| c.id).collect();
        let n = copies.len();
        let mut tx = Transaction::new(format!("Paste {n} elements"));
        for c in copies {
            tx = tx.insert(c);
        }
        let r = self.doc.commit(tx);
        if r.is_ok() {
            self.selection.extend(ids, false);
            self.face = None;
        }
        let left = match dropped {
            0 => String::new(),
            d => format!(" {d} left out: they hang on elements this project doesn't have."),
        };
        self.report(
            r,
            format!("Pasted {n} elements in place. Press G to move them.{left}"),
        );
    }

    /// Paste from text another window (or this one) put on the clipboard.
    fn paste_text(&mut self, text: &str) {
        let parsed = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| v.get("creabim_elements").cloned())
            .and_then(|v| serde_json::from_value::<Vec<Element>>(v).ok());
        let elements = parsed.unwrap_or_else(|| self.clipboard.clone());
        self.paste(elements);
    }

    fn hide_selected(&mut self) {
        let ids = self.selection.all();
        if ids.is_empty() {
            return;
        }
        let n = ids.len();
        // a wall's doors and windows go with it
        let with = creabim_core::transform::copy_set(self.doc.model(), &ids);
        self.hidden.extend(with);
        self.hidden_rev += 1;
        self.selection.clear();
        self.face = None;
        self.status = format!("Hid {n}. Alt+H shows everything again.");
    }

    /// Hide everything but the selection.
    fn isolate_selected(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let keep = self.selection.all();
        // keep what the selection needs to show: hosts, levels, cutters
        let model = self.doc.model();
        let mut needed = keep.clone();
        for id in &keep {
            if let Some(e) = model.get(*id) {
                needed.extend(e.references());
            }
            needed.extend(model.dependents(*id));
        }
        self.hidden = model
            .iter()
            .map(|e| e.id)
            .filter(|id| !needed.contains(id))
            .collect();
        self.hidden_rev += 1;
        self.status = format!("Showing only {}. Alt+H shows everything again.", keep.len());
    }

    fn show_all(&mut self) {
        let n = self.hidden.len();
        self.hidden.clear();
        self.hidden_rev += 1;
        self.status = match n {
            0 => "Nothing was hidden.".into(),
            n => format!("Showing {n} hidden elements again."),
        };
    }

    fn extrude_face(&mut self) {
        let (Some(id), Some(face)) = (self.selection.active, self.face) else {
            return;
        };
        let d = self.extrude_distance;
        let r = self.doc.edit(id, "Extrude face", |e| {
            if let ElementKind::FreeMesh(m) = &mut e.kind {
                if face < m.mesh.faces.len() {
                    m.mesh.extrude_face(face, d);
                }
            }
        });
        self.report(
            r,
            format!(
                "Extruded face {face} by {}. Press E again to keep going.",
                crate::units::len(d)
            ),
        );
    }

    fn extrude_sketch(&mut self) {
        let Some(id) = self.selection.active else {
            return;
        };
        let d = self.extrude_distance;
        match crate::sketch::extrude(&mut self.doc, id, d) {
            Ok((mass, cap)) => {
                self.selection.only(mass);
                self.face = cap;
                self.status = match cap {
                    Some(_) => format!(
                        "Extruded the sketch {} into a mass. Its far face is selected: press E to push it further.", crate::units::len(d)
                    ),
                    None => format!(
                        "Extruded the open sketch {} into a surface. Add a Solidify modifier to give it thickness.", crate::units::len(d)
                    ),
                };
            }
            Err(e) => self.status = e,
        }
    }

    // ---- 3D -------------------------------------------------------------

    fn rebuild_geometry(&mut self, rs: &egui_wgpu::RenderState) {
        // the scene of this revision: shared if built, else the newest a
        // worker has finished while it builds this one, so an edit or a
        // drag never waits for the 3D view (A6-1.2); the first is built here
        let model = self.doc.model();
        let shown = self.geometry_key.as_ref().map(|k| k.0 .0);
        let (scene_rev, scene) = match creabim_geom::shared_scene_ready(model) {
            Some(s) => (model.revision(), s),
            None if shown.is_none() || !self.builds.enabled() => {
                // TEMP-DIAG for the 3D-view beachball: sync builds block
                // the UI thread; log before/after so a hang is located.
                let rev = model.revision();
                eprintln!("[diag] 3d: sync scene build start (rev {rev})");
                let t = std::time::Instant::now();
                let s = creabim_geom::shared_scene(model);
                eprintln!(
                    "[diag] 3d: sync scene built in {:.0} ms",
                    t.elapsed().as_secs_f64() * 1000.0
                );
                (model.revision(), s)
            }
            None => {
                self.builds.start_scene(model);
                self.builds.poll();
                match self.builds.latest_scene() {
                    Some((r, s)) if shown.is_none_or(|v| r > v) => (r, s),
                    _ => return,
                }
            }
        };
        // the model's buffers, built again only when what is shown changes;
        // the selection is drawn over them by its own small pass
        // (rebuild_selection), so a click builds nothing here (MU39)
        let key = (
            (scene_rev, self.hidden_rev),
            Selection::default(),
            None,
            self.plan_in_3d,
            self.active_view,
        );
        if self.geometry_key.as_ref() == Some(&key) {
            return;
        }
        self.geometry_key = Some(key);
        self.geometry_gen += 1;
        let _perf = Perf::new("3d rebuild");
        self.scene = (*scene).clone();
        let left_out = self.hidden_in_3d();
        self.scene
            .retain(|i| !self.hidden.contains(&i.id) && !left_out.contains(&i.id));
        let cut_edges = self.crop_scene_3d();
        snaps::begin_edge_round();

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut see_through = Vec::new();
        // the selection in the interaction colour every view and grip uses
        // (A7-2): a tint over the element's own colour and a crisp
        // outline, never a repaint
        let highlight = {
            let a = crate::planview::accent_selected();
            [
                a.r() as f32 / 255.0,
                a.g() as f32 / 255.0,
                a.b() as f32 / 255.0,
            ]
        };
        let tint = |c: [f32; 3]| {
            [0, 1, 2].map(|k| c[k] * (1.0 - SELECTION_TINT) + highlight[k] * SELECTION_TINT)
        };
        let mut ghost_lines: Vec<(DVec3, DVec3, [f32; 3])> = Vec::new();
        // what is selected of the cutters and shafts shows through everything
        let mut on_top: Vec<(DVec3, DVec3, [f32; 3])> = Vec::new();
        let xray = self.shell.settings.xray;
        let mode = self.shell.settings.render_3d;
        // wireframe on a dark theme: light wires, as Blender's (MU31: dark
        // wires on the dark ground all but vanished)
        let edge_colour = if mode == crate::render3d::RenderMode::Wireframe && self.dark_3d {
            [0.72, 0.74, 0.78]
        } else {
            [0.10, 0.10, 0.11]
        };
        // a type shown in the model: its instances lit as if selected, the
        // rest see-through when asked
        let spot = &self.plan.spotlight;
        let ghost = self.plan.spotlight_ghost && !spot.is_empty();
        for item in &self.scene {
            let selected = spot.contains(&item.id);
            // X-ray: what holds others turns see-through with its edges
            // drawn; the framing that sits in it stays solid
            let xrayed = !selected
                && (ghost
                    || xray
                        && !self
                            .doc
                            .model()
                            .get(item.id)
                            .is_some_and(|e| is_framing(&e.kind)));
            if item.ghost {
                // cutters show as their outline only
                let c = if selected {
                    highlight
                } else {
                    [0.35, 0.45, 0.75]
                };
                let out = if selected {
                    &mut on_top
                } else {
                    &mut ghost_lines
                };
                for &(a, b) in snaps::feature_edges_cached(&item.mesh).iter() {
                    out.push((a, b, c));
                }
                continue;
            }
            // a selected element is outlined whatever the shading shows
            // wireframe draws every wire, X-ray or not, as Blender's (MU31:
            // X-ray left only the framing standing)
            let wire = mode == crate::render3d::RenderMode::Wireframe;
            if (mode.edges() || selected) && (!xrayed || wire) {
                let c = if selected { highlight } else { edge_colour };
                for &(a, b) in snaps::feature_edges_cached(&item.mesh).iter() {
                    ghost_lines.push((a, b, c));
                }
            }
            if mode == crate::render3d::RenderMode::Wireframe {
                continue;
            }
            let base = vertices.len() as u32;
            let face_tris = match (
                false,
                self.face,
                self.doc.model().get(item.id).map(|e| &e.kind),
            ) {
                (true, Some(f), Some(ElementKind::FreeMesh(m))) => {
                    // the face and its copies made by modifiers
                    let ev = m.evaluated();
                    let tris: Vec<usize> = (0..ev.mesh.faces.len())
                        .filter(|&i| ev.source[i] == Some(f))
                        .flat_map(|i| face_triangles(&ev.mesh, i))
                        .collect();
                    Some(tris)
                }
                _ => None,
            };
            let plain = if mode == crate::render3d::RenderMode::HiddenLine {
                [0.97, 0.97, 0.96]
            } else {
                item.color
            };
            let mut colors =
                vec![if selected { tint(plain) } else { plain }; item.mesh.positions.len()];
            if let Some(tris) = &face_tris {
                for &t in tris {
                    for k in 0..3 {
                        // a cut mesh no longer matches its faces one to one
                        if let Some(&v) = item.mesh.indices.get(t * 3 + k) {
                            colors[v as usize] = highlight;
                        }
                    }
                }
            }
            // a selected element keeps its own surface (glass stays glass)
            let surface = if xrayed {
                [
                    item.surface[0],
                    item.surface[1],
                    XRAY_OPACITY.min(item.surface[2]),
                ]
            } else {
                item.surface
            };
            if xrayed {
                for &(a, b) in snaps::feature_edges_cached(&item.mesh).iter() {
                    ghost_lines.push((a, b, XRAY_EDGE));
                }
            }
            for (i, p) in item.mesh.positions.iter().enumerate() {
                vertices.push(Vertex {
                    pos: *p,
                    normal: item.mesh.normals[i],
                    color: colors[i],
                    surface,
                });
            }
            let target = if surface[2] < 0.999 {
                &mut see_through
            } else {
                &mut indices
            };
            target.extend(item.mesh.indices.iter().map(|i| i + base));
        }

        // the working grid, 1 m spacing, just under the workplane or the level
        // the 3D tools draw on, so it stays put whatever the model's lowest
        // point is
        let level_plane = Workplane::ground(self.work_level().map_or(0.0, |(_, z)| z));
        let spacing = ground_spacing(self.shell.settings.ground_grid_spacing);
        let grid_lines = |plane: Workplane, lo: f64, hi: f64, step: f64, color: [f32; 3]| {
            let on = |u: f64, v: f64| -> [f32; 3] {
                (plane.to_model(DVec2::new(u, v)) - plane.normal() * 0.002)
                    .as_vec3()
                    .to_array()
            };
            let mut out = Vec::new();
            let (first, last) = ((lo / step).ceil() as i32, (hi / step).floor() as i32);
            for i in first..=last {
                let v = i as f64 * step;
                for (a, b) in [((v, lo), (v, hi)), ((lo, v), (hi, v))] {
                    out.push(line_vertex(on(a.0, a.1), color));
                    out.push(line_vertex(on(b.0, b.1), color));
                }
            }
            out
        };
        let mut lines = Vec::new();
        // where crop boxes cut, the outlines of the cuts
        for (a, b) in &cut_edges {
            lines.push(line_vertex(a.as_vec3().to_array(), [0.18, 0.16, 0.15]));
            lines.push(line_vertex(b.as_vec3().to_array(), [0.18, 0.16, 0.15]));
        }
        for (a, b, c) in &ghost_lines {
            lines.push(line_vertex(a.as_vec3().to_array(), *c));
            lines.push(line_vertex(b.as_vec3().to_array(), *c));
        }
        // one plane at a time (MU18): while a face is the workplane, the
        // ground grid dims to a hint
        let face_plane = self.workplane.is_some();
        if self.shell.settings.ground_grid {
            // as far out as 50 lines each way, at least the old 50 m
            let reach = (spacing * 25.0).max(25.0);
            // quiet: a little off the paper, so edges and vertex dots
            // stand well clear of it (MU39: too strong)
            let color = match (self.paper_is_dark, face_plane) {
                (false, true) => [0.91, 0.92, 0.93],
                (false, false) => [0.865, 0.865, 0.88],
                (true, true) => [0.165, 0.17, 0.185],
                (true, false) => [0.215, 0.22, 0.245],
            };
            lines.extend(grid_lines(
                level_plane,
                -reach + 5.0,
                reach + 5.0,
                spacing,
                color,
            ));
            // the model's X and Y axes through its origin, red and green as
            // Blender's (MU39); Z shows only while a move keeps to it
            if self.shell.settings.axis_lines && !face_plane {
                let z = level_plane.to_model(DVec2::ZERO).z as f32 + 0.001;
                let (lo, hi) = ((-reach + 5.0) as f32, (reach + 5.0) as f32);
                let (red, green) = if self.paper_is_dark {
                    ([0.78, 0.30, 0.30], [0.42, 0.66, 0.30])
                } else {
                    ([0.86, 0.36, 0.36], [0.45, 0.70, 0.35])
                };
                lines.push(line_vertex([lo, 0.0, z], red));
                lines.push(line_vertex([hi, 0.0, z], red));
                lines.push(line_vertex([0.0, lo, z], green));
                lines.push(line_vertex([0.0, hi, z], green));
            }
        }
        if let Some((plane, _)) = self.workplane {
            let bg = if self.paper_is_dark {
                BACKGROUND_3D_DARK
            } else {
                BACKGROUND_3D
            };
            lines.extend(workplane_lines(plane, spacing, WORKPLANE_FADE, bg));
        }
        lines.extend(self.grid_lines_3d());

        // 2D drawing shown in 3D space, on its level
        let mut overlay = Vec::new();
        if self.plan_in_3d {
            let placed = self.active_view.and_then(|id| {
                let kind = self.view(id)?.kind.clone();
                Some((kind, self.drawing_live(id)?))
            });
            if let Some((kind, plan)) = placed {
                // drawing coordinates → world: plans lie flat at their cut
                // height, sections stand on their section line
                let to_world: Option<Box<dyn Fn(DVec2) -> [f32; 3]>> = match kind {
                    ViewKind::Plan { level, cut_height } => {
                        let z = self
                            .doc
                            .model()
                            .level(level)
                            .map_or(0.0, |l| l.elevation + cut_height)
                            as f32;
                        Some(Box::new(move |p| [p.x as f32, p.y as f32, z]))
                    }
                    ViewKind::Section { start, end, .. } => {
                        let along = (end - start).normalize_or_zero();
                        Some(Box::new(move |p| {
                            let w = start + along * p.x;
                            [w.x as f32, w.y as f32, p.y as f32]
                        }))
                    }
                    // 3D views and schedules have no place of their own in the model
                    ViewKind::Axonometric { .. } | ViewKind::Schedule { .. } => None,
                };
                let to_world = to_world.unwrap_or_else(|| Box::new(|_| [0.0; 3]));
                let plan = if matches!(
                    kind,
                    ViewKind::Axonometric { .. } | ViewKind::Schedule { .. }
                ) {
                    Drawing::default()
                } else {
                    plan
                };
                let mut seg = |a: DVec2, b: DVec2, c: [f32; 3]| {
                    overlay.push(line_vertex(to_world(a), c));
                    overlay.push(line_vertex(to_world(b), c));
                };
                for it in plan.items.iter().filter(|it| !left_out.contains(&it.id())) {
                    let c = match it {
                        PlanItem::Polygon { pen: Pen::Cut, .. }
                        | PlanItem::Line { pen: Pen::Cut, .. } => [0.85, 0.15, 0.10],
                        _ => [0.15, 0.25, 0.55],
                    };
                    match it {
                        PlanItem::Polygon { points, .. } => {
                            for i in 0..points.len() {
                                seg(points[i], points[(i + 1) % points.len()], c);
                            }
                        }
                        // guides show as their own lines in 3D; underlays stay in the plan
                        PlanItem::Line {
                            pen: Pen::Guide | Pen::Underlay,
                            ..
                        }
                        | PlanItem::Arc {
                            pen: Pen::Underlay, ..
                        } => {}
                        PlanItem::Line { a, b, .. } => seg(*a, *b, c),
                        PlanItem::Arc {
                            center,
                            radius,
                            start,
                            end,
                            ..
                        } => {
                            let n = 16;
                            for i in 0..n {
                                let t0 = start + (end - start) * i as f64 / n as f64;
                                let t1 = start + (end - start) * (i + 1) as f64 / n as f64;
                                seg(
                                    *center + DVec2::from_angle(t0) * *radius,
                                    *center + DVec2::from_angle(t1) * *radius,
                                    c,
                                );
                            }
                        }
                        PlanItem::Dimension { a, b, offset, .. } => {
                            let n = (*b - *a).normalize_or_zero().perp() * *offset;
                            seg(*a + n, *b + n, [0.12, 0.24, 0.47]);
                        }
                        PlanItem::Label { .. }
                        | PlanItem::Fill { .. }
                        | PlanItem::Text { .. }
                        | PlanItem::Tag { .. }
                        | PlanItem::Annotation { .. }
                        | PlanItem::Tint { .. }
                        | PlanItem::Room { .. }
                        | PlanItem::Shade { .. }
                        | PlanItem::Image { .. } => {}
                    }
                }
            }
        }

        // what is inside walls, slabs and roofs, in its colours where they hide it
        let behind = if self.shell.settings.hidden_colour_3d {
            self.buried_vertices()
        } else {
            vec![]
        };
        for (a, b, c) in &on_top {
            overlay.push(line_vertex(a.as_vec3().to_array(), *c));
            overlay.push(line_vertex(b.as_vec3().to_array(), *c));
        }
        if let Some(vp) = &mut self.viewport {
            vp.set_geometry(
                &rs.device,
                &vertices,
                &indices,
                &see_through,
                &lines,
                &overlay,
            );
            vp.set_behind(&rs.device, &behind);
        }
    }

    /// The selection's highlight over the model's buffers: its triangles
    /// tinted, the face picked in the accent, its edges, a selected
    /// cutter's outline through everything. Only the selection is built.
    fn rebuild_selection(&mut self, rs: &egui_wgpu::RenderState) {
        let key = (self.geometry_gen, self.selection.clone(), self.face);
        if self.selection_key.as_ref() == Some(&key) {
            return;
        }
        self.selection_key = Some(key);
        let a = crate::planview::accent_selected();
        let highlight = [
            a.r() as f32 / 255.0,
            a.g() as f32 / 255.0,
            a.b() as f32 / 255.0,
        ];
        let tint = |c: [f32; 3]| {
            [0, 1, 2].map(|k| c[k] * (1.0 - SELECTION_TINT) + highlight[k] * SELECTION_TINT)
        };
        let mode = self.shell.settings.render_3d;
        let (mut tris, mut lines, mut top) = (vec![], vec![], vec![]);
        for item in self.scene.iter().filter(|i| self.selection.contains(i.id)) {
            if item.ghost {
                for &(a, b) in snaps::feature_edges_cached(&item.mesh).iter() {
                    top.push(line_vertex(a.as_vec3().to_array(), highlight));
                    top.push(line_vertex(b.as_vec3().to_array(), highlight));
                }
                continue;
            }
            for &(a, b) in snaps::feature_edges_cached(&item.mesh).iter() {
                lines.push(line_vertex(a.as_vec3().to_array(), highlight));
                lines.push(line_vertex(b.as_vec3().to_array(), highlight));
            }
            if mode == crate::render3d::RenderMode::Wireframe {
                continue;
            }
            // the face picked, and its copies made by modifiers
            let face: std::collections::HashSet<usize> =
                match (self.face, self.doc.model().get(item.id).map(|e| &e.kind)) {
                    (Some(f), Some(ElementKind::FreeMesh(m))) => {
                        let ev = m.evaluated();
                        (0..ev.mesh.faces.len())
                            .filter(|&i| ev.source[i] == Some(f))
                            .flat_map(|i| face_triangles(&ev.mesh, i))
                            .collect()
                    }
                    _ => Default::default(),
                };
            let plain = if mode == crate::render3d::RenderMode::HiddenLine {
                [0.97, 0.97, 0.96]
            } else {
                item.color
            };
            for (t, c) in item.mesh.indices.as_chunks::<3>().0.iter().enumerate() {
                let color = if face.contains(&t) {
                    highlight
                } else {
                    tint(plain)
                };
                for &v in c {
                    let v = v as usize;
                    tris.push(Vertex {
                        pos: item.mesh.positions[v],
                        normal: item.mesh.normals[v],
                        color,
                        surface: item.surface,
                    });
                }
            }
        }
        if let Some(vp) = &mut self.viewport {
            vp.set_selection(&rs.device, &tris, &lines, &top);
        }
    }

    /// Triangles of the elements inside walls, slabs, roofs or masses (their
    /// middle within one's bounds), see-through, for drawing where they are
    /// hidden.
    fn buried_vertices(&self) -> Vec<Vertex> {
        let model = self.doc.model();
        let hosts: Vec<(ElementId, GVec3, GVec3)> = self
            .scene
            .iter()
            .filter(|i| !i.ghost)
            .filter(|i| {
                model
                    .get(i.id)
                    .is_some_and(|e| planview::is_xray_host(&e.kind))
            })
            .filter_map(|i| {
                let (lo, hi) = i.mesh.bounds()?;
                Some((i.id, lo, hi))
            })
            .collect();
        let mut out = Vec::new();
        for item in self.scene.iter().filter(|i| !i.ghost) {
            let Some(e) = model.get(item.id) else {
                continue;
            };
            if planview::is_xray_host(&e.kind) || matches!(e.kind, ElementKind::Opening(_)) {
                continue;
            }
            let Some((lo, hi)) = item.mesh.bounds() else {
                continue;
            };
            let mid = (lo + hi) / 2.0;
            let inside = hosts.iter().any(|(id, hlo, hhi)| {
                *id != item.id && mid.cmpge(*hlo - 0.01).all() && mid.cmple(*hhi + 0.01).all()
            });
            if !inside {
                continue;
            }
            let surface = [item.surface[0], item.surface[1], BURIED_OPACITY];
            for i in &item.mesh.indices {
                let i = *i as usize;
                out.push(Vertex {
                    pos: item.mesh.positions[i],
                    normal: item.mesh.normals[i],
                    color: item.color,
                    surface,
                });
            }
        }
        out
    }

    /// The drawing view Alt+Z turns X-ray on in, or none for the 3D view:
    /// the 2D pane's when the pointer is (or was last) over it.
    fn xray_view_target(&self) -> Option<ElementId> {
        let id = self.active_view?;
        let shown = self.rect_2d.is_some();
        let drawing = matches!(
            self.view(id).map(|v| &v.kind),
            Some(ViewKind::Plan { .. } | ViewKind::Section { .. } | ViewKind::Axonometric { .. })
        );
        (shown && drawing && self.last_in_2d).then_some(id)
    }

    /// Turn X-ray on or off in drawing view `id`, while working: what
    /// holds others faint, what is inside them drawn solid and picked first.
    pub(super) fn toggle_view_xray(&mut self, id: ElementId) {
        let on = !self.plan.xray.contains(&id);
        if on {
            self.plan.xray.insert(id);
        } else {
            self.plan.xray.remove(&id);
        }
        self.plan.hover = None;
        let name = self
            .doc
            .model()
            .get(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        self.status = if on {
            format!("X-ray in {name}: walls, slabs and roofs see-through, what is inside them drawn and clicked first. Alt+Z turns it off. Sheets still print it as it was.")
        } else {
            format!("X-ray off in {name}.")
        };
    }

    fn show_3d(&mut self, ui: &mut egui::Ui, rect: Rect, frame: &eframe::Frame) {
        let _perf = Perf::new("show_3d");
        self.rect_3d = Some(rect);
        let response = ui.interact(rect, ui.id().with("3d"), Sense::click_and_drag());
        let Some(rs) = frame.wgpu_render_state() else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "3D needs the wgpu renderer",
                egui::FontId::proportional(16.0),
                Color32::RED,
            );
            return;
        };
        let modal_here = self.plan.modal.as_ref().is_some_and(|m| m.pane.is_none());
        if self.cube.step(&mut self.camera, ui.input(|i| i.time)) {
            ui.ctx().request_repaint();
        }
        // the mouse (MU19): right drag pans, and so does Shift + middle;
        // middle drag orbits (Classic: middle pans too)
        let classic = self.shell.settings.navigation == "classic";
        let shift_now = ui.input(|i| i.modifiers.shift);
        // Ctrl + middle drag zooms, up in and down out, as Blender (MU39)
        let ctrl_now = ui.input(|i| i.modifiers.command);
        let mid = response.dragged_by(egui::PointerButton::Middle);
        if mid && ctrl_now {
            self.camera.zoom(-response.drag_delta().y / 60.0);
        } else {
            if response.dragged_by(egui::PointerButton::Secondary)
                || (mid && (classic || shift_now))
            {
                let d = response.drag_delta();
                self.camera.pan(GVec2::new(d.x, d.y), rect.height());
            }
            if !classic && mid && !shift_now {
                self.orbit_3d(response.drag_delta());
            }
        }
        if response.hovered() {
            // two fingers on a touchpad pan; a wheel and a pinch zoom (MU24)
            let (pan, scroll) = crate::planview::wheel(ui);
            if pan != egui::Vec2::ZERO {
                self.camera.pan(GVec2::new(pan.x, pan.y), rect.height());
            }
            if scroll != 0.0 {
                let aspect = rect.width() / rect.height().max(1.0);
                match response.hover_pos() {
                    Some(p) => {
                        let ndc = GVec2::new(
                            (p.x - rect.left()) / rect.width().max(1.0) * 2.0 - 1.0,
                            1.0 - (p.y - rect.top()) / rect.height().max(1.0) * 2.0,
                        );
                        self.camera.zoom_at(scroll / 50.0, ndc, aspect);
                    }
                    None => self.camera.zoom(scroll / 50.0),
                }
            }
        }
        let aspect = rect.width() / rect.height().max(1.0);
        self.aspect = aspect;

        // wires in the theme's ink: a new theme draws them again
        let dark = ui.visuals().dark_mode;
        if dark != self.dark_3d {
            self.dark_3d = dark;
            self.geometry_key = None;
        }
        self.rebuild_geometry(rs);
        self.rebuild_selection(rs);
        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        if let Some(vp) = &mut self.viewport {
            let tex = vp.render(
                rs,
                size,
                self.camera.view_proj(aspect),
                self.camera.eye().to_array(),
                if self.paper_is_dark {
                    BACKGROUND_3D_DARK
                } else {
                    BACKGROUND_3D
                }
                .map(f64::from),
            );
            ui.painter().image(
                tex,
                rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            "3D",
            egui::FontId::proportional(13.0),
            Color32::from_gray(80),
        );

        // overlays and pointer input, drawn over the rendered image
        let painter = ui.painter_at(rect);
        let cam = self.camera.clone();
        let view_proj = cam.view_proj(aspect);
        let project = move |p: DVec3| -> Option<egui::Pos2> {
            let c = view_proj * p.as_vec3().extend(1.0);
            if c.w <= 1e-4 {
                return None;
            }
            let n = c.truncate() / c.w;
            Some(egui::pos2(
                rect.left() + (n.x + 1.0) / 2.0 * rect.width(),
                rect.top() + (1.0 - n.y) / 2.0 * rect.height(),
            ))
        };
        let ray = |p: egui::Pos2| {
            let ndc = GVec2::new(
                (p.x - rect.left()) / rect.width() * 2.0 - 1.0,
                1.0 - (p.y - rect.top()) / rect.height() * 2.0,
            );
            let (o, d) = cam.ray(ndc, aspect);
            (o.as_dvec3(), d.as_dvec3())
        };
        let hover = response.hover_pos();
        let _pp = Perf::new("refs3d");
        self.draw_references_3d(&painter, &project);
        drop(_pp);
        self.draw_divide_preview(&painter, &project);
        let _pp = Perf::new("sketches");
        self.draw_sketches(&painter, &project);
        drop(_pp);
        self.draw_grid_bubbles(&painter, &project);
        self.draw_crop_frames(&painter, &project);
        let outside = self.outside_crop();
        if !outside.is_empty() {
            self.draw_outside_crop(&painter, &project, &outside);
            self.outside_crop_chip(ui, rect, &outside);
        }
        let _pp = Perf::new("hover_3d");
        self.hover_3d(ui, &response, &painter, &project, &ray, modal_here);
        drop(_pp);
        self.view_cube(ui, rect);

        // the Move and Rotate tools, or a keyboard move, rotate or scale,
        // running here
        let starting = response.hovered()
            && self.plan.modal.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && self.selection.is_some()
            && !(planview::start_keys(ui.ctx()).is_none()
                && planview::only_paper(&self.doc, &self.selection.all()))
            && (self.plan.armed.is_some() || planview::start_keys(ui.ctx()).is_some());
        if modal_here || starting {
            let moving = self
                .plan
                .modal
                .as_ref()
                .and_then(|m| m.snap_exclude(&self.doc));
            let exclude = moving.unwrap_or_else(|| {
                self.selection
                    .active
                    .map(|id| planview::joined_with(&self.doc, id))
                    .unwrap_or_default()
            });
            self.refresh_snaps_3d(&exclude);
        }
        if modal_here {
            // the view still orbits, pans and zooms between clicks
            if response.dragged_by(egui::PointerButton::Primary) {
                let d = response.drag_delta();
                self.orbit_3d(d);
            }
            let eye = cam.eye().as_dvec3();
            let work_z = self.work_level().map_or(0.0, |(_, z)| z);
            // a sloped or upright workplane (a face): edits slide on it
            let face_plane = self
                .workplane
                .as_ref()
                .map(|w| w.0)
                .filter(|w| w.normal().z.abs() < 0.999);
            let parallel = |at: DVec3| match face_plane {
                Some(w) => Workplane { origin: at, ..w },
                None => Workplane::ground(at.z),
            };
            let to_model = |pos: egui::Pos2, axis: Option<Axis>, pivot: DVec3| {
                let (o, d) = ray(pos);
                let plane = if axis == Some(Axis::Z) {
                    // stand a plane up through the pivot, facing the camera
                    let f = (pivot - eye).truncate().normalize_or(DVec2::X);
                    Workplane::facing(pivot, -f.extend(0.0))
                } else {
                    parallel(pivot)
                };
                plane.intersect(o, d)
            };
            let snap_cfg = self.plan.snap.live(ui.input(|i| i.modifiers.alt));
            let snapper = &self.snaps_3d.as_ref().expect("built above").2;
            let scene = &self.scene;
            let snap = |p: egui::Pos2, from: Option<DVec3>| {
                let (o, d) = ray(p);
                // from a base point: on its level; else on what is under
                // the pointer, or the working level
                let free = match from {
                    Some(f) => parallel(f).intersect(o, d),
                    None => pick::pick(scene, o.as_vec3(), d.as_vec3())
                        .map(|h| o + d * h.t as f64)
                        .or_else(|| Workplane::ground(work_z).intersect(o, d)),
                };
                let s = snapper.snap(&snap_cfg, p, &project, from, free)?;
                Some((s.p, s))
            };
            // masses, for the face of their own mesh a picked triangle is
            let faces_on = self
                .plan
                .modal
                .as_ref()
                .is_some_and(|m| m.works_on == crate::modal::Target::Face);
            let masses: HashMap<ElementId, creabim_core::FreeMesh> = if faces_on {
                self.doc
                    .model()
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ElementKind::FreeMesh(m) => Some((e.id, m.clone())),
                        _ => None,
                    })
                    .collect()
            } else {
                HashMap::new()
            };
            let pick_face = |p: egui::Pos2| -> Option<crate::modal::FacePick> {
                let (o, d) = ray(p);
                let f = pick::pick_face(scene, o.as_vec3(), d.as_vec3())?;
                let mesh_face = match masses.get(&f.hit.id) {
                    Some(m) => {
                        let ev = m.evaluated();
                        let face = pick::face_of_triangle(&ev.mesh, f.hit.triangle)?;
                        ev.source.get(face).copied().flatten()
                    }
                    _ => None,
                };
                Some(crate::modal::FacePick {
                    id: f.hit.id,
                    face: creabim_core::pushpull::FaceRef {
                        point: f.point,
                        normal: f.normal,
                        mesh_face,
                    },
                    tris: f.triangles,
                })
            };
            let map = PaneMap {
                to_model: &to_model,
                project: &project,
                look: ray(rect.center()).1,
                snap: &snap,
                pick_face: Some(&pick_face),
            };
            let ctx = ui.ctx().clone();
            let shift = ui.input(|i| i.modifiers.shift);
            let base_stage = self
                .plan
                .modal
                .as_ref()
                .is_some_and(|m| m.stage() == crate::modal::Stage::Base);
            if base_stage && shift && response.clicked() {
                // Shift+click while the base point is picked adds or takes away
                if let Some(p) = response.interact_pointer_pos() {
                    let (o, d) = ray(p);
                    let hit = pick::pick(&self.scene, o.as_vec3(), d.as_vec3())
                        .map(|h| h.id)
                        .or_else(|| self.grid_bubble_near(p, &project))
                        .or_else(|| self.grid_line_near(p, &project).map(|g| g.0));
                    if let Some(id) = hit {
                        self.selection.toggle(id);
                        self.face = None;
                        let ids = self.selection.all();
                        self.status = planview::restart_modal(
                            &mut self.doc,
                            &mut self.plan.modal,
                            &ids,
                            p,
                            self.plan.pivot,
                            self.plan.cursor,
                        );
                    }
                }
            } else if let Some(m) = self.plan.modal.as_mut() {
                m.panel(&ctx, rect);
                let right = response.secondary_clicked() && !shift;
                match m.update(
                    &mut self.doc,
                    &ctx,
                    &painter,
                    hover,
                    response.clicked(),
                    right,
                    &map,
                ) {
                    Outcome::Running(s) => self.status = s,
                    Outcome::Done(s) | Outcome::Cancelled(s) => {
                        self.status = s;
                        self.plan.modal = None;
                    }
                }
                if let Some(m) = &self.plan.modal {
                    self.plan.move_target = m.works_on;
                }
            }
            ui.ctx().request_repaint();
            return;
        }
        if starting {
            if let Some(grab) = hover {
                let keys = planview::start_keys(ui.ctx());
                let flow = if keys.is_some() {
                    crate::modal::Flow::Keys
                } else {
                    crate::modal::Flow::Tool
                };
                let mode = keys.or(self.plan.armed);
                let extras = (
                    self.plan.armed_grip.take(),
                    std::mem::take(&mut self.plan.in_place),
                );
                self.plan.cancel();
                self.plan.armed = None;
                let ids = self.selection.all();
                if let Some(mode) = mode {
                    match planview::start_modal(
                        &mut self.doc,
                        mode,
                        &ids,
                        self.face,
                        Space::Model,
                        None,
                        grab,
                        self.plan.pivot,
                        self.plan.cursor,
                        flow,
                        self.plan.move_target,
                        self.workplane.as_ref().map(|w| w.0),
                    )
                    .and_then(|(m, t)| Ok((planview::with_extras(&mut self.doc, m, extras)?, t)))
                    {
                        Ok((m, targets)) => {
                            if targets != ids {
                                self.selection.extend(targets, false);
                                self.face = None;
                            }
                            self.status = planview::start_hint(&m);
                            self.plan.modal = Some(m);
                        }
                        Err(e) => self.status = e,
                    }
                }
                return;
            }
        }

        if self.point_pick_3d(ui, &response, &painter, &project, &ray) {
            return;
        }
        if self.pick_plane_3d(ui, &response, &painter, &project, &ray) {
            return;
        }
        if self.crop_pick_3d(ui, &response, &ray) {
            return;
        }

        // Esc or right-click steps back: drop the tool's clicks, then the
        // tool; in Select, right-click opens the menu
        if self.plan.sketch.is_none() && self.plan.outline.is_none() {
            let (esc, shift) = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.modifiers.shift));
            // a grip being dragged takes Esc itself; in Edit mode a
            // right-click opens the grips' menu
            let esc = esc
                && response.hovered()
                && !ui.ctx().egui_wants_keyboard_input()
                && self.handle_drag.is_none()
                && !self.plan.grips.busy();
            let right = response.secondary_clicked()
                && !shift
                && !(self.plan.edit.is_some() && self.plan.tool == Tool::Select);
            if esc || right {
                if self.pending_3d.take().is_some() || self.guide_from.take().is_some() {
                    self.plan.cancel();
                    self.status = "Cancelled. Right-click or Esc again to leave the tool.".into();
                } else {
                    let stepped = self.plan.step_back();
                    // Esc with nothing to step back from selects nothing (MU39)
                    if !stepped
                        && esc
                        && !right
                        && self.plan.tool == Tool::Select
                        && self.plan.edit.is_none()
                        && self.selection.is_some()
                    {
                        self.selection.clear();
                        self.face = None;
                        self.status = "Nothing selected.".into();
                    } else if !stepped && right && self.plan.tool == Tool::Select {
                        if self.plan.web_menus {
                            self.plan.menu_request = response.interact_pointer_pos();
                            self.shell.menu_3d = true;
                        }
                        if let Some(p) = response.interact_pointer_pos() {
                            let (o, d) = ray(p);
                            let hit = pick::pick(&self.scene, o.as_vec3(), d.as_vec3());
                            if let Some(h) = hit.as_ref().filter(|h| !self.selection.contains(h.id))
                            {
                                self.selection.only(h.id);
                                self.face = None;
                            }
                            self.menu_at_3d = hit
                                .map(|h| o + d * h.t as f64)
                                .or_else(|| Workplane::ground(0.0).intersect(o, d));
                            self.menu_plane_3d = face_plane(&self.scene, o, d);
                        }
                    }
                }
            }
            if self.plan.tool == Tool::Select && !self.plan.web_menus {
                let n = self.selection.len();
                let at = self.menu_at_3d;
                let mut picked = None;
                response.context_menu(|ui| picked = crate::commands::context_menu(ui, n));
                match picked {
                    Some(Command::WorkplaneHere) => match self.menu_plane_3d {
                        Some(plane) => self.set_workplane(plane, "on a face".into()),
                        None => {
                            self.status = "Right-click a face to put the workplane on it.".into()
                        }
                    },
                    Some(c) => match c.modal() {
                        Some(m) => self.plan.armed = Some(m),
                        None => self.plan.commands.push((c, at)),
                    },
                    None => {}
                }
            }
        }

        // tools that draw on the face under the pointer (a wall opening
        // only once the workplane is set on a face: else a click places one)
        let on_face = match self.plan.tool {
            Tool::Sketch | Tool::Extrusion | Tool::Void | Tool::Sweep | Tool::Slab => true,
            Tool::Opening => self.workplane.is_some(),
            _ => false,
        };
        if on_face {
            if response.dragged_by(egui::PointerButton::Primary) {
                let d = response.drag_delta();
                self.orbit_3d(d);
            }
            self.sketch_3d(ui, &response, &painter, &project, &ray, &cam);
            return;
        }

        // Shift+right-click places the pivot cursor where the pointer hits
        if let Some(c) = self.plan.cursor {
            if let Some(at) = project(c) {
                planview::draw_cursor(&painter, at);
            }
        }
        if response.secondary_clicked() && ui.input(|i| i.modifiers.shift) {
            if let Some(p) = response.interact_pointer_pos() {
                let (o, d) = ray(p);
                let at = pick::pick(&self.scene, o.as_vec3(), d.as_vec3())
                    .map(|h| o + d * h.t as f64)
                    .or_else(|| Workplane::ground(0.0).intersect(o, d));
                if let Some(c) = at {
                    self.plan.cursor = Some(c);
                    self.plan.pivot = crate::modal::Pivot::Cursor;
                    self.status = format!(
                        "Pivot cursor at {:.2}, {:.2}, {:.2}. Rotate and scale now turn about it.",
                        c.x, c.y, c.z
                    );
                }
            }
            return;
        }

        // B, then drag: select with a box
        if self.plan.tool == Tool::Select {
            let (b, esc) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::B),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            if response.hovered() && b && !ui.ctx().egui_wants_keyboard_input() {
                self.box_armed = true;
                self.status = "Drag a box: left to right takes what is inside, right to left what it touches. Shift adds.".into();
            }
            if esc {
                self.box_armed = false;
                self.box_3d = None;
            }
            // the mouse (MU19): a left drag on empty space draws a box,
            // as in drawings; on the cube or on something selected it
            // doesn't (that drag turns the view or moves it); Alt + left
            // drag orbits. B arms it in Classic.
            let auto_box = self.shell.settings.navigation != "classic"
                && !ui.input(|i| i.modifiers.alt)
                && response.drag_started_by(egui::PointerButton::Primary)
                && ui.input(|i| i.pointer.press_origin()).is_some_and(|p0| {
                    if self.cube.covers(p0) {
                        return false;
                    }
                    // nor on one of the selection's grips: that drag is the
                    // grip's (a crop box's face, MU21)
                    let on_grip = self.selection.active.is_some_and(|id| {
                        handles::handles_in(
                            self.doc.model(),
                            id,
                            self.face,
                            self.plan.edit == Some(id),
                        )
                        .iter()
                        .filter_map(|h| project(h.at))
                        .any(|s| s.distance(p0) < 14.0)
                    });
                    if on_grip {
                        return false;
                    }
                    let (o, d) = ray(p0);
                    !pick::pick(&self.scene, o.as_vec3(), d.as_vec3())
                        .is_some_and(|h| self.selection.contains(h.id))
                });
            if (self.box_armed || auto_box)
                && response.drag_started_by(egui::PointerButton::Primary)
            {
                self.box_3d = ui.input(|i| i.pointer.press_origin());
                if let Some(p0) = self.box_3d {
                    crate::boxsel::trace(p0, true);
                }
                self.box_armed = false;
            }
            if let Some(p0) = self.box_3d {
                let now = ui.input(|i| i.pointer.latest_pos()).unwrap_or(p0);
                crate::boxsel::trace(now, false);
                crate::boxsel::draw(&painter, p0, now);
                let ended = response.drag_stopped() || !response.dragged();
                {
                    let tris = self.scene.iter().flat_map(|item| {
                        let pts: Vec<Option<egui::Pos2>> = item
                            .mesh
                            .positions
                            .iter()
                            .map(|p| project(glam::Vec3::from(*p).as_dvec3()))
                            .collect();
                        item.mesh
                            .indices
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .filter_map(move |t| {
                                let p = |i: u32| pts.get(i as usize).copied().flatten();
                                Some((item.id, [p(t[0])?, p(t[1])?, p(t[2])?]))
                            })
                    });
                    let ids: Vec<ElementId> = crate::boxsel::pick_triangles(p0, now, tris)
                        .into_iter()
                        .filter(|id| planview::box_selectable(&self.doc, *id))
                        .collect();
                    if !ended {
                        crate::boxsel::draw_count(&painter, now, ids.len());
                        return;
                    }
                    let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
                    crate::boxsel::apply(
                        &mut self.selection,
                        ids,
                        crate::boxsel::combine_with(shift, ctrl),
                    );
                    self.face = None;
                    self.box_3d = None;
                    self.status = format!("{} selected.", self.selection.len());
                    self.plan.boxed = true;
                }
                return;
            }
        }

        // handles on the selection, and dragging the selection itself
        let exclude = self
            .selection
            .active
            .map(|id| planview::joined_with(&self.doc, id))
            .unwrap_or_default();
        self.refresh_snaps_3d(&exclude);
        let snap_cfg = self.plan.snap.live(ui.input(|i| i.modifiers.alt));
        // a grid line pressed on, to drag it square to itself
        let grid_hit = ui
            .input(|i| i.pointer.press_origin())
            .zip(self.selection.active)
            .and_then(|(p, id)| Some((id, self.grid_grab(p, id, &project)?)));
        let _pd = Perf::new("handles_3d");
        let dragging = {
            let snapper = &self.snaps_3d.as_ref().expect("just built").2;
            let scene = &self.scene;
            let ray_fn = |p: egui::Pos2| {
                let (o, d) = ray(p);
                Some(Ray { o, d })
            };
            let snap = |p: egui::Pos2, from: Option<DVec3>| {
                let (o, d) = ray(p);
                let free = Workplane::ground(from.map_or(0.0, |f| f.z)).intersect(o, d);
                let s = snapper.snap(&snap_cfg, p, &project, from, free)?;
                Some((s.p, s))
            };
            let grab_body = |p: egui::Pos2, id: ElementId| {
                if let Some((_, at)) = grid_hit.filter(|g| g.0 == id) {
                    return Some(at);
                }
                // the element itself, even behind or inside another (picked
                // with Tab or in X-ray)
                let (o, d) = ray(p);
                let h = pick::pick_all(scene, o.as_vec3(), d.as_vec3())
                    .into_iter()
                    .find(|h| h.id == id)?;
                Some(o + d * h.t as f64)
            };
            let grab_snap = |p: egui::Pos2, id: ElementId| {
                let only: Vec<SceneItem> = scene.iter().filter(|i| i.id == id).cloned().collect();
                let own = Snapper::from_scene(&only, &[]);
                own.snap(&snap_cfg.points_only(), p, &project, None, None)
                    .map(|s| s.p)
            };
            // the face under the pointer, what is edited left out: its point,
            // and its plane from two more rays a pixel aside
            let editing = exclude.clone();
            let face = |p: egui::Pos2| {
                let hit = |q: egui::Pos2| {
                    let (o, d) = ray(q);
                    let h = pick::pick_where(scene, o.as_vec3(), d.as_vec3(), &|id| {
                        !editing.contains(&id)
                    })?;
                    Some((h.id, o + d * h.t as f64))
                };
                let (id, a) = hit(p)?;
                let (ib, b) = hit(p + egui::vec2(1.0, 0.0))?;
                let (ic, c) = hit(p + egui::vec2(0.0, 1.0))?;
                if ib != id || ic != id {
                    return None;
                }
                let n = (b - a).cross(c - a).normalize_or_zero();
                (n != DVec3::ZERO).then_some((a, n))
            };
            let (_, look) = ray(rect.center());
            let input = PaneInput {
                ray: &ray_fn,
                project: &project,
                look,
                snap: &snap,
                grab_body: &grab_body,
                grab_snap: &grab_snap,
                body_normal: DVec3::Z,
                view: None,
                face: &face,
            };
            let mut status = None;
            let edit = self.plan.edit.is_some() && self.plan.edit == self.selection.active;
            let busy = self.plan.tool == Tool::Select
                && handles::interact(
                    &mut self.doc,
                    &painter,
                    &response,
                    self.selection.active,
                    self.face,
                    edit,
                    &mut self.handle_drag,
                    &mut self.plan.grips,
                    &input,
                    &mut status,
                );
            if let Some(s) = status {
                self.status = s;
            }
            busy
        };
        drop(_pd);
        if dragging {
            return;
        }
        // a left drag orbits in Classic, or with Alt (no middle button)
        let classic = self.shell.settings.navigation == "classic";
        if response.dragged_by(egui::PointerButton::Primary)
            && self.box_3d.is_none()
            && (classic || ui.input(|i| i.modifiers.alt))
        {
            let d = response.drag_delta();
            self.orbit_3d(d);
        }

        let slab_outline = self.plan.outline.as_ref().is_some_and(|o| !o.is_wall());
        if self.plan.tool.draws_outline() || slab_outline {
            let modes = self.plan.tool.draws_outline();
            let (doc, plan, sel) = (&mut self.doc, &mut self.plan, &mut self.selection);
            if let Some(msg) = crate::slabtool::controls(ui, rect, None, modes, doc, plan, sel) {
                self.status = msg;
            }
        }
        if self.plan.outline.is_some() {
            self.outline_3d(ui, &response, &painter, &project, &ray);
            return;
        }
        if self.plan.tool == Tool::Align {
            self.align_3d(ui, &response, &painter, &project, &ray);
            return;
        }
        if self.plan.tool != Tool::Select {
            self.tools_3d(ui, &response, &painter, &project, &ray);
            return;
        }

        let on_cube = response
            .interact_pointer_pos()
            .is_some_and(|p| self.cube.covers(p));
        if response.clicked() && !on_cube {
            let _perf = Perf::new("click_3d");
            if let Some(p) = response.interact_pointer_pos() {
                // sketch lines first: they have no surface to hit
                let near_sketch = self.sketch_near(p, &project);
                let shift = ui.input(|i| i.modifiers.shift);
                // a double-click on the selected element opens it in Edit mode
                if response.double_clicked() && !shift {
                    if let Some(id) = self.selection.active.filter(|id| {
                        self.under_3d(p, &project, &ray).contains(id) || near_sketch == Some(*id)
                    }) {
                        self.status = editmode::enter(self.doc.model(), id, &mut self.plan.edit);
                        return;
                    }
                }
                let pick = |sel: &mut Selection, id: Option<ElementId>| match (id, shift) {
                    (Some(id), true) => sel.toggle(id),
                    (None, true) => {}
                    (id, false) => sel.set(id),
                };
                let (o, d) = ray(p);
                // what Tab moved the pre-selection to, if not the first
                let chosen = self.plan.hover.as_mut().and_then(|h| {
                    h.list = false;
                    h.at(hoverpick::Pane::Window3d, p)
                });
                let first = self.under_3d(p, &project, &ray).first().copied();
                if let Some(id) = chosen.filter(|c| Some(*c) != first) {
                    pick(&mut self.selection, Some(id));
                    let hit = pick::pick_all(&self.scene, o.as_vec3(), d.as_vec3())
                        .into_iter()
                        .find(|h| h.id == id);
                    self.face = hit.filter(|_| !shift).and_then(|h| self.face_of_hit(&h));
                    return;
                }
                // grid bubbles are drawn over everything, so they take
                // clicks first
                let bubble = self.grid_bubble_near(p, &project);
                // a reference's lines, as a sketch's (MU33: only a 2D view
                // could pick one)
                let reference = self.reference_near_3d(p, &project);
                // and so are crop box frames
                let frame = || self.crop_frame_near(p, &project);
                if let Some(id) = near_sketch.or(bubble).or(reference).or_else(frame) {
                    pick(&mut self.selection, Some(id));
                    self.face = None;
                    return;
                }
                let hit = self.first_hit_3d(o, d);
                // guides and grid lines cross everything, so they only take
                // clicks that miss
                if hit.is_none() {
                    let near = self
                        .grid_line_near(p, &project)
                        .map(|g| g.0)
                        .or_else(|| self.guide_near(p, &project));
                    if let Some(id) = near {
                        pick(&mut self.selection, Some(id));
                        self.face = None;
                        return;
                    }
                }
                pick(&mut self.selection, hit.map(|h| h.id));
                if shift {
                    self.face = None;
                    return;
                }
                self.face = hit.and_then(|h| self.face_of_hit(&h));
            }
        }
    }

    /// A point (or, for Split, a guide) asked for, picked in 3D: on the
    /// working level under the pointer. True while it takes the clicks.
    fn point_pick_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) -> bool {
        if !self.plan.point_pick {
            return false;
        }
        if response.secondary_clicked() {
            self.plan.point_pick = false;
            return true;
        }
        let Some(h) = response.hover_pos() else {
            return true;
        };
        let z = self.work_level().map_or(0.0, |(_, z)| z);
        let (o, d) = ray(h);
        let Some(p) = Workplane::ground(z).intersect(o, d) else {
            return true;
        };
        let step = crate::snaps::current_step();
        let w = if step > 0.0 {
            (p.truncate() / step).round() * step
        } else {
            p.truncate()
        };
        self.plan.point_hover = Some(w);
        // a guide under the pointer, for Split's line
        let guide = self
            .plan
            .point_lines
            .then(|| self.guide_near(h, project))
            .flatten()
            .and_then(|id| match self.doc.model().get(id).map(|e| &e.kind) {
                Some(ElementKind::Guide(g)) => Some((g.a.truncate(), g.b.truncate())),
                _ => None,
            });
        let red = egui::Stroke::new(1.6, crate::planview::SPLIT_RED);
        match guide {
            Some((a, b)) => {
                if let (Some(pa), Some(pb)) = (project(a.extend(z)), project(b.extend(z))) {
                    painter
                        .line_segment([pa, pb], egui::Stroke::new(3.0, crate::planview::SPLIT_RED));
                }
            }
            None => {
                if let Some(c) = project(w.extend(z)) {
                    let s = egui::Stroke::new(1.5, crate::planview::accent_selected());
                    painter.line_segment([c - egui::vec2(12.0, 0.0), c + egui::vec2(12.0, 0.0)], s);
                    painter.line_segment([c - egui::vec2(0.0, 12.0), c + egui::vec2(0.0, 12.0)], s);
                    painter.circle_stroke(c, 5.0, if self.split.is_some() { red } else { s });
                }
            }
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if response.clicked() {
            match guide {
                Some(l) => self.plan.point_line_picked = Some(l),
                None => self.plan.point_picked = Some(w),
            }
            if !self.plan.point_lines && self.split.is_none() {
                self.plan.point_pick = false;
            }
        }
        true
    }

    /// The face of a mass a hit lands on.
    fn face_of_hit(&self, h: &pick::Hit) -> Option<usize> {
        match self.doc.model().get(h.id).map(|e| &e.kind) {
            Some(ElementKind::FreeMesh(m)) => {
                let ev = m.evaluated();
                let f = pick::face_of_triangle(&ev.mesh, h.triangle)?;
                ev.source.get(f).copied().flatten()
            }
            _ => None,
        }
    }

    /// The element a click on the 3D view takes along a ray: the nearest,
    /// or in X-ray the nearest of what is inside others.
    fn first_hit_3d(&self, o: DVec3, d: DVec3) -> Option<pick::Hit> {
        let mut hits = pick::pick_all(&self.scene, o.as_vec3(), d.as_vec3());
        if self.shell.settings.xray {
            hits.sort_by_key(|h| !self.is_framing_id(h.id));
        }
        hits.first().copied()
    }

    fn is_framing_id(&self, id: ElementId) -> bool {
        self.doc
            .model()
            .get(id)
            .is_some_and(|e| is_framing(&e.kind))
    }

    /// The sketch line within a few pixels of `p`, if any.
    fn sketch_near(
        &self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) -> Option<ElementId> {
        creabim_geom::scene::sketch_lines(self.doc.model())
            .into_iter()
            .filter_map(|(id, a, b)| {
                let (a, b) = (project(a)?, project(b)?);
                let d = egui::Rect::from_two_pos(a, b)
                    .expand(6.0)
                    .contains(p)
                    .then(|| {
                        let ab = b - a;
                        let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                        (a + ab * t).distance(p)
                    })?;
                (d < 6.0).then_some((id, d))
            })
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|s| s.0)
    }

    /// Everything under point `p` of the 3D view, in the order a click
    /// takes it: sketch lines and grid bubbles, then what the ray passes
    /// (in X-ray, what is inside others first), else grid lines and guides.
    fn under_3d(
        &self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) -> Vec<ElementId> {
        let mut out: Vec<ElementId> = Vec::new();
        out.extend(self.sketch_near(p, project));
        out.extend(self.grid_bubble_near(p, project));
        // crop box frames are drawn over everything too, and take clicks
        // the same way
        out.extend(
            self.crop_frame_near(p, project)
                .filter(|f| !out.contains(f)),
        );
        // the scene is cropped, so what a box crops away is not under it
        let (o, d) = ray(p);
        let mut hits = pick::pick_all(&self.scene, o.as_vec3(), d.as_vec3());
        if self.shell.settings.xray {
            hits.sort_by_key(|h| !self.is_framing_id(h.id));
        }
        if hits.is_empty() {
            let near = self
                .grid_line_near(p, project)
                .map(|g| g.0)
                .or_else(|| self.guide_near(p, project));
            out.extend(near);
        }
        for h in hits {
            if !out.contains(&h.id) {
                out.push(h.id);
            }
        }
        out
    }

    /// The 3D view's pre-selection: read what is under the pointer, trace
    /// the element a click would take (or that another view is reading),
    /// and show the badge when several are under it.
    fn hover_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
        modal_here: bool,
    ) {
        let pane = hoverpick::Pane::Window3d;
        if !response.dragged() {
            // (the Align tool takes faces here, so Tab reaches them too)
            let reading = matches!(self.plan.tool, Tool::Select | Tool::Align)
                && !modal_here
                && self.plan.modal.is_none()
                && self.plane_pick.is_none()
                && self.box_3d.is_none()
                && !self.box_armed
                // in Edit mode Tab is Layers', in a drag the typed length's
                && self.plan.edit.is_none()
                && self.handle_drag.is_none();
            let pointer = response.hover_pos().filter(|_| reading);
            let rev = self.doc.model().revision();
            let mut slot = self.plan.hover.take();
            let said = hoverpick::track(
                ui.ctx(),
                &mut slot,
                pane,
                pointer,
                rev,
                self.doc.model(),
                |p| self.under_3d(p, project, ray),
            );
            self.plan.hover = slot;
            if let Some(s) = said {
                self.status = s;
            }
        }
        let lit = hoverpick::highlighted(&self.plan.hover)
            .filter(|id| !self.selection.contains(*id) && !response.dragged());
        if let Some(id) = lit {
            let stroke = egui::Stroke::new(2.0, hoverpick::HOVER.gamma_multiply(0.85));
            let seg = |a: DVec3, b: DVec3| {
                if let (Some(a), Some(b)) = (project(a), project(b)) {
                    painter.line_segment([a, b], stroke);
                }
            };
            for item in self.scene.iter().filter(|i| i.id == id) {
                for (a, b) in snaps::feature_edges(&item.mesh) {
                    seg(a, b);
                }
            }
            for (sid, a, b) in creabim_geom::scene::sketch_lines(self.doc.model()) {
                if sid == id {
                    seg(a, b);
                }
            }
            // a guide lights along its whole reach, as plans light it (MU39)
            if let Some(ElementKind::Guide(g)) = self.doc.model().get(id).map(|e| &e.kind) {
                if let Some(dir) = (g.b - g.a).try_normalize() {
                    let m = (g.a + g.b) / 2.0;
                    let steps = 120;
                    let at = |i: usize| {
                        m + dir * (-GUIDE_REACH_3D + 2.0 * GUIDE_REACH_3D * i as f64 / steps as f64)
                    };
                    for i in 0..steps {
                        seg(at(i), at(i + 1));
                    }
                }
            }
        }
        if let Some(id) = hoverpick::badge(ui.ctx(), &mut self.plan.hover, pane, self.doc.model()) {
            if ui.input(|i| i.modifiers.shift) {
                self.selection.toggle(id);
            } else {
                self.selection.set(Some(id));
            }
            self.face = None;
            self.status = format!("{} selected.", hoverpick::describe(self.doc.model(), id));
        }
    }

    /// Rebuild the 3D snaps when the model changed, but not during a drag,
    /// which changes the model every frame.
    fn refresh_snaps_3d(&mut self, exclude: &[ElementId]) {
        let _perf = Perf::new("snaps_3d");
        let rev = self.doc.model().revision();
        let frozen = self.handle_drag.is_some()
            || self
                .plan
                .modal
                .as_ref()
                .is_some_and(|m| m.pane.is_none() && m.freezes_snaps());
        // built once per scene with everything; the selection is hidden
        // from it, not left out of a new one (MU39: 60 ms a click)
        // and not while a drag runs anywhere (a grip dragged in a plan
        // changes the model every frame: MU39, 60 ms a frame here)
        let frozen = frozen || crate::trace::held();
        let fresh = self
            .snaps_3d
            .as_ref()
            .is_some_and(|(r, _, _)| *r == rev || frozen);
        if !fresh {
            let mut snapper = Snapper::from_scene(&self.scene, &[]);
            snapper.add_guides(self.doc.model(), GUIDE_REACH_3D);
            snapper.add_grids(self.doc.model(), &self.hidden_in_3d());
            snapper.add_sketches(self.doc.model(), &[]);
            // the references' lines, where 3D shows them (MU33)
            let mut n = 0usize;
            for (id, r) in self.refs_in_3d() {
                let model = self.doc.model();
                if let crate::trace::Shown::Lines(lines) = self.plan.ref_cache.get(model, id, &r) {
                    for l in lines.iter() {
                        for w in l.windows(2) {
                            if n < 20_000 {
                                let a = crate::handles::ref_world(model, &r, r.place(w[0]));
                                let b = crate::handles::ref_world(model, &r, r.place(w[1]));
                                snapper.add_segment(a, b, crate::snaps::Source::Underlay, Some(id));
                                n += 1;
                            }
                        }
                    }
                }
            }
            self.snaps_3d = Some((rev, vec![], snapper));
        }
        if let Some((_, ex, s)) = &mut self.snaps_3d {
            if ex != exclude {
                *ex = exclude.to_vec();
                s.hide(exclude);
            }
        }
    }

    fn set_workplane(&mut self, plane: Workplane, name: String) {
        self.status = format!(
            "Workplane: {name}. Sketches and guides drawn in 3D go on it. Pick Level in the Workplane list to go back."
        );
        self.workplane = Some((plane, name));
        self.geometry_key = None;
    }

    /// Picking a workplane: a face, or three points. Returns true while it
    /// takes the view's input.
    fn pick_plane_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) -> bool {
        let Some(mode) = self.plane_pick.clone() else {
            return false;
        };
        let accent = crate::theme::interaction();
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc || response.secondary_clicked() {
            self.plane_pick = None;
            self.status = "Workplane unchanged.".into();
            return true;
        }
        let Some(hover) = response.hover_pos() else {
            return true;
        };
        let (o, d) = ray(hover);
        match mode {
            PlanePick::Face => {
                let plane = face_plane(&self.scene, o, d);
                if let Some(plane) = plane {
                    crate::sketch::draw_plane(painter, project, &plane, accent);
                }
                self.status = "Click the face the workplane lies on. Right-click cancels.".into();
                if response.clicked() {
                    match plane {
                        Some(plane) => {
                            self.plane_pick = None;
                            self.set_workplane(plane, "on a face".into());
                        }
                        None => {
                            self.status = "That isn't a face. Click a wall, slab or mass.".into()
                        }
                    }
                }
            }
            PlanePick::Points(mut pts) => {
                self.refresh_snaps_3d(&[]);
                let free = pick::pick(&self.scene, o.as_vec3(), d.as_vec3())
                    .map(|h| o + d * h.t as f64)
                    .or_else(|| Workplane::ground(0.0).intersect(o, d));
                let snapper = &self.snaps_3d.as_ref().expect("just built").2;
                let snap_cfg = self.plan.snap.live(ui.input(|i| i.modifiers.alt));
                let snap = snapper.snap(&snap_cfg, hover, project, pts.last().copied(), free);
                let at = snap.map(|s| s.p);
                if let (Some(s), Some(c)) = (snap, at.and_then(project)) {
                    snaps::draw(painter, s, c, None);
                }
                let mut shown = pts.clone();
                shown.extend(at);
                let scr: Vec<egui::Pos2> = shown.iter().filter_map(|p| project(*p)).collect();
                for c in &scr {
                    painter.circle_filled(*c, 3.5, accent);
                }
                if scr.len() > 1 {
                    painter.add(egui::Shape::closed_line(
                        scr,
                        egui::Stroke::new(1.5, accent),
                    ));
                }
                self.status = format!(
                    "Click point {} of 3 for the workplane: the first two set its x axis. Right-click cancels.",
                    pts.len() + 1
                );
                if response.clicked() {
                    if let Some(p) = at {
                        pts.push(p);
                    }
                    if pts.len() == 3 {
                        self.plane_pick = None;
                        let x = (pts[1] - pts[0]).normalize_or_zero();
                        let n = x.cross(pts[2] - pts[0]).normalize_or_zero();
                        if x == DVec3::ZERO || n == DVec3::ZERO {
                            self.status =
                                "Those points are in a line. Pick the workplane again.".into();
                        } else {
                            let plane = Workplane {
                                origin: pts[0],
                                x,
                                y: n.cross(x),
                            };
                            self.set_workplane(plane, "through three points".into());
                        }
                    } else {
                        self.plane_pick = Some(PlanePick::Points(pts));
                    }
                }
            }
        }
        true
    }

    /// A guide drawn on the workplane in 3D: two points, snapped and laid
    /// onto the plane.
    fn guide_on_plane(
        &mut self,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
        plane: Workplane,
    ) {
        let accent = crate::theme::interaction();
        let Some(hover) = response.hover_pos() else {
            return;
        };
        let (o, d) = ray(hover);
        let n = plane.normal();
        let onto = |p: DVec3| p - n * (p - plane.origin).dot(n);
        let from = self.guide_from;
        self.refresh_snaps_3d(&[]);
        let snapper = &self.snaps_3d.as_ref().expect("just built").2;
        let snap_cfg = self.plan.snap.live(response.ctx.input(|i| i.modifiers.alt));
        let Some(snap) = snapper.snap(&snap_cfg, hover, project, from, plane.intersect(o, d))
        else {
            return;
        };
        let at = onto(snap.p);
        if let Some(c) = project(at) {
            snaps::draw(painter, snap, c, from.and_then(project));
        }
        if let Some(a) = from {
            draw_guide_3d(painter, project, a, at, egui::Stroke::new(2.0, accent));
        }
        self.status = match from {
            None => "Guide on the workplane: click a point it runs through.".into(),
            Some(a) => format!(
                "{}. Click a second point.",
                crate::units::len(a.distance(at))
            ),
        };
        if !response.clicked() {
            return;
        }
        match from {
            None => self.guide_from = Some(at),
            Some(a) if a.distance(at) > 0.01 => {
                self.guide_from = None;
                let e = creabim_core::Element::new(
                    "Guide",
                    ElementKind::Guide(creabim_core::Guide {
                        level: None,
                        a,
                        b: at,
                    }),
                );
                let r = self
                    .doc
                    .commit(creabim_core::Transaction::new("Add guide").insert(e));
                self.report(r, "Guide added on the workplane.");
            }
            Some(_) => {}
        }
    }

    /// The level the 3D drawing tools draw on: the active plan's, else the
    /// selected element's, else the lowest.
    fn work_level(&self) -> Option<(ElementId, f64)> {
        let m = self.doc.model();
        let plan = self
            .active_view
            .and_then(|v| match self.view(v).map(|v| &v.kind) {
                Some(ViewKind::Plan { level, .. }) => Some(*level),
                _ => None,
            });
        let of_selection = self
            .selection
            .active
            .and_then(|id| match m.get(id).map(|e| &e.kind) {
                Some(ElementKind::Wall(w)) => Some(w.level),
                Some(ElementKind::Slab(s)) => Some(s.level),
                Some(ElementKind::FreeMesh(f)) => f.level,
                Some(ElementKind::Level(_)) => Some(id),
                Some(k) => creabim_core::members::level_of(k),
                _ => None,
            });
        let lowest = m
            .levels()
            .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
            .map(|(e, _)| e.id);
        let id = plan.or(of_selection).or(lowest)?;
        Some((id, m.level(id).map_or(0.0, |l| l.elevation)))
    }

    /// Walls, slabs, doors, windows, masses and section lines drawn right in
    /// the 3D view, on the working level, with the same snaps as dragging.
    /// The Align tool in the 3D window: a reference, then a face to align,
    /// stretch or extend to it (see [`crate::align3d`]).
    fn align_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) {
        // on a sloped or upright workplane, things turn on it to line up
        self.plan.align.workplane = self.workplane.as_ref().map(|w| w.0);
        // the tool's hint, for 3D, once the pointer is here
        if response.hovered() && self.plan.align.reference_3d.is_none() {
            self.status = crate::align3d::hint(&self.plan.align);
        }
        let under = match response.hover_pos() {
            Some(p) => {
                let (o, d) = ray(p);
                let chosen = self
                    .plan
                    .hover
                    .as_ref()
                    .and_then(|h| h.at(hoverpick::Pane::Window3d, p));
                let face = crate::align3d::face_pick(self.doc.model(), &self.scene, o, d, chosen);
                self.refresh_snaps_3d(&[]);
                let snapper = &self.snaps_3d.as_ref().expect("just built").2;
                let cfg = self.plan.snap.live(ui.input(|i| i.modifiers.alt));
                let free = face.as_ref().map(|f| f.face.point);
                let snap = snapper
                    .snap(&cfg, p, project, None, free)
                    .filter(|s| s.kind != SnapKind::Grid);
                let edge = snap.and_then(|s| snapper.edge_at(s.p));
                let (centres, centre) = crate::align3d::centres_under(
                    self.doc.model(),
                    &self.scene,
                    face.as_ref(),
                    p,
                    project,
                );
                let centre = centre.filter(|_| crate::align3d::takes_centres(&self.plan.align));
                crate::align3d::Under {
                    face,
                    snap,
                    edge,
                    level: self.work_level(),
                    centres,
                    centre,
                }
            }
            None => Default::default(),
        };
        crate::align3d::draw(painter, project, self.doc.model(), &self.plan.align, &under);
        if response.clicked() {
            if let Some(h) = self.plan.hover.as_mut() {
                h.list = false;
            }
            self.status = crate::align3d::click(
                &mut self.doc,
                &mut self.plan.align,
                &under,
                &mut self.selection,
            );
            self.face = None;
        }
    }

    /// The Beam tool's On face mode in 3D: the first click picks a face
    /// (a roof slope, a wall) and a point on it; every next click is kept
    /// in that face's plane, and each beam lies on the face, its section
    /// square to it, sitting on its surface. Clicks chain beams; Esc or a
    /// right-click ends the chain.
    fn beam_on_face_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
        level: ElementId,
    ) {
        let accent = crate::theme::interaction();
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) || response.secondary_clicked() {
            if self.beam_face.take().is_none() {
                self.plan.tool = Tool::Select;
            }
            self.status = "Beams on face: click a face to start.".into();
            return;
        }
        let Some(hover) = response.hover_pos() else {
            return;
        };
        let (o, d) = ray(hover);
        // where the pointer is: on the face picked first, else on the face
        // under it
        let face = pick::pick_face(&self.scene, o.as_vec3(), d.as_vec3());
        let (at, plane) = match self.beam_face {
            Some((_, p0, n)) => {
                let den = d.dot(n);
                if den.abs() < 1e-9 {
                    return;
                }
                let t = (p0 - o).dot(n) / den;
                if t <= 0.0 {
                    return;
                }
                (o + d * t, Some((p0, n)))
            }
            None => match &face {
                Some(f) => (f.point, Some((f.point, f.normal))),
                None => {
                    if response.clicked() {
                        self.status =
                            "Beams on face: click a face (a roof slope, a wall, a slab).".into();
                    }
                    return;
                }
            },
        };
        // the face's corners and the middles of its edges snap
        let mut at = at;
        if let Some(f) = &face {
            let mut best = 12.0f32;
            for tri in &f.triangles {
                for k in 0..3 {
                    for q in [tri[k], (tri[k] + tri[(k + 1) % 3]) / 2.0] {
                        let on_plane = plane.is_none_or(|(p0, n)| (q - p0).dot(n).abs() < 1e-4);
                        if let (Some(s), true) = (project(q), on_plane) {
                            let dd = s.distance(hover);
                            if dd < best {
                                best = dd;
                                at = q;
                            }
                        }
                    }
                }
            }
            if best < 12.0 {
                if let Some(s) = project(at) {
                    painter.rect_stroke(
                        egui::Rect::from_center_size(s, egui::vec2(10.0, 10.0)),
                        0.0,
                        egui::Stroke::new(1.5, accent),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        }
        let set = self.plan.members.clone();
        if let Some((a, _, n)) = self.beam_face {
            if let (Some(sa), Some(sb)) = (project(a), project(at)) {
                painter.line_segment([sa, sb], egui::Stroke::new(2.0, accent));
                // its depth, standing off the face
                if let (Some(sa2), Some(sb2)) = (
                    project(a + n * set.beam_depth),
                    project(at + n * set.beam_depth),
                ) {
                    painter.add(egui::Shape::closed_line(
                        vec![sa, sb, sb2, sa2],
                        egui::Stroke::new(1.0, accent),
                    ));
                }
                painter.text(
                    sb + egui::vec2(10.0, -12.0),
                    egui::Align2::LEFT_BOTTOM,
                    crate::units::len(a.distance(at)),
                    egui::FontId::proportional(13.0),
                    accent,
                );
            }
        } else if let Some(s) = project(at) {
            painter.circle_stroke(s, 6.0, egui::Stroke::new(1.5, accent));
        }
        if !response.clicked() {
            return;
        }
        let Some((p0, n)) = plane else { return };
        let Some((a, _, _)) = self.beam_face else {
            self.plan.members.last_beam = None;
            self.beam_face = Some((at, p0, n));
            self.status = "Beam on face: click its other end; it stays on this face.".into();
            return;
        };
        if a.distance(at) < 1e-3 {
            return;
        }
        // its top line stands off the face by its section's own height,
        // so it sits on it (MU40: a library section floated by the beam
        // depth instead)
        let model = self.doc.model();
        let section = set
            .beam_section
            .as_deref()
            .and_then(|nm| creabim_core::profiles::find(model, nm));
        let height = match &section {
            Some(pts) if !pts.is_empty() => {
                let lo = pts.iter().map(|p| p.y).fold(f64::MAX, f64::min);
                let hi = pts.iter().map(|p| p.y).fold(f64::MIN, f64::max);
                hi - lo
            }
            _ => set.beam_depth,
        };
        let (ta, tb) = (a + n * height, at + n * height);
        // it belongs to the level it sits on, the highest one at or below
        // its lowest point, not the plan that was open (MU40: drawn on the
        // roof, it was on the ground floor and the roof plan never showed
        // it)
        let low = ta.z.min(tb.z) - height;
        let level = model
            .levels()
            .filter(|(_, l)| l.elevation <= low + 1e-6)
            .max_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
            .map_or(level, |(e, _)| e.id);
        let top0 = creabim_core::members::elevation(model, level)
            + creabim_core::members::storey_height(model, level);
        let mut b = creabim_core::Beam::new(level, ta.truncate(), tb.truncate());
        b.width = set.beam_width;
        b.depth = set.beam_depth;
        b.top_offset = top0 - ta.z;
        b.rise = [0.0, tb.z - ta.z];
        b.up = Some(n);
        if let Some(nm) = set.beam_section.as_deref() {
            if let Some(pts) = creabim_core::profiles::find(model, nm) {
                b.section = Some(creabim_core::members::Section {
                    name: nm.to_string(),
                    points: pts,
                });
            }
        }
        let e = self
            .plan
            .typed(model, Element::new("Beam", ElementKind::Beam(b)));
        // the one before in this chain, ending where this starts
        let prev = self.plan.members.last_beam.and_then(|id| model.get(id).cloned()).filter(|pe| {
            matches!(&pe.kind, ElementKind::Beam(pb) if pb.end.distance(ta.truncate()) < 1e-6)
        });
        let id = e.id;
        self.plan.members.last_beam = Some(id);
        let tx = match prev {
            Some(pe) => {
                let (mut pe2, mut ne) = (pe.clone(), e);
                if let (ElementKind::Beam(pb), ElementKind::Beam(nb)) =
                    (&mut pe2.kind, &mut ne.kind)
                {
                    creabim_core::members::join_beams(pb, nb, set.beam_corner);
                }
                Transaction::new("Beam on face").replace(pe, pe2).insert(ne)
            }
            None => Transaction::new("Beam on face").insert(e),
        };
        let r = self.doc.commit(tx);
        self.selection.only(id);
        self.beam_face = Some((at, p0, n));
        self.report(
            r,
            "Beam on the face. Click on to chain the next; Esc or a right-click ends.",
        );
    }

    fn tools_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) {
        let tool = self.plan.tool;
        if tool.is_annotation() {
            if response.clicked() {
                self.status = format!(
                    "The {} tool works in plans and sections for now.",
                    tool.label()
                );
            }
            return;
        }
        if let (Tool::Guide, Some(plane)) = (tool, self.workplane.as_ref().map(|w| w.0)) {
            self.guide_on_plane(response, painter, project, ray, plane);
            return;
        }
        let Some((level, z)) = self.work_level() else {
            return;
        };
        let accent = crate::theme::interaction();
        let stroke = egui::Stroke::new(2.0, accent);
        // a right-click with nothing drawn leaves the Slab tool
        if tool.draws_outline() && response.secondary_clicked() && self.pending_3d.is_none() {
            self.plan.tool = Tool::Select;
            self.status = format!("Left the {} tool.", tool.label());
            return;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) || response.secondary_clicked() {
            self.pending_3d = None;
        }
        let Some(hover) = response.hover_pos() else {
            return;
        };
        let (o, d) = ray(hover);
        let hit = pick::pick(&self.scene, o.as_vec3(), d.as_vec3());
        let level_name = self
            .doc
            .model()
            .get(level)
            .map_or(String::new(), |e| e.name.clone());

        // doors and windows go into the wall under the pointer
        if matches!(tool, Tool::Door | Tool::Window | Tool::Opening) {
            let wall = hit.and_then(|h| match self.doc.model().get(h.id).map(|e| &e.kind) {
                Some(ElementKind::Wall(w)) => Some((h, w.level)),
                Some(ElementKind::Opening(op)) => {
                    match self.doc.model().get(op.host).map(|e| &e.kind) {
                        Some(ElementKind::Wall(w)) => Some((h, w.level)),
                        _ => None,
                    }
                }
                _ => None,
            });
            if let Some((h, wl)) = wall {
                let at = o + d * h.t as f64;
                // the window or door a click puts here (A7-4)
                match crate::ghost::cached(self.doc.model(), wl, tool, at.truncate(), &self.plan) {
                    Some(g) => {
                        crate::ghost::draw(painter, &project, &g, accent);
                        self.plan.ghost_now = Some((Some(usize::MAX), g));
                    }
                    None => {
                        if let Some(c) = project(at) {
                            painter.circle_stroke(c, 6.0, stroke);
                        }
                    }
                }
                if response.clicked() {
                    let mut sel = self.selection.clone();
                    let mut state = std::mem::take(&mut self.plan);
                    let r = planview::plan_tool(
                        &mut self.doc,
                        wl,
                        tool,
                        at.truncate(),
                        &mut state,
                        &mut sel,
                    );
                    self.plan = state;
                    self.selection = sel;
                    self.report(
                        r,
                        format!(
                            "{} placed. Drag its round handles to resize it.",
                            tool.label()
                        ),
                    );
                }
            } else if response.clicked() {
                self.status = format!(
                    "Click a wall to put the {} in it.",
                    tool.label().to_lowercase()
                );
            }
            return;
        }

        // a beam on a face: it lies in the face's plane, square to it,
        // wherever the second point is drawn (MU40)
        if tool == Tool::Beam && self.plan.members.beam_on_face {
            self.beam_on_face_3d(ui, response, painter, project, ray, level);
            return;
        }
        // everything else is drawn on the working level
        let from = self
            .pending_3d
            .filter(|(_, l)| *l == level)
            .map(|(p, _)| p.extend(z));
        let free = Workplane::ground(z).intersect(o, d);
        let exclude: Vec<ElementId> = vec![];
        self.refresh_snaps_3d(&exclude);
        let snapper = &self.snaps_3d.as_ref().expect("just built").2;
        let snap_cfg = self.plan.snap.live(ui.input(|i| i.modifiers.alt));
        // the corners of the chain of walls (or beams) drawn so far
        let chain: Vec<DVec3> = match (&self.plan.chain, from) {
            ((Some(l), pts), Some(_)) if *l == level => pts.iter().map(|p| p.extend(z)).collect(),
            _ => vec![],
        };
        // and the points rested on, as in plans (MU26: walls, beams and
        // the other level tools in 3D too)
        // drafting's temporary guides on the level too (MU37): from the
        // points rested on and through where the line starts
        let guides = guides_3d(&self.rested_3d.0, from, z, self.plan.guide_step, &snap_cfg);
        let rested: Vec<DVec3> = if guides.is_empty() {
            self.rested_3d
                .0
                .iter()
                .map(|p| p.truncate().extend(z))
                .collect()
        } else {
            vec![]
        };
        let track = snaps::track_points(&chain, &rested);
        let Some(hit) = snapper.snap_guided(&snap_cfg, hover, project, from, &track, &guides, free)
        else {
            return;
        };
        snaps::draw_guides(painter, project, &guides, &hit.guides);
        let (snap, align) = (hit.snap, hit.align);
        rest_points(&mut self.rested_3d, ui.ctx(), Some(&snap), true);
        let p = snap.p.truncate();
        let at = p.extend(z);
        // a point snapped above or below the working plane drops onto it:
        // a dashed line shows where it lands (ArchiCAD's projections, MU18)
        if (snap.p.z - z).abs() > 1e-3 {
            if let (Some(a), Some(b)) = (project(snap.p), project(at)) {
                painter.extend(egui::Shape::dashed_line(
                    &[a, b],
                    egui::Stroke::new(1.2, Color32::from_rgb(78, 123, 196)),
                    5.0,
                    4.0,
                ));
                painter.circle_stroke(
                    a,
                    4.0,
                    egui::Stroke::new(1.2, Color32::from_rgb(78, 123, 196)),
                );
                painter.circle_filled(b, 3.0, Color32::from_rgb(78, 123, 196));
            }
        }
        if let Some(c) = project(at) {
            if snap.kind == SnapKind::Grid {
                painter.circle_stroke(c, 4.0, egui::Stroke::new(1.0, accent));
            } else if !hit.guides.is_empty() {
                snaps::draw_guided(painter, project, &hit, &guides, from.and_then(project));
            } else {
                snaps::draw_tracked(painter, project, snap, align, c, from.and_then(project));
            }
        }
        if from.is_none() && crate::ghost::shows(tool) {
            if let Some(g) = crate::ghost::cached(self.doc.model(), level, tool, p, &self.plan) {
                crate::ghost::draw(painter, &project, &g, accent);
                self.plan.ghost_now = Some((Some(usize::MAX), g));
            }
        }
        if tool == Tool::Object {
            if ui.input(|i| !i.modifiers.command && i.key_pressed(egui::Key::Space))
                && !ui.ctx().egui_wants_keyboard_input()
            {
                self.plan.objects.rotation = (self.plan.objects.rotation + 90.0).rem_euclid(360.0);
            }
            crate::objecttool::preview(
                painter,
                project,
                self.doc.model(),
                level,
                p,
                &self.plan,
                true,
            );
        }
        if let Some(a) = from {
            match tool {
                Tool::Guide => {
                    let (through, dir) = match self.plan.guide_edge {
                        Some((e0, e1)) => (at, (e1 - e0).extend(0.0)),
                        None => (a, at - a),
                    };
                    draw_guide_3d(painter, project, through, through + dir, stroke);
                    if let Some(sb) = project(at) {
                        let text = match self.plan.guide_edge {
                            Some((e0, e1)) => {
                                let d = (e1 - e0).normalize_or_zero();
                                crate::units::len((p - e0).perp_dot(d).abs())
                            }
                            None => crate::units::len(a.truncate().distance(p)),
                        };
                        painter.text(
                            sb + Vec2::new(10.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            text,
                            egui::FontId::proportional(13.0),
                            accent,
                        );
                    }
                }
                t if t.draws_outline() => {
                    let (lo, hi) = (a.truncate().min(p), a.truncate().max(p));
                    let pts = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y), lo];
                    let scr: Vec<egui::Pos2> =
                        pts.iter().filter_map(|q| project(q.extend(z))).collect();
                    painter.add(egui::Shape::line(scr, stroke));
                }
                _ => {
                    crate::memberstool::preview(
                        painter,
                        project,
                        self.doc.model(),
                        Some(level),
                        tool,
                        &self.plan,
                        a,
                        at,
                    );
                    // a shaped wall (arc, circle, rectangle) draws its own
                    // lines below: no straight one to the pointer
                    let shaped = tool == Tool::Wall
                        && crate::planview::wall_shape_preview(
                            &self.plan,
                            a.truncate(),
                            at.truncate(),
                        )
                        .is_some();
                    if let (Some(sa), Some(sb)) = (project(a), project(at)) {
                        if !shaped {
                            painter.line_segment([sa, sb], stroke);
                        }
                        painter.text(
                            sb + Vec2::new(10.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            crate::units::len(a.truncate().distance(p)),
                            egui::FontId::proportional(13.0),
                            accent,
                        );
                    }
                    if tool == Tool::Wall {
                        // the wall's height, so it reads as a wall; an arc,
                        // a circle or a rectangle drawn as it will be (MU40)
                        let h = self.doc.model().level(level).map_or(3.0, |l| l.height);
                        let up = DVec3::Z * h;
                        let lines = crate::planview::wall_shape_preview(
                            &self.plan,
                            a.truncate(),
                            at.truncate(),
                        )
                        .unwrap_or_else(|| vec![vec![a.truncate(), at.truncate()]]);
                        for pts in lines {
                            let base: Vec<DVec3> = pts.iter().map(|p| p.extend(z)).collect();
                            for (k, row) in [base.clone(), base.iter().map(|p| *p + up).collect()]
                                .into_iter()
                                .enumerate()
                            {
                                let scr: Vec<egui::Pos2> =
                                    row.iter().filter_map(|q| project(*q)).collect();
                                painter.add(egui::Shape::line(
                                    scr,
                                    egui::Stroke::new(if k == 0 { 2.0 } else { 1.0 }, accent),
                                ));
                            }
                            for q in [base.first(), base.last()].into_iter().flatten() {
                                if let (Some(s0), Some(s1)) = (project(*q), project(*q + up)) {
                                    painter.line_segment([s0, s1], egui::Stroke::new(1.0, accent));
                                }
                            }
                        }
                    }
                }
            }
        }
        if response.clicked() && tool == Tool::Guide && from.is_none() {
            // clicking an edge first draws the guide parallel to it
            self.plan.guide_edge = matches!(
                snap.kind,
                SnapKind::OnEdge | SnapKind::Guide | SnapKind::Midpoint
            )
            .then(|| snapper.edge_at(snap.p))
            .flatten()
            .filter(|(a, b)| a.truncate().distance(b.truncate()) > 1e-3)
            .map(|(a, b)| (a.truncate(), b.truncate()));
        }
        if response.clicked() {
            let mut sel = self.selection.clone();
            let mut state = std::mem::take(&mut self.plan);
            state.pending = self.pending_3d.filter(|(_, l)| *l == level).map(|(p, _)| p);
            let r = planview::plan_tool(&mut self.doc, level, tool, p, &mut state, &mut sel);
            self.pending_3d = state.pending.take().map(|p| (p, level));
            self.plan = state;
            self.selection = sel;
            if let Err(e) = r {
                self.status = format!("Couldn't do that: {e}");
            } else {
                self.status = match (tool, self.pending_3d) {
                    (Tool::Wall, Some(_)) => format!("Drawing walls on {level_name}. Click the next corner; Esc or right-click ends."),
                    (t, _) if t.draws_outline() || t == Tool::RoomSeparator => {
                        self.plan.status.take().unwrap_or_default()
                    }
                    (Tool::Column | Tool::Beam | Tool::Stair | Tool::Railing | Tool::Footing | Tool::Object | Tool::Sweep | Tool::Planting, _) => self
                        .plan
                        .status
                        .take()
                        .unwrap_or_else(|| format!("Added on {level_name}.")),
                    (Tool::Section | Tool::Elevation, Some(_)) => "Click the second corner.".into(),
                    (Tool::Guide, Some(_)) if self.plan.guide_edge.is_some() => "Click where the parallel guide goes.".into(),
                    (Tool::Guide, Some(_)) => "Click a second point the guide runs through.".into(),
                    _ => format!("Added on {level_name}."),
                };
            }
        } else if from.is_none() && response.hovered() && !tool.draws_outline() {
            self.status = format!(
                "{} on {level_name} (the open plan's level). {}",
                tool.label(),
                tool.hint()
            );
        }
    }

    /// While drawing in 3D, a point snapped to and rested on for a moment
    /// joins the points the next one lines up with (the last four), as in
    /// plans; a frame is asked for after the dwell, as nothing repaints
    /// while the pointer rests (MU24).
    fn rest_3d(&mut self, ctx: &egui::Context, snap: Option<&crate::snaps::Snap>, drawing: bool) {
        rest_points(&mut self.rested_3d, ctx, snap, drawing);
    }
}

/// The temporary guides of drafting, on a level's plane in 3D (MU37: "make
/// sure drafting temp guides also appear when modeling"): level, plumb and
/// angled through each point rested on, and angled through `from`.
fn guides_3d(
    rested: &[DVec3],
    from: Option<DVec3>,
    z: f64,
    step: f64,
    cfg: &crate::snaps::SnapSettings,
) -> Vec<crate::snaps::TempGuide> {
    if !cfg.allows(SnapKind::OnTempGuide) || (rested.is_empty() && from.is_none()) {
        return vec![];
    }
    let refs: Vec<snaps::Reference> = rested
        .iter()
        .map(|p| snaps::Reference::Point {
            p: *p,
            kind: SnapKind::Endpoint,
        })
        .collect();
    let mut g = snaps::temp_guides(&refs, from, step);
    snaps::lift(&mut g, z);
    g
}

/// [`CreaBimApp::rest_3d`] on the rested points alone, for callers that
/// hold another part of the app.
#[allow(clippy::type_complexity)]
fn rest_points(
    rested: &mut (Vec<DVec3>, Option<(DVec3, f64)>),
    ctx: &egui::Context,
    snap: Option<&crate::snaps::Snap>,
    drawing: bool,
) {
    {
        const DWELL: f64 = 0.4;
        const KEEP: usize = 4;
        if !drawing {
            *rested = (vec![], None);
            return;
        }
        let now = ctx.input(|i| i.time);
        let Some(p) = snap.filter(|s| s.kind.is_point()).map(|s| s.p) else {
            rested.1 = None;
            return;
        };
        match rested.1 {
            Some((q, since)) if q.distance(p) < 1e-9 => {
                let pts = &mut rested.0;
                if now - since > DWELL && !pts.iter().any(|r| r.distance(p) < 1e-9) {
                    pts.push(p);
                    if pts.len() > KEEP {
                        pts.remove(0);
                    }
                }
            }
            _ => {
                rested.1 = Some((p, now));
                ctx.request_repaint_after(std::time::Duration::from_millis(450));
            }
        }
    }
}

impl CreaBimApp {
    /// The Modifiers tab (MU27): each modifier a card, as Blender's: a
    /// header line (fold, its name, shown or not, its menu, ×) and, open,
    /// its settings; Add modifier above them.
    fn modifiers_tab(&mut self, ui: &mut egui::Ui, id: ElementId) {
        use creabim_core::{Modifier, ModifierKind};
        let Some(ElementKind::FreeMesh(before)) = self.doc.model().get(id).map(|e| e.kind.clone())
        else {
            ui.label(egui::RichText::new("This element takes no modifiers yet.").weak());
            return;
        };
        let mut m = before.clone();
        let mut label: &'static str = "";
        // what the modifiers can reach in the model: paths to follow,
        // levels to slice at, in the mesh's own axes
        let model = self.doc.model();
        let lift = |level: Option<ElementId>| {
            level
                .and_then(|l| model.level(l))
                .map_or(0.0, |l| l.elevation)
        };
        let home = m.origin + DVec3::Z * lift(m.level);
        let local = |w: DVec3| m.to_local(w - home);
        let paths: Vec<(String, Vec<DVec3>)> = model
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Sketch(sk) => {
                    let z = DVec3::Z * lift(sk.level);
                    let mut pts: Vec<DVec3> = sk.segments().iter().map(|(a, _)| *a + z).collect();
                    if let Some((_, b)) = sk.segments().last() {
                        pts.push(*b + z);
                    }
                    Some((e.name.clone(), pts.into_iter().map(local).collect()))
                }
                ElementKind::Guide(g) => {
                    let z = DVec3::Z * lift(g.level);
                    Some((e.name.clone(), vec![local(g.a + z), local(g.b + z)]))
                }
                _ => None,
            })
            .filter(|(_, p)| p.len() >= 2)
            .collect();
        let level_heights: Vec<f64> = {
            let mut h: Vec<f64> = model
                .iter()
                .filter_map(|e| match &e.kind {
                    ElementKind::Level(l) => Some(local(DVec3::Z * l.elevation).z),
                    _ => None,
                })
                .collect();
            h.sort_by(f64::total_cmp);
            h
        };
        let (lo, hi) = creabim_core::modgen::bounds(&m.mesh);
        let middle = (lo + hi) / 2.0;
        // + Add modifier: grouped, each with its icon and what it is for
        egui::ComboBox::from_id_salt(("add_modifier", id))
            .icon(crate::props::combo_icon)
            .selected_text("+ Add modifier")
            .width(ui.available_width())
            .height(600.0)
            .show_ui(ui, |ui| {
                ui.set_min_width(180.0);
                let mut group = "";
                for (g, k, hint) in ModifierKind::menu() {
                    if g != group {
                        group = g;
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(g.to_uppercase()).small().weak());
                    }
                    // the icon in the toolbar's quiet grey, what it is
                    // for in a tooltip, so the list stays narrow (MU39)
                    let r = ui
                        .horizontal(|ui| {
                            let muted = crate::theme::Palette::of(ui.ctx()).muted;
                            let (ir, _) = ui
                                .allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                            crate::svgicon::paint(ui.painter(), ir.shrink(2.0), k.icon(), muted);
                            ui.selectable_label(false, k.label())
                        })
                        .inner
                        .on_hover_text(hint);
                    if r.clicked() {
                        let mut k = k;
                        // a sensible start for this mesh: round its middle,
                        // mirrored at its middle, cut at the project's levels
                        match &mut k {
                            ModifierKind::RadialArray { center, .. } => {
                                *center = glam::DVec2::new(hi.x + (hi.x - lo.x), middle.y);
                            }
                            ModifierKind::Mirror { plane, .. } => *plane = hi.x,
                            ModifierKind::Slice { heights, .. } if !level_heights.is_empty() => {
                                *heights = level_heights.clone();
                            }
                            _ => {}
                        }
                        m.modifiers.push(Modifier::new(k));
                        label = "Add modifier";
                    }
                }
            });
        if m.modifiers.is_empty() {
            ui.label(egui::RichText::new("None yet. A modifier changes the shape without touching the mesh: repeat it, panel it, round its corners, slice it into floors. Switch it off or apply it any time.").weak());
        }
        let count = m.modifiers.len();
        let mut action: Option<(usize, i32)> = None;
        let p = crate::theme::Palette::of(ui.ctx());
        // the card worked in (its guides show in the views), and a centre
        // to pick in a plan
        let mut focus = self.mod_focus.filter(|f| f.0 == id).map(|f| f.1);
        let mut pick_centre = None;
        for (i, md) in m.modifiers.iter_mut().enumerate() {
            let key = egui::Id::new(("mod.open", id, i));
            let open = ui.ctx().data(|d| d.get_temp::<bool>(key)).unwrap_or(true);
            let mut touched = false;
            let card = egui::Frame::new()
                .fill(crate::props::box_fill(ui.visuals()))
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    // folded or open, a card is as wide as the panel
                    ui.set_min_width(ui.available_width());
                    // one row that never wraps: the title gives way instead
                    ui.horizontal(|ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        let chev = if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        };
                        if crate::props::icon_button(ui, chev, if open { "Fold" } else { "Open" })
                            .clicked()
                        {
                            ui.ctx().data_mut(|d| d.insert_temp(key, !open));
                        }
                        // its icon is its switch: lit while it shows, grey
                        // when off (one button less in a narrow panel)
                        let (ir, sw) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
                        let ink = if md.enabled {
                            p.accent
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        if sw.hovered() {
                            ui.painter().rect_filled(ir, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
                        }
                        crate::svgicon::paint(ui.painter(), ir.shrink(1.0), md.kind.icon(), ink);
                        if sw
                            .on_hover_text(if md.enabled {
                                "Showing: click to switch it off (the mesh without it)"
                            } else {
                                "Off: click to show it again"
                            })
                            .clicked()
                        {
                            md.enabled = !md.enabled;
                            label = "Switch modifier";
                        }
                        // its name (and, folded, its main settings) in what
                        // the three buttons leave, cut short, never under them
                        let mut job = egui::text::LayoutJob::default();
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        job.append(
                            md.kind.label(),
                            0.0,
                            egui::TextFormat::simple(font.clone(), ui.visuals().strong_text_color()),
                        );
                        if !open {
                            job.append(
                                &format!("  {}", md.kind.summary()),
                                0.0,
                                egui::TextFormat::simple(
                                    egui::FontId::proportional(font.size - 1.5),
                                    ui.visuals().weak_text_color(),
                                ),
                            );
                        }
                        // the title in a box of its own, then ⋯ and ×, one
                        // after the other: nothing drawn over the title
                        let w = (ui.available_width() - 3.0 * (20.0 + ui.spacing().item_spacing.x) - 4.0).max(24.0);
                        let (tr, tresp) = ui.allocate_exact_size(egui::vec2(w, 20.0), egui::Sense::hover());
                        let galley = ui.fonts_mut(|f| {
                            job.wrap = egui::text::TextWrapping::truncate_at_width(w);
                            f.layout_job(job)
                        });
                        ui.painter().galley(
                            egui::pos2(tr.min.x, tr.center().y - galley.size().y / 2.0),
                            galley,
                            ui.visuals().text_color(),
                        );
                        tresp.on_hover_text(md.kind.summary());
                        // what it does, as the ? beside Properties' headings
                        // (MU39: not a block of small text in the card)
                        ui.add(
                            egui::Button::new(egui::RichText::new("?").size(11.0).strong())
                                .small()
                                .corner_radius(8.0)
                                .min_size(egui::vec2(16.0, 16.0)),
                        )
                        .on_hover_ui(|ui| {
                            ui.set_max_width(280.0);
                            ui.label(egui::RichText::new(md.kind.label()).strong());
                            ui.label(modifier_help(&md.kind));
                        });
                        {
                            let more = crate::props::icon_button(ui, "ellipsis", "Apply, duplicate, move");
                            egui::Popup::menu(&more).show(|ui| {
                                if i == 0
                                    && ui
                                        .button("Apply")
                                        .on_hover_text(
                                            "Make it part of the mesh, so you can edit the result",
                                        )
                                        .clicked()
                                {
                                    action = Some((i, 2));
                                    ui.close();
                                }
                                if ui.button("Duplicate").clicked() {
                                    action = Some((i, 3));
                                    ui.close();
                                }
                                if i > 0 && ui.button("Move up").clicked() {
                                    action = Some((i, -1));
                                    ui.close();
                                }
                                if i + 1 < count && ui.button("Move down").clicked() {
                                    action = Some((i, 1));
                                    ui.close();
                                }
                            });
                            if crate::props::icon_button(ui, "x", "Remove").clicked() {
                                action = Some((i, 0));
                            }
                        }
                    });
                    if !open {
                        return;
                    }
                    fn changed(ui: &mut egui::Ui, name: &str, w: egui::DragValue) -> bool {
                        crate::props::row(ui, name, |ui| crate::props::fill(ui, w).changed())
                    }
                    fn len(v: &mut f64) -> egui::DragValue<'_> {
                        crate::units::drag(v)
                    }
                    fn deg(v: &mut f64) -> egui::DragValue<'_> {
                        egui::DragValue::new(v).speed(0.5).suffix("°")
                    }
                    fn factor(v: &mut f64) -> egui::DragValue<'_> {
                        egui::DragValue::new(v).speed(0.01).range(0.0..=10.0).max_decimals(3)
                    }
                    fn two(ui: &mut egui::Ui, a: (&str, &str), b: (&str, &str)) -> Option<usize> {
                        match crate::props::actions2(ui, "", (a.0, a.1, true), (b.0, b.1, true)) {
                            (true, _) => Some(0),
                            (_, true) => Some(1),
                            _ => None,
                        }
                    }
                    let what = md.kind.label();
                    let mut edited = false;
                    match &mut md.kind {
                        ModifierKind::Array {
                            count,
                            offset,
                            relative,
                            fit_length,
                            turn,
                            grow,
                        } => {
                            let mut fit = fit_length.is_some();
                            if crate::props::choice(ui, "Fit", &mut fit, &[(false, "Count", "A number of copies"), (true, "Length", "As many as fit a length")]) {
                                *fit_length = fit.then_some(10.0);
                                edited = true;
                            }
                            match fit_length {
                                Some(l) => edited |= changed(ui, "Length", len(l)),
                                None => edited |= changed(ui, "Count", egui::DragValue::new(count).speed(0.1).range(1..=200)),
                            }
                            if crate::props::choice(ui, "Step", relative, &[(false, "Fixed", "A distance"), (true, "× size", "In multiples of its own size: 1 puts copies end to end")]) {
                                edited = true;
                            }
                            for (n, v) in [("Step X", &mut offset.x), ("Step Y", &mut offset.y), ("Step Z", &mut offset.z)] {
                                edited |= if *relative {
                                    changed(ui, n, egui::DragValue::new(v).speed(0.01).suffix(" ×").max_decimals(3))
                                } else {
                                    changed(ui, n, len(v))
                                };
                            }
                            edited |= changed(ui, "Turn each", deg(turn));
                            edited |= changed(ui, "Grow each", factor(grow));
                        }
                        ModifierKind::RadialArray {
                            count,
                            angle,
                            center,
                            turn_copies,
                        } => {
                            edited |= changed(ui, "Count", egui::DragValue::new(count).speed(0.1).range(1..=360));
                            edited |= changed(ui, "Over", egui::DragValue::new(angle).speed(1.0).range(-360.0..=360.0).suffix("°"));
                            edited |= changed(ui, "Centre X", len(&mut center.x));
                            edited |= changed(ui, "Centre Y", len(&mut center.y));
                            if let Some(w) = two(ui, ("Pick in a plan", "Click the centre in a plan view: a column's middle, a grid crossing"), ("At its middle", "Round its own middle: a ring of copies about where it stands")) {
                                if w == 0 {
                                    pick_centre = Some(i);
                                } else {
                                    *center = middle.truncate();
                                    edited = true;
                                }
                            }
                            if crate::props::switch(ui, turn_copies, "Turn each copy to face the centre").changed() {
                                edited = true;
                            }
                        }
                        ModifierKind::PathArray { path, count, align } => {
                            crate::props::row(ui, "Path", |ui| {
                                egui::ComboBox::from_id_salt(("mod.path", id, i))
                                    .icon(crate::props::combo_icon)
                                    .selected_text(if path.len() < 2 { "Pick a sketch or guide".to_string() } else { format!("{} points", path.len()) })
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for (name, pts) in &paths {
                                            if ui.selectable_label(false, name).clicked() {
                                                *path = pts.clone();
                                                edited = true;
                                            }
                                        }
                                    });
                            });
                            edited |= changed(ui, "Count", egui::DragValue::new(count).speed(0.1).range(1..=500));
                            if crate::props::switch(ui, align, "Turn each copy along the path").changed() {
                                edited = true;
                            }
                        }
                        ModifierKind::Mirror { axis, plane } => {
                            crate::props::row(ui, "Across", |ui| {
                                crate::props::flow(ui, |ui| {
                                    for a in Axis::ALL {
                                        if ui.selectable_value(axis, a, a.label()).changed() {
                                            edited = true;
                                        }
                                    }
                                })
                            });
                            edited |= changed(ui, "Plane at", len(plane));
                            let k = axis.index();
                            if let Some(w) = two(ui, ("At its middle", "The plane through the middle of the mesh"), ("At its edge", "The plane on its far side: a mirrored twin")) {
                                *plane = if w == 0 { middle[k] } else { hi[k] };
                                edited = true;
                            }
                        }
                        ModifierKind::Panelize {
                            width,
                            height,
                            joint,
                            depth,
                        } => {
                            edited |= changed(ui, "Panel width", len(width));
                            edited |= changed(ui, "Panel height", len(height));
                            edited |= changed(ui, "Joint", len(joint));
                            edited |= changed(ui, "Proud by", len(depth));
                        }
                        ModifierKind::Louvres {
                            sides,
                            upright,
                            spacing,
                            depth,
                            thickness,
                            angle,
                        } => {
                            edited |= crate::props::toggles(
                                ui,
                                "On sides",
                                sides,
                                &[
                                    ("N", "North: the mesh's +Y side before it is turned"),
                                    ("E", "East: its +X side"),
                                    ("S", "South: its −Y side"),
                                    ("W", "West: its −X side"),
                                ],
                            );
                            if crate::props::choice(ui, "Run", upright, &[(true, "Fins", "Upright fins"), (false, "Slats", "Level louvres")]) {
                                edited = true;
                            }
                            edited |= changed(ui, "Spacing", len(spacing));
                            edited |= changed(ui, "Depth", len(depth));
                            edited |= changed(ui, "Thickness", len(thickness));
                            edited |= changed(ui, "Angle", deg(angle));
                        }
                        ModifierKind::Bevel {
                            radius,
                            segments,
                            chamfer,
                            corners,
                        } => {
                            if crate::props::choice(ui, "Kind", chamfer, &[(false, "Round", "A fillet: rounded"), (true, "Cut", "A chamfer: cut straight")]) {
                                edited = true;
                            }
                            edited |= changed(ui, if *chamfer { "Setback" } else { "Radius" }, len(radius));
                            if !*chamfer {
                                edited |= changed(ui, "Segments", egui::DragValue::new(segments).speed(0.1).range(1..=32));
                            }
                            if crate::props::choice(ui, "Corners", corners, &[(creabim_core::modifiers::Corners::All, "All", "Every corner"), (creabim_core::modifiers::Corners::Convex, "Out", "Outside corners only"), (creabim_core::modifiers::Corners::Concave, "In", "Inside corners only")]) {
                                edited = true;
                            }
                        }
                        ModifierKind::Shell { thickness, open } => {
                            use creabim_core::modifiers::OpenFaces as O;
                            edited |= changed(ui, "Thickness", len(thickness));
                            if crate::props::choice_icons(ui, "Open", open, &[(O::None, "shell-closed", "Closed", "hollow inside, open nowhere (a shell round a void; cut a hole with a Boolean)"), (O::Top, "shell-top", "Open top", "a tub, a planter, a pool"), (O::Bottom, "shell-bottom", "Open bottom", "a shade, a cover"), (O::Both, "shell-both", "Open top and bottom", "a tube, a chimney")]) {
                                edited = true;
                            }
                        }
                        ModifierKind::Solidify { thickness } => {
                            let mut out = *thickness < 0.0;
                            let mut t = thickness.abs();
                            if changed(ui, "Thickness", len(&mut t)) {
                                edited = true;
                            }
                            if crate::props::choice(ui, "Grows", &mut out, &[(false, "In", "Behind the faces"), (true, "Out", "In front of the faces")]) {
                                edited = true;
                            }
                            *thickness = if out { -t } else { t };
                        }
                        ModifierKind::Slice { heights, gap } => {
                            edited |= changed(ui, "Gap", len(gap));
                            let mut gone = None;
                            for (k, h) in heights.iter_mut().enumerate() {
                                crate::props::row(ui, &format!("Cut {}", k + 1), |ui| {
                                    crate::props::flow(ui, |ui| {
                                        let w = (ui.available_width() - 24.0).max(40.0);
                                        if ui.add_sized([w, 20.0], len(h)).changed() {
                                            edited = true;
                                        }
                                        if crate::props::icon_button(ui, "x", "Take this cut away").clicked() {
                                            gone = Some(k);
                                        }
                                    })
                                });
                            }
                            if let Some(k) = gone {
                                heights.remove(k);
                                edited = true;
                            }
                            if let Some(w) = two(ui, ("+ A cut", "Another cut, 3 m above the last"), ("At the levels", "A cut at each of the project's levels")) {
                                if w == 0 {
                                    let next = heights.last().copied().unwrap_or(lo.z) + 3.0;
                                    heights.push(next);
                                } else {
                                    *heights = level_heights.clone();
                                }
                                edited = true;
                            }
                        }
                        ModifierKind::Taper { x, y } => {
                            edited |= changed(ui, "Top width ×", factor(x));
                            edited |= changed(ui, "Top depth ×", factor(y));
                        }
                        ModifierKind::Twist { angle } => {
                            edited |= changed(ui, "Turn", deg(angle));
                        }
                        ModifierKind::Subdivision { levels, simple } => {
                            edited |= changed(ui, "Levels", egui::DragValue::new(levels).speed(0.05).range(0..=4));
                            if crate::props::choice(ui, "Kind", simple, &[(false, "Smooth", "Rounded, for soft organic forms"), (true, "Split", "Finer faces in the same shape, to edit, panel or twist")]) {
                                edited = true;
                            }
                        }
                    }
                    if edited {
                        touched = true;
                        label = match what {
                            "Array" => "Array settings",
                            "Mirror" => "Mirror settings",
                            _ => "Modifier settings",
                        };
                    }
                });
            if open && (touched || card.response.contains_pointer()) {
                focus = Some(i);
            }
            ui.add_space(3.0);
        }
        self.mod_focus = focus.filter(|f| *f < m.modifiers.len()).map(|f| (id, f));
        if let Some(i) = pick_centre {
            self.mod_focus = Some((id, i));
            self.plan.point_pick = true;
            self.status =
                "Click the centre in a plan. Esc or a right-click leaves it as it is.".into();
        }
        match action {
            Some((i, 0)) => {
                m.modifiers.remove(i);
                label = "Remove modifier";
            }
            Some((i, 1)) => {
                m.modifiers.swap(i, i + 1);
                label = "Reorder modifiers";
            }
            Some((i, -1)) => {
                m.modifiers.swap(i, i - 1);
                label = "Reorder modifiers";
            }
            Some((i, 3)) => {
                let copy = m.modifiers[i].clone();
                m.modifiers.insert(i + 1, copy);
                label = "Duplicate modifier";
            }
            Some((_, _)) => {
                let first = m.modifiers.remove(0);
                m.mesh = creabim_core::modifiers::evaluate(&m.mesh, &[first]).mesh;
                self.face = None;
                label = "Apply modifier";
            }
            None => {}
        }
        if m != before && !label.is_empty() {
            let r = self
                .doc
                .edit_merged(id, label, |e| e.kind = ElementKind::FreeMesh(m.clone()));
            self.report(r, format!("{label}."));
        }
    }

    /// A mass's face tools (MU39): pick a face in a view, then extrude it,
    /// move it, inset it or taper it; or subdivide the whole mesh.
    fn mesh_edit_section(&mut self, ui: &mut egui::Ui, id: ElementId) {
        let Some(ElementKind::FreeMesh(before)) = self.doc.model().get(id).map(|e| e.kind.clone())
        else {
            return;
        };
        let hint = "say:Click a face of the mass in a view to pick it, then: Extrude pushes it out with new sides (E), Move slides it, Inset draws a smaller face inside its edges (for a reveal, a recessed panel, a parapet: inset the roof face, then push it down), Taper shrinks it. The tools change the mesh under its modifiers.";
        if !self.prop_header(ui, "meshedit", "Mesh edit", true, Some(hint)) {
            return;
        }
        let mut m = before.clone();
        let mut label = "";
        let on = self.face.is_some_and(|f| f < m.mesh.faces.len());
        crate::props::row(ui, "Face", |ui| match self.face.filter(|_| on) {
            Some(f) => ui.label(format!("{} ({} corners)", f + 1, m.mesh.faces[f].len())),
            None => ui.label(egui::RichText::new("click one in a view").weak()),
        });
        crate::props::row(ui, "Distance", |ui| {
            crate::props::fill(ui, crate::units::drag(&mut self.extrude_distance))
                .on_hover_text("How far Extrude and Move go")
        });
        let (ex, mv) = crate::props::actions2(
            ui,
            "",
            (
                "Extrude",
                "Push the face out by the distance, with new sides (E)",
                on,
            ),
            ("Move", "Move the face along its normal by the distance", on),
        );
        if let (true, Some(f)) = (ex, self.face) {
            m.mesh.extrude_face(f, self.extrude_distance);
            label = "Extrude face";
        }
        if let (true, Some(f)) = (mv, self.face) {
            let n = m.mesh.face_normal(f) * self.extrude_distance;
            let verts = m.mesh.faces[f].clone();
            m.mesh.translate_vertices(&verts, n);
            label = "Move face";
        }
        crate::props::row(ui, "Inset by", |ui| {
            crate::props::fill(
                ui,
                crate::units::drag(&mut self.inset_by).range(0.001..=100.0),
            )
            .on_hover_text("How far inside its edges Inset draws the new face")
        });
        let (ins, tap) = crate::props::actions2(
            ui,
            "",
            (
                "Inset",
                "A smaller face inside its edges, joined by a ring of faces: then push it in for a reveal or a parapet",
                on,
            ),
            ("Taper", "Shrink the face to 80% about its centre", on),
        );
        if let (true, Some(f)) = (ins, self.face) {
            if m.mesh.inset_face(f, self.inset_by) {
                label = "Inset face";
            } else {
                self.status = format!(
                    "Can't inset that face by {}: it would close up. Try less.",
                    crate::units::len(self.inset_by)
                );
            }
        }
        if let (true, Some(f)) = (tap, self.face) {
            m.mesh.scale_face(f, 0.8);
            label = "Taper face";
        }
        if crate::props::action(
            ui,
            "Mesh",
            "Subdivide",
            "Every face split in four, the shape smoothed (Catmull-Clark)",
            true,
        )
        .clicked()
        {
            m.mesh = m.mesh.subdivide();
            self.face = None;
            label = "Subdivide mesh";
        }
        if !label.is_empty() && m != before {
            let r = self
                .doc
                .edit(id, label, |e| e.kind = ElementKind::FreeMesh(m.clone()));
            self.report(
                r,
                if label == "Inset face" {
                    "Inset the face: it is still picked, so Extrude with a negative distance pushes it in.".to_string()
                } else {
                    format!("{label}.")
                },
            );
        }
    }

    /// The project's notepad (MU28): to-dos and notes, each one row (a tick
    /// or a dot, its first line, its link, then edit and delete tucked at
    /// the right), edited in place and closed by clicking away. Copying and
    /// saving sit in the ⋯ menu; markdown is only how it is written out.
    fn notes_pad(&mut self, ui: &mut egui::Ui) {
        use creabim_core::project::PadItem;
        let mut info = self
            .doc
            .model()
            .project_info()
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        if info.pad.is_empty() && !info.notes.trim().is_empty() {
            info.pad = creabim_core::project::pad_from_markdown(&info.notes);
            info.notes.clear();
        }
        let before = info.clone();
        let edit_key = egui::Id::new("pad.edit");
        let mut editing: Option<usize> = ui.ctx().data(|d| d.get_temp(edit_key)).flatten();
        let mut remove = None;
        let mut follow: Option<ElementId> = None;
        let mut export: Option<&'static str> = None;
        let names: std::collections::HashMap<ElementId, String> = info
            .pad
            .iter()
            .filter_map(|i| i.link)
            .filter_map(|l| self.doc.model().get(l).map(|e| (l, e.name.clone())))
            .collect();
        let target = self.selection.active.or(self.active_view);
        // the ⋯ menu: copy and save, and tidying
        crate::props::flow(ui, |ui| {
            let open = info.pad.iter().filter(|i| i.todo && !i.done).count();
            ui.label(egui::RichText::new(format!("{} items · {open} open", info.pad.len())).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("\u{22ef}", |ui| {
                    if ui.button("Copy all").clicked() {
                        export = Some("copy");
                        ui.close();
                    }
                    if ui.button("Save as text (.txt)\u{2026}").clicked() {
                        export = Some("txt");
                        ui.close();
                    }
                    if ui.button("Save as Markdown (.md)\u{2026}").clicked() {
                        export = Some("md");
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Remove the done to-dos").clicked() {
                        export = Some("clean");
                        ui.close();
                    }
                });
            });
        });
        for todo in [true, false] {
            let idx: Vec<usize> = (0..info.pad.len())
                .filter(|k| info.pad[*k].todo == todo)
                .collect();
            let done = idx.iter().filter(|k| info.pad[**k].done).count();
            let title = if todo {
                format!("To-dos  {done}/{}", idx.len())
            } else {
                format!("Notes  {}", idx.len())
            };
            if !self.prop_header(
                ui,
                if todo { "pad.todos" } else { "pad.notes" },
                &title,
                true,
                None,
            ) {
                continue;
            }
            if idx.is_empty() {
                ui.label(egui::RichText::new("None yet.").weak());
            }
            for k in idx {
                let item = &mut info.pad[k];
                if editing == Some(k) {
                    // edited in place: its whole text, then two small
                    // switches; clicking away closes it
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut item.text)
                            .id(egui::Id::new(("pad.text", k)))
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    // focused as it opens, so typing goes straight in
                    let fk = egui::Id::new("pad.focus");
                    if ui.ctx().data(|d| d.get_temp::<usize>(fk)) == Some(k) {
                        r.request_focus();
                        ui.ctx().data_mut(|d| d.remove::<usize>(fk));
                    }
                    if !r.has_focus()
                        && !r.gained_focus()
                        && ui.input(|i| i.pointer.any_pressed())
                        && !r.hovered()
                    {
                        editing = None;
                    }
                    crate::props::flow(ui, |ui| {
                        ui.checkbox(&mut item.todo, "To-do");
                        match item.link {
                            Some(_) => {
                                if ui.small_button("Unlink").clicked() {
                                    item.link = None;
                                }
                            }
                            None => {
                                if ui
                                    .add_enabled(target.is_some(), egui::Button::new("Link to the selection").small())
                                    .on_hover_text("Ties it to the element selected, or else the view in front")
                                    .clicked()
                                {
                                    item.link = target;
                                }
                            }
                        }
                    });
                    continue;
                }
                let row = crate::props::flow(ui, |ui| {
                    if todo {
                        ui.checkbox(&mut item.done, "");
                    } else {
                        ui.label(egui::RichText::new("\u{2022}").weak());
                    }
                    // tools first, from the right, so they keep their place
                    let mut title_rect = ui.available_rect_before_wrap();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::props::icon_button(ui, "trash-2", "Delete").clicked() {
                            remove = Some(k);
                        }
                        if crate::props::icon_button(ui, "pencil", "Edit").clicked() {
                            editing = Some(k);
                            ui.ctx()
                                .data_mut(|d| d.insert_temp(egui::Id::new("pad.focus"), k));
                        }
                        if let Some(l) = item.link {
                            let name = names
                                .get(&l)
                                .cloned()
                                .unwrap_or_else(|| "something deleted".into());
                            if crate::props::icon_button(ui, "link", &format!("Go to {name}"))
                                .clicked()
                            {
                                follow = Some(l);
                            }
                        }
                        title_rect.max.x = ui.min_rect().left() - 4.0;
                    });
                    title_rect
                });
                let r = row.inner;
                let r = egui::Rect::from_min_max(
                    egui::pos2(r.left(), row.response.rect.top()),
                    egui::pos2(r.right().max(r.left() + 20.0), row.response.rect.bottom()),
                );
                let resp = ui.interact(r, egui::Id::new(("pad.row", k)), egui::Sense::click());
                let text = if item.title().is_empty() {
                    "(empty)"
                } else {
                    item.title()
                };
                let color = if item.done {
                    ui.visuals().weak_text_color()
                } else {
                    ui.visuals().text_color()
                };
                let galley = ui.painter().layout_no_wrap(
                    text.to_string(),
                    egui::FontId::proportional(12.0),
                    color,
                );
                let painter = ui.painter().with_clip_rect(r);
                let wide = galley.size().x > r.width();
                let y = r.center().y - galley.size().y / 2.0;
                painter.galley(egui::pos2(r.left(), y), galley, color);
                if item.done {
                    painter.line_segment(
                        [
                            egui::pos2(r.left(), r.center().y),
                            egui::pos2(r.right(), r.center().y),
                        ],
                        egui::Stroke::new(1.0, color.gamma_multiply(0.6)),
                    );
                }
                if wide {
                    let bg = ui.visuals().panel_fill;
                    for s in 0..14 {
                        let x = r.right() - 14.0 + s as f32;
                        painter.line_segment(
                            [egui::pos2(x, r.top()), egui::pos2(x, r.bottom())],
                            egui::Stroke::new(1.0, bg.gamma_multiply(s as f32 / 14.0)),
                        );
                    }
                }
                if resp.on_hover_text(item.text.as_str()).double_clicked() {
                    editing = Some(k);
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("pad.focus"), k));
                }
            }
        }
        // a new item, as a to-do or a note
        let key = egui::Id::new("notes.new");
        let kind_key = egui::Id::new("notes.new.todo");
        let mut new: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
        let mut as_todo: bool = ui.ctx().data(|d| d.get_temp(kind_key)).unwrap_or(true);
        ui.add_space(4.0);
        crate::props::flow(ui, |ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut new)
                    .vertical_align(egui::Align::Center)
                    .hint_text(if as_todo {
                        "New to-do, Enter"
                    } else {
                        "New note, Enter"
                    })
                    .desired_width((ui.available_width() - 64.0).max(60.0)),
            );
            if ui
                .selectable_label(as_todo, "To-do")
                .on_hover_text("Add it as a to-do (else a note)")
                .clicked()
            {
                as_todo = !as_todo;
            }
            if r.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !new.trim().is_empty()
            {
                info.pad.push(PadItem {
                    text: new.trim().to_string(),
                    todo: as_todo,
                    ..Default::default()
                });
                new.clear();
                r.request_focus();
            }
        });
        ui.ctx().data_mut(|d| {
            d.insert_temp(key, new);
            d.insert_temp(kind_key, as_todo);
        });
        if let Some(k) = remove {
            info.pad.remove(k);
            editing = None;
        }
        ui.ctx().data_mut(|d| d.insert_temp(edit_key, editing));
        if let Some(what) = export {
            let model = self.doc.model();
            let all: Vec<&PadItem> = info.pad.iter().collect();
            let md = creabim_core::project::pad_to_markdown(&all, &|id| {
                model.get(id).map(|e| e.name.clone())
            });
            match what {
                "copy" => {
                    ui.ctx().copy_text(md);
                    self.status = "Notes copied.".into();
                }
                "clean" => info.pad.retain(|i| !(i.todo && i.done)),
                ext => {
                    let stem = std::path::Path::new(&self.path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("project")
                        .to_string();
                    if let Some(path) = self.export_to(format!("{stem} notes.{ext}")) {
                        self.status = match std::fs::write(&path, &md) {
                            Ok(()) => format!("Saved the notes to {path}."),
                            Err(e) => format!("Couldn't save: {e}"),
                        };
                    }
                }
            }
        }
        if info != before {
            let tx = creabim_core::set_project(self.doc.model(), info);
            if let Err(e) = self.doc.commit_merged(tx) {
                self.status = format!("Couldn't do that: {e}");
            }
        }
        if let Some(l) = follow {
            match self.doc.model().get(l).map(|e| &e.kind) {
                Some(ElementKind::View(_)) => self.active_view = Some(l),
                Some(_) => {
                    self.selection.only(l);
                    self.prop_tab = PropTab::Element;
                }
                None => self.status = "What it was linked to is gone.".into(),
            }
        }
    }

    /// After an edit, say so if a selected wall now overlaps another along
    /// its length (MU27): it looks merged in plan but stays two in 3D.
    fn warn_overlaps(&mut self) {
        let rev = self.doc.model().revision();
        if self.overlap_seen == rev {
            return;
        }
        self.overlap_seen = rev;
        let picked = self.selection.all();
        if picked.is_empty() {
            return;
        }
        let pairs = creabim_core::checks::overlapping_walls(self.doc.model());
        if let Some((a, b)) = pairs
            .iter()
            .find(|(a, b)| picked.contains(a) || picked.contains(b))
        {
            let name = |id: &ElementId| {
                self.doc
                    .model()
                    .get(*id)
                    .map_or("a wall".into(), |e| e.name.clone())
            };
            self.status = format!(
                "{} now overlaps {} along its length: one wall in plan, two in 3D. Manage \u{203a} Overlapping walls merges them.",
                name(a),
                name(b)
            );
        }
    }

    fn note_editor_window(&mut self, ctx: &egui::Context) {
        self.warn_overlaps();
        if let Some((from, g)) = self.copy_guides.take() {
            let others: Vec<ElementId> = self
                .doc
                .model()
                .iter()
                .filter(|e| e.id != from && matches!(e.kind, ElementKind::Sheet(_)))
                .map(|e| e.id)
                .collect();
            let mut tx = creabim_core::Transaction::new("Copy guides to all sheets");
            for sid in &others {
                if let Some(e) = self.doc.model().get(*sid).cloned() {
                    let mut after = e.clone();
                    if let ElementKind::Sheet(sh) = &mut after.kind {
                        sh.guides = g.clone();
                    }
                    tx = tx.replace(e, after);
                }
            }
            let r = self.doc.commit(tx);
            self.report(
                r,
                format!("Guides copied to {} other sheets.", others.len()),
            );
        }
        if let Some(id) = self.plan.edit_note.take() {
            self.note_editor = Some(id);
        }
        let Some(id) = self.note_editor else { return };
        let Some(ElementKind::Note(n)) = self.doc.model().get(id).map(|e| e.kind.clone()) else {
            self.note_editor = None;
            return;
        };
        let mut n = n;
        let mut open = true;
        let mut changed = false;
        crate::app::shell::window("Note text")
            .id(egui::Id::new(("note-editor", id)))
            .open(&mut open)
            .default_size([520.0, 420.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                crate::props::flow(ui, |ui| {
                    changed |= ui.checkbox(&mut n.markdown, "Markdown").changed();
                    ui.label(
                        egui::RichText::new("# heading  - bullet  1. numbered  - [ ] to do").weak(),
                    );
                });
                egui::ScrollArea::vertical().show(ui, |ui| {
                    changed |= ui
                        .add(
                            egui::TextEdit::multiline(&mut n.text)
                                .desired_width(f32::INFINITY)
                                .desired_rows(18)
                                .font(egui::TextStyle::Monospace),
                        )
                        .changed();
                });
            });
        if changed {
            let r = self
                .doc
                .edit_merged(id, "Note text", |e| e.kind = ElementKind::Note(n.clone()));
            if let Err(e) = r {
                self.status = format!("Couldn't do that: {e}");
            }
        }
        if !open {
            self.note_editor = None;
        }
    }

    /// Drawing a slab, or redrawing a slab or wall, in the 3D view, on its
    /// level, with snaps.
    fn outline_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
    ) {
        let Some(outline) = self.plan.outline.clone() else {
            return;
        };
        let here = response.hovered();
        let typing = ui.ctx().egui_wants_keyboard_input();
        let (enter, escape, shift, backspace) = ui.input(|i| {
            (
                here && (i.key_pressed(egui::Key::Enter)
                    || !typing && !i.modifiers.any() && i.key_pressed(egui::Key::C)),
                here && i.key_pressed(egui::Key::Escape),
                i.modifiers.shift,
                here && !typing && i.key_pressed(egui::Key::Backspace),
            )
        });
        let exclude: Vec<ElementId> = outline.target.into_iter().collect();
        self.refresh_snaps_3d(&exclude);
        let snap_cfg = self.plan.snap.live(ui.input(|i| i.modifiers.alt));
        let hover = response.hover_pos().and_then(|h| {
            let (o, d) = ray(h);
            let free = Workplane::ground(outline.z).intersect(o, d);
            let from = outline.points.last().map(|p| p.extend(outline.z));
            let snapper = &self.snaps_3d.as_ref()?.2;
            let shape: Vec<DVec3> = outline.points.iter().map(|p| p.extend(outline.z)).collect();
            let guides = guides_3d(
                &self.rested_3d.0,
                from,
                outline.z,
                self.plan.guide_step,
                &snap_cfg,
            );
            let rested: Vec<DVec3> = if guides.is_empty() {
                self.rested_3d
                    .0
                    .iter()
                    .map(|p| p.truncate().extend(outline.z))
                    .collect()
            } else {
                vec![]
            };
            let track = snaps::track_points(&shape, &rested);
            let hit = snapper.snap_guided(&snap_cfg, h, project, from, &track, &guides, free)?;
            snaps::draw_guides(painter, project, &guides, &hit.guides);
            let (s, align) = (hit.snap, hit.align);
            if let Some(c) = project(s.p.truncate().extend(outline.z)) {
                if !hit.guides.is_empty() {
                    snaps::draw_guided(painter, project, &hit, &guides, from.and_then(project));
                } else if s.kind != SnapKind::Grid {
                    snaps::draw_tracked(painter, project, s, align, c, from.and_then(project));
                }
            }
            Some((h, s.p.truncate(), s))
        });
        outline.draw(painter, project, hover.map(|h| h.1));
        // resting on a point lines the next corner up with it (MU24)
        self.rest_3d(ui.ctx(), hover.as_ref().map(|h| &h.2), true);
        let hover = hover.map(|(h, p, _)| (h, p));
        let click = match (response.clicked(), hover) {
            (true, Some((h, p))) => Some((p, outline.near_first(h, project))),
            _ => None,
        };
        let right = response.secondary_clicked() && !shift;
        let arc = self.plan.slab_arc;
        let enter = enter || std::mem::take(&mut self.plan.close_outline);
        let Some(o) = self.plan.outline.as_mut() else {
            return;
        };
        match o.input_more(
            enter,
            escape,
            right,
            click,
            arc,
            response.double_clicked(),
            backspace,
        ) {
            crate::outline::Next::Drawing(hint) => self.status = hint,
            crate::outline::Next::Cancel(msg) => {
                self.plan.outline = None;
                self.status = msg.into();
            }
            crate::outline::Next::Finish(closed) => {
                if let Some(o) = self.plan.outline.take() {
                    match o.finish(&mut self.doc, closed) {
                        Ok((msg, made)) => {
                            if let Some(id) = made {
                                self.selection.only(id);
                                self.face = None;
                            }
                            self.status = msg;
                        }
                        Err(e) => self.status = format!("Couldn't draw that: {e}"),
                    }
                }
            }
        }
    }

    /// The guide within a few pixels of `p` in the 3D view.
    fn guide_near(
        &self,
        p: egui::Pos2,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) -> Option<ElementId> {
        let mut best: Option<(ElementId, f32)> = None;
        for e in self.doc.model().iter() {
            let ElementKind::Guide(g) = &e.kind else {
                continue;
            };
            let Some(dir) = (g.b - g.a).try_normalize() else {
                continue;
            };
            let m = (g.a + g.b) / 2.0;
            let steps = 240;
            let pts: Vec<Option<egui::Pos2>> = (0..=steps)
                .map(|i| {
                    let t = -GUIDE_REACH_3D + 2.0 * GUIDE_REACH_3D * i as f64 / steps as f64;
                    project(m + dir * t)
                })
                .collect();
            for w in pts.windows(2) {
                let (Some(a), Some(b)) = (w[0], w[1]) else {
                    continue;
                };
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let d = (a + ab * t).distance(p);
                if d < 5.0 && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((e.id, d));
                }
            }
        }
        best.map(|(id, _)| id)
    }

    /// Every sketch as lines over the 3D view, and every guide dashed; the
    /// selected one stands out.
    fn draw_sketches(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    ) {
        let left_out = self.hidden_in_3d();
        for e in self.doc.model().iter() {
            let ElementKind::Guide(g) = &e.kind else {
                continue;
            };
            if left_out.contains(&e.id) {
                continue;
            }
            let stroke = if self.selection.contains(e.id) {
                egui::Stroke::new(2.0, Color32::from_rgb(242, 140, 51))
            } else {
                egui::Stroke::new(1.0, planview::GUIDE_COLOR)
            };
            draw_guide_3d(painter, project, g.a, g.b, stroke);
        }
        // a closed sketch is a face: a light fill inside its outline (A7-1)
        for e in self.doc.model().iter() {
            let ElementKind::Sketch(sk) = &e.kind else {
                continue;
            };
            if !sk.closed || left_out.contains(&e.id) {
                continue;
            }
            let outline = sk.outline();
            if outline.len() < 3 || creabim_core::polygon::self_intersects(&outline, true) {
                continue;
            }
            let z = sk
                .level
                .and_then(|l| self.doc.model().level(l))
                .map_or(0.0, |l| l.elevation);
            let pts: Option<Vec<egui::Pos2>> = outline
                .iter()
                .map(|p| project(sk.plane.to_model(*p) + DVec3::Z * z))
                .collect();
            let Some(pts) = pts else {
                continue;
            };
            let fill = if self.selection.contains(e.id) {
                Color32::from_rgba_unmultiplied(242, 140, 51, 60)
            } else {
                Color32::from_rgba_unmultiplied(40, 70, 140, 40)
            };
            let mut mesh = egui::Mesh::default();
            for p in &pts {
                mesh.colored_vertex(*p, fill);
            }
            for t in creabim_core::polygon::triangulate(&outline) {
                mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        for (id, a, b) in creabim_geom::scene::sketch_lines(self.doc.model()) {
            if left_out.contains(&id) {
                continue;
            }
            let selected = self.selection.contains(id);
            let stroke = if selected {
                egui::Stroke::new(2.5, Color32::from_rgb(242, 140, 51))
            } else {
                egui::Stroke::new(1.5, Color32::from_rgb(40, 70, 140))
            };
            if let (Some(a), Some(b)) = (project(a), project(b)) {
                painter.line_segment([a, b], stroke);
            }
        }
        crate::sketch::draw_marks(
            painter,
            self.doc.model(),
            self.selection.active,
            project,
            &|_| true,
        );
    }

    /// The Sketch tool in the 3D view: the first click sets the workplane on
    /// the face under the pointer (or the ground), later clicks add corners.
    fn sketch_3d(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
        ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
        cam: &OrbitCamera,
    ) {
        // a sketch being drawn in a 2D pane is left alone until clicking here
        let mine = self.plan.sketch.as_ref().is_some_and(|d| d.pane.is_none());
        if !mine && response.clicked() {
            self.plan.sketch = None;
        }
        // the Sketch tool's own Line / Arc / Circle; an outline tool has its
        // bar's (MU31: both set the shape each frame, so an arc's end was
        // dropped as soon as it was clicked)
        if self.plan.tool == Tool::Sketch {
            let draft = self.plan.sketch.as_mut().filter(|d| d.pane.is_none());
            if let Some(hint) = crate::sketch::shape_controls(ui, response.rect, None, draft) {
                self.status = hint;
            }
        }
        // a slab, roof or other outline tool drawn on a face: its own bar's
        // Line / Arc choice, not the Sketch tool's (MU27: Arc did nothing)
        if self.plan.tool != Tool::Sketch {
            if response.hovered() && !ui.ctx().egui_wants_keyboard_input() {
                let (a, l) = ui.input(|i| {
                    let plain = !i.modifiers.command && !i.modifiers.alt;
                    (
                        plain && i.key_pressed(egui::Key::A),
                        plain && i.key_pressed(egui::Key::L),
                    )
                });
                if a || l {
                    self.plan.slab_arc = a;
                }
            }
            let want = if self.plan.slab_arc {
                crate::sketch::Shape::Arc
            } else {
                crate::sketch::Shape::Line
            };
            if let Some(d) = self.plan.sketch.as_mut() {
                if d.shape != want {
                    d.arc_end = None;
                    d.shape = want;
                }
            }
        }
        let (enter, escape, shift, alt) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.modifiers.shift,
                i.modifiers.alt,
            )
        });
        let hover = response.hover_pos();
        // corners, edges and centres of what is modelled; Shift keeps lines square instead
        self.refresh_snaps_3d(&[]);
        let snap_cfg = self.plan.snap.live(alt || shift);
        let last = self
            .plan
            .sketch
            .as_ref()
            .filter(|_| mine)
            .and_then(|d| Some(d.plane.to_model(*d.points.last()?)));
        let shape: Vec<DVec3> = self
            .plan
            .sketch
            .as_ref()
            .filter(|_| mine)
            .map(|d| d.points.iter().map(|p| d.plane.to_model(*p)).collect())
            .unwrap_or_default();
        let track = snaps::track_points(&shape, &self.rested_3d.0);
        let plane = self.plan.sketch.as_ref().filter(|_| mine).map(|d| d.plane);
        let tracked = hover.and_then(|h| {
            let snapper = &self.snaps_3d.as_ref()?.2;
            // on the sketch's plane, the pointer's point lines up with its corners
            let (o, d) = ray(h);
            let free = plane.and_then(|p| p.intersect(o, d));
            let (s, align) = snapper.snap_tracked(&snap_cfg, h, project, last, &track, free)?;
            // only real snaps here: a free point is placed on the plane below
            (s.kind != SnapKind::Grid).then_some((s, align))
        });
        // only snaps on the plane drawn on: a corner behind the face, seen
        // through it, would land somewhere else once laid on the plane
        let near_plane = |q: DVec3| -> bool {
            let pl = plane.or_else(|| {
                let (o, d) = ray(hover?);
                face_plane(&self.scene, o, d)
            });
            pl.is_none_or(|pl| (q - pl.origin).dot(pl.normal()).abs() < 0.02)
        };
        let tracked = tracked.filter(|(s, _)| near_plane(s.p));
        let snapped = tracked.map(|t| t.0);
        // resting on a point lines the next one up with it (MU24)
        let drawing = self.plan.sketch.is_some() || self.plan.tool != Tool::Select;
        self.rest_3d(ui.ctx(), snapped.as_ref(), drawing);
        if let Some((s, align)) = tracked {
            if let Some(c) = project(s.p) {
                snaps::draw_tracked(painter, project, s, align, c, last.and_then(project));
            }
        }
        // the plane a first click would start on; off every face, a sketch
        // starts on the ground and the other tools on the working level
        let ground = match self.plan.tool {
            Tool::Sketch => 0.0,
            _ => self.work_level().map_or(0.0, |(_, z)| z),
        };
        let (scene, custom) = (&self.scene, self.workplane.as_ref().map(|w| w.0));
        let candidate = |p: egui::Pos2| -> Option<Workplane> {
            let (o, d) = ray(p);
            if let Some(plane) = custom {
                // the first corner is where the pointer meets it
                let at = plane.intersect(o, d)?;
                return Some(Workplane {
                    origin: at,
                    ..plane
                });
            }
            face_plane(scene, o, d).or_else(|| {
                let at = Workplane::ground(ground).intersect(o, d)?;
                Some(Workplane {
                    origin: at,
                    ..Workplane::ground(ground)
                })
            })
        };
        let on_plane = match (&self.plan.sketch, hover) {
            (Some(d), Some(_)) if mine && snapped.is_some() => {
                snapped.map(|s| d.snap(d.plane.to_plane(s.p), 1e-6, false, false))
            }
            (Some(d), Some(p)) if mine => {
                let (o, dir) = ray(p);
                d.plane.intersect(o, dir).map(|m| {
                    // 10 px at that distance, for closing on the first corner
                    let dist = m.distance(cam.eye().as_dvec3());
                    let tol = dist * 2.0 * (cam.fov_y as f64 / 2.0).tan() * 10.0
                        / response.rect.height() as f64;
                    d.snap(
                        d.plane.to_plane(m),
                        tol,
                        shift,
                        self.plan.snap.live(alt).allows(SnapKind::Grid),
                    )
                })
            }
            _ => None,
        };
        let accent = crate::theme::interaction();
        // a slab, roof or shaft's Rectangle too, not only a face's (MU31)
        let rect_preview = (crate::partstool::on_face(&self.plan).is_some()
            || self.plan.tool.draws_outline())
            && self.plan.slab_mode == crate::slabtool::SlabMode::Rectangle;
        match self.plan.sketch.as_ref().filter(|_| mine) {
            // a rectangle on a face shows whole as the corner follows (MU24)
            Some(d) if rect_preview && d.points.len() == 1 && on_plane.is_some() => {
                let (a, b) = (d.points[0], on_plane.unwrap_or_default());
                let corners = [a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y), a];
                let pts: Vec<egui::Pos2> = corners
                    .iter()
                    .filter_map(|c| project(d.plane.to_model(*c)))
                    .collect();
                painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0, accent)));
            }
            Some(d) => d.draw(painter, project, on_plane),
            None => {
                if let Some(plane) = hover.and_then(candidate) {
                    crate::sketch::draw_plane(painter, project, &plane, accent);
                }
            }
        }
        let finish = |app: &mut Self, closed: bool| {
            let Some(d) = app.plan.sketch.take() else {
                return;
            };
            // a level slab, or a bare hole in the wall the outline is on
            if matches!(app.plan.tool, Tool::Slab | Tool::Opening) {
                if !closed && app.plan.tool == Tool::Slab {
                    app.plan.sketch = Some(d);
                    app.status = "Close the outline on its first corner to make it.".into();
                    return;
                }
                let r = if app.plan.tool == Tool::Slab {
                    d.finish_slab(&mut app.doc, &app.plan)
                } else {
                    d.finish_opening(&mut app.doc)
                };
                match r {
                    Ok(Some(id)) => {
                        app.selection.only(id);
                        app.face = None;
                        app.status = if app.plan.tool == Tool::Slab {
                            "Slab added with its top on the face. Ctrl+Z to undo."
                        } else {
                            "Opening cut in the wall, the rectangle around what you drew. Set its size and sill in Properties. Ctrl+Z to undo."
                        }
                        .into();
                    }
                    Ok(None) => app.status = "Draw at least two corners.".into(),
                    Err(m) => app.status = m,
                }
                return;
            }
            // a sweep along a path drawn on a face
            if let Some((profile, name)) = crate::partstool::face_sweep(&app.plan, app.doc.model())
            {
                let name = name.as_str();
                match d.finish_sweep(&mut app.doc, profile, name, closed) {
                    Ok(Some(id)) => {
                        app.selection.only(id);
                        app.face = None;
                        app.status = format!("{name} added along the path on the face: its profile stands out of the face. Ctrl+Z to undo.");
                    }
                    Ok(None) => app.status = "A path needs two points.".into(),
                    Err(e) => app.status = format!("Couldn't do that: {e}"),
                }
                return;
            }
            // an extrusion or a void on the face the outline was drawn on
            if let Some((base, height, void)) = crate::partstool::on_face(&app.plan) {
                if !closed {
                    app.plan.sketch = Some(d);
                    app.status = "Close the outline on its first corner to make it.".into();
                    return;
                }
                match d.finish_part(&mut app.doc, base, height, void) {
                    Ok(Some(id)) => {
                        app.selection.only(id);
                        app.face = None;
                        app.status = if void {
                            "Void added on the face: it cuts square into it. Set its depth in Properties. Ctrl+Z to undo."
                        } else {
                            "Extrusion added on the face, standing square out of it. Set its depth in Properties. Ctrl+Z to undo."
                        }
                        .into();
                    }
                    Ok(None) => app.status = "An outline needs three corners.".into(),
                    Err(e) => app.status = format!("Couldn't do that: {e}"),
                }
                return;
            }
            match d.finish(&mut app.doc, closed) {
                Ok(Some(id)) => {
                    app.selection.only(id);
                    app.face = None;
                    app.status =
                        "Sketch added. Press E to extrude it into a mass, or G to move it.".into();
                }
                Ok(None) => {}
                Err(e) => app.status = format!("Couldn't do that: {e}"),
            }
        };
        if !mine && !response.clicked() && !response.secondary_clicked() {
            return;
        }
        if escape {
            self.plan.sketch = None;
            return;
        }
        // a right-click on a shape just started cancels it, as everywhere
        // (MU27: a slab on a face kept going); the Sketch tool's open
        // lines still end on it once they have two points
        if response.secondary_clicked() && self.plan.sketch.is_some() && !on_face_now(&self.plan) {
            let n = self.plan.sketch.as_ref().map_or(0, |d| d.points.len());
            if self.plan.tool == Tool::Sketch && n >= 2 {
                finish(self, false);
            } else if n > 1 {
                // as in plans: takes back the last point
                if let Some(d) = self.plan.sketch.as_mut() {
                    d.points.pop();
                    d.kinds.truncate(d.points.len().saturating_sub(1));
                    d.arc_end = None;
                }
                self.status = format!("Took back a point ({} left). Right-click again for more; with none left it cancels.", n - 1);
            } else {
                self.plan.sketch = None;
                self.status = "Cancelled. Right-click again to leave the tool.".into();
            }
            return;
        }
        // an extrusion's or a void's outline on a face (MU23): a
        // right-click with under three corners cancels it, with three or
        // more closes and makes it; again with nothing drawn, it leaves
        // the tool, as everywhere
        let on_face = crate::partstool::on_face(&self.plan).is_some();
        if response.secondary_clicked()
            && self.plan.sketch.is_none()
            && self.plan.tool != Tool::Select
        {
            self.plan.step_back();
            if let Some(s) = self.plan.status.take() {
                self.status = s;
            }
            return;
        }
        if on_face && response.secondary_clicked() {
            let corners = self.plan.sketch.as_ref().map_or(0, |d| d.points.len());
            if corners < 3 {
                self.plan.sketch = None;
                self.status = "Cancelled. Right-click again to leave the tool.".into();
            } else {
                finish(self, true);
            }
            return;
        }
        if enter || (response.secondary_clicked() && self.plan.sketch.is_some()) {
            finish(self, false);
            return;
        }
        if !response.clicked() {
            return;
        }
        let rect_on_face = (crate::partstool::on_face(&self.plan).is_some()
            || self.plan.tool.draws_outline())
            && self.plan.slab_mode == crate::slabtool::SlabMode::Rectangle;
        match &mut self.plan.sketch {
            None => {
                if let Some(mut plane) = hover.and_then(candidate) {
                    // start on the snapped point, kept on the face's plane
                    if let Some(s) = snapped {
                        plane.origin = plane.to_model(plane.to_plane(s.p));
                    }
                    let mut draft = SketchDraft::new(plane, None);
                    // an outline tool's own Line / Arc from the first click
                    // (MU31: the hint and first segment read the Sketch tool's)
                    let shape = match (self.plan.tool, self.plan.slab_arc) {
                        (Tool::Sketch, _) => crate::sketch::shape(ui.ctx()),
                        (_, true) => crate::sketch::Shape::Arc,
                        (_, false) => crate::sketch::Shape::Line,
                    };
                    draft.shape = shape;
                    // what each outline mode does on a face (MU24)
                    let on_face = crate::partstool::on_face(&self.plan).is_some();
                    let outline = on_face || self.plan.tool.draws_outline();
                    self.status = match (outline, self.plan.slab_mode) {
                        (true, crate::slabtool::SlabMode::Rectangle) => {
                            "Workplane set. Click the opposite corner of the rectangle.".into()
                        }
                        (true, crate::slabtool::SlabMode::FillRoom | crate::slabtool::SlabMode::FromSketch) => format!(
                            "Workplane set. Fill room and From sketch work on a level; on a face, click the corners. {}",
                            crate::sketch::hint(shape, Some(&draft))
                        ),
                        _ => format!("Workplane set. {}", crate::sketch::hint(shape, Some(&draft))),
                    };
                    self.plan.sketch = Some(draft);
                }
            }
            Some(d) => {
                // Rectangle (2) of an extrusion or a void: the second click
                // is the opposite corner (MU23)
                if let (Some(p), true, 1) = (on_plane, rect_on_face, d.points.len()) {
                    let a = d.points[0];
                    if (p.x - a.x).abs() > 1e-6 && (p.y - a.y).abs() > 1e-6 {
                        d.points = vec![a, DVec2::new(p.x, a.y), p, DVec2::new(a.x, p.y)];
                        d.kinds = vec![creabim_core::sketch::SegmentKind::Line; 4];
                        finish(self, true);
                    }
                    return;
                }
                if let Some(p) = on_plane {
                    match d.click(p) {
                        Some(Click::Closed) => finish(self, true),
                        Some(Click::Added) => self.status = crate::sketch::hint(d.shape, Some(d)),
                        None => {}
                    }
                }
            }
        }
    }

    fn show_plan(&mut self, ui: &mut egui::Ui, rect: Rect, background: Color32) {
        let _perf = Perf::new("show_plan");
        let Some(view) = self.active_view else { return };
        self.rect_2d = Some(rect);
        let (before, revision) = (self.selection.clone(), self.doc.model().revision());
        let schedule = match self.view(view).map(|v| &v.kind) {
            Some(ViewKind::Schedule { category, by_type }) => Some((*category, *by_type)),
            _ => None,
        };
        let result = if let Some((category, by_type)) = schedule {
            let csv = with_extension(
                &self.path,
                &format!("{}.csv", category.label().to_lowercase()),
            );
            // on dark paper too, as sheets are (MU29)
            let dark = self.dark_paper(ui.ctx()) && !self.print_preview;
            let layer = ui.layer_id();
            let from = ui.ctx().graphics(|g| g.get(layer).map(|l| l.next_idx()));
            let export = self.schedule_export.take();
            let msg = crate::schedulepane::show(
                ui,
                rect,
                &mut self.doc,
                view,
                (category, by_type),
                &mut self.selection,
                (&csv, &with_extension(&self.path, "xlsx")),
                export.as_deref(),
            );
            if dark {
                darken_shapes(ui.ctx(), layer, from);
            }
            if let Some(m) = msg {
                self.status = m;
            }
            None
        } else if self.is_sheet(view) {
            let views = self.placed_views(view);
            let info = self.sheet_info();
            // a sheet follows dark paper while you work on it; Print
            // preview shows it as it prints, on white (MU21)
            let dark = self.dark_paper(ui.ctx()) && !self.print_preview;
            let layer = ui.layer_id();
            let from = ui.ctx().graphics(|g| g.get(layer).map(|l| l.next_idx()));
            let r = sheetview::show(
                ui,
                rect,
                &mut self.doc,
                view,
                &views,
                &info,
                self.sheets.entry(self.area_now).or_default(),
                &mut self.plan,
                &mut self.selection,
            );
            if dark {
                darken_shapes(ui.ctx(), layer, from);
            }
            r
        } else {
            let Some(kind) = self.view(view).map(planview::pane_kind) else {
                return;
            };
            let _pp = Perf::new("drawing_live");
            let Some(mut drawing) = self.drawing_live(view) else {
                return;
            };
            drop(_pp);
            let _pr = Perf::new("show_plan: hidden and guides");
            if !self.hidden.is_empty() {
                drawing.items.retain(|it| !self.hidden.contains(&it.id()));
            }
            if self.shell.settings.guides_hidden {
                // kept in the model, left out of every view (and so not snapped to)
                drawing.items.retain(|it| {
                    !matches!(
                        it,
                        PlanItem::Line {
                            pen: Pen::Guide,
                            ..
                        }
                    )
                });
            }
            drop(_pr);
            let style = PaneStyle {
                background: Some(background),
                decorations: true,
                camera: None,
                scale: self.view(view).map_or(100.0, |v| v.scale),
            };
            // dark paper (A4-3): the pane is drawn as on light paper, then
            // its greys turn over; colours (selection, dimensions, the
            // accent) and pictures keep theirs
            let dark = self.dark_paper(ui.ctx());
            let layer = ui.layer_id();
            let from = ui.ctx().graphics(|g| g.get(layer).map(|l| l.next_idx()));
            let _pp = Perf::new("planview::show");
            let r = planview::show(
                ui,
                rect,
                &mut self.doc,
                view,
                kind,
                &drawing,
                &mut self.plan,
                &mut self.selection,
                style,
            );
            drop(_pp);
            if dark {
                let _pd = Perf::new("show_plan: dark paper");
                darken_shapes(ui.ctx(), layer, from);
            }
            r
        };
        let _pt = Perf::new("show_plan: after");
        if let Some(err) = result {
            self.status = format!("Couldn't do that: {err}");
        } else if self.doc.model().revision() != revision {
            if let Some(label) = self.doc.undo_label() {
                self.status = format!("{label}. Ctrl+Z to undo.");
                if label.starts_with("Add Section") || label.starts_with("Add Elevation") {
                    self.status
                        .push_str(" Open it from the View list or the Views group.");
                }
            }
        }
        if self.selection != before {
            self.face = None;
        }
        if self.selection != before || self.doc.model().revision() != revision {
            // other views drew this frame before the edit; draw them again
            ui.ctx().request_repaint();
        }
        let name = self
            .doc
            .model()
            .get(view)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let label = match (self.view(view), sheets::sheet_of(self.doc.model(), view)) {
            (Some(_), _) if schedule.is_some() => format!("{name}  ·  schedule"),
            (Some(v), _) => format!("{name}  ·  1:{:.0}", v.scale),
            (_, Some(s)) => format!(
                "Sheet {} {name}  ·  {}",
                s.number,
                s.size_name().unwrap_or("custom size")
            ),
            _ => name,
        };
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(13.0),
            // light on dark paper, so it reads in every theme (MU29)
            if self.dark_paper(ui.ctx()) && !self.print_preview {
                Color32::from_gray(190)
            } else {
                Color32::from_gray(80)
            },
        );
    }

    // ---- panels ---------------------------------------------------------

    /// Where 3D sketches and guides go: the level, a face, three points,
    /// with an offset along the plane's normal.
    fn workplane_bar(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label("Workplane:").on_hover_text(
            "Where sketches and guides drawn in 3D go. Plans draw on their level and sections on the section plane.",
        );
        let text = match &self.workplane {
            Some((_, name)) => name.clone(),
            None => "Level".into(),
        };
        egui::ComboBox::from_id_salt("workplane")
            .icon(crate::props::combo_icon)
            .selected_text(text)
            .width(ui.available_width().min(150.0))
            .truncate()
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.workplane.is_none(), "Level (follows the plan)")
                    .clicked()
                {
                    self.workplane = None;
                    self.geometry_key = None;
                    self.status = "Workplane: the working level.".into();
                }
                if ui.button("Pick a face…").clicked() {
                    self.plane_pick = Some(PlanePick::Face);
                }
                if ui.button("Through three points…").clicked() {
                    self.plane_pick = Some(PlanePick::Points(vec![]));
                }
                let level = self.work_level();
                if let Some((_, z)) = level {
                    if ui.button("Level plane you can offset").clicked() {
                        self.set_workplane(
                            Workplane::ground(z),
                            format!("level at {}", crate::units::elevation(z)),
                        );
                    }
                }
            });
        if let Some((plane, _)) = self.workplane.as_mut() {
            let mut d = 0.0;
            let r = ui
                .add(crate::units::drag(&mut d).prefix("Offset ± "))
                .on_hover_text("Drag to slide the workplane along its normal");
            if r.changed() && d != 0.0 {
                plane.origin += plane.normal() * d;
                self.geometry_key = None;
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        crate::props::flow(ui, |ui| {
            for t in Tool::ALL {
                if ui
                    .selectable_label(self.plan.tool == t, t.label())
                    .on_hover_text(t.hint())
                    .clicked()
                {
                    // a detail path being drawn is kept, not dropped
                    crate::detailtool::keep(&mut self.doc, &mut self.plan, &mut self.selection);
                    self.plan.tool = t;
                    self.plan.cancel();
                }
            }
            ui.separator();
            ui.label("Pivot:").on_hover_text(
                "What R and S turn and scale about. Shift+right-click places the pivot cursor.",
            );
            egui::ComboBox::from_id_salt("pivot")
                .icon(crate::props::combo_icon)
                .selected_text(self.plan.pivot.label())
                .width(ui.available_width().min(150.0))
                .truncate()
                .show_ui(ui, |ui| {
                    for p in crate::modal::Pivot::ALL {
                        ui.selectable_value(&mut self.plan.pivot, p, p.label());
                    }
                });
            self.workplane_bar(ui);
            snaps::snap_bar(ui, &mut self.plan.snap);
            ui.separator();
            ui.label("View:");
            let current = self
                .active_view
                .and_then(|id| self.doc.model().get(id))
                .map(|e| e.name.clone())
                .unwrap_or_default();
            let views = self.views();
            let sheet_list = self.sheet_list();
            let current = match self
                .active_view
                .and_then(|id| sheets::sheet_of(self.doc.model(), id))
            {
                Some(s) => format!("{} {current}", s.number),
                None => current,
            };
            egui::ComboBox::from_id_salt("view")
                .icon(crate::props::combo_icon)
                .selected_text(current)
                .width(ui.available_width().min(190.0))
                .truncate()
                .show_ui(ui, |ui| {
                    for (id, name, _) in views {
                        ui.selectable_value(&mut self.active_view, Some(id), name);
                    }
                    if !sheet_list.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new("Sheets").weak());
                    }
                    for (id, name) in sheet_list {
                        ui.selectable_value(&mut self.active_view, Some(id), name);
                    }
                });
            if let Some(id) = self.active_view.filter(|id| self.is_sheet(*id)) {
                self.place_view_combo(ui, id);
            }
            if let Some(id) = self.active_view {
                if let Some(mut v) = self.view(id).cloned() {
                    let shadable = matches!(
                        v.kind,
                        ViewKind::Section { .. } | ViewKind::Axonometric { .. }
                    );
                    let (text, value, unit) = match &mut v.kind {
                        ViewKind::Plan { cut_height, .. } => ("Cut height:", cut_height, " m"),
                        ViewKind::Section { depth, .. } => ("Depth:", depth, " m"),
                        ViewKind::Axonometric { altitude, .. } => ("Altitude:", altitude, "°"),
                        ViewKind::Schedule { .. } => return,
                    };
                    ui.label(text);
                    let field = if unit == "°" {
                        egui::DragValue::new(value)
                            .speed(0.05)
                            .range(-89.0..=89.0)
                            .suffix(unit)
                    } else {
                        crate::units::drag(value).range(0.1..=200.0)
                    };
                    let changed = ui.add(field).changed();
                    let restyled = shadable && view_style_combo(ui, "classic_style", &mut v.style);
                    if changed || restyled {
                        let r = self.doc.edit_merged(id, "Change view", |e| {
                            e.kind = ElementKind::View(v.clone())
                        });
                        self.report(r, "View changed.");
                    }
                    if ui
                        .button("Tag doors and windows")
                        .on_hover_text("Tag every door and window in this view that has no tag yet")
                        .clicked()
                    {
                        let r = planview::tag_openings(&mut self.doc, id);
                        self.status = match r {
                            Ok(0) => "Every door and window here is tagged already.".into(),
                            Ok(n) => {
                                format!("Tagged {n} doors and windows. Drag a tag to move it.")
                            }
                            Err(e) => format!("Couldn't do that: {e}"),
                        };
                    }
                    if matches!(v.kind, ViewKind::Plan { .. })
                        && ui
                            .button("Tag fixtures")
                            .on_hover_text(
                                "Tag every socket, switch, light, drain, radiator and other fixture on this plan with its type's mark",
                            )
                            .clicked()
                    {
                        let r = planview::tag_fixtures(&mut self.doc, id);
                        self.status = match r {
                            Ok(0) => "Every fixture here is tagged already.".into(),
                            Ok(n) => format!("Tagged {n} fixtures. Drag a tag to move it."),
                            Err(e) => format!("Couldn't do that: {e}"),
                        };
                    }
                }
            }
            ui.label("Wall thickness:");
            ui.add(crate::units::drag(&mut self.plan.wall_thickness).range(0.05..=1.0));
            egui::ComboBox::from_id_salt("wall_location")
                .icon(crate::props::combo_icon)
                .selected_text(self.plan.wall_location.label())
                .show_ui(ui, |ui| {
                    for l in creabim_core::LocationLine::ALL {
                        ui.selectable_value(&mut self.plan.wall_location, l, l.label());
                    }
                });
            ui.checkbox(&mut self.plan.wall_flip, "Exterior left");
        });
    }

    /// "Place view" picker shown while a sheet is open.
    fn place_view_combo(&mut self, ui: &mut egui::Ui, sheet: ElementId) {
        let placed: Vec<ElementId> = self
            .doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Viewport(vp) if vp.sheet == sheet => Some(vp.view),
                _ => None,
            })
            .collect();
        let mut chosen = None;
        egui::ComboBox::from_id_salt("place_view")
            .icon(crate::props::combo_icon)
            .selected_text("Place view…")
            .width(ui.available_width().min(150.0))
            .truncate()
            .show_ui(ui, |ui| {
                for (id, name, _) in self.views() {
                    let label = if placed.contains(&id) {
                        format!("{name} (placed)")
                    } else {
                        name
                    };
                    if ui.selectable_label(false, label).clicked() {
                        chosen = Some(id);
                    }
                }
            });
        if let Some(view) = chosen {
            self.place_view(sheet, view);
        }
    }

    fn new_sheet(&mut self) {
        let used: Vec<String> = sheets::sheets(self.doc.model())
            .into_iter()
            .map(|(_, s)| s.number.clone())
            .collect();
        let number = (301..)
            .map(|n| format!("A{n}"))
            .find(|n| !used.contains(n))
            .expect("unbounded");
        let e = Element::new("New Sheet", ElementKind::Sheet(Sheet::a(number, "A3")));
        let id = e.id;
        let r = self.doc.commit(Transaction::new("New sheet").insert(e));
        self.active_view = Some(id);
        self.selection.only(id);
        self.report(
            r,
            "New A3 sheet. Use Place view… in the toolbar to put views on it.",
        );
    }

    fn new_schedule(&mut self, category: ScheduleCategory) {
        let label = category.label();
        let e = Element::new(
            category.schedule_name(),
            ElementKind::View(View::schedule(category)),
        );
        let id = e.id;
        let r = self
            .doc
            .commit(Transaction::new(format!("New {} schedule", label.to_lowercase())).insert(e));
        self.active_view = Some(id);
        self.report(
            r,
            "New schedule. Edit cells to change the model, or place it on a sheet with Place view….",
        );
    }

    fn materials(&self) -> Vec<(ElementId, String)> {
        self.doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Material(_)))
            .map(|e| (e.id, e.name.clone()))
            .collect()
    }

    /// A material picker; "New material" adds one and picks it.
    fn material_combo(&mut self, ui: &mut egui::Ui, current: &mut Option<ElementId>) -> bool {
        let materials = self.materials();
        let name = current
            .and_then(|id| materials.iter().find(|m| m.0 == id))
            .map_or("By type", |m| m.1.as_str())
            .to_string();
        let mut changed = false;
        let mut create = false;
        crate::app::helpui::combo(ui, "Material")
            .selected_text(name)
            .show_ui(ui, |ui| {
                changed |= ui.selectable_value(current, None, "By type").changed();
                for (id, n) in &materials {
                    changed |= ui.selectable_value(current, Some(*id), n).changed();
                }
                ui.separator();
                create = ui.button("New material…").clicked();
            });
        if create {
            let e = Element::new(
                creabim_core::library::unique_name(self.doc.model(), "New material"),
                ElementKind::Material(Material::new(Hatch::Diagonal, [0.8, 0.8, 0.8])),
            );
            *current = Some(e.id);
            let r = self.doc.commit(Transaction::new("New material").insert(e));
            self.open_materials(*current);
            self.report(r, "New material. Set its look in the material browser.");
            changed = true;
        }
        changed
    }

    fn outliner(&mut self, ui: &mut egui::Ui) {
        let _perf = Perf::new("outliner");
        ui.heading("Model");
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut groups: Vec<(String, Vec<(ElementId, String)>)> = Vec::new();
            for e in self.doc.model().iter() {
                if matches!(e.kind, ElementKind::Units(_)) {
                    continue;
                }
                let class = e.ifc_class().to_string();
                match groups.iter_mut().find(|(c, _)| *c == class) {
                    Some((_, v)) => v.push((e.id, e.name.clone())),
                    None => groups.push((class, vec![(e.id, e.name.clone())])),
                }
            }
            for (class, items) in groups {
                let opens = class == "View" || class == "Sheet";
                let title = match class.as_str() {
                    "View" => format!("Views ({}), click to open", items.len()),
                    "Sheet" => format!("Sheets ({}), click to open", items.len()),
                    _ => format!("{class} ({})", items.len()),
                };
                let id = ui.make_persistent_id(("group", &class));
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    class == "IfcBuildingStorey" || opens,
                )
                .show_header(ui, |ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                    Icon::of_class(&class).paint(ui.painter(), r);
                    ui.label(title);
                })
                .body(|ui| {
                    for (id, name) in items {
                        let icon = self.doc.model().get(id).map_or(Icon::Mass, Icon::of);
                        let row = icons::row(ui, icon, self.selection.contains(id), &name);
                        if row.clicked() && ui.input(|i| i.modifiers.shift || i.modifiers.command) {
                            self.selection.toggle(id);
                            self.face = None;
                        } else if row.clicked() {
                            self.selection.only(id);
                            self.face = None;
                            if opens {
                                self.active_view = Some(id);
                            }
                        }
                    }
                });
            }
        });
    }

    /// Booleans on a wall, slab or mass: the cuts it has, and a menu to cut
    /// it with another element (which then turns into a see-through ghost).
    fn cuts_panel(&mut self, ui: &mut egui::Ui, id: ElementId) {
        let cuttable = |k: &ElementKind| {
            matches!(
                k,
                ElementKind::Wall(_) | ElementKind::Slab(_) | ElementKind::FreeMesh(_)
            )
        };
        let model = self.doc.model();
        if !model.get(id).is_some_and(|e| cuttable(&e.kind)) {
            return;
        }
        let existing: Vec<(ElementId, Cut, String)> = model
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Cut(c) if c.target == id => Some((
                    e.id,
                    *c,
                    model
                        .get(c.cutter)
                        .map_or(String::new(), |x| x.name.clone()),
                )),
                _ => None,
            })
            .collect();
        // masses first: they are what you usually cut with
        let mut candidates: Vec<(bool, ElementId, String)> = model
            .iter()
            .filter(|e| e.id != id && cuttable(&e.kind))
            .filter(|e| !existing.iter().any(|(_, c, _)| c.cutter == e.id))
            .map(|e| {
                (
                    !matches!(e.kind, ElementKind::FreeMesh(_)),
                    e.id,
                    e.name.clone(),
                )
            })
            .collect();
        candidates.sort_by_key(|c| c.0);
        ui.separator();
        ui.label("Booleans");
        let mut remove = None;
        let mut select = None;
        let mut set_op = None;
        for (cid, c, name) in &existing {
            crate::props::flow(ui, |ui| {
                // cut away, joined on, or only what both hold (MU38)
                egui::ComboBox::from_id_salt(("cut.op", *cid))
                    .icon(crate::props::combo_icon)
                    .selected_text(c.op.label())
                    .show_ui(ui, |ui| {
                        for op in BoolOp::ALL {
                            if ui.selectable_label(op == c.op, op.label()).clicked() && op != c.op {
                                set_op = Some((*cid, op));
                            }
                        }
                    });
                ui.label(name.as_str());
                if ui.small_button("Select").clicked() {
                    select = Some(*cid);
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(*cid);
                }
            });
        }
        let mut add = None;
        egui::ComboBox::from_id_salt("cut_with")
            .icon(crate::props::combo_icon)
            .selected_text("Subtract…")
            .show_ui(ui, |ui| {
                for (_, cid, name) in &candidates {
                    if ui.selectable_label(false, name).clicked() {
                        add = Some(*cid);
                    }
                }
            });
        ui.label(
            egui::RichText::new(
                "Pick an element to cut away. It turns into a see-through ghost you can still select and move.",
            )
            .weak(),
        );
        if let Some((c, op)) = set_op {
            let r = self.doc.edit_merged(c, "Boolean kind", |e| {
                if let ElementKind::Cut(k) = &mut e.kind {
                    k.op = op;
                }
            });
            self.report(r, format!("Boolean: {}.", op.label()));
        }
        if let Some(c) = remove {
            let r = self
                .doc
                .commit(Transaction::new("Remove boolean").remove_cascade(self.doc.model(), c));
            self.report(r, "Boolean removed.");
        }
        if let Some(cutter) = add {
            let e = Element::new(
                "Cut",
                ElementKind::Cut(Cut {
                    target: id,
                    cutter,
                    op: BoolOp::Difference,
                }),
            );
            let r = self.doc.commit(Transaction::new("Subtract").insert(e));
            self.report(r, "Subtracted. Move the ghost to change the cut.");
        }
        if let Some(c) = select {
            self.selection.only(c);
        }
    }

    /// While a placing or drawing tool is on: what the next element will be,
    /// the tool bar's settings as Properties rows, edited in both places
    /// (MU18). False when the tool has none.
    fn dark_paper(&self, ctx: &egui::Context) -> bool {
        match self.shell.settings.drawing_paper.as_str() {
            "dark" => true,
            "light" => false,
            _ => crate::theme::Palette::of(ctx).dark,
        }
    }

    fn next_element_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let ctx = ui.ctx().clone();
        let (_, options) = self.tool_strip(&ctx);
        let rows: Vec<&serde_json::Value> = options
            .iter()
            .filter(|o| o["kind"] == "number" || o["kind"] == "toggle")
            .filter(|o| o["key"] != "view.number")
            .collect();
        if rows.is_empty() {
            return false;
        }
        let tool = self.plan.tool;
        ui.label(
            egui::RichText::new(format!("Next {}", tool.label().to_lowercase()))
                .strong()
                .size(15.0),
        );
        if let Some(t) = typesui_type_name(self, tool) {
            ui.label(egui::RichText::new(format!("Type: {t}")).weak());
        }
        ui.label(
            egui::RichText::new(
                "Set it before you click; the ghost shows it. The same settings are in the tool bar.",
            )
            .weak(),
        );
        // an object's own make: its size and how it is fixed (MU19)
        if tool == Tool::Object {
            if let Some(c) = &self.plan.objects.current {
                let (lo, hi) = c.object.bounds();
                let s = hi - lo;
                ui.label(
                    egui::RichText::new(format!(
                        "{}: {} × {} × {}, {}",
                        c.name,
                        crate::units::len(s.x),
                        crate::units::len(s.y),
                        crate::units::len(s.z),
                        match c.object.mount {
                            creabim_core::objects::Mount::Wall => "on a wall",
                            creabim_core::objects::Mount::Floor => "on the floor",
                            creabim_core::objects::Mount::Ceiling => "hung from the ceiling",
                        }
                    ))
                    .weak(),
                );
            }
        }
        ui.add_space(4.0);
        let mut set: Vec<(String, f64)> = vec![];
        let mut act: Vec<String> = vec![];
        for o in rows {
            let label = o["label"].as_str().unwrap_or("").to_string();
            let hint = o["hint"].as_str().unwrap_or("");
            if o["kind"] == "number" {
                let key = o["key"].as_str().unwrap_or("").to_string();
                let mut v = o["value"].as_f64().unwrap_or(0.0);
                let (lo, hi) = (
                    o["min"].as_f64().unwrap_or(f64::MIN),
                    o["max"].as_f64().unwrap_or(f64::MAX),
                );
                let unit = o["unit"].as_str().unwrap_or("");
                let changed = crate::props::row(ui, &label, |ui| {
                    let r = if unit == "m" {
                        crate::props::fill(ui, crate::units::drag(&mut v).range(lo..=hi))
                    } else {
                        crate::props::fill(
                            ui,
                            egui::DragValue::new(&mut v)
                                .range(lo..=hi)
                                .suffix(format!(" {unit}")),
                        )
                    };
                    r.on_hover_text(hint).changed()
                });
                if changed {
                    set.push((key, v));
                }
            } else if o["action"].as_bool().unwrap_or(false) {
                // a step, not a setting: a button
                if ui.button(label).on_hover_text(hint).clicked() {
                    act.push(o["id"].as_str().unwrap_or("").to_string());
                }
            } else {
                let mut on = o["on"].as_bool().unwrap_or(false);
                if ui.checkbox(&mut on, label).on_hover_text(hint).changed() {
                    act.push(o["id"].as_str().unwrap_or("").to_string());
                }
            }
        }
        for (k, v) in set {
            self.set_tool_number(&k, v);
        }
        for id in act {
            self.run_action(&ctx, &id);
        }
        true
    }

    /// Trace and reference for view `v` (MU18): what shows under it, in what
    /// colour and strength, moved or turned to compare.
    /// Take up `tool`, as its button does.
    fn pick_tool(&mut self, ctx: &egui::Context, tool: Tool) {
        if let Some(i) = Tool::ALL.iter().position(|t| *t == tool) {
            self.run_action(ctx, &format!("tool:{i}"));
        }
    }

    /// The sheet, or the view of the same kind, `by` places on in the
    /// project's order (PgUp / PgDn, MU24).
    fn step_view(&mut self, by: i32) {
        let Some(now) = self.active_view else { return };
        let model = self.doc.model();
        let kind = |id: ElementId| match model.get(id).map(|e| &e.kind) {
            Some(ElementKind::Sheet(_)) => Some(0u8),
            Some(ElementKind::View(v)) => Some(match v.kind {
                ViewKind::Plan { .. } => 1,
                ViewKind::Section { .. } => 2,
                ViewKind::Axonometric { .. } => 3,
                ViewKind::Schedule { .. } => 4,
            }),
            _ => None,
        };
        let Some(k) = kind(now) else { return };
        let list: Vec<(ElementId, String)> = if k == 0 {
            sheets::sheets(model)
                .into_iter()
                .map(|(id, s)| {
                    (
                        id,
                        format!(
                            "{} {}",
                            s.number,
                            model.get(id).map_or("", |e| e.name.as_str())
                        ),
                    )
                })
                .collect()
        } else {
            model
                .iter()
                .filter(|e| kind(e.id) == Some(k))
                .map(|e| (e.id, e.name.clone()))
                .collect()
        };
        let Some(i) = list.iter().position(|(id, _)| *id == now) else {
            return;
        };
        let j = (i as i32 + by).rem_euclid(list.len() as i32) as usize;
        let (id, name) = list[j].clone();
        self.active_view = Some(id);
        self.status = format!(
            "{name} ({}/{}). PgUp / PgDn step through them.",
            j + 1,
            list.len()
        );
    }

    /// Shift+A (MU24, as Blender's Add menu): every tool, found by typing,
    /// the ones that can't be used in the view in front greyed.
    fn add_menu_window(&mut self, ctx: &egui::Context) {
        let Some(mut q) = self.add_menu.clone() else {
            return;
        };
        let mut pick = None;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let at = ctx
            .input(|i| i.pointer.hover_pos())
            .unwrap_or(ctx.content_rect().center());
        let shown = egui::Area::new(egui::Id::new("add.menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(240.0);
                    ui.label(egui::RichText::new("Add").strong());
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut q)
                            .vertical_align(egui::Align::Center)
                            .hint_text("Type to find a tool"),
                    );
                    r.request_focus();
                    let want = q.to_lowercase();
                    let found: Vec<Tool> = Tool::ALL
                        .iter()
                        .copied()
                        .filter(|t| *t != Tool::Select && t.label().to_lowercase().contains(&want))
                        .collect();
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for t in &found {
                                if ui
                                    .add(egui::Button::new(t.label()).frame(false))
                                    .on_hover_text(t.hint())
                                    .clicked()
                                {
                                    pick = Some(*t);
                                }
                            }
                        });
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        pick = found.first().copied();
                    }
                    if found.is_empty() {
                        ui.label(egui::RichText::new("Nothing by that name").weak());
                    }
                });
            });
        let outside = ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !shown.response.rect.contains(p));
        if ctx.input(|i| i.pointer.any_click()) && outside {
            close = true;
        }
        if let Some(t) = pick {
            self.add_menu = None;
            self.pick_tool(ctx, t);
            return;
        }
        self.add_menu = if close { None } else { Some(q) };
    }

    /// A north arrow or a scale bar on `sheet`, where it usually goes:
    /// over the title block's right end, or its left (MU22).
    /// A revision cloud (the next letter after the project's last), or the
    /// sheet's revisions table, on `sheet` (Annotate board step 4).
    fn add_revision_mark(&mut self, sheet: ElementId, table: bool) {
        let model = self.doc.model();
        let Some(s) = sheets::sheet_of(model, sheet) else {
            return;
        };
        let (w, h) = (s.width, s.height);
        let mark = if table {
            let m = creabim_geom::sheet::MARGIN;
            let top =
                creabim_geom::sheet::title_block_box(model, sheet).map_or(m + 45.0, |(_, hi)| hi.y);
            creabim_core::PaperMark::revision_table(
                sheet,
                glam::DVec2::new(w - m - 112.0, top + 34.0),
            )
        } else {
            let last = model
                .iter()
                .filter_map(|e| match &e.kind {
                    ElementKind::PaperMark(pm) => match &pm.kind {
                        creabim_core::PaperMarkKind::Cloud { rev, .. } => rev.chars().next(),
                        _ => None,
                    },
                    _ => None,
                })
                .max();
            let next = last
                .map_or('A', |c| ((c as u8).saturating_add(1)) as char)
                .to_string();
            creabim_core::PaperMark::cloud(sheet, glam::DVec2::new(w / 2.0, h / 2.0), &next)
        };
        let what = if table {
            "Revisions table"
        } else {
            "Revision cloud"
        };
        let e = Element::new(what, ElementKind::PaperMark(mark));
        let id = e.id;
        if self
            .doc
            .commit(
                creabim_core::Transaction::new(format!("Add {}", what.to_lowercase())).insert(e),
            )
            .is_ok()
        {
            self.selection.only(id);
            self.prop_tab = PropTab::Element;
            self.status = if table {
                "Revisions table added: a row for each revision cloud on this sheet. Drag it where it goes.".into()
            } else {
                "Revision cloud added: drag it over the change, size it, and say what changed in Properties.".into()
            };
        }
    }

    fn add_paper_mark(&mut self, sheet: ElementId, north: bool) {
        let model = self.doc.model();
        let Some(s) = sheets::sheet_of(model, sheet) else {
            return;
        };
        let (w, m) = (s.width, creabim_geom::sheet::MARGIN);
        // clear of the title block as drawn, whatever its template
        let tb = creabim_geom::sheet::TITLE_BLOCK;
        let (tb_left, tb_top) = creabim_geom::sheet::title_block_box(model, sheet)
            .map_or((w - m - tb.x, m + tb.y), |(lo, hi)| (lo.x, hi.y));
        let mark = if north {
            creabim_core::PaperMark::north_arrow(
                sheet,
                glam::DVec2::new(w - m - 16.0, tb_top + 18.0),
            )
        } else {
            // the scale of the views on it, else 1:100
            let scale = model
                .iter()
                .find_map(|e| match &e.kind {
                    ElementKind::Viewport(vp) if vp.sheet == sheet => {
                        match model.get(vp.view).map(|v| &v.kind) {
                            Some(ElementKind::View(v))
                                if !matches!(v.kind, ViewKind::Schedule { .. }) =>
                            {
                                Some(v.scale)
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                })
                .unwrap_or(100.0);
            creabim_core::PaperMark::scale_bar(
                sheet,
                glam::DVec2::new(tb_left + 4.0, tb_top + 10.0),
                scale,
            )
        };
        let what = if north { "North arrow" } else { "Scale bar" };
        let e = Element::new(what, ElementKind::PaperMark(mark));
        let id = e.id;
        if self
            .doc
            .commit(
                creabim_core::Transaction::new(format!("Add {}", what.to_lowercase())).insert(e),
            )
            .is_ok()
        {
            self.selection.only(id);
            self.prop_tab = PropTab::Element;
            self.status = format!(
                "{what} added. Drag it where it goes; Properties sets its size{}.",
                if north {
                    " and angle"
                } else {
                    ", scale and length"
                }
            );
        }
    }

    /// The sheets the Export sheets dialog would write, in sheet order.
    fn sheets_to_export(&self, this_only: bool) -> Vec<ElementId> {
        let all: Vec<ElementId> = sheets::sheets(self.doc.model())
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        if this_only {
            return self
                .active_view
                .filter(|id| all.contains(id))
                .into_iter()
                .collect();
        }
        match self.sheets_export.scope {
            ExportScope::All => all,
            ExportScope::Chosen => all
                .into_iter()
                .filter(|id| self.sheets_export.chosen.contains(id))
                .collect(),
            ExportScope::This => self
                .active_view
                .filter(|id| all.contains(id))
                .into_iter()
                .collect(),
        }
    }

    /// Write sheets as PDF: this one (`this_only`), else as the dialog says.
    pub(super) fn export_sheets_now(&mut self, this_only: bool) {
        let ids = self.sheets_to_export(this_only);
        if ids.is_empty() {
            self.status = if this_only {
                "Open a sheet first: Ctrl+P makes a PDF of the sheet in front.".into()
            } else {
                "No sheets to export: pick some, or choose All.".into()
            };
            return;
        }
        let info = self.sheet_info();
        let model = self.doc.model();
        let name_of = |id: ElementId| {
            let n = sheets::sheet_of(model, id)
                .map(|s| s.number.clone())
                .unwrap_or_default();
            let t = model.get(id).map(|e| e.name.clone()).unwrap_or_default();
            let raw = format!("{n} {t}");
            raw.trim()
                .chars()
                .map(|c| if "\\/:*?\"<>|".contains(c) { '-' } else { c })
                .collect::<String>()
        };
        if this_only || !self.sheets_export.each {
            let default = if ids.len() == 1 {
                let p = std::path::Path::new(&self.path);
                p.parent()
                    .unwrap_or(std::path::Path::new(""))
                    .join(format!("{}.pdf", name_of(ids[0])))
                    .to_string_lossy()
                    .into_owned()
            } else {
                with_extension(&self.path, "pdf")
            };
            let pdf = sheets::sheets_pdf_of(model, &info, &ids);
            let Some(out) = self.export_to(default) else {
                return;
            };
            let r = std::fs::write(&out, pdf);
            let n = ids.len();
            self.report(
                r,
                format!(
                    "Exported {n} sheet{} to {out}",
                    if n == 1 { "" } else { "s" }
                ),
            );
            return;
        }
        // a PDF each, named by number and name, in a folder
        let dir = if cfg!(test) {
            Some(std::env::temp_dir())
        } else {
            let start = std::path::absolute(&self.path)
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let mut d = rfd::FileDialog::new().set_title("Export sheets: a PDF each, into…");
            if let Some(s) = start {
                d = d.set_directory(s);
            }
            d.pick_folder()
        };
        let Some(dir) = dir else { return };
        let mut written = 0;
        for id in &ids {
            let pdf = sheets::sheets_pdf_of(model, &info, &[*id]);
            if std::fs::write(dir.join(format!("{}.pdf", name_of(*id))), pdf).is_ok() {
                written += 1;
            }
        }
        self.last_export_dir = Some(dir.clone());
        self.status = format!(
            "Exported {written} sheet PDF{} to {}",
            if written == 1 { "" } else { "s" },
            dir.display()
        );
    }

    /// The Export sheets dialog (MU22): all, chosen or this sheet, as one
    /// PDF or a PDF each.
    fn sheets_export_window(&mut self, ctx: &egui::Context) {
        if !self.sheets_export_open {
            return;
        }
        let model = self.doc.model();
        let all: Vec<(ElementId, String)> = sheets::sheets(model)
            .into_iter()
            .map(|(id, s)| {
                let t = model.get(id).map(|e| e.name.clone()).unwrap_or_default();
                (id, format!("{} {t}", s.number))
            })
            .collect();
        // chosen starts as the sheets selected in the browser
        if self.sheets_export.chosen.is_empty() {
            self.sheets_export.chosen = self
                .selection
                .all()
                .iter()
                .copied()
                .filter(|id| all.iter().any(|(s, _)| s == id))
                .collect();
        }
        let this = self
            .active_view
            .filter(|id| all.iter().any(|(s, _)| s == id));
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        let x = &mut self.sheets_export;
        crate::app::shell::window("Export sheets")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                crate::props::row(ui, "Sheets", |ui| {
                    crate::props::flow(ui, |ui| {
                        ui.selectable_value(
                            &mut x.scope,
                            ExportScope::All,
                            format!("All ({})", all.len()),
                        );
                        ui.selectable_value(
                            &mut x.scope,
                            ExportScope::Chosen,
                            format!("Chosen ({})", x.chosen.len()),
                        );
                        ui.add_enabled_ui(this.is_some(), |ui| {
                            ui.selectable_value(&mut x.scope, ExportScope::This, "This one")
                                .on_disabled_hover_text("Open a sheet first");
                        });
                    })
                });
                if x.scope == ExportScope::Chosen {
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for (id, name) in &all {
                                let mut on = x.chosen.contains(id);
                                if crate::props::check(ui, "", &mut on, name.as_str()).changed() {
                                    if on {
                                        x.chosen.insert(*id);
                                    } else {
                                        x.chosen.remove(id);
                                    }
                                }
                            }
                        });
                }
                crate::props::row(ui, "As", |ui| {
                    crate::props::flow(ui, |ui| {
                        ui.selectable_value(&mut x.each, false, "One PDF");
                        ui.selectable_value(&mut x.each, true, "A PDF each");
                    })
                });
                crate::props::row(ui, "Paper", |ui| {
                    ui.label(egui::RichText::new("As printed: white, at the sheets' sizes").weak())
                });
                ui.add_space(6.0);
                crate::props::flow(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        go = ui.button("Export…").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            });
        if go {
            self.sheets_export_open = false;
            self.export_sheets_now(false);
        } else if cancel || !open {
            self.sheets_export_open = false;
        }
    }

    /// Hide every reference shown in view `v`, or show them again.
    fn toggle_references(&mut self, v: ElementId) {
        let model = self.doc.model();
        let refs: Vec<Element> = creabim_core::reference::in_view(model, v)
            .iter()
            .filter_map(|(id, _)| model.get(*id).cloned())
            .collect();
        if refs.is_empty() {
            self.status = "No reference in this view: add one in Properties > Trace.".into();
            return;
        }
        let hide = refs
            .iter()
            .any(|e| matches!(&e.kind, ElementKind::Reference(r) if !r.hidden));
        let mut tx = creabim_core::Transaction::new(if hide {
            "Hide references"
        } else {
            "Show references"
        });
        for e in refs {
            let mut after = e.clone();
            if let ElementKind::Reference(r) = &mut after.kind {
                r.hidden = hide;
            }
            tx = tx.replace(e, after);
        }
        if self.doc.commit(tx).is_ok() {
            self.status = if hide {
                "References hidden (Alt+F2 shows them).".into()
            } else {
                "References shown (Alt+F2 hides them).".into()
            };
        }
    }

    /// A new reference shown in view `v`, in place, selected.
    fn add_reference(&mut self, v: ElementId, source: creabim_core::RefSource) {
        let model = self.doc.model();
        let shown = crate::trace::read(model, &source);
        if let crate::trace::Shown::Missing(why) = &shown {
            self.status = why.clone();
            return;
        }
        let mut r = creabim_core::Reference::new(source, vec![v]);
        if let Some(b) = crate::trace::bounds_of(&r.source, &shown) {
            r.in_place(b);
        }
        // a picture keeps its own colours; lines take the first colour
        if matches!(shown, crate::trace::Shown::Picture(_)) {
            r.colour = None;
            r.opacity = 0.6;
        }
        // named for what it shows, without a prefix eating the field (MU24)
        let name = crate::trace::label(model, &r.source);
        let e = Element::new(name.clone(), ElementKind::Reference(r));
        let id = e.id;
        if self
            .doc
            .commit(creabim_core::Transaction::new(format!("Add {name}")).insert(e))
            .is_ok()
        {
            self.selection.only(id);
            self.prop_tab = PropTab::Element;
            self.status = "Reference added. Drag its middle to move it, a corner to scale it, the round grip to turn it; Calibrate sets an exact size. Alt+F2 hides it.".into();
        }
    }

    fn trace_panel(&mut self, ui: &mut egui::Ui, v: ElementId) {
        use creabim_core::RefSource;
        ui.add_space(6.0);
        // the tab's own title says what it is; this section lists them,
        // and its ? says what a reference is (MU27)
        if !self.prop_header(
            ui,
            "trace.list",
            "References here",
            true,
            Some("trace-reference"),
        ) {
            return;
        }
        let model = self.doc.model();
        let here: Vec<(ElementId, String, bool)> = creabim_core::reference::in_view(model, v)
            .into_iter()
            .map(|(id, r)| (id, crate::trace::label(model, &r.source), r.hidden))
            .collect();
        let others: Vec<(ElementId, String)> = model
            .iter()
            .filter(|e| e.id != v)
            .filter_map(|e| match &e.kind {
                ElementKind::View(view)
                    if matches!(view.kind, ViewKind::Plan { .. } | ViewKind::Section { .. }) =>
                {
                    Some((e.id, e.name.clone()))
                }
                _ => None,
            })
            .collect();
        if here.is_empty() {
            ui.label(egui::RichText::new("None yet: add one below.").weak());
        }
        let mut add = None;
        // one row of three equal buttons, as Blender's Add row (MU27)
        crate::props::flow(ui, |ui| {
            let w = ((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0).max(40.0);
            ui.spacing_mut().interact_size.x = w;
            egui::ComboBox::from_id_salt(("trace.add", v))
                .icon(crate::props::combo_icon)
                .selected_text("+ View")
                .width(w)
                .show_ui(ui, |ui| {
                    for (id, name) in &others {
                        if ui.selectable_label(false, name).clicked() {
                            add = Some(RefSource::View(*id));
                        }
                    }
                })
                .response
                .on_hover_text("Another plan or section of this project, under this one");
            if ui
                .add_sized([w, 22.0], egui::Button::new("+ Picture"))
                .on_hover_text("A picture to trace over: a scanned plan, a photo of a sketch (PNG or JPEG). Calibrate it to size in its properties.")
                .clicked()
            {
                if let Some(p) = rfd::FileDialog::new()
                    .set_title("Add a picture as a reference")
                    .add_filter("Pictures", &["png", "jpg", "jpeg"])
                    .pick_file()
                {
                    add = Some(RefSource::Image {
                        path: p.display().to_string(),
                        width: 10.0,
                    });
                }
            }
            if ui
                .add_sized([w, 22.0], egui::Button::new("+ DXF"))
                .on_hover_text("A CAD drawing's lines (DXF). A DWG opens here once saved as DXF: any CAD program, or the free ODA File Converter, does it offline.")
                .clicked()
            {
                if let Some(p) = rfd::FileDialog::new()
                    .set_title("Add a DXF drawing as a reference")
                    .add_filter("DXF", &["dxf"])
                    .pick_file()
                {
                    add = Some(RefSource::Dxf {
                        path: p.display().to_string(),
                    });
                }
            }
        });
        if let Some(src) = add {
            self.add_reference(v, src);
        }
        ui.add_space(4.0);
        let mut select = None;
        let mut flip = None;
        // a long list scrolls in its own box; Add stays in reach (MU28)
        egui::ScrollArea::vertical()
            .id_salt(("trace.list", v))
            .max_height(240.0)
            .show(ui, |ui| {
                for (id, name, hidden) in &here {
                    crate::props::flow(ui, |ui| {
                        let eye = if *hidden { "eye-off" } else { "eye" };
                        let (rr, resp) =
                            ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                        crate::svgicon::paint(
                            ui.painter(),
                            rr.shrink(2.0),
                            eye,
                            ui.visuals().text_color(),
                        );
                        if resp
                            .on_hover_text(if *hidden { "Show it" } else { "Hide it" })
                            .clicked()
                        {
                            flip = Some(*id);
                        }
                        // one line, cut with … and whole on hover (MU23)
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        let on = self.selection.contains(*id);
                        if ui
                            .add_sized(
                                [ui.available_width(), 22.0],
                                egui::Button::selectable(on, name.as_str()).truncate(),
                            )
                            .on_hover_text(name.as_str())
                            .clicked()
                        {
                            select = Some(*id);
                        }
                    });
                }
            });
        if let Some(id) = select {
            self.selection.only(id);
            self.prop_tab = PropTab::Element;
        }
        if let Some(id) = flip {
            if let Some(e) = self.doc.model().get(id).cloned() {
                let mut after = e.clone();
                if let ElementKind::Reference(r) = &mut after.kind {
                    r.hidden = !r.hidden;
                }
                let _ = self.doc.commit(
                    creabim_core::Transaction::new("Show or hide a reference").replace(e, after),
                );
            }
        }
    }

    /// A reference's own properties (MU19): what it shows and where, how
    /// strongly, and its size. Returns the undo label of a change.
    fn reference_fields(
        &mut self,
        ui: &mut egui::Ui,
        id: ElementId,
        r: &mut creabim_core::Reference,
    ) -> Option<&'static str> {
        use creabim_core::reference::REFERENCE_COLOURS;
        use creabim_core::RefSource;
        let mut label = None;
        let model = self.doc.model();
        let what = match &r.source {
            RefSource::View(_) => "View",
            RefSource::Image { .. } => "Picture",
            RefSource::Dxf { .. } => "DXF drawing",
        };
        let name = crate::trace::label(model, &r.source);
        let path = match &r.source {
            RefSource::Image { path, .. } | RefSource::Dxf { path } => Some(path.clone()),
            RefSource::View(_) => None,
        };
        // as Blender's image rows (MU23): its name, cut with … and whole
        // on hover; copy it, pick another file, or take the reference away
        let mut replace = false;
        let mut remove = false;
        crate::props::row(ui, what, |ui| {
            crate::props::flow(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let icons = if path.is_some() { 3.0 } else { 2.0 };
                let w = (ui.available_width() - icons * 22.0).max(30.0);
                ui.add_sized(
                    [w, 22.0],
                    egui::Label::new(egui::RichText::new(name.clone()).strong()).truncate(),
                );
                if crate::props::icon_button(ui, "copy", "Copy the name").clicked() {
                    ui.ctx().copy_text(name.clone());
                }
                if path.is_some()
                    && crate::props::icon_button(ui, "folder-open", "Another file in its place")
                        .clicked()
                {
                    replace = true;
                }
                if crate::props::icon_button(ui, "x", "Take this reference away").clicked() {
                    remove = true;
                }
            })
        });
        if let Some(p) = &path {
            crate::props::row(ui, "File", |ui| {
                crate::props::flow(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let w = (ui.available_width() - 24.0).max(30.0);
                    ui.add_sized(
                        [w, 22.0],
                        egui::Label::new(egui::RichText::new(p.clone()).weak()).truncate(),
                    );
                    if crate::props::icon_button(
                        ui,
                        "refresh-cw",
                        "Read the file again, after it changed",
                    )
                    .clicked()
                    {
                        self.plan.ref_cache.forget(id);
                    }
                })
            });
        }
        if replace {
            let filter: (&str, &[&str]) = match &r.source {
                RefSource::Dxf { .. } => ("DXF", &["dxf"]),
                _ => ("Pictures", &["png", "jpg", "jpeg"]),
            };
            if let Some(p) = rfd::FileDialog::new()
                .set_title("Another file for this reference")
                .add_filter(filter.0, filter.1)
                .pick_file()
            {
                let p = p.display().to_string();
                match &mut r.source {
                    RefSource::Image { path, .. } | RefSource::Dxf { path } => *path = p,
                    RefSource::View(_) => {}
                }
                // it keeps its place, turn and scale; its size is read again
                let shown = crate::trace::read(self.doc.model(), &r.source);
                if let Some(b) = crate::trace::bounds_of(&r.source, &shown) {
                    let at = r.at;
                    r.bounds = Some(b);
                    r.at = at;
                }
                self.plan.ref_cache.forget(id);
                label = Some("Reference file");
            }
        }
        if remove {
            self.remove_ref = Some(id);
        }
        let model = self.doc.model();
        if let crate::trace::Shown::Missing(why) = self.plan.ref_cache.get(model, id, r) {
            ui.colored_label(ui.visuals().warn_fg_color, why);
        }
        // the views it shows in: plans and sections
        let views: Vec<(ElementId, String)> = model
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::View(view)
                    if matches!(view.kind, ViewKind::Plan { .. } | ViewKind::Section { .. }) =>
                {
                    Some((e.id, e.name.clone()))
                }
                _ => None,
            })
            .collect();
        // and the profiles, each by its trace view (MU38)
        let profs: Vec<(ElementId, String)> = creabim_core::profiles::project_profiles(model)
            .iter()
            .map(|p| (creabim_core::profiles::trace_view(&p.name), p.name.clone()))
            .collect();
        let on_profile = profs
            .iter()
            .find(|(v, _)| r.shown_in.contains(v))
            .map(|(v, n)| (*v, n.clone()));
        let mut views = views;
        views.extend(profs.iter().map(|(v, n)| (*v, format!("Profile {n}"))));
        if let Some((pv, pname)) = &on_profile {
            crate::props::row(ui, "Shown in", |ui| {
                if ui
                    .checkbox(&mut r.only_profile, "Only in this profile")
                    .on_hover_text(
                        "Kept to this profile: not in the plans, sections or 3D. \
                         Untick it to show it in other views too.",
                    )
                    .changed()
                {
                    if r.only_profile {
                        r.shown_in = vec![*pv];
                    }
                    label = Some(if r.only_profile {
                        "Reference only in its profile"
                    } else {
                        "Reference shared"
                    });
                }
            });
            if r.only_profile {
                crate::props::row(ui, "", |ui| {
                    ui.label(egui::RichText::new(format!("Profile: {pname}")).weak());
                });
            }
        }
        let n = r.shown_in.len();
        if !r.only_profile {
            // under the checkbox, the list needs no second label
            let title = if on_profile.is_some() { "" } else { "Shown in" };
            crate::props::row(ui, title, |ui| {
                egui::ComboBox::from_id_salt(("ref.views", id))
                    .icon(crate::props::combo_icon)
                    .selected_text(match n {
                        0 => "No view".to_string(),
                        1 => views
                            .iter()
                            .find(|(v, _)| r.shown_in.contains(v))
                            .map_or("1 view".into(), |(_, n)| n.clone()),
                        n => format!("{n} views"),
                    })
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (v, name) in &views {
                            let mut on = r.shown_in.contains(v);
                            if ui.checkbox(&mut on, name).changed() {
                                if on {
                                    r.shown_in.push(*v);
                                } else {
                                    r.shown_in.retain(|x| x != v);
                                }
                                label = Some("Reference views");
                            }
                        }
                    })
            });
        }
        // narrow, the swatches wrap rather than run off the panel (MU34)
        crate::props::row(ui, "Colour", |ui| {
            crate::props::flow(ui, |ui| {
                let swatch = |ui: &mut egui::Ui, c: Option<Color32>, on: bool, tip: &str| {
                    let (rr, resp) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                    match c {
                        Some(c) => {
                            ui.painter().circle_filled(rr.center(), 8.0, c);
                        }
                        None => {
                            // its own colours: a split swatch
                            ui.painter().circle_stroke(
                                rr.center(),
                                7.5,
                                egui::Stroke::new(1.2, ui.visuals().weak_text_color()),
                            );
                            ui.painter().line_segment(
                                [
                                    rr.center() + egui::vec2(-5.0, 5.0),
                                    rr.center() + egui::vec2(5.0, -5.0),
                                ],
                                egui::Stroke::new(1.2, ui.visuals().weak_text_color()),
                            );
                        }
                    }
                    if on {
                        ui.painter().circle_stroke(
                            rr.center(),
                            10.0,
                            egui::Stroke::new(1.5, ui.visuals().text_color()),
                        );
                    }
                    resp.on_hover_text(tip).clicked()
                };
                if swatch(ui, None, r.colour.is_none(), "Its own colours") {
                    r.colour = None;
                    label = Some("Reference colour");
                }
                for (i, (cname, [cr, cg, cb])) in REFERENCE_COLOURS.iter().enumerate() {
                    let c = Color32::from_rgb(*cr, *cg, *cb);
                    if swatch(ui, Some(c), r.colour == Some(i as u8), cname) {
                        r.colour = Some(i as u8);
                        label = Some("Reference colour");
                    }
                }
            })
        });
        crate::props::row(ui, "Strength", |ui| {
            if ui
                .add(egui::Slider::new(&mut r.opacity, 0.05..=1.0).show_value(false))
                .drag_stopped()
            {
                label = Some("Reference strength");
            }
        });
        if crate::props::choice(
            ui,
            "Drawn",
            &mut r.over,
            &[
                (false, "Under", "Under the model"),
                (true, "Over", "Over the model"),
            ],
        ) {
            label = Some(if r.over {
                "Reference over"
            } else {
                "Reference under"
            });
        }
        // one switch a row, so none runs off a narrow panel (MU34)
        if crate::props::switch(ui, &mut r.locked, "Locked").changed() {
            label = Some("Lock reference");
        }
        if crate::props::switch(ui, &mut r.hidden, "Hidden (Alt+F2)").changed() {
            label = Some("Hide reference");
        }
        // its 3D view, apart from its views', as Blender's image empties
        // (MU34)
        if !r.only_profile && crate::props::subsection(ui, "ref.3d", "In 3D", true) {
            if crate::props::switch(ui, &mut r.in_3d, "Shown")
                .on_hover_text(
                    "Shown in the 3D view too, where its views are; off, only in its views",
                )
                .changed()
            {
                label = Some("Reference in 3D");
            }
            if r.in_3d {
                let mut own = r.opacity_3d.is_some();
                if crate::props::switch(ui, &mut own, "Own strength").changed() {
                    r.opacity_3d = own.then_some(r.opacity);
                    label = Some("Reference strength in 3D");
                }
                if let Some(o) = &mut r.opacity_3d {
                    crate::props::row(ui, "Strength", |ui| {
                        if ui
                            .add(egui::Slider::new(o, 0.05..=1.0).show_value(false))
                            .drag_stopped()
                        {
                            label = Some("Reference strength in 3D");
                        }
                    });
                }
                if crate::props::switch(ui, &mut r.in_perspective, "In perspective").changed() {
                    label = Some("Reference in perspective");
                }
                if crate::props::switch(ui, &mut r.in_orthographic, "In orthographic").changed() {
                    label = Some("Reference in orthographic");
                }
            }
        }
        // its own box, folding (MU23)
        if !self.prop_header(
            ui,
            "refplace",
            "Place and size",
            true,
            Some("trace-reference"),
        ) {
            return label;
        }
        let mut at = r.at;
        let mut changed = false;
        crate::props::row(ui, "X", |ui| {
            changed |= crate::props::fill(ui, crate::units::drag(&mut at.x)).changed()
        });
        crate::props::row(ui, "Y", |ui| {
            changed |= crate::props::fill(ui, crate::units::drag(&mut at.y)).changed()
        });
        if changed && !r.locked {
            r.at = at;
            label = Some("Move reference");
        }
        crate::props::row(ui, "Rotation", |ui| {
            if crate::props::fill(
                ui,
                egui::DragValue::new(&mut r.rotation).speed(0.5).suffix("°"),
            )
            .changed()
            {
                label = Some("Turn reference");
            }
        });
        crate::props::row(ui, "Scale", |ui| {
            if crate::props::fill(
                ui,
                egui::DragValue::new(&mut r.scale)
                    .speed(0.01)
                    .range(0.0001..=10000.0)
                    .max_decimals(5),
            )
            .changed()
            {
                label = Some("Scale reference");
            }
        });
        if let RefSource::Image { width, .. } = &mut r.source {
            let before = *width;
            crate::props::row(ui, "Picture width", |ui| {
                crate::props::fill(ui, crate::units::drag(width).range(0.1..=10000.0))
            });
            if *width != before {
                // the picture keeps its middle where it is
                let k = *width / before;
                if let Some([a, b]) = r.bounds {
                    r.bounds = Some([a * k, b * k]);
                }
                label = Some("Picture width");
            }
        }
        // three equal buttons across the row, as Blender's (MU24); stacked
        // when the panel is too narrow for their words (MU34)
        let mut pressed = None;
        let narrow = ui.available_width() < 270.0;
        let layout = if narrow {
            egui::Layout::top_down_justified(egui::Align::Min)
        } else {
            egui::Layout::left_to_right(egui::Align::Center)
        };
        ui.with_layout(layout, |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let w = if narrow { ui.available_width() } else { ((ui.available_width() - 6.0) / 3.0).max(40.0) };
            for (i, (text, tip)) in [
                ("Calibrate…", "Click two points on it a known distance apart, then type that distance: it is scaled to fit (the first point stays)."),
                ("Align…", "Click two points on it (a wall line on a scan), then two on the model, a guide or an edge: it turns and moves so the first line lies on the second. Its size stays."),
                ("Back in place", "Where it was drawn: no move, turn or scale."),
            ]
            .into_iter()
            .enumerate()
            {
                if ui
                    .add_sized([w, 24.0], egui::Button::new(text).truncate())
                    .on_hover_text(tip)
                    .clicked()
                {
                    pressed = Some(i);
                }
            }
        });
        // the same drawing in another view, placed apart from this one
        // (MU38): moving one never moves the other
        let mut copy_to = None;
        let copy_btn = ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::Button::new("Copy to view…").truncate(),
            )
            .on_hover_text("A copy of it in another view, with its own place, size and settings");
        egui::Popup::menu(&copy_btn).show(|ui| {
            for (v, name) in &views {
                if r.shown_in.contains(v) {
                    continue;
                }
                if ui.button(name).clicked() {
                    copy_to = Some(*v);
                    ui.close();
                }
            }
        });
        if let Some(v) = copy_to {
            self.copy_ref = Some((id, v));
        }
        match pressed {
            Some(0) => {
                self.plan.calibrate = Some((id, vec![]));
                self.plan.align_ref = None;
                self.status = "Calibrate: click a first point on the reference (Alt: no snapping; Shift: square to it).".into();
            }
            Some(1) => {
                self.plan.align_ref = Some((id, vec![]));
                self.plan.calibrate = None;
                self.status = "Align: click a first point on the reference.".into();
            }
            Some(_) => {
                if let Some(b) = r.bounds {
                    r.in_place(b);
                    label = Some("Reference back in place");
                }
            }
            None => {}
        }
        label
    }

    /// Align's points picked: turn and move the reference (MU23); files
    /// dropped on a view become references there. While Calibrate or
    /// Align picks points, the reference stays selected, so its Properties
    /// stay in front.
    fn reference_steps(&mut self) {
        let picking = self
            .plan
            .calibrate
            .as_ref()
            .map(|c| c.0)
            .or(self.plan.align_ref.as_ref().map(|a| a.0))
            .or(self.plan.calibrated.map(|c| c.0));
        if let Some(id) = picking {
            if self.selection.active != Some(id) {
                self.selection.only(id);
                self.prop_tab = PropTab::Element;
            }
        }
        if let Some((id, v)) = self.copy_ref.take() {
            if let Some(e) = self.doc.model().get(id).cloned() {
                if let ElementKind::Reference(r) = &e.kind {
                    let mut r = r.clone();
                    let to_profile = !matches!(
                        self.doc.model().get(v).map(|x| &x.kind),
                        Some(ElementKind::View(_))
                    );
                    r.shown_in = vec![v];
                    r.only_profile = to_profile;
                    r.hidden = false;
                    let name = e.name.clone();
                    let copy = Element::new(name.clone(), ElementKind::Reference(r));
                    let cid = copy.id;
                    let res = self.doc.commit(
                        creabim_core::Transaction::new(format!("Copy {name} to a view"))
                            .insert(copy),
                    );
                    if res.is_ok() {
                        self.selection.only(cid);
                    }
                    self.report(
                        res,
                        format!("{name} copied: the copy moves and scales apart from the first."),
                    );
                }
            }
        }
        if let Some(id) = self.remove_ref.take() {
            let tx = creabim_core::Transaction::new("Remove reference")
                .remove_cascade(self.doc.model(), id);
            if self.doc.commit(tx).is_ok() {
                self.selection.clear();
                self.plan.ref_cache.forget(id);
                self.status = "Reference taken away (Ctrl+Z brings it back).".into();
            }
        }
        if let Some((id, [a1, a2, b1, b2])) = self.plan.aligned.take() {
            if let Some(e) = self.doc.model().get(id).cloned() {
                let mut after = e.clone();
                if let ElementKind::Reference(r) = &mut after.kind {
                    match r.align(a1, a2, b1, b2) {
                        Ok(()) => {
                            let turn = r.rotation;
                            if self
                                .doc
                                .commit(
                                    creabim_core::Transaction::new("Align reference")
                                        .replace(e, after),
                                )
                                .is_ok()
                            {
                                self.status = format!("Aligned: turned to {turn:.2}°.");
                            }
                        }
                        Err(why) => self.status = why,
                    }
                }
            }
        }
        for (view, at, path) in std::mem::take(&mut self.plan.dropped) {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let p = path.display().to_string();
            let source = match ext.as_str() {
                "png" | "jpg" | "jpeg" => creabim_core::RefSource::Image {
                    path: p,
                    width: 10.0,
                },
                "dxf" => creabim_core::RefSource::Dxf { path: p },
                _ => {
                    self.status = format!(
                        "{} can't be a reference: drop a PNG, JPEG or DXF file.",
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("That file")
                    );
                    continue;
                }
            };
            let picture = matches!(source, creabim_core::RefSource::Image { .. });
            self.add_reference(view, source);
            // a picture lands where it was dropped; a drawing keeps its
            // own coordinates
            if let (true, Some(id)) = (picture, self.selection.active) {
                let _ = self.doc.edit_merged(id, "Place reference", |e| {
                    if let ElementKind::Reference(r) = &mut e.kind {
                        r.at = at;
                    }
                });
            }
        }
    }

    /// Calibrate's last step: the distance between the two points picked.
    fn calibrate_window(&mut self, ctx: &egui::Context) {
        let Some((id, a, b)) = self.plan.calibrated else {
            return;
        };
        let mut open = true;
        let mut done = false;
        let mut go = false;
        let id_d = egui::Id::new("calibrate.distance");
        let mut d: f64 = ctx.data(|m| m.get_temp(id_d)).unwrap_or(a.distance(b));
        crate::app::shell::window("Calibrate")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!(
                    "The two points are {} apart now. How far apart should they be?",
                    crate::units::len(a.distance(b))
                ));
                let r = ui.add(crate::units::drag(&mut d).range(0.001..=100000.0));
                r.request_focus();
                crate::props::flow(ui, |ui| {
                    go = ui.button("Scale it").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Enter));
                    done = ui.button("Cancel").clicked();
                });
            });
        ctx.data_mut(|m| m.insert_temp(id_d, d));
        if go {
            if let Some(e) = self.doc.model().get(id).cloned() {
                let mut after = e.clone();
                if let ElementKind::Reference(r) = &mut after.kind {
                    match r.calibrate(a, b, d) {
                        Ok(()) => {
                            let k = r.scale;
                            if self
                                .doc
                                .commit(
                                    creabim_core::Transaction::new("Calibrate reference")
                                        .replace(e, after),
                                )
                                .is_ok()
                            {
                                self.status = format!("Calibrated: scale {k:.4}.");
                            }
                        }
                        Err(why) => self.status = why,
                    }
                }
            }
            done = true;
        }
        if done || !open {
            self.plan.calibrated = None;
            ctx.data_mut(|m| m.remove::<f64>(id_d));
        }
    }

    fn properties(&mut self, ui: &mut egui::Ui) {
        let _perf = Perf::new("properties");
        if self.crop_pick.is_some() {
            self.crop_pick_banner(ui);
        }
        let tab = self.prop_tab;
        let Some(id) = self.selection.active else {
            if tab == PropTab::Next {
                self.next_element_panel(ui);
                return;
            }
            if tab == PropTab::Project {
                self.project_props(ui);
                return;
            }
            if tab == PropTab::Notes {
                self.notes_pad(ui);
                return;
            }
            if tab == PropTab::Profile {
                self.profile_props(ui);
                return;
            }
            if tab == PropTab::Master {
                self.master_props(ui);
                return;
            }
            if tab == PropTab::Log {
                self.log_props(ui);
                return;
            }
            if tab == PropTab::Model3d {
                self.visibility_panel(ui, gridvis::VisTarget::Window3d, Some("3D view"));
                self.crop_list(ui, self.crop_owner());
                return;
            }
            if self.plan.tool == Tool::Grid {
                self.grid_systems_panel(ui);
            }
            // with nothing selected, the view shown in the 2D pane
            if let Some(gridvis::VisTarget::View(v)) = self.vis_target(false) {
                if tab == PropTab::Trace {
                    self.trace_panel(ui, v);
                    return;
                }
                let name = self.doc.model().get(v).map_or("", |e| e.name.as_str());
                let name = name.to_string();
                self.visibility_panel(
                    ui,
                    gridvis::VisTarget::View(v),
                    Some(&format!("View: {name}")),
                );
                // a section's or elevation's drawing style, as a plan's (MU21)
                // folding with its ?, as the other sections (MU37)
                let is_section = matches!(
                    self.doc.model().get(v).map(|e| &e.kind),
                    Some(ElementKind::View(x)) if matches!(x.kind, ViewKind::Section { .. })
                );
                let open = is_section
                    && self.prop_header(
                        ui,
                        "view.graphics",
                        "Graphics",
                        true,
                        Some("graphic-styles"),
                    );
                if let Some(ElementKind::View(view)) = self.doc.model().get(v).map(|e| &e.kind) {
                    if matches!(view.kind, ViewKind::Section { .. }) {
                        let mut style = view.graphics.drawing_style;
                        if open {
                            crate::props::row(ui, "Drawing style", |ui| {
                                egui::ComboBox::from_id_salt("section.panel.drawing_style")
                                    .icon(crate::props::combo_icon)
                                    .selected_text(style.label())
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for s in creabim_core::graphics::DrawingStyle::ALL {
                                            ui.selectable_value(&mut style, s, s.label())
                                                .on_hover_text(s.hint());
                                        }
                                    });
                            });
                        }
                        if style != view.graphics.drawing_style {
                            let r = self.doc.edit(v, "Drawing style", |e| {
                                if let ElementKind::View(x) = &mut e.kind {
                                    x.graphics.drawing_style = style;
                                }
                            });
                            self.report(r, format!("Drawing style: {}.", style.label()));
                        }
                    }
                }
                // a plan's graphics: weights, below-cut tint, view depth
                let is_plan = matches!(
                    self.doc.model().get(v).map(|e| &e.kind),
                    Some(ElementKind::View(x)) if matches!(x.kind, ViewKind::Plan { .. })
                );
                let plan_open = is_plan
                    && self.prop_header(
                        ui,
                        "view.graphics",
                        "Graphics",
                        true,
                        Some("graphic-styles"),
                    );
                if let Some(ElementKind::View(view)) = self.doc.model().get(v).map(|e| &e.kind) {
                    if matches!(view.kind, ViewKind::Plan { .. }) {
                        let (mut g, scale) = (view.graphics.clone(), view.scale);
                        let footings_hidden = view
                            .hidden
                            .categories
                            .contains(&creabim_core::visibility::Category::Footings);
                        if let Some(label) = if plan_open {
                            plan_graphics_ui(ui, &mut g, scale, false)
                        } else {
                            None
                        } {
                            let r = self.doc.edit_merged(v, label, |e| {
                                if let ElementKind::View(x) = &mut e.kind {
                                    x.graphics = g.clone();
                                }
                            });
                            self.report(r, "View changed.");
                        }
                        // a view depth can't show what the view hides (A7-3)
                        if footings_hidden && g.range.view_depth.is_some() && footings_note(ui) {
                            let r = self.doc.edit(v, "Show footings", |e| {
                                if let ElementKind::View(x) = &mut e.kind {
                                    x.hidden.categories.retain(|c| {
                                        *c != creabim_core::visibility::Category::Footings
                                    });
                                }
                            });
                            self.report(r, "Footings show in this view, dashed under the floor.");
                        }
                    }
                }
                // its range, template and overrides
                self.view_graphics_sections(ui, v);
            }
            return;
        };
        let Some(e) = self.doc.model().get(id).cloned() else {
            return;
        };
        // (its name, mark and actions are in the pinned header above)
        if tab == PropTab::View {
            if let Some(gridvis::VisTarget::View(v)) = self.vis_target(false) {
                self.visibility_panel(ui, gridvis::VisTarget::View(v), None);
                self.view_graphics_sections(ui, v);
            }
            return;
        }
        if tab == PropTab::Project {
            self.project_props(ui);
            return;
        }
        if tab == PropTab::Notes {
            self.notes_pad(ui);
            return;
        }
        if tab == PropTab::Modifiers {
            self.modifiers_tab(ui, id);
            return;
        }
        if tab == PropTab::Data {
            self.assign_panel(ui, id);
            return;
        }
        if tab == PropTab::Joins {
            if creabim_core::joins::joinable(&e.kind)
                && self.prop_header(ui, "joins", "Junctions", true, Some("joins"))
            {
                self.joins_section(ui, id);
            }
            self.cuts_panel(ui, id);
            return;
        }
        if tab == PropTab::Graphics {
            if matches!(e.kind, ElementKind::View(_)) {
                self.visibility_panel(ui, gridvis::VisTarget::View(id), None);
                self.view_graphics_sections(ui, id);
            } else {
                self.element_override_section(ui, id);
            }
            return;
        }
        if creabim_core::types::TypeCategory::of(&e.kind).is_some() {
            self.type_picker(ui, id);
        }
        // the type picker may have changed it
        let Some(e) = self.doc.model().get(id).cloned() else {
            return;
        };
        // its own folding panel, as Type and Geometry are (MU32)
        let hint = format!("say:{}", editmode::panel_hint(&e.kind));
        if self.plan.edit == Some(id)
            && self.prop_header(ui, "editing", "Editing", true, Some(&hint))
        {
            // the layers' grips, without having to know Tab (MU18), as a
            // choice in the field column
            if handles::has_layers(self.doc.model(), id) {
                let mut on = handles::layers();
                if crate::props::choice(
                    ui,
                    "Grips",
                    &mut on,
                    &[
                        (false, "Element", "The element's own grips (Shift+Tab)"),
                        (true, "Layers", "A grip at each layer's ends (Shift+Tab)"),
                    ],
                ) {
                    handles::set_layers(on);
                    self.status = if on {
                        "Layers: a grip at each layer's ends (and, on walls, its bottom and top). Drag one to run that layer on or hold it back; right-click to reset.".into()
                    } else {
                        "Back to the element's own grips.".into()
                    };
                }
            }
            ui.add_space(4.0);
        }

        let mut edited = e.clone();
        let mut label = "";
        let mut extrude_sketch = false;
        let mut draw_hole = false;
        let num = |ui: &mut egui::Ui, text: &str, v: &mut f64, lo: f64, hi: f64| -> bool {
            crate::props::row(ui, text, |ui| {
                crate::props::fill(ui, crate::units::drag(v).range(lo..=hi)).changed()
            })
        };
        let mut redraw = false;
        let mut new_style = false;
        let show_visibility = matches!(e.kind, ElementKind::View(_));
        let mut object_type = false;
        // a placed view's new scale: (its view, the scale)
        let mut rescale: Option<(ElementId, f64)> = None;
        // who it is comes first (above), then its sizes and build here,
        // then how views show it; each section folds and stays folded
        let section = if creabim_core::types::TypeCategory::of(&e.kind).is_some() {
            "Geometry and build"
        } else {
            // named for what it is, as Blender's panels (MU23)
            match &e.kind {
                ElementKind::Reference(_) => "Reference",
                ElementKind::Note(_) => "Note",
                ElementKind::PaperMark(m) => match m.kind {
                    creabim_core::PaperMarkKind::NorthArrow { .. } => "North arrow",
                    creabim_core::PaperMarkKind::ScaleBar { .. } => "Scale bar",
                    creabim_core::PaperMarkKind::Cloud { .. } => "Revision cloud",
                    creabim_core::PaperMarkKind::RevisionTable => "Revisions table",
                    creabim_core::PaperMarkKind::Picture { .. } => "Picture",
                    creabim_core::PaperMarkKind::Vector { .. } => "Vector picture",
                },
                _ => "Settings",
            }
        };
        if self.prop_header(ui, "element", section, true, None) {
            match &mut edited.kind {
                ElementKind::Level(l) => {
                    if num(ui, "Elevation", &mut l.elevation, -100.0, 1000.0) {
                        label = "Level elevation"
                    }
                    if num(ui, "Height", &mut l.height, 0.5, 50.0) {
                        label = "Level height"
                    }
                    ui.label(egui::RichText::new("Moving a level moves everything on it.").weak());
                    if ui
                        .button("Levels…")
                        .on_hover_text("Add, duplicate, rename and delete levels")
                        .clicked()
                    {
                        self.open_levels();
                    }
                    self.level_head_controls(ui, id);
                }
                ElementKind::Wall(w) => {
                    let mut curtain = w.curtain.is_some();
                    if crate::props::switch(ui, &mut curtain, "Curtain wall")
                    .on_hover_text("A grid of mullions holding glass or solid panels; doors in it take the place of panels")
                    .changed()
                {
                    w.curtain = curtain.then(|| creabim_core::CurtainGrid {
                        mullion_depth: w.thickness.clamp(0.05, 0.3),
                        ..Default::default()
                    });
                    if curtain {
                        w.assembly = None;
                    }
                    label = "Curtain wall"
                }
                    if let Some(c) = &mut w.curtain {
                        let hosts: Vec<String> =
                            creabim_core::curtain::hosts_of(self.doc.model(), id)
                                .iter()
                                .filter_map(|(h, _)| {
                                    self.doc.model().get(*h).map(|e| e.name.clone())
                                })
                                .collect();
                        if !hosts.is_empty() {
                            ui.label(
                            egui::RichText::new(format!(
                                "Set in {}: it cuts its own opening there, like a door, and the opening moves with it.",
                                hosts.join(", ")
                            ))
                            .weak(),
                        );
                        }
                        let l = typesui::curtain_fields(ui, c);
                        if !l.is_empty() {
                            label = l;
                        }
                        // a curtain wall is as thick as its mullions
                        w.thickness = c.mullion_depth;
                        if self.material_combo(ui, &mut w.material) {
                            label = "Wall material"
                        }
                    } else {
                        if self.assembly_combo(ui, creabim_core::AssemblyUse::Wall, &mut w.assembly)
                        {
                            label = "Wall assembly"
                        }
                        let layered = w.assembly.is_some_and(|a| {
                            matches!(
                                self.doc.model().get(a).map(|e| &e.kind),
                                Some(ElementKind::Assembly(_))
                            )
                        });
                        if layered {
                            ui.label(format!(
                                "Thickness {}, from its layers",
                                crate::units::len(w.thickness)
                            ));
                        } else if num(ui, "Thickness", &mut w.thickness, 0.02, 2.0) {
                            label = "Wall thickness"
                        }
                        if !layered && self.material_combo(ui, &mut w.material) {
                            label = "Wall material"
                        }
                    }
                    let l = wallsui::wall_fields(ui, self.doc.model(), id, w);
                    if !l.is_empty() {
                        label = l;
                    }
                    if crate::props::action(
                        ui,
                        "Path",
                        "Redraw",
                        "Click a new path for this wall, in a plan or the 3D view. More corners make more walls like it.",
                        true,
                    )
                    .clicked()
                    {
                        redraw = true;
                    }
                }
                ElementKind::Slab(s) => {
                    if self.assembly_combo(ui, creabim_core::AssemblyUse::Floor, &mut s.assembly) {
                        label = "Slab assembly"
                    }
                    let layered = s.assembly.is_some_and(|a| {
                        matches!(
                            self.doc.model().get(a).map(|e| &e.kind),
                            Some(ElementKind::Assembly(_))
                        )
                    });
                    let mut thickness = s.thickness;
                    if layered {
                        ui.label(format!(
                            "Thickness {}, from its layers",
                            crate::units::len(s.thickness)
                        ));
                    } else if num(ui, "Thickness", &mut thickness, 0.02, 3.0) {
                        s.set_thickness(thickness, self.plan.slab_keep_bottom);
                        label = "Slab thickness"
                    }
                    crate::props::choice(
                        ui,
                        "Keep",
                        &mut self.plan.slab_keep_bottom,
                        &[
                            (
                                false,
                                "Top",
                                "A new thickness moves the underside; the top stays put.",
                            ),
                            (
                                true,
                                "Bottom",
                                "A new thickness moves the top; the underside stays put.",
                            ),
                        ],
                    );
                    if num(ui, "Top offset", &mut s.top_offset, -100.0, 100.0) {
                        label = "Slab top offset"
                    }
                    let level = self.doc.model().level(s.level).map_or(0.0, |l| l.elevation);
                    crate::props::note(
                        ui,
                        format!(
                            "Top at {}, underside at {}.",
                            crate::units::elevation(s.top(level)),
                            crate::units::elevation(s.top(level) - s.thickness)
                        ),
                    );
                    if !layered && self.material_combo(ui, &mut s.material) {
                        label = "Slab material"
                    }
                    let arcs = s.bulges.iter().filter(|b| b.abs() > 1e-9).count();
                    let said = match arcs {
                        0 => format!(
                            "{} corners, {}",
                            s.boundary.len(),
                            crate::units::area(s.area())
                        ),
                        n => format!(
                            "{} corners, {n} arc{}, {}",
                            s.boundary.len(),
                            if n == 1 { "" } else { "s" },
                            crate::units::area(s.area())
                        ),
                    };
                    // as Blender: the operator in the field column, what it
                    // works on said under it (MU31)
                    if crate::props::action(
                        ui,
                        "Outline",
                        "Redraw",
                        "Click a new outline for this slab, in a plan or the 3D view.",
                        true,
                    )
                    .clicked()
                    {
                        redraw = true;
                    }
                    crate::props::note(ui, said);
                    let shafts = creabim_core::slab::holes_of(self.doc.model(), &e)
                        .len()
                        .saturating_sub(s.holes.len());
                    let (l, draw) = typesui::holes_fields(ui, &mut s.holes, shafts);
                    if !l.is_empty() {
                        label = l;
                    }
                    draw_hole |= draw;
                }
                ElementKind::Opening(o) => {
                    let old = o.width;
                    if num(ui, "Width", &mut o.width, 0.1, 20.0) {
                        // about its centre, or from the jamb by a corner
                        creabim_core::walls::widen_in_place(self.doc.model(), old, o);
                        label = "Opening width"
                    }
                    if num(ui, "Height", &mut o.height, 0.1, 20.0) {
                        label = "Opening height"
                    }
                    if num(ui, "Sill", &mut o.sill, 0.0, 20.0) {
                        label = "Opening sill"
                    }
                    if num(ui, "Offset along wall", &mut o.offset, 0.0, 1e4) {
                        label = "Move opening"
                    }
                    if self.material_combo(ui, &mut o.material) {
                        label = "Material";
                    }
                    let l = wallsui::opening_fields(ui, self.doc.model(), o);
                    if !l.is_empty() {
                        label = l;
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("Frame").strong());
                    if num(ui, "Frame width", &mut o.frame.width, 0.01, 0.5) {
                        label = "Frame width"
                    }
                    if num(ui, "Frame depth", &mut o.frame.depth, 0.02, 1.0) {
                        label = "Frame depth"
                    }
                    if o.kind == creabim_core::OpeningKind::Window {
                        let panes = |ui: &mut egui::Ui, text: &str, v: &mut u32| {
                            crate::props::row(ui, text, |ui| {
                                crate::props::fill(ui, egui::DragValue::new(v).range(1..=12))
                                    .changed()
                            })
                        };
                        if panes(ui, "Panes across", &mut o.frame.panes_across) {
                            label = "Window panes"
                        }
                        if panes(ui, "Panes up", &mut o.frame.panes_up) {
                            label = "Window panes"
                        }
                    }
                }
                ElementKind::View(v) if matches!(v.kind, ViewKind::Schedule { .. }) => {
                    if let ViewKind::Schedule { category, by_type } = &mut v.kind {
                        crate::app::helpui::combo(ui, "Lists")
                            .selected_text(category.label())
                            .show_ui(ui, |ui| {
                                for c in ScheduleCategory::ALL {
                                    if ui.selectable_value(category, c, c.label()).changed() {
                                        label = "Schedule category"
                                    }
                                }
                            });
                        let (what, hint, undo) = if *category == ScheduleCategory::Materials {
                            (
                                "Group by level",
                                "List each level's materials under a heading with its totals.",
                                "Group take-off by level",
                            )
                        } else {
                            (
                                "Group by type",
                                "List the rows under a heading per element type, with its count.",
                                "Group schedule by type",
                            )
                        };
                        if crate::props::switch(ui, by_type, what)
                            .on_hover_text(hint)
                            .changed()
                        {
                            label = undo
                        }
                    }
                    if self.active_view != Some(id) && ui.button("Open this schedule").clicked() {
                        self.active_view = Some(id);
                    }
                }
                ElementKind::View(v) => {
                    // on its sheets too, each viewport round its centre
                    if let Some(s) = sheetview::scale_picker(ui, id, v.scale) {
                        rescale = Some((id, s));
                    }
                    match &mut v.kind {
                        ViewKind::Plan { cut_height, level } => {
                            if num(ui, "Cut height", cut_height, 0.1, 50.0) {
                                label = "View cut height"
                            }
                            let choices = self.underlay_choices(*level);
                            let current = choices
                                .iter()
                                .find(|(u, _)| *u == v.underlay)
                                .map_or("None".to_string(), |(_, l)| l.clone());
                            crate::app::helpui::combo(ui, "Underlay")
                                .selected_text(current)
                                .show_ui(ui, |ui| {
                                    for (u, l) in &choices {
                                        if ui.selectable_value(&mut v.underlay, *u, l).changed() {
                                            label = "View underlay"
                                        }
                                    }
                                });
                            ui.label(
                            egui::RichText::new(
                                "Another level drawn faintly under this one, to trace and snap to. Not printed.",
                            )
                            .weak(),
                        );
                            if let Some(l) = plan_graphics_ui(ui, &mut v.graphics, v.scale, true) {
                                label = l;
                            }
                        }
                        ViewKind::Section {
                            start,
                            end,
                            depth,
                            cut,
                        } => {
                            if crate::props::switch(ui, cut, "Cut through the model (section)")
                                .changed()
                            {
                                label = "Section or elevation"
                            }
                            if num(ui, "Depth", depth, 0.1, 500.0) {
                                label = "View depth"
                            }
                            if num(ui, "Start X", &mut start.x, -1e4, 1e4)
                                | num(ui, "Start Y", &mut start.y, -1e4, 1e4)
                                | num(ui, "End X", &mut end.x, -1e4, 1e4)
                                | num(ui, "End Y", &mut end.y, -1e4, 1e4)
                            {
                                label = "Move section line"
                            }
                        }
                        ViewKind::Axonometric { azimuth, altitude } => {
                            let deg =
                                |ui: &mut egui::Ui, text: &str, v: &mut f64, lo: f64, hi: f64| {
                                    crate::props::row(ui, text, |ui| {
                                        crate::props::fill(
                                            ui,
                                            egui::DragValue::new(v)
                                                .speed(0.5)
                                                .range(lo..=hi)
                                                .suffix("°"),
                                        )
                                        .changed()
                                    })
                                };
                            if deg(ui, "Azimuth", azimuth, 0.0, 360.0)
                                | deg(ui, "Altitude", altitude, -89.0, 89.0)
                            {
                                label = "Orbit 3D view"
                            }
                            ui.label(
                                egui::RichText::new("Or drag in the view to orbit it.").weak(),
                            );
                        }
                        ViewKind::Schedule { .. } => {}
                    }
                    if matches!(
                        v.kind,
                        ViewKind::Plan { .. } | ViewKind::Section { cut: true, .. }
                    ) && crate::props::switch(ui, &mut v.room_colours, "Colour rooms")
                        .on_hover_text(
                            "Fill each room with a colour of its own, the same in every view",
                        )
                        .changed()
                    {
                        label = "Colour rooms"
                    }
                    if matches!(v.kind, ViewKind::Plan { .. })
                        && crate::props::switch(ui, &mut v.ceiling_plan, "Reflected ceiling plan")
                            .on_hover_text("Draw the ceilings and what hangs from them, not the floor and furniture")
                            .changed()
                    {
                        label = "Ceiling plan"
                    }
                    if matches!(
                        v.kind,
                        ViewKind::Section { .. } | ViewKind::Axonometric { .. }
                    ) && view_style_combo(ui, "view_style", &mut v.style)
                    {
                        label = "View style"
                    }
                    // how what is cut is drawn, as in plans (A4-11, MU20)
                    if matches!(v.kind, ViewKind::Section { .. }) {
                        crate::props::row(ui, "Drawing style", |ui| {
                            egui::ComboBox::from_id_salt("section.drawing_style")
                                .icon(crate::props::combo_icon)
                                .selected_text(v.graphics.drawing_style.label())
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    for s in creabim_core::graphics::DrawingStyle::ALL {
                                        if ui
                                            .selectable_value(
                                                &mut v.graphics.drawing_style,
                                                s,
                                                s.label(),
                                            )
                                            .on_hover_text(s.hint())
                                            .changed()
                                        {
                                            label = "Drawing style";
                                        }
                                    }
                                });
                        });
                    }
                    if matches!(v.kind, ViewKind::Axonometric { .. }) {
                        ui.add_space(6.0);
                        self.crop_list(ui, Some(id));
                    }
                }
                ElementKind::Sheet(sh) => {
                    // label column and field, as every other row (MU40:
                    // labels stacked over their fields read unlike Blender)
                    if crate::props::row(ui, "Number", |ui| {
                        crate::props::fill(ui, egui::TextEdit::singleline(&mut sh.number))
                    })
                    .changed()
                    {
                        label = "Sheet number"
                    }
                    let current = sh.size_name().unwrap_or("Custom");
                    crate::props::row(ui, "Paper", |ui| {
                        egui::ComboBox::from_id_salt(("sheet.paper", id))
                            .icon(crate::props::combo_icon)
                            .width(ui.available_width())
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (name, w, h) in Sheet::SIZES {
                                    if ui.selectable_label(current == *name, *name).clicked() {
                                        (sh.width, sh.height) = (*w, *h);
                                        label = "Sheet size"
                                    }
                                }
                            })
                    });
                    if let Some(l) = self.sheet_template_ui(ui, sh) {
                        label = l;
                    }
                    // its guides, to reuse on other sheets (MU27)
                    let n = sh.guides.len();
                    crate::props::row(ui, "Guides", |ui| {
                        ui.label(match n {
                            0 => "None".to_string(),
                            1 => "1 guide".to_string(),
                            n => format!("{n} guides"),
                        })
                    });
                    if n > 0 {
                        let (copy, clear) = crate::props::actions2(
                            ui,
                            "",
                            (
                                "Copy to all",
                                "Every other sheet gets these guides in place of its own: one layout across the set",
                                true,
                            ),
                            ("Clear", "Take this sheet's guides away", true),
                        );
                        if copy {
                            self.copy_guides = Some((id, sh.guides.clone()));
                        }
                        if clear {
                            sh.guides.clear();
                            label = "Clear guides";
                        }
                    }
                    if self.active_view != Some(id)
                        && crate::props::action(
                            ui,
                            "",
                            "Open this sheet",
                            "Show it in the sheet view",
                            true,
                        )
                        .clicked()
                    {
                        self.active_view = Some(id);
                    }
                }
                ElementKind::Viewport(vp) => {
                    let view_name = self
                        .doc
                        .model()
                        .get(vp.view)
                        .map(|e| e.name.clone())
                        .unwrap_or_default();
                    ui.label(format!("Shows {view_name}"));
                    // its scale is the view's: a new one redraws it at that
                    // size, round the viewport's centre
                    let view = vp.view;
                    let scale = self.view(view).and_then(|v| {
                        (!matches!(v.kind, ViewKind::Schedule { .. })).then_some(v.scale)
                    });
                    if let Some(scale) = scale {
                        if let Some(s) = sheetview::scale_picker(ui, id, scale) {
                            rescale = Some((view, s));
                        }
                    }
                    let mm = |ui: &mut egui::Ui, text: &str, v: &mut f64| {
                        crate::props::row(ui, text, |ui| {
                            crate::props::fill(ui, egui::DragValue::new(v).speed(1.0).suffix(" mm"))
                                .changed()
                        })
                    };
                    if mm(ui, "Centre X", &mut vp.center.x) | mm(ui, "Centre Y", &mut vp.center.y) {
                        label = "Move view on sheet"
                    }
                    let mut cropped = vp.crop.is_some();
                    if crate::props::switch(ui, &mut cropped, "Crop the view").changed() {
                        vp.crop = if cropped {
                            let view = vp.view;
                            self.drawing(view)
                                .and_then(|d| d.bounds())
                                .map(|(lo, hi)| [lo, hi])
                        } else {
                            None
                        };
                        label = "Crop view"
                    }
                    if let Some([lo, hi]) = &mut vp.crop {
                        if num(ui, "Crop left", &mut lo.x, -1e4, hi.x - 0.1)
                            | num(ui, "Crop right", &mut hi.x, lo.x + 0.1, 1e4)
                            | num(ui, "Crop bottom", &mut lo.y, -1e4, hi.y - 0.1)
                            | num(ui, "Crop top", &mut hi.y, lo.y + 0.1, 1e4)
                        {
                            label = "Crop view"
                        }
                    }
                    ui.label(
                    egui::RichText::new(
                        "Drag the view on the sheet to move it; double-click it to edit the model through it.",
                    )
                    .weak(),
                );
                    if ui.button("Open the view itself").clicked() {
                        self.active_view = Some(vp.view);
                    }
                    // its title on the sheet (MU27): its view's, so every
                    // sheet it is on agrees
                    if let Some(ElementKind::View(view)) =
                        self.doc.model().get(vp.view).map(|e| e.kind.clone())
                    {
                        let mut t = view.sheet_title.clone();
                        let before = t.clone();
                        if crate::props::subsection(ui, "viewport.title", "Title", true) {
                            let mut shown = !t.hidden;
                            if crate::props::switch(ui, &mut shown, "Show the title").changed() {
                                t.hidden = !shown;
                            }
                            if !t.hidden {
                                crate::props::switch(ui, &mut t.show_number, "Number in a circle");
                                crate::props::switch(ui, &mut t.show_scale, "Scale under it");
                                crate::props::row(ui, "Words", |ui| {
                                    crate::props::fill(
                                        ui,
                                        egui::TextEdit::singleline(&mut t.text)
                                            .vertical_align(egui::Align::Center)
                                            .hint_text("The view's name"),
                                    )
                                });
                                crate::props::row(ui, "Size", |ui| {
                                    crate::props::fill(
                                        ui,
                                        egui::DragValue::new(&mut t.size)
                                            .speed(0.1)
                                            .range(1.5..=10.0)
                                            .suffix(" mm"),
                                    )
                                });
                                // its font, weight and colour: a text style
                                let styles: Vec<(ElementId, String)> = self
                                    .doc
                                    .model()
                                    .iter()
                                    .filter(|e| matches!(e.kind, ElementKind::TextStyle(_)))
                                    .map(|e| (e.id, e.name.clone()))
                                    .collect();
                                let now = t
                                    .style
                                    .and_then(|s| styles.iter().find(|(i, _)| *i == s))
                                    .map_or("Plain".to_string(), |(_, n)| n.clone());
                                crate::props::row(ui, "Text style", |ui| {
                                    egui::ComboBox::from_id_salt(("title.style", vp.view))
                                        .icon(crate::props::combo_icon)
                                        .selected_text(now)
                                        .width(ui.available_width())
                                        .show_ui(ui, |ui| {
                                            if ui
                                                .selectable_label(t.style.is_none(), "Plain")
                                                .clicked()
                                            {
                                                t.style = None;
                                            }
                                            for (i, n) in &styles {
                                                if ui
                                                    .selectable_label(t.style == Some(*i), n)
                                                    .clicked()
                                                {
                                                    t.style = Some(*i);
                                                }
                                            }
                                        })
                                });
                                for (name, v) in
                                    [("Moved X", &mut t.offset.x), ("Moved Y", &mut t.offset.y)]
                                {
                                    crate::props::row(ui, name, |ui| {
                                        crate::props::fill(
                                            ui,
                                            egui::DragValue::new(v).speed(0.5).suffix(" mm"),
                                        )
                                    });
                                }
                                ui.label(
                                    egui::RichText::new("Or drag the title itself on the sheet.")
                                        .weak(),
                                );
                            }
                        }
                        if t != before {
                            let id = vp.view;
                            let r = self.doc.edit_merged(id, "View title", |e| {
                                if let ElementKind::View(v) = &mut e.kind {
                                    v.sheet_title = t.clone();
                                }
                            });
                            if let Err(e) = r {
                                self.status = format!("Couldn't do that: {e}");
                            }
                        }
                    }
                }
                ElementKind::Assembly(_) => {
                    self.assembly_properties(ui, id);
                }
                ElementKind::CropBox(c) => {
                    if let Some(l) = self.crop_properties(ui, id, c) {
                        label = l
                    }
                }
                ElementKind::ElementType(_) => {
                    self.type_properties(ui, id);
                }
                ElementKind::Shaft(s) => {
                    let levels: Vec<(ElementId, String)> = self
                        .doc
                        .model()
                        .levels()
                        .map(|(e, _)| (e.id, e.name.clone()))
                        .collect();
                    let name = |id: Option<ElementId>| {
                        id.and_then(|i| levels.iter().find(|l| l.0 == i))
                            .map_or("One storey up".to_string(), |l| l.1.clone())
                    };
                    crate::app::helpui::combo(ui, "Base level")
                        .selected_text(name(Some(s.level)))
                        .show_ui(ui, |ui| {
                            for (lid, n) in &levels {
                                if ui.selectable_value(&mut s.level, *lid, n).changed() {
                                    label = "Shaft base"
                                }
                            }
                        });
                    if num(ui, "Base offset", &mut s.base_offset, -100.0, 100.0) {
                        label = "Shaft base"
                    }
                    crate::app::helpui::combo(ui, "Top level")
                        .selected_text(name(s.top_level))
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(&mut s.top_level, None, "One storey up")
                                .changed()
                            {
                                label = "Shaft top"
                            }
                            for (lid, n) in &levels {
                                if ui
                                    .selectable_value(&mut s.top_level, Some(*lid), n)
                                    .changed()
                                {
                                    label = "Shaft top"
                                }
                            }
                        });
                    if num(ui, "Top offset", &mut s.top_offset, -100.0, 100.0) {
                        label = "Shaft top"
                    }
                    let (z0, z1) = s.span(self.doc.model());
                    let area = creabim_core::polygon::signed_area2(&s.boundary).abs() / 2.0;
                    ui.label(
                    egui::RichText::new(format!(
                        "From {} up to {}, {}. It cuts a hole in every slab and roof in between that it lies inside; move or reshape it (Edit) and they follow. Pick it in any view it passes through.",
                        crate::units::elevation(z0),
                        crate::units::elevation(z1),
                        crate::units::area(area)
                    ))
                    .weak(),
                );
                }
                ElementKind::Material(m) => {
                    materials::preview(ui, m, 72.0);
                    label = materials::material_fields(ui, m);
                    let users = creabim_core::library::users(self.doc.model(), id).len();
                    ui.label(egui::RichText::new(format!("Used by {users} elements.")).weak());
                    if ui.button("Open in the material browser").clicked() {
                        self.open_materials(Some(id));
                    }
                }
                ElementKind::Dimension(d) => {
                    let measured = self
                        .drawing(d.view)
                        .and_then(|dr| {
                            dr.items.into_iter().find_map(|i| match i {
                                PlanItem::Dimension { id: i, text, .. } if i == id => Some(text),
                                PlanItem::Annotation { id: i, item } if i == id => Some(item.text),
                                _ => None,
                            })
                        })
                        .unwrap_or_else(|| "-".into());
                    // metric dimension text carries no unit; say which here
                    let unit = if !matches!(
                        d.kind,
                        creabim_core::DimensionKind::Linear
                            | creabim_core::DimensionKind::ArcLength
                    ) || crate::units::get().is_imperial()
                    {
                        String::new()
                    } else {
                        format!(" {}", crate::units::symbol())
                    };
                    ui.label(format!("{}: {measured}{unit}", d.kind.label()));
                    let attached = [Some(d.a), Some(d.b), d.center]
                        .iter()
                        .flatten()
                        .filter(|p| p.host.is_some())
                        .count();
                    ui.label(
                        egui::RichText::new(match attached {
                            0 => "Not attached: it stays where it was drawn.".to_string(),
                            n => format!("Attached at {n} end(s): it follows what it measures."),
                        })
                        .weak(),
                    );
                    if d.kind == creabim_core::DimensionKind::Spot {
                        let mut pattern = d
                            .pattern
                            .clone()
                            .unwrap_or_else(|| creabim_core::annotation::SPOT_PATTERN.into());
                        crate::props::flow(ui, |ui| {
                            ui.label("Reads").on_hover_text(
                                "{elevation} in the project's units, {m} in metres (+0.45), {m3} to three places, {mm} in millimetres, {level} the level's name. For example: {m} FFL",
                            );
                            if ui.text_edit_singleline(&mut pattern).changed() {
                                d.pattern = Some(pattern.clone());
                                label = "Spot level text";
                            }
                        });
                    } else {
                        let what = crate::annotate::offset_label(d.kind);
                        if num(ui, what, &mut d.offset, -100.0, 100.0) {
                            label = "Move dimension line"
                        }
                    }
                    if d.kind == creabim_core::DimensionKind::Linear {
                        crate::app::helpui::combo(ui, "Measures")
                            .selected_text(d.axis.label())
                            .show_ui(ui, |ui| {
                                for a in creabim_core::DimAxis::ALL {
                                    if ui.selectable_value(&mut d.axis, a, a.label()).changed() {
                                        label = "Dimension direction"
                                    }
                                }
                            });
                    }
                    match crate::annotate::look_ui(ui, self.doc.model(), &mut d.style, &mut d.color)
                    {
                        crate::annotate::LookEdit::Changed(l) => label = l,
                        crate::annotate::LookEdit::NewStyle => new_style = true,
                        crate::annotate::LookEdit::None => {}
                    }
                    // its own number format over the style pack's (A5-2)
                    if d.kind != creabim_core::DimensionKind::Spot
                        && crate::props::subsection(ui, "dim.format", "Number format", false)
                    {
                        let mut own = d.format.is_some();
                        if crate::props::switch(ui, &mut own, "Own format")
                            .on_hover_text("This dimension writes its number its own way; off, it follows Dimensions in the style pack")
                            .changed()
                        {
                            d.format = own.then(Default::default);
                            label = "Dimension number format";
                        }
                        if let Some(f) = &mut d.format {
                            let was = *f;
                            let units = self.doc.model().units();
                            levelsui::kind_format(
                                ui,
                                f,
                                &units,
                                creabim_core::NumberKind::Dimensions,
                            );
                            if *f != was {
                                label = "Dimension number format";
                            }
                        }
                        if crate::props::action(
                            ui,
                            "",
                            "Number formats…",
                            "The style pack's formats for every kind",
                            true,
                        )
                        .clicked()
                        {
                            self.levels.units_kind = Some(creabim_core::NumberKind::Dimensions);
                            self.levels.units_open = true;
                        }
                    }
                }
                ElementKind::Tag(t) => {
                    let host = self
                        .doc
                        .model()
                        .get(t.host)
                        .map(|e| e.name.clone())
                        .unwrap_or_default();
                    ui.label(format!("Tags {host}"));
                    if crate::props::switch(ui, &mut t.material, "Reads the material")
                        .on_hover_text("A material tag: its material, or its layers one line each, outside to inside")
                        .changed()
                    {
                        label = "Tag reads";
                    }
                    if num(ui, "Offset X", &mut t.offset.x, -100.0, 100.0)
                        | num(ui, "Offset Y", &mut t.offset.y, -100.0, 100.0)
                    {
                        label = "Move tag"
                    }
                    if ui.button("Select the tagged element").clicked() {
                        self.selection.only(t.host);
                    }
                    if matches!(
                        self.doc.model().get(t.host).map(|e| &e.kind),
                        Some(ElementKind::Room(_))
                    ) {
                        let l = crate::roomtool::tag_fields(ui, &mut t.fields);
                        if !l.is_empty() {
                            label = l;
                        }
                    } else {
                        if let Some(l) = crate::annotate::leader_ui(
                            ui,
                            &mut t.leader_end,
                            "Style's (a plain line)",
                            &mut t.elbow,
                        ) {
                            label = l;
                        }
                        if t.end.is_some()
                            && ui
                                .button("Leader back to the centre")
                                .on_hover_text(
                                    "Its leader points at the middle of what it tags again",
                                )
                                .clicked()
                        {
                            t.end = None;
                            label = "Move leader";
                        }
                    }
                    match crate::annotate::look_ui(ui, self.doc.model(), &mut t.style, &mut t.color)
                    {
                        crate::annotate::LookEdit::Changed(l) => label = l,
                        crate::annotate::LookEdit::NewStyle => new_style = true,
                        crate::annotate::LookEdit::None => {}
                    }
                }
                ElementKind::Note(n) => {
                    // a leader's default reach: a metre at 1:100, 10 mm on a sheet
                    let unit = match self.view(n.view) {
                        Some(v) => v.scale / 100.0,
                        None => 10.0,
                    };
                    let mut pop = false;
                    if let Some(l) = crate::annotate::note_ui(
                        ui,
                        n,
                        &mut self.plan.anno.focus_text,
                        unit,
                        &mut pop,
                    ) {
                        label = l;
                    }
                    if pop {
                        self.note_editor = Some(id);
                    }
                    if n.leader.is_some() {
                        if let Some(l) = crate::annotate::leader_ui(
                            ui,
                            &mut n.leader_end,
                            "Style's (an arrow)",
                            &mut n.elbow,
                        ) {
                            label = l;
                        }
                    }
                    match crate::annotate::look_ui(ui, self.doc.model(), &mut n.style, &mut n.color)
                    {
                        crate::annotate::LookEdit::Changed(l) => label = l,
                        crate::annotate::LookEdit::NewStyle => new_style = true,
                        crate::annotate::LookEdit::None => {}
                    }
                    // where it sits, typed (MU26): mm on a sheet, lengths
                    // in a view
                    let on_sheet = self.is_sheet(n.view);
                    for (name, v) in [("X", &mut n.at.x), ("Y", &mut n.at.y)] {
                        crate::props::row(ui, name, |ui| {
                            let r = if on_sheet {
                                crate::props::fill(
                                    ui,
                                    egui::DragValue::new(v).speed(0.5).suffix(" mm"),
                                )
                            } else {
                                crate::props::fill(ui, crate::units::drag(v))
                            };
                            if r.changed() {
                                label = "Place note";
                            }
                        });
                    }
                }
                ElementKind::Units(_) => {
                    if ui.button("Project units…").clicked() {
                        self.levels.units_open = true;
                    }
                }
                ElementKind::Project(_) => {
                    if ui.button("Project information…").clicked() {
                        self.project_open = true;
                    }
                }
                ElementKind::ScriptObject(s) => {
                    if let Some(l) =
                        crate::scriptui::script_panel(ui, &mut self.scripts, &self.doc, id, s)
                    {
                        label = l;
                    }
                }
                ElementKind::Reference(r) => {
                    if let Some(l) = self.reference_fields(ui, id, r) {
                        label = l;
                    }
                }
                ElementKind::PaperMark(m) => {
                    let north = self
                        .doc
                        .model()
                        .project_info()
                        .map_or(0.0, |(_, p)| p.true_north);
                    if let Some(l) = paper_mark_fields(ui, m, north) {
                        label = l;
                    }
                }
                ElementKind::MarkRules(_) => {
                    if ui.button("Mark numbering…").clicked() {
                        self.marks.open = true;
                    }
                }
                ElementKind::StylePack(p) => {
                    ui.label(egui::RichText::new(p.description.clone()).weak());
                    if ui.button("Graphic styles…").clicked() {
                        self.gstyles.open = true;
                    }
                }
                ElementKind::TextStyle(s) => {
                    if let Some(l) = crate::annotate::style_ui(ui, s) {
                        label = l;
                    }
                    let users = crate::annotate::style_users(self.doc.model(), id);
                    ui.label(
                        egui::RichText::new(format!(
                            "Used by {users} dimensions, tags and notes. Rename it above."
                        ))
                        .weak(),
                    );
                    crate::props::flow(ui, |ui| {
                        if ui
                            .add_enabled(users > 0, egui::Button::new("Select all using it"))
                            .clicked()
                        {
                            self.select_style_users(id);
                        }
                        if ui.button("Annotation styles…").clicked() {
                            self.open_styles(Some(id));
                        }
                    });
                }
                ElementKind::SheetTemplate(_) => {
                    let users = sheets::sheets(self.doc.model())
                        .into_iter()
                        .filter(|(_, s)| s.template == Some(id))
                        .count();
                    ui.label(egui::RichText::new(format!("Used by {users} sheets.")).weak());
                    if ui.button("Sheet templates…").clicked() {
                        self.drafting.templates_open = true;
                    }
                }
                ElementKind::Grid(g) => {
                    crate::props::flow(ui, |ui| {
                        ui.label("Label");
                        if let Some((_, gg)) = creabim_core::grids::group_of(self.doc.model(), g) {
                            ui.label(egui::RichText::new(&gg.prefix).weak());
                        }
                        if ui.text_edit_singleline(&mut g.label).changed() {
                            label = "Grid label"
                        }
                    });
                    let l = self.grid_properties(ui, g);
                    if !l.is_empty() {
                        label = l;
                    }
                }
                ElementKind::GridGroup(g) => {
                    label = self.grid_group_properties(ui, id, g);
                }
                ElementKind::Sketch(sk) => {
                    match crate::sketch::properties(ui, sk) {
                        crate::sketch::Edit::Done(l) => label = l,
                        crate::sketch::Edit::Refused(why) => self.status = why,
                        crate::sketch::Edit::None => {}
                    }
                    ui.add_space(4.0);
                    if num(ui, "Plane X", &mut sk.plane.origin.x, -1e4, 1e4)
                        | num(ui, "Plane Y", &mut sk.plane.origin.y, -1e4, 1e4)
                        | num(ui, "Plane Z", &mut sk.plane.origin.z, -1e3, 1e3)
                    {
                        label = "Move sketch"
                    }
                    let n = sk.plane.normal();
                    crate::props::note(
                        ui,
                        format!("Its plane faces ({:.2}, {:.2}, {:.2}).", n.x, n.y, n.z),
                    );
                    crate::props::row(ui, "Extrude by", |ui| {
                        crate::props::fill(ui, crate::units::drag(&mut self.extrude_distance))
                    });
                    // a closed outline that crosses itself has no inside
                    let crossed =
                        sk.closed && creabim_core::polygon::self_intersects(&sk.outline(), true);
                    let extrude = crate::props::action(
                        ui,
                        "",
                        if sk.closed {
                            "Extrude into a mass (E)"
                        } else {
                            "Extrude into a surface (E)"
                        },
                        "",
                        !crossed,
                    )
                    .clicked();
                    crate::props::note(
                        ui,
                        if crossed {
                            "Closed, but it crosses itself: not a face. Move a point so no lines cross."
                        } else if sk.closed {
                            "Closed: a face. Extruding it makes a solid."
                        } else {
                            "Open: a wire. Extruding it makes a surface."
                        },
                    );
                    if extrude {
                        extrude_sketch = true;
                    }
                }
                ElementKind::FreeMesh(m) => {
                    crate::app::helpui::combo(ui, "IFC class")
                        .selected_text(m.ifc_class.clone())
                        .show_ui(ui, |ui| {
                            for c in FREE_MESH_CLASSES {
                                if ui
                                    .selectable_value(&mut m.ifc_class, c.to_string(), *c)
                                    .changed()
                                {
                                    label = "Classify mesh"
                                }
                            }
                        });
                    if self.material_combo(ui, &mut m.material) {
                        label = "Mesh material"
                    }
                    // Transform, as Blender's (MU24): location, rotation
                    // about each axis (XYZ Euler), scale
                    if crate::props::subsection(ui, "mesh.location", "Location", true)
                        && (num(ui, "X", &mut m.origin.x, -1e4, 1e4)
                            | num(ui, "Y", &mut m.origin.y, -1e4, 1e4)
                            | num(ui, "Z", &mut m.origin.z, -1e3, 1e3))
                    {
                        label = "Move mesh"
                    }
                    if crate::props::subsection(ui, "mesh.rotation", "Rotation (XYZ Euler)", true) {
                        let deg = |ui: &mut egui::Ui, name: &str, v: &mut f64| {
                            crate::props::row(ui, name, |ui| {
                                crate::props::fill(ui, egui::DragValue::new(v).speed(1.0).suffix("°"))
                                    .on_hover_text("Turns about this axis through its origin. R in a view turns it with the pointer; X, Y or Z then keeps to an axis.")
                                    .changed()
                            })
                        };
                        let (mut x, mut y, mut z) = (m.rot_x, m.rot_y, m.rotation);
                        if deg(ui, "X", &mut x) | deg(ui, "Y", &mut y) | deg(ui, "Z", &mut z) {
                            m.rot_x = x.rem_euclid(360.0);
                            m.rot_y = y.rem_euclid(360.0);
                            m.rotation = z.rem_euclid(360.0);
                            label = "Rotate mesh"
                        }
                    }
                    crate::props::row(ui, "Scale", |ui| {
                        if crate::props::fill(
                            ui,
                            egui::DragValue::new(&mut m.scale)
                                .speed(0.01)
                                .range(0.01..=100.0),
                        )
                        .on_hover_text("M moves by a base point. With the pointer over a view: G moves, R rotates, S scales, Shift+D duplicates.")
                        .changed()
                        {
                            label = "Scale mesh"
                        }
                    });

                    ui.label(
                        egui::RichText::new(format!(
                            "Modifiers: {} (the wrench tab)",
                            m.modifiers.len()
                        ))
                        .weak(),
                    );
                    ui.label(format!(
                        "{} vertices, {} faces",
                        m.mesh.vertices.len(),
                        m.mesh.faces.len()
                    ));
                }
                ElementKind::Cut(c) => {
                    let name = |id| {
                        self.doc
                            .model()
                            .get(id)
                            .map_or("(missing)".to_string(), |e| e.name.clone())
                    };
                    ui.label(format!(
                        "{} {} with {}",
                        c.op.label(),
                        name(c.target),
                        name(c.cutter)
                    ));
                    for op in BoolOp::ALL {
                        if ui.radio_value(&mut c.op, op, op.label()).changed() {
                            label = "Change boolean";
                        }
                    }
                }
                ElementKind::Room(r) => {
                    label = crate::roomtool::room_fields(ui, self.doc.model(), r);
                }
                ElementKind::Ceiling(c) => {
                    label = crate::roomtool::ceiling_fields(ui, self.doc.model(), c);
                    if self.material_combo(ui, &mut c.material) {
                        label = "Material";
                    }
                }
                ElementKind::Join(j) => {
                    label = crate::detailtool::join_fields(ui, self.doc.model(), j);
                }
                ElementKind::Detail(d) => {
                    label = crate::detailtool::detail_fields(ui, d);
                }
                ElementKind::RoomSeparator(sep) => {
                    ui.label(format!(
                        "{} long. Rooms on either side of it are measured apart.",
                        crate::units::len(sep.a.distance(sep.b))
                    ));
                }
                ElementKind::Object(o) => {
                    let hung = creabim_core::objects::hung(self.doc.model(), &e);
                    label = objectsui::object_fields(ui, o, hung);
                    object_type = true;
                }
                k @ (ElementKind::Column(_)
                | ElementKind::Beam(_)
                | ElementKind::Roof(_)
                | ElementKind::Stair(_)
                | ElementKind::Railing(_)
                | ElementKind::Footing(_)
                | ElementKind::Part(_)
                | ElementKind::Site(_)) => {
                    if let ElementKind::Roof(r) = k {
                        if self.assembly_combo(ui, creabim_core::AssemblyUse::Roof, &mut r.assembly)
                        {
                            label = "Roof assembly";
                        }
                    }
                    let layered = matches!(k, ElementKind::Roof(r) if r.assembly.is_some_and(|a| {
                        matches!(self.doc.model().get(a).map(|e| &e.kind), Some(ElementKind::Assembly(_)))
                    }));
                    let l = crate::memberstool::properties(ui, self.doc.model(), k);
                    if !l.is_empty() {
                        label = l;
                    }
                    let shafts = creabim_core::slab::holes_of(self.doc.model(), &e).len();
                    if let ElementKind::Roof(r) = k {
                        let shafts = shafts.saturating_sub(r.holes.len());
                        let (l, draw) = typesui::holes_fields(ui, &mut r.holes, shafts);
                        if !l.is_empty() {
                            label = l;
                        }
                        draw_hole |= draw;
                    }
                    // a void cuts, so it has no material to show
                    let void = matches!(k, ElementKind::Part(p) if p.void);
                    if let Some(m) =
                        creabim_core::members::material_mut(k).filter(|_| !layered && !void)
                    {
                        if self.material_combo(ui, m) {
                            label = "Material";
                        }
                    }
                    if matches!(k, ElementKind::Stair(_))
                        && crate::props::action(
                            ui,
                            "Railings",
                            "Add on both sides",
                            "A railing along each side; they follow the stair.",
                            true,
                        )
                        .clicked()
                    {
                        let mut sel = self.selection.clone();
                        let h = self.plan.members.railing_height;
                        let r =
                            crate::memberstool::add_stair_railings(&mut self.doc, id, h, &mut sel);
                        self.report(r, "Railings added along both sides.");
                    }
                }
                ElementKind::Guide(g) => {
                    ui.label(
                        egui::RichText::new(
                            "A construction line to snap to. It is never printed or exported.",
                        )
                        .weak(),
                    );
                    if num(ui, "From X", &mut g.a.x, -1e4, 1e4)
                        | num(ui, "From Y", &mut g.a.y, -1e4, 1e4)
                        | num(ui, "To X", &mut g.b.x, -1e4, 1e4)
                        | num(ui, "To Y", &mut g.b.y, -1e4, 1e4)
                    {
                        label = "Move guide";
                    }
                }
            }
        }
        if new_style {
            match crate::annotate::new_text_style(&mut self.doc) {
                Ok(sid) => {
                    crate::annotate::set_style(&mut edited.kind, sid);
                    label = "Text style";
                }
                Err(e) => self.status = format!("Couldn't do that: {e}"),
            }
        }
        if !label.is_empty() {
            // a layered element is always as thick as its layers
            let slot = creabim_core::assembly::assembly_slot(&edited).flatten();
            if slot.is_some() {
                creabim_core::assembly::set_assembly(self.doc.model(), &mut edited, slot);
            }
            // with several selected, the change goes to each of the same kind
            let mut others = vec![];
            for o in self.selection.all() {
                let Some(other) = self.doc.model().get(o) else {
                    continue;
                };
                if let Some(mut out) = crate::multiedit::apply_to(&e, &edited, other) {
                    if let (ElementKind::Opening(was), ElementKind::Opening(o)) =
                        (&other.kind, &mut out.kind)
                    {
                        creabim_core::walls::widen_in_place(self.doc.model(), was.width, o);
                    }
                    let slot = creabim_core::assembly::assembly_slot(&out).flatten();
                    if slot.is_some() {
                        creabim_core::assembly::set_assembly(self.doc.model(), &mut out, slot);
                    }
                    others.push((other.clone(), out));
                }
            }
            let mut msg = if others.is_empty() {
                format!("{label}.")
            } else {
                format!("{label}: {} elements.", others.len() + 1)
            };
            // a layer the new type hasn't got loses its edited ends
            let edits = |k: &ElementKind| match k {
                ElementKind::Wall(w) => w.layer_ends.len(),
                ElementKind::Slab(s) => s.layer_edges.len(),
                ElementKind::Roof(r) => r.layer_edges.len(),
                _ => 0,
            };
            let lost = edits(&e.kind).saturating_sub(edits(&edited.kind));
            if lost > 0 {
                let what = match (&edited.kind, lost) {
                    (ElementKind::Wall(_), 1) => "edited layer end",
                    (ElementKind::Wall(_), _) => "edited layer ends",
                    (_, 1) => "edited layer edge",
                    _ => "edited layer edges",
                };
                msg = format!("{msg} {lost} {what} dropped: the new type has no such layer.");
            }
            if let ElementKind::Opening(o) = &edited.kind {
                let model = self.doc.model();
                if let Some(w) = creabim_core::walls::opening_crosses_join(model, id, o) {
                    let name = model.get(w).map_or("another wall", |e| e.name.as_str());
                    msg = format!(
                        "{msg} It now runs into {name}, where the walls join: move it along or make it narrower."
                    );
                }
            }
            let r = if others.is_empty() {
                self.doc.edit_merged(id, label, |e| *e = edited.clone())
            } else {
                let mut tx = creabim_core::Transaction::new(label);
                if let Some(cur) = self.doc.model().get(id).cloned() {
                    if cur != edited {
                        tx = tx.replace(cur, edited.clone());
                    }
                }
                for (b, a) in others {
                    tx = tx.replace(b, a);
                }
                self.doc.commit_merged(tx)
            };
            self.report(r, msg);
        }
        if object_type {
            self.object_type_section(ui, id);
        }
        if let Some((view, scale)) = rescale {
            self.set_view_scale(view, scale);
        }
        if new_style {
            self.status = "New text style: set its font, size and colour below.".into();
        }
        if let Some(msg) = crate::annotate::style_inline(ui, &mut self.doc, &edited.kind, new_style)
        {
            self.status = msg;
        }
        // a mass's face tools in a section of their own (MU39: the element
        // section had grown dense)
        self.mesh_edit_section(ui, id);
        // a sweep's profile from the library, under its settings (MU40:
        // settings always first)
        let own_profile = self.sweep_profile_rows(ui, id);
        // what can be made from it, in a section of its own (MU40: it hung
        // glued under Settings): a profile of its shape (a sweep's Profile
        // section has Save as new for that), even parts of its length
        let make =
            !own_profile && creabim_core::profiles::from_shape(self.doc.model(), id).is_some();
        let divide = creabim_core::divide::span(self.doc.model(), id).is_some();
        if (make || divide) && self.prop_header(ui, "make", "Make from it", true, None) {
            if make
                && crate::props::action(ui, "Profile", "Make a profile", "Its shape (a sketch on any plane, open or closed; a slab; an extrusion) kept in the project's profile library, to sweep or place", true).clicked()
            {
                self.profile_from_selection();
            }
            // even parts of its length (MU34 B2)
            if divide
                && crate::props::action(
                    ui,
                    "Divide",
                    "Into even parts…",
                    "Cut it into parts, put guides at the divisions, or copy something into each part",
                    true,
                )
                .clicked()
            {
                self.open_divide();
            }
        }
        if extrude_sketch {
            self.extrude_sketch();
            return;
        }
        let _ = show_visibility;
        if redraw {
            self.plan.cancel();
            self.plan.tool = Tool::Select;
            self.plan.outline = crate::outline::Outline::start(self.doc.model(), id);
            if let Some(o) = &self.plan.outline {
                self.status = o.hint();
            }
        }
        if draw_hole {
            self.plan.cancel();
            self.plan.tool = Tool::Select;
            self.plan.outline = crate::outline::Outline::hole(self.doc.model(), id);
            if let Some(o) = &self.plan.outline {
                self.status = o.hint();
            }
        }
    }

    /// The top of Properties, pinned while its tabs scroll (MU19/MU20):
    /// what the selected element is, its name and mark, and its actions.
    fn properties_header(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selection.active else {
            // the open tab's name, as Blender's context line
            crate::props::flow(ui, |ui| {
                let p = crate::theme::Palette::of(ui.ctx());
                let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 22.0), egui::Sense::hover());
                crate::svgicon::paint(
                    ui.painter(),
                    egui::Rect::from_center_size(r.center(), egui::vec2(15.0, 15.0)),
                    self.prop_tab.icon(),
                    p.muted,
                );
                ui.add(
                    egui::Label::new(egui::RichText::new(self.prop_tab.label()).strong())
                        .truncate(),
                );
            });
            ui.add_space(2.0);
            return;
        };
        let Some(e) = self.doc.model().get(id).cloned() else {
            return;
        };
        let mut multi: Option<String> = None;
        if self.selection.len() > 1 {
            // how many of each kind, then the active element below
            let mut kinds: Vec<(&str, usize)> = Vec::new();
            for e in self
                .selection
                .all()
                .iter()
                .filter_map(|i| self.doc.model().get(*i))
            {
                let k = kind_word(&e.kind);
                match kinds.iter_mut().find(|(l, _)| *l == k) {
                    Some((_, n)) => *n += 1,
                    None => kinds.push((k, 1)),
                }
            }
            let list: Vec<String> = kinds
                .iter()
                .map(|(k, n)| format!("{n} {}", plural(k, *n)))
                .collect();
            // one quiet line under the header rows, its help on hover
            // (MU36: a block above the name)
            multi = Some(format!(
                "{} selected: {}",
                self.selection.len(),
                list.join(", ")
            ));
        }
        // an object is what its type says: a basin an IfcSanitaryTerminal
        let class = creabim_core::objects::type_of(self.doc.model(), &e)
            .map_or_else(|| e.ifc_class().to_string(), |t| t.ifc.class.clone());
        // labelled rows, as every field (MU22): its name, its IFC class and
        // the guide's page on it, its mark; then its actions
        let pal = crate::theme::Palette::of(ui.ctx());
        if matches!(e.kind, ElementKind::Level(_)) {
            // a level is renamed with the plans named after it, once typed
            self.level_name_field(ui, id, &e.name);
        } else {
            let mut name = e.name.clone();
            if crate::props::head_row(ui, "Name", |ui| {
                crate::props::fill(
                    ui,
                    egui::TextEdit::singleline(&mut name).vertical_align(egui::Align::Center),
                )
                .changed()
            }) {
                let r = self
                    .doc
                    .edit_merged(id, "Rename", |e| e.name = name.clone());
                self.report(r, "Renamed.");
            }
        }
        crate::props::head_row(ui, "IFC class", |ui| {
            // the ? at the row's end, the class cut to what is left (MU36:
            // the ? fell to the next line)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let page = crate::help::element_page(&e.kind);
                let title = crate::help::text(page.split('#').next().unwrap_or(page))
                    .map(crate::help::title)
                    .unwrap_or("This element");
                self.quick_help_button(ui, page, title);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(class.clone()).color(pal.accent))
                            .truncate(),
                    )
                    .on_hover_text(class.clone());
                });
            })
        });
        if let Some(m) = multi.take() {
            ui.add(egui::Label::new(egui::RichText::new(m).weak().small()).truncate())
                .on_hover_text("G, R, S, Shift+D, Delete, Ctrl+C and H act on all of them. Below is the last one picked; Shift+click picks another.");
        }
        if marksui::markable(&e.kind) {
            crate::props::head_row(ui, "Mark", |ui| {
                let w = ui.available_width();
                self.mark_inline(ui, id, w);
            });
        }
        self.mark_warning(ui, id);
        ui.add_space(2.0);
        // its actions, under its name (brief 02, 2.7)
        self.action_bar(ui);
        ui.add_space(2.0);
        let r = ui.available_rect_before_wrap();
        ui.painter()
            .hline(r.x_range(), r.top(), egui::Stroke::new(1.0, pal.border));
        ui.add_space(3.0);
    }

    /// The tabs Properties shows now, in order (MU19).
    fn prop_tabs(&self) -> Vec<PropTab> {
        let mut out = vec![];
        match self
            .selection
            .active
            .and_then(|id| self.doc.model().get(id))
        {
            Some(_) => {
                out.extend([
                    PropTab::Element,
                    PropTab::Joins,
                    PropTab::Graphics,
                    PropTab::Data,
                ]);
            }
            None => {
                if self.plan.tool != Tool::Select {
                    out.push(PropTab::Next);
                }
            }
        }
        // the Profile view in front: its profile, first of the view's tabs
        if self.profiles_in_front() {
            out.push(PropTab::Profile);
        }
        if self.masters_in_front() {
            out.push(PropTab::Master);
        }
        if matches!(
            self.areas.root.areas().get(self.areas.focus),
            Some(crate::areas::Shows::Log)
        ) {
            out.push(PropTab::Log);
        }
        if matches!(self.vis_target(false), Some(gridvis::VisTarget::View(_))) {
            out.push(PropTab::View);
            if self.selection.active.is_none() {
                out.push(PropTab::Trace);
            }
        }
        if self.selection.active.is_none() {
            out.push(PropTab::Model3d);
        }
        if let Some(ElementKind::FreeMesh(_)) = self
            .selection
            .active
            .and_then(|id| self.doc.model().get(id))
            .map(|e| &e.kind)
        {
            if let Some(at) = out.iter().position(|t| *t == PropTab::Element) {
                out.insert(at + 1, PropTab::Modifiers);
            }
        }
        out.push(PropTab::Project);
        out.push(PropTab::Notes);
        out
    }

    /// The Project tab: what the whole project holds (MU19).
    fn project_props(&mut self, ui: &mut egui::Ui) {
        crate::props::section(ui, "Project");
        let info = self
            .doc
            .model()
            .project_info()
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        let name = if info.name.trim().is_empty() {
            "No name yet".to_string()
        } else {
            info.name.clone()
        };
        crate::props::row(ui, "Name", |ui| ui.label(name));
        if !info.client.trim().is_empty() {
            crate::props::row(ui, "Client", |ui| ui.label(info.client.clone()));
        }
        if ui.button("Project information…").clicked() {
            self.project_open = true;
        }
        ui.add_space(6.0);
        crate::props::flow(ui, |ui| {
            if ui.button("Levels…").clicked() {
                self.open_levels();
            }
            if ui.button("Materials…").clicked() {
                self.open_materials(None);
            }
        });
    }

    /// The strip of tab icons at Properties' left (MU19).
    fn prop_strip(&mut self, ui: &mut egui::Ui, tabs: &[PropTab], height: f32) {
        // as Blender's (MU22): a column a tone darker than the panel, flush
        // at its edge; 26 px cells with 16 px icons; the open tab painted
        // in the panel's own colour so it reads as one piece with what it
        // shows, with a thin accent bar at its left
        let p = crate::theme::Palette::of(ui.ctx());
        let panel = ui.visuals().panel_fill;
        let column = {
            let (to, t) = if p.dark { (0.0, 0.25) } else { (0.0, 0.06) };
            let m = |c: u8| (c as f32 + (to - c as f32) * t).round() as u8;
            Color32::from_rgb(m(panel.r()), m(panel.g()), m(panel.b()))
        };
        const W: f32 = 30.0;
        const CELL: f32 = 26.0;
        let top = ui.cursor().min;
        // the column runs from the panel's very top, so its edge meets the
        // window's line cleanly (MU27: a stub of panel showed above it)
        let from = ui.clip_rect().top().min(top.y);
        let strip = egui::Rect::from_min_max(
            egui::pos2(top.x, from),
            egui::pos2(top.x + W, top.y + height),
        );
        ui.painter().rect_filled(strip, 0.0, column);
        ui.allocate_ui_with_layout(
            egui::vec2(W, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add_space(4.0);
                let mut last_group = None;
                for t in tabs {
                    let group = t.group();
                    if last_group.is_some_and(|g| g != group) {
                        // a gap and a hairline between groups, as Blender's
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(W, 9.0), egui::Sense::hover());
                        ui.painter().line_segment(
                            [
                                egui::pos2(r.left() + 7.0, r.center().y),
                                egui::pos2(r.right() - 7.0, r.center().y),
                            ],
                            egui::Stroke::new(1.0, p.border),
                        );
                    }
                    last_group = Some(group);
                    let (row, resp) =
                        ui.allocate_exact_size(egui::vec2(W, CELL), egui::Sense::click());
                    let r =
                        egui::Rect::from_min_max(egui::pos2(row.left() + 2.0, row.top()), row.max);
                    let on = *t == self.prop_tab;
                    if on {
                        ui.painter().rect_filled(
                            r,
                            egui::CornerRadius {
                                nw: 4,
                                sw: 4,
                                ne: 0,
                                se: 0,
                            },
                            panel,
                        );
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                r.min + egui::vec2(0.0, 4.0),
                                egui::vec2(2.0, CELL - 8.0),
                            ),
                            1.0,
                            p.accent,
                        );
                    } else if resp.hovered() {
                        ui.painter()
                            .rect_filled(r.shrink(1.0), 4.0, p.accent.gamma_multiply(0.12));
                    }
                    let ink = if on { p.accent } else { p.muted };
                    let icon = egui::Rect::from_center_size(r.center(), egui::vec2(16.0, 16.0));
                    crate::svgicon::paint(ui.painter(), icon, t.icon(), ink);
                    if resp.on_hover_text(t.label()).clicked() {
                        self.prop_tab = *t;
                        // folded, a tab opens it again on that tab (MU27)
                        if self.shell.settings.right_folded {
                            self.shell.settings.right_folded = false;
                            self.shell.settings.store();
                        }
                    }
                }
            },
        );
    }
}

/// Which sheets the Export sheets dialog writes (MU22).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExportScope {
    #[default]
    All,
    Chosen,
    This,
}

/// The Export sheets dialog's choices, kept for the session.
#[derive(Default)]
pub(crate) struct SheetsExport {
    scope: ExportScope,
    /// A PDF a sheet, not one for all.
    each: bool,
    chosen: std::collections::BTreeSet<ElementId>,
}

/// Width and height rows for a sheet picture, its proportions kept when
/// `aspect` (height over width) is given. True when changed.
fn sheet_size_rows(ui: &mut egui::Ui, w: &mut f64, h: &mut f64, aspect: Option<f64>) -> bool {
    let (mut nw, mut nh) = (*w, *h);
    let cw = crate::props::row(ui, "Width", |ui| {
        crate::props::fill(
            ui,
            egui::DragValue::new(&mut nw)
                .range(2.0..=2000.0)
                .speed(0.5)
                .suffix(" mm"),
        )
    })
    .changed();
    let ch = crate::props::row(ui, "Height", |ui| {
        crate::props::fill(
            ui,
            egui::DragValue::new(&mut nh)
                .range(2.0..=2000.0)
                .speed(0.5)
                .suffix(" mm"),
        )
    })
    .changed();
    match (cw, ch, aspect) {
        (true, _, Some(a)) => {
            *w = nw;
            *h = nw * a;
        }
        (false, true, Some(a)) => {
            *h = nh;
            *w = nh / a.max(1e-9);
        }
        (true, _, None) | (_, true, None) => {
            *w = nw;
            *h = nh;
        }
        _ => return false,
    }
    true
}

/// A north arrow's or a scale bar's settings (MU22). The undo label of a
/// change.
fn paper_mark_fields(
    ui: &mut egui::Ui,
    m: &mut creabim_core::PaperMark,
    true_north: f64,
) -> Option<&'static str> {
    use creabim_core::PaperMarkKind;
    let mut label = None;
    let color = &mut m.color;
    match &mut m.kind {
        PaperMarkKind::Cloud {
            w,
            h,
            rev,
            date,
            what,
            points,
            arc,
        } => {
            // its shape: a box sized by its grips, or points of its own
            // (MU25)
            let mut free = points.len() >= 3;
            crate::props::row(ui, "Shape", |ui| {
                crate::props::flow(ui, |ui| {
                    if ui
                        .selectable_label(!free, "Box")
                        .on_hover_text("Drag its corner and edge grips to size it")
                        .clicked()
                        && free
                    {
                        let (lo, hi) = points.iter().fold(
                            (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)),
                            |(a, b), p| (a.min(*p), b.max(*p)),
                        );
                        *w = (hi.x - lo.x).max(5.0);
                        *h = (hi.y - lo.y).max(5.0);
                        points.clear();
                        free = false;
                        label = Some("Revision cloud shape");
                    }
                    if ui
                        .selectable_label(free, "Freeform")
                        .on_hover_text("Its own points: drag one to move it, drag a + to add one, right-click one to remove it")
                        .clicked()
                        && !free
                    {
                        let (x, y) = (*w / 2.0, *h / 2.0);
                        *points = vec![DVec2::new(-x, -y), DVec2::new(x, -y), DVec2::new(x, y), DVec2::new(-x, y)];
                        free = true;
                        label = Some("Revision cloud shape");
                    }
                });
            });
            crate::props::row(ui, "Scallops", |ui| {
                if crate::props::fill(
                    ui,
                    egui::DragValue::new(arc)
                        .speed(0.1)
                        .range(2.0..=30.0)
                        .suffix(" mm"),
                )
                .on_hover_text("How long each arc is along the outline")
                .changed()
                {
                    label = Some("Revision cloud");
                }
            });
            crate::props::row(ui, "Colour", |ui| {
                crate::props::flow(ui, |ui| {
                    let mut own = color.is_some();
                    if ui
                        .checkbox(&mut own, "Own")
                        .on_hover_text("Black without one")
                        .changed()
                    {
                        *color = own.then_some([0.85, 0.1, 0.1]);
                        label = Some("Revision cloud colour");
                    }
                    if let Some(c) = color {
                        if ui.color_edit_button_rgb(c).changed() {
                            label = Some("Revision cloud colour");
                        }
                    }
                })
            });
            let text = |ui: &mut egui::Ui, name: &str, v: &mut String| {
                crate::props::row(ui, name, |ui| {
                    crate::props::fill(
                        ui,
                        egui::TextEdit::singleline(v).vertical_align(egui::Align::Center),
                    )
                    .changed()
                })
            };
            if text(ui, "Revision", rev) | text(ui, "Date", date) | text(ui, "What changed", what) {
                label = Some("Revision cloud");
            }
            if !free {
                for (name, v) in [("Width", w), ("Height", h)] {
                    crate::props::row(ui, name, |ui| {
                        if crate::props::fill(
                            ui,
                            egui::DragValue::new(v)
                                .speed(0.5)
                                .range(5.0..=800.0)
                                .suffix(" mm"),
                        )
                        .changed()
                        {
                            label = Some("Revision cloud");
                        }
                    });
                }
            } else {
                ui.label(egui::RichText::new(format!("{} points: drag one to move it, drag a + to add one, right-click one to remove it.", points.len())).weak());
            }
        }
        PaperMarkKind::Picture {
            w,
            h,
            width,
            height,
            opacity,
            ..
        } => {
            let aspect = *height as f64 / (*width).max(1) as f64;
            if sheet_size_rows(ui, w, h, Some(aspect)) {
                label = Some("Picture size");
            }
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
            crate::props::row(ui, "Pixels", |ui| ui.label(format!("{width} × {height}")));
            if crate::props::action(
                ui,
                "",
                "Its own proportions",
                "Undo any stretching: its height from its width",
                ((*h / *w) - aspect).abs() > 1e-3,
            )
            .clicked()
            {
                *h = *w * aspect;
                label = Some("Picture proportions");
            }
            crate::props::note(ui, "Drag its grips to size it: the corners keep its proportions, the edges stretch it.");
        }
        PaperMarkKind::Vector { w, h, shapes } => {
            let aspect = *h / w.max(1e-9);
            if sheet_size_rows(ui, w, h, Some(aspect)) {
                label = Some("Picture size");
            }
            crate::props::row(ui, "Shapes", |ui| ui.label(format!("{}", shapes.len())));
            let mut one = match shapes.first() {
                Some(creabim_core::titleblock::TitleItem::Shape { fill: Some(c), .. })
                | Some(creabim_core::titleblock::TitleItem::Shape { colour: c, .. }) => *c,
                _ => [0.0; 3],
            };
            if crate::props::row(ui, "One colour", |ui| ui.color_edit_button_rgb(&mut one))
                .changed()
            {
                for s in shapes.iter_mut() {
                    if let creabim_core::titleblock::TitleItem::Shape { colour, fill, .. } = s {
                        *colour = one;
                        if fill.is_some() {
                            *fill = Some(one);
                        }
                    }
                }
                label = Some("Picture colour");
            }
            crate::props::note(ui, "From an SVG: crisp at any size, on screen and in PDF.");
        }
        PaperMarkKind::RevisionTable => {
            crate::props::row(ui, "Text", |ui| {
                if crate::props::fill(
                    ui,
                    egui::DragValue::new(&mut m.size)
                        .speed(0.1)
                        .range(1.5..=6.0)
                        .suffix(" mm"),
                )
                .changed()
                {
                    label = Some("Revisions table");
                }
            });
            ui.label(egui::RichText::new("A row for each revision cloud on this sheet.").weak());
        }
        PaperMarkKind::NorthArrow { rotation } => {
            let mut own = rotation.is_some();
            if crate::props::check(ui, "Points to", &mut own, "An angle of its own")
                .on_hover_text(format!(
                    "Off, it follows the project's true north ({true_north:.1}°, Project information)"
                ))
                .changed()
            {
                *rotation = own.then_some(true_north);
                label = Some("North arrow");
            }
            if let Some(r) = rotation {
                crate::props::row(ui, "Angle", |ui| {
                    if crate::props::fill(ui, egui::DragValue::new(r).speed(0.5).suffix("°"))
                        .changed()
                    {
                        label = Some("North arrow");
                    }
                });
            }
            crate::props::row(ui, "Height", |ui| {
                if crate::props::fill(
                    ui,
                    egui::DragValue::new(&mut m.size)
                        .speed(0.2)
                        .range(6.0..=60.0)
                        .suffix(" mm"),
                )
                .changed()
                {
                    label = Some("North arrow");
                }
            });
        }
        PaperMarkKind::ScaleBar {
            scale,
            length,
            parts,
        } => {
            crate::props::row(ui, "Scale", |ui| {
                crate::props::flow(ui, |ui| {
                    ui.label("1 :");
                    if crate::props::fill(
                        ui,
                        egui::DragValue::new(scale).speed(1.0).range(1.0..=10000.0),
                    )
                    .changed()
                    {
                        label = Some("Scale bar");
                    }
                })
            });
            crate::props::row(ui, "Length", |ui| {
                if crate::props::fill(
                    ui,
                    egui::DragValue::new(length)
                        .speed(0.1)
                        .range(0.1..=10000.0)
                        .suffix(" m"),
                )
                .changed()
                {
                    label = Some("Scale bar");
                }
            });
            crate::props::row(ui, "Parts", |ui| {
                if crate::props::fill(ui, egui::DragValue::new(parts).range(1..=10)).changed() {
                    label = Some("Scale bar");
                }
            });
            crate::props::row(ui, "Thickness", |ui| {
                if crate::props::fill(
                    ui,
                    egui::DragValue::new(&mut m.size)
                        .speed(0.1)
                        .range(0.5..=10.0)
                        .suffix(" mm"),
                )
                .changed()
                {
                    label = Some("Scale bar");
                }
            });
        }
    }
    // typed, to the millimetre, so marks line up sheet to sheet (MU25)
    for (name, v, tip) in [
        ("X", &mut m.at.x, "Millimetres from the sheet's left edge"),
        ("Y", &mut m.at.y, "Millimetres from the sheet's bottom edge"),
    ] {
        crate::props::row(ui, name, |ui| {
            if crate::props::fill(
                ui,
                egui::DragValue::new(v)
                    .speed(0.5)
                    .range(-2000.0..=2000.0)
                    .suffix(" mm"),
            )
            .on_hover_text(tip)
            .changed()
            {
                label = Some("Place paper mark");
            }
        });
    }
    label
}

/// The tabs of Properties (MU19): what a tab holds is its sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropTab {
    /// The next element a placing tool makes.
    Next,
    /// What it is: its type, sizes and build.
    Element,
    /// Its junctions and booleans.
    Joins,
    /// How views show it: overrides, visibility.
    Graphics,
    /// IFC class and data.
    Data,
    /// The view in the focused area.
    View,
    /// What is traced under the view.
    Trace,
    /// The 3D view: what it shows, crop boxes.
    Model3d,
    /// The project: information, levels, materials.
    Project,
    /// The project's notepad: notes and to-dos (MU26).
    Notes,
    /// The element's modifiers (MU27): array, mirror, solidify, subdivide.
    Modifiers,
    /// The profile in the Profile view in front (MU40).
    Profile,
    /// The master in the Masters view in front (MU40).
    Master,
    /// The Log view in front: its filter and what is in it (MU40).
    Log,
}

impl PropTab {
    /// The strip's groups, a hairline apart: what is selected, the view,
    /// the whole project.
    fn group(self) -> u8 {
        match self {
            PropTab::Next
            | PropTab::Element
            | PropTab::Joins
            | PropTab::Graphics
            | PropTab::Data => 0,
            PropTab::View
            | PropTab::Trace
            | PropTab::Model3d
            | PropTab::Profile
            | PropTab::Master
            | PropTab::Log => 1,
            PropTab::Project | PropTab::Notes => 2,
            PropTab::Modifiers => 0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            PropTab::Next => "Next element",
            PropTab::Element => "Element",
            PropTab::Joins => "Junctions and booleans",
            PropTab::Graphics => "Graphics",
            PropTab::Data => "Data (IFC)",
            PropTab::View => "View",
            PropTab::Trace => "Trace and reference",
            PropTab::Model3d => "3D view",
            PropTab::Project => "Project",
            PropTab::Notes => "Notes and to-dos",
            PropTab::Modifiers => "Modifiers",
            PropTab::Profile => "Profile",
            PropTab::Master => "Master",
            PropTab::Log => "Log",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            PropTab::Next => "plus",
            PropTab::Element => "box",
            PropTab::Joins => "join",
            PropTab::Graphics => "palette",
            PropTab::Data => "table-2",
            PropTab::View => "eye",
            PropTab::Trace => "underlay",
            PropTab::Model3d => "view-3d",
            PropTab::Project => "house",
            PropTab::Notes => "sticky-note",
            PropTab::Modifiers => "wrench",
            PropTab::Profile => "profile",
            PropTab::Master => "sheet-template",
            PropTab::Log => "log",
        }
    }
}

/// What a modifier does and how to use it: the ? on its card (MU39).
fn modifier_help(k: &creabim_core::ModifierKind) -> &'static str {
    use creabim_core::ModifierKind as K;
    match k {
        K::Array { .. } => "Repeats the shape in a row: bays, posts, rows of seats. Fit a number of copies or as many as fit a length; step by a distance or by its own size (1 puts copies end to end). Each copy can turn or grow a little more than the last.",
        K::RadialArray { .. } => "Repeats the shape round a centre: columns round a rotunda, seats round a table. Set the centre with its X and Y, or Pick and click it in a view. Turn each copy to face the centre, or keep them all facing one way.",
        K::PathArray { .. } => "Copies the shape to evenly spaced points along a sketch or a guide, its own origin on each: balusters, posts, bollards along a curve. The path is copied in: change it and pick it again.",
        K::Mirror { .. } => "A mirrored copy across a plane: at its middle for a symmetric shape, at its edge for a twin.",
        K::Panelize { .. } => "Cuts its upright rectangular faces into cladding panels with joints between, as near the size asked as fits evenly. Proud by sets the panels out from the face.",
        K::Louvres { .. } => "Fins or slats on the sides you pick, for shade. N is the mesh's +Y side before it is turned. Angle tilts each blade.",
        K::Bevel { .. } => "Rounds (fillet) or cuts (chamfer) the upright corners of a shape drawn from a plan outline and extruded: a slab, a counter, a mass. Outside corners, inside corners or both.",
        K::Shell { .. } => "Hollows it to a thickness. Closed keeps a sealed shell round a void (a coconut); open the top for a tub, a planter or a pool, the bottom for a cover, both for a tube or a chimney. To cut a door into a closed shell, add a Boolean.",
        K::Solidify { .. } => "Gives a surface a thickness, behind or in front of its faces: a roof sheet, a canopy, a wall drawn as one face.",
        K::Slice { .. } => "Cuts a mass into floors at heights above its own base point, with a gap between them: a slab's depth or a shadow joint. Starts at the project's levels.",
        K::Taper { .. } => "Narrows (or flares) the shape going up: 1 keeps the top as wide as the bottom, 0.5 halves it.",
        K::Twist { .. } => "Turns the top about the middle, the bottom staying put. Give it more floors (Slice, or Smooth set to Split) for an even curve.",
        K::Subdivision { .. } => "Smooth rounds the shape into soft, organic forms; Split only cuts its faces into smaller ones, keeping its shape, for Twist and Taper to bend.",
    }
}

/// With CREABIM_PERF set, how long a frame (or a part of one) took, on
/// stderr, when it took more than 8 ms (MU37: the Profile view felt late).
pub(crate) struct Perf(std::time::Instant, &'static str);

impl Perf {
    pub(crate) fn new(what: &'static str) -> Self {
        Perf(std::time::Instant::now(), what)
    }
}

impl Drop for Perf {
    fn drop(&mut self) {
        let ms = self.0.elapsed().as_secs_f64() * 1000.0;
        // every slow step is kept for problem reports (MU40: lag measured,
        // not only told): a line in the log, at most one a second for each
        // kind, and its count and worst in the timing table
        crate::actlog::slow(self.1, ms);
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *ON.get_or_init(|| std::env::var_os("CREABIM_PERF").is_some()) {
            static MIN: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
            let min = *MIN.get_or_init(|| {
                std::env::var("CREABIM_PERF_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(8.0)
            });
            if ms > min {
                eprintln!("{}: {ms:.1} ms", self.1);
            }
        }
    }
}

impl eframe::App for CreaBimApp {
    /// Keys typed while a web pane had the keyboard reach the views.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.shell.feed_keys(raw_input) {
            ctx.request_repaint();
        }
        self.palette_keys(raw_input);
        // Tab while a line is drawn moves the tracker to its next field
        // (2.10): taken here, before egui moves the keyboard focus with it
        let hosting = crate::memberstool::hosted(self.plan.tool) && self.plan.pending.is_none();
        if (self.plan.pending.is_some() || hosting) && !ctx.egui_wants_keyboard_input() {
            let mut presses = 0;
            raw_input.events.retain(|e| match e {
                egui::Event::Key {
                    key: egui::Key::Tab,
                    modifiers,
                    pressed,
                    ..
                } if modifiers.is_none() => {
                    presses += usize::from(*pressed);
                    false
                }
                _ => true,
            });
            for _ in 0..presses {
                if hosting {
                    // a host tool: the next host under the pointer
                    self.plan.host_cycle += 1;
                    ctx.request_repaint();
                } else {
                    self.plan.tracker.next_field();
                }
            }
        }
        // the web bars cover the top and bottom of the window: egui's own
        // drop-downs and windows keep out from under them, opening upwards
        // when there is no room below
        let bands = if self.shell.web.is_some() {
            egui::epaint::MarginF32 {
                top: shell::TOP_H,
                bottom: shell::STATUS_H,
                ..Default::default()
            }
        } else {
            egui::epaint::MarginF32::default()
        };
        raw_input.safe_area_insets = Some(egui::SafeAreaInsets(bands));
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let _perf = Perf::new("frame");
        // never lose work to a close: unsaved changes ask first
        if ui.ctx().input(|i| i.viewport().close_requested()) && !self.close_ok && self.unsaved() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_ask = true;
        }
        // script blocks recompute off the UI thread; a finished run
        // repaints so the panel and the views pick it up
        let (scripts, doc) = (&mut self.scripts, &mut self.doc);
        if scripts.poll(doc) {
            ui.ctx().request_repaint();
        }
        let ctx = ui.ctx().clone();
        crate::trace::hold(ctx.input(|i| i.pointer.any_down()));
        let m = ctx.input(|i| i.modifiers);
        shell::MODS.store(
            m.shift as u8 | (m.command as u8) << 1 | (m.alt as u8) << 2,
            std::sync::atomic::Ordering::Relaxed,
        );
        // the colour of what is worked on: orange, or the theme's accent;
        // the 3D view's highlight is built again when it changes (MU39)
        let ink = if self.shell.settings.interaction == "theme" {
            self.shell.theme.selection()
        } else {
            crate::theme::ORANGE
        };
        crate::planview::set_plan_look(
            self.shell.settings.plan_grid,
            self.shell.settings.axis_lines,
        );
        if ink != crate::theme::interaction() {
            crate::theme::set_interaction(ink);
            self.geometry_key = None;
            self.selection_key = None;
        }
        // the ghost the pointer's view drew last frame, for the others
        let at = |g: &Option<(Option<usize>, crate::ghost::Ghost)>| {
            g.as_ref()
                .and_then(|(_, g)| g.parts.first()?.points.first().copied())
        };
        let moved = at(&self.plan.ghost_now) != at(&self.plan.ghost_shared);
        self.plan.ghost_shared = self.plan.ghost_now.take();
        // the other views catch up once the pointer stops
        if moved {
            ctx.request_repaint();
        }
        // Divide's marks, for every view to draw this frame (MU38)
        let _pd = Perf::new("pre-frame");
        (self.plan.divide_marks, self.plan.divide_labels) = self.divide_marks();
        self.plan.divide_guides = self.divide.s.make == creabim_core::divide::Make::Guides;
        // while a point is asked for, the selection stays as it was
        if self.plan.point_pick {
            match &self.pick_selection {
                Some(s) if *s != self.selection => self.selection = s.clone(),
                Some(_) => {}
                None => self.pick_selection = Some(self.selection.clone()),
            }
        } else {
            self.pick_selection = None;
        }
        // Split's clicks, else what the modifier being set shows, and a
        // centre picked for it
        if self.split.is_some() {
            self.split_steps(&ctx);
        } else {
            self.take_picked_centre();
            self.plan.mod_guides = self.modifier_guides();
        }
        drop(_pd);
        // R draws a rectangle while sketching, not a rotate (MU38)
        crate::sketch::set_sketching(&ctx, self.plan.tool == Tool::Sketch);
        // the activity log: what the status bar said (it names every edit
        // and undo) and the tool, when they change
        if self.status != self.logged.0 {
            crate::actlog::line(format!("status: {}", self.status));
            self.logged.0 = self.status.clone();
        }
        if Some(self.plan.tool) != self.logged.1 {
            crate::actlog::line(format!("tool: {:?}", self.plan.tool));
            self.logged.1 = Some(self.plan.tool);
        }
        let title = window_title(&self.path, self.unsaved());
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        crate::units::set(self.doc.model().units());
        // the step drags round to: none while Alt is held (2.10)
        let live = self.plan.snap.live(ctx.input(|i| i.modifiers.alt));
        crate::snaps::set_current_step(if live.allows(crate::snaps::SnapKind::Grid) {
            live.step_len()
        } else {
            0.0
        });
        crate::snaps::set_current_marks(live.marks_len());
        {
            let _p = Perf::new("frame: prompts and keys");
            self.step_prompt(&ctx);
            self.workspace_prompt(&ctx);
            self.area_keys(&ctx);
            self.shortcuts(&ctx);
        }
        {
            let _p = Perf::new("frame: edit mode and spotlight");
            self.keep_edit_mode();
            self.refresh_spotlight();
        }
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().display().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        });
        for path in dropped {
            // a dropped material file joins this project, which keeps its path
            if !materials::is_material_file(&path) {
                self.path = path.clone();
            }
            self.open_path(&path);
        }
        if self.live_reload {
            let _p = Perf::new("frame: watch files");
            for f in self.watch.changed(self.doc.model()) {
                self.reload(&f);
            }
            if !crate::files::linked_files(self.doc.model()).is_empty() {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
        }

        let _pw = Perf::new("web_panels");
        let web = self.web_panels(ui, frame);
        drop(_pw);
        // the panes say again this frame what the one under the pointer does
        self.plan.hint = None;
        self.rect_3d = None;
        self.rect_2d = None;
        self.plan.web_menus = web;
        crate::slabtool::set_options_in_top_bar(&ctx, web);
        self.shell.menu_3d = false;
        if !web {
            egui::Panel::top("menu").show(ui, |ui| {
                self.classic_menu(ui);
                self.toolbar(ui);
                crate::props::flow(ui, |ui| {
                    let some = self.selection.is_some();
                    if let Some(c) = crate::commands::modify_bar(ui, some) {
                        self.run_command(&ctx, c, None);
                    }
                });
            });
            egui::Panel::bottom("status").show(ui, |ui| {
                crate::props::flow(ui, |ui| {
                    ui.label(&self.status);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{} elements", self.element_count()));
                    });
                });
            });
            egui::Panel::left("outliner")
                .default_size(220.0)
                .show(ui, |ui| self.outliner(ui));
        }
        // to the end of the frame from here: Properties, the areas and on
        let _pr = Perf::new("frame: properties on");
        let right_folded = self.shell.settings.right_folded;
        let right = egui::Panel::right(if right_folded {
            "properties_rail"
        } else {
            "properties"
        })
        .default_size(if right_folded {
            40.0
        } else {
            self.shell.settings.right_width
        })
        .size_range(if right_folded {
            40.0..=40.0
        } else {
            220.0..=640.0
        })
        .frame(
            // flush at the left, where the tab column is (MU22)
            egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin {
                left: 0,
                right: 4,
                top: 0,
                bottom: 4,
            }),
        )
        .show(ui, |ui| {
            // the panel keeps the width it was given: what is wider
            // wraps, folds into one column or is clipped, and never
            // pushes the panel wider as the selection changes
            let rect = ui.available_rect_before_wrap();
            let clip = rect.expand2(egui::vec2(6.0, 0.0)).intersect(ui.clip_rect());
            let folded = self.shell.settings.right_folded;
            let edge = ui.max_rect();
            if fold_handle(
                ui.ctx(),
                "props",
                egui::pos2(edge.left() - 11.0, edge.center().y - 23.0),
                folded,
                true,
                if folded {
                    "Open Properties (Ctrl+])"
                } else {
                    "Fold Properties to its tabs (Ctrl+])"
                },
            ) {
                self.shell.settings.right_folded = !folded;
                self.shell.settings.store();
            }
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            inner.set_clip_rect(clip);
            inner.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            // one grid for every row (A4-1): controls as tight as
            // Blender's (MU22), the scroll bar in a gutter of its own
            inner.spacing_mut().interact_size.y = 24.0;
            // text a size down, as Blender's panels (MU22): rows fit
            // on one line at the panel's usual width
            for (style, font) in inner.style_mut().text_styles.iter_mut() {
                font.size = match style {
                    egui::TextStyle::Small => 10.5,
                    egui::TextStyle::Heading => 14.0,
                    _ => (font.size - 1.5).clamp(11.0, 12.5),
                };
            }
            inner.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
            inner.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
            // editable fields (dropdowns, numbers, text) in the field
            // colour, a tone apart from the panel, as Blender does, so
            // they read at a glance as things you can change (MU20)
            {
                let p = crate::theme::Palette::of(ui.ctx());
                let w = &mut inner.visuals_mut().widgets;
                w.inactive.weak_bg_fill = p.field;
                w.inactive.bg_stroke = egui::Stroke::new(1.0, p.edge);
            }
            let mut bar = egui::style::ScrollStyle::solid();
            bar.bar_width = 6.0;
            bar.bar_inner_margin = 4.0;
            inner.spacing_mut().scroll = bar;
            // as Blender's editor (MU22): the tab column down the left
            // edge, a tone darker, then the pinned header and the
            // tab's sections, which scroll
            inner.set_max_width(rect.width());
            let tabs = self.prop_tabs();
            // a placing tool just picked opens its Next tab; a first
            // selection opens Element; otherwise the tab stays
            let now = (self.plan.tool, self.selection.active.is_some());
            if now != self.prop_seen {
                if now.0 != self.prop_seen.0 && now.0 != Tool::Select && !now.1 {
                    self.prop_tab = PropTab::Next;
                } else if now.1 && !self.prop_seen.1 {
                    self.prop_tab = PropTab::Element;
                }
                self.prop_seen = now;
            }
            // the view in front leads, as Blender's editors do (MU40: a
            // sheet's settings showed over the Masters view)
            let front = self.areas.root.areas().get(self.areas.focus).copied();
            if front != self.prop_front {
                match front {
                    Some(crate::areas::Shows::Masters) => self.prop_tab = PropTab::Master,
                    Some(crate::areas::Shows::Profiles) => self.prop_tab = PropTab::Profile,
                    Some(crate::areas::Shows::Log) => self.prop_tab = PropTab::Log,
                    _ => {}
                }
                self.prop_front = front;
            }
            if !tabs.contains(&self.prop_tab) {
                self.prop_tab = tabs[0];
            }
            // a tool just picked opens its Next tab; a new selection
            // with no tab of its own opens Element
            inner.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                self.prop_strip(ui, &tabs, rect.height());
                if self.shell.settings.right_folded {
                    return;
                }
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                    let w = ui.available_width();
                    // the header as wide as the boxes under it, which
                    // leave the scroll bar its gutter: wider, its
                    // fields and Delete ran off the panel (MU24)
                    ui.set_max_width(w - 16.0);
                    ui.add_space(4.0);
                    self.properties_header(ui);
                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            ui.set_max_width(w - 16.0);
                            crate::props::boxes(ui, true);
                            self.properties(ui);
                            crate::props::close_box(ui);
                            crate::props::boxes(ui, false);
                        });
                });
            });
            ui.allocate_rect(rect, egui::Sense::hover());
        })
        .response
        .rect;
        // remember a new width once the drag is over
        let width = right.width().round();
        if (width - self.shell.settings.right_width).abs() >= 1.0
            && !ui.input(|i| i.pointer.any_down())
        {
            self.shell.settings.right_width = width;
            self.shell.settings.store();
        }

        // to the end of the frame from here: the areas, then what follows
        let _pa = Perf::new("frame: areas on");
        egui::CentralPanel::default().show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            self.mat.area = Some(full);
            let plan_bg = Color32::from_rgb(252, 252, 250);
            match self.placement {
                PlanPlacement::Docked => self.show_areas(ui, full, frame, plan_bg),
                PlanPlacement::Overlay => {
                    self.show_3d(ui, full, frame);
                    let inset = Rect::from_min_size(
                        full.right_top() + Vec2::new(-full.width() * 0.42 - 12.0, 12.0),
                        Vec2::new(full.width() * 0.42, full.height() * 0.45),
                    );
                    self.show_plan(
                        ui,
                        inset,
                        Color32::from_rgba_unmultiplied(252, 252, 250, 225),
                    );
                    ui.painter().rect_stroke(
                        inset,
                        4.0,
                        egui::Stroke::new(1.0, Color32::from_gray(160)),
                        egui::StrokeKind::Outside,
                    );
                }
                PlanPlacement::Window | PlanPlacement::Hidden => self.show_3d(ui, full, frame),
            }
            ui.allocate_rect(full, Sense::hover());
        });

        if let Some(s) = self.plan.status.take() {
            self.status = s;
        }
        for (c, at) in std::mem::take(&mut self.plan.commands) {
            self.run_command(&ctx, c, at);
        }
        if std::mem::take(&mut self.plan.want_levels) {
            self.open_levels();
        }
        // a view mark double-clicked: its view opens
        if let Some(v) = self.plan.want_view.take() {
            self.active_view = Some(v);
        }
        if std::mem::take(&mut self.plan.want_catalogue) {
            self.catalogue.open = true;
        }
        // a box selection that took more than one kind: Keep only… is offered
        if std::mem::take(&mut self.plan.boxed) {
            self.after_box_selection();
        }
        // a box dragged with the Footing tool: footings under all it took
        let hosts = std::mem::take(&mut self.plan.want_footings);
        if !hosts.is_empty() {
            self.selection.clear();
            self.selection.extend(hosts, false);
            self.footings_under_selection();
        }
        self.open_view_menu();
        self.materials_window(&ctx);
        self.assemblies_window(&ctx);
        self.types_window(&ctx);
        self.type_preview_window(&ctx);
        self.catalogue_window(&ctx);
        self.levels_window(&ctx);
        self.level_copy_window(&ctx);
        self.styles_window(&ctx);
        self.templates_window(&ctx);
        self.units_window(&ctx);
        self.divide_window(&ctx);
        self.profiles_window(&ctx);
        self.settings_window(&ctx);
        // the paper follows the setting and the theme; a change redraws
        // the 3D view's grid and background
        let dark = self.dark_paper(&ctx);
        if dark != self.paper_is_dark {
            self.paper_is_dark = dark;
            self.geometry_key = None;
        }
        self.project_window(&ctx);
        self.note_editor_window(&ctx);
        self.ifc_export_window(&ctx);
        self.reference_steps();
        self.calibrate_window(&ctx);
        self.sheets_export_window(&ctx);
        self.new_project_window(&ctx);
        self.marks_window(&ctx);
        self.graphic_styles_window(&ctx);
        self.ground_spacing_window(&ctx);
        self.import_report_window(&ctx);
        self.help_window(&ctx);
        self.report_window(&ctx);
        self.close_window(&ctx);
        // a plan in its own window keeps egui's menus
        self.plan.web_menus = false;

        if self.placement == PlanPlacement::Window {
            let mut close = false;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("creabim_plan"),
                egui::ViewportBuilder::default()
                    .with_title("CreaBIM · Plan")
                    .with_inner_size([760.0, 560.0]),
                |ui, _class| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        let rect = ui.available_rect_before_wrap();
                        self.show_plan(ui, rect, plan_bg_color());
                        ui.allocate_rect(rect, Sense::hover());
                    });
                    if ui.input(|i| i.viewport().close_requested()) {
                        close = true;
                    }
                },
            );
            if close {
                self.placement = PlanPlacement::Docked;
            }
        }
    }
}

/// The spacings the 3D ground grid offers, in metres.
const GROUND_SPACINGS: [f64; 6] = [0.25, 0.5, 1.0, 2.0, 5.0, 10.0];

/// The finest and the widest ground grid spacing that can be typed, metres.
const GROUND_SPACING_RANGE: (f64, f64) = (0.05, 100.0);

/// A usable ground grid spacing: a setting from the file, kept sensible.
/// Turn a pane's shapes painted since `from` over to dark paper: greys
/// (black ink, hatches, fills, the paper) invert, coloured marks and
/// pictures stay as they are.
fn darken_shapes(ctx: &egui::Context, layer: egui::LayerId, from: Option<egui::layers::ShapeIdx>) {
    let start = from.map_or(0, |i| i.0);
    let keep = crate::planview::kept_colours(ctx, layer);
    ctx.graphics_mut(|g| {
        let list = g.entry(layer);
        let end = list.next_idx().0;
        for i in start..end {
            if keep.iter().any(|(a, b)| i >= *a && i <= *b) {
                continue;
            }
            list.mutate_shape(egui::layers::ShapeIdx(i), |cs| {
                if let egui::Shape::Mesh(m) = &cs.shape {
                    if m.texture_id != egui::TextureId::default() {
                        return;
                    }
                }
                egui::epaint::shape_transform::adjust_colors(&mut cs.shape, |c| {
                    *c = paper_dark(*c);
                });
            });
        }
    });
}

/// A colour on light paper, on dark paper: a grey turns over (white paper
/// to near black, black ink to near white), a colour is kept.
fn paper_dark(c: Color32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    if hi.saturating_sub(lo) > 28 {
        // a colour keeps its hue; a dark one (a blue annotation) is lifted
        // toward white until it reads on dark paper as it did on white (MU30)
        let lum = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
        const WANT: f32 = 0.5;
        if lum >= WANT {
            return c;
        }
        let t = (WANT - lum) / (1.0 - lum);
        let up = |v: u8| (v as f32 + (255.0 - v as f32) * t).round() as u8;
        return Color32::from_rgba_unmultiplied(up(r), up(g), up(b), a);
    }
    let v = (r as f32 + g as f32 + b as f32) / 3.0;
    let w = (34.0 + (255.0 - v) * 0.82).round() as u8;
    Color32::from_rgba_unmultiplied(w, w, w.saturating_add(3), a)
}

/// How far the workplane's grid reaches from its origin before it has
/// faded into the background, metres.
const WORKPLANE_FADE: f64 = 4.0;

/// The 3D view's background, which the workplane's grid fades into.
const BACKGROUND_3D: [f32; 3] = [0.93, 0.94, 0.96];

/// The 3D view's background on dark paper (A4-3).
const BACKGROUND_3D_DARK: [f32; 3] = [0.12, 0.13, 0.15];

/// A face workplane's grid (MU18): soft green-grey lines at the ground
/// grid's spacing (every fifth a little stronger) that fade into the
/// background `reach` metres from its origin, and a small axis mark there.
/// Drawn just off the face, as line pairs for the 3D view.
fn workplane_lines(
    plane: Workplane,
    spacing: f64,
    reach: f64,
    background: [f32; 3],
) -> Vec<crate::render3d::Vertex> {
    let lift = plane.normal() * 0.003;
    let at = |u: f64, v: f64| {
        (plane.to_model(DVec2::new(u, v)) + lift)
            .as_vec3()
            .to_array()
    };
    let mix = |c: [f32; 3], t: f32| {
        [
            c[0] + (background[0] - c[0]) * t,
            c[1] + (background[1] - c[1]) * t,
            c[2] + (background[2] - c[2]) * t,
        ]
    };
    let minor = [0.62, 0.72, 0.60];
    let major = [0.40, 0.55, 0.38];
    // fine enough to read, coarse enough not to fill the face
    let step = spacing.clamp(0.1, 2.0).max(reach / 40.0);
    let n = (reach / step).floor() as i32;
    let piece = step.min(reach / 16.0);
    let mut out = Vec::new();
    for i in -n..=n {
        let k = i as f64 * step;
        let base = if i % 5 == 0 { major } else { minor };
        // each line in short pieces, so it fades along its length too
        let mut t = -reach;
        while t < reach - 1e-9 {
            let t1 = (t + piece).min(reach);
            for (a, b) in [((k, t), (k, t1)), ((t, k), (t1, k))] {
                let fade = |p: (f64, f64)| {
                    let d = (p.0 * p.0 + p.1 * p.1).sqrt() / reach;
                    mix(base, (d as f32).clamp(0.0, 1.0).powf(1.5))
                };
                if (a.0 * a.0 + a.1 * a.1).sqrt() > reach && (b.0 * b.0 + b.1 * b.1).sqrt() > reach
                {
                    continue;
                }
                out.push(line_vertex(at(a.0, a.1), fade(a)));
                out.push(line_vertex(at(b.0, b.1), fade(b)));
            }
            t = t1;
        }
    }
    // its origin and axes, small
    for (d, c) in [
        ((0.6, 0.0), [0.75, 0.30, 0.25]),
        ((0.0, 0.6), [0.30, 0.55, 0.30]),
    ] {
        out.push(line_vertex(at(0.0, 0.0), c));
        out.push(line_vertex(at(d.0, d.1), c));
    }
    out
}

fn ground_spacing(s: f64) -> f64 {
    if s.is_finite() {
        s.clamp(GROUND_SPACING_RANGE.0, GROUND_SPACING_RANGE.1)
    } else {
        1.0
    }
}

/// Framing that stays solid in X-ray: columns, beams, stairs and railings,
/// and masses that stand for them (a pergola's rafters as IfcBeam).
fn is_framing(k: &ElementKind) -> bool {
    match k {
        ElementKind::Column(_)
        | ElementKind::Beam(_)
        | ElementKind::Stair(_)
        | ElementKind::Railing(_) => true,
        ElementKind::FreeMesh(m) => matches!(
            m.ifc_class.as_str(),
            "IfcBeam" | "IfcColumn" | "IfcMember" | "IfcStair" | "IfcRailing"
        ),
        _ => false,
    }
}

/// How opaque walls, slabs, roofs and masses are in X-ray.
const XRAY_OPACITY: f32 = 0.18;

/// How strongly what is inside walls and slabs shows through them, in its
/// colours.
const BURIED_OPACITY: f32 = 0.5;
/// The colour of their edges then.
const XRAY_EDGE: [f32; 3] = [0.30, 0.34, 0.42];

fn plan_bg_color() -> Color32 {
    Color32::from_rgb(252, 252, 250)
}

fn line_vertex(pos: [f32; 3], color: [f32; 3]) -> Vertex {
    Vertex {
        pos,
        normal: [0.0, 0.0, 1.0],
        color,
        surface: crate::render3d::PLAIN,
    }
}

/// Triangle indices (into the flattened mesh) that make up polygon face `face`.
fn face_triangles(mesh: &creabim_core::EditMesh, face: usize) -> std::ops::Range<usize> {
    let start: usize = mesh.faces[..face.min(mesh.faces.len())]
        .iter()
        .map(|f| f.len() - 2)
        .sum();
    let len = mesh.faces.get(face).map_or(0, |f| f.len() - 2);
    start..start + len
}

fn stem(path: &str) -> String {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.split('.').next().unwrap_or("project").to_string()
}

fn with_extension(path: &str, ext: &str) -> String {
    let p = std::path::Path::new(path);
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    dir.join(format!("{}.{ext}", stem(path)))
        .to_string_lossy()
        .into_owned()
}

/// Give every level a plan view so older files open with something to show.
fn with_plan_views(model: Model) -> Model {
    let planned: Vec<ElementId> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::View(View {
                kind: ViewKind::Plan { level, .. },
                ..
            }) => Some(*level),
            _ => None,
        })
        .collect();
    let mut tx = Transaction::new("Add plan views");
    let mut any = false;
    for (e, _) in model.levels() {
        if !planned.contains(&e.id) {
            tx = tx.insert(Element::new(
                format!("{} plan", e.name),
                ElementKind::View(View::plan(e.id)),
            ));
            any = true;
        }
    }
    if !any {
        return model;
    }
    let mut doc = Document::from_model(model);
    // a failed insert leaves the model as it was, which still opens
    let _ = doc.commit(tx);
    doc.model().clone()
}

/// A file's name without its folders.
/// What to call an element in a count.
pub(crate) fn kind_word(k: &ElementKind) -> &'static str {
    match k {
        ElementKind::Level(_) => "level",
        ElementKind::Grid(_) => "grid line",
        ElementKind::GridGroup(_) => "grid system",
        ElementKind::Wall(_) => "wall",
        ElementKind::Slab(_) => "slab",
        ElementKind::Opening(o) => match o.kind {
            creabim_core::OpeningKind::Door => "door",
            creabim_core::OpeningKind::Window => "window",
            creabim_core::OpeningKind::Hole => "wall opening",
        },
        ElementKind::FreeMesh(_) => "mass",
        ElementKind::Sketch(_) => "sketch",
        ElementKind::Guide(_) => "guide",
        ElementKind::Cut(_) => "cut",
        ElementKind::Dimension(_) => "dimension",
        ElementKind::Tag(_) => "tag",
        ElementKind::Note(_) => "note",
        ElementKind::TextStyle(_) => "text style",
        ElementKind::SheetTemplate(_) => "sheet template",
        ElementKind::Units(_) => "project units",
        ElementKind::Project(_) => "project information",
        ElementKind::Reference(_) => "reference",
        ElementKind::PaperMark(_) => "paper mark",
        ElementKind::MarkRules(_) => "mark numbering",
        ElementKind::StylePack(_) => "style pack",
        ElementKind::View(_) => "view",
        ElementKind::Sheet(_) => "sheet",
        ElementKind::Viewport(_) => "placed view",
        ElementKind::Material(_) => "material",
        ElementKind::Assembly(_) => "assembly",
        ElementKind::Column(_) => "column",
        ElementKind::Beam(_) => "beam",
        ElementKind::Roof(_) => "roof",
        ElementKind::Stair(_) => "stair",
        ElementKind::Railing(_) => "railing",
        ElementKind::CropBox(_) => "crop box",
        ElementKind::Object(_) => "object",
        ElementKind::Room(_) => "room",
        ElementKind::RoomSeparator(_) => "room separator",
        ElementKind::Join(_) => "junction rule",
        ElementKind::Detail(_) => "detail component",
        ElementKind::Ceiling(_) => "ceiling",
        ElementKind::ElementType(_) => "type",
        ElementKind::Shaft(_) => "shaft",
        ElementKind::Footing(_) => "footing",
        ElementKind::Part(p) if p.void => "void",
        ElementKind::Part(_) => "part",
        ElementKind::Site(s) if s.is_plant() => "plant",
        ElementKind::Site(_) => "site element",
        ElementKind::ScriptObject(_) => "script",
    }
}

/// "wall" or "walls", for counts in messages.
pub(crate) fn plural(kind: &str, n: usize) -> String {
    let k = kind.to_lowercase();
    match (n, k.as_str()) {
        (1, _) => k,
        (_, "mass") => "masses".into(),
        _ => format!("{k}s"),
    }
}

/// The window's title: the project, a • while it has unsaved changes,
/// then the app, with its version on test builds (0.x) (A7-8).
fn window_title(path: &str, unsaved: bool) -> String {
    let name = std::path::Path::new(path)
        .file_stem()
        .map_or(path.to_string(), |n| n.to_string_lossy().into_owned());
    let version = env!("CARGO_PKG_VERSION");
    // a build being worked on says so in full: which version it leads to
    // (MU39: 1.0 showed while 1.0.1 was out and the next one in hand)
    let app = match version.split('.').collect::<Vec<_>>().as_slice() {
        _ if version.contains('-') => format!("CreaBIM {version}"),
        ["0", minor, ..] => format!("CreaBIM 0.{minor} beta"),
        [major, minor, ..] => format!("CreaBIM {major}.{minor}"),
        _ => "CreaBIM".to_string(),
    };
    format!("{}{name} — {app}", if unsaved { "• " } else { "" })
}

fn file_label(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or(path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// How far guides reach each way in the 3D view.
const GUIDE_REACH_3D: f64 = 60.0;

/// A guide through `a` and `b`, dashed across the 3D view. It is sampled
/// along its length so the parts behind the camera drop out cleanly.
/// The plane of the face a ray hits, facing back along the ray.
fn face_plane(scene: &[creabim_geom::SceneItem], o: DVec3, d: DVec3) -> Option<Workplane> {
    let h = pick::pick(scene, o.as_vec3(), d.as_vec3())?;
    let item = scene.iter().find(|s| s.id == h.id)?;
    let idx = &item.mesh.indices[h.triangle * 3..h.triangle * 3 + 3];
    let v = |k: usize| glam::Vec3::from(item.mesh.positions[idx[k] as usize]);
    let mut n = (v(1) - v(0))
        .cross(v(2) - v(0))
        .normalize_or_zero()
        .as_dvec3();
    if n.dot(d) > 0.0 {
        n = -n;
    }
    Some(Workplane::facing(o + d * h.t as f64, n))
}

fn draw_guide_3d(
    painter: &egui::Painter,
    project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
    a: DVec3,
    b: DVec3,
    stroke: egui::Stroke,
) {
    let Some(d) = (b - a).try_normalize() else {
        return;
    };
    let m = (a + b) / 2.0;
    let mut run: Vec<egui::Pos2> = Vec::new();
    let steps = 240;
    for i in 0..=steps {
        let t = -GUIDE_REACH_3D + 2.0 * GUIDE_REACH_3D * i as f64 / steps as f64;
        match project(m + d * t).filter(|p| painter.clip_rect().expand(200.0).contains(*p)) {
            Some(p) => run.push(p),
            None => {
                if run.len() > 1 {
                    painter.extend(egui::Shape::dashed_line(&run, stroke, 6.0, 4.0));
                }
                run.clear();
            }
        }
    }
    if run.len() > 1 {
        painter.extend(egui::Shape::dashed_line(&run, stroke, 6.0, 4.0));
    }
}

/// Under a plan's view depth, when the view hides its footings: why the
/// depth shows none, and a button to show them. True when clicked.
fn footings_note(ui: &mut egui::Ui) -> bool {
    let mut show = false;
    crate::props::flow(ui, |ui| {
        ui.label(
            egui::RichText::new(
                "Footings are hidden in this view, so the view depth can't show them.",
            )
            .weak(),
        );
        show = ui
            .small_button("Show footings")
            .on_hover_text("Footings under the floor draw dashed, as the slab hides them.")
            .clicked();
    });
    show
}

/// A plan's graphics in its Properties: the tint of what lies below the
/// cut, how far below its floor it looks, and its pens' line weights.
/// Returns the undo label of a change.
fn plan_graphics_ui(
    ui: &mut egui::Ui,
    g: &mut creabim_core::PlanGraphics,
    scale: f64,
    header: bool,
) -> Option<&'static str> {
    let mut label = None;
    // its own box, folding as the others (MU22); in Properties the panel
    // draws the header, with its ? (MU37)
    if header {
        let id = egui::Id::new("props.view.graphics.open");
        let open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
        let (resp, _) = crate::props::header(ui, "Graphics", open, false);
        if resp.clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(id, !open));
        }
        if !open {
            return None;
        }
    }
    // how what is cut is drawn (A4-11)
    crate::props::row(ui, "Drawing style", |ui| {
        egui::ComboBox::from_id_salt("view.drawing_style")
            .icon(crate::props::combo_icon)
            .selected_text(g.drawing_style.label())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for s in creabim_core::graphics::DrawingStyle::ALL {
                    if ui
                        .selectable_value(&mut g.drawing_style, s, s.label())
                        .on_hover_text(s.hint())
                        .changed()
                    {
                        label = Some("Drawing style");
                    }
                }
            });
    });
    if crate::props::check(ui, "Below the cut", &mut g.below_fill, "Tinted")
        .on_hover_text("Furniture, fittings and imported meshes under the cut get a light fill.")
        .changed()
    {
        label = Some("Below-cut fill");
    }
    if g.below_fill {
        crate::props::row(ui, "Strength", |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 56.0).max(40.0);
            if ui
                .add(egui::Slider::new(&mut g.below_strength, 0.05..=1.0))
                .changed()
            {
                label = Some("Below-cut fill");
            }
        });
        if crate::props::check(ui, "", &mut g.below_grey, "In grey")
            .on_hover_text("Grey, not the materials' colours")
            .changed()
        {
            label = Some("Below-cut fill");
        }
    }
    let mut to_level = g.range.view_depth.is_none();
    if crate::props::check(ui, "View depth", &mut to_level, "To the level below")
        .on_hover_text(
            "How far below its floor the plan looks, through holes in the floor such as a stair well.",
        )
        .changed()
    {
        g.range.view_depth = if to_level { None } else { Some(1.0) };
        label = Some("View depth");
    }
    if let Some(d) = &mut g.range.view_depth {
        crate::props::row(ui, "Below the floor", |ui| {
            if crate::props::fill(ui, crate::units::drag(d).range(0.0..=100.0)).changed() {
                label = Some("View depth");
            }
        });
    }
    let mut auto = g.weights.is_none();
    if crate::props::check(ui, "Line weights", &mut auto, "From the style pack")
        .on_hover_text("The style pack's pens for the view's scale: in the Standard pack, heavier at 1:50 and larger, lighter at 1:100 and smaller.")
        .changed()
    {
        g.weights = if auto {
            None
        } else {
            Some(creabim_core::LineWeights::for_scale(scale))
        };
        label = Some("Line weights");
    }
    // pens by category are object styles now; a view's own from before
    // still draws, and can be cleared
    if g.category_pens != creabim_core::graphics::default_category_pens() {
        crate::props::flow(ui, |ui| {
            ui.label(egui::RichText::new("This view has pens by category of its own.").weak());
            if ui
                .small_button("Use the style pack's")
                .on_hover_text(
                    "Object styles in File, Graphic styles set each category's pens for every view",
                )
                .clicked()
            {
                g.category_pens = creabim_core::graphics::default_category_pens();
                label = Some("Pens by category");
            }
        });
    }
    if let Some(w) = &mut g.weights {
        for (name, v) in w.fields_mut() {
            crate::props::flow(ui, |ui| {
                ui.label(name);
                if ui
                    .add(
                        egui::DragValue::new(v)
                            .speed(0.01)
                            .range(0.02..=2.0)
                            .max_decimals(2)
                            .suffix(" mm"),
                    )
                    .changed()
                {
                    label = Some("Line weights");
                }
            });
        }
    }
    label
}

/// A drop-down of the ways a section, elevation or 3D view draws.
fn view_style_combo(ui: &mut egui::Ui, salt: &str, style: &mut ViewStyle) -> bool {
    let mut changed = false;
    crate::props::row(ui, "Style", |ui| {
        egui::ComboBox::from_id_salt(salt)
            .icon(crate::props::combo_icon)
            .selected_text(style.label())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for s in ViewStyle::ALL {
                    changed |= ui.selectable_value(style, s, s.label()).changed();
                }
            });
    });
    changed
}

/// The file named on the command line, if any: the first argument that
/// isn't an option.
pub(crate) fn launch_file(args: impl Iterator<Item = String>) -> Option<String> {
    args.into_iter()
        .find(|a| !a.starts_with('-') && !a.is_empty())
}

/// What the status bar says when the starter house opens.
pub(crate) const STARTER_LOADED: &str = "Loaded the starter house. Orbit with the middle button (or Alt + left drag), pan with right drag, zoom with the wheel; a left drag selects with a box. The Sheets group has its drawings.";

/// The app round the sample house, for tests.
#[cfg(test)]
pub(crate) fn test_app() -> CreaBimApp {
    let mut app = CreaBimApp::blank(None);
    app.load_model(sample_house());
    app
}

impl CreaBimApp {
    /// The Snap menu's Custom step: a small window to type it in.
    fn step_prompt(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.plan.step_prompt.take() else {
            return;
        };
        let mut open = true;
        let mut done = false;
        crate::app::shell::window("Step")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("What points and drags round to: 25 mm, 1/4\", 2 cm.");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .vertical_align(egui::Align::Center)
                        .desired_width(140.0),
                );
                r.request_focus();
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (enter || ui.button("Use it").clicked()) && !text.trim().is_empty() {
                    match crate::units::parse(&text) {
                        Some(m) if m > 0.0 && m < 100.0 => {
                            self.plan.snap.increment = true;
                            self.plan.snap.step_um = (m * 1e6).round() as u32;
                            self.status = format!(
                                "Step {}: points and drags round to it. Alt ignores it for a move.",
                                self.plan.snap.step_label()
                            );
                            done = true;
                        }
                        _ => {
                            self.status = format!("\"{text}\" isn't a length: try 25 mm or 1/4\".")
                        }
                    }
                }
            });
        if open && !done && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.plan.step_prompt = Some(text);
        }
    }
}

/// The type the tool's next element takes, by name, if one is picked.
fn typesui_type_name(app: &CreaBimApp, tool: Tool) -> Option<String> {
    let cat = typesui::category_of_tool(tool)?;
    let id = app.plan.types.get(&cat)?;
    app.doc.model().get(*id).map(|e| e.name.clone())
}

#[cfg(test)]
mod title_tests {
    use super::window_title;

    #[test]
    fn dark_paper_turns_greys_over_and_keeps_colours() {
        use super::{paper_dark, Color32};
        let paper = paper_dark(Color32::from_rgb(250, 250, 247));
        let ink = paper_dark(Color32::BLACK);
        assert!(paper.r() < 50, "paper {paper:?}");
        assert!(ink.r() > 200, "ink {ink:?}");
        let orange = crate::theme::interaction();
        assert_eq!(paper_dark(orange), orange);
    }

    #[test]
    fn the_title_names_the_project_marks_unsaved_and_the_test_version() {
        let v = env!("CARGO_PKG_VERSION");
        let saved = window_title("projects/Starter house.creabim", false);
        assert!(saved.starts_with("Starter house — CreaBIM"), "{saved}");
        if v.starts_with("0.") {
            assert!(saved.ends_with(" beta"), "{saved}");
        }
        let unsaved = window_title("Starter house.creabim", true);
        assert!(unsaved.starts_with("• Starter house — "), "{unsaved}");
    }
}

#[cfg(test)]
mod box_select_tests {
    use super::*;

    #[test]
    fn a_left_drag_in_the_apps_plan_area_selects_with_a_box() {
        let mut app = crate::app::test_app();
        app.load_model(creabim_core::starter::starter_house());
        let view = app
            .doc
            .model()
            .iter()
            .find(|e| e.name == "Ground Floor plan")
            .unwrap()
            .id;
        app.active_view = Some(view);
        app.selection.clear();
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0));
        let mut t = 0.0;
        let mut frame = |app: &mut CreaBimApp, events: Vec<egui::Event>| {
            t += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(rect),
                events,
                time: Some(t),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| app.show_plan(ui, rect, Color32::WHITE));
            out.textures_delta.clear();
        };
        // left to right, round the whole drawing: takes what is inside
        let (a, b) = (egui::Pos2::new(5.0, 5.0), egui::Pos2::new(1195.0, 895.0));
        frame(&mut app, vec![egui::Event::PointerMoved(a)]);
        frame(&mut app, vec![egui::Event::PointerMoved(a)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: a,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        for k in 1..=10 {
            frame(
                &mut app,
                vec![egui::Event::PointerMoved(a + (b - a) * (k as f32 / 10.0))],
            );
        }
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: b,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(b)]);
        assert!(!app.selection.is_empty(), "status: {}", app.status);
    }
}

#[cfg(test)]
mod sheet_tools_tests {
    use super::*;

    fn a_sheet(app: &CreaBimApp) -> ElementId {
        sheets::sheets(app.doc.model())
            .first()
            .map(|(id, _)| *id)
            .expect("the test project has sheets")
    }

    #[test]
    fn the_header_buttons_reach_their_actions() {
        // MU22: these were handled but never routed from a click
        let mut app = crate::app::test_app();
        let ctx = egui::Context::default();
        let before = app.plan.temp_dims_off;
        app.run_action(&ctx, "view.tempdims");
        assert_ne!(
            app.plan.temp_dims_off, before,
            "temporary dimensions toggle"
        );
        app.run_action(&ctx, "sheet.preview");
        assert!(app.print_preview, "print preview toggles");
        app.run_action(&ctx, "export.sheets");
        assert!(app.sheets_export_open, "Export sheets opens");
    }

    #[test]
    fn a_north_arrow_and_a_scale_bar_go_on_the_sheet_and_into_its_pdf() {
        let mut app = crate::app::test_app();
        let ctx = egui::Context::default();
        let sheet = a_sheet(&app);
        app.active_view = Some(sheet);
        let page = |app: &CreaBimApp| {
            creabim_geom::sheet::sheets_pdf_of(app.doc.model(), &app.sheet_info(), &[sheet]).len()
        };
        let plain = page(&app);
        app.run_action(&ctx, "sheet.north");
        let north = app.selection.active.expect("the north arrow is selected");
        app.run_action(&ctx, "sheet.scalebar");
        let bar = app.selection.active.expect("the scale bar is selected");
        let marks: Vec<&creabim_core::PaperMark> = app
            .doc
            .model()
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::PaperMark(m) => Some(m),
                _ => None,
            })
            .collect();
        assert_eq!(marks.len(), 2);
        assert!(marks.iter().all(|m| m.sheet == sheet));
        // inside the sheet, clear of each other
        let s = sheets::sheet_of(app.doc.model(), sheet).unwrap().clone();
        let (_, tb_hi) = creabim_geom::sheet::title_block_box(app.doc.model(), sheet).unwrap();
        for m in &marks {
            let (lo, hi) = m.bounds();
            assert!(
                lo.x > 0.0 && lo.y > 0.0 && hi.x < s.width && hi.y < s.height,
                "{lo} {hi}"
            );
            assert!(
                lo.y > tb_hi.y,
                "clear of the title block: {lo} over {tb_hi}"
            );
        }
        assert!(page(&app) > plain, "the PDF draws them");
        // they move with Move, and only across the sheet
        let r = app.doc.commit(
            creabim_core::Transaction::new("t").replace(
                app.doc.model().get(north).unwrap().clone(),
                creabim_core::transform::transformed(
                    app.doc.model(),
                    app.doc.model().get(north).unwrap(),
                    &creabim_core::transform::Xform::Move(glam::DVec3::new(5.0, 0.0, 0.0)),
                )
                .unwrap(),
            ),
        );
        assert!(r.is_ok());
        // their Properties draw
        app.selection.only(bar);
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            app.properties_header(ui);
            app.properties(ui);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn export_sheets_writes_chosen_sheets_one_pdf_each() {
        let mut app = crate::app::test_app();
        let all: Vec<ElementId> = sheets::sheets(app.doc.model())
            .iter()
            .map(|(id, _)| *id)
            .collect();
        app.sheets_export.scope = ExportScope::Chosen;
        app.sheets_export.chosen = all.iter().take(2).copied().collect();
        assert_eq!(app.sheets_to_export(false).len(), all.len().min(2));
        app.sheets_export.each = true;
        app.export_sheets_now(false);
        assert!(app.status.starts_with("Exported"), "{}", app.status);
        app.sheets_export.scope = ExportScope::This;
        app.active_view = None;
        assert!(app.sheets_to_export(false).is_empty());
    }
}

#[cfg(test)]
mod prop_tab_tests {
    use super::*;

    fn draw(app: &mut CreaBimApp) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            app.properties_header(ui);
            app.properties(ui);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn every_properties_tab_draws_for_a_wall_and_for_nothing_selected() {
        let mut app = crate::app::test_app();
        let wall = app
            .doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap()
            .id;
        app.selection.only(wall);
        let tabs = app.prop_tabs();
        assert_eq!(
            &tabs[..4],
            &[
                PropTab::Element,
                PropTab::Joins,
                PropTab::Graphics,
                PropTab::Data
            ]
        );
        for t in tabs {
            app.prop_tab = t;
            draw(&mut app);
        }
        app.selection.clear();
        let tabs = app.prop_tabs();
        assert!(tabs.contains(&PropTab::Model3d) && tabs.contains(&PropTab::Project));
        for t in tabs {
            app.prop_tab = t;
            draw(&mut app);
        }
        app.plan.tool = Tool::Window;
        assert_eq!(app.prop_tabs()[0], PropTab::Next);
        app.prop_tab = PropTab::Next;
        draw(&mut app);
    }

    #[test]
    fn a_view_added_as_a_reference_is_placed_selected_and_hidden_with_alt_f2() {
        let mut app = crate::app::test_app();
        let plans: Vec<ElementId> = app
            .doc
            .model()
            .iter()
            .filter(|e| matches!(&e.kind, ElementKind::View(v) if matches!(v.kind, ViewKind::Plan { .. })))
            .map(|e| e.id)
            .collect();
        assert!(plans.len() >= 2, "the test project has two plans");
        let (here, below) = (plans[1], plans[0]);
        app.add_reference(here, creabim_core::RefSource::View(below));
        let id = app.selection.active.expect("the new reference is selected");
        let Some(ElementKind::Reference(r)) = app.doc.model().get(id).map(|e| &e.kind) else {
            panic!("a reference was added")
        };
        assert_eq!(r.shown_in, vec![here]);
        assert!(r.bounds.is_some(), "its bounds were read from the view");
        // in place: its own points are where they were
        let c = r.centre();
        assert!(r.place(c).distance(c) < 1e-9);
        assert_eq!(
            creabim_core::reference::in_view(app.doc.model(), here).len(),
            1
        );
        // its Properties draw, every tab
        for t in app.prop_tabs() {
            app.prop_tab = t;
            draw(&mut app);
        }
        // Alt+F2's toggle hides it, then shows it again
        app.toggle_references(here);
        assert!(
            matches!(&app.doc.model().get(id).unwrap().kind, ElementKind::Reference(r) if r.hidden)
        );
        app.toggle_references(here);
        assert!(
            matches!(&app.doc.model().get(id).unwrap().kind, ElementKind::Reference(r) if !r.hidden)
        );
        // the Trace tab lists it, with nothing selected
        app.selection.clear();
        app.prop_tab = PropTab::Trace;
        draw(&mut app);
    }
}

/// Is the tool drawing an outline on a face (an extrusion or a void)?
fn on_face_now(plan: &crate::planview::PlanState) -> bool {
    crate::partstool::on_face(plan).is_some()
}

/// A slim handle on a panel's edge that folds or opens it (MU29): a
/// control of the panel, not one of its tabs. `left_of` puts it on the
/// panel's left edge, pointing the way it will go.
pub(crate) fn fold_handle(
    ctx: &egui::Context,
    id: &str,
    at: egui::Pos2,
    folded: bool,
    left_of: bool,
    tip: &str,
) -> bool {
    let p = crate::theme::Palette::of(ctx);
    let mut clicked = false;
    egui::Area::new(egui::Id::new(("fold-handle", id)))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(11.0, 46.0), egui::Sense::click());
            let fill = if resp.hovered() {
                p.raise2
            } else {
                p.raise.gamma_multiply(0.8)
            };
            let round = if left_of {
                egui::CornerRadius {
                    nw: 6,
                    sw: 6,
                    ne: 0,
                    se: 0,
                }
            } else {
                egui::CornerRadius {
                    nw: 0,
                    sw: 0,
                    ne: 6,
                    se: 6,
                }
            };
            ui.painter().rect(
                r,
                round,
                fill,
                egui::Stroke::new(1.0, p.border),
                egui::StrokeKind::Inside,
            );
            // a small chevron: which way it goes
            let c = r.center();
            let s = if folded == left_of { -1.0 } else { 1.0 };
            let ink = if resp.hovered() { p.text } else { p.muted };
            let pts = [
                c + egui::vec2(-1.5 * s, -4.0),
                c + egui::vec2(1.5 * s, 0.0),
                c + egui::vec2(-1.5 * s, 4.0),
            ];
            ui.painter()
                .line_segment([pts[0], pts[1]], egui::Stroke::new(1.4, ink));
            ui.painter()
                .line_segment([pts[1], pts[2]], egui::Stroke::new(1.4, ink));
            clicked = resp.on_hover_text(tip).clicked();
        });
    clicked
}
