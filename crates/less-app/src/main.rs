//! LESS-rs giriş noktası: pencereyi açar ve karşılama penceresini gösterir.

mod app;
mod appearance;
mod clipboard;
mod cmdline;
mod dialogs;
mod disk;
mod dispatch;
mod editor;
mod files;
mod fonts;
mod grep;
mod keys;
mod notes;
mod panels;
mod persistence;
mod picker;
mod project;
mod settings;
mod startup;
mod status;
mod sysinfo;
mod welcome;
mod widgets;
mod window;
mod window_state;

use std::time::Instant;

use eframe::egui;

/// Pencereyi `LESS` başlığı, uygulama ikonu ve en son pencerenin durumuyla (yoksa varsayılan boyutla) açar; süreç başlangıcı açılış süresi için tutulur.
fn main() -> eframe::Result {
    let started = Instant::now();
    let launch = match startup::parse(std::env::args_os().skip(1)) {
        Ok(startup::ParseOutcome::Run(launch)) => {
            startup::adopt_login_env();
            launch
        }
        Ok(startup::ParseOutcome::Help) => {
            println!("{}", startup::HELP);
            return Ok(());
        }
        Ok(startup::ParseOutcome::Version) => {
            println!("LESS {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Err(error) => {
            eprintln!("{error}\n\n{}", startup::HELP);
            std::process::exit(2);
        }
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("LESS")
        .with_app_id("com.oguz-dev.less-rs")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([640.0, 420.0])
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false);
    let windows = window_state::Windows::load();
    if let Some(last) = windows.last() {
        viewport = last.builder(viewport);
    }
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "LESS",
        options,
        Box::new(move |cc| Ok(Box::new(app::LessApp::new(cc, started, launch, windows)))),
    )
}
