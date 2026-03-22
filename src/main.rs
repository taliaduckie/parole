// hi. welcome to parole. we start here.
// if this file ever gets longer than 30 lines something has gone wrong
// and I will want to talk about it.

use eframe::egui;
use std::path::PathBuf;

mod app;
mod audio;
mod dsp;
mod ui;
mod annotation;

fn main() {
    env_logger::init();

    // grab the first CLI arg if there is one — lets you do `parole myfile.wav`
    // like a person who uses their tools correctly
    let audio_path: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Parole")
            .with_inner_size([1280.0, 800.0])   // vibes-based window size
            .with_min_inner_size([800.0, 500.0]), // below this it gets sad
        ..Default::default()
    };

    eframe::run_native(
        "Parole",
        options,
        // .clone() here because the closure needs to own it and honestly I respect that
        Box::new(move |cc| Box::new(app::PraatlyApp::new(cc, audio_path.clone()))),
    ).expect("Failed to start Parole"); // if this panics we have bigger problems than a comment can address
}
