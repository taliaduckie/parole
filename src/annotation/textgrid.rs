use eframe::egui;
use serde::{Deserialize, Serialize};

mod praat;
pub use praat::TextGridError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interval { pub start: f64, pub end: f64, pub label: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point { pub time: f64, pub label: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Tier {
    Interval { name: String, xmin: f64, xmax: f64, intervals: Vec<Interval> },
    Point    { name: String, xmin: f64, xmax: f64, points:    Vec<Point>    },
}

impl Tier {
    pub fn name(&self) -> &str {
        match self { Tier::Interval { name, .. } | Tier::Point { name, .. } => name }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextGrid { pub xmin: f64, pub xmax: f64, pub tiers: Vec<Tier> }

pub fn show(ui: &mut egui::Ui, app: &mut crate::app::PraatlyApp, height: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height), egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 28));

    if app.textgrid.tiers.is_empty() {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
            "No annotation tiers", egui::FontId::default(), egui::Color32::DARK_GRAY);
        return;
    }

    let dur    = app.view.end - app.view.start;
    if !dur.is_finite() || dur <= 0.0 {
        return;
    }
    let tier_h = height / app.textgrid.tiers.len() as f32;
    let x = |time: f64| rect.left() + ((time - app.view.start) / dur) as f32 * rect.width();

    for (ti, tier) in app.textgrid.tiers.iter().enumerate() {
        let ty = rect.top() + ti as f32 * tier_h;
        painter.text(egui::pos2(rect.left() + 4.0, ty + 8.0), egui::Align2::LEFT_TOP,
            tier.name(), egui::FontId::monospace(11.0), egui::Color32::GRAY);

        let row = egui::Rect::from_min_max(
            egui::pos2(rect.left(), ty),
            egui::pos2(rect.right(), ty + tier_h),
        );
        let row_painter = painter.with_clip_rect(row);
        if tier_h <= 20.0 { continue; }

        match tier {
            Tier::Interval { intervals, .. } => {
                for interval in intervals {
                    if interval.end < app.view.start || interval.start > app.view.end { continue; }
                    let r = egui::Rect::from_min_max(
                        egui::pos2(x(interval.start.max(app.view.start)), ty + 18.0),
                        egui::pos2(x(interval.end.min(app.view.end)), ty + tier_h - 2.0),
                    );
                    let clipped = row_painter.with_clip_rect(r);
                    row_painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(70, 70, 110)));
                    clipped.text(r.center(), egui::Align2::CENTER_CENTER, &interval.label,
                        egui::FontId::default(), egui::Color32::WHITE);
                }
            }
            Tier::Point { points, .. } => {
                for point in points {
                    if point.time < app.view.start || point.time > app.view.end { continue; }
                    let px = x(point.time);
                    let top = ty + 18.0;
                    let bottom = ty + tier_h - 2.0;
                    row_painter.line_segment(
                        [egui::pos2(px, top), egui::pos2(px, bottom)],
                        egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_BLUE),
                    );
                    row_painter.text(egui::pos2(px, (top + bottom) * 0.5),
                        egui::Align2::CENTER_CENTER, &point.label,
                        egui::FontId::default(), egui::Color32::WHITE);
                }
            }
        }
    }
}
