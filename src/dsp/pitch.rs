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

/// User-tweakable knobs for pitch extraction. Window and hop stay internal —
/// users mostly want to tell us where to look for F0, and how strict to be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchSettings {
    pub min_hz:             f32,
    pub max_hz:             f32,
    pub voicing_threshold:  f32,
}

impl Default for PitchSettings {
    fn default() -> Self {
        // 75–600 Hz covers most adult voices comfortably; 0.45 was the
        // pre-existing magic number, which I'm choosing to inherit rather than relitigate.
        Self { min_hz: 75.0, max_hz: 600.0, voicing_threshold: 0.45 }
    }
}

pub fn extract(buf: &AudioBuffer, settings: PitchSettings) -> PitchTrack {
    let mono    = buf.mono();
    let sr      = buf.sample_rate as f32;
    let window  = 1024usize;
    let hop     = 256usize;
    // Defensive clamp: max_hz < min_hz would give an empty lag range and
    // every frame returning None — confusing for the user, prevents the panic
    // that comes from RangeInclusive<usize>::start > end.
    let lo = settings.min_hz.max(1.0);
    let hi = settings.max_hz.max(lo + 1.0);
    let min_lag = (sr / hi).ceil()  as usize;
    let max_lag = (sr / lo).floor() as usize;

    let frames: Vec<Option<f32>> = mono
        .windows(window).step_by(hop)
        .map(|frame| acf_f0(frame, min_lag, max_lag, sr, settings.voicing_threshold))
        .collect();

    PitchTrack { frames, hop_size: hop, sample_rate: buf.sample_rate }
}

fn acf_f0(frame: &[f32], min_lag: usize, max_lag: usize, sr: f32, threshold: f32) -> Option<f32> {
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

    // threshold is the user's call now — voicing is partly opinion anyway
    if best_corr > threshold { Some(sr / best_lag as f32) } else { None }
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
        let track = extract(&buf, PitchSettings::default());
        let med = median_voiced(&track).expect("expected voiced frames for a clean sine");
        assert!((med - 200.0).abs() < 5.0, "median F0 was {}, expected ~200", med);
    }

    #[test]
    fn extract_finds_440hz_sine() {
        let buf = sine(440.0, 22050, 0.5);
        let track = extract(&buf, PitchSettings::default());
        let med = median_voiced(&track).expect("expected voiced frames");
        assert!((med - 440.0).abs() < 10.0, "median F0 was {}, expected ~440", med);
    }

    #[test]
    fn extract_finds_low_pitch_near_floor() {
        // 100 Hz is comfortably above the 75 Hz lower bound — male voice territory.
        let buf = sine(100.0, 16000, 1.0);
        let track = extract(&buf, PitchSettings::default());
        let med = median_voiced(&track).expect("expected voiced frames");
        assert!((med - 100.0).abs() < 5.0, "median F0 was {}, expected ~100", med);
    }

    #[test]
    fn extract_marks_silence_as_unvoiced() {
        // pure silence: nothing in there to find. and we admit it.
        let buf = AudioBuffer { samples: vec![0.0; 16000], sample_rate: 16000, channels: 1 };
        let track = extract(&buf, PitchSettings::default());
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
        let track = extract(&buf, PitchSettings::default());
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
        let track = extract(&buf, PitchSettings::default());
        assert!(track.frames.is_empty());
    }

    #[test]
    fn high_voicing_threshold_marks_more_frames_unvoiced() {
        // Same input, two thresholds: the strict one should kill at least as many frames.
        let buf = sine(200.0, 16000, 1.0);
        let lax = extract(&buf, PitchSettings { voicing_threshold: 0.30, ..Default::default() });
        let strict = extract(&buf, PitchSettings { voicing_threshold: 0.95, ..Default::default() });
        let lax_voiced = lax.frames.iter().filter(|f| f.is_some()).count();
        let strict_voiced = strict.frames.iter().filter(|f| f.is_some()).count();
        assert!(strict_voiced <= lax_voiced,
            "strict threshold somehow kept more voiced frames: strict={}, lax={}",
            strict_voiced, lax_voiced);
    }

    #[test]
    fn pitch_range_outside_target_makes_signal_unvoiced() {
        // 200 Hz sine searched in 400-600 Hz window — should find nothing,
        // because the true period's lag is outside the (min_lag, max_lag) range.
        let buf = sine(200.0, 16000, 1.0);
        let track = extract(&buf, PitchSettings {
            min_hz: 400.0, max_hz: 600.0, voicing_threshold: 0.45,
        });
        let voiced = track.frames.iter().filter(|f| f.is_some()).count();
        // I'll allow a tiny stragglers — but the great majority should be None.
        assert!(voiced * 5 < track.frames.len(),
            "expected mostly-unvoiced for out-of-range sine, got {}/{}",
            voiced, track.frames.len());
    }

    #[test]
    fn extract_does_not_panic_when_min_exceeds_max() {
        // The clamp inside extract should keep us out of the panic zone.
        let buf = sine(200.0, 16000, 0.2);
        let _ = extract(&buf, PitchSettings {
            min_hz: 800.0, max_hz: 100.0, voicing_threshold: 0.45,
        });
    }
}
