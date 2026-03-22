use eframe::egui;
use crate::app::PraatlyApp;

// viridis colormap approximated by polynomial regression.
// I did not derive these coefficients. I adapted them from the internet.
// I understood them enough to be dangerous!!! Heheheheh.
fn viridis(t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let r = (0.267 + 0.003*t + 1.785*t*t - 2.229*t*t*t).clamp(0.0,1.0);
    let g = (0.005 + 1.698*t - 1.318*t*t).clamp(0.0,1.0);
    let b = (0.329 + 1.498*t - 2.950*t*t + 1.926*t*t*t).clamp(0.0,1.0);
    egui::Color32::from_rgb((r*255.0) as u8, (g*255.0) as u8, (b*255.0) as u8)
}

pub fn show(ui: &mut egui::Ui, app: &mut PraatlyApp, height: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(10, 10, 16));

    let Some(spec) = &app.spectrogram else { return; };
    let n_frames = spec.n_frames();
    let n_bins   = spec.n_bins();
    if n_frames == 0 { return; }

    // global max so we can normalise per-cell magnitude — avoids the whole thing
    // going dark because one frame is extremely loud. been there.
    let global_max = spec.magnitudes.iter().flatten().cloned().fold(0.0f32, f32::max);
    if global_max == 0.0 { return; }

    let cw = rect.width()  / n_frames as f32;
    let ch = rect.height() / n_bins   as f32;

    for (fi, frame) in spec.magnitudes.iter().enumerate() {
        let x = rect.left() + fi as f32 * cw;
        for (bi, &mag) in frame.iter().enumerate() {
            let y    = rect.bottom() - (bi + 1) as f32 * ch;
            // log10 scale to bring out the quiet detail — the 9.0 is not arbitrary just indefensible lol
            let norm = (1.0 + mag / global_max * 9.0).log10();
            painter.rect_filled(
                // +0.5 on cell size plugs sub-pixel gaps between adjacent cells.
                // tiny hack, surprisingly big difference. teehee.
                egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(cw + 0.5, ch + 0.5)),
                0.0, viridis(norm),
            );
        }
    }

    // Pitch overlay — yellow dots over the spectrogram, one per voiced frame
    if app.show_pitch {
        if let Some(pitch) = &app.pitch {
            let dur    = app.view_end - app.view_start;
            let max_hz = 600.0f32; // matches the extraction ceiling — they agreed on this without talking
            for (i, f0) in pitch.frames.iter().enumerate() {
                let Some(hz) = f0 else { continue }; // unvoiced frame: skip silently, as nature intended
                let t = pitch.frame_to_sec(i);
                if t < app.view_start || t > app.view_end { continue; }
                let x = rect.left() + ((t - app.view_start) / dur) as f32 * rect.width();
                let y = rect.bottom() - (hz / max_hz) * rect.height();
                painter.circle_filled(egui::pos2(x, y), 1.5, egui::Color32::YELLOW);
            }
        }
    }
}
