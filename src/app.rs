//! Top-level application state and egui update loop.
//! this file has several jobs and handles most of them with dignity.

use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::audio::{loader::AudioBuffer, player::AudioPlayer, recorder::Recorder};
use crate::dsp::{
    formants::{FormantSettings, FormantTrack},
    job::DspJob,
    pitch::{PitchSettings, PitchTrack},
    spectrogram::{SpectrogramData, SpectrogramSettings},
};
use crate::annotation::textgrid::TextGrid;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SaveFormat {
    #[default]
    Wav,
    Mp3,
}

/// Severity flavour for status-bar messages. Drives the colour the toolbar
/// renders the text in — you should be able to tell at a glance whether the
/// thing that just happened was good news.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub kind: StatusKind,
}

impl StatusMessage {
    pub fn info(text: impl Into<String>) -> Self {
        Self { text: text.into(), kind: StatusKind::Info }
    }
    pub fn success(text: impl Into<String>) -> Self {
        Self { text: text.into(), kind: StatusKind::Success }
    }
    pub fn error(text: impl Into<String>) -> Self {
        Self { text: text.into(), kind: StatusKind::Error }
    }
}

/// Everything the recorder needs to remember between frames.
pub struct RecordingState {
    pub recorder:     Recorder,
    pub started_at:   Option<Instant>,
    pub samples:      Vec<f32>,
    pub sample_rate:  u32,
    pub save_format:  SaveFormat,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            recorder: Recorder::new(),
            started_at: None,
            samples: Vec::new(),
            sample_rate: 44100,
            save_format: SaveFormat::Wav,
        }
    }
}

impl Default for RecordingState {
    fn default() -> Self { Self::new() }
}

/// Computed DSP outputs, plus the GPU-side cache for the spectrogram.
/// All `Option`s — None means "not computed yet (or just invalidated)".
#[derive(Default)]
pub struct DspResults {
    pub spectrogram:         Option<SpectrogramData>,
    pub pitch:               Option<PitchTrack>,
    pub formants:            Option<FormantTrack>,
    pub spectrogram_texture: Option<egui::TextureHandle>,
}

/// In-flight worker handles. Each becomes None once its result lands in DspResults.
#[derive(Default)]
pub struct DspJobs {
    pub spectrogram: Option<DspJob<SpectrogramData>>,
    pub pitch:       Option<DspJob<PitchTrack>>,
    pub formants:    Option<DspJob<FormantTrack>>,
}

/// User-tweakable knobs that drive the DSP passes. Settings panel writes here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DspParams {
    pub spectrogram: SpectrogramSettings,
    pub pitch:       PitchSettings,
    pub formants:    FormantSettings,
}

/// What's visible right now: time window, selection, and which overlays/panels
/// the user has on. Pretty much anything that affects "what the user sees" but
/// not "what's been computed" lives here.
pub struct ViewState {
    pub start:            f64,
    pub end:              f64,
    pub selection:        Option<(f64, f64)>,
    pub show_spectrogram: bool,
    pub show_pitch:       bool,
    pub show_formants:    bool,
    pub show_textgrid:    bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 5.0,
            selection: None,
            show_spectrogram: true,
            show_pitch:       true,
            show_formants:    true,
            show_textgrid:    true,
        }
    }
}

/// Floaty UI bits that aren't really part of the analysis state — modal toggles,
/// status bar. The kind of state that's allowed to be ephemeral.
#[derive(Default)]
pub struct UiState {
    pub show_help:     bool,
    pub show_settings: bool,
    pub status:        Option<StatusMessage>,
}

impl UiState {
    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Some(StatusMessage::info(text));
    }
    pub fn success(&mut self, text: impl Into<String>) {
        self.status = Some(StatusMessage::success(text));
    }
    pub fn error(&mut self, text: impl Into<String>) {
        self.status = Some(StatusMessage::error(text));
    }
}

pub struct PraatlyApp {
    pub buffer:    Option<Arc<AudioBuffer>>,
    pub textgrid:  TextGrid,
    pub player:    AudioPlayer,

    pub recording: RecordingState,
    pub dsp:       DspResults,
    pub jobs:      DspJobs,
    pub params:    DspParams,
    pub view:      ViewState,
    pub ui:        UiState,
}

impl PraatlyApp {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = egui::Rounding::same(8.0);
        cc.egui_ctx.set_visuals(visuals);

        let mut app = Self {
            buffer:    None,
            textgrid:  TextGrid::default(),
            player:    AudioPlayer::new(),
            recording: RecordingState::new(),
            dsp:       DspResults::default(),
            jobs:      DspJobs::default(),
            params:    DspParams::default(),
            view:      ViewState::default(),
            ui:        UiState::default(),
        };

        if let Some(p) = path {
            app.load_file(p, &cc.egui_ctx);
        }
        app
    }

    pub fn load_file(&mut self, path: PathBuf, ctx: &egui::Context) {
        match crate::audio::loader::load_audio(&path) {
            Ok(buf) => {
                self.view.end = buf.duration_secs();
                let buf = Arc::new(buf);

                // Wipe any stale results — old data is for the wrong file now,
                // and any in-flight jobs from a previous load are quietly orphaned
                // (their senders will fail silently when they finally finish).
                self.dsp = DspResults::default();

                self.buffer = Some(buf);
                self.respawn_spectrogram(ctx);
                self.respawn_pitch(ctx);
                self.respawn_formants(ctx);
            }
            Err(e) => {
                log::error!("Failed to load {:?}: {}", path, e);
                self.ui.error(format!("Couldn't load file: {}", e));
            }
        }
    }

    /// Re-run the spectrogram pass with the current settings, on a worker thread.
    /// No-op if no audio is loaded.
    pub fn respawn_spectrogram(&mut self, ctx: &egui::Context) {
        let Some(buf) = self.buffer.as_ref().map(Arc::clone) else { return; };
        let s = self.params.spectrogram;
        // Wipe the old result + texture immediately so the UI shows "loading"
        // instead of mismatched-stale.
        self.dsp.spectrogram = None;
        self.dsp.spectrogram_texture = None;
        self.jobs.spectrogram = Some(DspJob::spawn(ctx.clone(), move || {
            crate::dsp::spectrogram::compute(&buf, s.window_size, s.overlap)
        }));
    }

    pub fn respawn_pitch(&mut self, ctx: &egui::Context) {
        let Some(buf) = self.buffer.as_ref().map(Arc::clone) else { return; };
        let s = self.params.pitch;
        self.dsp.pitch = None;
        self.jobs.pitch = Some(DspJob::spawn(ctx.clone(), move || {
            crate::dsp::pitch::extract(&buf, s)
        }));
    }

    pub fn respawn_formants(&mut self, ctx: &egui::Context) {
        let Some(buf) = self.buffer.as_ref().map(Arc::clone) else { return; };
        let s = self.params.formants;
        self.dsp.formants = None;
        self.jobs.formants = Some(DspJob::spawn(ctx.clone(), move || {
            crate::dsp::formants::extract(&buf, s)
        }));
    }

    /// Drain any DSP jobs that have completed and swap their results into place.
    /// Cheap to call every frame — try_recv is non-blocking.
    fn poll_dsp_jobs(&mut self) {
        if let Some(job) = &self.jobs.spectrogram {
            if let Some(result) = job.poll() {
                self.dsp.spectrogram = Some(result);
                self.jobs.spectrogram = None;
                // Force a texture rebuild on next paint — old pixels are
                // for a stale spectrogram (or there were no pixels to begin with).
                self.dsp.spectrogram_texture = None;
            }
        }
        if let Some(job) = &self.jobs.pitch {
            if let Some(result) = job.poll() {
                self.dsp.pitch = Some(result);
                self.jobs.pitch = None;
            }
        }
        if let Some(job) = &self.jobs.formants {
            if let Some(result) = job.poll() {
                self.dsp.formants = Some(result);
                self.jobs.formants = None;
            }
        }
    }

    /// Save recorded samples to a WAV file at the given path.
    pub fn save_recording_wav(&mut self, path: PathBuf) {
        if self.recording.samples.is_empty() {
            self.ui.info("Nothing recorded yet.");
            return;
        }
        match crate::audio::encoder::write_wav_mono_f32(
            &path,
            &self.recording.samples,
            self.recording.sample_rate,
        ) {
            Ok(()) => self.ui.success(format!(
                "Saved to {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
            Err(e) => self.ui.error(format!("WAV save failed: {}", e)),
        }
    }

    /// Save recorded samples to an MP3 file at the given path.
    pub fn save_recording_mp3(&mut self, path: PathBuf) {
        if self.recording.samples.is_empty() {
            self.ui.info("Nothing recorded yet.");
            return;
        }
        match crate::audio::encoder::write_mp3_mono(
            &path,
            &self.recording.samples,
            self.recording.sample_rate,
        ) {
            Ok(()) => self.ui.success(format!(
                "Saved to {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
            Err(e) => self.ui.error(format!("MP3 save failed: {}", e)),
        }
    }
}

impl eframe::App for PraatlyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Pick up any DSP results that finished between frames. Cheap; try_recv is non-blocking.
        self.poll_dsp_jobs();

        // Keyboard shortcut: F1 toggles help
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.ui.show_help = !self.ui.show_help;
        }

        crate::ui::toolbar::show(ctx, self);

        // Help panel — renders as a floating window
        if self.ui.show_help {
            crate::ui::help::show(ctx, self);
        }
        if self.ui.show_settings {
            crate::ui::settings::show(ctx, self);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let h = ui.available_height();
            crate::ui::waveform::show(ui, self, h * 0.25);
            if self.view.show_spectrogram {
                crate::ui::spectrogram::show(ui, self, h * 0.45);
            }
            if self.view.show_textgrid {
                crate::annotation::textgrid::show(ui, self, h * 0.30);
            }
        });
    }
}
