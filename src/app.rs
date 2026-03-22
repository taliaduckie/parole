//! Top-level application state and egui update loop.
//! this file has several jobs and handles most of them with dignity.

use eframe::egui;
use std::path::PathBuf;
use std::time::Instant;

use crate::audio::{loader::AudioBuffer, player::AudioPlayer};
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
    pub recording:          bool,
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
            recording: false,
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

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.record_sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        match hound::WavWriter::create(&path, spec) {
            Ok(mut writer) => {
                for &sample in &self.recorded_samples {
                    if let Err(e) = writer.write_sample(sample) {
                        self.save_status = Some(format!("Write error: {}", e));
                        return;
                    }
                }
                if let Err(e) = writer.finalize() {
                    self.save_status = Some(format!("Finalize error: {}", e));
                    return;
                }
                self.save_status = Some(format!(
                    "Saved to {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                self.save_status = Some(format!("Could not create file: {}", e));
            }
        }
    }

    /// Save recorded samples to an MP3 file at the given path.
    pub fn save_recording_mp3(&mut self, path: PathBuf) {
        if self.recorded_samples.is_empty() {
            self.save_status = Some("Nothing recorded yet.".to_string());
            return;
        }

        use mp3lame_encoder::{Builder, FlushNoGap, MonoPcm};

        let Some(mut builder) = Builder::new() else {
            self.save_status = Some("Failed to create MP3 encoder.".to_string());
            return;
        };

        let ok = builder.set_num_channels(1).is_ok()
            && builder.set_sample_rate(self.record_sample_rate).is_ok()
            && builder.set_brate(mp3lame_encoder::Bitrate::Kbps128).is_ok()
            && builder.set_quality(mp3lame_encoder::Quality::Best).is_ok();

        if !ok {
            self.save_status = Some("Failed to configure MP3 encoder.".to_string());
            return;
        }

        let mut encoder = match builder.build() {
            Ok(e) => e,
            Err(e) => {
                self.save_status = Some(format!("Failed to build MP3 encoder: {:?}", e));
                return;
            }
        };

        let pcm: Vec<i16> = self.recorded_samples.iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();

        let mut mp3_data = Vec::new();
        if encoder.encode(MonoPcm(&pcm), &mut mp3_data).is_err()
            || encoder.flush::<FlushNoGap>(&mut mp3_data).is_err()
        {
            self.save_status = Some("MP3 encoding failed.".to_string());
            return;
        }

        // mp3lame-encoder returns Vec<MaybeUninit<u8>> instead of Vec<u8>.
        // this is load-bearing duct tape on someone else's architecture decision.
        // SAFETY: all bytes were written by the encoder so assume_init is fine.
        // I've named it; I'm moving on. woot.
        let mp3_bytes: Vec<u8> = mp3_data.into_iter()
            .map(|b| unsafe { b.assume_init() })
            .collect();

        match std::fs::write(&path, &mp3_bytes) {
            Ok(_) => self.save_status = Some(format!(
                "Saved to {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
            Err(e) => self.save_status = Some(format!("Could not write MP3: {}", e)),
        }
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
