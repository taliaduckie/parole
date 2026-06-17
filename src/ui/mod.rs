pub mod waveform;
pub mod spectrogram;
pub mod toolbar;
pub mod help;
pub mod settings;

use eframe::egui;

/// Draw a vertical playhead line at file-relative time `t`, clipped to the
/// visible time window [view_start, view_end]. No-op if the playhead is
/// outside the window or the window has zero width
pub fn paint_playhead(
    painter: &egui::Painter,
    rect: egui::Rect,
    view_start: f64,
    view_end: f64,
    t: f64,
) {
    let dur = view_end - view_start;
    if dur <= 0.0 || t < view_start || t > view_end { return; }
    let x = rect.left() + ((t - view_start) / dur) as f32 * rect.width();
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 230, 200)),
    );
}
