use eframe::egui;
use crate::app::PraatlyApp;

pub fn show(ctx: &egui::Context, app: &mut PraatlyApp) {
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {

            // ── File ─────────────────────────────────────────────────────
            if ui.button("Open…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Audio", &["wav", "flac", "mp3", "aiff"])
                    .pick_file()
                { app.load_file(path); }
            }

            ui.separator();

            // ── Playback ──────────────────────────────────────────────────
            let play_lbl = if app.player.playing { "⏹ Stop" } else { "▶ Play" };
            if ui.button(play_lbl).clicked() {
                if app.player.playing {
                    app.player.stop();
                } else if let Some(buf) = &app.buffer {
                    let (s, e) = app.selection.unwrap_or((app.view_start, app.view_end));
                    app.player.play(buf.slice_mono(s, e), buf.sample_rate);
                }
            }

            ui.separator();

            // ── Recording ────────────────────────────────────────────────
            let rec_lbl = if app.recording {
                // Pulsing red dot via a simple label — nothing fancy
                "⏺ Stop recording"
            } else {
                "⏺ Record"
            };

            let rec_btn = egui::Button::new(rec_lbl);
            let rec_btn = if app.recording {
                // Tint the button red while recording so it's obvious
                rec_btn.fill(egui::Color32::from_rgb(160, 40, 40))
            } else {
                rec_btn
            };

            if ui.add(rec_btn).clicked() {
                if app.recording {
                    // Stop recording
                    app.recording = false;

                    // Offer save dialog immediately
                    if !app.recorded_samples.is_empty() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("WAV audio", &["wav"])
                            .set_file_name("recording.wav")
                            .save_file()
                        {
                            app.save_recording_wav(path);
                        }
                    }
                } else {
                    // Start recording — clear previous buffer
                    app.recorded_samples.clear();
                    app.recording = true;
                    app.save_status = None;
                    // TODO: wire up cpal input stream in audio/player.rs
                    // For now this sets the flag; the cpal input callback
                    // will push samples into app.recorded_samples via Arc<Mutex<>>
                    log::info!("Recording started");
                }
            }

            // Save button — available after recording without re-recording
            if !app.recorded_samples.is_empty() && !app.recording {
                if ui.button("💾 Save…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("WAV audio", &["wav"])
                        .set_file_name("recording.wav")
                        .save_file()
                    {
                        app.save_recording_wav(path);
                    }
                }
            }

            // Status message (appears briefly after save attempt)
            if let Some(status) = &app.save_status {
                ui.separator();
                ui.label(
                    egui::RichText::new(status)
                        .color(egui::Color32::from_rgb(140, 200, 140))
                        .small()
                );
            }

            ui.separator();

            // ── View toggles ──────────────────────────────────────────────
            ui.label("Show:");
            ui.checkbox(&mut app.show_spectrogram, "Spectrogram");
            ui.checkbox(&mut app.show_pitch,       "Pitch");
            ui.checkbox(&mut app.show_formants,    "Formants");
            ui.checkbox(&mut app.show_textgrid,    "TextGrid");

            ui.separator();

            // ── Help ──────────────────────────────────────────────────────
            let help_lbl = if app.show_help { "✕ Help" } else { "? Help" };
            if ui.button(help_lbl).clicked() {
                app.show_help = !app.show_help;
            }
        });
    });
}
