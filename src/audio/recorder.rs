//! Microphone capture via cpal.
//! Owns an input stream while recording and accumulates samples
//! (downmixed to mono) into a shared buffer.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::Arc;

pub struct Recorder {
    samples: Arc<Mutex<Vec<f32>>>,
    runtime_error: Arc<Mutex<Option<String>>>,
    stream: Option<cpal::Stream>,
    pub sample_rate: u32,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            runtime_error: Arc::new(Mutex::new(None)),
            stream: None,
            sample_rate: 44100,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.stream.is_some()
    }

    /// Drains any error reported by the cpal callback since the last call.
    pub fn take_runtime_error(&self) -> Option<String> {
        self.runtime_error.lock().take()
    }

    pub fn start(&mut self) -> Result<()> {
        if self.stream.is_some() {
            return Ok(());
        }
        self.samples.lock().clear();
        *self.runtime_error.lock() = None;

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No default input device available"))?;
        let supported = device
            .default_input_config()
            .context("Could not query default input config")?;

        let sample_format = supported.sample_format();
        let channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.into();
        self.sample_rate = config.sample_rate.0;

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let samples = Arc::clone(&self.samples);
                let err_buf = Arc::clone(&self.runtime_error);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| append_downmixed(&samples, data, channels),
                    move |e| *err_buf.lock() = Some(format!("Input stream error: {}", e)),
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let samples = Arc::clone(&self.samples);
                let err_buf = Arc::clone(&self.runtime_error);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let scaled: Vec<f32> =
                            data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                        append_downmixed(&samples, &scaled, channels);
                    },
                    move |e| *err_buf.lock() = Some(format!("Input stream error: {}", e)),
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let samples = Arc::clone(&self.samples);
                let err_buf = Arc::clone(&self.runtime_error);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let scaled: Vec<f32> = data
                            .iter()
                            .map(|s| (*s as f32 - 32768.0) / 32768.0)
                            .collect();
                        append_downmixed(&samples, &scaled, channels);
                    },
                    move |e| *err_buf.lock() = Some(format!("Input stream error: {}", e)),
                    None,
                )?
            }
            other => return Err(anyhow!("Unsupported input sample format: {:?}", other)),
        };

        stream.play().context("Failed to start input stream")?;
        self.stream = Some(stream);
        Ok(())
    }

    /// Stop the stream and return the captured samples (mono, f32, at `sample_rate`).
    pub fn stop(&mut self) -> Vec<f32> {
        self.stream = None;
        std::mem::take(&mut *self.samples.lock())
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Append `data` (interleaved, `channels`-channel) to `buf`, downmixing to mono.
pub(crate) fn append_downmixed(buf: &Arc<Mutex<Vec<f32>>>, data: &[f32], channels: usize) {
    let mut g = buf.lock();
    if channels <= 1 {
        g.extend_from_slice(data);
    } else {
        for frame in data.chunks_exact(channels) {
            g.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_mono_passthrough() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        append_downmixed(&buf, &[0.1, 0.2, 0.3], 1);
        assert_eq!(*buf.lock(), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn downmix_zero_channels_treated_as_mono() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        append_downmixed(&buf, &[0.5, -0.5], 0);
        assert_eq!(*buf.lock(), vec![0.5, -0.5]);
    }

    #[test]
    fn downmix_stereo_averages_pairs() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        append_downmixed(&buf, &[1.0, -1.0, 0.4, 0.6], 2);
        let g = buf.lock();
        assert_eq!(g.len(), 2);
        assert!((g[0] - 0.0).abs() < 1e-6);
        assert!((g[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn downmix_drops_partial_trailing_frame() {
        // 3-channel, 7 samples → 2 full frames (the last sample is dropped)
        let buf = Arc::new(Mutex::new(Vec::new()));
        append_downmixed(&buf, &[3.0, 3.0, 3.0, 6.0, 6.0, 6.0, 9.0], 3);
        assert_eq!(*buf.lock(), vec![3.0, 6.0]);
    }

    #[test]
    fn downmix_appends_across_calls() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        append_downmixed(&buf, &[0.1, 0.1], 2);
        append_downmixed(&buf, &[0.2, 0.2], 2);
        assert_eq!(*buf.lock(), vec![0.1, 0.2]);
    }

    #[test]
    fn new_recorder_is_idle() {
        let r = Recorder::new();
        assert!(!r.is_recording());
        assert!(r.take_runtime_error().is_none());
    }
}
