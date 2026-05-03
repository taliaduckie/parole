use eframe::egui;
use crate::app::PraatlyApp;
use crate::dsp::spectrogram::SpectrogramData;

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

/// Bake the spectrogram into a single ColorImage we can hand to the GPU.
/// log10 scale + global-max normalisation + viridis, applied once per
/// spectrogram (instead of once per frame *per paint* — the old hot path
/// painted ~n_frames × n_bins rects every redraw, which scaled extremely badly).
pub(crate) fn build_image(spec: &SpectrogramData) -> egui::ColorImage {
    let n_frames = spec.n_frames();
    let n_bins   = spec.n_bins();
    // .max(1e-12) so all-silence input doesn't divide by zero — the resulting
    // norm values clamp to 0.0 and we paint a uniform dark blue, as nature intended.
    let global_max = spec.magnitudes.iter().flatten().cloned()
        .fold(0.0_f32, f32::max).max(1e-12);

    // ColorImage is row-major with row 0 at the top. The spectrogram thinks
    // bottom-up (bin 0 = DC at the bottom), so we flip rows on the way in.
    let mut pixels = vec![egui::Color32::BLACK; n_frames.saturating_mul(n_bins)];
    for (fi, frame) in spec.magnitudes.iter().enumerate() {
        for (bi, &mag) in frame.iter().enumerate() {
            // log10 to bring out the quiet detail — the 9.0 is not arbitrary just indefensible lol
            let norm = (1.0 + mag / global_max * 9.0).log10();
            let row = n_bins - 1 - bi;
            pixels[row * n_frames + fi] = viridis(norm);
        }
    }

    egui::ColorImage { size: [n_frames, n_bins], pixels }
}

pub fn show(ui: &mut egui::Ui, app: &mut PraatlyApp, height: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(10, 10, 16));

    let Some(spec) = &app.spectrogram else { return; };
    let n_frames = spec.n_frames();
    if n_frames == 0 { return; }

    // Build the texture lazily on the first paint after a new spectrogram lands.
    // Subsequent frames are a single quad blit — finally something my GPU
    // doesn't need to be brave about.
    if app.spectrogram_texture.is_none() {
        let image = build_image(spec);
        let handle = ui.ctx().load_texture(
            "parole-spectrogram",
            image,
            egui::TextureOptions::LINEAR,
        );
        app.spectrogram_texture = Some(handle);
    }

    if let Some(tex) = &app.spectrogram_texture {
        // Full UV — paint the whole texture into the panel rect. (Zoom-aware
        // rendering would just narrow the U range here. saving that for later.)
        painter.image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
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

    // Formant overlay — F1/F2/F3 in red/green/blue, plotted on the spectrogram's
    // own y-scale (0 .. Nyquist) so they align with formant bands visually.
    if app.show_formants {
        if let Some(formants) = &app.formants {
            let dur     = app.view_end - app.view_start;
            let nyquist = spec.sample_rate as f32 / 2.0;
            let colors = [
                egui::Color32::from_rgb(255, 80, 80),   // F1
                egui::Color32::from_rgb(80, 255, 120),  // F2
                egui::Color32::from_rgb(120, 160, 255), // F3
            ];
            for (i, frame) in formants.frames.iter().enumerate() {
                let t = formants.frame_to_sec(i);
                if t < app.view_start || t > app.view_end { continue; }
                let x = rect.left() + ((t - app.view_start) / dur) as f32 * rect.width();
                for (slot, hz) in [frame.f1, frame.f2, frame.f3].iter().enumerate() {
                    let Some(hz) = hz else { continue };
                    let y = rect.bottom() - (hz / nyquist) * rect.height();
                    painter.circle_filled(egui::pos2(x, y), 1.6, colors[slot]);
                }
            }
        }
    }
}
