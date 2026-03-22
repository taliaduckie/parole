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
