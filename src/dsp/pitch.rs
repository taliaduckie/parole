//! F0 extraction via normalised autocorrelation.
//! good enough for most purposes. maybe not for phoneticians who are paying very
//! close attention. hi if that's u!

use crate::audio::loader::AudioBuffer;

pub struct PitchTrack {
    pub frames:      Vec<Option<f32>>,
    pub hop_size:    usize,
    pub sample_rate: u32,
}

impl PitchTrack {
    pub fn frame_to_sec(&self, frame: usize) -> f64 {
        frame as f64 * self.hop_size as f64 / self.sample_rate as f64
    }
}

pub fn extract(buf: &AudioBuffer) -> PitchTrack {
    let mono    = buf.mono();
    let sr      = buf.sample_rate as f32;
    let window  = 1024usize;
    let hop     = 256usize;
    let min_lag = (sr / 600.0).ceil()  as usize; // 600 Hz upper bound — if your voice goes higher, respect
    let max_lag = (sr / 75.0).floor()  as usize;  // 75 Hz lower bound — below this we're in bass guitar territory drnrnrnr

    let frames: Vec<Option<f32>> = mono
        .windows(window).step_by(hop)
        .map(|frame| acf_f0(frame, min_lag, max_lag, sr))
        .collect();

    PitchTrack { frames, hop_size: hop, sample_rate: buf.sample_rate }
}

fn acf_f0(frame: &[f32], min_lag: usize, max_lag: usize, sr: f32) -> Option<f32> {
    let n      = frame.len();
    let energy: f32 = frame.iter().map(|&s| s * s).sum();
    // silence check — if there's nothing there, we don't need to pretend otherwise
    if energy < 1e-6 { return None; }

    // for each possible lag, compute normalized correlation and find the peak
    let (best_lag, best_corr) = (min_lag..=max_lag.min(n / 2))
        .map(|lag| {
            let corr = frame[..n - lag].iter().zip(&frame[lag..])
                .map(|(&a, &b)| a * b).sum::<f32>() / energy;
            (lag, corr)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .unwrap_or((min_lag, 0.0));

    // empirically reasonable for voiced/unvoiced separation. theoretically: whatever
    if best_corr > 0.45 { Some(sr / best_lag as f32) } else { None }
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

    /// Median voiced F0 across all voiced frames — robust to a few outliers
    /// near the boundaries (where the window straddles startup transients).
    fn median_voiced(track: &PitchTrack) -> Option<f32> {
        let mut hz: Vec<f32> = track.frames.iter().filter_map(|f| *f).collect();
        if hz.is_empty() { return None; }
        hz.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Some(hz[hz.len() / 2])
    }

    #[test]
    fn extract_finds_200hz_sine() {
        let buf = sine(200.0, 16000, 1.0);
        let track = extract(&buf);
        let med = median_voiced(&track).expect("expected voiced frames for a clean sine");
        assert!((med - 200.0).abs() < 5.0, "median F0 was {}, expected ~200", med);
    }

    #[test]
    fn extract_finds_440hz_sine() {
        let buf = sine(440.0, 22050, 0.5);
        let track = extract(&buf);
        let med = median_voiced(&track).expect("expected voiced frames");
        assert!((med - 440.0).abs() < 10.0, "median F0 was {}, expected ~440", med);
    }

    #[test]
    fn extract_finds_low_pitch_near_floor() {
        // 100 Hz is comfortably above the 75 Hz lower bound — male voice territory.
        let buf = sine(100.0, 16000, 1.0);
        let track = extract(&buf);
        let med = median_voiced(&track).expect("expected voiced frames");
        assert!((med - 100.0).abs() < 5.0, "median F0 was {}, expected ~100", med);
    }

    #[test]
    fn extract_marks_silence_as_unvoiced() {
        // pure silence: nothing in there to find. and we admit it.
        let buf = AudioBuffer { samples: vec![0.0; 16000], sample_rate: 16000, channels: 1 };
        let track = extract(&buf);
        assert!(
            track.frames.iter().all(|f| f.is_none()),
            "expected all-None for silence, got {:?}",
            &track.frames[..5.min(track.frames.len())]
        );
    }

    #[test]
    fn extract_marks_white_noise_as_mostly_unvoiced() {
        // Deterministic xorshift noise — doesn't have a periodic structure so
        // the autocorrelation peak shouldn't clear the 0.45 threshold.
        let mut seed: u32 = 0xCAFEF00D;
        let samples: Vec<f32> = (0..16000)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                ((seed as i32) as f32) / (i32::MAX as f32) * 0.3
            })
            .collect();
        let buf = AudioBuffer { samples, sample_rate: 16000, channels: 1 };
        let track = extract(&buf);
        let voiced = track.frames.iter().filter(|f| f.is_some()).count();
        let total = track.frames.len();
        // I'll allow a few stray frames — the ACF is allowed its little hallucinations.
        assert!(
            voiced * 4 < total,
            "noise was reported as voiced in {}/{} frames — expected mostly unvoiced",
            voiced, total
        );
    }

    #[test]
    fn frame_to_sec_is_consistent() {
        let track = PitchTrack { frames: vec![None; 4], hop_size: 256, sample_rate: 16000 };
        assert!((track.frame_to_sec(0) - 0.0).abs() < 1e-12);
        // frame 1 starts 256 samples in → 256/16000 = 0.016 s
        assert!((track.frame_to_sec(1) - 0.016).abs() < 1e-9);
    }

    #[test]
    fn extract_handles_short_buffer_gracefully() {
        // shorter than the analysis window — windows() yields nothing, no panic.
        let buf = AudioBuffer { samples: vec![0.1; 100], sample_rate: 16000, channels: 1 };
        let track = extract(&buf);
        assert!(track.frames.is_empty());
    }
}
