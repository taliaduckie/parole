//! Top-level application state and egui update loop.

use eframe::egui;
use std::path::PathBuf;

use crate::audio::{loader::AudioBuffer, player::AudioPlayer};
use crate::dsp::{spectrogram::SpectrogramData, pitch::PitchTrack, formants::FormantTrack};
use crate::annotation::textgrid::TextGrid;

pub struct PraatlyApp {
    pub buffer:      Option<AudioBuffer>,
    pub spectrogram: Option<SpectrogramData>,
    pub pitch:       Option<PitchTrack>,
    pub formants:    Option<FormantTrack>,
    pub textgrid:    TextGrid,
    pub player:      AudioPlayer,
    pub view_start:  f64,
    pub view_end:    f64,
    pub selection:   Option<(f64, f64)>,
    pub show_spectrogram: bool,
    pub show_pitch:       bool,
    pub show_formants:    bool,
    pub show_textgrid:    bool,
}

impl PraatlyApp {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = egui::Rounding::same(8.0);
        cc.egui_ctx.set_visuals(visuals);

        let mut app = Self {
            buffer: None, spectrogram: None, pitch: None, formants: None,
            textgrid: TextGrid::default(),
            player: AudioPlayer::new(),
            view_start: 0.0, view_end: 5.0,
            selection: None,
            show_spectrogram: true, show_pitch: true,
            show_formants: true,   show_textgrid: true,
        };

        if let Some(p) = path { app.load_file(p); }
        app
    }

    pub fn load_file(&mut self, path: PathBuf) {
        match crate::audio::loader::load_audio(&path) {
            Ok(buf) => {
                self.view_end    = buf.duration_secs();
                self.spectrogram = Some(crate::dsp::spectrogram::compute(&buf, 1024, 0.75));
                self.pitch       = Some(crate::dsp::pitch::extract(&buf));
                self.formants    = Some(crate::dsp::formants::extract(&buf));
                self.buffer      = Some(buf);
            }
            Err(e) => log::error!("Failed to load {:?}: {}", path, e),
        }
    }
}

impl eframe::App for PraatlyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::ui::toolbar::show(ctx, self);

        egui::CentralPanel::default().show(ctx, |ui| {
            let h = ui.available_height();
            crate::ui::waveform::show(ui, self, h * 0.25);
            if self.show_spectrogram {
                crate::ui::spectrogram::show(ui, self, h * 0.45);
            }
            if self.show_textgrid {
                crate::annotation::textgrid::show(ui, self, h * 0.30);
            }
        });
    }
}
