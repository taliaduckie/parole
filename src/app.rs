//! Top-level application state and egui update loop.
//! this file has several jobs and handles most of them with dignity.

use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::audio::{loader::AudioBuffer, player::AudioPlayer, recorder::Recorder};
use crate::dsp::{
    formants::FormantTrack, job::DspJob, pitch::PitchTrack, spectrogram::SpectrogramData,
};
use crate::annotation::textgrid::TextGrid;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SaveFormat {
    #[default]
    Wav,
    Mp3,
}

pub struct PraatlyApp {
    pub buffer:      Option<Arc<AudioBuffer>>,
    pub spectrogram: Option<SpectrogramData>,
    pub pitch:       Option<PitchTrack>,
    pub formants:    Option<FormantTrack>,

    // Background DSP jobs — populated when load_file fires, drained as each
    // worker reports back. None means "no work pending for this lane".
    pub spectrogram_job: Option<DspJob<SpectrogramData>>,
    pub pitch_job:       Option<DspJob<PitchTrack>>,
    pub formants_job:    Option<DspJob<FormantTrack>>,

    /// Cached GPU texture for the spectrogram render. Built lazily on first
    /// paint after a new spectrogram lands, freed (set to None) when the
    /// underlying data changes — so we never upload twice for the same frames.
    pub spectrogram_texture: Option<egui::TextureHandle>,

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
            spectrogram_job: None, pitch_job: None, formants_job: None,
            spectrogram_texture: None,
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

        if let Some(p) = path {
            app.load_file(p, &cc.egui_ctx);
        }
        app
    }

    pub fn load_file(&mut self, path: PathBuf, ctx: &egui::Context) {
        match crate::audio::loader::load_audio(&path) {
            Ok(buf) => {
                self.view_end = buf.duration_secs();
                let buf = Arc::new(buf);

                // Wipe any stale results — old data is for the wrong file now,
                // and any in-flight jobs from a previous load are quietly orphaned
                // (their senders will fail silently when they finally finish).
                self.spectrogram = None;
                self.pitch = None;
                self.formants = None;
                // Drop the cached texture too — it's pixels for a file we're done with.
                self.spectrogram_texture = None;

                let buf_spec = Arc::clone(&buf);
                self.spectrogram_job = Some(DspJob::spawn(ctx.clone(), move || {
                    crate::dsp::spectrogram::compute(&buf_spec, 1024, 0.75)
                }));
                let buf_pitch = Arc::clone(&buf);
                self.pitch_job = Some(DspJob::spawn(ctx.clone(), move || {
                    crate::dsp::pitch::extract(&buf_pitch)
                }));
                let buf_formants = Arc::clone(&buf);
                self.formants_job = Some(DspJob::spawn(ctx.clone(), move || {
                    crate::dsp::formants::extract(&buf_formants)
                }));

                self.buffer = Some(buf);
            }
            Err(e) => log::error!("Failed to load {:?}: {}", path, e),
        }
    }

    /// Drain any DSP jobs that have completed and swap their results into place.
    /// Cheap to call every frame — try_recv is non-blocking.
    fn poll_dsp_jobs(&mut self) {
        if let Some(job) = &self.spectrogram_job {
            if let Some(result) = job.poll() {
                self.spectrogram = Some(result);
                self.spectrogram_job = None;
                // Force a texture rebuild on next paint — old pixels are
                // for a stale spectrogram (or there were no pixels to begin with).
                self.spectrogram_texture = None;
            }
        }
        if let Some(job) = &self.pitch_job {
            if let Some(result) = job.poll() {
                self.pitch = Some(result);
                self.pitch_job = None;
            }
        }
        if let Some(job) = &self.formants_job {
            if let Some(result) = job.poll() {
                self.formants = Some(result);
                self.formants_job = None;
            }
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
        // Pick up any DSP results that finished between frames. Cheap; try_recv is non-blocking.
        self.poll_dsp_jobs();

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
