//! DSP parameter panel — Praat's analysis dialogs reimagined as one floating
//! window. Changes don't take effect until you hit Apply, because dragging a
//! slider would otherwise spawn a worker every frame and we don't have
//! cancellation yet (pls do not make me explain this to your CPU)

use eframe::egui;
use crate::app::PraatlyApp;

pub fn show(ctx: &egui::Context, app: &mut PraatlyApp) {
    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(true)
        .default_width(360.0)
        .default_pos([120.0, 120.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Analysis settings");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✕ Close").clicked() {
                        app.show_settings = false;
                    }
                });
            });
            ui.add_space(6.0);

            spectrogram_section(ui, ctx, app);
            ui.add_space(8.0);
            pitch_section(ui, ctx, app);
            ui.add_space(8.0);
            formant_section(ui, ctx, app);
        });
}

fn spectrogram_section(ui: &mut egui::Ui, ctx: &egui::Context, app: &mut PraatlyApp) {
    section_heading(ui, "Spectrogram");
    let mut s = app.spec_settings;

    ui.horizontal(|ui| {
        ui.label("Window size:");
        // Snap to power-of-2 sizes — the rest just confuses the FFT planner.
        for size in [256, 512, 1024, 2048, 4096] {
            ui.selectable_value(&mut s.window_size, size, format!("{}", size));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Overlap:");
        ui.add(egui::Slider::new(&mut s.overlap, 0.0..=0.95).fixed_decimals(2));
    });

    app.spec_settings = s;

    if ui.button("Apply").clicked() {
        app.respawn_spectrogram(ctx);
    }
}

fn pitch_section(ui: &mut egui::Ui, ctx: &egui::Context, app: &mut PraatlyApp) {
    section_heading(ui, "Pitch (F0)");
    let mut s = app.pitch_settings;

    ui.horizontal(|ui| {
        ui.label("Min Hz:");
        ui.add(egui::Slider::new(&mut s.min_hz, 30.0..=300.0).integer());
    });
    ui.horizontal(|ui| {
        ui.label("Max Hz:");
        ui.add(egui::Slider::new(&mut s.max_hz, 100.0..=2000.0).integer());
    });
    ui.horizontal(|ui| {
        ui.label("Voicing threshold:");
        ui.add(egui::Slider::new(&mut s.voicing_threshold, 0.1..=0.9).fixed_decimals(2));
    });

    // Keep min < max; the extractor clamps too but the slider experience is
    // less surprising if we enforce it here.
    if s.min_hz >= s.max_hz {
        s.max_hz = (s.min_hz + 10.0).min(2000.0);
    }
    app.pitch_settings = s;

    if ui.button("Apply").clicked() {
        app.respawn_pitch(ctx);
    }
}

fn formant_section(ui: &mut egui::Ui, ctx: &egui::Context, app: &mut PraatlyApp) {
    section_heading(ui, "Formants");
    let mut s = app.formant_settings;

    ui.horizontal(|ui| {
        ui.label("Max formant Hz:");
        ui.add(egui::Slider::new(&mut s.max_formant_hz, 1500.0..=8000.0).integer());
    });

    app.formant_settings = s;

    if ui.button("Apply").clicked() {
        app.respawn_formants(ctx);
    }
}

fn section_heading(ui: &mut egui::Ui, label: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(label).strong().size(14.0));
    ui.separator();
}
