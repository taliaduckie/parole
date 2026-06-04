//! Top-level application state and egui update loop
//! this file has several jobs and handles most of them with dignity

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
/// thing that just happened was good news
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

/// Everything the recorder needs to remember between frames
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

/// Computed DSP outputs, plus the GPU-side cache for the spectrogram
/// All `Option`s — None means "not computed yet (or just invalidated)"
#[derive(Default)]
pub struct DspResults {
    pub spectrogram:         Option<SpectrogramData>,
    pub pitch:               Option<PitchTrack>,
    pub formants:            Option<FormantTrack>,
    pub spectrogram_texture: Option<egui::TextureHandle>,
}

/// In-flight worker handles. Each becomes None once its result lands in DspResults
#[derive(Default)]
pub struct DspJobs {
    pub spectrogram: Option<DspJob<SpectrogramData>>,
    pub pitch:       Option<DspJob<PitchTrack>>,
    pub formants:    Option<DspJob<FormantTrack>>,
}

/// User-tweakable knobs that drive the DSP passes. Settings panel writes here
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DspParams {
    pub spectrogram: SpectrogramSettings,
    pub pitch:       PitchSettings,
    pub formants:    FormantSettings,
}

/// What's visible right now: time window, selection, and which overlays/panels
/// the user has on. Pretty much anything that affects "what the user sees" but
/// not "what's been computed" lives here
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

/// Smallest visible window. Below this we'd start dividing by ~0 in time→pixel
/// math and the user would be lost in their own audio
const MIN_VIEW_DURATION: f64 = 1e-3;

impl ViewState {
    /// Zoom in so the current selection fills the view. Clears the selection
    /// since you're now looking at exactly that range — no need to highlight
    pub fn zoom_to_selection(&mut self) {
        if let Some((s, e)) = self.selection {
            if e - s > MIN_VIEW_DURATION {
                self.start = s;
                self.end = e;
                self.selection = None;
            }
        }
    }

    /// Reset view to the whole loaded file
    pub fn zoom_to_full(&mut self, file_duration: Option<f64>) {
        self.start = 0.0;
        if let Some(d) = file_duration {
            self.end = d.max(MIN_VIEW_DURATION);
        }
    }

    /// Multiplicative zoom around a time anchor (kept at the same screen position).
    /// `factor` < 1 zooms in, > 1 zooms out. Clamps to [0, file_duration] when known
    pub fn zoom_around(&mut self, anchor: f64, factor: f64, file_duration: Option<f64>) {
        let cur_dur = self.end - self.start;
        if cur_dur <= 0.0 { return; }
        let new_dur = (cur_dur * factor).max(MIN_VIEW_DURATION);
        let rel = ((anchor - self.start) / cur_dur).clamp(0.0, 1.0);
        let mut new_start = anchor - rel * new_dur;
        let mut new_end = new_start + new_dur;

        // Clamp into [0, max_end]. If file_duration is unknown we let the right
        // side float — load_file will reset view.end anyway
        if let Some(max_end) = file_duration {
            let max_end = max_end.max(MIN_VIEW_DURATION);
            if new_dur >= max_end {
                new_start = 0.0;
                new_end = max_end;
            } else if new_start < 0.0 {
                new_start = 0.0;
                new_end = new_dur;
            } else if new_end > max_end {
                new_end = max_end;
                new_start = max_end - new_dur;
            }
        } else if new_start < 0.0 {
            new_start = 0.0;
            new_end = new_dur;
        }

        self.start = new_start;
        self.end = new_end;
    }
}

/// Floaty UI bits that aren't really part of the analysis state — modal toggles,
/// status bar. The kind of state that's allowed to be ephemeral
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
                // (their senders will fail silently when they finally finish)
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

    /// Re-run the spectrogram pass with the current settings, on a worker thread
    /// No-op if no audio is loaded
    pub fn respawn_spectrogram(&mut self, ctx: &egui::Context) {
        let Some(buf) = self.buffer.as_ref().map(Arc::clone) else { return; };
        let s = self.params.spectrogram;
        // Wipe the old result + texture immediately so the UI shows "loading"
        // instead of mismatched-stale
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

    /// Drain any DSP jobs that have completed and swap their results into place
    /// Cheap to call every frame — try_recv is non-blocking
    fn poll_dsp_jobs(&mut self) {
        if let Some(job) = &self.jobs.spectrogram {
            if let Some(result) = job.poll() {
                self.dsp.spectrogram = Some(result);
                self.jobs.spectrogram = None;
                // Force a texture rebuild on next paint — old pixels are
                // for a stale spectrogram (or there were no pixels to begin with)
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

    /// Duration of the currently-loaded file in seconds, if any
    pub fn buffer_duration(&self) -> Option<f64> {
        self.buffer.as_deref().map(|b| b.duration_secs())
    }

    /// Save recorded samples to a WAV file at the given path
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

    /// Save recorded samples to an MP3 file at the given path
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
        // Pick up any DSP results that finished between frames. Cheap; try_recv is non-blocking
        self.poll_dsp_jobs();

        // Keyboard shortcuts
        let (toggle_help, zoom_sel, zoom_all) = ctx.input(|i| (
            i.key_pressed(egui::Key::F1),
            i.key_pressed(egui::Key::Z) && !i.modifiers.shift,
            (i.key_pressed(egui::Key::Z) && i.modifiers.shift) || i.key_pressed(egui::Key::A),
        ));
        if toggle_help {
            self.ui.show_help = !self.ui.show_help;
        }
        if zoom_sel {
            self.view.zoom_to_selection();
        }
        if zoom_all {
            self.view.zoom_to_full(self.buffer_duration());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn view(start: f64, end: f64) -> ViewState {
        let mut v = ViewState::default();
        v.start = start;
        v.end = end;
        v
    }

    #[test]
    fn zoom_to_selection_sets_bounds_and_clears_selection() {
        let mut v = view(0.0, 10.0);
        v.selection = Some((2.0, 5.0));
        v.zoom_to_selection();
        assert!((v.start - 2.0).abs() < 1e-9);
        assert!((v.end   - 5.0).abs() < 1e-9);
        assert!(v.selection.is_none());
    }

    #[test]
    fn zoom_to_selection_noop_without_selection() {
        let mut v = view(0.0, 10.0);
        v.zoom_to_selection();
        assert!((v.start - 0.0).abs() < 1e-9);
        assert!((v.end   - 10.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_to_selection_noop_for_zero_width_selection() {
        let mut v = view(0.0, 10.0);
        v.selection = Some((4.0, 4.0));
        v.zoom_to_selection();
        assert!((v.start - 0.0).abs() < 1e-9);
        assert!((v.end   - 10.0).abs() < 1e-9);
        // selection preserved since we didn't act
        assert_eq!(v.selection, Some((4.0, 4.0)));
    }

    #[test]
    fn zoom_to_full_resets_to_file_bounds() {
        let mut v = view(3.0, 7.0);
        v.zoom_to_full(Some(12.5));
        assert!((v.start - 0.0).abs() < 1e-9);
        assert!((v.end   - 12.5).abs() < 1e-9);
    }

    #[test]
    fn zoom_to_full_without_known_duration_only_resets_start() {
        let mut v = view(3.0, 7.0);
        v.zoom_to_full(None);
        assert_eq!(v.start, 0.0);
        // end untouched since we don't know how big the file is
        assert!((v.end - 7.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_in_keeps_anchor_at_same_relative_position() {
        // View [0, 10], anchor at t=2 (20% across). After zooming to 50%
        // duration, anchor should still be at the 20% mark
        let mut v = view(0.0, 10.0);
        v.zoom_around(2.0, 0.5, Some(100.0));
        let new_dur = v.end - v.start;
        assert!((new_dur - 5.0).abs() < 1e-9);
        let rel = (2.0 - v.start) / new_dur;
        assert!((rel - 0.2).abs() < 1e-6, "anchor drifted: rel = {}", rel);
    }

    #[test]
    fn zoom_around_clamps_to_left_edge() {
        // View [0.5, 1.5], anchor at the middle (t=1.0). Zoom out 4x →
        // new_dur=4, would put new_start at -1. Clamp to 0, shift right
        let mut v = view(0.5, 1.5);
        v.zoom_around(1.0, 4.0, Some(100.0));
        assert_eq!(v.start, 0.0);
        let new_dur = v.end - v.start;
        assert!((new_dur - 4.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_clamps_to_right_edge() {
        // View [8, 9], anchor at the middle (t=8.5). Zoom out 4x → new_end
        // would land at 10.5, past the 10s file end. Clamp to 10, shift left
        let mut v = view(8.0, 9.0);
        v.zoom_around(8.5, 4.0, Some(10.0));
        assert!((v.end   - 10.0).abs() < 1e-9);
        assert!((v.start - 6.0).abs()  < 1e-9);
        let new_dur = v.end - v.start;
        assert!((new_dur - 4.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_caps_at_file_duration() {
        // Zooming out beyond file duration should fill the whole file
        let mut v = view(2.0, 4.0);
        v.zoom_around(3.0, 100.0, Some(10.0));
        assert_eq!(v.start, 0.0);
        assert!((v.end - 10.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_respects_minimum_duration() {
        let mut v = view(0.0, 10.0);
        v.zoom_around(5.0, 0.0, Some(100.0));
        assert!(v.end - v.start >= MIN_VIEW_DURATION);
    }

}
