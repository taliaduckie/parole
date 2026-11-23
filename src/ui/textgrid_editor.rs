use std::path::Path;

use eframe::egui;

use crate::{annotation::textgrid::Tier, app::PraatlyApp};

pub fn show(ctx: &egui::Context, app: &mut PraatlyApp) {
    let Some(path) = app.textgrid_path.as_ref() else { return; };
    let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned();
    let mut open = true;
    let mut changed = false;
    let mut save_copy = false;
    let mut close_grid = false;
    let duration = app.buffer_duration();
    egui::Window::new("TextGrid annotations")
        .open(&mut open)
        .default_width(475.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(name);
                if app.textgrid_dirty {
                    ui.strong("Unsaved edits");
                }
            });
            ui.horizontal(|ui| {
                save_copy = ui.button("Save TextGrid copy…").on_hover_text(
                    "Choose a new file name. Existing files are never overwritten."
                ).clicked();
                if ui.button("Close TextGrid").clicked() {
                    if app.textgrid_dirty {
                        app.ui.confirm_discard_textgrid = true;
                    } else {
                        close_grid = true;
                    }
                }
            });
            if duration.is_some_and(|audio_end| app.textgrid.xmin < -0.01 || app.textgrid.xmax > audio_end + 0.01) {
                ui.label("The TextGrid extends beyond the audio timeline. Check that the files correspond.");
            }
            if app.ui.confirm_discard_textgrid {
                ui.horizontal(|ui| {
                    ui.strong("Discard unsaved label edits?");
                    if ui.button("Discard and close").clicked() {
                        close_grid = true;
                    }
                    if ui.button("Keep editing").clicked() {
                        app.ui.confirm_discard_textgrid = false;
                    }
                });
            }
            ui.label("Times are read-only. Edit labels below.");
            egui::ScrollArea::vertical().max_height(395.0).show(ui, |ui| {
                if app.textgrid.tiers.is_empty() {
                    ui.label("This TextGrid contains no tiers.");
                }
                for (index, tier) in app.textgrid.tiers.iter_mut().enumerate() {
                    let kind = match tier {
                        Tier::Interval { .. } => "Intervals",
                        Tier::Point { .. } => "Points",
                    };
                    let title = format!("{} ({kind})", tier.name());
                    ui.push_id(index, |ui| {
                        egui::CollapsingHeader::new(title).default_open(index == 0).show(ui, |ui| {
                            match tier {
                                Tier::Interval { intervals, .. } => {
                                    for (i, interval) in intervals.iter_mut().enumerate() {
                                        ui.push_id(i, |ui| ui.horizontal(|ui| {
                                            ui.monospace(format!("{:.3}–{:.3} s", interval.start, interval.end));
                                            changed |= label_field(ui, &mut interval.label).changed();
                                        }));
                                    }
                                }
                                Tier::Point { points, .. } => {
                                    for (i, point) in points.iter_mut().enumerate() {
                                        ui.push_id(i, |ui| ui.horizontal(|ui| {
                                            ui.monospace(format!("{:.3} s", point.time));
                                            changed |= label_field(ui, &mut point.label).changed();
                                        }));
                                    }
                                }
                            }
                        });
                    });
                }
            });
        });
    app.textgrid_dirty |= changed;
    app.ui.show_textgrid_editor = open;
    if close_grid {
        app.close_textgrid();
        return;
    }
    if save_copy {
        let stem = path.file_stem().unwrap_or(path.as_os_str()).to_string_lossy();
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Praat TextGrid", &["TextGrid", "textgrid"])
            .set_file_name(format!("{stem}-edited.TextGrid"));
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            dialog = dialog.set_directory(parent);
        }
        if let Some(path) = dialog.save_file() {
            let path = if path.extension().is_none() { path.with_extension("TextGrid") } else { path };
            app.save_textgrid_copy(path);
        }
    }
}

pub fn open_dialog(app: &mut PraatlyApp) {
    let mut dialog = rfd::FileDialog::new()
        .add_filter("Praat TextGrid", &["TextGrid", "textgrid"]);
    if let Some(parent) = app.audio_path.as_deref().and_then(Path::parent).filter(|p| !p.as_os_str().is_empty()) {
        dialog = dialog.set_directory(parent);
    }
    if let Some(path) = dialog.pick_file() {
        app.open_textgrid(path);
    }
}

pub(crate) fn label_field(ui: &mut egui::Ui, label: &mut String) -> egui::Response {
    ui.add(egui::TextEdit::multiline(label).desired_rows(1))
}
