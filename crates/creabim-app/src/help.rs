//! The built-in guide. Its pages are Markdown files in `help/`, compiled
//! into the app, so they are edited like any other text in the repository.
//! The Help panel (a web pane, see `helpui.rs`) draws them; the classic
//! panels draw them with egui.
//!
//! Pages link to each other by id (`[Walls](walls)`, `[joins](walls#joins)`)
//! and show pictures from `help/img/`. A part about a feature that has not
//! arrived yet sits between `<!-- upcoming: … -->` and `<!-- /upcoming -->`
//! and is shown as "Coming soon"; search the pages for `upcoming:` when it
//! lands.
//!
//! The keyboard shortcuts page is made from the menus and from [`KEYS`],
//! and a test checks [`KEYS`] against the keys the source code reads.

use crate::planview::Tool;
use crate::ui_model::Item;
use creabim_core::{ElementKind, OpeningKind, ViewKind};

/// A page: (section, id, its text from `help/<id>.md`).
macro_rules! page {
    ($section:expr, $id:literal) => {
        ($section, $id, include_str!(concat!("../help/", $id, ".md")))
    };
}

/// A picture: (the path pages show it by, `help/img/<name>.jpg`).
macro_rules! img {
    ($name:literal) => {
        (
            concat!("img/", $name, ".jpg"),
            include_bytes!(concat!("../help/img/", $name, ".jpg")),
        )
    };
}

/// The pages in their order, by section: (section, id, text).
pub const PAGES: &[(&str, &str, &str)] = &[
    page!("Getting started", "welcome"),
    page!("Getting started", "starter-house"),
    page!("Getting started", "start"),
    page!("Getting started", "start-project"),
    page!("Getting started", "start-levels"),
    page!("Getting started", "start-walls"),
    page!("Getting started", "start-openings"),
    page!("Getting started", "start-floors-roofs"),
    page!("Getting started", "start-views"),
    page!("Getting started", "start-sheets"),
    page!("Getting started", "start-export"),
    page!("Getting started", "interface"),
    page!("Getting started", "navigate"),
    page!("Selecting and editing", "select"),
    page!("Selecting and editing", "move"),
    page!("Selecting and editing", "align"),
    page!("Selecting and editing", "handles"),
    page!("Selecting and editing", "edit-mode"),
    page!("Selecting and editing", "divide"),
    page!("Selecting and editing", "edit"),
    page!("Building the model", "levels"),
    page!("Building the model", "walls"),
    page!("Building the model", "doors-windows"),
    page!("Building the model", "holes"),
    page!("Building the model", "slabs"),
    page!("Building the model", "roofs"),
    page!("Building the model", "columns-beams"),
    page!("Building the model", "stairs-railings"),
    page!("Building the model", "footings"),
    page!("Building the model", "parts"),
    page!("Building the model", "scripts"),
    page!("Building the model", "profiles"),
    page!("Building the model", "site"),
    page!("Building the model", "curtain-walls"),
    page!("Building the model", "types"),
    page!("Building the model", "assemblies"),
    page!("Building the model", "materials"),
    page!("Building the model", "masses"),
    page!("Building the model", "sketches"),
    page!("Building the model", "grids"),
    page!("Building the model", "furniture"),
    page!("Building the model", "fixtures"),
    page!("Building the model", "rooms"),
    page!("Building the model", "ceilings"),
    page!("Building the model", "joins"),
    page!("Views and drawings", "views"),
    page!("Views and drawings", "sections"),
    page!("Views and drawings", "view-range"),
    page!("Views and drawings", "graphic-styles"),
    page!("Views and drawings", "visibility"),
    page!("Views and drawings", "crop-boxes"),
    page!("Views and drawings", "trace-reference"),
    page!("Views and drawings", "details"),
    page!("Views and drawings", "schedules"),
    page!("Views and drawings", "take-off"),
    page!("Views and drawings", "sheets"),
    page!("Views and drawings", "masters"),
    page!("Annotation", "dimensions"),
    page!("Annotation", "tags"),
    page!("Annotation", "marks"),
    page!("Annotation", "spot-levels"),
    page!("Annotation", "notes"),
    page!("Annotation", "styles"),
    page!("Drawing aids", "snaps"),
    page!("Drawing aids", "guides"),
    page!("Drawing aids", "units"),
    page!("Drawing aids", "project-info"),
    page!("Files and exchange", "files"),
    page!("Files and exchange", "ifc"),
    page!("Files and exchange", "blender"),
    page!("Files and exchange", "freecad"),
    page!("Files and exchange", "gltf"),
    page!("Files and exchange", "dxf-svg"),
    page!("Reference", "shortcuts"),
    page!("Reference", "glossary"),
    page!("Reference", "about"),
];

/// The list of every page, drawn from [`PAGES`] rather than written.
pub const CONTENTS: &str = "contents";

/// The pictures the pages show, by the path they are written with.
pub const IMAGES: &[(&str, &[u8])] = &[
    // the logo: the weaver and its nest (assets/logo)
    (
        "img/creabim-lockup.png",
        include_bytes!("../assets/logo/png/creabim-lockup-640x200.png"),
    ),
    img!("interface"),
    img!("palette"),
    img!("levels"),
    img!("wall-options"),
    img!("location-line"),
    img!("door-flip"),
    img!("slab-arc"),
    img!("roofs"),
    img!("section"),
    img!("sheet"),
    img!("move"),
    img!("stretch"),
    img!("box-select"),
    img!("types"),
    img!("curtain-wall"),
    img!("slab-holes"),
    img!("assemblies"),
    img!("materials"),
    img!("dimensions"),
    img!("guides"),
    img!("crop-box"),
    img!("grids-3d"),
    img!("xray-2d"),
    img!("tab-pick"),
    img!("catalogue"),
    img!("object-against-wall"),
    img!("details"),
    img!("profile-view"),
    img!("masters"),
    img!("divide-bar"),
    img!("modifiers"),
    img!("edit-mode-mass"),
];

/// A page's text by id.
pub fn text(id: &str) -> Option<&'static str> {
    PAGES.iter().find(|p| p.1 == id).map(|p| p.2)
}

/// A page's title: its first heading.
pub fn title(md: &str) -> &str {
    md.lines()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or("")
        .trim()
}

/// Is there a page `id`, and on it a heading for `anchor` (in `id#anchor`)?
pub fn exists(link: &str) -> bool {
    let (id, anchor) = link.split_once('#').unwrap_or((link, ""));
    if id == CONTENTS {
        return anchor.is_empty();
    }
    let Some(md) = text(id) else { return false };
    anchor.is_empty() || headings(md).iter().any(|h| slug(h) == anchor)
}

/// The id a heading is linked by: lower case, words joined by dashes. The
/// Help panel's script makes the same.
pub fn slug(heading: &str) -> String {
    let mut out = String::new();
    for c in heading.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The text of a page's headings, from `##` down.
pub fn headings(md: &str) -> Vec<&str> {
    md.lines()
        .filter_map(|l| {
            let t = l.trim_start_matches('#');
            (l.starts_with("##") && t.starts_with(' ')).then(|| t.trim())
        })
        .collect()
}

/// Where a page's links go, and the pictures it shows: (links, images).
pub fn targets(md: &str) -> (Vec<&str>, Vec<&str>) {
    let (mut links, mut images) = (vec![], vec![]);
    let mut rest = md;
    while let Some(i) = rest.find("](") {
        let image = rest[..i]
            .rfind("![")
            .is_some_and(|b| !rest[b..i].contains(']'));
        let after = &rest[i + 2..];
        let Some(end) = after.find(')') else { break };
        let target = &after[..end];
        if image {
            images.push(target);
        } else {
            links.push(target);
        }
        rest = &after[end..];
    }
    (links, images)
}

/// The page about a tool, as a link (`page` or `page#heading`).
pub fn tool_page(t: Tool) -> &'static str {
    match t {
        Tool::Select => "select",
        Tool::Wall => "walls",
        Tool::Door | Tool::Window => "doors-windows",
        Tool::Opening => "holes#wall-openings",
        Tool::Slab => "slabs",
        Tool::Shaft => "holes#shafts",
        Tool::Mass => "masses",
        Tool::Section | Tool::Elevation => "sections",
        Tool::Dimension => "dimensions",
        Tool::Angle => "dimensions#angles",
        Tool::Radius | Tool::Diameter => "dimensions#radius-and-diameter",
        Tool::ArcLength => "dimensions#arc-length",
        Tool::Tag => "tags",
        Tool::Note => "notes",
        Tool::SpotLevel => "spot-levels",
        Tool::Sketch => "sketches",
        Tool::Guide => "guides",
        Tool::Grid => "grids",
        Tool::Column | Tool::Beam => "columns-beams",
        Tool::Roof => "roofs",
        Tool::Stair | Tool::Railing => "stairs-railings",
        Tool::Footing => "footings",
        Tool::Level => "levels",
        Tool::Align => "align",
        Tool::Object => "furniture",
        Tool::Extrusion | Tool::Sweep => "parts",
        Tool::Void => "parts#voids",
        Tool::Terrain | Tool::Paving | Tool::Planting => "site",
        Tool::Room | Tool::RoomSeparator => "rooms",
        Tool::Ceiling => "ceilings",
        Tool::Callout => "details#callouts",
        Tool::Detail => "details#detail-components",
    }
}

/// The page about a kind of element.
pub fn element_page(kind: &ElementKind) -> &'static str {
    match kind {
        ElementKind::Wall(w) if w.curtain.is_some() => "curtain-walls",
        ElementKind::Wall(_) => "walls",
        ElementKind::Opening(o) if o.kind == OpeningKind::Hole => "holes#wall-openings",
        ElementKind::Opening(_) => "doors-windows",
        ElementKind::Slab(_) => "slabs",
        ElementKind::Roof(_) => "roofs",
        ElementKind::Column(_) | ElementKind::Beam(_) => "columns-beams",
        ElementKind::Stair(_) | ElementKind::Railing(_) => "stairs-railings",
        ElementKind::Footing(_) => "footings",
        ElementKind::Part(p) if p.void => "parts#voids",
        ElementKind::Part(_) => "parts",
        ElementKind::Site(_) => "site",
        ElementKind::Level(_) => "levels",
        ElementKind::Grid(_) | ElementKind::GridGroup(_) => "grids",
        ElementKind::FreeMesh(_) | ElementKind::Cut(_) => "masses",
        ElementKind::Sketch(_) => "sketches",
        ElementKind::Guide(_) => "guides",
        ElementKind::Material(_) => "materials",
        ElementKind::ScriptObject(_) => "scripts",
        ElementKind::Assembly(_) => "assemblies",
        ElementKind::ElementType(_) => "types",
        ElementKind::Shaft(_) => "holes#shafts",
        ElementKind::CropBox(_) => "crop-boxes",
        ElementKind::Object(_) => "furniture",
        ElementKind::Room(_) | ElementKind::RoomSeparator(_) => "rooms",
        ElementKind::Ceiling(_) => "ceilings",
        ElementKind::Join(_) => "joins",
        ElementKind::Detail(_) => "details#detail-components",
        ElementKind::Dimension(_) => "dimensions",
        ElementKind::Tag(_) => "tags",
        ElementKind::Note(_) => "notes",
        ElementKind::TextStyle(_) => "styles",
        ElementKind::Sheet(_) | ElementKind::Viewport(_) | ElementKind::SheetTemplate(_) => {
            "sheets"
        }
        ElementKind::View(v) if v.callout.is_some() => "details#callouts",
        ElementKind::View(v) => match v.kind {
            ViewKind::Section { .. } => "sections",
            ViewKind::Schedule { .. } => "schedules",
            _ => "views",
        },
        ElementKind::Units(_) => "units",
        ElementKind::Project(_) => "project-info",
        ElementKind::Reference(_) => "trace-reference",
        ElementKind::PaperMark(_) => "sheets",
        ElementKind::MarkRules(_) => "marks",
        ElementKind::StylePack(_) => "graphic-styles",
    }
}

/// A few lines from a page (`page` or `page#heading`) for a quick-help
/// popover: its first paragraph, cut to two sentences, or the first steps
/// of its first list; both when a short paragraph leads into a list. Links
/// become their words; `code` and **bold** are kept for the popover to
/// draw.
pub fn quick(link: &str) -> Vec<String> {
    let (id, anchor) = link.split_once('#').unwrap_or((link, ""));
    let Some(md) = text(id) else { return vec![] };
    let mut lines = md.lines();
    if !anchor.is_empty() {
        // to the heading
        for l in lines.by_ref() {
            let t = l.trim_start_matches('#');
            if l.starts_with("##") && slug(t) == anchor {
                break;
            }
        }
    }
    let mut out: Vec<String> = vec![];
    let mut para = String::new();
    let mut steps = 0;
    let item = |l: &str| -> Option<String> {
        let t = l.strip_prefix("- ").or_else(|| l.strip_prefix("* "));
        let t = t.or_else(|| {
            let (n, rest) = l.split_once(". ")?;
            n.chars().all(|c| c.is_ascii_digit()).then_some(rest)
        })?;
        Some(t.to_string())
    };
    for l in lines {
        let t = l.trim_end();
        if t.is_empty() || t.starts_with('#') {
            if !para.is_empty() {
                // a short paragraph leads into what follows
                let short = para.chars().count() < 110;
                out.push(sentences(&std::mem::take(&mut para), 2));
                if !short {
                    break;
                }
            } else if steps > 0 {
                break;
            }
            // a heading after the lines taken ends them; the page's title
            // and a heading the page opens with don't
            if t.starts_with('#') && !out.is_empty() {
                break;
            }
            continue;
        }
        if t.starts_with("![") || t.starts_with("<!--") || t.starts_with('|') || t.starts_with('>')
        {
            continue;
        }
        if let Some(i) = item(t) {
            if !para.is_empty() {
                out.push(sentences(&std::mem::take(&mut para), 2));
            }
            steps += 1;
            if steps <= 3 {
                out.push(unlink(&i));
            }
            continue;
        }
        if l.starts_with(' ') {
            // a list item's next line, or one under it
            if steps > 0 && steps <= 3 && !l.trim_start().starts_with(['-', '*']) {
                if let Some(last) = out.last_mut() {
                    last.push(' ');
                    last.push_str(&unlink(l.trim()));
                }
            }
            continue;
        }
        if steps > 0 {
            break;
        }
        if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(&unlink(t));
    }
    if !para.is_empty() {
        out.push(sentences(&para, 2));
    }
    out.truncate(4);
    out
}

/// The first `n` sentences of `s`.
fn sentences(s: &str, n: usize) -> String {
    let mut ends = 0;
    for (i, c) in s.char_indices() {
        if matches!(c, '.' | '!' | '?') && s[i + 1..].starts_with(' ') {
            ends += 1;
            if ends == n {
                return s[..=i].to_string();
            }
        }
    }
    s.to_string()
}

/// `[words](link)` as its words, pictures left out.
fn unlink(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        let Some(j) = rest[i..].find("](") else { break };
        let Some(k) = rest[i + j..].find(')') else {
            break;
        };
        let image = rest[..i].ends_with('!');
        out.push_str(&rest[..if image { i - 1 } else { i }]);
        if !image {
            out.push_str(&rest[i + 1..i + j]);
        }
        rest = &rest[i + j + k + 1..];
    }
    out.push_str(rest);
    out
}

/// A quick-help line without its marks, for egui.
pub fn plain(line: &str) -> String {
    line.replace(['`', '*'], "")
}

/// A key or a mouse action that isn't on a menu, and what it does.
pub struct Binding {
    /// As it is shown: `Ctrl+K`, `Shift+D`, `1–4`, `Shift+right-click`.
    pub keys: &'static str,
    pub what: &'static str,
    /// The heading it is listed under on the shortcuts page.
    pub group: &'static str,
    /// Only the tests look at it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub read: Read,
}

/// How the app reads a binding, which says how the test finds it in the
/// code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// As a key press.
    Key,
    /// As typed text (the axis letters while moving).
    Typed,
}

const fn key(group: &'static str, keys: &'static str, what: &'static str) -> Binding {
    Binding {
        keys,
        what,
        group,
        read: Read::Key,
    }
}

const fn typed(group: &'static str, keys: &'static str, what: &'static str) -> Binding {
    Binding {
        keys,
        what,
        group,
        read: Read::Typed,
    }
}

const VIEWS: &str = "In the views";
const SELECT: &str = "Selecting";
const EDIT: &str = "Move, rotate and scale";
const DRAW: &str = "While drawing";
const MENUS: &str = "Menus and the command palette";
const EDITING: &str = "In Edit mode";

/// Every key the app reads outside its menus. The menus' own shortcuts are
/// read from the menus themselves.
pub const KEYS: &[Binding] = &[
    key(VIEWS, "Home", "Frame everything in the 3D view"),
    key(VIEWS, ".", "Frame the selection in the 3D view"),
    key(VIEWS, "Ctrl+Space", "Show the area under the pointer alone, and back"),
    key(VIEWS, "Ctrl+PgUp", "The previous workspace"),
    key(VIEWS, "Ctrl+PgDn", "The next workspace"),
    key(VIEWS, "/", "Show the selection alone, and back"),
    key(VIEWS, "F3", "Snapping on or off"),
    key(VIEWS, "Alt (hold)", "Suspend snapping while you click"),
    key(VIEWS, "Shift+G", "Show or hide the guides in every view"),
    key(VIEWS, "Alt+Z", "X-ray: see through walls, slabs and roofs"),
    key(VIEWS, "Alt+F2", "Show or hide the trace reference under the view"),
    key(VIEWS, "Ctrl+P", "The sheet in front as a PDF"),
    key(VIEWS, "D", "Dimension (with nothing selected; with a selection, D shows or hides its live dimensions)"),
    key(VIEWS, "T", "Tag"),
    key(VIEWS, "Ctrl+OpenBracket", "Fold the project browser to a rail, or open it"),
    key(VIEWS, "Ctrl+CloseBracket", "Fold Properties to its tabs, or open it"),
    key(VIEWS, "Tab", "Object mode / Edit mode (with several things under the pointer: the next of them)"),
    key(EDITING, "Num1", "Pick points"),
    key(EDITING, "Plus", "Divide: one part more (or a larger size)"),
    key(EDITING, "Equals", "Divide: one part more"),
    key(EDITING, "CloseBracket", "Divide: one part more"),
    key(EDITING, "Minus", "Divide: one part fewer (or a smaller size)"),
    key(EDITING, "OpenBracket", "Divide: one part fewer"),
    key(EDITING, "Num2", "Pick edges"),
    key(EDITING, "Num3", "Pick faces"),
    key(VIEWS, "Shift+T", "Tag all: every door, window, fixture and room not tagged yet"),
    key(VIEWS, "N", "Note (on a sheet: text on the paper)"),
    key(VIEWS, "Shift+A", "Add: find any tool by typing (with nothing selected; with a selection, all of its type)"),
    key(VIEWS, "PageUp", "The previous sheet, or view of the same kind"),
    key(VIEWS, "PageDown", "The next sheet, or view of the same kind"),
    key(VIEWS, "Ctrl+Shift+P", "Export sheets: all, chosen or this one, as PDF"),
    key(VIEWS, "Shift+right-click", "Put the pivot cursor here"),
    key(VIEWS, "F1", "Help about the tool in use (again to close)"),
    key(
        VIEWS,
        "Space",
        "Flip the selected wall, door, window or section, or the wall being drawn; with the Object tool, turn the next object a quarter",
    ),
    key(VIEWS, "Shift+Space", "Flip the hand of the selected door"),
    key(VIEWS, "Ctrl+O", "Open a project"),
    key(VIEWS, "Ctrl+S", "Save"),
    key(VIEWS, "Ctrl+Shift+T", "Next theme"),
    key(SELECT, "Ctrl+A", "Select everything"),
    key(SELECT, "Alt+A", "Select nothing"),
    key(SELECT, "B", "Then drag in 3D: select with a box"),
    key(SELECT, "Shift+click", "Add to the selection, or take away"),
    key(
        SELECT,
        "Tab",
        "Pick the next of the elements under the pointer",
    ),
    key(SELECT, "Shift+Tab", "Pick the previous one"),
    key(
        SELECT,
        "Enter",
        "Open the selected element in Edit mode, or finish editing",
    ),
    key(
        SELECT,
        "Double-click",
        "On the selected element: open it in Edit mode",
    ),
    key(SELECT, "Del", "Delete what is selected"),
    key(SELECT, "H", "Hide the selection"),
    key(SELECT, "Shift+H", "Show only the selection"),
    key(SELECT, "Alt+H", "Show everything again"),
    key(EDIT, "M", "Move by a base point"),
    key(EDIT, "G", "Grab: move with the pointer"),
    key(EDIT, "R", "Rotate"),
    key(EDIT, "S", "Scale"),
    key(EDIT, "Shift+D", "Duplicate and move the copy"),
    typed(
        EDIT,
        "X",
        "While moving: keep to the X axis (Y and Z the same)",
    ),
    typed(
        EDIT,
        "F",
        "While moving: move a face instead of the whole element",
    ),
    key(
        EDIT,
        "Ctrl",
        "While moving: leave copies behind, or snap to steps",
    ),
    key(EDIT, "Enter", "Finish the move with the distance typed"),
    key(EDIT, "Backspace", "Take back the last digit typed"),
    key(EDIT, "Esc", "Step back, then cancel"),
    key(EDIT, "E", "Extrude the selected face or sketch"),
    key(EDITING, "click", "On a corner: pick it (Shift+click picks more)"),
    key(
        EDITING,
        "drag",
        "Off the grips: pick the corners in a box; inside a hole: move it",
    ),
    key(
        EDITING,
        "Del",
        "Remove the picked corners or hole, never the element",
    ),
    key(EDITING, "Backspace", "Remove the picked corners, as Delete"),
    key(EDITING, "Double-click", "On a corner: remove it"),
    key(
        EDITING,
        "right-click",
        "On a corner, an edge or a hole: add or remove corners, make an arc, remove the hole",
    ),
    key(EDITING, "Tab", "While dragging a grip: type a length"),
    key(
        EDITING,
        "Esc",
        "Cancel the drag; then drop the picks; then finish editing",
    ),
    key(DRAW, "Shift (hold)", "Keep lines square"),
    key(
        DRAW,
        "Esc",
        "Drop the clicks so far; again to go back to Select",
    ),
    key(
        DRAW,
        "Enter",
        "Finish an outline, a sketch or a detail component",
    ),
    key(
        DRAW,
        "1–4",
        "Slab, roof and shaft: Polygon, Rectangle, Fill room, From sketch",
    ),
    key(
        DRAW,
        "L",
        "Draw straight lines (slab outlines and sketches)",
    ),
    key(DRAW, "A", "Draw an arc (slab outlines and sketches)"),
    key(DRAW, "C", "Draw a circle (sketches)"),
    key(
        DRAW,
        "Q",
        "Rest on the point or edge under the pointer for its guide lines",
    ),
    key(
        DRAW,
        "K",
        "Keep the guide line under the pointer as a guide",
    ),
    key(
        MENUS,
        "Ctrl+K",
        "Command palette: find any command, tool or view",
    ),
    key(MENUS, "Arrow keys", "Walk an open menu"),
    key(MENUS, "Enter", "Pick the highlighted entry"),
    key(MENUS, "Esc", "Close the menu"),
];

/// The keys of a binding as egui names them (`Ctrl+Shift+T` reads T),
/// leaving out mouse actions and modifiers held on their own.
#[cfg(test)]
pub fn key_names(keys: &str) -> Vec<String> {
    let last = keys.rsplit('+').next().unwrap_or(keys).trim();
    let last = last.trim_end_matches(" (hold)");
    match last {
        "Ctrl" | "Shift" | "Alt" | "click" | "right-click" | "drag" | "Double-click" => vec![],
        "Del" => vec!["Delete".into()],
        "Esc" => vec!["Escape".into()],
        "." => vec!["Period".into()],
        "," => vec!["Comma".into()],
        "/" => vec!["Slash".into()],
        "PgUp" => vec!["PageUp".into()],
        "PgDn" => vec!["PageDown".into()],
        "1–4" => (1..=4).map(|n| format!("Num{n}")).collect(),
        "Arrow keys" => ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]
            .map(String::from)
            .to_vec(),
        k => vec![k.to_string()],
    }
}

/// The keyboard shortcuts page: the page's own text, then a table for the
/// menus' shortcuts (from `menus`, as (menu, items)) and one for each
/// group of [`KEYS`].
pub fn shortcuts_page(menus: &[(&str, Vec<Item>)]) -> String {
    fn walk(items: &[Item], path: &str, out: &mut Vec<(String, String, String)>) {
        for i in items {
            match i {
                Item::Action {
                    label, shortcut, ..
                } if !shortcut.is_empty() => {
                    if !out.iter().any(|(k, l, _)| k == shortcut && l == label) {
                        out.push((shortcut.to_string(), label.clone(), path.to_string()));
                    }
                }
                Item::Sub { label, items, .. } => walk(items, &format!("{path} › {label}"), out),
                _ => {}
            }
        }
    }
    let mut md = text("shortcuts").unwrap_or("").trim_end().to_string();
    md.push_str("\n\n## On the menus\n\n| Keys | What it does | Menu |\n| --- | --- | --- |\n");
    let mut rows = vec![];
    for (menu, items) in menus {
        walk(items, menu, &mut rows);
    }
    for (k, label, path) in rows {
        md.push_str(&format!("| `{k}` | {label} | {path} |\n"));
    }
    let mut groups: Vec<&str> = vec![];
    for b in KEYS {
        if !groups.contains(&b.group) {
            groups.push(b.group);
        }
    }
    for g in groups {
        md.push_str(&format!(
            "\n## {g}\n\n| Keys | What it does |\n| --- | --- |\n"
        ));
        for b in KEYS.iter().filter(|b| b.group == g) {
            md.push_str(&format!("| `{}` | {} |\n", b.keys, b.what));
        }
    }
    md
}

/// A page's text as it is shown: the version filled in.
pub fn shown(md: &str) -> String {
    md.replace("{version}", env!("CARGO_PKG_VERSION"))
        .replace("{build}", &build())
}

/// The commit and day this app was built: `3b056f4 · 4 Nov 2026`.
pub fn build() -> String {
    let day: i64 = env!("CREABIM_BUILD_DAY").parse().unwrap_or(0);
    format!("{} · {}", env!("CREABIM_COMMIT"), date_of_day(day))
}

/// The version line of About CreaBIM: `Version 0.1.0 · build 3b056f4 · 4 Nov 2026`.
pub fn version_line() -> String {
    format!("Version {} · build {}", env!("CARGO_PKG_VERSION"), build())
}

/// A day counted from 1970-01-01, as `4 Nov 2026` (the civil calendar).
fn date_of_day(z: i64) -> String {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}

/// The white lockup, for dark themes.
const LOCKUP_WHITE: &[u8] = include_bytes!("../assets/logo/png/creabim-lockup-white-640x200.png");

/// A picture as a `data:` address, for the Help panel's page; on a dark
/// theme the logo is the white one.
pub fn data_url_for(path: &str, dark: bool) -> Option<String> {
    if dark && path == "img/creabim-lockup.png" {
        return Some(format!("data:image/png;base64,{}", base64(LOCKUP_WHITE)));
    }
    let (_, bytes) = IMAGES.iter().find(|i| i.0 == path)?;
    let mime = if path.ends_with(".png") {
        "image/png"
    } else {
        "image/jpeg"
    };
    Some(format!("data:{mime};base64,{}", base64(bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= c.len() {
                A[(n >> shift & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// What the Help panel's script reads first: every page with its section,
/// id, title and text.
pub fn pages_json() -> serde_json::Value {
    serde_json::Value::Array(
        PAGES
            .iter()
            .map(|(section, id, md)| {
                serde_json::json!({"section": section, "id": id, "title": title(md), "md": shown(md)})
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_days_read_as_dates() {
        assert_eq!(date_of_day(0), "1 Jan 1970");
        assert_eq!(date_of_day(20_396), "4 Nov 2025");
        assert_eq!(date_of_day(20_761), "4 Nov 2026");
        assert_eq!(date_of_day(11_016), "29 Feb 2000");
    }

    #[test]
    fn base64_matches_the_usual_encoding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn every_page_has_a_title_and_every_file_is_a_page() {
        for (_, id, md) in PAGES {
            assert!(!title(md).is_empty(), "{id} has no # title");
            assert!(
                id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{id}"
            );
        }
        let mut ids: Vec<&str> = PAGES.iter().map(|p| p.1).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), PAGES.len(), "two pages share an id");
        // a Markdown file left out of the list would never show
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/help");
        for f in std::fs::read_dir(dir).unwrap() {
            let name = f.unwrap().file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_suffix(".md").filter(|n| *n != "README") {
                assert!(text(id).is_some(), "help/{name} is not in PAGES");
            }
        }
    }

    #[test]
    fn links_go_to_pages_and_headings_that_exist() {
        for (_, id, md) in PAGES {
            let (links, images) = targets(md);
            for l in links {
                if l.starts_with("https://") || l.starts_with("http://") {
                    continue;
                }
                assert!(
                    exists(l),
                    "{id} links to {l}, which isn't a page or heading"
                );
            }
            for i in images {
                assert!(
                    data_url_for(i, false).is_some(),
                    "{id} shows {i}, which isn't in IMAGES"
                );
            }
        }
        // and every picture is shown somewhere
        let all: Vec<&str> = PAGES.iter().flat_map(|p| targets(p.2).1).collect();
        for (path, bytes) in IMAGES {
            assert!(all.contains(path), "{path} isn't shown on any page");
            assert!(bytes.len() < 160_000, "{path} is big for the guide");
        }
    }

    #[test]
    fn every_tool_has_a_page() {
        for t in Tool::ALL {
            assert!(exists(tool_page(t)), "{:?}: {}", t, tool_page(t));
        }
    }

    #[test]
    fn every_tool_has_a_few_lines_of_quick_help() {
        for t in Tool::ALL {
            let q = quick(tool_page(t));
            assert!(!q.is_empty() && q.len() <= 4, "{t:?}: {q:?}");
            for l in &q {
                assert!(l.chars().count() < 420, "{t:?}: {l}");
                assert!(!l.contains("](") && !l.starts_with('#'), "{t:?}: {l}");
            }
        }
    }

    #[test]
    fn quick_help_is_the_first_paragraph_or_the_first_steps() {
        // a page that opens with a paragraph
        let q = quick("footings");
        assert!(q[0].starts_with("A **footing** is the foundation"), "{q:?}");
        assert_eq!(q.len(), 1, "two sentences of a long paragraph: {q:?}");
        // a page that opens with steps: the first three
        let q = quick("walls");
        assert_eq!(q.len(), 3, "{q:?}");
        assert!(q[0].contains("**Wall** tool"), "{q:?}");
        // a heading of a page, links as their words
        let q = quick("holes#shafts");
        assert!(q[0].starts_with("A **shaft** is a vertical hole"), "{q:?}");
        assert_eq!(
            unlink("See [walls](walls#joins) ![x](img/a.jpg)here."),
            "See walls here."
        );
        assert_eq!(sentences("One. Two. Three.", 2), "One. Two.");
    }

    #[test]
    fn slugs_read_like_the_heading() {
        assert_eq!(slug("Location line"), "location-line");
        assert_eq!(slug("Radius and diameter"), "radius-and-diameter");
        assert_eq!(slug("Flip, hand & swing"), "flip-hand-swing");
        assert_eq!(slug("IFC4 (.ifc)"), "ifc4-ifc");
    }

    #[test]
    fn links_and_images_are_found() {
        let (l, i) = targets("See [walls](walls#joins) and ![a wall](img/a.jpg), [x](y).");
        assert_eq!(l, vec!["walls#joins", "y"]);
        assert_eq!(i, vec!["img/a.jpg"]);
    }

    #[test]
    fn every_binding_names_keys_egui_knows() {
        for b in KEYS {
            for k in key_names(b.keys) {
                // as the code names them: egui::Key::Num1, Key::Period
                assert!(
                    eframe::egui::Key::ALL
                        .iter()
                        .any(|key| format!("{key:?}") == k),
                    "{}: {k} isn't a key",
                    b.keys
                );
            }
        }
    }

    /// The keys the app's code reads (`egui::Key::…`), by name.
    fn keys_in_code() -> Vec<String> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut out = vec![];
        for f in std::fs::read_dir(dir).unwrap() {
            let path = f.unwrap().path();
            if path.file_name().is_some_and(|n| n == "help.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            let mut rest = src.as_str();
            while let Some(i) = rest.find("Key::") {
                rest = &rest[i + 5..];
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
                // `Key::from_name` and the like are not keys
                if name.starts_with(|c: char| c.is_ascii_uppercase()) && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }

    #[test]
    fn the_shortcuts_page_lists_every_key_the_app_reads() {
        let app = crate::app::test_app();
        let menus = app.help_menus();
        let mut listed: Vec<String> = KEYS.iter().flat_map(|b| key_names(b.keys)).collect();
        fn menu_keys(items: &[Item], out: &mut Vec<String>) {
            for i in items {
                match i {
                    Item::Action { shortcut, .. } => out.extend(key_names(shortcut)),
                    Item::Sub { items, .. } => menu_keys(items, out),
                    _ => {}
                }
            }
        }
        for (_, items) in &menus {
            menu_keys(items, &mut listed);
        }
        for k in keys_in_code() {
            assert!(
                listed.contains(&k),
                "the code reads the {k} key, but the shortcuts page doesn't list it: add it to help::KEYS"
            );
        }
        // and no binding is listed that the code no longer reads
        let code = keys_in_code();
        for b in KEYS.iter().filter(|b| b.read == Read::Key) {
            for k in key_names(b.keys) {
                assert!(
                    code.contains(&k),
                    "help::KEYS lists {} but the code doesn't read {k}",
                    b.keys
                );
            }
        }
        // every menu shortcut and every binding is on the page
        let page = shortcuts_page(&menus);
        for (_, items) in &menus {
            let mut rows = vec![];
            fn rows_of(items: &[Item], out: &mut Vec<(String, String)>) {
                for i in items {
                    match i {
                        Item::Action {
                            label, shortcut, ..
                        } if !shortcut.is_empty() => {
                            out.push((label.clone(), shortcut.to_string()))
                        }
                        Item::Sub { items, .. } => rows_of(items, out),
                        _ => {}
                    }
                }
            }
            rows_of(items, &mut rows);
            for (label, k) in rows {
                assert!(
                    page.contains(&format!("| `{k}` | {label} |")),
                    "{label} ({k})"
                );
            }
        }
        for b in KEYS {
            assert!(page.contains(&format!("| `{}` | {}", b.keys, b.what)));
        }
        for c in crate::commands::Command::ALL {
            if !c.shortcut().is_empty() {
                assert!(page.contains(&format!("`{}`", c.shortcut())), "{c:?}");
            }
        }
    }

    #[test]
    fn about_shows_the_lockup_in_the_themes_colour() {
        assert!(text("about").unwrap().contains("img/creabim-lockup.png"));
        let light = data_url_for("img/creabim-lockup.png", false).unwrap();
        let dark = data_url_for("img/creabim-lockup.png", true).unwrap();
        assert!(light.starts_with("data:image/png;base64,"));
        assert_ne!(light, dark);
    }
}
