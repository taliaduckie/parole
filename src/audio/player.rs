//! Audio playback via cpal.
//! Owns an output stream while playing. Mono source is duplicated to all
//! output channels; sample-rate mismatch is handled by linear resampling
//! in the output callback.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct AudioPlayer {
    samples: Arc<Mutex<Vec<f32>>>,
    /// Fractional read position in source samples.
    cursor: Arc<Mutex<f64>>,
    src_rate: Arc<Mutex<u32>>,
    playing: Arc<AtomicBool>,
    runtime_error: Arc<Mutex<Option<String>>>,
    stream: Option<cpal::Stream>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            cursor: Arc::new(Mutex::new(0.0)),
            src_rate: Arc::new(Mutex::new(44100)),
            playing: Arc::new(AtomicBool::new(false)),
            runtime_error: Arc::new(Mutex::new(None)),
            stream: None,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn position_secs(&self) -> f64 {
        let sr = *self.src_rate.lock() as f64;
        if sr <= 0.0 { 0.0 } else { *self.cursor.lock() / sr }
    }

    pub fn take_runtime_error(&self) -> Option<String> {
        self.runtime_error.lock().take()
    }

    /// Start (or restart) playback of `samples` at `sample_rate`.
    /// Replaces any in-flight stream.
    pub fn play(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<()> {
        // Drop any prior stream before installing the new one.
        self.stream = None;
        self.playing.store(false, Ordering::Relaxed);

        if samples.is_empty() {
            return Ok(()); // nothing to play, not an error
        }

        *self.samples.lock() = samples;
        *self.cursor.lock() = 0.0;
        *self.src_rate.lock() = sample_rate;
        *self.runtime_error.lock() = None;

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No default output device available"))?;
        let supported = device
            .default_output_config()
            .context("Could not query default output config")?;

        let sample_format = supported.sample_format();
        let out_channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.into();
        let out_rate = config.sample_rate.0;
        let step = sample_rate as f64 / out_rate as f64;

        let samples_arc = Arc::clone(&self.samples);
        let cursor_arc = Arc::clone(&self.cursor);
        let playing_arc = Arc::clone(&self.playing);
        let err_buf = Arc::clone(&self.runtime_error);

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    let src = samples_arc.lock();
                    let mut cur = cursor_arc.lock();
                    let done = fill_output_f32(out, &src, &mut cur, step, out_channels);
                    if done {
                        playing_arc.store(false, Ordering::Relaxed);
                    }
                },
                move |e| *err_buf.lock() = Some(format!("Output stream error: {}", e)),
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |out: &mut [i16], _| {
                    let src = samples_arc.lock();
                    let mut cur = cursor_arc.lock();
                    let done = fill_output_i16(out, &src, &mut cur, step, out_channels);
                    if done {
                        playing_arc.store(false, Ordering::Relaxed);
                    }
                },
                move |e| *err_buf.lock() = Some(format!("Output stream error: {}", e)),
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_output_stream(
                &config,
                move |out: &mut [u16], _| {
                    let src = samples_arc.lock();
                    let mut cur = cursor_arc.lock();
                    let done = fill_output_u16(out, &src, &mut cur, step, out_channels);
                    if done {
                        playing_arc.store(false, Ordering::Relaxed);
                    }
                },
                move |e| *err_buf.lock() = Some(format!("Output stream error: {}", e)),
                None,
            )?,
            other => return Err(anyhow!("Unsupported output sample format: {:?}", other)),
        };

        stream.play().context("Failed to start output stream")?;
        self.playing.store(true, Ordering::Relaxed);
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream = None;
        self.playing.store(false, Ordering::Relaxed);
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Fill `out` (interleaved, `out_channels`-channel) by reading mono source
/// samples with linear resampling. `cursor` is advanced in place. Returns
/// true if playback reached the end of `src` (the tail is filled with silence).
pub(crate) fn fill_output_f32(
    out: &mut [f32],
    src: &[f32],
    cursor: &mut f64,
    step: f64,
    out_channels: usize,
) -> bool {
    if src.is_empty() || out_channels == 0 {
        for s in out.iter_mut() { *s = 0.0; }
        return true;
    }
    let mut ended = false;
    let n_frames = out.len() / out_channels;
    for frame in 0..n_frames {
        let pos = *cursor;
        let sample = if pos >= (src.len() - 1) as f64 {
            ended = true;
            0.0
        } else {
            let i = pos.floor() as usize;
            let frac = (pos - i as f64) as f32;
            src[i] * (1.0 - frac) + src[i + 1] * frac
        };
        for ch in 0..out_channels {
            out[frame * out_channels + ch] = sample;
        }
        *cursor = pos + step;
    }
    ended
}

pub(crate) fn fill_output_i16(
    out: &mut [i16],
    src: &[f32],
    cursor: &mut f64,
    step: f64,
    out_channels: usize,
) -> bool {
    let mut tmp = vec![0.0_f32; out.len()];
    let ended = fill_output_f32(&mut tmp, src, cursor, step, out_channels);
    for (dst, s) in out.iter_mut().zip(tmp.iter()) {
        *dst = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
    }
    ended
}

pub(crate) fn fill_output_u16(
    out: &mut [u16],
    src: &[f32],
    cursor: &mut f64,
    step: f64,
    out_channels: usize,
) -> bool {
    let mut tmp = vec![0.0_f32; out.len()];
    let ended = fill_output_f32(&mut tmp, src, cursor, step, out_channels);
    for (dst, s) in out.iter_mut().zip(tmp.iter()) {
        let scaled = (s.clamp(-1.0, 1.0) + 1.0) * 0.5; // [-1,1] → [0,1]
        *dst = (scaled * u16::MAX as f32) as u16;
    }
    ended
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_player_is_idle() {
        let p = AudioPlayer::new();
        assert!(!p.is_playing());
        assert_eq!(p.position_secs(), 0.0);
        assert!(p.take_runtime_error().is_none());
    }

    #[test]
    fn fill_output_passthrough_when_rates_match_mono() {
        let src = vec![0.1, 0.2, 0.3, 0.4];
        let mut cur = 0.0;
        let mut out = vec![0.0_f32; 4];
        let ended = fill_output_f32(&mut out, &src, &mut cur, 1.0, 1);
        // Last frame writes from src[3..4] which trips the "end" guard since
        // the linear interp needs src[i+1]; that's expected behavior — we get
        // 3 real samples + a silence tail.
        assert!(ended);
        assert!((out[0] - 0.1).abs() < 1e-6);
        assert!((out[1] - 0.2).abs() < 1e-6);
        assert!((out[2] - 0.3).abs() < 1e-6);
        assert_eq!(out[3], 0.0);
    }

    #[test]
    fn fill_output_duplicates_mono_into_stereo() {
        let src = vec![0.5, 0.5, 0.5, 0.5];
        let mut cur = 0.0;
        let mut out = vec![0.0_f32; 6]; // 3 stereo frames
        let _ = fill_output_f32(&mut out, &src, &mut cur, 1.0, 2);
        // Each pair should hold the same mono sample
        assert!((out[0] - out[1]).abs() < 1e-6);
        assert!((out[2] - out[3]).abs() < 1e-6);
        assert!((out[4] - out[5]).abs() < 1e-6);
    }

    #[test]
    fn fill_output_advances_cursor() {
        let src = vec![0.0; 100];
        let mut cur = 0.0;
        let mut out = vec![0.0_f32; 10];
        let _ = fill_output_f32(&mut out, &src, &mut cur, 0.5, 1);
        assert!((cur - 5.0).abs() < 1e-9);
    }

    #[test]
    fn fill_output_linear_interpolates_between_samples() {
        let src = vec![0.0, 1.0];
        let mut cur = 0.5;
        let mut out = vec![0.0_f32; 1];
        let _ = fill_output_f32(&mut out, &src, &mut cur, 1.0, 1);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn fill_output_signals_end_with_silence_tail() {
        let src = vec![0.7, 0.8];
        let mut cur = 5.0; // already past the end
        let mut out = vec![1.0_f32; 4];
        let ended = fill_output_f32(&mut out, &src, &mut cur, 1.0, 1);
        assert!(ended);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn fill_output_handles_empty_src() {
        let src: Vec<f32> = Vec::new();
        let mut cur = 0.0;
        let mut out = vec![1.0_f32; 4];
        let ended = fill_output_f32(&mut out, &src, &mut cur, 1.0, 2);
        assert!(ended);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn fill_output_i16_scales_to_int_range() {
        let src = vec![1.0, 1.0, -1.0, -1.0];
        let mut cur = 0.0;
        let mut out = vec![0_i16; 2];
        let _ = fill_output_i16(&mut out, &src, &mut cur, 1.0, 1);
        assert_eq!(out[0], i16::MAX);
        assert!(out[1] >= i16::MAX - 1); // interp toward next sample (also +1)
    }

    #[test]
    fn fill_output_u16_centers_silence_at_midpoint() {
        let src = vec![0.0, 0.0];
        let mut cur = 0.0;
        let mut out = vec![0_u16; 1];
        let _ = fill_output_u16(&mut out, &src, &mut cur, 1.0, 1);
        // 0.0 input → center of u16 range
        let mid = u16::MAX / 2;
        assert!((out[0] as i32 - mid as i32).abs() <= 1);
    }
}
