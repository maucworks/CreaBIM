//! CreaBIM desktop app.

mod actlog;
mod align3d;
mod aligntool;
mod annotate;
mod app;
mod areas;
mod boxsel;
mod browser;
mod builds;
mod camera;
mod centres;
mod choices;
mod commands;
mod detailtool;
mod editmode;
mod files;
mod fillpreview;
mod ghost;
mod handles;
mod help;
mod hoverpick;
mod icons;
mod memberstool;
mod modal;
mod modguides;
mod multiedit;
mod objecttool;
mod optionbar;
mod outline;
mod paneface;
mod partstool;
mod planview;
mod props;
mod render3d;
mod roomtool;
mod schedulepane;
mod scriptrun;
mod scriptui;
mod selection;
mod sheetview;
mod sketch;
mod slabtool;
mod snaps;
mod svgicon;
mod tempdims;
mod textures;
mod theme;
mod tooltabs;
mod trace;
mod tracker;
mod ui_model;
mod units;
mod viewcube;
mod webui;

/// The window's and the taskbar's icon: the app tile, sage.
fn app_icon() -> std::sync::Arc<eframe::egui::IconData> {
    let png = include_bytes!("../assets/logo/png/creabim-app-icon-256.png");
    std::sync::Arc::new(eframe::icon_data::from_png_bytes(png).unwrap_or_default())
}

fn main() -> eframe::Result {
    // the window's thread answers first on a busy machine
    creabim_geom::prio::ui();
    // a crash says why in the activity log, with where it happened (MU39:
    // one closed the app with nothing to go on)
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let trace = std::backtrace::Backtrace::force_capture();
        actlog::line(format!(
            "CRASH: {info}
{trace}"
        ));
        before(info);
    }));
    #[cfg(target_os = "linux")]
    linux_setup();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("CreaBIM")
            .with_inner_size([1400.0, 860.0])
            .with_icon(app_icon()),
        renderer: eframe::Renderer::Wgpu,
        #[cfg(target_os = "linux")]
        event_loop_builder: Some(Box::new(|b| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            b.with_x11();
        })),
        ..Default::default()
    };
    eframe::run_native(
        "CreaBIM",
        options,
        Box::new(|cc| Ok(Box::new(app::CreaBimApp::new(cc)))),
    )
}

/// The web panes are GTK child windows of an X11 window, so on Wayland the
/// app runs through XWayland, and GTK has to be started first.
#[cfg(target_os = "linux")]
fn linux_setup() {
    // Without this WebKit draws nothing on some GPUs and in virtual displays.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    std::env::set_var("GDK_BACKEND", "x11");
    if gtk::init().is_err() {
        // The app still runs, with the classic panels.
        std::env::set_var("CREABIM_CLASSIC_UI", "1");
    }
}
