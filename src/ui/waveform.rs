use eframe::egui;
use crate::app::PraatlyApp;

pub fn show(ui: &mut egui::Ui, app: &mut PraatlyApp, height: f32) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(16, 16, 22));

    let Some(buf) = &app.buffer else {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
            "Open a file to begin", egui::FontId::default(), egui::Color32::GRAY);
        return;
    };

    let mono = buf.slice_mono(app.view_start, app.view_end);
    if mono.is_empty() { return; }

    // one pixel column = one chunk of samples, drawn as a min/max vertical line.
    // lossy? yes. fast? also yes. this is a workbench not an oscilloscope. woot.
    let width = rect.width() as usize;
    let chunk  = (mono.len() / width).max(1);
    let mid_y  = rect.center().y;
    let scale  = rect.height() * 0.44; // 0.44 — leaves breathing room at top and bottom. lol
    let color  = egui::Color32::from_rgb(72, 152, 210);

    for (i, ch) in mono.chunks(chunk).enumerate() {
        let x   = rect.left() + i as f32 * rect.width() / (mono.len() / chunk) as f32;
        let min = ch.iter().cloned().fold(f32::INFINITY,     f32::min);
        let max = ch.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        painter.line_segment(
            [egui::pos2(x, mid_y - max * scale), egui::pos2(x, mid_y - min * scale)],
            egui::Stroke::new(1.0, color),
        );
    }

    // Selection overlay
    if let Some((s, e)) = app.selection {
        let dur = app.view_end - app.view_start;
        let x0  = rect.left() + ((s - app.view_start) / dur) as f32 * rect.width();
        let x1  = rect.left() + ((e - app.view_start) / dur) as f32 * rect.width();
        painter.rect_filled(
            egui::Rect::from_x_y_ranges(x0..=x1, rect.y_range()),
            0.0, egui::Color32::from_rgba_unmultiplied(72, 152, 210, 45),
        );
    }

    // drag to select: first drag sets the anchor, subsequent motion extends from it.
    // the .min/.max at the end quietly handles dragging leftward without complaint.
    // guarding against an edge case that shouldn't exist but keeps existing anyway heh
    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let t = (app.view_start + (pos.x - rect.left()) as f64
                     / rect.width() as f64 * (app.view_end - app.view_start))
                    .clamp(app.view_start, app.view_end);
            match app.selection {
                None         => app.selection = Some((t, t)),
                Some((s, _)) => app.selection = Some((s.min(t), s.max(t))),
            }
        }
    }
    // double-click to clear — simple, obvious, works
    if response.double_clicked() { app.selection = None; }
}
