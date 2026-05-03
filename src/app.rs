//! Top-level application state and egui update loop.
//! this file has several jobs and handles most of them with dignity.

use eframe::egui;
use std::path::PathBuf;
use std::time::Instant;

use crate::audio::{loader::AudioBuffer, player::AudioPlayer, recorder::Recorder};
use crate::dsp::{spectrogram::SpectrogramData, pitch::PitchTrack, formants::FormantTrack};
use crate::annotation::textgrid::TextGrid;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SaveFormat {
    #[default]
    Wav,
    Mp3,
}

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

    // Help panel toggle
    pub show_help: bool,

    // Recording state
    pub recorder:           Recorder,
    pub record_start:       Option<Instant>,
    pub recorded_samples:   Vec<f32>,
    pub record_sample_rate: u32,
    pub save_format:        SaveFormat,
    pub save_status:        Option<String>, // feedback message after save attempt
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
            show_help: false,
            recorder: Recorder::new(),
            record_start: None,
            recorded_samples: Vec::new(),
            record_sample_rate: 44100,
            save_format: SaveFormat::Wav,
            save_status: None,
        };

        if let Some(p) = path { app.load_file(p); }
        app
    }

    pub fn load_file(&mut self, path: PathBuf) {
        match crate::audio::loader::load_audio(&path) {
            Ok(buf) => {
                self.view_end    = buf.duration_secs();
                // compute everything up front — a little eager but avoids lazy-loading complexity
                // that I'm not ready to take on today. woot.
                self.spectrogram = Some(crate::dsp::spectrogram::compute(&buf, 1024, 0.75));
                self.pitch       = Some(crate::dsp::pitch::extract(&buf));
                self.formants    = Some(crate::dsp::formants::extract(&buf));
                self.buffer      = Some(buf);
            }
            Err(e) => log::error!("Failed to load {:?}: {}", path, e),
        }
    }

    /// Save recorded samples to a WAV file at the given path.
    pub fn save_recording_wav(&mut self, path: PathBuf) {
        if self.recorded_samples.is_empty() {
            self.save_status = Some("Nothing recorded yet.".to_string());
            return;
        }
        self.save_status = Some(match crate::audio::encoder::write_wav_mono_f32(
            &path,
            &self.recorded_samples,
            self.record_sample_rate,
        ) {
            Ok(()) => format!(
                "Saved to {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            Err(e) => format!("WAV save failed: {}", e),
        });
    }

    /// Save recorded samples to an MP3 file at the given path.
    pub fn save_recording_mp3(&mut self, path: PathBuf) {
        if self.recorded_samples.is_empty() {
            self.save_status = Some("Nothing recorded yet.".to_string());
            return;
        }
        self.save_status = Some(match crate::audio::encoder::write_mp3_mono(
            &path,
            &self.recorded_samples,
            self.record_sample_rate,
        ) {
            Ok(()) => format!(
                "Saved to {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            Err(e) => format!("MP3 save failed: {}", e),
        });
    }
}

impl eframe::App for PraatlyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keyboard shortcut: ? toggles help
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.show_help = !self.show_help;
        }

        crate::ui::toolbar::show(ctx, self);

        // Help panel — renders as a floating window
        if self.show_help {
            crate::ui::help::show(ctx, self);
        }

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
