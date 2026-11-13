//! Opening, saving, importing and exporting, and watching linked files so
//! a model saved in Blender shows up here without asking.

use creabim_core::Model;
use creabim_exchange::Incoming;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

/// The kinds of file that come in as linked objects.
pub const LINKABLE: &[&str] = &["blend", "fcstd"];

fn ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Is this a project file (opened in place of the current project)?
pub fn is_project(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".creabim") || ext(path) == "json"
}

pub fn is_linkable(path: &str) -> bool {
    LINKABLE.contains(&ext(path).as_str())
}

/// The objects in a linked file.
pub fn read_incoming(path: &str) -> Result<Vec<Incoming>, String> {
    match ext(path).as_str() {
        "blend" => creabim_exchange::blend::read_file(Path::new(path)).map_err(|e| e.to_string()),
        "fcstd" => creabim_exchange::freecad::read_file(Path::new(path)).map_err(|e| e.to_string()),
        other => Err(format!("CreaBIM can't read .{other} files yet.")),
    }
}

/// The linked files the model mirrors.
pub fn linked_files(model: &Model) -> Vec<String> {
    let mut files: Vec<String> = model
        .iter()
        .filter_map(|e| e.source.as_ref().map(|s| s.file.clone()))
        .filter(|f| is_linkable(f))
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Watches linked files for saves.
#[derive(Default)]
pub struct Watch {
    seen: HashMap<String, Option<SystemTime>>,
    last: Option<Instant>,
}

impl Watch {
    /// Remember a file as it is now, e.g. right after reading it.
    pub fn mark(&mut self, file: &str) {
        self.seen.insert(file.to_string(), modified(file));
    }

    /// Files saved since they were last seen. Looks at most once a second.
    pub fn changed(&mut self, model: &Model) -> Vec<String> {
        if self
            .last
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return vec![];
        }
        self.last = Some(Instant::now());
        let mut out = Vec::new();
        for f in linked_files(model) {
            let now = modified(&f);
            match self.seen.get(&f) {
                None => {
                    self.seen.insert(f, now);
                }
                // a save in progress is left to finish before reading
                Some(before)
                    if *before != now
                        && now.is_some_and(|t| {
                            t.elapsed().unwrap_or_default() > Duration::from_millis(300)
                        }) =>
                {
                    self.seen.insert(f.clone(), now);
                    out.push(f);
                }
                _ => {}
            }
        }
        out
    }
}

fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

/// Files opened or saved lately, newest first, kept in the user's settings
/// folder between sessions.
#[derive(Debug, Default)]
pub struct Recent {
    pub files: Vec<String>,
    path: Option<std::path::PathBuf>,
}

const RECENT_MAX: usize = 10;

impl Recent {
    /// Read the list from `<settings>/CreaBIM/recent.json`, where settings
    /// is %APPDATA% on Windows and $XDG_CONFIG_HOME or ~/.config elsewhere.
    pub fn load() -> Self {
        let path = settings_dir().map(|d| d.join("recent.json"));
        let files = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_default();
        Self { files, path }
    }

    /// Put `file` at the top of the list and save it.
    pub fn add(&mut self, file: &str) {
        let full = std::fs::canonicalize(file)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| file.to_string());
        self.files.retain(|f| *f != full);
        self.files.insert(0, full);
        self.files.truncate(RECENT_MAX);
        self.store();
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.store();
    }

    fn store(&self) {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(t) = serde_json::to_string_pretty(&self.files) {
                let _ = std::fs::write(p, t);
            }
        }
    }
}

/// Interface choices kept between sessions in `<settings>/CreaBIM/settings.json`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    /// Width of the project browser and of the properties panel, in points.
    pub left_width: f32,
    pub right_width: f32,
    /// The browser folded to a rail of its groups (MU27).
    #[serde(default)]
    pub left_folded: bool,
    /// Properties folded to its tab column (MU27).
    #[serde(default)]
    pub right_folded: bool,
    /// One view at a time instead of the 3D view and a drawing side by
    /// side, and which of the two it shows.
    pub single_view: bool,
    pub single_shows_3d: bool,
    /// The ground grid in the 3D view, and its spacing in metres.
    pub ground_grid: bool,
    pub ground_grid_spacing: f64,
    /// The drawings' paper and the 3D background (A4-3): "theme" follows
    /// the theme (dark paper in a dark theme), "light" or "dark" always.
    /// Prints and PDFs are always on white.
    pub drawing_paper: String,
    /// The mouse (MU19): "creabim" (left selects and boxes, middle
    /// orbits, right or Shift + middle pans) or "classic" (left orbits).
    pub navigation: String,
    /// How the last IFC export wrote walls, slabs and roofs: "modelled"
    /// (recommended), "joined" or "layers" (MU20).
    pub ifc_body: String,
    /// Temporary dimensions muted in drawings (MU21).
    pub temp_dims_off: bool,
    /// The kinds of element the 3D view leaves out (grids, annotation,
    /// guides…), from its Visibility settings.
    pub hidden_3d: Vec<creabim_core::Category>,
    /// The 3D view in X-ray: walls, slabs, roofs and masses see-through, so
    /// what is inside them shows.
    pub xray: bool,
    /// The 3D view shows what is hidden inside walls, slabs and roofs in its
    /// own colours, faintly through what hides it.
    pub hidden_colour_3d: bool,
    /// Guides hidden everywhere (they are kept, and come back when shown).
    pub guides_hidden: bool,
    /// How the 3D view draws: shaded, with edges, hidden line or wireframe.
    pub render_3d: crate::render3d::RenderMode,
    /// Width of the Help panel, in points.
    pub help_width: f32,
    /// Which sections of Properties are open, by key, where it differs
    /// from how each opens at first.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub props_open: std::collections::BTreeMap<String, bool>,
    /// The tab the tools row shows: Model, Annotate, View or Manage.
    pub tools_tab: usize,
    /// The workspace last picked, by name, and the areas as they were left.
    pub workspace: String,
    pub areas: Option<crate::areas::Node>,
    /// Layouts saved with *Save current layout*.
    pub workspaces: Vec<crate::areas::Workspace>,
    /// How grips look (brief 02, 2.13): a personal choice.
    pub grips: crate::handles::GripStyle,
    /// Distance marks while dragging (the Grid snap menu).
    pub distance_marks: bool,
    /// The grid snap's and the distance marks' spacing, micrometres. Grid
    /// snap itself always starts off.
    pub snap_spacing_um: u32,
    /// Where Move, Rotate, Scale and the like show their bar: a slim pill
    /// at the top of the view, or a small palette by the pointer (MU38).
    pub edit_bar: String,
    /// The model's X and Y axes drawn red and green on the 3D ground
    /// grid, as Blender's (MU39).
    pub axis_lines: bool,
    /// The light millimetre grid behind plans, sections and details; off,
    /// plain paper (MU40).
    pub plan_grid: bool,
    /// The colour of what is worked on (selection, grips, ghosts): "orange",
    /// the default, or "theme" for the theme's accent (MU39).
    pub interaction: String,
    /// Edit mode's wires and corner dots, in points.
    pub edit_edge_width: f32,
    pub edit_vertex_size: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // the approved look (brief 02); anyone's saved theme stays theirs
            theme: "sage".into(),
            left_width: 264.0,
            right_width: 290.0,
            left_folded: false,
            right_folded: false,
            single_view: false,
            single_shows_3d: true,
            ground_grid: true,
            ground_grid_spacing: 1.0,
            drawing_paper: "theme".into(),
            navigation: "creabim".into(),
            ifc_body: "modelled".into(),
            temp_dims_off: false,
            hidden_3d: vec![],
            xray: false,
            hidden_colour_3d: false,
            guides_hidden: false,
            render_3d: Default::default(),
            help_width: 400.0,
            props_open: Default::default(),
            tools_tab: 0,
            workspace: "Drawing".into(),
            areas: None,
            workspaces: vec![],
            grips: Default::default(),
            distance_marks: false,
            snap_spacing_um: 50_000,
            edit_bar: "top".into(),
            axis_lines: true,
            plan_grid: true,
            interaction: "orange".into(),
            edit_edge_width: 1.0,
            edit_vertex_size: 4.0,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        settings_dir()
            .and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn store(&self) {
        // tests change settings without touching the user's own
        if cfg!(test) {
            return;
        }
        if let Some(dir) = settings_dir() {
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(t) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(dir.join("settings.json"), t);
            }
        }
    }
}

pub fn settings_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("CreaBIM"))
}

/// Ask for a file to open with the system's own file picker.
pub fn pick_open() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Open or link a file")
        .add_filter(
            "Everything CreaBIM reads",
            &[
                "creabim", "json", "ifc", "ifcx", "blend", "FCStd", "fcstd", "FCMat", "fcmat",
                "gltf", "glb",
            ],
        )
        .add_filter("CreaBIM project", &["creabim"])
        .add_filter("IFC", &["ifc", "ifcx"])
        .add_filter("Blender and FreeCAD (linked)", &["blend", "FCStd", "fcstd"])
        .add_filter(
            "Materials (FreeCAD cards, glTF)",
            &["FCMat", "fcmat", "gltf", "glb"],
        )
        .pick_file()
        .map(|p| p.display().to_string())
}

/// Show `dir` in the system's file browser.
pub fn open_folder(dir: &Path) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't open {}: {e}", dir.display()))
}

/// Open `file` with the system's default handler (the editor, for a
/// linked script's `.py`).
pub fn open_file(file: &Path) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(file)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't open {}: {e}", file.display()))
}

/// Ask where to save, starting from `current`.
pub fn pick_save(current: &str, title: &str, ext: &str) -> Option<String> {
    // a project not saved yet starts in the folder CreaBIM was started in
    let abs = std::path::absolute(current).unwrap_or_else(|_| current.into());
    let p = abs.as_path();
    let mut d = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(ext.to_uppercase(), &[ext]);
    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        d = d.set_file_name(name);
    }
    if let Some(dir) = p.parent().filter(|d| d.is_dir()) {
        d = d.set_directory(dir);
    }
    d.save_file().map(|p| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_from_before_the_layout_options_still_read() {
        let s: Settings = serde_json::from_str(r#"{"theme": "paper", "left_width": 300.0}"#)
            .expect("old settings read");
        assert_eq!(s.theme, "paper");
        assert!(!s.single_view);
        assert!(s.ground_grid);
        assert_eq!(s.ground_grid_spacing, 1.0);
        assert!(s.hidden_3d.is_empty() && !s.xray && !s.guides_hidden && !s.hidden_colour_3d);
        assert_eq!(s.render_3d, crate::render3d::RenderMode::Shaded);
        assert!(s.props_open.is_empty());
    }

    #[test]
    fn folded_properties_sections_round_trip_and_none_writes_nothing() {
        let mut s = Settings::default();
        s.props_open.insert("visibility.view".into(), false);
        s.props_open.insert("visibility.3d".into(), true);
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.props_open, s.props_open);
        let plain = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!plain.contains("props_open"));
    }

    #[test]
    fn the_3d_views_visibility_round_trips_through_the_settings() {
        let s = Settings {
            hidden_3d: vec![
                creabim_core::Category::Grids,
                creabim_core::Category::Guides,
            ],
            xray: true,
            hidden_colour_3d: true,
            guides_hidden: true,
            ground_grid_spacing: 1.2,
            render_3d: crate::render3d::RenderMode::Wireframe,
            ..Default::default()
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn recent_files_keep_the_newest_first_without_repeats() {
        let mut r = Recent::default();
        for f in ["a.creabim", "b.creabim", "a.creabim"] {
            r.add(f);
        }
        assert_eq!(r.files.len(), 2);
        assert!(r.files[0].ends_with("a.creabim"));
        for i in 0..20 {
            r.add(&format!("{i}.creabim"));
        }
        assert_eq!(r.files.len(), RECENT_MAX);
    }
}
