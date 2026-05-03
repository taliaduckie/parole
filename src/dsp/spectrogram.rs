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

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, sample_rate: u32, secs: f32) -> AudioBuffer {
        let n = (sample_rate as f32 * secs) as usize;
        let samples = (0..n)
            .map(|i| (i as f32 * freq * 2.0 * std::f32::consts::PI / sample_rate as f32).sin() * 0.5)
            .collect();
        AudioBuffer { samples, sample_rate, channels: 1 }
    }

    /// Index of the loudest bin in a single STFT frame.
    fn argmax(frame: &[f32]) -> usize {
        frame.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap()
    }

    #[test]
    fn n_frames_and_n_bins_basic() {
        let buf = sine(440.0, 16000, 0.5);
        let spec = compute(&buf, 1024, 0.5);
        assert_eq!(spec.n_bins(), 1024 / 2 + 1);
        // 8000 samples, hop = 512 → frames where window fully fits.
        let expected = (8000usize - 1024) / 512 + 1;
        assert_eq!(spec.n_frames(), expected);
    }

    #[test]
    fn bin_to_hz_endpoints() {
        let spec = SpectrogramData {
            magnitudes: vec![],
            n_fft: 1024,
            hop_size: 256,
            sample_rate: 16000,
        };
        assert_eq!(spec.bin_to_hz(0), 0.0);
        // Last bin at index n_fft/2 should be at the Nyquist frequency.
        assert!((spec.bin_to_hz(512) - 8000.0).abs() < 1e-3);
    }

    #[test]
    fn frame_to_sec_endpoints() {
        let spec = SpectrogramData {
            magnitudes: vec![],
            n_fft: 1024,
            hop_size: 256,
            sample_rate: 16000,
        };
        assert!((spec.frame_to_sec(0) - 0.0).abs() < 1e-12);
        // frame 4 starts at sample 1024 → 1024/16000 = 0.064 s
        assert!((spec.frame_to_sec(4) - 0.064).abs() < 1e-9);
    }

    #[test]
    fn sine_peaks_in_expected_bin() {
        // 1000 Hz sine at 16 kHz, 1024-pt FFT → bin spacing 15.625 Hz → expected bin 64.
        // We allow ±1 bin of slop — the Hann window smears energy a little.
        let buf = sine(1000.0, 16000, 0.5);
        let spec = compute(&buf, 1024, 0.5);
        let mid_frame = &spec.magnitudes[spec.n_frames() / 2];
        let peak = argmax(mid_frame);
        let expected = (1000.0_f32 * 1024.0 / 16000.0).round() as usize; // 64
        assert!(
            (peak as i32 - expected as i32).abs() <= 1,
            "peak bin = {}, expected ~{}", peak, expected
        );
    }

    #[test]
    fn sine_peak_bin_matches_bin_to_hz_inverse() {
        let buf = sine(2500.0, 22050, 0.5);
        let spec = compute(&buf, 2048, 0.5);
        let peak = argmax(&spec.magnitudes[spec.n_frames() / 2]);
        let hz = spec.bin_to_hz(peak);
        assert!((hz - 2500.0).abs() < spec.bin_to_hz(1), "got {} Hz", hz);
    }

    #[test]
    fn silence_produces_near_zero_magnitudes() {
        let buf = AudioBuffer { samples: vec![0.0; 8192], sample_rate: 16000, channels: 1 };
        let spec = compute(&buf, 1024, 0.5);
        assert!(spec.n_frames() > 0);
        for frame in &spec.magnitudes {
            for &m in frame { assert!(m < 1e-6, "non-zero mag in silence: {}", m); }
        }
    }

    #[test]
    fn short_buffer_yields_no_frames() {
        // Buffer shorter than window — should produce zero frames, no panic.
        let buf = AudioBuffer { samples: vec![0.1; 500], sample_rate: 16000, channels: 1 };
        let spec = compute(&buf, 1024, 0.5);
        assert_eq!(spec.n_frames(), 0);
    }

    #[test]
    fn overlap_zero_uses_non_overlapping_hop() {
        let buf = sine(440.0, 16000, 1.0);
        let spec = compute(&buf, 1024, 0.0);
        assert_eq!(spec.hop_size, 1024);
    }

    #[test]
    fn overlap_clamps_hop_to_at_least_one() {
        // overlap = 1.0 would naively give hop = 0. The compute clamps to 1
        // so the loop terminates instead of helpfully crashing the universe.
        let buf = AudioBuffer { samples: vec![0.0; 2048], sample_rate: 16000, channels: 1 };
        let spec = compute(&buf, 1024, 1.0);
        assert_eq!(spec.hop_size, 1);
    }
}
