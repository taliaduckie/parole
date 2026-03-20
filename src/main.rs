use eframe::egui;
use std::path::PathBuf;

mod app;
mod audio;
mod dsp;
mod ui;
mod annotation;

fn main() {
    env_logger::init();
    let audio_path: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Parole")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Parole",
        options,
        Box::new(move |cc| Box::new(app::PraatlyApp::new(cc, audio_path.clone()))),
    ).expect("Failed to start Parole");
}
