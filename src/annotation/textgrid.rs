use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interval { pub start: f64, pub end: f64, pub label: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point { pub time: f64, pub label: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tier {
    Interval { name: String, intervals: Vec<Interval> },
    Point    { name: String, points:    Vec<Point>    },
}

impl Tier {
    pub fn name(&self) -> &str {
        match self { Tier::Interval { name, .. } | Tier::Point { name, .. } => name }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TextGrid { pub tiers: Vec<Tier> }

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

    let dur    = app.view_end - app.view_start;
    let tier_h = height / app.textgrid.tiers.len() as f32;

    for (ti, tier) in app.textgrid.tiers.iter().enumerate() {
        let ty = rect.top() + ti as f32 * tier_h;
        painter.text(egui::pos2(rect.left() + 4.0, ty + 8.0), egui::Align2::LEFT_TOP,
            tier.name(), egui::FontId::monospace(11.0), egui::Color32::GRAY);

        if let Tier::Interval { intervals, .. } = tier {
            for iv in intervals {
                if iv.end < app.view_start || iv.start > app.view_end { continue; }
                let x0 = rect.left() + ((iv.start - app.view_start) / dur) as f32 * rect.width();
                let x1 = rect.left() + ((iv.end   - app.view_start) / dur) as f32 * rect.width();
                let r  = egui::Rect::from_min_max(egui::pos2(x0, ty+18.0), egui::pos2(x1, ty+tier_h-2.0));
                painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(70,70,110)));
                painter.text(r.center(), egui::Align2::CENTER_CENTER,
                    &iv.label, egui::FontId::default(), egui::Color32::WHITE);
            }
        }
    }
}
