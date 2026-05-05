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

/// Map the visible time window [view_start, view_end] to the (u0, u1) range
/// of the spectrogram texture. Returns None when the window doesn't overlap
/// any actual spectrogram data (start past the end, or zero-width view).
pub(crate) fn view_uv(view_start: f64, view_end: f64, total_spec_dur: f64) -> Option<(f32, f32)> {
    if total_spec_dur <= 0.0 || view_end <= view_start {
        return None;
    }
    let u0 = (view_start / total_spec_dur).clamp(0.0, 1.0) as f32;
    let u1 = (view_end   / total_spec_dur).clamp(0.0, 1.0) as f32;
    if u1 <= u0 { return None; } // entirely past the data, or numerical pinch
    Some((u0, u1))
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
        // Total time covered by the (computed) spectrogram. Always ≤ buffer
        // duration because the last partial window gets dropped during compute.
        let total_spec_dur =
            spec.n_frames() as f64 * spec.hop_size as f64 / spec.sample_rate as f64;
        if let Some((u0, u1)) = view_uv(app.view_start, app.view_end, total_spec_dur) {
            painter.image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(u0, 0.0), egui::pos2(u1, 1.0)),
                egui::Color32::WHITE,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(magnitudes: Vec<Vec<f32>>, n_fft: usize, sample_rate: u32) -> SpectrogramData {
        SpectrogramData { magnitudes, n_fft, hop_size: 256, sample_rate }
    }

    #[test]
    fn build_image_dimensions_match_spectrogram() {
        let s = spec(vec![vec![0.0; 5]; 7], 8, 16000); // 7 frames × 5 bins
        let img = build_image(&s);
        assert_eq!(img.size, [7, 5]);
        assert_eq!(img.pixels.len(), 35);
    }

    #[test]
    fn build_image_silence_produces_uniform_dark_pixels() {
        let s = spec(vec![vec![0.0; 4]; 3], 6, 16000);
        let img = build_image(&s);
        // All zeros → norm = log10(1) = 0 → viridis(0).
        let expected = viridis(0.0);
        for px in &img.pixels { assert_eq!(*px, expected); }
    }

    #[test]
    fn build_image_flips_y_so_bin0_lands_at_bottom_row() {
        // Single frame with a peak only in bin 0 (DC). After the y-flip,
        // the brightest pixel should be at the bottom row of the image.
        let s = spec(vec![vec![1.0, 0.0, 0.0, 0.0]], 6, 16000);
        let img = build_image(&s);
        let n_frames = img.size[0];
        let n_bins = img.size[1];
        assert_eq!(n_frames, 1);
        assert_eq!(n_bins, 4);
        // Bottom row index = n_bins - 1; only column = 0.
        let bottom = img.pixels[(n_bins - 1) * n_frames];
        // The 1.0 magnitude at bin 0 should map to the brightest viridis output.
        let bright = viridis(1.0);
        assert_eq!(bottom, bright);
        // Top row (bin 3, all zeros) should be the silence color.
        let top = img.pixels[0];
        assert_eq!(top, viridis(0.0));
    }

    #[test]
    fn build_image_handles_empty_spectrogram() {
        let s = spec(vec![], 8, 16000);
        let img = build_image(&s);
        // n_frames = 0, n_bins = 5 → 0×5 = 0 pixels. No panic.
        assert_eq!(img.size[0], 0);
        assert!(img.pixels.is_empty());
    }

    #[test]
    fn viridis_endpoints_differ() {
        let dark = viridis(0.0);
        let bright = viridis(1.0);
        assert_ne!(dark, bright);
    }

    #[test]
    fn view_uv_full_window_is_full_uv() {
        let uv = view_uv(0.0, 5.0, 5.0).unwrap();
        assert!((uv.0 - 0.0).abs() < 1e-6);
        assert!((uv.1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn view_uv_half_window_is_first_half_of_uv() {
        let uv = view_uv(0.0, 2.5, 5.0).unwrap();
        assert!((uv.0 - 0.0).abs() < 1e-6);
        assert!((uv.1 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn view_uv_offset_window_maps_to_offset_uv() {
        // 1s..3s of a 4s spec → u in [0.25, 0.75]
        let uv = view_uv(1.0, 3.0, 4.0).unwrap();
        assert!((uv.0 - 0.25).abs() < 1e-6);
        assert!((uv.1 - 0.75).abs() < 1e-6);
    }

    #[test]
    fn view_uv_clamps_window_past_end_of_spectrogram() {
        // view_end exceeds total duration (last partial window dropped during compute);
        // u1 should clamp at 1.0 instead of overshooting.
        let uv = view_uv(0.0, 5.5, 5.0).unwrap();
        assert!((uv.1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn view_uv_returns_none_for_zero_width_window() {
        assert!(view_uv(2.0, 2.0, 5.0).is_none());
    }

    #[test]
    fn view_uv_returns_none_when_window_starts_past_data() {
        // view_start > total duration → entirely off the right edge.
        assert!(view_uv(10.0, 12.0, 5.0).is_none());
    }

    #[test]
    fn view_uv_returns_none_for_empty_spectrogram() {
        assert!(view_uv(0.0, 5.0, 0.0).is_none());
    }

    #[test]
    fn build_image_normalises_to_global_max() {
        // Two frames: [0.5, 0.0] and [0.0, 0.5]. Both peaks are equal so they
        // should map to the same color after global-max normalisation.
        // n_fft = 2 → n_bins() = 2, matching the frame data above.
        let s = spec(vec![vec![0.5, 0.0], vec![0.0, 0.5]], 2, 16000);
        let img = build_image(&s);
        // n_frames = 2, n_bins = 2. Pixel (frame=0, bin=0) is at row=1, col=0;
        // pixel (frame=1, bin=1) is at row=0, col=1.
        let p_lo = img.pixels[1 * 2 + 0];
        let p_hi = img.pixels[0 * 2 + 1];
        assert_eq!(p_lo, p_hi);
    }
}
