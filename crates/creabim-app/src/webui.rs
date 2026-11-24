//! The interface around the views: menus, tools, the project browser and
//! the status bar are web pages (the webview Tauri uses, through wry),
//! placed as child windows over the egui window. The 3D and 2D views stay
//! native underneath.
//!
//! Pages talk to the app with JSON: `window.ipc.postMessage` one way and
//! `CB.state(...)` scripts the other.

use eframe::egui;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{Rect, WebContext, WebView, WebViewBuilder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Top,
    Left,
    Status,
    Popup,
    /// The 3D view's navigation buttons, over its top-right corner.
    Nav,
    /// The guide, at the right of the window while it is open.
    Help,
}

const PANES: usize = 6;

impl Pane {
    /// In the order they are made, which stacks the popup on top.
    const ALL: [Pane; PANES] = [
        Pane::Top,
        Pane::Left,
        Pane::Status,
        Pane::Nav,
        Pane::Help,
        Pane::Popup,
    ];

    fn name(self) -> &'static str {
        match self {
            Pane::Top => "top",
            Pane::Left => "left",
            Pane::Status => "status",
            Pane::Popup => "popup",
            Pane::Nav => "nav",
            Pane::Help => "help",
        }
    }
}

/// A message from a page, with the pane it came from.
pub struct Msg {
    pub pane: Pane,
    pub body: Value,
}

pub struct WebUi {
    views: Vec<(Pane, WebView)>,
    /// The pages' shared browser profile, kept alive as long as they are.
    _context: WebContext,
    inbox: Arc<Mutex<Vec<Msg>>>,
    /// The last state sent to each pane, so unchanged state isn't re-sent.
    sent: [String; PANES],
    /// Where each pane is, in points.
    pub rects: [egui::Rect; PANES],
    popup_shown: bool,
    #[cfg(target_os = "linux")]
    x11: Option<X11Focus>,
}

const CSS: &str = include_str!("../ui/app.css");
const JS: &str = include_str!("../ui/app.js");
const ICONS: &str = include_str!("../ui/icons.js");

/// icons.js as name -> markup: one `"name": "markup",` per line.
pub fn icon_markup() -> Vec<(String, String)> {
    ICONS
        .lines()
        .filter(|l| l.starts_with('"'))
        .filter_map(|l| {
            let l = l.trim_end().trim_end_matches(',');
            let v: serde_json::Value = serde_json::from_str(&format!("{{{l}}}")).ok()?;
            let (k, v) = v.as_object()?.iter().next()?;
            Some((k.clone(), v.as_str()?.to_string()))
        })
        .collect()
}

fn page(pane: Pane) -> String {
    // the guide's pages are in its pane from the start, for searching
    let pages = match pane {
        Pane::Help => format!(
            "<script>window.HELP = {};</script>",
            crate::help::pages_json().to_string().replace("</", "<\\/")
        ),
        _ => String::new(),
    };
    format!(
        "<!doctype html><html data-pane=\"{}\"><head><meta charset=\"utf-8\"><style>{CSS}</style></head>\
         <body><div id=\"root\"></div><script>{ICONS}</script>{pages}<script>{JS}</script></body></html>",
        pane.name()
    )
}

/// Where the pages keep their browser data (cache, local storage): the
/// user's own app data, never beside the program, which an installed copy
/// can't write to (C:\Program Files). `None` leaves the platform's default.
fn data_dir() -> Option<std::path::PathBuf> {
    let var = |k: &str| std::env::var_os(k).map(std::path::PathBuf::from);
    let base = if cfg!(windows) {
        var("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local").join("share")))
    }?;
    Some(base.join("CreaBIM").join("WebView"))
}

impl WebUi {
    /// Make the pages as children of the app window. Fails where child
    /// webviews aren't possible (Wayland without XWayland), and the app then
    /// keeps its egui panels.
    ///
    /// A message from a page wakes the app (`ctx`), so no frame has to run
    /// while nothing happens.
    pub fn new(frame: &eframe::Frame, ctx: &egui::Context) -> Result<Self, String> {
        let inbox: Arc<Mutex<Vec<Msg>>> = Default::default();
        let mut context = WebContext::new(data_dir());
        let mut views = vec![];
        for pane in Pane::ALL {
            let inbox = inbox.clone();
            let ctx = ctx.clone();
            let view = WebViewBuilder::new_with_web_context(&mut context)
                .with_html(page(pane))
                .with_bounds(Rect {
                    position: PhysicalPosition::new(0, 0).into(),
                    size: PhysicalSize::new(10, 10).into(),
                })
                .with_background_color(BACKGROUND)
                // the popup's corners can be round (Save the changes):
                // what its page leaves clear shows the app behind
                .with_transparent(pane == Pane::Popup)
                .with_ipc_handler(move |r| {
                    if let Ok(body) = serde_json::from_str(r.body()) {
                        inbox.lock().unwrap().push(Msg { pane, body });
                        ctx.request_repaint();
                    }
                })
                .build_as_child(frame)
                .map_err(|e| e.to_string())?;
            views.push((pane, view));
        }
        Ok(Self {
            views,
            _context: context,
            inbox,
            sent: Default::default(),
            rects: [egui::Rect::NOTHING; PANES],
            popup_shown: false,
            #[cfg(target_os = "linux")]
            x11: X11Focus::new(frame),
        })
    }

    fn view(&self, pane: Pane) -> &WebView {
        &self.views.iter().find(|(p, _)| *p == pane).unwrap().1
    }

    /// Let the pages run: on Linux they live on GTK, which needs pumping.
    pub fn pump() {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    pub fn take(&self) -> Vec<Msg> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }

    /// Put a pane over `rect` (in points).
    pub fn place(&mut self, pane: Pane, rect: egui::Rect, ppp: f32) {
        let i = pane as usize;
        if self.rects[i] == rect {
            return;
        }
        self.rects[i] = rect;
        let _ = self.view(pane).set_bounds(Rect {
            position: PhysicalPosition::new(
                (rect.min.x * ppp).round() as i32,
                (rect.min.y * ppp).round() as i32,
            )
            .into(),
            size: PhysicalSize::new(
                (rect.width() * ppp).round().max(1.0) as u32,
                (rect.height() * ppp).round().max(1.0) as u32,
            )
            .into(),
        });
    }

    /// Send a pane its state, if it changed since last time.
    pub fn send(&mut self, pane: Pane, state: &Value) {
        let text = state.to_string();
        let i = pane as usize;
        if self.sent[i] == text {
            return;
        }
        let _ = self
            .view(pane)
            .evaluate_script(&format!("window.CB && CB.state({text})"));
        self.sent[i] = text;
    }

    /// Run a script in a pane's page.
    pub fn run(&self, pane: Pane, script: &str) {
        let _ = self.view(pane).evaluate_script(script);
    }

    /// Run a script in every pane's page.
    pub fn run_all(&self, script: &str) {
        for (_, v) in &self.views {
            let _ = v.evaluate_script(script);
        }
    }

    /// Re-send every state next frame (after a page reloads).
    pub fn resend(&mut self, pane: Pane) {
        self.sent[pane as usize].clear();
    }

    pub fn show_popup(&mut self, rect: egui::Rect, ppp: f32) {
        self.place(Pane::Popup, rect, ppp);
        self.popup_shown = true;
    }

    /// The popup is parked off the window rather than hidden: GTK child
    /// windows don't always hide and show again cleanly.
    pub fn hide_popup(&mut self) {
        if self.popup_shown || self.rects[Pane::Popup as usize] == egui::Rect::NOTHING {
            self.place(Pane::Popup, PARKED, 1.0);
            self.popup_shown = false;
            let _ = self
                .view(Pane::Popup)
                .evaluate_script("window.CB && CB.state({items: []})");
            self.sent[Pane::Popup as usize].clear();
        }
    }

    /// Hide the panes for the classic panels, or let them come back.
    /// Like the popup, they are parked off the window rather than hidden.
    pub fn hide_all(&mut self, hidden: bool) {
        for pane in Pane::ALL {
            if hidden {
                self.place(pane, PARKED, 1.0);
            } else {
                // placed again on the next frame
                self.rects[pane as usize] = egui::Rect::NOTHING;
            }
        }
        self.popup_shown = false;
    }

    /// Park a pane off the window, as for the popup.
    pub fn park(&mut self, pane: Pane) {
        self.place(pane, PARKED, 1.0);
    }

    /// Give a pane the keyboard, for typing in it straight away.
    pub fn focus(&mut self, pane: Pane) {
        let _ = self.view(pane).focus();
    }

    /// Hand the keyboard back to the views after a click on a page.
    pub fn focus_views(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(x) = &self.x11 {
            x.focus();
            return;
        }
        let _ = self.view(Pane::Top).focus_parent();
    }
}

const PARKED: egui::Rect = egui::Rect {
    min: egui::pos2(-4000.0, -4000.0),
    max: egui::pos2(-3990.0, -3990.0),
};

/// The page background, so a page never flashes white while loading.
const BACKGROUND: (u8, u8, u8, u8) = (23, 25, 30, 255);

/// Giving the keyboard back to the app window. GTK's own call only reaches
/// the page's container, so this asks the X server directly.
#[cfg(target_os = "linux")]
struct X11Focus {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    window: std::ffi::c_ulong,
}

#[cfg(target_os = "linux")]
impl X11Focus {
    fn new(frame: &eframe::Frame) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window = match frame.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(w) => w.window,
            _ => return None,
        };
        let xlib = x11_dl::xlib::Xlib::open().ok()?;
        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return None;
        }
        Some(Self {
            xlib,
            display,
            window,
        })
    }

    fn focus(&self) {
        unsafe {
            (self.xlib.XSetInputFocus)(
                self.display,
                self.window,
                x11_dl::xlib::RevertToParent,
                x11_dl::xlib::CurrentTime,
            );
            (self.xlib.XFlush)(self.display);
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for X11Focus {
    fn drop(&mut self) {
        unsafe { (self.xlib.XCloseDisplay)(self.display) };
    }
}

#[cfg(test)]
mod approved_icons {
    //! The toolbar and action icons are the owner's approved set, copied
    //! verbatim from `ui/approved/*.json`. Changing one needs the owner's
    //! OK: change the JSON first, then this test says where icons.js lags.

    use std::collections::BTreeMap;

    /// Each approved icon's name in the JSON, and its key in icons.js.
    const KEYS: &[(&str, &str)] = &[
        ("Select", "mouse-pointer-2"),
        ("Align", "align-start-vertical"),
        ("Wall", "wall"),
        ("Door", "door"),
        ("Shaft", "shaft"),
        ("Mass", "box"),
        ("Column", "column"),
        ("Beam", "beam"),
        ("Slab", "slab"),
        ("Stair", "stair"),
        ("Footing", "footing"),
        ("Object", "object"),
        ("Extrusion", "extrusion"),
        ("Sweep", "sweep"),
        ("Void", "void"),
        ("Room", "room"),
        ("Ceiling", "ceiling"),
        ("Window", "window"),
        ("Opening", "opening"),
        ("Roof", "roof"),
        ("Railing", "railing"),
        ("Room separator", "room-separator"),
        ("Curtain wall", "curtain-wall"),
        ("Screen wall", "screen-wall"),
        ("Fixtures", "fixtures"),
        ("Terrain", "terrain"),
        ("Paving", "paving"),
        ("Planting", "planting"),
        ("Section", "scissors-line-dashed"),
        ("Callout", "callout"),
        ("Elevation", "elevation"),
        ("Crop box", "crop-box"),
        ("Frame all", "frame-all"),
        ("Hidden elements", "hidden-elements"),
        ("X-ray", "xray"),
        ("Dimension", "dimension"),
        ("Tag", "tag"),
        ("Angle", "angle"),
        ("Radius", "radius"),
        ("Diameter", "diameter"),
        ("Arc length", "arc-length"),
        ("Note", "message-square-text"),
        ("Spot level", "spot-level"),
        ("Detail component", "detail-component"),
        ("Sketch", "pen-tool"),
        ("Guide", "guide"),
        ("Guide lines", "guide-lines"),
        ("Level", "level"),
        ("Copy to levels", "copy-levels"),
        ("Grid", "grid-lines"),
        ("Switch join order", "switch-join"),
        ("Units", "units"),
        ("Material", "palette"),
        ("Graphic styles", "anno-styles"),
        ("Colour scheme", "colour-scheme"),
        ("View range", "view-range"),
        ("Underlay", "underlay"),
        ("Workspace", "workspace"),
        ("Maximise area", "maximise"),
        ("Frame selection (view header)", "frame-selection"),
        ("Edit outline", "edit-outline"),
        ("Duplicate", "duplicate"),
        ("Select similar", "select-similar"),
        ("Hide (H)", "eye-off"),
        ("Isolate (/)", "isolate"),
        ("Frame (.)", "frame-selection"),
        ("Open view", "open-view"),
        ("Flip", "flip"),
        ("Make a type", "make-type"),
        ("Attach top", "attach-top"),
        ("Select tagged", "select-tagged"),
        ("Straighten leader", "straighten-leader"),
        ("More", "ellipsis"),
        ("Delete", "trash-2"),
        ("FreeCAD file (F and gear)", "freecad"),
    ];

    /// icons.js as key -> markup: one `"key": "markup",` per line.
    fn icons_js() -> BTreeMap<String, String> {
        super::ICONS
            .lines()
            .filter(|l| l.starts_with('"'))
            .map(|l| {
                let l = l.trim_end().trim_end_matches(',');
                let v: serde_json::Value =
                    serde_json::from_str(&format!("{{{l}}}")).expect("an icons.js line");
                let (k, v) = v.as_object().unwrap().iter().next().unwrap();
                (k.clone(), v.as_str().unwrap().to_string())
            })
            .collect()
    }

    fn approved(json: &str) -> Vec<(String, String)> {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut out = Vec::new();
        let mut take = |m: &serde_json::Map<String, serde_json::Value>| {
            for (k, v) in m {
                match v {
                    serde_json::Value::String(s) => out.push((k.clone(), s.clone())),
                    serde_json::Value::Object(_) => {}
                    _ => panic!("{k}: not markup"),
                }
            }
        };
        let top = v.as_object().unwrap();
        take(top);
        for g in top.values() {
            if let Some(m) = g.as_object() {
                take(m);
            }
        }
        out
    }

    #[test]
    fn icons_js_matches_the_approved_set_verbatim() {
        let js = icons_js();
        let keys: BTreeMap<_, _> = KEYS.iter().copied().collect();
        let mut drift = Vec::new();
        for json in [
            include_str!("../ui/approved/icons_final.json"),
            include_str!("../ui/approved/icons_actions.json"),
        ] {
            for (name, markup) in approved(json) {
                let Some(key) = keys.get(name.as_str()) else {
                    drift.push(format!(
                        "{name}: approved, but no icons.js key is named for it"
                    ));
                    continue;
                };
                match js.get(*key) {
                    None => drift.push(format!("{name}: icons.js lacks {key}")),
                    Some(m) if *m != markup => drift.push(format!("{name}: {key} differs")),
                    _ => {}
                }
            }
        }
        assert!(drift.is_empty(), "icons drifted:\n{}", drift.join("\n"));
    }

    /// The New project window's choices scroll between its title and its
    /// buttons, so a window too short for them keeps Create in reach
    /// (issue 24).
    #[test]
    fn the_new_project_windows_choices_scroll() {
        let rule = super::CSS
            .lines()
            .find(|l| l.starts_with(".np-body {"))
            .expect("a rule for the choices");
        assert!(rule.contains("overflow-y: auto") && rule.contains("min-height: 0"));
        // the page puts the choices, and only them, in that part
        let body = super::JS
            .find("class=\"np-body\"")
            .expect("the choices' part");
        let starts = super::JS.find("class=\"np-starts\"").unwrap();
        let samples = super::JS.find("class=\"np-samples\"").unwrap();
        let foot = super::JS.find("class=\"np-foot\"").unwrap();
        assert!(body < starts && starts < samples && samples < foot);
        let between = &super::JS[samples..foot];
        assert!(
            between.contains("</div></div>"),
            "the part closes before the buttons"
        );
    }
}
