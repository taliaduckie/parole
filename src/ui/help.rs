/// Help panel — a floating window explaining what everything does.
///
/// Toggled by the ? Help button in the toolbar, or F1.
/// Written to be genuinely useful rather than just existing. Hi praat 
/// NO TEA NO SHADE

use eframe::egui;
use crate::app::PraatlyApp;

pub fn show(ctx: &egui::Context, app: &mut PraatlyApp) {
    egui::Window::new("Help")
        .collapsible(false)
        .resizable(true)
        .default_width(440.0)
        .default_pos([80.0, 80.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Parole");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✕ Close").clicked() {
                        app.ui.show_help = false;
                    }
                });
            });

            ui.label(
                egui::RichText::new(
                    "A modern phonetic analysis workbench. \
                     Open an audio file, analyse it, annotate it."
                ).weak()
            );

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // ── Getting started ──────────────────────────────────────────
            section(ui, "Getting started");
            row(ui, "Open a file",     "Click Open… in the toolbar, or drag a WAV/FLAC/MP3/AIFF file onto the window.");
            row(ui, "Supported formats", "WAV, FLAC, MP3, AIFF. Files are decoded to mono f32 internally.");

            ui.add_space(8.0);

            // ── Panels ───────────────────────────────────────────────────
            section(ui, "Panels");
            row(ui, "Waveform",     "The amplitude envelope of the signal over time. Always visible.");
            row(ui, "Spectrogram",  "Short-time Fourier transform. Frequency (Hz) on the Y axis, time on X. \
                                     Colour = magnitude (viridis scale: dark = quiet, yellow = loud). \
                                     Toggle with the Spectrogram checkbox.");
            row(ui, "Pitch",        "F0 (fundamental frequency) extracted via autocorrelation. \
                                     Rendered as yellow dots overlaid on the spectrogram. \
                                     Unvoiced frames are blank. Toggle with the Pitch checkbox.");
            row(ui, "Formants",     "F1/F2/F3 via LPC. Overlay on spectrogram. \
                                     (Implementation in progress — currently stubbed.)");
            row(ui, "TextGrid",     "Annotation tiers. Interval and point tiers supported. \
                                     Toggle with the TextGrid checkbox.");

            ui.add_space(8.0);

            // ── Selection ────────────────────────────────────────────────
            section(ui, "Selection & navigation");
            row(ui, "Select a region",   "Click and drag in the waveform panel.");
            row(ui, "Clear selection",   "Double-click in the waveform panel.");
            row(ui, "Play selection",    "Make a selection, then click ▶ Play. \
                                          Without a selection, plays the full view window.");

            ui.add_space(8.0);

            // ── Recording ────────────────────────────────────────────────
            section(ui, "Recording");
            row(ui, "Record",       "Click ⏺ Record to start capturing from your default microphone. \
                                     The button turns red while recording.");
            row(ui, "Stop & save",  "Click ⏺ Stop recording — a save dialog appears immediately. \
                                     Choose a location and filename to export as WAV.");
            row(ui, "Save later",   "If you dismiss the save dialog, a 💾 Save… button appears \
                                     in the toolbar until you start a new recording.");
            row(ui, "Format",       "Recordings are saved as 32-bit float mono WAV at 44100 Hz. \
                                     Convert to MP3 with ffmpeg if needed: \
                                     ffmpeg -i recording.wav -q:a 2 recording.mp3");

            ui.add_space(8.0);

            // ── Keyboard shortcuts ───────────────────────────────────────
            section(ui, "Keyboard shortcuts");
            row(ui, "F1",  "Toggle this help panel.");

            ui.add_space(8.0);

            // ── Notes ────────────────────────────────────────────────────
            section(ui, "Notes & known limitations");
            ui.label(
                egui::RichText::new(
                    "• Formant extraction (LPC root-finding) is currently stubbed — \
                       F1/F2/F3 overlays are not yet rendered.\n\
                     • TextGrid import/export (.TextGrid format) is not yet implemented."
                )
                .weak()
                .small()
            );

            ui.add_space(4.0);
        });
}

/// Render section heading
fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).strong());
    ui.add_space(2.0);
}

/// Render two-column key/value row
fn row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(format!("{key}:")).strong().small());
        ui.label(egui::RichText::new(value).small().weak());
    });
    ui.add_space(1.0);
}
