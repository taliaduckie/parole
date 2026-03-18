use eframe::egui;
use crate::app::PraatlyApp;

pub fn show(ctx: &egui::Context, app: &mut PraatlyApp) {
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Open…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Audio", &["wav", "flac", "mp3", "aiff"])
                    .pick_file()
                { app.load_file(path); }
            }
            ui.separator();
            let lbl = if app.player.playing { "⏹ Stop" } else { "▶ Play" };
            if ui.button(lbl).clicked() {
                if app.player.playing { app.player.stop(); }
                else if let Some(buf) = &app.buffer {
                    let (s, e) = app.selection.unwrap_or((app.view_start, app.view_end));
                    app.player.play(buf.slice_mono(s, e), buf.sample_rate);
                }
            }
            ui.separator();
            ui.label("Show:");
            ui.checkbox(&mut app.show_spectrogram, "Spectrogram");
            ui.checkbox(&mut app.show_pitch,       "Pitch");
            ui.checkbox(&mut app.show_formants,    "Formants");
            ui.checkbox(&mut app.show_textgrid,    "TextGrid");
        });
    });
}
