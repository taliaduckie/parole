use eframe::egui;
use crate::app::{PraatlyApp, SaveFormat};

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
            let rec_lbl = if app.recording { "⏹ Stop recording" } else { "⏺ Record" };

            let rec_btn = egui::Button::new(rec_lbl);
            let rec_btn = if app.recording {
                rec_btn.fill(egui::Color32::from_rgb(160, 40, 40))
            } else {
                rec_btn
            };

            if ui.add(rec_btn).clicked() {
                if app.recording {
                    // Stop recording
                    app.recording = false;
                    app.record_start = None;

                    // Offer save dialog immediately
                    if !app.recorded_samples.is_empty() {
                        let (filter_name, ext, file_name) = match app.save_format {
                            SaveFormat::Wav => ("WAV audio", vec!["wav"], "recording.wav"),
                            SaveFormat::Mp3 => ("MP3 audio", vec!["mp3"], "recording.mp3"),
                        };
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(filter_name, &ext)
                            .set_file_name(file_name)
                            .save_file()
                        {
                            match app.save_format {
                                SaveFormat::Wav => app.save_recording_wav(path),
                                SaveFormat::Mp3 => app.save_recording_mp3(path),
                            }
                        }
                    }
                } else {
                    // Start recording — clear previous buffer
                    app.recorded_samples.clear();
                    app.recording = true;
                    app.record_start = Some(std::time::Instant::now());
                    app.save_status = None;
                    // TODO: wire up the actual cpal input stream in audio/player.rs
                    // so samples get pushed into recorded_samples via Arc<Mutex<>>
                    // (I will do this. I'm going to do this. I'm en route to doing this!)
                    log::info!("Recording started");
                }
            }

            // Timer — shown while recording
            if app.recording {
                if let Some(start) = app.record_start {
                    let elapsed = start.elapsed().as_secs();
                    let mm = elapsed / 60;
                    let ss = elapsed % 60;
                    ui.label(
                        egui::RichText::new(format!("  {:02}:{:02}", mm, ss))
                            .color(egui::Color32::from_rgb(255, 90, 90))
                            .monospace(),
                    );
                    ctx.request_repaint(); // keep the clock ticking
                }
            }

            // Format selector — always visible so user can pick before recording
            egui::ComboBox::from_id_source("save_format")
                .selected_text(match app.save_format {
                    SaveFormat::Wav => "WAV",
                    SaveFormat::Mp3 => "MP3",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.save_format, SaveFormat::Wav, "WAV");
                    ui.selectable_value(&mut app.save_format, SaveFormat::Mp3, "MP3");
                });

            // Save button — available after recording without re-recording
            if !app.recorded_samples.is_empty() && !app.recording {
                if ui.button("💾 Save…").clicked() {
                    let (filter_name, ext, file_name) = match app.save_format {
                        SaveFormat::Wav => ("WAV audio", vec!["wav"], "recording.wav"),
                        SaveFormat::Mp3 => ("MP3 audio", vec!["mp3"], "recording.mp3"),
                    };
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(filter_name, &ext)
                        .set_file_name(file_name)
                        .save_file()
                    {
                        match app.save_format {
                            SaveFormat::Wav => app.save_recording_wav(path),
                            SaveFormat::Mp3 => app.save_recording_mp3(path),
                        }
                    }
                }
            }

            // Status message (appears real fast after save attempt)
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
