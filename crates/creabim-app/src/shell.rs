//! The app's side of the web interface: what each pane shows, the menus,
//! and what the user's clicks there do. Menus are built as data here, so
//! the classic egui menu bar draws the same items.

use super::*;
use crate::theme::Theme;
use crate::ui_model::{self, Item};
use crate::webui::{Msg, Pane, WebUi};
use serde_json::{json, Value};

/// Height of the top bar (menus, tools and the tool options strip) and the
/// status bar, in points.
pub(super) const TOP_H: f32 = 105.0;
pub(super) const STATUS_H: f32 = 26.0;
/// Size of the 3D view's navigation buttons.
const NAV_W: f32 = 420.0;
const NAV_H: f32 = 34.0;

thread_local! {
    static LEFT_CACHE: std::cell::RefCell<Option<(String, Value)>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) struct Shell {
    pub web: Option<WebUi>,
    /// Why the web interface couldn't start, when it couldn't.
    pub failed: Option<String>,
    /// The classic egui panels, by choice.
    pub classic: bool,
    pub theme: Theme,
    /// The theme and panel widths, kept between sessions.
    pub settings: crate::files::Settings,
    popup: Option<Popup>,
    /// A web row's hint to show as the app's own themed tooltip (MU26):
    /// its text and the control it is for, in window points.
    web_tip: Option<(String, egui::Rect)>,
    /// Keys pressed while a page had the keyboard, for the views.
    pub keys: Vec<egui::Event>,
    /// Ctrl, Shift or Alt came with the keys fed last frame, and are to
    /// be let go of now.
    keys_held: bool,
    /// A right-click menu was asked for from the 3D view this frame.
    pub menu_3d: bool,
    /// The open right-click menu came from the 3D view (else a 2D pane).
    pub(super) context_3d: bool,
    /// Give the popup the keyboard once it is shown (the palette).
    focus_popup: bool,
    /// The ground grid spacing being typed, while its window is open.
    pub ground_custom: Option<String>,
    /// The Help panel: open or not, and its page.
    pub help: super::helpui::HelpState,
    /// The tool options strip as its page last measured it, for what goes
    /// in its More menu.
    pub bar: Option<crate::optionbar::Measure>,
    /// The tool the tools row last showed its tab for: a tool picked by
    /// its key, off the tab shown, brings its own tab.
    tab_tool: Option<Tool>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            web: None,
            failed: None,
            classic: std::env::var_os("CREABIM_CLASSIC_UI").is_some(),
            theme: Theme::default(),
            settings: Default::default(),
            popup: None,
            web_tip: None,
            keys: vec![],
            keys_held: false,
            menu_3d: false,
            context_3d: false,
            focus_popup: false,
            ground_custom: None,
            help: Default::default(),
            bar: None,
            tab_tool: None,
        }
    }
}

impl Shell {
    /// Feed the keys pressed while a page had the keyboard to the views.
    /// egui keeps Ctrl, Shift and Alt from `ModifiersChanged` events, not
    /// from each key, and the window saw none of them go down while the
    /// page had the keyboard. So the keys' own modifiers are set for the
    /// frame they arrive in and let go of in the next: otherwise Ctrl+S
    /// over a view reads as S, which scales. True when a next frame is
    /// needed to let go of them.
    pub fn feed_keys(&mut self, raw: &mut egui::RawInput) -> bool {
        if std::mem::take(&mut self.keys_held) {
            // first, so a real change this frame still wins
            raw.events
                .insert(0, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        }
        let held = self.keys.iter().find_map(|e| match e {
            egui::Event::Key { modifiers, .. } if !modifiers.is_none() => Some(*modifiers),
            _ => None,
        });
        if let Some(m) = held {
            raw.events.push(egui::Event::ModifiersChanged(m));
            self.keys_held = true;
        }
        raw.events.append(&mut self.keys);
        self.keys_held
    }
}

#[derive(Clone)]
struct Popup {
    key: String,
    /// Top-left corner, in points.
    at: egui::Pos2,
    /// When the popup opens upwards, where its bottom goes.
    up: Option<f32>,
    /// The top of the button it opened from: a menu with no room below it
    /// opens upwards from there instead.
    above: Option<f32>,
    size: egui::Vec2,
}

pub(super) fn view_style_icon(s: ViewStyle) -> &'static str {
    match s {
        ViewStyle::HiddenLine => "render-hidden-line",
        ViewStyle::Shaded => "render-edges",
        ViewStyle::Wireframe => "render-wireframe",
    }
}

pub(crate) fn tool_icon(t: Tool) -> &'static str {
    match t {
        Tool::Select => "mouse-pointer-2",
        Tool::Wall => "wall",
        Tool::Door => "door",
        Tool::Window => "window",
        Tool::Opening => "opening",
        Tool::Slab => "slab",
        Tool::Shaft => "shaft",
        Tool::Mass => "box",
        Tool::Section => "scissors-line-dashed",
        Tool::Elevation => "elevation",
        Tool::Dimension => "dimension",
        Tool::Tag => "tag",
        Tool::Angle => "angle",
        Tool::Radius => "radius",
        Tool::Diameter => "diameter",
        Tool::ArcLength => "arc-length",
        Tool::Note => "message-square-text",
        Tool::SpotLevel => "spot-level",
        Tool::Sketch => "pen-tool",
        Tool::Guide => "guide",
        Tool::Grid => "grid-lines",
        Tool::Column => "column",
        Tool::Beam => "beam",
        Tool::Roof => "roof",
        Tool::Stair => "stair",
        Tool::Railing => "railing",
        Tool::Footing => "footing",
        Tool::Level => "level",
        Tool::Align => "align-start-vertical",
        Tool::Object => "object",
        Tool::Extrusion => "extrusion",
        Tool::Sweep => "sweep",
        Tool::Void => "void",
        Tool::Terrain => "terrain",
        Tool::Paving => "paving",
        Tool::Planting => "planting",
        Tool::Room => "room",
        Tool::RoomSeparator => "room-separator",
        Tool::Ceiling => "ceiling",
        Tool::Callout => "callout",
        Tool::Detail => "detail-component",
    }
}

fn tool_group(t: Tool) -> u8 {
    match t {
        Tool::Select | Tool::Align => 0,
        Tool::Wall
        | Tool::Door
        | Tool::Window
        | Tool::Opening
        | Tool::Slab
        | Tool::Shaft
        | Tool::Mass
        | Tool::Column
        | Tool::Beam
        | Tool::Roof
        | Tool::Stair
        | Tool::Railing
        | Tool::Footing
        | Tool::Object
        | Tool::Extrusion
        | Tool::Sweep
        | Tool::Void
        | Tool::Room
        | Tool::RoomSeparator
        | Tool::Ceiling => 1,
        Tool::Terrain | Tool::Paving | Tool::Planting => 5,
        Tool::Section | Tool::Elevation | Tool::Callout => 2,
        Tool::Dimension
        | Tool::Tag
        | Tool::Angle
        | Tool::Radius
        | Tool::Diameter
        | Tool::ArcLength
        | Tool::Note
        | Tool::Detail
        | Tool::SpotLevel => 3,
        Tool::Sketch | Tool::Guide | Tool::Grid | Tool::Level => 4,
    }
}

pub(super) fn icon_name(i: Icon) -> &'static str {
    match i {
        Icon::Storey => "layers-3",
        Icon::Grid => "grid-lines",
        Icon::Wall => "wall",
        Icon::Slab => "slab",
        Icon::Door => "door",
        Icon::Window => "window",
        Icon::Roof => "roof",
        Icon::Column => "column",
        Icon::Beam => "beam",
        Icon::Mass => "box",
        Icon::Material => "palette",
        Icon::Plan => "map",
        Icon::Section => "scissors-line-dashed",
        Icon::Elevation => "elevation",
        Icon::View3d => "view-3d",
        Icon::Schedule => "table-2",
        Icon::Sheet => "file-text",
        Icon::Viewport => "frame",
        Icon::Dimension => "dimension",
        Icon::Tag => "tag",
        Icon::Sketch => "pen-tool",
        Icon::Guide => "guide",
        Icon::CropBox => "crop-box",
    }
}

/// A plain name for a group of the project browser.
/// The project browser's group an element is listed in: every kind of
/// type in one, the rest by IFC class.
fn class_of(e: &Element) -> String {
    match e.kind {
        ElementKind::ElementType(_) => "ElementType".to_string(),
        _ => e.ifc_class().to_string(),
    }
}

fn group_title(class: &str) -> String {
    match class {
        "View" => "Views".into(),
        "Sheet" => "Sheets".into(),
        "Viewport" => "Viewports".into(),
        "IfcBuildingStorey" => "Levels".into(),
        "IfcBuildingElementProxy" => "Masses".into(),
        "Sketch" => "Sketches".into(),
        "IfcMaterial" => "Materials".into(),
        "IfcMaterialLayerSet" => "Assemblies".into(),
        "ElementType" => "Types".into(),
        "IfcOpeningElement" => "Openings and shafts".into(),
        "TextStyle" => "Text styles".into(),
        "CropBox" => "Crop boxes".into(),
        "IfcFurniture" => "Furniture and fixtures".into(),
        "IfcSpace" => "Rooms".into(),
        "IfcVirtualElement" => "Room separators".into(),
        "IfcCovering" => "Ceilings".into(),
        c => {
            let c = c.strip_prefix("Ifc").unwrap_or(c);
            if c.ends_with('s') {
                c.to_string()
            } else {
                format!("{c}s")
            }
        }
    }
}

/// The icon of a schedule of `c`.
fn schedule_icon(c: ScheduleCategory) -> &'static str {
    match c {
        ScheduleCategory::Doors => "door-open",
        ScheduleCategory::Windows => "app-window",
        ScheduleCategory::Walls => "brick-wall",
        ScheduleCategory::Slabs => "layers-2",
        ScheduleCategory::Structure => "rectangle-vertical",
        ScheduleCategory::Furniture => "sofa",
        ScheduleCategory::Fixtures => "fixtures",
        ScheduleCategory::Rooms => "room",
        ScheduleCategory::Materials => "table-2",
    }
}

/// The New schedule submenu, one entry a category.
fn schedule_items() -> Vec<Item> {
    ScheduleCategory::ALL
        .iter()
        .enumerate()
        .map(|(i, c)| Item::new(format!("new.schedule:{i}"), c.label()).icon(schedule_icon(*c)))
        .collect()
}

fn command_icon(c: Command) -> &'static str {
    match c {
        Command::Undo => "undo-2",
        Command::Redo => "redo-2",
        Command::Move => "move",
        Command::Rotate => "rotate-cw",
        Command::Scale => "scaling",
        Command::Mirror => "flip-horizontal-2",
        Command::Offset => "offset",
        Command::Duplicate => "duplicate",
        Command::Copy => "copy",
        Command::Cut => "scissors",
        Command::Paste => "clipboard-paste",
        Command::Delete => "trash-2",
        Command::Hide => "eye-off",
        Command::Isolate => "circle-dot-dashed",
        Command::ShowAll => "eye",
        Command::SelectAll => "square-dashed-mouse-pointer",
        Command::SelectNone => "square-dashed",
        Command::CursorHere => "crosshair",
        Command::WorkplaneHere => "square-dashed",
    }
}

/// The modifier keys held this frame (1 Shift, 2 Ctrl, 4 Alt), for the
/// status bar's mouse hints.
pub(super) static MODS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

impl CreaBimApp {
    fn cmd_item(&self, c: Command) -> Item {
        let enabled = match c {
            Command::Undo => self.doc.can_undo(),
            Command::Redo => self.doc.can_redo(),
            Command::Paste => !self.clipboard.is_empty(),
            Command::ShowAll => !self.hidden.is_empty(),
            c => self.selection.is_some() || !c.needs_selection(),
        };
        let label = match c {
            Command::Undo => self
                .doc
                .undo_label()
                .map_or("Undo".into(), |l| format!("Undo {l}")),
            Command::Redo => self
                .doc
                .redo_label()
                .map_or("Redo".into(), |l| format!("Redo {l}")),
            Command::Delete if self.selection.len() > 1 => {
                format!("Delete {} selected", self.selection.len())
            }
            c => c.label().into(),
        };
        let running = self
            .plan
            .modal
            .as_ref()
            .map(|m| m.mode)
            .or(self.plan.armed.map(|a| a.0))
            .is_some_and(|m| {
                matches!(
                    (m, c),
                    (crate::modal::Mode::Move, Command::Move)
                        | (crate::modal::Mode::Rotate, Command::Rotate)
                        | (crate::modal::Mode::Scale, Command::Scale)
                        | (crate::modal::Mode::Mirror, Command::Mirror)
                        | (crate::modal::Mode::Offset, Command::Offset)
                )
            });
        Item::new(format!("cmd:{c:?}"), label)
            .key(c.shortcut())
            .enabled(enabled)
            .icon(command_icon(c))
            .checked(running)
    }

    /// The items of a menu or dropdown, by its key.
    pub(super) fn menu_items(&self, key: &str) -> Vec<Item> {
        if let Some(items) = self.area_menu(key) {
            return items;
        }
        // a group of the folded browser's rail: its items, to pick (MU29)
        if let Some(group) = key.strip_prefix("rail:") {
            let state = self.left_state();
            let Some(g) = state["groups"]
                .as_array()
                .and_then(|gs| gs.iter().find(|g| g["key"].as_str() == Some(group)))
            else {
                return vec![];
            };
            let mut items = vec![];
            for r in g["rows"].as_array().into_iter().flatten().take(60) {
                items.push(
                    Item::new(
                        format!("railpick:{}", r["id"].as_str().unwrap_or("")),
                        r["name"].as_str().unwrap_or(""),
                    )
                    .icon(crate::svgicon::named(r["icon"].as_str().unwrap_or("")))
                    .checked(r["sel"].as_bool().unwrap_or(false)),
                );
            }
            return items;
        }
        match key {
            "menu:CreaBIM" => self.app_menu(),
            "units.menu" => self.unit_items(),
            "select.menu" => self.select_menu_items(),
            "sweep.library" => self.sweep_library_items(),
            "beam.corner" => creabim_core::members::BeamCorner::ALL
                .iter()
                .map(|c| {
                    Item::new(format!("beam.corner:{}", c.label()), c.label())
                        .icon(c.icon())
                        .hint(c.hint())
                        .checked(*c == self.plan.members.beam_corner)
                })
                .collect(),
            "railing.library" => self.library_items(
                "railing.lib",
                self.plan.members.railing_section.clone(),
                Some("Its own: a square bar"),
            ),
            "column.library" => self.library_items(
                "column.lib",
                self.plan.members.column_section.clone(),
                Some("Its own: square or round"),
            ),
            "beam.library" => self.library_items(
                "beam.lib",
                self.plan.members.beam_section.clone(),
                Some("Its own: width × depth"),
            ),
            "extrusion.library" => self.extrusion_library_items(),
            "menu:File" => self.file_menu(),
            "menu:Edit" => self.edit_menu(),
            "menu:View" => self.view_menu(),
            "menu:Materials" => self.materials_menu(),
            "menu:Help" => self.help_menu(),
            "menu:Create" => self.create_menu(),
            "tab:schedules" => schedule_items(),
            "tab:ifc" => vec![
                Item::new("open", "Open an IFC file…").icon("folder-open"),
                Item::Separator,
                Item::new("export.ifc4", "Export IFC4 (.ifc)…").icon("upload"),
                Item::new("export.ifcx", "Export IFC5 (.ifcx, alpha)…").icon("upload"),
            ],
            "underlay" => self.underlay_items(),
            "keeponly" => self.keep_only_items(),
            "render3d" => self.render_items(),
            "viewstyle" => self.view_style_items(),
            "wall.location" => self.wall_location_items(),
            "type.tool" => self.tool_type_items(),
            "palette" => self.palette_items(),
            "look" => crate::camera::Look::ALL
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    // a cube with the face looked at filled (MU29)
                    let icon = match l.label() {
                        "Top" => "look-top",
                        "Bottom" => "look-bottom",
                        "Front" => "look-front",
                        "Back" => "look-back",
                        "Right" => "look-right",
                        "Left" => "look-left",
                        _ => "look-iso",
                    };
                    Item::new(format!("nav.look:{i}"), l.label()).icon(icon)
                })
                .collect(),
            "views" => {
                let mut items = vec![];
                let model = self.doc.model();
                for (g, ids) in crate::browser::view_groups(model) {
                    items.push(Item::Header(g.title().into()));
                    for id in ids {
                        let Some(e) = model.get(id) else { continue };
                        items.push(
                            Item::new(format!("view:{}", id.0), e.name.clone())
                                .checked(self.active_view == Some(id))
                                .icon(icon_name(Icon::of(e))),
                        );
                    }
                }
                let sheets = self.sheet_list();
                if !sheets.is_empty() {
                    items.push(Item::Header("Sheets".into()));
                }
                for (id, name) in sheets {
                    items.push(
                        Item::new(format!("view:{}", id.0), name)
                            .checked(self.active_view == Some(id))
                            .icon("file-text"),
                    );
                }
                items
            }
            "placeview" => {
                let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) else {
                    return vec![];
                };
                let placed: Vec<ElementId> = self
                    .doc
                    .model()
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ElementKind::Viewport(vp) if vp.sheet == sheet => Some(vp.view),
                        _ => None,
                    })
                    .collect();
                self.views()
                    .into_iter()
                    .map(|(id, name, _)| {
                        let label = if placed.contains(&id) {
                            format!("{name} (placed)")
                        } else {
                            name
                        };
                        Item::new(format!("place:{}", id.0), label)
                    })
                    .collect()
            }
            "context" => {
                let n = self.selection.len();
                let ctx = |c: Command| match self.cmd_item(c) {
                    Item::Action {
                        label,
                        shortcut,
                        enabled,
                        icon,
                        ..
                    } => Item::new(format!("ctx:{c:?}"), label)
                        .key(shortcut)
                        .enabled(enabled)
                        .icon(icon),
                    other => other,
                };
                let mut items = vec![];
                if n > 0 {
                    items.push(Item::Header(match n {
                        1 => "1 selected".into(),
                        n => format!("{n} selected"),
                    }));
                    // a view placed on a sheet: its scale first
                    let placed = self.selection.active.and_then(|id| {
                        match &self.doc.model().get(id)?.kind {
                            ElementKind::Viewport(vp) => Some(vp.view),
                            _ => None,
                        }
                    });
                    if let Some(view) = placed.filter(|_| n == 1) {
                        let scale = self.view(view).map_or(0.0, |v| v.paper_scale());
                        if self
                            .view(view)
                            .is_some_and(|v| !matches!(v.kind, ViewKind::Schedule { .. }))
                        {
                            items.push(
                                Item::sub(
                                    format!("Scale 1:{scale:.0}"),
                                    self.viewport_scale_items(view),
                                )
                                .icon("ruler"),
                            );
                        }
                        items.push(
                            Item::new(format!("view:{}", view.0), "Open the view itself")
                                .icon("map"),
                        );
                        items.push(Item::Separator);
                    }
                    // cutting it up first, where they are found (MU39): Split
                    // for anything, Divide for what has a length
                    items.push(Item::new("split", "Split…").icon("split").hint(
                        "Cut it along a guide, a line or two points you click; keep the side you click",
                    ));
                    if self.selection.active.is_some_and(|id| {
                        creabim_core::divide::span(self.doc.model(), id).is_some()
                    }) {
                        items.push(
                            Item::new("divide", "Divide…")
                                .icon("mod-array")
                                .hint("Into parts along it, up it or both: cuts, guides or copies"),
                        );
                    }
                    items.push(Item::Separator);
                    items.extend(self.wall_menu_items());
                    items.extend(self.join_menu_items());
                    items.extend(self.level_menu_items());
                    for group in [
                        &[
                            Command::Move,
                            Command::Rotate,
                            Command::Scale,
                            Command::Mirror,
                            Command::Offset,
                            Command::Duplicate,
                        ][..],
                        &[Command::Copy, Command::Cut, Command::Delete],
                        &[Command::Hide, Command::Isolate],
                    ] {
                        items.extend(group.iter().map(|c| ctx(*c)));
                        items.push(Item::Separator);
                    }
                    if !self.shell.context_3d {
                        items.push(align_item());
                        items.push(Item::Separator);
                    }
                    items.extend(self.hide_in_view_items(self.shell.context_3d));
                    items.extend(self.similar_items());
                    items.push(Item::Separator);
                }
                items.extend([
                    ctx(Command::Paste),
                    ctx(Command::ShowAll),
                    Item::Separator,
                    ctx(Command::SelectAll),
                ]);
                if n > 0 {
                    items.push(ctx(Command::SelectNone));
                }
                items.push(Item::Separator);
                items.push(ctx(Command::CursorHere));
                items.push(ctx(Command::WorkplaneHere));
                items
            }
            "pivot" => {
                let mut items: Vec<Item> = crate::modal::Pivot::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        Item::new(format!("pivot:{i}"), p.label()).checked(self.plan.pivot == *p)
                    })
                    .collect();
                items.push(Item::Separator);
                items.push(
                    Item::new("cursor.clear", "Clear the pivot cursor")
                        .enabled(self.plan.cursor.is_some()),
                );
                items
            }
            "workplane" => {
                let mut items = vec![
                    Item::new("wp.level", "Level (follows the plan)")
                        .checked(self.workplane.is_none()),
                    Item::new("wp.face", "Pick a face…").icon("square-dashed-mouse-pointer"),
                    Item::new("wp.points", "Through three points…").icon("spline"),
                ];
                if let Some((_, z)) = self.work_level() {
                    items.push(
                        Item::new("wp.offset", "Level plane you can offset")
                            .icon("level-offset")
                            .hint(format!(
                                "A plane at {} to slide up and down",
                                crate::units::elevation(z)
                            )),
                    );
                }
                items
            }
            "step" => self.step_items(),
            "snaps" => {
                let mut s = self.plan.snap;
                let mut items = vec![
                    Item::new("snap.on", "Snapping")
                        .key("F3")
                        .checked(s.on)
                        .keep_open()
                        .hint("Hold Alt while picking to suspend snaps for a moment"),
                    Item::Separator,
                    Item::new("snap.all", "Turn all on").keep_open(),
                    Item::new("snap.none", "Turn all off").keep_open(),
                    Item::Separator,
                ];
                const ICONS: [&str; 12] = [
                    "snap-endpoint",
                    "snap-midpoint",
                    "snap-intersection",
                    "snap-perpendicular",
                    "snap-nearest",
                    "snap-centre",
                    "snap-quadrant",
                    "hash",
                    "slash",
                    "snap-axis",
                    "snap-track",
                    "ruler",
                ];
                for (i, (label, v)) in s.switches().into_iter().enumerate() {
                    items.push(
                        Item::new(format!("snap:{i}"), label)
                            .checked(*v)
                            .icon(ICONS.get(i).copied().unwrap_or(""))
                            .keep_open()
                            .enabled(self.plan.snap.on),
                    );
                }
                items.push(Item::Separator);
                items.push(Item::sub(
                    format!("Grid snap: {}", self.plan.snap.step_label()),
                    self.step_items(),
                ));
                // every edge at its even parts (MU34 B1)
                let now = crate::snaps::divisions();
                let mut div = vec![Item::new("snap.div:0", "Off").checked(now == 0)];
                for n in [3u8, 4, 5, 6, 8, 10] {
                    div.push(
                        Item::new(format!("snap.div:{n}"), format!("{n} parts")).checked(now == n),
                    );
                }
                items.push(Item::sub(
                    if now == 0 {
                        "Divisions: off".to_string()
                    } else {
                        format!("Divisions: {now}")
                    },
                    div,
                ));
                items
            }
            key => self
                .drafting_menu(key)
                .or_else(|| self.grid_vis_menu(key))
                .unwrap_or_default(),
        }
    }

    fn file_menu(&self) -> Vec<Item> {
        let mut recent: Vec<Item> = self
            .recent
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| Item::new(format!("recent:{i}"), file_label(f)).hint(f.clone()))
            .collect();
        if !recent.is_empty() {
            recent.push(Item::Separator);
            recent.push(Item::new("recent.clear", "Clear the list"));
        }
        let mut items = vec![
            Item::new("new.project", "New project…")
                .key("Ctrl+N")
                .icon("file-plus")
                .hint("Start empty or from the starter house, or open a sample made for learning"),
            Item::new("new.sheet", "New sheet").icon("file-text"),
            Item::sub("New schedule", schedule_items()).icon("table-2"),
            Item::Separator,
            Item::new("open", "Open…")
                .key("Ctrl+O")
                .icon("folder-open")
                .hint("Open a .creabim, .ifc or .ifcx file. Other files (.blend, .FCStd) are linked into this project."),
            Item::sub("Open recent", recent).icon("folder"),
        ];
        if self.shell.web.is_none() {
            items.push(
                Item::new("open.path", "Open the path in the File box")
                    .icon("folder-open")
                    .hint("For typing or pasting a path instead of browsing."),
            );
        }
        items.extend([
            Item::new("save", "Save")
                .key("Ctrl+S")
                .icon("save")
                .hint("Save as a .creabim project file, with its undo history."),
            Item::new("saveas", "Save as…").icon("save"),
            Item::Separator,
            Item::Header("Project".into()),
            Item::new("units", "Project units…")
                .icon("units")
                .hint("Metric or imperial: how lengths are shown and typed, in dimensions, schedules and on sheets"),
            Item::new("levels", "Levels…")
                .icon("level")
                .hint("Add, duplicate, rename, move and delete levels"),
            Item::new("marks", "Mark numbering…")
                .icon("tag")
                .hint("How doors, windows and other elements are numbered: prefixes, digits, level-based marks, order. Renumber, and check for marks used twice."),
            Item::new("graphicstyles", "Graphic styles…")
                .icon("align-left")
                .hint("Style packs: pens by scale, object styles, hatch patterns and view templates. Load an office's pack to restyle every drawing."),
            Item::new("styles", "Annotation styles…")
                .icon("anno-styles")
                .hint("Named styles for dimensions, notes and tags: text, ends, tag shape, colour"),
            Item::new("templates", "Sheet templates…")
                .icon("sheet-template")
                .hint("Title blocks for sheets: change the built-in one, or import one from DXF, SVG or another project"),
            Item::Separator,
            Item::Header("Linked files".into()),
            Item::new("link", "Import and link…")
                .icon("link")
                .hint("Bring in the objects of a .blend or .FCStd file. Saving it in Blender or FreeCAD updates them here."),
            Item::new("live", "Reload linked files when saved").checked(self.live_reload),
        ]);
        if self.import_report.is_some() {
            items.push(
                Item::new("import.report", "Import report")
                    .icon("file-text")
                    .hint("What the last IFC import brought in as native elements and as meshes, and why, element by element. It can be copied."),
            );
        }
        let linked = crate::files::linked_files(self.doc.model());
        if !linked.is_empty() {
            items.push(
                Item::sub(
                    "Reload a linked file",
                    linked
                        .iter()
                        .enumerate()
                        .map(|(i, f)| Item::new(format!("reload:{i}"), file_label(f)))
                        .collect(),
                )
                .icon("refresh-cw"),
            );
        }
        items.extend([
            Item::Separator,
            Item::Header("Export".into()),
            Item::new("export.ifc4", "IFC4 (.ifc)").icon("upload"),
            Item::new("export.ifcx", "IFC5 (.ifcx, alpha)")
                .icon("upload")
                .hint("The new JSON-based IFC. Its schema is still an alpha, so other tools may not read it yet."),
            Item::new("export.glb", "glTF (.glb) for Blender")
                .icon("box")
                .hint("Every element as a mesh with its CreaBIM id, so edits saved in Blender come back to the same elements."),
            Item::new("export.fcstd", "FreeCAD (.FCStd)")
                .icon("freecad")
                .hint("Every element as a solid with its BIM type and CreaBIM id; open it in FreeCAD, edit, save, and link it back."),
            Item::new("export.pdf", "Sheets as PDF").icon("file-text"),
            Item::new("export.svg", "This view or sheet as SVG").icon("file"),
            Item::new("export.xlsx", "Schedules to Excel (.xlsx)")
                .icon("table-2")
                .hint("A summary, then a sheet for each schedule, with numbers as numbers so the spreadsheet can sum them."),
        ]);
        items
    }

    fn edit_menu(&self) -> Vec<Item> {
        let mut items = vec![];
        for group in [
            &[Command::Undo, Command::Redo][..],
            &[
                Command::Move,
                Command::Rotate,
                Command::Scale,
                Command::Mirror,
                Command::Offset,
                Command::Duplicate,
            ],
            &[Command::Copy, Command::Cut, Command::Paste, Command::Delete],
            &[Command::SelectAll, Command::SelectNone],
            &[Command::Hide, Command::Isolate, Command::ShowAll],
        ] {
            items.extend(group.iter().map(|c| self.cmd_item(*c)));
            if group.contains(&Command::SelectNone) {
                items.extend(self.similar_items());
            }
            if group.contains(&Command::Paste) {
                items.push(
                    Item::sub("Paste aligned to level", self.paste_level_items())
                        .icon("copy-levels"),
                );
            }
            // cutting up, after the edits (MU39): found in the menu and by
            // typing, not only on a right-click
            if group.contains(&Command::Duplicate) {
                let some = !self.selection.is_empty();
                let span = self
                    .selection
                    .active
                    .is_some_and(|id| creabim_core::divide::span(self.doc.model(), id).is_some());
                items.push(
                    Item::new("split", "Split…")
                        .icon("split")
                        .enabled(some)
                        .hint("Cut the selection along a guide, a line or two points; keep the side you click"),
                );
                items.push(
                    Item::new("divide", "Divide…")
                        .icon("mod-array")
                        .enabled(span)
                        .hint("Into parts along it, up it or both: cuts, guides or copies"),
                );
            }
            items.push(Item::Separator);
        }
        let levels = self.level_menu_items();
        if !levels.is_empty() {
            items.extend(levels);
            items.push(Item::Separator);
        }
        items.push(align_item());
        items.push(Item::Separator);
        let mut pivot = self.menu_items("pivot");
        pivot.insert(0, Item::Header("Rotate and scale about".into()));
        items.push(Item::sub("Pivot", pivot).icon("crosshair"));
        items
    }

    /// Everything that can be placed or made, by kind, as in the tool
    /// palette, with the views and sheets the File menu can also make.
    fn create_menu(&self) -> Vec<Item> {
        let mut items = vec![];
        for (group, title) in [
            (1, "Building elements"),
            (2, "Views"),
            (5, "Site"),
            (3, "Annotation"),
            (4, "Sketching"),
        ] {
            items.push(Item::Header(title.into()));
            for (i, t) in Tool::ALL.iter().enumerate() {
                if tool_group(*t) == group {
                    items.push(
                        Item::new(format!("tool:{i}"), t.label())
                            .icon(tool_icon(*t))
                            .hint(t.hint()),
                    );
                }
            }
            if group == 1 {
                items.push(
                    Item::new("tool.curtain", "Curtain wall")
                        .icon("curtain-wall")
                        .hint("The Wall tool drawing a curtain wall type: mullions and glass on a grid"),
                );
                items.push(
                    Item::new("tool.screen", "Screen wall")
                        .icon("screen-wall")
                        .hint("The Wall tool drawing a screen wall type: breeze blocks, louvres or a lattice"),
                );
                items.push(
                    Item::new("catalogue", "Catalogue…")
                        .icon("sofa")
                        .hint("Furniture and fixtures to place: bathroom, kitchen, living, dining, bedroom and office, and your own from Blender, FreeCAD or glTF"),
                );
                items.push(
                    Item::new("types.browser", "Types…")
                        .icon("element-types")
                        .hint(
                            "Wall, curtain wall, door, window, slab, roof, column, beam and object types",
                        ),
                );
                items.push(
                    Item::new("rooms.fromnotes", "Rooms from room notes")
                        .icon("room")
                        .hint("Turn room labels written as notes (a name over an area) into rooms with tags, wherever walls close them off"),
                );
                items.push(
                    Item::new("check.doorways", "Check doorways")
                        .icon("door-open")
                        .hint("List and select the furniture and fixtures standing in a doorway, a window or a door's swing"),
                );
                items.push(
                    Item::new("check.layers", "Check layer joins")
                        .icon("layers")
                        .hint("List and select the walls with a layer held back from a join (a gap) or run on into the wall it meets (an overlap)"),
                );
                items.push(
                    Item::new("level.add", "Level above the top one")
                        .icon("level")
                        .hint("A new level one storey up, with its floor plan"),
                );
                items.push(
                    Item::new("levels", "Levels…")
                        .icon("level")
                        .hint("Add, duplicate, rename, move and delete levels"),
                );
            }
            if group == 5 {
                items.push(
                    Item::new("catalogue.entourage", "Cars and people…")
                        .icon("car")
                        .hint("Entourage from the catalogue: a car, people, for scale"),
                );
            }
            if group == 2 {
                items.push(Item::new("new.sheet", "Sheet").icon("file-text"));
                items.push(
                    Item::new("new.script", "Script")
                        .icon("file-plus")
                        .hint("A script block from a starter template: INPUTS in, OUTPUTS out"),
                );
                items.push(
                    Item::new("new.3dview", "3D view")
                        .icon("cuboid")
                        .hint("A saved 3D view to orbit: drag it round, place it on a sheet"),
                );
                items.push(Item::sub("Schedule", schedule_items()).icon("table-2"));
            }
        }
        items
    }

    /// Every action of the menus in one list, with its menu path, for the
    /// command palette.
    fn palette_items(&self) -> Vec<Item> {
        fn flatten(items: Vec<Item>, path: &str, out: &mut Vec<Item>) {
            for item in items {
                match item {
                    Item::Action {
                        id,
                        label,
                        shortcut,
                        hint,
                        enabled,
                        icon,
                        ..
                    } if enabled
                        && !out
                            .iter()
                            .any(|o| matches!(o, Item::Action { id: i, .. } if *i == id)) =>
                    {
                        out.push(
                            Item::new(id, format!("{path}{label}"))
                                .key(shortcut)
                                .hint(hint)
                                .icon(icon),
                        )
                    }
                    Item::Sub { label, items, .. } => {
                        flatten(items, &format!("{path}{label} › "), out)
                    }
                    _ => {}
                }
            }
        }
        let mut out = vec![];
        flatten(self.create_menu(), "Create › ", &mut out);
        flatten(self.edit_menu(), "", &mut out);
        flatten(self.view_menu(), "View › ", &mut out);
        flatten(self.file_menu(), "File › ", &mut out);
        flatten(self.materials_menu(), "Materials › ", &mut out);
        flatten(self.help_menu(), "Help › ", &mut out);
        // what the tool tabs hold that the menus don't (MU39: Profiles, on
        // Manage, could not be found by typing)
        for (tab, name) in crate::tooltabs::TABS.iter().enumerate() {
            for e in crate::tooltabs::entries(tab) {
                use crate::tooltabs::Entry;
                match e {
                    Entry::Act {
                        id,
                        label,
                        icon,
                        hint,
                    } => flatten(
                        vec![Item::new(id, label).icon(icon).hint(hint)],
                        &format!("{name} › "),
                        &mut out,
                    ),
                    Entry::Menu { key, label, .. } => flatten(
                        self.menu_items(key),
                        &format!("{name} › {label} › "),
                        &mut out,
                    ),
                    Entry::Tool(_) | Entry::Sep => {}
                }
            }
        }
        // every page of the guide, found by its title
        for (section, id, md) in crate::help::PAGES {
            let item = Item::new(format!("help:{id}"), crate::help::title(md))
                .icon("book-open")
                .hint(*section);
            flatten(vec![item], "Help › ", &mut out);
        }
        flatten(self.menu_items("views"), "Open › ", &mut out);
        // and the model's elements, by name: picking one selects and frames it
        for e in self.doc.model().iter() {
            if matches!(
                e.kind,
                ElementKind::Units(_) | ElementKind::View(_) | ElementKind::Sheet(_)
            ) {
                continue;
            }
            out.push(
                Item::new(
                    format!("find:{}", e.id.0),
                    format!("Find › {} › {}", group_title(&class_of(e)), e.name),
                )
                .icon(icon_name(Icon::of(e)))
                .hint("Select it and frame it in the 3D view"),
            );
        }
        out
    }

    /// How many elements the project has, leaving out its units, which
    /// every project has once.
    pub(super) fn element_count(&self) -> usize {
        self.doc
            .model()
            .iter()
            .filter(|e| !matches!(e.kind, ElementKind::Units(_)))
            .count()
    }

    /// The 3D view's navigation buttons.
    pub(super) fn nav_state(&self) -> Value {
        let b = |id: &str, icon: &str, hint: &str, on: bool| json!({"id": id, "icon": icon, "hint": hint, "on": on});
        let sel = self.selection.is_some();
        let mut fitsel = b("nav.fitsel", "scan", "Frame the selection (.)", false);
        fitsel["off"] = json!(!sel);
        json!({
            "theme": self.theme(),
            "popup": self.popup_key(),
            "buttons": [
                b("frame", "frame-all", "Frame all (Home)", false),
                fitsel,
                b("nav.orbitsel", "rotate-3d", "Orbit around the selection", self.orbit_selection),
                b("nav.zoomin", "zoom-in", "Zoom in", false),
                b("nav.zoomout", "zoom-out", "Zoom out", false),
                b("nav.ortho", if self.camera.ortho { "box" } else { "cuboid" },
                  if self.camera.ortho { "Orthographic: click for perspective" } else { "Perspective: click for orthographic" },
                  self.camera.ortho),
                b("ground.grid", "grid-3x3",
                  if self.shell.settings.ground_grid { "Ground grid shown: click to hide it" } else { "Ground grid hidden: click to show it" },
                  self.shell.settings.ground_grid),
                b("xray", "xray",
                  if self.shell.settings.xray { "X-ray on: click for solid surfaces (Alt+Z)" } else { "X-ray: see through walls, slabs and roofs to what is inside them (Alt+Z)" },
                  self.shell.settings.xray),
            ],
            "menus": [
                {"key": "vis3d", "icon": if self.vis3d.is_empty() { "eye" } else { "eye-off" },
                 "label": "Show", "on": !self.vis3d.is_empty(),
                 "hint": "What the 3D view shows: grids, annotation, guides, sketches and every category"},
                {"key": "render3d", "icon": self.shell.settings.render_3d.icon(), "label": "",
                 "hint": format!("How the 3D view draws: {}. Shaded, with edges, hidden line or wireframe", self.shell.settings.render_3d.label())},
            ],
            "look": {"key": "look", "hint": "Look from the top, a side or the corner"},
        })
    }

    /// The View menu: where the 2D view goes and how the 3D view looks,
    /// with the longer lists in submenus so it stays short.
    pub(super) fn view_menu(&self) -> Vec<Item> {
        let place = |p: PlanPlacement, i: u8, label: &str| {
            Item::new(format!("placement:{i}"), label).checked(self.placement == p)
        };
        let layout = self.area_menu("workspaces").unwrap_or_default();
        let now = ground_spacing(self.shell.settings.ground_grid_spacing);
        let custom = !GROUND_SPACINGS.iter().any(|s| (now - s).abs() < 1e-9);
        let mut ground = vec![Item::new("ground.grid", "Show the ground grid")
            .checked(self.shell.settings.ground_grid)
            .hint(
                "The grid on the working level (or the workplane), also on the 3D view's buttons",
            )];
        ground.push(Item::Separator);
        ground.push(Item::Header("Spacing".into()));
        ground.extend(GROUND_SPACINGS.iter().enumerate().map(|(i, s)| {
            Item::new(format!("ground.spacing:{i}"), format!("{s} m"))
                .checked((now - s).abs() < 1e-9)
                .keep_open()
        }));
        ground.push(
            Item::new(
                "ground.custom",
                if custom {
                    format!("Custom: {}…", crate::units::len(now))
                } else {
                    "Custom…".to_string()
                },
            )
            .checked(custom)
            .hint("Type any spacing, in the project's units"),
        );
        let mut camera = vec![
            Item::new("nav.orbitsel", "Orbit around the selection")
                .checked(self.orbit_selection)
                .hint(
                    "Orbiting swings round the selected elements instead of the middle of the view",
                ),
            Item::new("nav.ortho", "Orthographic")
                .checked(self.camera.ortho)
                .hint("Parallel projection, without perspective"),
            Item::Separator,
            Item::Header("Look from".into()),
        ];
        camera.extend(self.menu_items("look"));
        let mut themes: Vec<Item> = Theme::ALL
            .into_iter()
            .map(|t| {
                Item::new(format!("theme:{}", t.id()), t.label())
                    .checked(self.shell.theme == t)
                    .keep_open()
            })
            .collect();
        themes.push(Item::Separator);
        themes.push(
            Item::new("theme.next", "Next theme")
                .key("Ctrl+Shift+T")
                .icon("palette"),
        );
        vec![
            Item::Header("2D view".into()),
            place(PlanPlacement::Docked, 0, "Docked beside 3D"),
            place(PlanPlacement::Window, 1, "In its own window"),
            place(PlanPlacement::Overlay, 2, "Overlaid in the 3D workspace"),
            place(PlanPlacement::Hidden, 3, "Hidden"),
            Item::sub("Workspace", layout).icon("workspace"),
            Item::new("plan3d", "Show the 2D view in 3D space").checked(self.plan_in_3d),
            Item::sub("Underlay", self.underlay_items())
                .icon("layers")
                .enabled(self.active_plan().is_some()),
            Item::new("levels", "Levels…").icon("level").hint(
                "Add, rename, move and delete levels, and make their floor plans, from any view",
            ),
            Item::Separator,
            Item::Header("3D view".into()),
            Item::new("frame", "Frame all")
                .key("Home")
                .icon("frame-all"),
            Item::new("nav.fitsel", "Frame the selection")
                .key(".")
                .icon("scan")
                .enabled(self.selection.is_some()),
            Item::sub(
                "3D view visibility",
                self.visibility_items(super::gridvis::VisTarget::Window3d),
            )
            .icon("eye"),
            Item::new("xray", "X-ray")
                .key("Alt+Z")
                .checked(self.shell.settings.xray)
                .icon("xray")
                .hint("Walls, slabs, roofs and masses see-through, so beams, columns and what is inside them show"),
            Item::sub("Render", self.render_items()).icon(self.shell.settings.render_3d.icon()),
            Item::sub("Crop boxes", self.crop_menu()).icon("crop-box"),
            Item::sub("Camera", camera).icon("axis-3d"),
            Item::sub("Ground grid", ground).icon("grid-3x3"),
            Item::Separator,
            Item::Header("Every view".into()),
            Item::new("guides.toggle", "Guides")
                .key("Shift+G")
                .checked(!self.shell.settings.guides_hidden)
                .icon("slash")
                .hint("Hide the guides in every view without deleting them, and bring them back"),
            Item::sub(
                "2D view visibility",
                self.vis_target(false)
                    .map(|t| self.visibility_items(t))
                    .unwrap_or_default(),
            )
            .icon("eye"),
            Item::Separator,
            Item::Header("Interface".into()),
            Item::sub("Theme", themes).icon("palette"),
            Item::new("palette", "Command palette")
                .key("Ctrl+K")
                .icon("search"),
            Item::new("ui.classic", "Classic panels")
                .checked(self.shell.classic)
                .hint("The plain built-in panels instead of the new interface"),
        ]
    }

    /// The Step choices: Off, three in the project's units, and Custom.
    pub(super) fn step_items(&self) -> Vec<Item> {
        let s = self.plan.snap;
        let steps: &[u32] = if crate::units::get().is_imperial() {
            &crate::snaps::STEPS_IMPERIAL
        } else {
            &crate::snaps::STEPS_METRIC
        };
        let mut items = vec![
            Item::Header("Grid snap (Alt+S)".into()),
            Item::new("step.off", "Off: free")
                .checked(!s.increment)
                .hint("Points and drags go exactly where the pointer is; other snaps still catch them when near"),
        ];
        for um in steps {
            items.push(
                Item::new(format!("step:{um}"), crate::units::len(*um as f64 / 1e6))
                    .checked(s.increment && s.step_um == *um),
            );
        }
        let custom = s.increment && !steps.contains(&s.step_um);
        items.push(
            Item::new("step.custom", "Custom…")
                .checked(custom)
                .hint("Type any spacing: 25 mm, 1/4\", 2 cm"),
        );
        items.push(Item::Separator);
        items.push(
            Item::new(
                "marks.toggle",
                format!("Distance marks every {}", s.spacing_label()),
            )
            .checked(s.marks)
            .hint("While dragging, faint marks from where you grabbed; a mark holds the drag only when the pointer is near it"),
        );
        items
    }

    /// Open the Foundation plan, or make one first: a level 600 mm below the
    /// lowest, its plan showing the level above as underlay (A3-1).
    pub(super) fn show_foundation_plan(&mut self) {
        self.plan.footing_placed = false;
        if let Some(v) = crate::memberstool::foundation_plan(self.doc.model()) {
            self.active_view = Some(v);
            self.status = "Foundation plan: the footings, under the floor above.".into();
            return;
        }
        let lowest = creabim_core::levels::sorted(self.doc.model())
            .first()
            .map_or(0.0, |(_, _, l)| l.elevation);
        let level = Element::new(
            "Foundation",
            ElementKind::Level(creabim_core::Level {
                elevation: lowest - 0.6,
                height: 0.6,
            }),
        );
        let mut view = creabim_core::View::plan(level.id);
        view.underlay = creabim_core::Underlay::Above;
        let view = Element::new("Foundation plan", ElementKind::View(view));
        let plan = view.id;
        let tx = creabim_core::Transaction::new("Make a Foundation plan")
            .insert(level)
            .insert(view);
        match self.doc.commit(tx) {
            Ok(()) => {
                self.active_view = Some(plan);
                self.status = format!(
                    "Made a Foundation plan at {}, with the floor above as underlay.",
                    crate::units::elevation(lowest - 0.6)
                );
            }
            Err(e) => self.status = format!("Couldn't make a Foundation plan: {e}"),
        }
    }

    /// Run an action picked in a menu, a toolbar or the project browser.
    pub(super) fn run_action(&mut self, ctx: &egui::Context, id: &str) {
        let (head, arg) = id.split_once(':').unwrap_or((id, ""));
        let index = arg.parse::<usize>().ok();
        let by_uuid = |s: &Self| {
            s.doc
                .model()
                .iter()
                .find(|e| e.id.0.to_string() == arg)
                .map(|e| e.id)
        };
        if self.area_action(head, arg) {
            return;
        }
        match head {
            "cmd" => {
                if let Some(c) = Command::ALL.into_iter().find(|c| format!("{c:?}") == arg) {
                    self.run_command(ctx, c, None);
                }
            }
            "ctx" => {
                let Some(c) = Command::ALL.into_iter().find(|c| format!("{c:?}") == arg) else {
                    return;
                };
                let at = if self.shell.context_3d {
                    self.menu_at_3d
                } else {
                    self.plan.menu_at
                };
                match (c, c.modal()) {
                    (_, Some(m)) => self.arm(c, m),
                    (Command::WorkplaneHere, None) if self.shell.context_3d => match self
                        .menu_plane_3d
                    {
                        Some(plane) => self.set_workplane(plane, "on a face".into()),
                        None => {
                            self.status = "Right-click a face to put the workplane on it.".into()
                        }
                    },
                    _ => self.run_command(ctx, c, at),
                }
            }
            "tab" => {
                if let Some(i) = index.filter(|i| *i < crate::tooltabs::TABS.len()) {
                    self.shell.settings.tools_tab = i;
                    self.shell.settings.store();
                }
            }
            "tool" => {
                if let Some(why) = index
                    .and_then(|i| Tool::ALL.get(i))
                    .and_then(|t| self.tool_unusable(*t).map(|w| (t.label(), w)))
                {
                    self.status = format!("{}: not here, {}.", why.0, why.1);
                    return;
                }
                if let Some(t) = index.and_then(|i| Tool::ALL.get(i)) {
                    // a detail path being drawn is kept, not dropped
                    crate::detailtool::keep(&mut self.doc, &mut self.plan, &mut self.selection);
                    self.plan.tool = *t;
                    self.plan.cancel();
                    // a chain begun in 3D ends with its tool (MU40)
                    self.pending_3d = None;
                    self.beam_face = None;
                    self.plan.members.last_beam = None;
                    self.status = t.hint().into();
                    if *t == Tool::Guide && self.shell.settings.guides_hidden {
                        // drawing guides you can't see helps no one
                        self.set_guides_hidden(false);
                        self.status = format!("Guides shown again. {}", t.hint());
                    }
                    if *t == Tool::Level && !self.shows_section() {
                        // level lines need a section or elevation; anywhere
                        // else the levels are edited in their window
                        self.open_levels();
                        self.status = "Levels window: add, rename and move levels here. To draw a level line, open a section or elevation.".into();
                    }
                }
            }
            "view" => {
                if let Some(v) = by_uuid(self) {
                    self.active_view = Some(v);
                }
            }
            "place" => {
                if let (Some(view), Some(sheet)) = (
                    by_uuid(self),
                    self.active_view.filter(|id| self.is_sheet(*id)),
                ) {
                    self.place_view(sheet, view);
                }
            }
            "find" => {
                if let Some(id) = by_uuid(self) {
                    self.selection.only(id);
                    self.frame_selection();
                    self.status = format!(
                        "{} selected and framed.",
                        crate::hoverpick::describe(self.doc.model(), id)
                    );
                }
            }
            // a link to a website, from the guide: the system's browser
            "url" if arg.starts_with("https://") || arg.starts_with("http://") => {
                ctx.open_url(egui::OpenUrl::new_tab(arg));
                self.status = format!("Opened {arg} in your browser.");
            }
            "vpscale" => {
                // the scale of the view placed in the selected viewport
                let view =
                    self.selection
                        .active
                        .and_then(|id| match &self.doc.model().get(id)?.kind {
                            ElementKind::Viewport(vp) => Some(vp.view),
                            _ => None,
                        });
                if let (Some(view), Ok(scale)) = (view, arg.parse::<f64>()) {
                    self.set_view_scale(view, scale);
                }
            }
            "pivot" => {
                if let Some(p) = index.and_then(|i| crate::modal::Pivot::ALL.get(i)) {
                    self.plan.pivot = *p;
                }
            }
            "cursor.clear" => self.plan.cursor = None,
            "wp.level" => {
                self.workplane = None;
                self.geometry_key = None;
                self.status = "Workplane: the working level.".into();
            }
            "wp.face" => {
                self.plane_pick = Some(PlanePick::Face);
                self.status = "Click a face in the 3D view to put the workplane on it.".into();
            }
            "wp.points" => {
                self.plane_pick = Some(PlanePick::Points(vec![]));
                self.status = "Click three points in the 3D view for the workplane.".into();
            }
            "wp.offset" => {
                if let Some((_, z)) = self.work_level() {
                    self.set_workplane(
                        Workplane::ground(z),
                        format!("level at {}", crate::units::elevation(z)),
                    );
                }
            }
            "snap.on" => self.plan.snap.on = !self.plan.snap.on,
            "step.off" => {
                self.plan.snap.increment = false;
                self.status =
                    "Grid snap off: points and drags go exactly where the pointer is.".into();
            }
            "marks.toggle" => {
                let s = &mut self.plan.snap;
                s.marks = !s.marks;
                self.status = if s.marks {
                    format!(
                        "Distance marks every {} while dragging: near one, the drag holds on it.",
                        s.spacing_label()
                    )
                } else {
                    "Distance marks off.".into()
                };
                self.shell.settings.distance_marks = s.marks;
                self.shell.settings.store();
            }
            "step.custom" => {
                self.plan.step_prompt =
                    Some(crate::units::len(self.plan.snap.step_um as f64 / 1e6));
            }
            "step" => {
                if let Some(um) = index.and_then(|i| u32::try_from(i).ok()) {
                    self.plan.snap.increment = true;
                    self.plan.snap.step_um = um;
                    self.status = format!(
                        "Grid snap {}: points land on its grid. Alt+S turns it off; Alt frees one move.",
                        self.plan.snap.step_label()
                    );
                    self.shell.settings.snap_spacing_um = um;
                    self.shell.settings.store();
                }
            }
            "snap.div" => {
                let n = index.unwrap_or(0).min(20) as u8;
                crate::snaps::set_divisions(n);
                self.plan.forget_snaps();
                self.snaps_3d = None;
                self.status = if n == 0 {
                    "Divisions off.".into()
                } else {
                    format!("Divisions: every edge snaps at its {n} even parts while you place or draw anything.")
                };
            }
            "snap.all" | "snap.none" => {
                let on = head == "snap.all";
                self.plan
                    .snap
                    .switches()
                    .into_iter()
                    .for_each(|(_, v)| *v = on);
            }
            "snap" => {
                if let Some((_, v)) =
                    index.and_then(|i| self.plan.snap.switches().into_iter().nth(i))
                {
                    *v = !*v;
                }
            }
            "new.layered" => {
                self.path = new_path("Layered walls");
                self.load_model(creabim_core::sample::layered_house());
                self.status = "Loaded the layered sample house.".into();
            }
            "new.typed" => {
                self.path = new_path("Wall types");
                self.load_model(creabim_core::sample::typed_house());
                self.status =
                    "Loaded the typed sample house: see Materials › Types… for its types.".into();
            }
            "new.furnished" => {
                self.path = new_path("Furnished house");
                self.load_model(creabim_core::sample::furnished_house());
                self.status =
                    "Loaded the furnished sample house: see Create › Catalogue… to place more."
                        .into();
            }
            "new.garden" => {
                self.path = new_path("House in its garden");
                self.load_model(creabim_core::garden::garden_house());
                self.status =
                    "Loaded the garden sample: see Create › Site and Create › Building elements."
                        .into();
            }
            "new.sample" => {
                self.path = new_path("Starter house");
                self.load_model(creabim_core::starter::starter_house());
                self.status = STARTER_LOADED.into();
            }
            "new.simple" => {
                self.path = new_path("Simple house");
                self.load_model(sample_house());
                self.status = "Loaded the simple sample house.".into();
            }
            "new.buried" => {
                self.path = new_path("Beam in a wall");
                self.load_model(creabim_core::sample::sample_with_buried_beam().0);
                self.status = "Loaded the sample house with a beam inside its south wall. Hover it in Section A and press Tab to reach the beam.".into();
            }
            "new.empty" => {
                // two levels, in the units in use
                let units = self.doc.model().units();
                let mut m = Document::new();
                let mut tx = Transaction::new("New project");
                for (name, z) in [("Level 0", 0.0), ("Level 1", 3.0)] {
                    tx = tx.insert(Element::new(
                        name,
                        ElementKind::Level(creabim_core::Level {
                            elevation: z,
                            height: 3.0,
                        }),
                    ));
                }
                let _ = m.commit(tx);
                let tx = creabim_core::units::set_units(m.model(), units);
                let _ = m.commit(tx);
                self.load_model(m.model().clone());
                self.path = new_path("Untitled");
                self.status =
                    "New project with two levels. Pick the Wall tool to start drawing.".into();
            }
            "new.project" => self.open_new_project(ctx),
            "new.sheet" => self.new_sheet(),
            "new.script" => self.new_script(),
            "new.3dview" => self.new_3d_view(),
            "levels" => self.open_levels(),
            "project.info" => self.project_open = true,
            "level.add" => self.add_level_above(),
            "browser.sort" => {
                if let Some(g) = crate::browser::ViewGroup::from_key(arg) {
                    let tx = crate::browser::sort_auto(self.doc.model(), g);
                    let r = self.doc.commit(tx);
                    self.report(
                        r,
                        match g {
                            crate::browser::ViewGroup::Plans => {
                                "Floor plans sorted by level again; new plans take their place by level.".to_string()
                            }
                            g => format!("{} back in their automatic order.", g.title()),
                        },
                    );
                }
            }
            "units" => self.levels.units_open = true,
            "marks" => self.marks.open = true,
            "graphicstyles" => self.gstyles.open = true,
            "underlay" => {
                if let Some((view, level)) = self.active_plan() {
                    let u = match arg {
                        "below" => Underlay::Below,
                        "above" => Underlay::Above,
                        "none" => Underlay::None,
                        _ => match by_uuid(self) {
                            Some(l) if l != level => Underlay::Level(l),
                            _ => return,
                        },
                    };
                    self.set_underlay(view, u);
                }
            }
            "new.schedule" => {
                if let Some(c) = index.and_then(|i| ScheduleCategory::ALL.get(i)) {
                    self.new_schedule(*c);
                }
            }
            "open" => self.open_dialog(),
            "open.path" => {
                let path = self.path.clone();
                self.open_path(&path);
            }
            "recent" => {
                if let Some(f) = index.and_then(|i| self.recent.files.get(i)).cloned() {
                    self.open_path(&f);
                }
            }
            "recent.clear" => self.recent.clear(),
            "save" => self.save(),
            "saveas" => self.save_as(),
            "link" => {
                if let Some(path) = crate::files::pick_open() {
                    self.link_file(&path);
                }
            }
            "live" => self.live_reload = !self.live_reload,
            "import.report" => {
                if let Some(r) = &mut self.import_report {
                    r.open = true;
                }
            }
            "reload" => {
                let linked = crate::files::linked_files(self.doc.model());
                if let Some(f) = index.and_then(|i| linked.get(i)) {
                    self.reload(f);
                }
            }
            "export.glb" | "export.fcstd" | "export.ifcx" | "export.ifc4" | "export.pdf"
            | "export.svg" | "export.xlsx" => self.export(head),
            "export.folder" => self.open_export_folder(),
            "footing.foundation" => self.show_foundation_plan(),
            "placement" => {
                self.placement = match index {
                    Some(1) => PlanPlacement::Window,
                    Some(2) => PlanPlacement::Overlay,
                    Some(3) => PlanPlacement::Hidden,
                    _ => PlanPlacement::Docked,
                }
            }
            "ground.grid" => {
                let s = &mut self.shell.settings;
                s.ground_grid = !s.ground_grid;
                s.store();
                self.geometry_key = None;
                self.status = if self.shell.settings.ground_grid {
                    "Ground grid shown. View > 3D view sets its spacing.".into()
                } else {
                    "Ground grid hidden.".into()
                };
            }
            "xray" => self.set_xray(!self.shell.settings.xray),
            "render3d" => {
                if let Some(m) = index.and_then(|i| RenderMode::ALL.get(i)) {
                    let s = &mut self.shell.settings;
                    s.render_3d = *m;
                    s.store();
                    self.geometry_key = None;
                    self.status = format!("3D view: {}. {}.", m.label(), m.hint());
                }
            }
            "viewstyle" => {
                if let Some(st) = index.and_then(|i| ViewStyle::ALL.get(i)) {
                    self.set_view_style(*st);
                }
            }
            "guides.toggle" => self.set_guides_hidden(!self.shell.settings.guides_hidden),
            "ground.custom" => {
                let now = ground_spacing(self.shell.settings.ground_grid_spacing);
                self.shell.ground_custom = Some(crate::units::len(now));
            }
            "ground.spacing" => {
                if let Some(v) = index.and_then(|i| GROUND_SPACINGS.get(i)) {
                    let s = &mut self.shell.settings;
                    s.ground_grid_spacing = *v;
                    s.ground_grid = true;
                    s.store();
                    self.geometry_key = None;
                    self.status = format!("Ground grid every {v} m.");
                }
            }
            "plan3d" => self.plan_in_3d = !self.plan_in_3d,
            "divide" => self.open_divide(),
            "split" => self.start_split(),
            "frame" => self.frame_pane(false),
            "nav.fitsel" => self.frame_selection(),
            "sched.csv" => self.schedule_export = Some("csv".into()),
            "sched.xlsx" => self.schedule_export = Some("xlsx".into()),
            "railpick" => {
                if let Some(e) = self.doc.model().iter().find(|e| e.id.0.to_string() == arg) {
                    let (id, opens) = (e.id, matches!(e.kind, ElementKind::View(_) | ElementKind::Sheet(_)));
                    self.selection.only(id);
                    if opens {
                        self.active_view = Some(id);
                    }
                    self.face = None;
                }
            }
            "browser.fold" => {
                self.shell.settings.left_folded = !self.shell.settings.left_folded;
                self.shell.settings.store();
            }
            "props.fold" => {
                self.shell.settings.right_folded = !self.shell.settings.right_folded;
                self.shell.settings.store();
            }
            "mode.object" => {
                if self.plan.edit.is_some() {
                    self.toggle_edit_mode();
                }
            }
            "mode.edit" => {
                if self.plan.edit.is_none() {
                    self.toggle_edit_mode();
                }
            }
            "view.zoomscale" => {
                if let Some(v) = self.active_view {
                    self.plan.zoom_to_scale = Some(v);
                    self.status = "At the drawing's scale: 1 mm on paper is 1 mm on screen.".into();
                }
            }
            "nav.orbitsel" => self.orbit_selection = !self.orbit_selection,
            "nav.ortho" => self.camera.ortho = !self.camera.ortho,
            "nav.zoomin" => self.camera.zoom(2.0),
            "nav.zoomout" => self.camera.zoom(-2.0),
            "nav.look" => {
                if let Some(l) = index.and_then(|i| crate::camera::Look::ALL.get(i)) {
                    self.camera.look(*l);
                }
            }
            "palette" => self.open_palette(ctx),
            "theme" | "theme.next" => {
                let t = match Theme::from_id(arg) {
                    Some(t) => t,
                    None => self.shell.theme.next(),
                };
                self.set_theme(ctx, t);
            }
            "ui.classic" => {
                self.shell.classic = !self.shell.classic;
                if let Some(web) = &mut self.shell.web {
                    web.hide_all(self.shell.classic);
                }
            }
            "view.shaded" | "view.tag" | "view.tagfixtures" | "view.reveal" | "view.colour"
            | "view.xray" | "view.rooms" | "view.ceiling" | "view.tagrooms" | "view.roomtags"
            // (these four were handled below but never routed here: MU22)
            | "view.tempdims" | "view.ranges" | "view.reference" | "sheet.preview" | "sheet.text"
            | "sheet.north" | "sheet.scalebar" | "sheet.pdf" | "export.sheets" | "sheet.cloud"
            | "sheet.revtable" | "tag.all" | "sheet.keyplan" | "sheet.guides" => {
                self.view_action(head)
            }
            "cleanup.stacked" => {
                // copies left on their originals (MU24): one undo step
                let ids = creabim_core::checks::stacked_copies(self.doc.model());
                if ids.is_empty() {
                    self.status = "No element sits on a copy of itself.".into();
                } else {
                    let n = ids.len();
                    let tx = creabim_core::Transaction::new("Remove copies on their originals")
                        .remove_all_cascade(self.doc.model(), &ids);
                    let r = self.doc.commit(tx);
                    self.selection.clear();
                    self.report(r, format!("Removed {n} copies that sat exactly on their originals. Ctrl+Z brings them back."));
                }
            }
            "cleanup.overlap" => {
                // walls along each other (MU27): the first time it selects
                // them; asked again with them selected, it merges each pair
                let pairs = creabim_core::checks::overlapping_walls(self.doc.model());
                if pairs.is_empty() {
                    self.status = "No walls overlap along their length.".into();
                    return;
                }
                let all: Vec<ElementId> = pairs.iter().flat_map(|(a, b)| [*a, *b]).collect();
                let picked = all.iter().all(|id| self.selection.contains(*id));
                if !picked {
                    self.selection.extend(all.iter().copied(), false);
                    self.status = format!(
                        "{} pair(s) of walls overlap along their length (selected). Run Overlapping walls again to merge each pair into one wall covering both.",
                        pairs.len()
                    );
                    return;
                }
                let model = self.doc.model();
                let mut tx = creabim_core::Transaction::new("Merge overlapping walls");
                let mut gone: Vec<ElementId> = vec![];
                for (a, b) in &pairs {
                    if gone.contains(a) || gone.contains(b) {
                        continue;
                    }
                    let (Some(ea), Some(eb)) = (model.get(*a), model.get(*b)) else { continue };
                    let (ElementKind::Wall(wa), ElementKind::Wall(wb)) = (&ea.kind, &eb.kind) else { continue };
                    // the thicker one stays, stretched over both
                    let (keep, k, other, o) = if wa.thickness >= wb.thickness { (ea, wa, eb, wb) } else { (eb, wb, ea, wa) };
                    let d = k.direction();
                    let along = |p: glam::DVec2| (p - k.start).dot(d);
                    let lo = [0.0, along(o.start), along(o.end)].into_iter().fold(f64::MAX, f64::min);
                    let hi = [k.length(), along(o.start), along(o.end)].into_iter().fold(f64::MIN, f64::max);
                    let mut after = keep.clone();
                    if let ElementKind::Wall(w) = &mut after.kind {
                        w.start = k.start + d * lo;
                        w.end = k.start + d * hi;
                    }
                    tx = tx.replace(keep.clone(), after);
                    gone.push(other.id);
                }
                let tx = tx.remove_all_cascade(model, &gone);
                let r = self.doc.commit(tx);
                self.selection.clear();
                self.report(r, format!("Merged {} pair(s) of overlapping walls. Ctrl+Z undoes it.", gone.len()));
            }
            "rooms.fromnotes" => {
                self.status = crate::roomtool::rooms_from_notes(&mut self.doc);
            }
            "check.doorways" => {
                let (ids, msg) = crate::roomtool::check_doorways(self.doc.model());
                if !ids.is_empty() {
                    self.selection.extend(ids, false);
                }
                self.status = msg;
            }
            "check.layers" => {
                let (ids, msg) = crate::roomtool::check_layers(self.doc.model());
                if !ids.is_empty() {
                    self.selection.extend(ids, false);
                }
                self.status = msg;
            }
            "hidden.colour3d" => self.set_hidden_colour_3d(!self.shell.settings.hidden_colour_3d),
            "object" => {
                if crate::objecttool::toggle_option(&mut self.plan, arg) {
                    self.catalogue.open = true;
                }
            }
            "part" => crate::partstool::toggle_option(&mut self.plan, arg),
            "member" => {
                if arg == "square" || arg == "round" {
                    self.plan.types.remove(&creabim_core::TypeCategory::Column);
                }
                crate::memberstool::toggle_option(&mut self.plan, arg)
            }
            "slab.mode" => {
                if let Some(m) = index.and_then(|i| crate::slabtool::SlabMode::ALL.get(i)) {
                    crate::slabtool::pick(ctx, Some(*m), None);
                }
            }
            "slab.arc" => crate::slabtool::pick(ctx, None, Some(arg == "1")),
            "sel.shape" => {
                use crate::boxsel::Shape;
                crate::boxsel::set_shape(match arg {
                    "1" => Shape::Circle,
                    "2" => Shape::Lasso,
                    _ => Shape::Box,
                });
            }
            "sel.combine" => {
                use crate::boxsel::Combine;
                crate::boxsel::set_combine(match arg {
                    "1" => Combine::Add,
                    "2" => Combine::Subtract,
                    "3" => Combine::Invert,
                    "4" => Combine::Intersect,
                    _ => Combine::New,
                });
            }
            "sel.pick" => {
                let k: u8 = arg.parse().unwrap_or(0);
                crate::handles::set_pick(if crate::handles::pick() == k { 0 } else { k });
            }
            "sel.invert" => self.select_invert(),
            "sel.linked" => self.select_linked(),
            "sel.random" => self.select_random(),
            "sel.kind" => self.select_kind(arg),
            "sketch.shape" => {
                if let Some(s) = index.and_then(|i| crate::sketch::Shape::ALL.get(i)) {
                    crate::sketch::pick_shape(ctx, *s);
                }
            }
            "note.leader" => self.plan.anno.leader = !self.plan.anno.leader,
            "detail.kind" => {
                if let Some(k) = index.and_then(|i| creabim_core::DetailKind::ALL.get(i)) {
                    self.plan.details.pick(*k);
                    self.plan.details.path.clear();
                }
            }
            "callout.scale" => {
                if let Some(s) = index.and_then(|i| crate::detailtool::CALLOUT_SCALES.get(i)) {
                    self.plan.details.callout_scale = *s;
                }
            }
            _ if self.join_action(head, arg) => {}
            _ if self.crop_action(head, arg) => {}
            _ if self.profiles_action(head, arg) => {}
            _ if self.masters_action(head, arg) => {}
            _ if self.log_action(ctx, head) => {}
            _ if self.similar_action(head, arg) => {}
            _ if self.app_action(ctx, head, arg) => {}
            _ if self.help_action(head, arg) => {}
            _ if self.wall_action(head, arg) => {}
            _ if self.level_action(head, arg) => {}
            _ => {
                if !self.materials_action(head, arg) && !self.drafting_action(head, arg) {
                    self.grid_vis_action(head, arg);
                }
            }
        }
    }

    /// Why `t` can't be used in the view in front, if it can't: the
    /// annotation tools on a sheet, in 3D or a schedule (MU24).
    pub(super) fn tool_unusable(&self, t: Tool) -> Option<&'static str> {
        let front = self
            .active_view
            .and_then(|v| match self.doc.model().get(v).map(|e| &e.kind) {
                Some(ElementKind::Sheet(_)) => Some("sheet"),
                Some(ElementKind::View(view)) => Some(match view.kind {
                    ViewKind::Plan { .. } | ViewKind::Section { .. } => "drawing",
                    ViewKind::Axonometric { .. } => "3d",
                    ViewKind::Schedule { .. } => "schedule",
                }),
                _ => None,
            });
        let measures = matches!(
            t,
            Tool::Dimension
                | Tool::Angle
                | Tool::Radius
                | Tool::Diameter
                | Tool::ArcLength
                | Tool::SpotLevel
                | Tool::Tag
                | Tool::Detail
        );
        match front? {
            "sheet" if measures => {
                Some("on a sheet, double-click a view to dimension or tag inside it")
            }
            "3d" if measures || t == Tool::Note => {
                Some("annotation goes in plans, sections and sheets, not the 3D view")
            }
            "schedule" if measures || t == Tool::Note => Some("a schedule takes no annotation"),
            _ => None,
        }
    }

    /// A key plan on `sheet` (Annotate step 5): the lowest floor plan
    /// again as a view of its own at 1:500, its annotation hidden, placed
    /// over the title block, all one undo step.
    pub(super) fn add_key_plan(&mut self, sheet: ElementId) {
        use creabim_core::visibility::Category;
        let model = self.doc.model();
        let plan = model
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::View(v)
                    if matches!(v.kind, creabim_core::ViewKind::Plan { .. })
                        && v.callout.is_none() =>
                {
                    let lvl = match v.kind {
                        creabim_core::ViewKind::Plan { level, .. } => {
                            creabim_core::members::elevation(model, level)
                        }
                        _ => 0.0,
                    };
                    Some((lvl, e.clone(), v.clone()))
                }
                _ => None,
            })
            .filter(|(z, ..)| *z >= -0.5)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, from, mut v)) = plan else {
            self.status = "A key plan needs a floor plan to draw from.".into();
            return;
        };
        v.scale = 500.0;
        v.browser_order = None;
        for c in [
            Category::Dimensions,
            Category::Tags,
            Category::Notes,
            Category::Grids,
            Category::SectionMarks,
            Category::Objects,
            Category::Guides,
            Category::Sketches,
        ] {
            if !v.hidden.categories.contains(&c) {
                v.hidden.categories.push(c);
            }
        }
        let view = Element::new(format!("Key plan ({})", from.name), ElementKind::View(v));
        let view_id = view.id;
        let Some(s) = creabim_geom::sheet::sheet_of(model, sheet) else {
            return;
        };
        let m = creabim_geom::sheet::MARGIN;
        let top =
            creabim_geom::sheet::title_block_box(model, sheet).map_or(m + 45.0, |(_, hi)| hi.y);
        let at = glam::DVec2::new(s.width - m - 40.0, top + 30.0);
        // the view first, then its viewport, made against the model with
        // the view in it; one undo step
        if let Err(e) = self
            .doc
            .commit(Transaction::new("Add key plan").insert(view))
        {
            self.status = format!("Couldn't do that: {e}");
            return;
        }
        let vp = creabim_geom::sheet::new_viewport(self.doc.model(), sheet, view_id, at);
        let e = Element::new("Key plan", ElementKind::Viewport(vp));
        let id = e.id;
        let r = self
            .doc
            .commit_merged(Transaction::new("Add key plan").insert(e));
        self.selection.only(id);
        self.report(r, "Key plan placed over the title block: the whole floor at 1:500. Drag it where it goes; it is a view of its own in the browser.");
    }

    pub(super) fn place_view(&mut self, sheet: ElementId, view: ElementId) {
        let at = sheets::free_spot(self.doc.model(), sheet, view);
        let vp = sheets::new_viewport(self.doc.model(), sheet, view, at);
        let e = Element::new("Viewport", ElementKind::Viewport(vp));
        let id = e.id;
        let r = self
            .doc
            .commit(Transaction::new("Place view on sheet").insert(e));
        self.selection.only(id);
        self.report(
            r,
            "Placed the view. Drag it to where it belongs on the sheet.",
        );
    }

    pub(super) fn export(&mut self, what: &str) {
        self.export_inner(what);
        // what was exported can be found from the status bar
        if self.status.starts_with("Exported ") {
            self.last_export = Some(self.status.clone());
        }
    }

    /// Where to write an export: asked with the system's save window, which
    /// starts next to the project with `default`'s name.
    pub(super) fn export_to(&mut self, default: String) -> Option<String> {
        let out = if cfg!(test) {
            Some(default)
        } else {
            Self::ask_export(default)
        }?;
        self.last_export_dir = std::path::absolute(&out)
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        Some(out)
    }

    fn ask_export(default: String) -> Option<String> {
        let ext = std::path::Path::new(&default)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let full = std::path::absolute(&default)
            .map(|p| p.display().to_string())
            .unwrap_or(default);
        let asked = std::time::Instant::now();
        let picked = crate::files::pick_save(&full, "Export", &ext);
        // a save window that closes at once never opened (no file picker
        // on this system): write next to the project as before
        if picked.is_none() && asked.elapsed() < std::time::Duration::from_millis(300) {
            return Some(full);
        }
        picked
    }

    /// Open the folder the last export went to, in the system's file browser.
    pub(super) fn open_export_folder(&mut self) {
        let Some(dir) = self.last_export_dir.clone() else {
            return;
        };
        let r = crate::files::open_folder(&dir);
        self.report(r, format!("Opened {}.", dir.display()));
    }

    fn export_inner(&mut self, what: &str) {
        match what {
            "export.glb" => {
                let Some(out) = self.export_to(with_extension(&self.path, "glb")) else {
                    return;
                };
                let r = std::fs::write(&out, creabim_exchange::gltf::export_glb(self.doc.model()));
                self.report(r, format!("Exported {out}. In Blender: File > Import > glTF, or use the CreaBIM add-on."));
            }
            "export.fcstd" => {
                let Some(out) = self.export_to(with_extension(&self.path, "FCStd")) else {
                    return;
                };
                let r =
                    creabim_exchange::freecad::export_fcstd(self.doc.model(), &stem(&self.path))
                        .map_err(|e| e.to_string())
                        .and_then(|b| std::fs::write(&out, b).map_err(|e| e.to_string()));
                self.report(r, format!("Exported {out}"));
            }
            "export.ifcx" => {
                let Some(out) = self.export_to(with_extension(&self.path, "ifcx")) else {
                    return;
                };
                let r = std::fs::write(&out, creabim_ifc::export_ifcx(self.doc.model()));
                self.report(r, format!("Exported {out}"));
            }
            // first how walls and slabs go out (MU20), then where
            "export.ifc4" => self.ifc_dialog = true,
            "export.xlsx" => {
                let Some(out) = self.export_to(with_extension(&self.path, "xlsx")) else {
                    return;
                };
                let model = self.doc.model();
                let n = creabim_geom::xlsx::schedules(model).len();
                let r = creabim_geom::xlsx::schedules_xlsx(model, &stem(&self.path))
                    .and_then(|b| std::fs::write(&out, b).map_err(|e| e.to_string()));
                self.report(r, format!("Exported {n} schedules and a summary to {out}"));
            }
            "export.pdf" if !cfg!(test) => {
                self.sheets_export_open = true;
            }
            "export.pdf" => {
                let Some(out) = self.export_to(with_extension(&self.path, "pdf")) else {
                    return;
                };
                if sheets::sheets(self.doc.model()).is_empty() {
                    self.status = "There are no sheets to export yet.".into();
                } else {
                    let pdf = sheets::sheets_pdf(self.doc.model(), &self.sheet_info());
                    let r = std::fs::write(&out, pdf);
                    let n = sheets::sheets(self.doc.model()).len();
                    self.report(r, format!("Exported {n} sheets to {out}"));
                }
            }
            "export.svg" => {
                if let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) {
                    let number = sheets::sheet_of(self.doc.model(), sheet)
                        .map(|s| s.number.clone())
                        .unwrap_or_default();
                    let Some(out) =
                        self.export_to(with_extension(&self.path, &format!("{number}.svg")))
                    else {
                        return;
                    };
                    if let Some(svg) =
                        sheets::sheet_svg(self.doc.model(), sheet, &self.sheet_info())
                    {
                        let r = std::fs::write(&out, svg);
                        self.report(r, format!("Exported {out}"));
                    }
                } else if let Some(view) = self.active_view {
                    let name = self
                        .doc
                        .model()
                        .get(view)
                        .map(|e| e.name.replace(' ', "-"))
                        .unwrap_or_default();
                    let scale = self.view(view).map_or(100.0, |v| v.scale);
                    let Some(out) =
                        self.export_to(with_extension(&self.path, &format!("{name}.svg")))
                    else {
                        return;
                    };
                    if let Some(d) = self.drawing(view) {
                        let r = std::fs::write(&out, d.to_svg(scale));
                        self.report(r, format!("Exported {out} at 1:{scale:.0}"));
                    }
                }
            }
            _ => {}
        }
    }

    /// The value the active view's number field shows: cut height, depth or
    /// altitude.
    fn view_number(&self) -> Option<(&'static str, f64, &'static str, ElementId)> {
        let id = self.active_view?;
        let v = self.view(id)?;
        match &v.kind {
            ViewKind::Plan { cut_height, .. } => Some(("Cut height", *cut_height, "m", id)),
            ViewKind::Section { depth, .. } => Some(("Depth", *depth, "m", id)),
            ViewKind::Axonometric { altitude, .. } => Some(("Altitude", *altitude, "°", id)),
            ViewKind::Schedule { .. } => None,
        }
    }

    pub(super) fn set_view_number(&mut self, value: f64) {
        let Some(id) = self.active_view else { return };
        let Some(mut v) = self.view(id).cloned() else {
            return;
        };
        match &mut v.kind {
            ViewKind::Plan { cut_height, .. } => *cut_height = value.clamp(0.1, 200.0),
            ViewKind::Section { depth, .. } => *depth = value.clamp(0.1, 200.0),
            ViewKind::Axonometric { altitude, .. } => *altitude = value.clamp(-89.0, 89.0),
            ViewKind::Schedule { .. } => return,
        }
        let r = self
            .doc
            .edit_merged(id, "Change view", |e| e.kind = ElementKind::View(v.clone()));
        self.report(r, "View changed.");
    }

    /// Draw `view` at 1:`scale`. Each sheet it is placed on keeps the
    /// middle of it where it was, so the viewport grows or shrinks round
    /// its centre. One undo step.
    pub(super) fn set_view_scale(&mut self, view: ElementId, scale: f64) {
        let Some(before) = self.doc.model().get(view).cloned() else {
            return;
        };
        let ElementKind::View(v) = &before.kind else {
            return;
        };
        let old = v.scale;
        if (old - scale).abs() < 1e-9 || scale < 1.0 {
            return;
        }
        let mid = self
            .drawing(view)
            .and_then(|d| d.bounds())
            .map(|(lo, hi)| (lo + hi) / 2.0);
        let mut after = before.clone();
        if let ElementKind::View(v) = &mut after.kind {
            v.scale = scale;
        }
        let mut tx = Transaction::new("View scale").replace(before, after);
        for e in self.doc.model().iter() {
            let ElementKind::Viewport(vp) = &e.kind else {
                continue;
            };
            if vp.view != view {
                continue;
            }
            let Some(m) = vp.crop.map(|[lo, hi]| (lo + hi) / 2.0).or(mid) else {
                continue;
            };
            let mut moved = e.clone();
            if let ElementKind::Viewport(p) = &mut moved.kind {
                p.center = sheetview::rescaled_center(vp, m, old, scale).round();
            }
            tx = tx.replace(e.clone(), moved);
        }
        let r = self.doc.commit(tx);
        self.report(
            r,
            format!("Drawn at 1:{scale:.0}, on its sheets too: each viewport grows or shrinks round its centre."),
        );
    }

    /// The scales a placed view can be drawn at, for its right-click menu.
    fn viewport_scale_items(&self, view: ElementId) -> Vec<Item> {
        let now = self.view(view).map_or(0.0, |v| v.scale);
        let mut items: Vec<Item> = sheetview::SCALES
            .iter()
            .map(|s| {
                Item::new(format!("vpscale:{s:.0}"), format!("1:{s:.0}"))
                    .checked((s - now).abs() < 1e-6)
            })
            .collect();
        if !sheetview::SCALES.iter().any(|s| (s - now).abs() < 1e-6) {
            items.push(Item::Separator);
            items.push(
                Item::new(
                    "vpscale.custom",
                    format!("1:{now:.0} (custom, set in Properties)"),
                )
                .checked(true)
                .enabled(false),
            );
        }
        items
    }

    /// How the 3D view can draw, the one it uses ticked.
    fn render_items(&self) -> Vec<Item> {
        RenderMode::ALL
            .iter()
            .enumerate()
            .map(|(i, m)| {
                Item::new(format!("render3d:{i}"), m.label())
                    .checked(self.shell.settings.render_3d == *m)
                    .icon(m.icon())
                    .hint(m.hint())
            })
            .collect()
    }

    /// How the active section, elevation or 3D view can draw.
    fn view_style_items(&self) -> Vec<Item> {
        let now = self
            .active_view
            .and_then(|id| self.view(id))
            .map(|v| v.style);
        ViewStyle::ALL
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Item::new(format!("viewstyle:{i}"), s.label())
                    .checked(now == Some(*s))
                    .icon(view_style_icon(*s))
            })
            .collect()
    }

    /// Draw the active view in `style`.
    pub(super) fn set_view_style(&mut self, style: ViewStyle) {
        let Some(id) = self.active_view else { return };
        let Some(mut v) = self.view(id).cloned() else {
            return;
        };
        v.style = style;
        let r = self
            .doc
            .edit_merged(id, "View style", |e| e.kind = ElementKind::View(v.clone()));
        self.report(r, format!("Drawn {}.", style.label().to_lowercase()));
    }

    fn view_action(&mut self, what: &str) {
        let Some(id) = self.active_view else { return };
        match what {
            "view.shaded" => {
                let Some(mut v) = self.view(id).cloned() else {
                    return;
                };
                v.style = if v.style == ViewStyle::Shaded {
                    ViewStyle::HiddenLine
                } else {
                    ViewStyle::Shaded
                };
                let r = self
                    .doc
                    .edit_merged(id, "Change view", |e| e.kind = ElementKind::View(v.clone()));
                self.report(r, "View changed.");
            }
            "view.rooms" | "view.ceiling" => {
                let Some(mut v) = self.view(id).cloned() else {
                    return;
                };
                if what == "view.rooms" {
                    v.room_colours = !v.room_colours;
                } else {
                    v.ceiling_plan = !v.ceiling_plan;
                }
                let r = self
                    .doc
                    .edit_merged(id, "Change view", |e| e.kind = ElementKind::View(v.clone()));
                self.report(r, "View changed.");
            }
            "view.roomtags" => {
                let Some(mut v) = self.view(id).cloned() else {
                    return;
                };
                let on = !v.shows_room_tags();
                v.room_tags = Some(on);
                let r = self
                    .doc
                    .edit_merged(id, "Change view", |e| e.kind = ElementKind::View(v.clone()));
                self.report(
                    r,
                    if on {
                        "Room tags shown in this view."
                    } else {
                        "Room tags hidden in this view."
                    },
                );
            }
            "view.tagrooms" => {
                self.status = match crate::roomtool::tag_rooms(&mut self.doc, id) {
                    Ok(0) => "Every room on this plan is tagged already.".into(),
                    Ok(n) => format!("Tagged {n} rooms. Drag a tag to move it; pick what it shows in Properties."),
                    Err(e) => format!("Couldn't do that: {e}"),
                };
            }
            "view.reveal" => self.toggle_hidden_elements(id),
            "view.colour" => self.toggle_hidden_colour(id),
            "view.xray" => self.toggle_view_xray(id),
            "view.reference" => {
                // the Trace tab of the view in the focused area (MU19)
                if matches!(self.vis_target(false), Some(gridvis::VisTarget::View(_))) {
                    self.selection.clear();
                    self.prop_tab = PropTab::Trace;
                    self.status = "Add a reference in Properties > Trace: another view, a picture or a DXF drawing.".into();
                } else {
                    self.status = "Open a plan or a section first: references show in them.".into();
                }
            }
            "sheet.text" => {
                self.plan.tool = Tool::Note;
                self.plan.cancel();
                self.status = "Text on the sheet: click where it starts, then type.".into();
            }
            "sheet.picture" => {
                if let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) {
                    self.add_sheet_picture(sheet);
                }
            }
            "sheet.north" | "sheet.scalebar" => {
                if let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) {
                    self.add_paper_mark(sheet, what == "sheet.north");
                }
            }
            "sheet.pdf" => self.export_sheets_now(true),
            "sheet.guides" => {
                self.plan.sheet_guides = !self.plan.sheet_guides;
                self.status = if self.plan.sheet_guides {
                    "Guides on: drag one out of a ruler at the sheet's top or left edge.".into()
                } else {
                    "Guides hidden; they are kept with the sheet.".into()
                };
            }
            "sheet.keyplan" => {
                if let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) {
                    self.add_key_plan(sheet);
                }
            }
            "sheet.cloud" | "sheet.revtable" => {
                if let Some(sheet) = self.active_view.filter(|id| self.is_sheet(*id)) {
                    self.add_revision_mark(sheet, what == "sheet.revtable");
                }
            }
            "tag.all" => {
                // doors and windows, fixtures and rooms, at once (Annotate board)
                let Some(view) = self.active_view else { return };
                let a = planview::tag_openings(&mut self.doc, view).unwrap_or(0);
                let b = planview::tag_fixtures(&mut self.doc, view).unwrap_or(0);
                let c = crate::roomtool::tag_rooms(&mut self.doc, view).unwrap_or(0);
                self.status = match a + b + c {
                    0 => "Everything here is tagged already (tags go on plans and sections).".into(),
                    _ => format!("Tagged {a} doors and windows, {b} fixtures, {c} rooms. Drag a tag to move it."),
                };
            }
            "export.sheets" => self.sheets_export_open = true,
            "sheet.preview" => {
                self.print_preview = !self.print_preview;
                self.status = if self.print_preview {
                    "Print preview: the sheet as it prints, on white.".into()
                } else {
                    "Back to the working paper.".into()
                };
            }
            "view.ranges" => {
                self.plan.plan_ranges = !self.plan.plan_ranges;
                self.status = if self.plan.plan_ranges {
                    "Plan ranges: each floor plan's cut (solid) and view depth (dashed). Drag a grip at the right to change it.".into()
                } else {
                    "Plan ranges hidden.".into()
                };
            }
            "view.grid" => {
                let s = &mut self.shell.settings;
                s.plan_grid = !s.plan_grid;
                s.store();
                self.status = if self.shell.settings.plan_grid {
                    "Grid on behind drawings.".into()
                } else {
                    "Grid off: plain paper behind drawings.".into()
                };
            }
            "view.tempdims" => {
                self.plan.temp_dims_off = !self.plan.temp_dims_off;
                self.shell.settings.temp_dims_off = self.plan.temp_dims_off;
                self.shell.settings.store();
                self.status = if self.plan.temp_dims_off {
                    "Temporary dimensions off: none show by what you select. The button turns them back on.".into()
                } else {
                    "Temporary dimensions on.".into()
                };
            }
            "view.tag" => {
                let r = planview::tag_openings(&mut self.doc, id);
                self.status = match r {
                    Ok(0) => "Every door and window here is tagged already.".into(),
                    Ok(n) => format!("Tagged {n} doors and windows. Drag a tag to move it."),
                    Err(e) => format!("Couldn't do that: {e}"),
                };
            }
            "view.tagfixtures" => {
                let r = planview::tag_fixtures(&mut self.doc, id);
                self.status = match r {
                    Ok(0) => "Every fixture here is tagged already (tags go on plans).".into(),
                    Ok(n) => {
                        format!("Tagged {n} fixtures with their type marks. Drag a tag to move it.")
                    }
                    Err(e) => format!("Couldn't do that: {e}"),
                };
            }
            _ => {}
        }
    }

    /// The tool options strip: what the tool in use (or the running Move
    /// or Rotate) can be set to. It has its own row under the tools, so it
    /// never covers a view or its buttons.
    /// A number set in the tool bar, or in Properties' "Next …" (MU18).
    pub(super) fn set_tool_number(&mut self, key: &str, v: f64) {
        match key {
            "wall.thickness" => {
                // a size of its own: new walls have no type
                self.plan.types.remove(&creabim_core::TypeCategory::Wall);
                self.plan.wall_thickness = v.clamp(0.05, 1.0)
            }
            k if k.starts_with("object.") => crate::objecttool::set_option(&mut self.plan, k, v),
            k if k.starts_with("part.") => crate::partstool::set_option(&mut self.plan, k, v),
            k if k.starts_with("member.") => {
                if k.contains("column") {
                    self.plan.types.remove(&creabim_core::TypeCategory::Column);
                }
                if k.contains("beam") {
                    self.plan.types.remove(&creabim_core::TypeCategory::Beam);
                }
                if k.contains("door_") {
                    self.plan.types.remove(&creabim_core::TypeCategory::Door);
                }
                if k.contains("window_") {
                    self.plan.types.remove(&creabim_core::TypeCategory::Window);
                }
                crate::memberstool::set_option(&mut self.plan, k, v)
            }
            "view.number" => self.set_view_number(v),
            "detail.width" => self.plan.details.width = v.clamp(0.0, 5.0),
            "dim.spacing" => {
                self.plan.dim_step_size = v.clamp(0.5, 100.0);
                self.plan.dim_step_mm = self.plan.dim_step_size;
            }
            _ => {}
        }
    }

    pub(super) fn tool_strip(&self, ctx: &egui::Context) -> (Value, Vec<Value>) {
        let seg = |items: Vec<Value>| json!({"kind": "seg", "items": items});
        let opt = |id: String, label: &str, on: bool, hint: &str| json!({"id": id, "label": label, "on": on, "hint": hint});
        let mut options = vec![];
        if let Some(m) = &self.plan.modal {
            let verb = m.mode.verb();
            let icon = match m.mode {
                crate::modal::Mode::Move => "move",
                crate::modal::Mode::Rotate => "rotate-cw",
                crate::modal::Mode::Scale => "scaling",
                crate::modal::Mode::Mirror => "flip-horizontal-2",
                crate::modal::Mode::Offset => "offset",
                crate::modal::Mode::Grip => "spline",
            };
            return (json!({"label": verb, "icon": icon}), options);
        }
        if let Some((m, _)) = self.plan.armed {
            let verb = format!("{m:?}");
            let icon = match m {
                crate::modal::Mode::Move => "move",
                crate::modal::Mode::Rotate => "rotate-cw",
                crate::modal::Mode::Scale => "scaling",
                crate::modal::Mode::Mirror => "flip-horizontal-2",
                crate::modal::Mode::Offset => "offset",
                crate::modal::Mode::Grip => "spline",
            };
            return (
                json!({"label": verb, "icon": icon, "hint": self.status}),
                options,
            );
        }
        let tool = self.plan.tool;
        if let Some(name) = self.tool_type_label() {
            options.push(json!({
                "kind": "menu", "key": "type.tool", "label": "Type", "value": name,
                "hint": "The type new elements are drawn as: they share its values, and follow it when it changes",
            }));
        }
        if tool == Tool::Wall {
            // straight, an arc, or a circle (MU40)
            options.push(seg([
                (
                    "wall.shape:0",
                    "Straight",
                    "Click to start, click for each corner",
                ),
                (
                    "wall.shape:1",
                    "Arc",
                    "Its start, its end, then a point it passes through",
                ),
                ("wall.shape:2", "Circle", "Its centre, then a point on it"),
                (
                    "wall.shape:3",
                    "Rectangle",
                    "Two opposite corners: four walls joined at the corners",
                ),
            ]
            .iter()
            .enumerate()
            .map(|(i, (id, label, hint))| {
                opt(
                    id.to_string(),
                    label,
                    self.plan.wall_shape as usize == i,
                    hint,
                )
            })
            .collect()));
            options.push(json!({
                "kind": "number", "key": "wall.thickness", "label": "Thickness",
                "value": self.plan.wall_thickness, "unit": "m", "step": 0.01, "min": 0.05, "max": 1.0,
                "hint": "Thickness of new walls. Drag the label or type a value.",
            }));
            options.push(json!({
                "kind": "menu", "key": "wall.location", "label": "Location line",
                "value": self.plan.wall_location.label(),
                "hint": "The line you draw new walls on: it stays put when they get thicker or are flipped",
            }));
            options.push(json!({
                "kind": "toggle", "id": "wall.flipnew", "label": "Exterior left",
                "on": self.plan.wall_flip, "icon": "brick-wall",
                "hint": "Which side of the line you draw the exterior face goes on (Space flips it while drawing)",
            }));
        }
        let redrawing = self.plan.outline.as_ref().is_some_and(|o| !o.is_wall());
        if tool.draws_outline() || redrawing {
            use crate::slabtool::SlabMode;
            if tool.draws_outline() {
                options.push(seg(SlabMode::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        opt(
                            format!("slab.mode:{i}"),
                            m.label(),
                            self.plan.slab_mode == *m,
                            m.hint(),
                        )
                    })
                    .collect()));
            }
            if !tool.draws_outline() || self.plan.slab_mode == SlabMode::Polygon {
                options.push(seg(vec![
                    opt(
                        "slab.arc:0".into(),
                        "Line (L)",
                        !self.plan.slab_arc,
                        "Straight sides",
                    ),
                    opt(
                        "slab.arc:1".into(),
                        "Arc (A)",
                        self.plan.slab_arc,
                        "The next side is an arc: click its end, then a point on it",
                    ),
                ]));
            }
        }
        if tool == Tool::Sketch {
            let now = crate::sketch::shape(ctx);
            options.push(seg(crate::sketch::Shape::ALL
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    opt(
                        format!("sketch.shape:{i}"),
                        s.label(),
                        now == *s,
                        "What the next clicks draw",
                    )
                })
                .collect()));
        }
        options.extend(crate::memberstool::tool_options(&self.plan));
        options.extend(crate::partstool::tool_options(&self.plan));
        if tool == Tool::Footing {
            // like Revit's multiple selection: footings under all the walls
            // and columns picked, in one step
            let n = self.footing_hosts().len();
            options.push(json!({
                "kind": "button", "id": "footing.under", "wide": true,
                "label": match n {
                    0 => "Under selected".to_string(),
                    n => format!("Under {n} selected"),
                },
                "icon": "footing",
                "hint": "Put footings under the selected walls and columns that have none: a strip under each wall, a pad under each column, in one undo step. Partitions thinner than 150 mm are left out unless only they are picked. Select them first, or drag a box over them with this tool.",
            }));
        }
        options.extend(
            crate::objecttool::tool_options(&self.plan)
                .into_iter()
                .map(|mut o| {
                    // the object placed is named, not drawn as an icon
                    if o["id"] == "object:catalogue" {
                        o["wide"] = json!(true);
                    }
                    o
                }),
        );
        if tool == Tool::Detail {
            options.push(seg(creabim_core::DetailKind::ALL
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    opt(
                        format!("detail.kind:{i}"),
                        k.label(),
                        self.plan.details.kind.same(*k),
                        "What the next clicks draw",
                    )
                })
                .collect()));
            if !matches!(
                self.plan.details.kind,
                creabim_core::DetailKind::Membrane
                    | creabim_core::DetailKind::Region { .. }
                    | creabim_core::DetailKind::BreakLine
            ) {
                options.push(json!({
                    "kind": "number", "key": "detail.width", "label": "Width",
                    "value": self.plan.details.width, "unit": "m", "step": 0.005, "min": 0.0, "max": 5.0,
                    "hint": "How wide the component is across its path: a batt's thickness, a course's depth",
                }));
            }
        }
        if tool == Tool::Callout {
            options.push(seg(crate::detailtool::CALLOUT_SCALES
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    opt(
                        format!("callout.scale:{i}"),
                        &format!("1:{s:.0}"),
                        self.plan.details.callout_scale == *s,
                        "The scale of the detail view the box makes",
                    )
                })
                .collect()));
        }
        if tool == Tool::Note {
            options.push(json!({
                "kind": "toggle", "id": "note.leader", "label": "Leader",
                "on": self.plan.anno.leader, "icon": "arrow-up-right",
                "hint": "Draw an arrow to what the note is about: click its point first, then the text",
            }));
        }
        options.extend(self.drafting_options());
        // the Grid tool's grid system
        options.extend(
            self.grid_vis_options()
                .into_iter()
                .filter(|o| o["key"] == "gridgroups"),
        );
        while options.last().is_some_and(|o| o["kind"] == "sep") {
            options.pop();
        }
        // Select's own tools (MU39, option B): the shape a drag takes, how
        // it joins what is selected, in Edit mode what a click picks, and
        // the Select menu; the hint names what is selected
        let mut hint = tool.hint().to_string();
        if tool == Tool::Select {
            let iseg = |items: Vec<(String, &str, bool, &str)>| {
                json!({"kind": "seg", "items": items
                    .into_iter()
                    .map(|(id, icon, on, h)| json!({"id": id, "label": "", "icon": icon, "on": on, "hint": h}))
                    .collect::<Vec<_>>()})
            };
            use crate::boxsel::{Combine as C, Shape as S};
            let shape = crate::boxsel::shape();
            let mut sel_opts = vec![iseg(vec![
                ("sel.shape:0".into(), "sel-box", shape == S::Box, "Box (B): drag a box; left to right takes what is inside, right to left what it touches"),
                ("sel.shape:1".into(), "sel-circle", shape == S::Circle, "Circle: drag out a circle; it takes what it touches"),
                ("sel.shape:2".into(), "sel-lasso", shape == S::Lasso, "Lasso: draw round what to take"),
            ])];
            let how = crate::boxsel::combine();
            sel_opts.push(iseg(vec![
                (
                    "sel.combine:0".into(),
                    "sel-new",
                    how == C::New,
                    "New: what you take replaces the selection",
                ),
                (
                    "sel.combine:1".into(),
                    "sel-add",
                    how == C::Add,
                    "Add (Shift): what you take joins the selection",
                ),
                (
                    "sel.combine:2".into(),
                    "sel-sub",
                    how == C::Subtract,
                    "Subtract (Ctrl): what you take leaves the selection",
                ),
                (
                    "sel.combine:3".into(),
                    "sel-invert",
                    how == C::Invert,
                    "Invert: what you take flips, in or out",
                ),
                (
                    "sel.combine:4".into(),
                    "sel-intersect",
                    how == C::Intersect,
                    "Intersect: keep only what is both selected and taken",
                ),
            ]));
            if self.plan.edit.is_some() {
                let k = crate::handles::pick();
                sel_opts.push(iseg(vec![
                    (
                        "sel.pick:1".into(),
                        "pick-vertex",
                        k == 1,
                        "Points (1): corners, ends, vertices",
                    ),
                    (
                        "sel.pick:2".into(),
                        "pick-edge",
                        k == 2,
                        "Edges (2): sides, edges, jambs",
                    ),
                    (
                        "sel.pick:3".into(),
                        "pick-face",
                        k == 3,
                        "Faces (3): faces, tops, undersides",
                    ),
                ]));
            }
            sel_opts.push(json!({
                "kind": "menu", "key": "select.menu", "label": "", "value": "Select",
                "hint": "All, none, invert, similar, linked, random",
            }));
            sel_opts.push(json!({"kind": "sep"}));
            sel_opts.extend(options);
            options = sel_opts;
            hint = self.selection_hint();
        }
        let head = json!({
            "label": tool.label(), "icon": tool_icon(tool), "hint": hint,
            "help": crate::help::tool_page(tool), "trail": tool == Tool::Select,
        });
        (head, options)
    }

    /// What the left, middle and right buttons do over the view the
    /// pointer was last in, for the status bar (MU39).
    fn mouse_hints(&self) -> Value {
        // as Blender's status bar: they change while Shift, Ctrl or Alt
        // is held, to what the buttons then do (MU39)
        let m = MODS.load(std::sync::atomic::Ordering::Relaxed);
        let (shift, ctrl, alt) = (m & 1 != 0, m & 2 != 0, m & 4 != 0);
        let in_2d = self.last_in_2d;
        let classic = self.shell.settings.navigation == "classic";
        let select = self.plan.tool == Tool::Select;
        let editing = self.plan.edit.is_some();
        let (left, middle, right): (String, &str, &str) = if self.plan.modal.is_some() {
            (
                if ctrl { "Place, snapping" } else { "Place" }.into(),
                if in_2d { "Pan" } else { "Rotate view" },
                "Cancel",
            )
        } else if ctrl {
            (
                if select {
                    "Take away"
                } else {
                    self.plan.tool.label()
                }
                .into(),
                "Zoom",
                "Menu",
            )
        } else if shift {
            (
                if editing {
                    "Pick more"
                } else if select {
                    "Add to selection"
                } else {
                    self.plan.tool.label()
                }
                .into(),
                "Pan",
                if in_2d { "Menu" } else { "Place pivot" },
            )
        } else if alt && !in_2d {
            ("Rotate view".into(), "Rotate view", "Menu")
        } else {
            (
                if editing {
                    "Pick a point".into()
                } else if select {
                    "Select".into()
                } else {
                    self.plan.tool.label().to_string()
                },
                match (in_2d, classic) {
                    (true, _) | (false, true) => "Pan",
                    (false, false) => "Rotate view",
                },
                if select { "Menu" } else { "Back" },
            )
        };
        json!([
            {"b": "l", "label": left},
            {"b": "m", "label": middle},
            {"b": "r", "label": right},
        ])
    }

    /// The tool bar's hint while Select is the tool: what is selected and
    /// what keys do to it, or how to select (MU39).
    fn selection_hint(&self) -> String {
        let model = self.doc.model();
        let ids = self.selection.all();
        match ids.len() {
            0 => "Click or drag to select. Shift adds, Ctrl takes away. Ctrl+A all, Alt+A or Esc none."
                .into(),
            1 => {
                let id = ids[0];
                format!(
                    "{} · M move · R rotate · Shift+A all of its type",
                    crate::hoverpick::describe(model, id)
                )
            }
            n => {
                let mut kinds: Vec<(String, usize)> = vec![];
                for e in ids.iter().filter_map(|i| model.get(*i)) {
                    let k = crate::app::kind_word(&e.kind).to_string();
                    match kinds.iter_mut().find(|(w, _)| *w == k) {
                        Some((_, c)) => *c += 1,
                        None => kinds.push((k, 1)),
                    }
                }
                let list: Vec<String> = kinds
                    .iter()
                    .map(|(k, c)| format!("{c} {}", crate::app::plural(k, *c)))
                    .collect();
                format!("{n} selected: {} · M move · R rotate", list.join(", "))
            }
        }
    }

    /// The Select menu of the tool bar (MU39).
    fn select_menu_items(&self) -> Vec<Item> {
        let some = !self.selection.is_empty();
        let mut items = vec![
            self.cmd_item(Command::SelectAll),
            self.cmd_item(Command::SelectNone),
            Item::new("sel.invert", "Invert")
                .hint("Select what is not selected, and drop what is"),
            Item::Separator,
            Item::new("sel.shape:0", "Box select").key("B").icon("sel-box"),
            Item::new("sel.shape:1", "Circle select").icon("sel-circle"),
            Item::new("sel.shape:2", "Lasso select").icon("sel-lasso"),
            Item::Separator,
            Item::new("sel.linked", "Select linked")
                .enabled(some)
                .hint("Add what is joined to the selection: walls meeting it, the doors and windows in it"),
            Item::new("sel.random", "Select random")
                .enabled(some)
                .hint("Keep about half of the selection, picked at random: to thin out a row"),
        ];
        // every kind the project holds, without a selection first (MU39)
        let mut kinds: Vec<(&'static str, usize)> = vec![];
        for id in self.selectable_ids() {
            if let Some(e) = self.doc.model().get(id) {
                let k = crate::app::kind_word(&e.kind);
                match kinds.iter_mut().find(|(w, _)| *w == k) {
                    Some((_, n)) => *n += 1,
                    None => kinds.push((k, 1)),
                }
            }
        }
        kinds.sort_by_key(|k| k.0);
        let by_kind: Vec<Item> = kinds
            .into_iter()
            .map(|(k, n)| {
                let word = crate::app::plural(k, 2);
                let mut c = word.chars();
                let label = c
                    .next()
                    .map_or(String::new(), |f| f.to_uppercase().chain(c).collect());
                Item::new(format!("sel.kind:{k}"), format!("{label} ({n})"))
            })
            .collect();
        if !by_kind.is_empty() {
            items.push(Item::sub("All by type", by_kind).icon("layers"));
        }
        let similar = self.similar_items();
        if !similar.is_empty() {
            items.push(Item::sub("Select similar", similar).icon("copy"));
        }
        items
    }

    /// The quick help the ? by the tool's name opens: what the tool is and
    /// how to use it, in a few lines, and its page in the guide.
    fn tool_quick_help(&self) -> Value {
        let tool = self.plan.tool;
        let page = crate::help::tool_page(tool);
        let mut lines = crate::help::quick(page);
        // a page that opens with what it is: then how to use the tool
        if lines.len() < 2 {
            lines.push(tool.hint().to_string());
        }
        json!({
            "title": format!("{} tool", tool.label()), "icon": tool_icon(tool),
            "keys": "F1", "lines": lines, "page": page,
        })
    }

    fn top_state(&self, ctx: &egui::Context) -> Value {
        let quick: Vec<Value> = [
            Some(Command::Undo),
            Some(Command::Redo),
            None,
            Some(Command::Move),
            Some(Command::Rotate),
            Some(Command::Scale),
            Some(Command::Duplicate),
            None,
            Some(Command::Copy),
            Some(Command::Cut),
            Some(Command::Paste),
            Some(Command::Delete),
            None,
            Some(Command::Hide),
            Some(Command::Isolate),
            Some(Command::ShowAll),
        ]
        .into_iter()
        .map(|c| match c {
            Some(c) => match self.cmd_item(c) {
                Item::Action {
                    id,
                    label,
                    shortcut,
                    enabled,
                    icon,
                    ..
                } => json!({"id": id, "label": label, "key": shortcut, "enabled": enabled, "icon": icon}),
                _ => Value::Null,
            },
            None => Value::Null,
        })
        .collect();
        // Split beside the other edits (MU39), its own picture
        let mut quick = quick;
        if let Some(i) = quick
            .iter()
            .position(|q| q["id"] == "ctx:Duplicate" || q["label"] == "Duplicate")
        {
            quick.insert(
                i + 1,
                json!({"id": "split", "label": "Split", "key": "", "enabled": !self.selection.is_empty(), "icon": "split"}),
            );
        }
        let tab = self
            .shell
            .settings
            .tools_tab
            .min(crate::tooltabs::TABS.len() - 1);
        let unusable = |t: Tool| self.tool_unusable(t);
        let tool = |t: Tool| {
            let i = Tool::ALL.iter().position(|x| *x == t).unwrap_or(0);
            let off = unusable(t);
            let hint = match off {
                Some(why) => format!("{} (not here: {why})", t.hint()),
                None => t.hint().to_string(),
            };
            json!({
                "kind": "tool", "id": format!("tool:{i}"), "label": t.label(), "hint": hint,
                "off": off.is_some(),
                // while Move, Rotate and the rest wait or run, it is
                // their button that is lit, not Select's (MU19)
                "icon": tool_icon(t),
                "active": self.plan.tool == t
                    && !(t == Tool::Select && (self.plan.armed.is_some() || self.plan.modal.is_some())),
            })
        };
        let mut tools: Vec<Value> = crate::tooltabs::ALWAYS.iter().map(|t| tool(*t)).collect();
        tools.push(json!({"kind": "sep"}));
        for e in crate::tooltabs::entries(tab) {
            use crate::tooltabs::Entry;
            tools.push(match e {
                Entry::Tool(t) => tool(t),
                Entry::Act {
                    id,
                    label,
                    icon,
                    hint,
                } => json!({
                    "kind": "act", "id": id, "label": label, "hint": hint, "icon": icon,
                    "active": false,
                }),
                Entry::Menu {
                    key,
                    label,
                    icon,
                    hint,
                } => json!({
                    "kind": "menu", "key": key, "label": label, "hint": hint, "icon": icon,
                    "open": self.popup_key().as_deref() == Some(key),
                }),
                Entry::Sep => json!({"kind": "sep"}),
            });
        }
        let tabs: Vec<Value> = crate::tooltabs::TABS
            .iter()
            .enumerate()
            .map(|(i, t)| json!({"id": format!("tab:{i}"), "label": t, "on": i == tab}))
            .collect();
        let (tool_head, tool_options, options, more) = self.option_strip(ctx);
        json!({
            "theme": self.theme(),
            "popup": self.popup_key(),
            "menus": (["File", "Edit", "View", "Create", "Materials", "Help"].map(|m| json!({"name": m, "key": format!("menu:{m}")}))),
            "quick": quick,
            // the selection count is in the status bar, where it belongs
            "doc": {
                "name": file_label(&self.path),
                "path": self.path,
                "unsaved": self.unsaved(),
                "note": "",
            },
            "app": {"key": "menu:CreaBIM", "label": "CreaBIM"},
            "right": [{
                "kind": "menu", "key": "workspaces", "label": "", "value": self.areas.workspace,
                "icon": "workspace", "big": true,
                "hint": "Workspace: named layouts of view areas (Ctrl+PgUp and Ctrl+PgDn step through them)",
            }],
            "tabs": tabs,
            "tools": tools,
            "tool": tool_head,
            "tool_options": tool_options,
            "options": options,
            "more": more,
        })
    }

    /// The tool options strip: the tool's head and options, the view's
    /// options, each keyed and marked compact or for the More menu as the
    /// page last measured them, and how many are in More.
    pub(super) fn option_strip(
        &self,
        ctx: &egui::Context,
    ) -> (Value, Vec<Value>, Vec<Value>, usize) {
        let (tool_head, mut tool_options) = self.tool_strip(ctx);
        // docked, each area's header has its view's controls
        let mut options = if self.placement == PlanPlacement::Docked {
            vec![]
        } else {
            self.view_options()
        };
        // lengths show and take the project's units; what is typed is read here
        for o in tool_options.iter_mut().chain(options.iter_mut()) {
            if o["kind"] == "number" && o["unit"] == "m" {
                let v = o["value"].as_f64().unwrap_or(0.0);
                o["text"] = json!(crate::units::len(v));
                o["unit"] = json!("");
            }
        }
        crate::optionbar::prepare(&mut tool_options, false);
        crate::optionbar::prepare(&mut options, true);
        let more =
            crate::optionbar::place(&mut tool_options, &mut options, self.shell.bar.as_ref());
        (tool_head, tool_options, options, more)
    }

    /// The active view's settings, at the right of the tool options strip.
    pub(super) fn view_options(&self) -> Vec<Value> {
        let mut options = vec![];
        if let Some((label, value, unit, _)) = self.view_number() {
            let (step, min, max) = if unit == "°" {
                (1.0, -89.0, 89.0)
            } else {
                (0.05, 0.1, 200.0)
            };
            options.push(json!({
                "kind": "number", "key": "view.number", "label": label, "value": value,
                "unit": unit, "step": step, "min": min, "max": max,
                "hint": "For the view shown in the 2D pane",
            }));
            if let Some(v) = self.active_view.and_then(|id| self.view(id)) {
                if matches!(
                    v.kind,
                    ViewKind::Section { .. } | ViewKind::Axonometric { .. }
                ) {
                    if matches!(v.kind, ViewKind::Section { .. }) {
                        options.push(json!({
                            "kind": "toggle", "id": "view.ranges", "label": "Plan ranges",
                            "icon": "view-range", "on": self.plan.plan_ranges,
                            "hint": "Each floor plan's cut and view depth as a band: drag their grips to change them (MU34)",
                        }));
                    }
                    options.push(json!({
                        "kind": "menu", "key": "viewstyle", "label": "Style",
                        "value": v.style.label(), "icon": view_style_icon(v.style),
                        "hint": "How this view draws, on its sheets too: hidden line, shaded with edges, or wireframe (every edge, the hidden ones too)",
                    }));
                }
                options.push(json!({
                    "kind": "toggle", "id": "view.reveal", "label": "Hidden",
                    "on": v.show_hidden, "icon": "hidden-elements",
                    "hint": "Show elements hidden inside others (a beam in a wall) with dashed lines",
                }));
                options.push(json!({
                    "kind": "toggle", "id": "view.colour", "label": "In colour",
                    "on": v.show_hidden && v.hidden_in_colour, "icon": "hidden-colour",
                    "hint": "For detailing: hidden elements in their material colours, see-through over what hides them",
                }));
                if let Some(id) = self.active_view {
                    options.push(json!({
                        "kind": "toggle", "id": "view.xray", "label": "X-ray",
                        "on": self.plan.xray.contains(&id), "icon": "xray", "shortcut": "Alt+Z",
                        "hint": "X-ray this view while working (Alt+Z over it): walls, slabs and roofs see-through, what is inside them solid and clicked first",
                    }));
                }
            }
            if self.active_plan().is_some() {
                options.push(json!({
                    "kind": "toggle", "id": "view.tempdims", "label": "Temporary dimensions",
                    "icon": "dimension", "on": !self.plan.temp_dims_off,
                    "hint": "The blue dimensions by what you select: on, they show and can be typed into; off, none show (MU21)",
                }));
            }
            // the grid behind the drawing, on or off for every drawing
            // (MU40: some prefer plain paper)
            options.push(json!({
                "kind": "toggle", "id": "view.grid", "label": "Grid",
                "icon": "grid-3x3", "on": self.shell.settings.plan_grid,
                "hint": "The light grid behind drawings: off for plain paper. For every plan, section and detail",
            }));
            options.push(json!({
                "kind": "button", "id": "view.tag", "label": "Tag openings", "icon": "tag",
                "hint": "Tag every door and window in this view that has no tag yet",
            }));
            if self.active_plan().is_some() {
                options.push(json!({
                    "kind": "button", "id": "view.tagfixtures", "label": "Tag fixtures", "icon": "fixtures",
                    "hint": "Tag every socket, switch, light, drain, radiator and other fixture on this plan with its type's mark",
                }));
                let shown = self.active_view.and_then(|id| self.view(id)).cloned();
                let (ceiling, colours) = shown
                    .as_ref()
                    .map_or((false, false), |v| (v.ceiling_plan, v.room_colours));
                options.push(json!({
                    "kind": "button", "id": "view.tagrooms", "label": "Tag rooms", "icon": "room",
                    "hint": "Tag every room on this plan that has no tag here yet",
                }));
                options.push(json!({
                    "kind": "toggle", "id": "view.roomtags", "label": "Show room tags",
                    "on": shown.as_ref().is_some_and(|v| v.shows_room_tags()), "icon": "tag",
                    "hint": "Draw this view's room tags. Off by default in detail and callout views, where they come out tiny",
                }));
                options.push(json!({
                    "kind": "toggle", "id": "view.ceiling", "label": "Ceiling plan",
                    "on": ceiling, "icon": "ceiling",
                    "hint": "A reflected ceiling plan: the ceilings, their grids and what hangs from them, not the floor and furniture",
                }));
                options.push(json!({
                    "kind": "toggle", "id": "view.rooms", "label": "Room colours",
                    "on": colours, "icon": "colour-scheme",
                    "hint": "Fill each room with a colour of its own, the same in every view",
                }));
            }
        }
        if let Some((view, level)) = self.active_plan() {
            let u = self.view(view).map(|v| v.underlay).unwrap_or_default();
            let value = self
                .underlay_choices(level)
                .into_iter()
                .find(|c| c.0 == u)
                .map_or("None".to_string(), |c| c.1);
            options.push(json!({
                "kind": "menu", "key": "underlay", "label": "Underlay", "value": value,
                "icon": "layers", "hint": "Show another level faintly under this plan, to trace and snap to",
            }));
        }
        options.extend(
            self.grid_vis_options()
                .into_iter()
                .filter(|o| o["key"] != "gridgroups" && o["kind"] != "sep"),
        );
        if self
            .active_view
            .and_then(|id| self.view(id))
            .is_some_and(|v| matches!(v.kind, ViewKind::Schedule { .. }))
        {
            options.push(json!({
                "kind": "button", "id": "sched.csv", "label": "Export CSV", "icon": "table-2",
                "hint": "Save this table as a CSV file beside the project",
            }));
            options.push(json!({
                "kind": "button", "id": "sched.xlsx", "label": "Export Excel", "icon": "sheet",
                "hint": "Save every schedule as one Excel workbook beside the project: a summary, then a sheet for each",
            }));
        }
        if self.active_view.is_some_and(|id| self.is_sheet(id)) {
            // compose
            options.push(json!({
                "kind": "menu", "key": "placeview", "label": "", "value": "Place view…",
                "icon": "place-view", "hint": "Put a view on this sheet", "wide": true,
            }));
            // paper marks: what belongs to the sheet, not to a view (MU22)
            options.push(json!({"kind": "sep"}));
            options.push(json!({
                "kind": "button", "id": "sheet.text", "label": "Text", "icon": "text",
                "hint": "Text on the sheet: click where it goes (N)",
            }));
            options.push(json!({
                "kind": "button", "id": "sheet.picture", "label": "Picture", "icon": "image",
                "hint": "A picture on the sheet: a collage, site context, a mood board (PNG, JPEG; an SVG stays crisp). Drag its grips to size it",
            }));
            options.push(json!({
                "kind": "button", "id": "sheet.north", "label": "North arrow", "icon": "north-arrow",
                "hint": "A north arrow, turned to the project's north; drag it where it goes",
            }));
            options.push(json!({
                "kind": "button", "id": "sheet.scalebar", "label": "Scale bar", "icon": "scale-bar",
                "hint": "A scale bar at the sheet's drawing scale; drag it where it goes",
            }));
            options.push(json!({
                "kind": "button", "id": "sheet.cloud", "label": "Revision cloud", "icon": "revision-cloud",
                "hint": "A revision cloud round a change, with the next revision letter",
            }));
            options.push(json!({
                "kind": "button", "id": "sheet.keyplan", "label": "Key plan", "icon": "map",
                "hint": "A small plan of the whole building by the title block, at 1:500, without dimensions, tags or notes",
            }));
            options.push(json!({
                "kind": "button", "id": "sheet.revtable", "label": "Revisions table", "icon": "table-2",
                "hint": "The sheet's revisions table: a row for each revision cloud on it",
            }));
            options.push(json!({
                "kind": "toggle", "id": "sheet.guides", "label": "Guides", "icon": "guides",
                "on": self.plan.sheet_guides,
                "hint": "Rulers along the sheet's edges to drag guides out of; marks, notes and views snap to the guides. Off, they hide and nothing snaps to them",
            }));
            // output
            options.push(json!({"kind": "sep"}));
            options.push(json!({
                "kind": "button", "id": "sheet.pdf", "label": "Sheet as PDF", "icon": "file-text",
                "hint": "This sheet as a PDF (Ctrl+P); all or chosen sheets: File › Export › Sheets as PDF (Ctrl+Shift+P)",
            }));
            // the sheet as it prints, on white, while the paper is dark (MU21)
            options.push(json!({
                "kind": "toggle", "id": "sheet.preview", "label": "Print preview",
                "icon": "print-preview", "on": self.print_preview,
                "hint": "Show the sheet as it prints, on white paper; off, it follows the drawing paper (dark or light)",
            }));
        }
        options
    }

    /// The browser's state, built again only when what it shows changed:
    /// building its rows (every element, marks and hints) took ~5 ms a
    /// frame, every frame (MU39: the app felt a tad slower).
    fn left_state(&self) -> Value {
        let key = format!(
            "{}|{:?}|{:?}|{}|{}|{:?}",
            self.doc.model().revision(),
            self.selection,
            self.active_view,
            self.shell.settings.left_folded,
            self.theme(),
            self.profiles.current,
        );
        LEFT_CACHE.with(|c| {
            if let Some((k, v)) = c.borrow().as_ref() {
                // the tree holds still through a drag: the model changes
                // each frame, its names and rows don't (5 ms a frame)
                if *k == key || self.dragging() {
                    return v.clone();
                }
            }
            let v = self.left_state_now();
            *c.borrow_mut() = Some((key, v.clone()));
            v
        })
    }

    fn left_state_now(&self) -> Value {
        let model = self.doc.model();
        // the search finds elements by their marks (D01, W05) too
        let marks = creabim_core::marks::marks(model);
        let mark = |e: &Element| match &e.kind {
            ElementKind::Object(_) => creabim_core::marks::mark_of(model, e.id),
            _ => marks.get(&e.id).cloned(),
        };
        let row = |e: &Element, opens: bool| {
            json!({
                "id": e.id.0.to_string(), "name": e.name, "icon": icon_name(Icon::of(e)),
                "mark": mark(e),
                "sel": self.selection.contains(e.id),
                "active": opens && self.active_view == Some(e.id),
                "hint": if opens {
                    format!("{}: click to open", e.name)
                } else if matches!(e.kind, ElementKind::ElementType(_)) {
                    format!("{}: double-click to see it on its own, in 3D, plan and section", e.name)
                } else {
                    format!("{}: double-click to frame it in the 3D view", e.name)
                },
            })
        };
        // views by kind, plans in their levels' order; drag a row to move it
        let mut out: Vec<Value> = crate::browser::view_groups(model)
            .into_iter()
            .map(|(g, ids)| {
                let rows: Vec<Value> = ids
                    .iter()
                    .filter_map(|id| model.get(*id))
                    .map(|e| row(e, true))
                    .collect();
                let manual = crate::browser::is_manual(model, g);
                let icon = match g {
                    crate::browser::ViewGroup::Plans => "map",
                    crate::browser::ViewGroup::Views3d => "view-3d",
                    crate::browser::ViewGroup::Sections => "scissors-line-dashed",
                    crate::browser::ViewGroup::Elevations => "elevation",
                    crate::browser::ViewGroup::Details => "callout",
                    crate::browser::ViewGroup::Schedules => "table-2",
                };
                let mut acts = vec![];
                if manual {
                    acts.push(json!({
                        "id": format!("browser.sort:{}", g.key()),
                        "icon": "sort-by-level",
                        "hint": match g {
                            crate::browser::ViewGroup::Plans => "Sort by level: put the plans back in the order of their levels, lowest first".to_string(),
                            _ => format!("{}: undo the order you dragged them into", g.auto_label()),
                        },
                    }));
                }
                json!({
                    "key": g.key(), "title": g.title(), "icon": icon, "open": true,
                    "rows": rows, "acts": acts, "sortable": true,
                    "note": if manual { "Your order" } else if g == crate::browser::ViewGroup::Plans { "By level" } else { "" },
                })
            })
            .collect();
        let mut groups: Vec<(String, Vec<&Element>)> = vec![];
        for e in model.iter() {
            if matches!(e.kind, ElementKind::Units(_) | ElementKind::View(_)) {
                continue;
            }
            let class = class_of(e);
            match groups.iter_mut().find(|(c, _)| *c == class) {
                Some((_, v)) => v.push(e),
                None => groups.push((class, vec![e])),
            }
        }
        let rank = |c: &str| match c {
            "View" => 0,
            "Sheet" => 1,
            "IfcBuildingStorey" => 2,
            _ => 3,
        };
        groups.sort_by_key(|(c, _)| rank(c));
        let groups: Vec<Value> = groups
            .into_iter()
            .map(|(class, items)| {
                let opens = class == "Sheet";
                let mut items = items;
                if class == "IfcBuildingStorey" {
                    // levels from the top down, as in a section
                    let z = |e: &Element| model.level(e.id).map_or(0.0, |l| l.elevation);
                    items.sort_by(|a, b| z(b).total_cmp(&z(a)));
                }
                let mut rows: Vec<Value> = items.iter().map(|e| row(e, opens)).collect();
                // a 3D view's crop boxes say whose they are
                for (r, e) in rows.iter_mut().zip(&items) {
                    if let ElementKind::CropBox(c) = &e.kind {
                        let owner = c.view.and_then(|v| model.get(v));
                        let off = if c.enabled { "" } else { " (off)" };
                        r["name"] = json!(match owner {
                            Some(v) => format!("{} · {}{off}", e.name, v.name),
                            None => format!("{}{off}", e.name),
                        });
                    }
                }
                // levels and assemblies are managed from their groups
                let act = match class.as_str() {
                    "IfcBuildingStorey" => Some(json!([
                        {"id": "level.add", "icon": "plus", "hint": "Add a level above the top one, with its floor plan"},
                        {"id": "levels", "icon": "ellipsis", "hint": "Levels window: add, duplicate, rename, move and delete levels, and make their plans"},
                    ])),
                    "CropBox" => Some(json!([
                        {"id": "crop.add", "icon": "plus", "hint": "Add a crop box to the 3D view: it crops everything outside it"},
                    ])),
                    "IfcMaterialLayerSet" => Some(json!([
                        {"id": "asm.browser", "icon": "ellipsis", "hint": "Assemblies: edit layers, add, duplicate, delete, assign"},
                    ])),
                    "IfcFurniture" => Some(json!([
                        {"id": "catalogue", "icon": "plus", "hint": "Catalogue: furniture and fixtures to place"},
                    ])),
                    "ElementType" => Some(json!([
                        {"id": "types.browser", "icon": "ellipsis", "hint": "Types: edit, add, duplicate, delete, assign, select instances"},
                    ])),
                    _ => None,
                };
                // each group its own picture (MU29: many shared the cube)
                let title = group_title(&class);
                let icon = match class.as_str() {
                    "ElementType" => "element-types",
                    "IfcFurniture" => "sofa",
                    _ => match title.to_lowercase().as_str() {
                        t if t.contains("curtain") => "curtain-wall",
                        t if t.contains("opening") || t.contains("shaft") => "shaft",
                        t if t.contains("railing") => "railing",
                        t if t.contains("footing") => "footing",
                        t if t.contains("room") => "room",
                        t if t.contains("ceiling") => "ceiling",
                        t if t.contains("join") => "join",
                        t if t.contains("stair") => "stair",
                        t if t.contains("detail") => "callout",
                        t if t.contains("note") => "sticky-note",
                        t if t.contains("text style") => "anno-styles",
                        t if t.contains("tag") => "tag",
                        t if t.contains("template") => "sheet-template",
                        t if t.contains("planting") || t.contains("tree") => "tree",
                        t if t.contains("void") => "void",
                        _ => icon_name(Icon::of_class(&class)),
                    },
                };
                json!({
                    "key": class, "title": group_title(&class), "icon": icon,
                    "open": rank(&class) < 3, "rows": rows, "acts": act,
                })
            })
            .collect();
        out.extend(groups);
        out.push(self.profiles_group());
        out.push(self.masters_group());
        let groups = out;
        json!({
            "theme": self.theme(),
            "count": format!("{} elements", self.element_count()),
            "folded": self.shell.settings.left_folded,
            "groups": groups,
        })
    }

    pub(super) fn status_state(&self) -> Value {
        let mut s = self.plan.snap;
        let n = s.switches().iter().filter(|(_, v)| **v).count();
        let total = s.switches().len();
        let snaps = match n {
            n if n == total => "All".to_string(),
            0 => "None".to_string(),
            n => format!("{n} of {total}"),
        };
        let workplane = match &self.workplane {
            Some((_, name)) => name.clone(),
            None => "Level".into(),
        };
        let mut state = json!({
            "theme": self.theme(),
            "popup": self.popup_key(),
            "message": self.status,
            // what the drawing under the pointer does, after the message
            "hint": self.plan.hint.filter(|h| *h != self.status),
            // what each mouse button does here, as Blender's status bar (MU39)
            "mouse": self.mouse_hints(),
            "controls": [
                {"kind": "toggle", "id": "snap.on", "label": "Snap", "on": self.plan.snap.on, "icon": "magnet",
                 "hint": "Object snaps on or off (F3). Hold Alt while picking to suspend them."},
                {"kind": "menu", "key": "snaps", "label": "", "value": snaps, "hint": "Which snaps are on"},
                {"kind": "menu", "key": "step", "label": "Grid snap", "value": self.plan.snap.step_label(),
                 "hint": "Off: points and drags go exactly where the pointer is. On (Alt+S): they land on a grid in space. Also: distance marks while dragging."},
                {"kind": "sep"},
                {"kind": "menu", "key": "workplane", "label": "Workplane", "value": workplane, "icon": "square-dashed",
                 "hint": "Where sketches and guides drawn in 3D go"},
                {"kind": "menu", "key": "pivot", "label": "Pivot", "value": self.plan.pivot.label(), "icon": "crosshair",
                 "hint": "What rotate and scale turn about. Shift+right-click places the pivot cursor."},
                {"kind": "menu", "key": "units.menu", "label": "Units", "value": crate::app::appmenu::unit_short(self.doc.model().units().length), "icon": "units",
                 "hint": "How lengths are shown and typed. Any unit can still be typed: 3 m, 10'6\", 450 mm."},
                {"kind": "sep"},
                {"kind": "meta", "label": format!("{} elements", self.element_count())},
            ],
        });
        // right after a box took several kinds, a way to keep one of them
        if self.offers_keep_only() {
            if let Some(c) = state["controls"].as_array_mut() {
                c.insert(0, json!({"kind": "menu", "key": "keeponly", "label": "Keep only", "value": "…", "icon": "square-dashed-mouse-pointer",
                    "hint": "Keep only one kind of what the box took: the walls, the doors…"}));
            }
        }
        // right after a footing went on a host, the plan that shows it
        if self.plan.footing_placed && self.status.contains(" footing placed under ") {
            let label = if crate::memberstool::foundation_plan(self.doc.model()).is_some() {
                "Show in Foundation plan"
            } else {
                "Make a Foundation plan"
            };
            if let Some(c) = state["controls"].as_array_mut() {
                c.insert(0, json!({"kind": "button", "id": "footing.foundation", "label": label, "icon": "footing",
                    "hint": "Open the plan where footings are drawn, with the floor above as its underlay"}));
            }
        }
        // right after an export, the way to the file it wrote
        if self.last_export.as_deref() == Some(self.status.as_str()) {
            if let Some(c) = state["controls"].as_array_mut() {
                c.insert(0, json!({"kind": "button", "id": "export.folder", "label": "Open the folder", "icon": "folder-open",
                    "hint": "Show the exported file in the system's file browser"}));
            }
        }
        state
    }

    fn theme(&self) -> &'static str {
        self.shell.theme.id()
    }

    /// Switch theme everywhere and remember it for next time.
    pub(super) fn set_theme(&mut self, ctx: &egui::Context, t: Theme) {
        self.shell.theme = t;
        style(ctx, t);
        self.shell.settings.theme = t.id().into();
        self.shell.settings.store();
        self.status = format!("Theme: {}.", t.label());
    }

    fn popup_key(&self) -> Option<String> {
        self.shell.popup.as_ref().map(|p| p.key.clone())
    }

    /// While the palette is open, the keys typed in the views go to it: its
    /// page doesn't always get the keyboard when opened from a shortcut.
    pub(super) fn palette_keys(&mut self, raw: &mut egui::RawInput) {
        let Some(key) = self.popup_key() else { return };
        let Some(web) = &self.shell.web else { return };
        if key != "palette" {
            // arrows and Enter walk an open menu
            let mut keys = vec![];
            raw.events.retain(|e| match e {
                egui::Event::Key {
                    key, pressed: true, ..
                } if matches!(
                    key,
                    egui::Key::ArrowDown
                        | egui::Key::ArrowUp
                        | egui::Key::ArrowLeft
                        | egui::Key::ArrowRight
                        | egui::Key::Enter
                ) =>
                {
                    keys.push(match key {
                        egui::Key::ArrowDown => "ArrowDown",
                        egui::Key::ArrowUp => "ArrowUp",
                        egui::Key::ArrowLeft => "ArrowLeft",
                        egui::Key::ArrowRight => "ArrowRight",
                        _ => "Enter",
                    });
                    false
                }
                _ => true,
            });
            for k in keys {
                web.run(Pane::Popup, &format!("window.CB && CB.nav({})", json!(k)));
            }
            return;
        }
        let mut text = String::new();
        let mut keys = vec![];
        raw.events.retain(|e| match e {
            egui::Event::Text(t) => {
                text.push_str(t);
                false
            }
            egui::Event::Key { key, pressed, .. } => {
                if *pressed {
                    // the page's names, as a browser gives them
                    keys.push(match key {
                        egui::Key::ArrowDown => "ArrowDown",
                        egui::Key::ArrowUp => "ArrowUp",
                        k => k.name(),
                    });
                }
                false
            }
            _ => true,
        });
        if !text.is_empty() || !keys.is_empty() {
            web.run(
                Pane::Popup,
                &format!(
                    "window.CB && CB.feed({})",
                    json!({"text": text, "keys": keys})
                ),
            );
        }
    }

    /// The command palette: every action, found by typing.
    pub(super) fn open_palette(&mut self, ctx: &egui::Context) {
        let screen = ctx.viewport_rect();
        let w = 560.0f32.min(screen.width() - 32.0);
        self.shell.popup = Some(Popup {
            key: "palette".into(),
            at: egui::pos2(screen.center().x - w / 2.0, screen.top() + 90.0),
            up: None,
            above: None,
            size: egui::vec2(w, 380.0),
        });
        self.shell.focus_popup = true;
    }

    /// The New project window, in the middle of the window.
    pub(super) fn open_new_project(&mut self, ctx: &egui::Context) {
        if self.shell.web.is_none() {
            // the classic panels keep the File menu's list
            self.new_project_classic = true;
            return;
        }
        let screen = ctx.viewport_rect();
        let size = egui::vec2(
            900.0f32.min(screen.width() - 32.0),
            520.0f32.min(screen.height() - 32.0),
        );
        self.shell.popup = Some(Popup {
            key: "newproject".into(),
            at: screen.center() - size / 2.0,
            up: None,
            above: None,
            size,
        });
        self.shell.focus_popup = true;
    }

    /// The New project window's choices, for its page.
    fn new_project_state(&self) -> Value {
        let card = |s: &crate::app::appmenu::Start| json!({"id": s.id, "title": s.title, "line": s.line, "icon": s.icon});
        json!({
            "title": "New project",
            "starts": crate::app::appmenu::STARTS.iter().map(card).collect::<Vec<_>>(),
            "samples_title": "Samples to learn from · each shows one thing",
            "samples": crate::app::appmenu::SAMPLES.iter().map(card).collect::<Vec<_>>(),
            "note": "A sample opens as a new, unsaved project, so the original is never changed.",
        })
    }

    fn close_popup(&mut self) {
        self.shell.popup = None;
        if let Some(web) = &mut self.shell.web {
            web.hide_popup();
        }
    }

    fn handle_web(&mut self, ctx: &egui::Context, msg: Msg) {
        let b = &msg.body;
        let t = b["t"].as_str().unwrap_or("");
        match t {
            "ready" => {
                if msg.pane == Pane::Help {
                    self.shell.help.resend();
                }
                if let Some(web) = &mut self.shell.web {
                    web.resend(msg.pane);
                }
            }
            "down" if msg.pane != Pane::Popup => self.close_popup(),
            "focus" => {
                if let Some(web) = &mut self.shell.web {
                    web.focus_views();
                }
            }
            "close" => self.close_popup(),
            "bar" => self.shell.bar = Some(crate::optionbar::Measure::from_json(b)),
            // a double-click in the project browser: a type opens on its
            // own, anything else in the model is framed in 3D
            "open" => {
                let want = b["id"].as_str().unwrap_or("");
                let Some(e) = self.doc.model().iter().find(|e| e.id.0.to_string() == want) else {
                    return;
                };
                match e.kind {
                    ElementKind::ElementType(_) => self.open_type_preview(e.id),
                    ElementKind::View(_) | ElementKind::Sheet(_) => {}
                    _ => {
                        self.selection.only(e.id);
                        self.frame_selection();
                    }
                }
            }
            "reorder" => {
                let id = |k: &str| {
                    let s = b[k].as_str()?;
                    self.doc
                        .model()
                        .iter()
                        .find(|e| e.id.0.to_string() == s)
                        .map(|e| e.id)
                };
                if let Some(moved) = id("id") {
                    let tx = crate::browser::reorder(self.doc.model(), moved, id("before"));
                    let r = self.doc.commit(tx);
                    let name = self
                        .doc
                        .model()
                        .get(moved)
                        .map(|e| e.name.clone())
                        .unwrap_or_default();
                    self.report(
                        r,
                        format!("Moved {name}. The group keeps your order; its sort button puts it back."),
                    );
                }
            }
            "act" => {
                let id = b["id"].as_str().unwrap_or("").to_string();
                let keep = b["keep"].as_bool().unwrap_or(false);
                if msg.pane == Pane::Popup && !keep {
                    self.close_popup();
                }
                self.run_action(ctx, &id);
            }
            "tip" => {
                // a web row's hint, drawn by the app in its theme (MU26)
                let text = b["text"].as_str().unwrap_or("").trim().to_string();
                self.shell.web_tip = match (&self.shell.web, text.is_empty()) {
                    (Some(web), false) => {
                        let o = web.rects[msg.pane as usize].min;
                        let f = |k: &str| b[k].as_f64().unwrap_or(0.0) as f32;
                        let r = egui::Rect::from_min_size(
                            o + egui::vec2(f("x"), f("y")),
                            egui::vec2(f("w"), f("h")),
                        );
                        Some((text, r))
                    }
                    _ => None,
                };
            }
            "popup" => {
                let Some(web) = &self.shell.web else { return };
                let origin = web.rects[msg.pane as usize].min;
                let x = b["x"].as_f64().unwrap_or(0.0) as f32;
                let y = b["y"].as_f64().unwrap_or(0.0) as f32;
                let w = b["w"].as_f64().unwrap_or(0.0) as f32;
                let up = b["up"].as_f64().map(|u| origin.y + u as f32);
                let above = b["top"].as_f64().map(|u| origin.y + u as f32);
                let key = b["key"].as_str().unwrap_or("").to_string();
                let n = self.menu_items(&key).len() as f32;
                self.shell.popup = Some(Popup {
                    key,
                    at: origin + egui::vec2(x, y),
                    up,
                    above,
                    size: egui::vec2(w.max(220.0), 12.0 + 29.0 * n),
                });
            }
            "size" => {
                // the New project window keeps the size it opened with
                if let Some(p) = self.shell.popup.as_mut().filter(|p| p.key != "newproject") {
                    let h = b["h"].as_f64().unwrap_or(100.0) as f32;
                    // the palette keeps the width it was opened with
                    let w = match p.key.as_str() {
                        "palette" => p.size.x,
                        _ => b["w"].as_f64().unwrap_or(200.0) as f32,
                    };
                    p.size = egui::vec2(w, h);
                }
            }
            "set" => {
                // a length field sends what was typed, in any unit
                let v = match (b["value"].as_f64(), b["text"].as_str()) {
                    (Some(v), _) => v,
                    (None, Some(t)) => match crate::units::parse(t) {
                        Some(v) => v,
                        None => {
                            self.status = format!(
                                "Couldn't read \"{t}\" as a length. Try 3'-6 1/2\", 1067 or 1.2 m."
                            );
                            return;
                        }
                    },
                    _ => return,
                };
                self.set_tool_number(b["key"].as_str().unwrap_or(""), v);
            }
            "pick" => {
                let want = b["id"].as_str().unwrap_or("");
                // a profile of the browser's Profiles group (MU27 block 5)
                if let Some(name) = want.strip_prefix("profile:") {
                    let name = name.to_string();
                    self.profiles_action("profiles.open", &name);
                    return;
                }
                // a master of the browser's Masters group (MU40)
                if let Some(id) = want.strip_prefix("master:") {
                    let id = id.to_string();
                    self.masters_action("master.open", &id);
                    return;
                }
                let Some(e) = self.doc.model().iter().find(|e| e.id.0.to_string() == want) else {
                    return;
                };
                let (id, opens) = (
                    e.id,
                    matches!(e.kind, ElementKind::View(_) | ElementKind::Sheet(_)),
                );
                if b["add"].as_bool().unwrap_or(false) {
                    self.selection.toggle(id);
                } else {
                    self.selection.only(id);
                    if opens {
                        self.active_view = Some(id);
                    }
                }
                self.face = None;
            }
            "key" => {
                let code = b["code"].as_str().unwrap_or("");
                let name = b["key"].as_str().unwrap_or("");
                let Some(key) = egui::Key::from_name(code).or_else(|| egui::Key::from_name(name))
                else {
                    return;
                };
                let ctrl = b["ctrl"].as_bool().unwrap_or(false);
                let modifiers = egui::Modifiers {
                    alt: b["alt"].as_bool().unwrap_or(false),
                    ctrl,
                    shift: b["shift"].as_bool().unwrap_or(false),
                    mac_cmd: false,
                    command: ctrl,
                };
                for pressed in [true, false] {
                    self.shell.keys.push(egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: b["repeat"].as_bool().unwrap_or(false),
                        modifiers,
                    });
                }
                if !ctrl && name.chars().count() == 1 {
                    self.shell.keys.push(egui::Event::Text(name.into()));
                }
                ctx.request_repaint();
            }
            _ => {}
        }
    }

    /// Lay out the web panes round the views. Returns false when there is
    /// no web interface, and the classic panels are drawn instead.
    pub(super) fn web_panels(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) -> bool {
        if self.shell.classic || self.shell.failed.is_some() {
            WebUi::pump();
            if let Some(web) = &self.shell.web {
                for m in web.take() {
                    let _ = m;
                }
            }
            return false;
        }
        if self.shell.web.is_none() {
            match WebUi::new(frame, ui.ctx()) {
                Ok(w) => self.shell.web = Some(w),
                Err(e) => {
                    self.status = format!("Using the classic panels: {e}");
                    self.shell.failed = Some(e);
                    return false;
                }
            }
        }
        let ctx = ui.ctx().clone();
        WebUi::pump();
        let msgs = self
            .shell
            .web
            .as_ref()
            .map(|w| w.take())
            .unwrap_or_default();
        for m in msgs {
            self.handle_web(&ctx, m);
        }
        // what the app draws itself: below the bars, right of the browser,
        // above the status bar
        if let Some(web) = &self.shell.web {
            use crate::webui::Pane;
            let mut r = ctx.content_rect();
            let top = web.rects[Pane::Top as usize];
            let left = web.rects[Pane::Left as usize];
            let status = web.rects[Pane::Status as usize];
            if top.height() > 1.0 {
                r.min.y = r.min.y.max(top.bottom());
            }
            if left.width() > 1.0 {
                r.min.x = r.min.x.max(left.right());
            }
            if status.height() > 1.0 {
                r.max.y = r.max.y.min(status.top());
            }
            NATIVE.with(|n| n.set(Some(r)));
        }
        // a web row's hint, as the app's own tooltips look (MU26)
        if let Some((text, r)) = self
            .shell
            .web_tip
            .clone()
            .filter(|_| self.shell.popup.is_none())
        {
            let screen = ctx.content_rect();
            let below = r.bottom() + 6.0;
            // web panels paint over the app's own drawing, so the tip
            // keeps to the right of the browser panel (MU27: it was cut)
            let left_edge = self
                .shell
                .web
                .as_ref()
                .map(|w| w.rects[crate::webui::Pane::Left as usize])
                .filter(|l| l.width() > 1.0 && l.top() < below + 40.0 && l.bottom() > below)
                .map_or(screen.left(), |l| l.right());
            let x = r.left().min(screen.right() - 380.0).max(left_edge + 4.0);
            egui::Area::new(egui::Id::new("web-tip"))
                .order(egui::Order::Tooltip)
                .fixed_pos(egui::pos2(x, below))
                .interactable(false)
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.label(text);
                    });
                });
        }
        // a tool picked off the tab shown (by its key, a menu or the
        // palette) brings its own tab
        if self.shell.tab_tool != Some(self.plan.tool) {
            self.shell.tab_tool = Some(self.plan.tool);
            if !crate::tooltabs::holds(self.shell.settings.tools_tab, self.plan.tool) {
                self.shell.settings.tools_tab = crate::tooltabs::home_of(self.plan.tool);
            }
        }
        if self.shell.classic {
            // just switched to the classic panels
            return false;
        }
        // A click in the views or Escape closes an open menu.
        if self.shell.popup.is_some()
            && ctx.input(|i| i.pointer.any_pressed() || i.key_pressed(egui::Key::Escape))
        {
            self.close_popup();
        }

        let ppp = ctx.pixels_per_point();
        let none = egui::Frame::NONE;
        let top = egui::Panel::top("web_top")
            .exact_size(TOP_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(none)
            .show(ui, |_| ())
            .response
            .rect;
        let status = egui::Panel::bottom("web_status")
            .exact_size(STATUS_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(none)
            .show(ui, |_| ())
            .response
            .rect;
        // drag the browser's right edge to widen it
        let folded = self.shell.settings.left_folded;
        let left = egui::Panel::left(if folded { "web_left_rail" } else { "web_left" })
            .default_size(if folded {
                44.0
            } else {
                self.shell.settings.left_width
            })
            .size_range(if folded { 44.0..=44.0 } else { 180.0..=520.0 })
            .resizable(!folded)
            .show_separator_line(true)
            .frame(egui::Frame::NONE.fill(ui.visuals().panel_fill))
            .show(ui, |ui| ui.allocate_space(ui.available_size()))
            .response
            .rect;
        // the page sits just inside the edge so the edge stays grabbable
        let left = left.with_max_x(left.max.x - 3.0);
        // the browser's fold handle on its right edge (MU29)
        if crate::app::fold_handle(
            &ctx,
            "browser",
            egui::pos2(left.right() + 3.0, left.center().y - 23.0),
            folded,
            false,
            if folded {
                "Open the project browser (Ctrl+[)"
            } else {
                "Fold the browser to a rail (Ctrl+[)"
            },
        ) {
            self.shell.settings.left_folded = !folded;
            self.shell.settings.store();
        }
        let width = (left.width() + 3.0).round();
        if !folded
            && (width - self.shell.settings.left_width).abs() >= 1.0
            && !ctx.input(|i| i.pointer.any_down())
        {
            self.shell.settings.left_width = width;
            self.shell.settings.store();
        }
        // the guide, at the right of everything while it is open; drag
        // its left edge to widen it
        let help = self.shell.help.open.then(|| {
            let r = egui::Panel::right("web_help")
                .default_size(self.shell.settings.help_width)
                .size_range(300.0..=760.0)
                .resizable(true)
                .show_separator_line(true)
                .frame(egui::Frame::NONE.fill(ui.visuals().panel_fill))
                .show(ui, |ui| ui.allocate_space(ui.available_size()))
                .response
                .rect;
            let width = r.width().round();
            if (width - self.shell.settings.help_width).abs() >= 1.0
                && !ctx.input(|i| i.pointer.any_down())
            {
                self.shell.settings.help_width = width;
                self.shell.settings.store();
            }
            r.with_min_x(r.min.x + 3.0)
        });
        let help_state = help.and_then(|_| self.help_state());

        // the 3D view's buttons, over its top-right corner (where it was
        // drawn last frame)
        let nav = self
            .rect_3d
            // docked, the 3D area's header has these
            .filter(|_| self.placement != PlanPlacement::Docked)
            .filter(|r| r.width() > 320.0 && r.height() > 120.0)
            .map(|r| {
                // as wide as it needs, or as the view when that is narrower
                let w = NAV_W.min(r.width() - 16.0);
                // the overlaid 2D view takes the top-right corner
                let x = if self.placement == PlanPlacement::Overlay {
                    r.left() + 36.0
                } else {
                    r.right() - w - 8.0
                };
                egui::Rect::from_min_size(egui::pos2(x, r.top() + 8.0), egui::vec2(w, NAV_H))
            })
            // a web pane draws over everything egui draws, so the buttons
            // step aside while a window (Types, Assemblies, materials, a
            // type's preview) lies over them
            .filter(|n| {
                !ctx.memory(|m| {
                    m.areas().visible_layer_ids().into_iter().any(|l| {
                        l.order == egui::Order::Middle
                            && m.area_rect(l.id).is_some_and(|a| a.intersects(*n))
                    })
                })
            });
        let states = [
            (Pane::Top, top, {
                let _p = crate::app::Perf::new("top_state");
                self.top_state(&ctx)
            }),
            (Pane::Left, left, {
                let _p = crate::app::Perf::new("left_state");
                self.left_state()
            }),
            (Pane::Status, status, self.status_state()),
        ];
        let nav_state = self.nav_state();
        // web menus float over the whole window, the web bands included
        let screen = ctx.viewport_rect();
        let popup = self.shell.popup.as_ref().map(|p| {
            let up = p.up.or_else(|| opens_up(p, screen));
            let size = popup_size(&Popup { up, ..p.clone() }, screen);
            let mut at = p.at;
            if let Some(bottom) = up {
                at.y = bottom - size.y;
            }
            at.x =
                at.x.min(screen.right() - size.x - 4.0)
                    .max(screen.left() + 4.0);
            at.y =
                at.y.min(screen.bottom() - size.y - 4.0)
                    .max(screen.top() + 4.0);
            let rect = egui::Rect::from_min_size(at, size);
            match p.key.as_str() {
                // the strip's controls that don't fit it
                "more" => {
                    let (head, tool, view, _) = self.option_strip(&ctx);
                    let rows = crate::optionbar::more_rows(
                        &tool,
                        &view,
                        head["label"].as_str().unwrap_or(""),
                    );
                    let empty = rows.is_empty();
                    let state = json!({"theme": self.theme(), "key": p.key, "items": [], "controls": rows});
                    return (rect, state, empty);
                }
                // the New project window keeps its own size
                "newproject" => {
                    let rect = egui::Rect::from_center_size(screen.center(), p.size);
                    let state = json!({"theme": self.theme(), "key": p.key, "items": [], "dialog": self.new_project_state()});
                    return (rect, state, false);
                }
                // a few lines on the tool in use
                "toolhelp" => {
                    // never over a view's own toolbar or over Properties:
                    // under the 3D view's buttons when it would cover them
                    let mut rect = rect;
                    if let Some(n) = nav.filter(|n| n.intersects(rect)) {
                        rect = rect.translate(egui::vec2(0.0, n.bottom() + 6.0 - rect.top()));
                    }
                    let right = [self.rect_2d, self.rect_3d]
                        .into_iter()
                        .flatten()
                        .map(|r| r.right())
                        .fold(f32::NEG_INFINITY, f32::max);
                    if right.is_finite() && rect.right() > right - 4.0 {
                        rect = rect.translate(egui::vec2(right - 4.0 - rect.right(), 0.0));
                    }
                    let help = self.tool_quick_help();
                    let state = json!({"theme": self.theme(), "key": p.key, "items": [], "help": help});
                    return (rect, state, false);
                }
                _ => {}
            }
            let items = self.menu_items(&p.key);
            // the Create menu lists every tool: its sections side by side
            let columns = match p.key.as_str() {
                "menu:Create" => 3,
                _ => 1,
            };
            (
                rect,
                json!({"theme": self.theme(), "key": p.key, "items": ui_model::list(&items), "columns": columns}),
                items.is_empty(),
            )
        });
        let icons_extra = self.profile_thumbs();
        let Some(web) = &mut self.shell.web else {
            return false;
        };
        for (pane, rect, state) in states {
            web.place(pane, rect, ppp);
            web.send(pane, &state);
        }
        match nav {
            Some(rect) => {
                web.place(Pane::Nav, rect, ppp);
                web.send(Pane::Nav, &nav_state);
            }
            None => web.park(Pane::Nav),
        }
        match help {
            Some(rect) => {
                web.place(Pane::Help, rect, ppp);
                if let Some(state) = help_state {
                    web.send(Pane::Help, &state);
                }
            }
            None => web.park(Pane::Help),
        }
        match popup {
            Some((_, _, true)) => self.close_popup(),
            Some((rect, state, false)) => {
                let mut state = state;
                state["icons"] = serde_json::Value::Object(icons_extra);
                web.send(Pane::Popup, &state);
                web.show_popup(rect, ppp);
                if std::mem::take(&mut self.shell.focus_popup) {
                    web.focus(Pane::Popup);
                }
            }
            None => web.hide_popup(),
        }
        // Pages wake the app when they send a message (see WebUi::new). On
        // Linux they live on GTK, which only runs while frames pump it.
        #[cfg(target_os = "linux")]
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
        true
    }

    /// Open the menu `key` under `r` (a button drawn by egui), or close it
    /// when it is open already.
    pub(super) fn toggle_popup(&mut self, key: &str, r: egui::Rect) {
        if self.shell.web.is_none() {
            return;
        }
        if self.shell.popup.as_ref().is_some_and(|p| p.key == key) {
            self.shell.popup = None;
            return;
        }
        let n = self.menu_items(key).len() as f32;
        // a menu that would run off the views' right edge ends under its button
        let right = self
            .areas
            .rects
            .iter()
            .map(|a| a.right())
            .fold(r.right(), f32::max);
        let x = if r.left() + 240.0 > right {
            r.right() - 240.0
        } else {
            r.left()
        };
        self.shell.popup = Some(Popup {
            key: key.into(),
            at: egui::pos2(x, r.bottom() + 4.0),
            up: None,
            above: Some(r.top() - 4.0),
            size: egui::vec2(240.0, 12.0 + 29.0 * n),
        });
    }

    /// Open the right-click menu a view asked for this frame.
    pub(super) fn open_view_menu(&mut self) {
        let Some(pos) = self.plan.menu_request.take() else {
            return;
        };
        if self.shell.web.is_none() {
            return;
        }
        let n = self.menu_items("context").len() as f32;
        self.shell.context_3d = self.shell.menu_3d;
        self.shell.popup = Some(Popup {
            key: "context".into(),
            at: pos + egui::vec2(2.0, 2.0),
            up: None,
            above: Some(pos.y - 2.0),
            size: egui::vec2(230.0, 12.0 + 28.0 * n),
        });
    }

    /// The classic egui menu bar, drawn from the same items as the web one.
    pub(super) fn classic_menu(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        egui::MenuBar::new().ui(ui, |ui| {
            for m in ["File", "Edit", "View", "Materials", "Help"] {
                let items = self.menu_items(&format!("menu:{m}"));
                ui.menu_button(m, |ui| {
                    if let Some(id) = ui_model::show(ui, &items) {
                        picked = Some(id);
                    }
                });
            }
            ui.separator();
            ui.label("File:");
            ui.add(
                egui::TextEdit::singleline(&mut self.path)
                    .vertical_align(egui::Align::Center)
                    .desired_width(220.0),
            );
        });
        if let Some(id) = picked {
            let ctx = ui.ctx().clone();
            self.run_action(&ctx, &id);
        }
    }
}

/// How big a popup is shown: as its page asks, but never past the window.
/// A menu opening down keeps to the room below where it opens (or the
/// whole window height when that is too little); the page scrolls the rest.
/// Where the bottom of a menu goes that has no room below its button but
/// more above it: it opens upwards from the button's top.
fn opens_up(p: &Popup, screen: egui::Rect) -> Option<f32> {
    let top = p.above?;
    let below = screen.bottom() - p.at.y - 6.0;
    let above = top - screen.top() - 6.0;
    (below < p.size.y && above > below).then_some(top)
}

fn popup_size(p: &Popup, screen: egui::Rect) -> egui::Vec2 {
    let margin = 6.0;
    let room = match p.up {
        Some(bottom) => bottom - screen.top() - margin,
        None => screen.bottom() - p.at.y - margin,
    };
    let room = if room < 240.0 {
        screen.height() - 2.0 * margin
    } else {
        room
    };
    egui::vec2(
        p.size.x.min(screen.width() - 2.0 * margin),
        p.size.y.min(room).max(40.0),
    )
}

/// Where a new project or a sample would be saved: beside the projects
/// opened before, never over one of them.
fn new_path(name: &str) -> String {
    format!("{name}.creabim")
}

/// egui's spacing and type, roomier than its defaults.
pub(super) fn style(ctx: &egui::Context, theme: Theme) {
    let p = theme.palette();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("theme.palette"), p));
    let mode = if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(mode);
    ctx.set_visuals_of(mode, visuals(theme));
    ctx.all_styles_mut(|s| {
        use egui::{FontId, TextStyle};
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 24.0;
        // a slider's track thick enough to read as one
        s.spacing.slider_rail_height = 6.0;
        // soft corners on every menu, list and popup, as the web menus
        // have (MU35: the app's own were square)
        s.visuals.menu_corner_radius = egui::CornerRadius::same(10);
        s.visuals.window_corner_radius = egui::CornerRadius::same(10);
        s.spacing.menu_margin = egui::Margin::same(6);
        // thin floating scroll bars, like the web panels' (MU18)
        let mut scroll = egui::style::ScrollStyle::thin();
        scroll.foreground_color = false;
        s.spacing.scroll = scroll;
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        s.text_styles
            .insert(TextStyle::Heading, FontId::proportional(16.0));
    });
}

/// `a` moved `t` of the way to `b`.
fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// egui's look, matched to the web interface so the properties panel and
/// the views' own controls sit with it. Every control has a fill and an
/// outline that stand out from the window it is in, so a button reads as
/// a button, a dropdown as a field with its arrow, a text box as a box and
/// a slider as a track; hovering strengthens both.
pub(super) fn visuals(theme: Theme) -> egui::Visuals {
    use egui::{CornerRadius, Stroke};
    let p = theme.palette();
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.panel;
    v.window_fill = p.panel2;
    v.faint_bg_color = p.panel2;
    v.extreme_bg_color = p.field;
    v.text_edit_bg_color = Some(p.field);
    v.slider_trailing_fill = true;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(8);
    // also the filled part of a slider's track
    v.selection.bg_fill = p.accent.gamma_multiply(if p.dark { 0.45 } else { 0.4 });
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;
    // a steady caret in the accent colour (MU27: the app repaints only on
    // input, so a blinking one was often caught off and seemed missing)
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.text_cursor.blink = false;
    v.weak_text_color = Some(p.muted);
    v.override_text_color = None;
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke.color = p.text;
    w.noninteractive.bg_fill = p.panel;
    w.inactive.fg_stroke.color = p.text;
    // buttons, dropdowns and slider tracks are filled a step off the
    // window; text fields stay light (dark in the dark themes), so the two
    // read apart
    let control = if p.dark {
        p.raise
    } else {
        mix(p.panel, p.raise2, 0.7)
    };
    let hover = if p.dark {
        p.raise2
    } else {
        mix(p.raise2, p.edge, 0.6)
    };
    for (state, fill, edge) in [
        (&mut w.inactive, control, p.edge),
        (&mut w.hovered, hover, p.muted),
        (&mut w.active, hover, p.accent),
        (&mut w.open, control, p.accent),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = CornerRadius::same(6);
        state.bg_stroke = Stroke::new(1.0, edge);
    }
    w.noninteractive.corner_radius = CornerRadius::same(6);
    v
}

/// The Align tool, in the Edit and right-click menus.
fn align_item() -> Item {
    Item::new("align", "Align to a reference…")
        .icon("align-tool")
        .hint("Pick a reference line (a wall face, an edge, a guide or a grid line), then an edge to bring flush with it")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key as a page forwards it: pressed and let go in one frame, with
    /// its modifiers on the key only.
    fn forwarded(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        [true, false]
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            })
            .to_vec()
    }

    #[test]
    fn ctrl_s_typed_in_a_page_saves_and_does_not_scale() {
        let ctx = egui::Context::default();
        let mut shell = Shell {
            keys: forwarded(egui::Key::S, egui::Modifiers::COMMAND),
            ..Default::default()
        };
        let mut raw = egui::RawInput::default();
        assert!(shell.feed_keys(&mut raw), "a next frame lets go of Ctrl");
        let mut seen = (false, None);
        ctx.run_ui(raw, |ui| {
            seen = (
                ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)),
                crate::planview::start_keys(ui.ctx()),
            );
        })
        .textures_delta
        .clear();
        assert!(seen.0, "Ctrl+S reads as Save");
        assert_eq!(seen.1, None, "and starts no Scale");

        // the next frame Ctrl is up again, so S alone scales
        let mut raw = egui::RawInput::default();
        assert!(!shell.feed_keys(&mut raw));
        ctx.run_ui(raw, |ui| {
            assert!(!ui.input(|i| i.modifiers.command));
        })
        .textures_delta
        .clear();
        shell.keys = forwarded(egui::Key::S, egui::Modifiers::NONE);
        let mut raw = egui::RawInput::default();
        shell.feed_keys(&mut raw);
        let mut scale = None;
        ctx.run_ui(raw, |ui| scale = crate::planview::start_keys(ui.ctx()))
            .textures_delta
            .clear();
        assert_eq!(scale, Some((crate::modal::Mode::Scale, false)));
    }

    #[test]
    fn browser_rows_carry_marks_for_the_search() {
        let app = crate::app::test_app();
        let model = app.doc.model();
        let door = model
            .iter()
            .find(|e| matches!(&e.kind, ElementKind::Opening(o) if o.kind == creabim_core::OpeningKind::Door))
            .unwrap();
        let mark = creabim_core::marks::mark_of(model, door.id).expect("doors are marked");
        let left = app.left_state();
        let row = left["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["rows"].as_array().unwrap())
            .find(|r| r["id"] == door.id.0.to_string())
            .unwrap();
        assert_eq!(row["mark"], mark.as_str());
    }
}

thread_local! {
    /// The part of the window the app draws itself, clear of the web panels
    /// (the browser, the bars), set each frame.
    static NATIVE: std::cell::Cell<Option<egui::Rect>> = const { std::cell::Cell::new(None) };
}

/// A window kept inside the part of the screen the app draws itself: web
/// panels paint over anything under them (MU28: the note editor slid
/// behind the browser).
pub(crate) fn window(title: impl Into<egui::WidgetText>) -> egui::Window<'static> {
    let w = egui::Window::new(title);
    match NATIVE.with(|n| n.get()) {
        Some(r) if r.width() > 200.0 && r.height() > 150.0 => w.constrain_to(r),
        _ => w,
    }
}
