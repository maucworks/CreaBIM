//! The CreaBIM application menu (About, Check for updates, Settings,
//! Exit), the Units switch in the status bar, the Settings window and the
//! New project window's choices.

use super::*;
use crate::ui_model::Item;
use creabim_core::{LengthUnit, Units};

impl CreaBimApp {
    /// Closing with unsaved changes: Save, Don't save or Cancel.
    pub(super) fn close_window(&mut self, ctx: &egui::Context) {
        if !self.close_ask {
            return;
        }
        let name = std::path::Path::new(&self.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("this project")
            .to_string();
        let pal = crate::theme::Palette::of(ctx);
        let (mut save, mut discard, mut cancel) = (false, false, false);
        egui::Modal::new(egui::Id::new("close-unsaved")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.heading(format!("Save the changes to {name}?"));
            ui.add_space(6.0);
            ui.label("They are lost if you close without saving.");
            ui.label(
                egui::RichText::new("Enter saves, Esc goes back to the project.")
                    .small()
                    .color(pal.muted),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                save = ui.button("Save").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
                discard = ui.button("Don't save").clicked();
                cancel =
                    ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        });
        if cancel {
            self.close_ask = false;
            self.status = "Close cancelled: the project is still open.".into();
            return;
        }
        if save {
            // where it has a file already, there; otherwise Save as asks
            if std::path::Path::new(&self.path).exists() {
                self.save();
            } else {
                self.save_as();
            }
            if self.unsaved() {
                // the save was cancelled or failed: the project stays open
                self.close_ask = false;
                return;
            }
        }
        if save || discard {
            self.close_ask = false;
            self.close_ok = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Where a manual update check looks: the project's releases page. Nothing
/// is fetched in the background; the page opens in the browser when asked.
pub(crate) const RELEASES: &str = "https://github.com/chopinregis/CreaBIM/releases";

/// One way to start a project in the New project window.
pub(crate) struct Start {
    /// The action that opens it.
    pub id: &'static str,
    pub title: &'static str,
    /// One line on what it is, or what it shows.
    pub line: &'static str,
    pub icon: &'static str,
}

/// The two large choices at the top of New project.
pub(crate) const STARTS: [Start; 2] = [
    Start {
        id: "new.empty",
        title: "Empty project",
        line: "Two levels and nothing else, in your units. The everyday start.",
        icon: "file-plus",
    },
    Start {
        id: "new.sample",
        title: "Starter house",
        line: "A complete two-storey house with drawings and sheets, ready to change.",
        icon: "house",
    },
];

/// The samples to learn from, each showing one thing.
pub(crate) const SAMPLES: [Start; 6] = [
    Start {
        id: "new.simple",
        title: "Simple house",
        line: "Walls, doors, windows, a roof",
        icon: "house",
    },
    Start {
        id: "new.layered",
        title: "Layered walls",
        line: "Assemblies and how layers join",
        icon: "layers",
    },
    Start {
        id: "new.buried",
        title: "Beam in a wall",
        line: "Structure cutting through walls",
        icon: "beam",
    },
    Start {
        id: "new.typed",
        title: "Wall types",
        line: "Types, and editing many at once",
        icon: "make-type",
    },
    Start {
        id: "new.furnished",
        title: "Furnished house",
        line: "Objects, fixtures and clearances",
        icon: "sofa",
    },
    Start {
        id: "new.garden",
        title: "House in its garden",
        line: "Terrain, paving and planting",
        icon: "planting",
    },
];

/// A short name for a unit, as the status bar shows it.
pub(crate) fn unit_short(u: LengthUnit) -> &'static str {
    match u {
        LengthUnit::Millimetres => "mm",
        LengthUnit::Centimetres => "cm",
        LengthUnit::Metres => "m",
        LengthUnit::FeetInches => "ft-in",
        LengthUnit::Inches => "in",
        LengthUnit::Feet => "ft",
    }
}

/// A unit's name in the Units list, with an example beside it.
fn unit_row(u: LengthUnit) -> (&'static str, &'static str) {
    match u {
        LengthUnit::Millimetres => ("Millimetres", "3200"),
        LengthUnit::Centimetres => ("Centimetres", "320.0"),
        LengthUnit::Metres => ("Metres", "3.200"),
        LengthUnit::FeetInches => ("Feet and inches", "10'-6\""),
        LengthUnit::Inches => ("Inches", "126\""),
        LengthUnit::Feet => ("Decimal feet", "10.50'"),
    }
}

/// The Units list's order: metric, then imperial.
const UNIT_ORDER: [LengthUnit; 6] = [
    LengthUnit::Millimetres,
    LengthUnit::Centimetres,
    LengthUnit::Metres,
    LengthUnit::FeetInches,
    LengthUnit::Inches,
    LengthUnit::Feet,
];

impl CreaBimApp {
    /// The CreaBIM menu, first in the menu bar.
    pub(super) fn app_menu(&self) -> Vec<Item> {
        vec![
            Item::new("app.about", "About CreaBIM")
                .icon("info")
                .hint("The version, the licence and the guide"),
            Item::Header(crate::help::version_line()),
            Item::Separator,
            Item::new("app.updates", "Check for updates…")
                .icon("refresh-cw")
                .hint("Opens the releases page in your browser. CreaBIM never checks by itself: it stays offline."),
            Item::new("app.settings", "Settings…")
                .key("Ctrl+,")
                .icon("settings")
                .hint("Your theme and the project's units"),
            Item::new("help:shortcuts", "Keyboard shortcuts")
                .key("Ctrl+/")
                .icon("keyboard"),
            Item::new("app.log", "Open the activity log")
                .icon("file")
                .hint("What CreaBIM did, step by step (drags frame by frame, edits, tools): attach it to a bug report. It stays on this computer."),
            Item::Separator,
            Item::new("app.exit", "Exit").key("Alt+F4").icon("log-out"),
        ]
    }

    /// The status bar's Units list: metric, then imperial, each with how a
    /// length looks in it.
    pub(super) fn unit_items(&self) -> Vec<Item> {
        let now = self.doc.model().units().length;
        let mut items = vec![Item::Header("Metric".into())];
        for (i, u) in UNIT_ORDER.into_iter().enumerate() {
            if u == LengthUnit::FeetInches {
                items.push(Item::Header("Imperial".into()));
            }
            let (name, example) = unit_row(u);
            items.push(
                Item::new(format!("units.set:{i}"), name)
                    .key(example)
                    .checked(u == now)
                    .hint("Applies at once to typing, readouts and dimensions, and is saved with the project"),
            );
        }
        // what each kind of annotation writes in (A5-2)
        items.push(Item::Header("Number formats".into()));
        let units = self.doc.model().units();
        let pack = creabim_core::styles::active_pack(self.doc.model());
        for (i, k) in creabim_core::NumberKind::ALL.into_iter().enumerate() {
            let f = pack.number_formats.get(k);
            let said = if f.is_default() {
                "project".to_string()
            } else {
                f.write(&units, k, if k.is_level() { 3.0 } else { 1.0668 })
            };
            items.push(
                Item::new(
                    format!("units.kind:{i}"),
                    format!("{}  ·  {said}", k.label()),
                )
                .hint("Opens its number format"),
            );
        }
        items.push(Item::Separator);
        items.push(
            Item::new("units", "Number formats…")
                .icon("units")
                .hint("Project units, and how dimensions, level heads, spot levels, tags and schedules write numbers. Saved with the style pack."),
        );
        items
    }

    /// Switch the project to a unit, keeping the rounding when it stays in
    /// the same kind (decimal or fractional).
    pub(super) fn set_unit(&mut self, u: LengthUnit) {
        let before = self.doc.model().units();
        if before.length == u {
            return;
        }
        let units = Units::of(u);
        let tx = creabim_core::units::set_units(self.doc.model(), units);
        let r = self.doc.commit(tx);
        crate::units::set(units);
        self.report(
            r,
            format!(
                "Units: {}. Typed values with a unit (3 m, 10'6\") still work.",
                unit_row(u).0.to_lowercase()
            ),
        );
    }

    /// The application menu's actions. Returns false for any other.
    pub(super) fn app_action(&mut self, ctx: &egui::Context, head: &str, arg: &str) -> bool {
        match head {
            "app.about" => self.open_help("about"),
            "app.updates" => {
                ctx.open_url(egui::OpenUrl::new_tab(RELEASES));
                self.status = format!(
                    "This is CreaBIM {}. The releases page is open in your browser; nothing was downloaded.",
                    crate::help::version_line().trim_start_matches("Version ")
                );
            }
            "app.log" => {
                self.status = match crate::actlog::path() {
                    Some(p) => match crate::files::open_folder(&p) {
                        Ok(()) => format!("The activity log is {}.", p.display()),
                        Err(e) => e,
                    },
                    None => "There is no activity log on this computer.".into(),
                };
            }
            "app.report" => self.report_problem(),
            "app.settings" => self.settings_open = true,
            "app.exit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "units.kind" => {
                if let Some(k) = arg
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| creabim_core::NumberKind::ALL.get(i))
                {
                    self.levels.units_kind = Some(*k);
                    self.levels.units_open = true;
                }
            }
            "units.set" => {
                if let Some(u) = arg.parse::<usize>().ok().and_then(|i| UNIT_ORDER.get(i)) {
                    self.set_unit(*u);
                }
            }
            _ => return false,
        }
        true
    }

    /// Settings: what each person sets for themselves (the theme), and the
    /// project's units.
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        let mut theme = self.shell.theme;
        let mut unit = self.doc.model().units().length;
        let mut keys = false;
        let mut grips = self.shell.settings.grips;
        let mut spacing = self.plan.snap.step_um;
        let mut marks = self.plan.snap.marks;
        let mut paper = self.shell.settings.drawing_paper.clone();
        let mut navigation = self.shell.settings.navigation.clone();
        let mut grip_hover = None;
        let mut edit_bar = self.shell.settings.edit_bar.clone();
        let mut axis_lines = self.shell.settings.axis_lines;
        let mut interaction = self.shell.settings.interaction.clone();
        let mut edge_w = self.shell.settings.edit_edge_width;
        let mut vert_s = self.shell.settings.edit_vertex_size;
        let theme_accent = self.shell.theme.selection();
        let theme_logo = self.shell.theme.logo();
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        // as Blender's Preferences (MU39): its categories down the left, the
        // one picked on the right, scrolling, so it never grows past the
        // window as settings are added
        let tab_id = egui::Id::new("settings.tab");
        let mut tab: usize = ctx.data(|d| d.get_temp(tab_id)).unwrap_or(0);
        const TABS: [(&str, &str); 6] = [
            ("Interface", "layout-panel-left"),
            ("Viewport", "box"),
            ("Navigation", "hand"),
            ("Snapping", "magnet"),
            ("Project", "folder"),
            ("Keymap", "keyboard"),
        ];
        let height = (area.height() - 80.0).clamp(300.0, 560.0);
        crate::app::shell::window("Settings")
            .open(&mut open)
            .default_pos(area.center() - Vec2::new(310.0, height / 2.0))
            .constrain_to(area)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_width(620.0);
                ui.set_height(height);
                let p = crate::theme::Palette::of(ui.ctx());
                ui.horizontal_top(|ui| {
                    // the categories
                    ui.vertical(|ui| {
                        ui.set_width(140.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        // the logo, rising in once as Settings opens (MU39)
                        let t = {
                            let now = ui.input(|i| i.time);
                            let key = egui::Id::new("settings.logo.t0");
                            let t0 = ui.ctx().data_mut(|d| *d.get_temp_mut_or_insert_with(key, || now));
                            let t = (((now - t0) / 0.6) as f32).clamp(0.0, 1.0);
                            if t < 1.0 {
                                ui.ctx().request_repaint();
                            }
                            1.0 - (1.0 - t).powi(3)
                        };
                        let (r, _) = ui.allocate_exact_size(Vec2::new(140.0, 46.0), egui::Sense::hover());
                        let tex = logo_texture(ui.ctx());
                        let s = 36.0;
                        let at = egui::Rect::from_min_size(
                            egui::pos2(r.left() + 8.0, r.top() + 4.0 + (1.0 - t) * 10.0),
                            Vec2::splat(s),
                        );
                        egui::Image::new((tex.id(), Vec2::splat(s)))
                            .tint(theme_logo.gamma_multiply(t))
                            .paint_at(ui, at);
                        ui.painter().text(
                            egui::pos2(at.right() + 8.0, r.center().y),
                            egui::Align2::LEFT_CENTER,
                            "Settings",
                            egui::FontId::proportional(14.0),
                            p.text.gamma_multiply(t),
                        );
                        for (i, (name, icon)) in TABS.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(
                                Vec2::new(140.0, 28.0),
                                egui::Sense::click(),
                            );
                            let on = i == tab;
                            if on {
                                ui.painter().rect_filled(r, 6.0, p.accent.gamma_multiply(0.22));
                            } else if resp.hovered() {
                                ui.painter().rect_filled(r, 6.0, p.raise);
                            }
                            let ink = if on { p.text } else { p.muted };
                            crate::svgicon::paint(
                                ui.painter(),
                                egui::Rect::from_center_size(
                                    egui::pos2(r.left() + 16.0, r.center().y),
                                    Vec2::splat(15.0),
                                ),
                                icon,
                                ink,
                            );
                            ui.painter().text(
                                egui::pos2(r.left() + 32.0, r.center().y),
                                egui::Align2::LEFT_CENTER,
                                *name,
                                egui::FontId::proportional(13.0),
                                ink,
                            );
                            if resp.clicked() {
                                tab = i;
                            }
                        }
                    });
                    let (sep, _) = ui.allocate_exact_size(Vec2::new(9.0, height), egui::Sense::hover());
                    ui.painter().line_segment(
                        [sep.center_top(), sep.center_bottom()],
                        egui::Stroke::new(1.0, p.border),
                    );
                    // the category picked
                    ui.vertical(|ui| {
                        ui.set_width(460.0);
                        egui::ScrollArea::vertical()
                            .id_salt(("settings.page", tab))
                            .max_height(height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(448.0);
                                let yours = |ui: &mut egui::Ui| {
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new("Yours alone: it never changes anyone's drawings, and goes with you, not with the project.")
                                            .weak()
                                            .small(),
                                    );
                                };
                                match tab {
                                    0 => {
                                        crate::props::section(ui, "Look");
                                        crate::props::row(ui, "Theme", |ui| {
                                            egui::ComboBox::from_id_salt("settings.theme").icon(crate::props::combo_icon)
                                                .selected_text(theme.label())
                                                .width(ui.available_width())
                                                .show_ui(ui, |ui| {
                                                    for t in crate::theme::Theme::ALL {
                                                        ui.selectable_value(&mut theme, t, t.label());
                                                    }
                                                });
                                        });
                                        crate::props::row(ui, "Drawing paper", |ui| {
                                            let label = |k: &str| match k {
                                                "light" => "Always light",
                                                "dark" => "Always dark",
                                                _ => "Follow the theme",
                                            };
                                            egui::ComboBox::from_id_salt("settings.paper").icon(crate::props::combo_icon)
                                                .selected_text(label(&paper))
                                                .width(ui.available_width())
                                                .show_ui(ui, |ui| {
                                                    for k in ["theme", "light", "dark"] {
                                                        ui.selectable_value(&mut paper, k.to_string(), label(k));
                                                    }
                                                })
                                                .response
                                                .on_hover_text("Plans, sections and the 3D view on light or dark paper. Prints and PDFs are always on white.");
                                        });
                                        crate::props::section(ui, "Move, Rotate and Scale bar");
                                        for (k, name, note) in [
                                            ("top", "Over the view", "Default. A slim bar at the top of the view"),
                                            ("pointer", "By the pointer", "A small palette where you started"),
                                        ] {
                                            let r = crate::choices::choice_row(ui, edit_bar == k, name, note, |p, r| {
                                                crate::choices::paint_edit_bar(p, r, k == "pointer")
                                            });
                                            if r.clicked() {
                                                edit_bar = k.to_string();
                                            }
                                        }
                                        yours(ui);
                                    }
                                    1 => {
                                        crate::props::section(ui, "Selection and grips colour");
                                        for (k, name, note, ink) in [
                                            ("orange", "Orange", "Default. The selection, grips and ghosts in orange", crate::theme::ORANGE),
                                            ("theme", "Follow the theme", "In the theme's own accent colour", theme_accent),
                                        ] {
                                            let r = crate::choices::choice_row(ui, interaction == k, name, note, |p, r| {
                                                crate::choices::paint_interaction(p, r, ink)
                                            });
                                            if r.clicked() {
                                                interaction = k.to_string();
                                            }
                                        }
                                        crate::props::section(ui, "Grips");
                                        for g in crate::handles::GripStyle::ALL {
                                            let note = match g {
                                                crate::handles::GripStyle::Quiet => "Default. Squares at corners, bars on edges",
                                                crate::handles::GripStyle::Edges => "Almost nothing at rest; edges drag",
                                                crate::handles::GripStyle::OneShape => "The same small circle everywhere",
                                            };
                                            let r = crate::choices::choice_row(ui, grips == g, g.label(), note, |p, r| {
                                                crate::choices::paint_grips(p, r, g)
                                            });
                                            if r.clicked() {
                                                grips = g;
                                            } else if r.hovered() {
                                                grip_hover = Some(g);
                                            }
                                        }
                                        crate::props::section(ui, "Edit mode");
                                        crate::props::row(ui, "Edge width", |ui| {
                                            ui.add(egui::Slider::new(&mut edge_w, 0.5..=3.0).step_by(0.25).suffix(" px"))
                                        });
                                        crate::props::row(ui, "Vertex size", |ui| {
                                            ui.add(egui::Slider::new(&mut vert_s, 2.0..=10.0).step_by(1.0).suffix(" px"))
                                        });
                                        crate::props::section(ui, "3D view");
                                        ui.checkbox(&mut axis_lines, "X and Y axes on the grid")
                                            .on_hover_text("The model's X axis in red and Y in green through its origin. Z shows, in blue, only while a move keeps to it.");
                                        yours(ui);
                                    }
                                    2 => {
                                        crate::props::section(ui, "Mouse");
                                        crate::props::row(ui, "Mouse", |ui| {
                                            let label = |k: &str| match k {
                                                "classic" => "Classic: left drag orbits",
                                                _ => "CreaBIM: middle orbits, left boxes",
                                            };
                                            egui::ComboBox::from_id_salt("settings.navigation").icon(crate::props::combo_icon)
                                                .selected_text(label(&navigation))
                                                .width(ui.available_width())
                                                .show_ui(ui, |ui| {
                                                    for k in ["creabim", "classic"] {
                                                        ui.selectable_value(&mut navigation, k.to_string(), label(k));
                                                    }
                                                });
                                        });
                                        ui.label(
                                            egui::RichText::new("CreaBIM (recommended): left click selects, a left drag on empty space draws a box, a middle drag orbits (Alt + left drag without one), a right drag or Shift + middle pans, a right click opens the menu, the wheel zooms. Classic: a left drag orbits in 3D.")
                                                .weak()
                                                .small(),
                                        );
                                        yours(ui);
                                    }
                                    3 => {
                                        crate::props::section(ui, "Grid snap");
                                        crate::props::row(ui, "Spacing", |ui| {
                                            let steps: &[u32] = if crate::units::get().is_imperial() {
                                                &crate::snaps::STEPS_IMPERIAL
                                            } else {
                                                &crate::snaps::STEPS_METRIC
                                            };
                                            egui::ComboBox::from_id_salt("settings.spacing").icon(crate::props::combo_icon)
                                                .selected_text(crate::units::len(spacing as f64 / 1e6))
                                                .width(ui.available_width())
                                                .show_ui(ui, |ui| {
                                                    for um in steps {
                                                        ui.selectable_value(&mut spacing, *um, crate::units::len(*um as f64 / 1e6));
                                                    }
                                                });
                                        });
                                        ui.checkbox(&mut marks, "Distance marks while dragging")
                                            .on_hover_text("Faint marks every spacing from where you grabbed; a mark holds the drag only when the pointer is near it.");
                                        ui.label(
                                            egui::RichText::new("Drags and points are free. Grid snap (Alt+S, or the status bar) lands them on a grid of this spacing; it starts off each time.")
                                                .weak()
                                                .small(),
                                        );
                                        yours(ui);
                                    }
                                    4 => {
                                        crate::props::section(ui, "This project");
                                        crate::props::row(ui, "Units", |ui| {
                                            egui::ComboBox::from_id_salt("settings.units").icon(crate::props::combo_icon)
                                                .selected_text(unit_row(unit).0)
                                                .width(ui.available_width())
                                                .show_ui(ui, |ui| {
                                                    for u in UNIT_ORDER {
                                                        ui.selectable_value(&mut unit, u, unit_row(u).0);
                                                    }
                                                });
                                        });
                                        ui.label(
                                            egui::RichText::new("Saved with the project: everyone who opens it works in these units.")
                                                .weak()
                                                .small(),
                                        );
                                    }
                                    _ => {
                                        crate::props::section(ui, "Shortcuts");
                                        ui.label(
                                            egui::RichText::new("Every key, by what it does. Ctrl+/ opens the list any time.")
                                                .weak()
                                                .small(),
                                        );
                                        ui.add_space(4.0);
                                        keys = ui.button("Keyboard shortcuts…").clicked();
                                    }
                                }
                            });
                    });
                });
            });
        ctx.data_mut(|d| d.insert_temp(tab_id, tab));
        self.settings_open = open;
        if !open {
            ctx.data_mut(|d| d.remove::<f64>(egui::Id::new("settings.logo.t0")));
        }
        // resting on a grip look shows it at once; leaving puts yours back
        crate::handles::set_grip_style(grip_hover.filter(|_| open).unwrap_or(grips));
        if edge_w != self.shell.settings.edit_edge_width
            || vert_s != self.shell.settings.edit_vertex_size
        {
            crate::handles::set_edit_display(edge_w, vert_s);
            self.shell.settings.edit_edge_width = edge_w;
            self.shell.settings.edit_vertex_size = vert_s;
            self.shell.settings.store();
        }
        if interaction != self.shell.settings.interaction {
            self.shell.settings.interaction = interaction;
            self.shell.settings.store();
        }
        if axis_lines != self.shell.settings.axis_lines {
            self.shell.settings.axis_lines = axis_lines;
            self.shell.settings.store();
            self.geometry_key = None;
        }
        if edit_bar != self.shell.settings.edit_bar {
            crate::modal::set_bar_by_pointer(edit_bar == "pointer");
            self.shell.settings.edit_bar = edit_bar;
            self.shell.settings.store();
        }
        if navigation != self.shell.settings.navigation {
            self.shell.settings.navigation = navigation;
            self.shell.settings.store();
        }
        if paper != self.shell.settings.drawing_paper {
            self.shell.settings.drawing_paper = paper;
            self.shell.settings.store();
        }
        if grips != self.shell.settings.grips {
            self.shell.settings.grips = grips;
            self.shell.settings.store();
            self.status = format!("Grips: {}.", grips.label());
        }
        if spacing != self.plan.snap.step_um || marks != self.plan.snap.marks {
            self.plan.snap.step_um = spacing;
            self.plan.snap.marks = marks;
            self.shell.settings.snap_spacing_um = spacing;
            self.shell.settings.distance_marks = marks;
            self.shell.settings.store();
        }
        if theme != self.shell.theme {
            self.set_theme(ctx, theme);
        }
        if unit != self.doc.model().units().length {
            self.set_unit(unit);
        }
        if keys {
            self.open_help("shortcuts");
        }
    }
}

impl CreaBimApp {
    /// New project with the classic panels: the same choices as the web
    /// window, as a list.
    pub(super) fn new_project_window(&mut self, ctx: &egui::Context) {
        if !self.new_project_classic {
            return;
        }
        let mut open = true;
        let mut pick = None;
        crate::app::shell::window("New project")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                for s in STARTS.iter() {
                    if ui
                        .button(egui::RichText::new(s.title).strong())
                        .on_hover_text(s.line)
                        .clicked()
                    {
                        pick = Some(s.id);
                    }
                }
                crate::props::section(ui, "Samples to learn from");
                for s in SAMPLES.iter() {
                    if ui.button(s.title).on_hover_text(s.line).clicked() {
                        pick = Some(s.id);
                    }
                }
            });
        self.new_project_classic = open && pick.is_none();
        if let Some(id) = pick {
            self.run_action(ctx, id);
        }
    }
}

impl CreaBimApp {
    /// Export > IFC (MU20): how walls, slabs and roofs go out, with what
    /// the other side will see under each; the last choice is ticked.
    pub(super) fn ifc_export_window(&mut self, ctx: &egui::Context) {
        use creabim_ifc::IfcBody;
        if !self.ifc_dialog {
            return;
        }
        let key = |b: IfcBody| match b {
            IfcBody::Modelled => "modelled",
            IfcBody::Joined => "joined",
            IfcBody::Layers => "layers",
        };
        let mut body = match self.shell.settings.ifc_body.as_str() {
            "joined" => IfcBody::Joined,
            "layers" => IfcBody::Layers,
            _ => IfcBody::Modelled,
        };
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        crate::app::shell::window("Export IFC")
            .open(&mut open)
            .default_pos(area.center() - Vec2::new(230.0, 160.0))
            .constrain_to(area)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(egui::RichText::new("How walls, slabs and roofs go out").strong());
                let choices = [
                    (
                        IfcBody::Modelled,
                        "As modelled elements (recommended)",
                        "Each wall and slab is one IfcWall or IfcSlab, its layers carried as its layer set, its body the plain extrusion. Every BIM program reads it, and it comes back into CreaBIM as it went out. The other program settles the joins.",
                    ),
                    (
                        IfcBody::Joined,
                        "Joined shapes, as drawn",
                        "Bodies cut where walls run through slabs, as CreaBIM shows them: for coordination and clash checks. Some programs then read a cut slab as a generic shape, not a slab, and it won't come back into CreaBIM as a slab.",
                    ),
                    (
                        IfcBody::Layers,
                        "Layers as separate elements",
                        "The structure stays the IfcWall or IfcSlab; finishes become IfcCovering (cladding, flooring, roofing), insulation and membranes too, each covering its element. For trades and quantities by layer.",
                    ),
                ];
                for (b, title, what) in choices {
                    ui.add_space(4.0);
                    ui.radio_value(&mut body, b, egui::RichText::new(title).strong());
                    ui.indent(("ifc", key(b)), |ui| {
                        ui.label(egui::RichText::new(what).weak());
                    });
                }
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Doors, windows, openings and everything else go out the same way in all three.")
                        .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    go = ui.button("Export…").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if key(body) != self.shell.settings.ifc_body {
            self.shell.settings.ifc_body = key(body).into();
            self.shell.settings.store();
        }
        self.ifc_dialog = open && !cancel && !go;
        if go {
            let Some(out) = self.export_to(with_extension(&self.path, "ifc")) else {
                return;
            };
            let mut opts =
                creabim_ifc::ExportOptions::for_model(self.doc.model(), &stem(&self.path));
            opts.body = body;
            let r = std::fs::write(&out, creabim_ifc::export_ifc4(self.doc.model(), &opts));
            self.report(r, format!("Exported {out}"));
        }
    }

    /// Project information (Manage › Project information, A7-5): a copy is
    /// edited, and Apply makes it one undo step.
    pub(super) fn project_window(&mut self, ctx: &egui::Context) {
        if !self.project_open {
            self.project_draft = None;
            return;
        }
        let current = self
            .doc
            .model()
            .project_info()
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        let mut draft = self.project_draft.take().unwrap_or_else(|| current.clone());
        let mut open = true;
        let mut apply = false;
        let mut close = false;
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        crate::app::shell::window("Project information")
            .open(&mut open)
            .default_pos(area.center() - Vec2::new(200.0, 220.0))
            .constrain_to(area)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_width(400.0);
                let text = |ui: &mut egui::Ui, label: &str, v: &mut String, hint: &str| {
                    crate::props::row(ui, label, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(v).vertical_align(egui::Align::Center)
                                .hint_text(hint)
                                .desired_width(ui.available_width()),
                        );
                    });
                };
                crate::props::section(ui, "Project");
                text(ui, "Name", &mut draft.name, "Maple Lane House");
                text(ui, "Number", &mut draft.number, "Job number");
                text(ui, "Client", &mut draft.client, "");
                text(ui, "Status", &mut draft.status, "Planning application");
                text(ui, "Issue date", &mut draft.issue_date, "");
                crate::props::section(ui, "Site");
                text(ui, "Site name", &mut draft.site_name, "");
                text(ui, "Address", &mut draft.address, "");
                let degrees = |ui: &mut egui::Ui, label: &str, v: &mut Option<f64>, range: f64| {
                    crate::props::row(ui, label, |ui| {
                        let mut on = v.is_some();
                        if ui.checkbox(&mut on, "").changed() {
                            *v = on.then_some(0.0);
                        }
                        if let Some(d) = v {
                            ui.add(
                                egui::DragValue::new(d)
                                    .range(-range..=range)
                                    .speed(0.0001)
                                    .max_decimals(6)
                                    .suffix("°"),
                            );
                        }
                    });
                };
                degrees(ui, "Latitude", &mut draft.latitude, 90.0);
                degrees(ui, "Longitude", &mut draft.longitude, 180.0);
                crate::props::row(ui, "Elevation", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut draft.elevation)
                            .speed(0.01)
                            .suffix(" m"),
                    )
                    .on_hover_text("The project's zero above sea level");
                });
                crate::props::row(ui, "True north", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut draft.true_north)
                            .range(-180.0..=180.0)
                            .speed(0.5)
                            .suffix("°"),
                    )
                    .on_hover_text("How far true north turns from the drawings' up, anticlockwise");
                });
                crate::props::section(ui, "Building");
                text(ui, "Name", &mut draft.building_name, "");
                text(ui, "Use", &mut draft.building_use, "Dwelling");
                ui.label(
                    egui::RichText::new(
                        "These fill the title blocks of every sheet (a value a sheet sets itself still wins) and name the project, site and building in IFC exports.",
                    )
                    .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    apply = ui
                        .add_enabled(draft != current, egui::Button::new("Apply"))
                        .clicked();
                    close = ui.button("Close").clicked();
                });
            });
        if apply {
            let tx = creabim_core::set_project(self.doc.model(), draft.clone());
            let r = self.doc.commit(tx);
            self.report(
                r,
                "Project information applied: the title blocks show it now.".to_string(),
            );
        }
        self.project_open = open && !close;
        self.project_draft = self.project_open.then_some(draft);
    }
}

/// The logo's mark as a white texture, to tint with a theme's colour.
pub(crate) fn logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    static TEX: std::sync::OnceLock<egui::TextureHandle> = std::sync::OnceLock::new();
    TEX.get_or_init(|| {
        let img =
            image::load_from_memory(include_bytes!("../assets/logo/png/creabim-mark-128.png"))
                .map(|i| i.to_rgba8())
                .unwrap_or_default();
        let (w, h) = img.dimensions();
        let px: Vec<egui::Color32> = img
            .pixels()
            .map(|p| egui::Color32::from_rgba_unmultiplied(255, 255, 255, p[3]))
            .collect();
        ctx.load_texture(
            "creabim-logo",
            egui::ColorImage::new([w as usize, h as usize], px),
            egui::TextureOptions::LINEAR,
        )
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_script_inserts_selects_and_names_uniquely() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.run_action(&ctx, "new.script");
        let scripts: Vec<String> = app
            .doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::ScriptObject(_)))
            .map(|e| e.name.clone())
            .collect();
        assert_eq!(scripts, vec!["Script".to_string()]);
        assert!(app.selection.contains(
            app.doc
                .model()
                .iter()
                .find(|e| e.name == "Script")
                .unwrap()
                .id
        ));
        app.run_action(&ctx, "new.script");
        let names: Vec<String> = app
            .doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::ScriptObject(_)))
            .map(|e| e.name.clone())
            .collect();
        assert!(names.contains(&"Script 2".to_string()), "{names:?}");
    }

    #[test]
    fn new_3d_view_inserts_activates_and_names_uniquely() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        // the sample ships one 3D view; the new one takes the next name
        app.run_action(&ctx, "new.3dview");
        let views: Vec<String> = app
            .doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::View(_)))
            .map(|e| e.name.clone())
            .collect();
        assert!(views.contains(&"3D View".to_string()), "{views:?}");
        assert!(views.contains(&"3D View South-East".to_string()));
        let active = app.active_view.and_then(|id| app.doc.model().get(id));
        assert_eq!(active.map(|e| e.name.as_str()), Some("3D View"));
        app.run_action(&ctx, "new.3dview");
        let names: Vec<String> = app
            .doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::View(_)))
            .map(|e| e.name.clone())
            .collect();
        assert!(names.contains(&"3D View 2".to_string()), "{names:?}");
    }

    #[test]
    fn after_an_export_the_status_bar_offers_its_folder() {
        let mut app = test_app();
        let dir = std::env::temp_dir().join(format!("creabim-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        app.path = dir.join("house.creabim").display().to_string();
        app.export("export.ifcx");
        assert!(app.status.starts_with("Exported "), "{}", app.status);
        assert!(dir.join("house.ifcx").exists());
        let has = |app: &CreaBimApp| {
            app.status_state()["controls"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == "export.folder")
        };
        assert!(has(&app));
        assert_eq!(
            app.last_export_dir
                .as_deref()
                .map(|d| d.canonicalize().unwrap()),
            Some(dir.canonicalize().unwrap())
        );
        // once the bar says something else, the button goes
        app.status = "Moved.".into();
        assert!(!has(&app));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_placed_footing_offers_the_foundation_plan_and_makes_one_if_need_be() {
        let mut app = test_app();
        assert!(crate::memberstool::foundation_plan(app.doc.model()).is_none());
        let placed = |app: &mut CreaBimApp| {
            app.plan.footing_placed = true;
            app.status = "Strip footing placed under GF East. Ctrl+Z to undo.".into();
        };
        let offered = |app: &CreaBimApp| {
            app.status_state()["controls"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == "footing.foundation")
                .map(|c| c["label"].as_str().unwrap().to_string())
        };
        // without one, it is made: 600 below the lowest level, the floor
        // above as its underlay, and opened
        placed(&mut app);
        assert_eq!(offered(&app).as_deref(), Some("Make a Foundation plan"));
        app.show_foundation_plan();
        let made = crate::memberstool::foundation_plan(app.doc.model()).unwrap();
        assert_eq!(app.active_view, Some(made));
        let ElementKind::View(v) = &app.doc.model().get(made).unwrap().kind else {
            panic!()
        };
        assert_eq!(v.underlay, creabim_core::Underlay::Above);
        // once there is one, it is shown
        app.active_view = None;
        placed(&mut app);
        assert_eq!(offered(&app).as_deref(), Some("Show in Foundation plan"));
        app.show_foundation_plan();
        assert_eq!(app.active_view, Some(made));
        // and the button goes once the bar says something else
        placed(&mut app);
        app.status = "Moved.".into();
        assert!(offered(&app).is_none());
    }

    #[test]
    fn frame_all_reframes_only_the_pane_it_is_asked_in() {
        let mut app = test_app();
        let (a, b) = (ElementId::new(), ElementId::new());
        app.active_view = Some(a);
        app.rect_2d = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 300.0),
        ));
        app.plan.cameras.insert(a, (glam::DVec2::ZERO, 3.0));
        app.plan.cameras.insert(b, (glam::DVec2::ZERO, 3.0));
        app.camera.distance = 1234.0;
        app.frame_pane(true);
        assert!(!app.plan.cameras.contains_key(&a));
        assert!(app.plan.cameras.contains_key(&b));
        assert_eq!(app.camera.distance, 1234.0);
        app.plan.cameras.insert(a, (glam::DVec2::ZERO, 3.0));
        app.frame_pane(false);
        assert_ne!(app.camera.distance, 1234.0);
        assert!(app.plan.cameras.contains_key(&a));
    }

    #[test]
    fn the_units_switch_changes_the_project_units_in_one_undo() {
        let mut app = test_app();
        assert_eq!(app.doc.model().units().length, LengthUnit::Millimetres);
        let items = app.unit_items();
        let ids: Vec<String> = items
            .iter()
            .filter_map(|i| match i {
                Item::Action { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids.len(),
            12,
            "six units, five number formats and the window: {ids:?}"
        );
        let ctx = egui::Context::default();
        app.run_action(&ctx, "units.set:3");
        assert_eq!(app.doc.model().units().length, LengthUnit::FeetInches);
        assert_eq!(crate::units::get().length, LengthUnit::FeetInches);
        app.run_command(&ctx, crate::commands::Command::Undo, None);
        assert_eq!(app.doc.model().units().length, LengthUnit::Millimetres);
    }

    #[test]
    fn the_application_menu_has_about_updates_settings_and_exit() {
        let app = test_app();
        let ids: Vec<String> = app
            .app_menu()
            .into_iter()
            .filter_map(|i| match i {
                Item::Action { id, .. } => Some(id),
                _ => None,
            })
            .collect();
        for want in ["app.about", "app.updates", "app.settings", "app.exit"] {
            assert!(ids.iter().any(|i| i == want), "{want} in {ids:?}");
        }
    }

    #[test]
    fn samples_open_as_new_unsaved_projects() {
        let mut app = test_app();
        app.path = "/work/my house.creabim".into();
        let ctx = egui::Context::default();
        app.run_action(&ctx, "new.garden");
        assert_eq!(app.path, "House in its garden.creabim");
        app.run_action(&ctx, "new.empty");
        assert_eq!(app.path, "Untitled.creabim");
        assert_eq!(app.doc.model().levels().count(), 2);
        // one New project… in the File menu, the samples under Help
        let file: Vec<String> = app
            .menu_items("menu:File")
            .into_iter()
            .filter_map(|i| match i {
                Item::Action { id, .. } if id.starts_with("new.") => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(file, ["new.project", "new.sheet"]);
        let help = app.help_menu();
        assert!(help.iter().any(|i| matches!(i, Item::Sub { label, items, .. } if label == "Samples to learn from" && items.len() == 6)));
    }

    #[test]
    fn a_file_named_on_the_command_line_is_opened() {
        let args = ["--verbose", "house.creabim"].map(String::from);
        assert_eq!(
            launch_file(args.into_iter()).as_deref(),
            Some("house.creabim")
        );
        assert_eq!(launch_file(std::iter::empty()), None);
        let dir = std::env::temp_dir().join("creabim-launch-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("small.creabim.json");
        let model = sample_house();
        std::fs::write(&path, model.to_json()).unwrap();
        let mut app = CreaBimApp::blank(None);
        app.open_path(path.to_str().unwrap());
        assert_eq!(app.doc.model().len(), model.len());
    }
}
