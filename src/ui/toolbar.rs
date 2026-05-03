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
                { app.load_file(path, ctx); }
            }

            ui.separator();

            // ── Playback ──────────────────────────────────────────────────
            let is_playing = app.player.is_playing();
            let play_lbl = if is_playing { "⏹ Stop" } else { "▶ Play" };
            if ui.button(play_lbl).clicked() {
                if is_playing {
                    app.player.stop();
                } else if let Some(buf) = &app.buffer {
                    let (s, e) = app.selection.unwrap_or((app.view_start, app.view_end));
                    if let Err(err) = app.player.play(buf.slice_mono(s, e), buf.sample_rate) {
                        app.save_status = Some(format!("Playback failed: {}", err));
                        log::error!("Playback failed: {}", err);
                    }
                }
            }
            if let Some(err) = app.player.take_runtime_error() {
                app.save_status = Some(err);
            }
            // Repaint while playing so the Play→Stop label flips on its own when
            // the audio thread reaches end-of-buffer.
            if is_playing {
                ctx.request_repaint();
            }

            ui.separator();

            // ── Recording ────────────────────────────────────────────────
            let recording = app.recorder.is_recording();
            let rec_lbl = if recording { "⏹ Stop recording" } else { "⏺ Record" };

            let rec_btn = egui::Button::new(rec_lbl);
            let rec_btn = if recording {
                rec_btn.fill(egui::Color32::from_rgb(160, 40, 40))
            } else {
                rec_btn
            };

            if ui.add(rec_btn).clicked() {
                if recording {
                    // Stop recording: drop the cpal stream, drain samples.
                    app.recorded_samples = app.recorder.stop();
                    app.record_sample_rate = app.recorder.sample_rate;
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
                    } else {
                        app.save_status = Some("Nothing recorded.".to_string());
                    }
                } else {
                    // Start recording
                    app.recorded_samples.clear();
                    app.save_status = None;
                    match app.recorder.start() {
                        Ok(()) => {
                            app.record_sample_rate = app.recorder.sample_rate;
                            app.record_start = Some(std::time::Instant::now());
                            log::info!("Recording started @ {}Hz", app.record_sample_rate);
                        }
                        Err(e) => {
                            app.save_status = Some(format!("Recording failed: {}", e));
                            log::error!("Recording failed: {}", e);
                        }
                    }
                }
            }

            // Surface any error reported asynchronously by the cpal callback.
            if let Some(err) = app.recorder.take_runtime_error() {
                app.save_status = Some(err);
            }

            // Timer — shown while recording
            if recording {
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
            if !app.recorded_samples.is_empty() && !recording {
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
