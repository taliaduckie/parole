use eframe::egui;
use crate::app::{PraatlyApp, SaveFormat, StatusKind};

pub fn show(ctx: &egui::Context, app: &mut PraatlyApp) {
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            file_section(ui, ctx, app);
            ui.separator();
            playback_section(ui, ctx, app);
            ui.separator();
            recording_section(ui, app);
            ui.separator();
            view_toggles(ui, app);
            ui.separator();
            panel_toggles(ui, app);
            status_label(ui, app);
        });
    });
}

fn file_section(ui: &mut egui::Ui, ctx: &egui::Context, app: &mut PraatlyApp) {
    if ui.button("Open…").clicked() {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "flac", "mp3", "aiff"])
            .pick_file()
        { app.load_file(path, ctx); }
    }
}

fn playback_section(ui: &mut egui::Ui, ctx: &egui::Context, app: &mut PraatlyApp) {
    let is_playing = app.player.is_playing();
    let play_lbl = if is_playing { "⏹ Stop" } else { "▶ Play" };
    if ui.button(play_lbl).clicked() {
        if is_playing {
            app.player.stop();
        } else if let Some(buf) = &app.buffer {
            let (s, e) = app.view.selection.unwrap_or((app.view.start, app.view.end));
            if let Err(err) = app.player.play(buf.slice_mono(s, e), buf.sample_rate) {
                app.ui.error(format!("Playback failed: {}", err));
                log::error!("Playback failed: {}", err);
            }
        }
    }
    if let Some(err) = app.player.take_runtime_error() {
        app.ui.error(err);
    }
    // Repaint while playing so the Play→Stop label flips on its own when
    // the audio thread reaches end-of-buffer.
    if is_playing {
        ctx.request_repaint();
    }
}

fn recording_section(ui: &mut egui::Ui, app: &mut PraatlyApp) {
    let recording = app.recording.recorder.is_recording();
    let rec_lbl = if recording { "⏹ Stop recording" } else { "⏺ Record" };

    let rec_btn = egui::Button::new(rec_lbl);
    let rec_btn = if recording {
        rec_btn.fill(egui::Color32::from_rgb(160, 40, 40))
    } else {
        rec_btn
    };

    if ui.add(rec_btn).clicked() {
        if recording {
            stop_recording_and_offer_save(app);
        } else {
            start_recording(app);
        }
    }

    // Surface any error reported asynchronously by the cpal callback.
    if let Some(err) = app.recording.recorder.take_runtime_error() {
        app.ui.error(err);
    }

    // Timer — shown while recording
    if recording {
        if let Some(start) = app.recording.started_at {
            let elapsed = start.elapsed().as_secs();
            let mm = elapsed / 60;
            let ss = elapsed % 60;
            ui.label(
                egui::RichText::new(format!("  {:02}:{:02}", mm, ss))
                    .color(egui::Color32::from_rgb(255, 90, 90))
                    .monospace(),
            );
            ui.ctx().request_repaint(); // keep the clock ticking
        }
    }

    // Format selector — always visible so user can pick before recording
    egui::ComboBox::from_id_source("save_format")
        .selected_text(match app.recording.save_format {
            SaveFormat::Wav => "WAV",
            SaveFormat::Mp3 => "MP3",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut app.recording.save_format, SaveFormat::Wav, "WAV");
            ui.selectable_value(&mut app.recording.save_format, SaveFormat::Mp3, "MP3");
        });

    // Save button — available after recording without re-recording
    if !app.recording.samples.is_empty() && !recording {
        if ui.button("💾 Save…").clicked() {
            offer_save_dialog(app);
        }
    }
}

fn start_recording(app: &mut PraatlyApp) {
    app.recording.samples.clear();
    app.ui.status = None;
    match app.recording.recorder.start() {
        Ok(()) => {
            app.recording.sample_rate = app.recording.recorder.sample_rate;
            app.recording.started_at = Some(std::time::Instant::now());
            log::info!("Recording started @ {}Hz", app.recording.sample_rate);
        }
        Err(e) => {
            app.ui.error(format!("Recording failed: {}", e));
            log::error!("Recording failed: {}", e);
        }
    }
}

fn stop_recording_and_offer_save(app: &mut PraatlyApp) {
    app.recording.samples = app.recording.recorder.stop();
    app.recording.sample_rate = app.recording.recorder.sample_rate;
    app.recording.started_at = None;

    if app.recording.samples.is_empty() {
        app.ui.info("Nothing recorded.");
        return;
    }
    offer_save_dialog(app);
}

/// Shared "where would you like to save this?" → call the right encoder.
/// Used both by the auto-prompt-after-stop path and the manual Save button.
fn offer_save_dialog(app: &mut PraatlyApp) {
    let (filter_name, ext, file_name) = match app.recording.save_format {
        SaveFormat::Wav => ("WAV audio", vec!["wav"], "recording.wav"),
        SaveFormat::Mp3 => ("MP3 audio", vec!["mp3"], "recording.mp3"),
    };
    if let Some(path) = rfd::FileDialog::new()
        .add_filter(filter_name, &ext)
        .set_file_name(file_name)
        .save_file()
    {
        match app.recording.save_format {
            SaveFormat::Wav => app.save_recording_wav(path),
            SaveFormat::Mp3 => app.save_recording_mp3(path),
        }
    }
}

fn view_toggles(ui: &mut egui::Ui, app: &mut PraatlyApp) {
    ui.label("Show:");
    ui.checkbox(&mut app.view.show_spectrogram, "Spectrogram");
    ui.checkbox(&mut app.view.show_pitch,       "Pitch");
    ui.checkbox(&mut app.view.show_formants,    "Formants");
    ui.checkbox(&mut app.view.show_textgrid,    "TextGrid");
}

fn panel_toggles(ui: &mut egui::Ui, app: &mut PraatlyApp) {
    let settings_lbl = if app.ui.show_settings { "✕ Settings" } else { "⚙ Settings" };
    if ui.button(settings_lbl).clicked() {
        app.ui.show_settings = !app.ui.show_settings;
    }
    let help_lbl = if app.ui.show_help { "✕ Help" } else { "? Help" };
    if ui.button(help_lbl).clicked() {
        app.ui.show_help = !app.ui.show_help;
    }
}

fn status_label(ui: &mut egui::Ui, app: &PraatlyApp) {
    if let Some(status) = &app.ui.status {
        ui.separator();
        let color = match status.kind {
            StatusKind::Info    => egui::Color32::from_rgb(180, 180, 200),
            StatusKind::Success => egui::Color32::from_rgb(140, 200, 140),
            StatusKind::Error   => egui::Color32::from_rgb(230, 120, 120),
        };
        ui.label(
            egui::RichText::new(&status.text)
                .color(color)
                .small()
        );
    }
}
