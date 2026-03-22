//! STFT spectrogram via rustfft.
//! if this file intimidates u a little: same. the math is real even if we
//! don't talk about it at parties.

use rustfft::{FftPlanner, num_complex::Complex};
use crate::audio::loader::AudioBuffer;

pub struct SpectrogramData {
    pub magnitudes:  Vec<Vec<f32>>,
    pub n_fft:       usize,
    pub hop_size:    usize,
    pub sample_rate: u32,
}

impl SpectrogramData {
    pub fn n_frames(&self) -> usize { self.magnitudes.len() }
    pub fn n_bins(&self)   -> usize { self.n_fft / 2 + 1 }
    pub fn bin_to_hz(&self, bin: usize) -> f32 {
        bin as f32 * self.sample_rate as f32 / self.n_fft as f32
    }
    pub fn frame_to_sec(&self, frame: usize) -> f64 {
        frame as f64 * self.hop_size as f64 / self.sample_rate as f64
    }
}

pub fn compute(buf: &AudioBuffer, window_size: usize, overlap: f64) -> SpectrogramData {
    let mono = buf.mono();
    let hop  = ((1.0 - overlap) * window_size as f64).round().max(1.0) as usize;

    // Hann window — 0.5 * (1 - cos(2πi/N)).
    // this is what it looks like. I understand what it's doing and I want it on record
    // that I'm not happy about either of us being here
    let window: Vec<f32> = (0..window_size)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32
                                / (window_size - 1) as f32).cos()))
        .collect();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(window_size);
    let mut magnitudes = Vec::new();

    let mut start = 0;
    while start + window_size <= mono.len() {
        // apply window, pack into complex (imaginary part = 0 because we're honest about where we are)
        let mut buf: Vec<Complex<f32>> = mono[start..start + window_size]
            .iter().zip(&window)
            .map(|(&s, &w)| Complex { re: s * w, im: 0.0 })
            .collect();
        fft.process(&mut buf);
        // take only the first N/2+1 bins — the rest are mirrored and we don't need the reflection
        let frame: Vec<f32> = buf[..window_size / 2 + 1].iter().map(|c| c.norm()).collect();
        magnitudes.push(frame);
        start += hop;
    }

    SpectrogramData { magnitudes, n_fft: window_size, hop_size: hop, sample_rate: buf.sample_rate }
}
