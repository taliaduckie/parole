//! synth sine → WAV → load → DSP
//! unit tests use in-memory AudioBuffers — this is the only thing that
//! crosses the encode/decode boundary

use parole::audio::{encoder::write_wav_mono_f32, loader::load_audio};
use parole::dsp::{
    formants::{self, FormantSettings},
    pitch::{self, PitchSettings},
    spectrogram,
};

fn sine(freq: f32, sample_rate: u32, secs: f32) -> Vec<f32> {
    let n = (sample_rate as f32 * secs) as usize;
    (0..n)
        .map(|i| (i as f32 * freq * 2.0 * std::f32::consts::PI / sample_rate as f32).sin() * 0.5)
        .collect()
}

fn argmax(frame: &[f32]) -> usize {
    frame
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap()
}

#[test]
fn wav_roundtrip_through_full_dsp_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sine200.wav");

    let sample_rate = 16_000u32;
    let samples = sine(200.0, sample_rate, 1.0);
    write_wav_mono_f32(&path, &samples, sample_rate).unwrap();

    let loaded = load_audio(&path).expect("synthesized WAV should load cleanly");
    assert_eq!(loaded.sample_rate, sample_rate);
    assert_eq!(loaded.channels, 1);
    assert_eq!(loaded.samples.len(), samples.len());

    // bin spacing = 16000/1024 ≈ 15.6 Hz → 200 Hz lands at bin ~13
    let spec = spectrogram::compute(&loaded, 1024, 0.5);
    assert!(spec.n_frames() > 0, "expected at least one STFT frame");
    let mid = &spec.magnitudes[spec.n_frames() / 2];
    let peak = argmax(mid);
    let expected_bin = (200.0_f32 * 1024.0 / sample_rate as f32).round() as usize;
    assert!(
        (peak as i32 - expected_bin as i32).abs() <= 1,
        "spectrogram peak bin = {}, expected ~{}",
        peak, expected_bin
    );

    let track = pitch::extract(&loaded, PitchSettings::default());
    let mut voiced: Vec<f32> = track.frames.iter().filter_map(|f| *f).collect();
    assert!(!voiced.is_empty(), "expected voiced frames for a clean sine");
    voiced.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = voiced[voiced.len() / 2];
    assert!(
        (median - 200.0).abs() < 5.0,
        "median F0 = {} Hz, expected ~200",
        median
    );

    // pure sine is a bad formant target, but extract should still produce frames
    let f = formants::extract(&loaded, FormantSettings::default());
    assert!(!f.frames.is_empty(), "formant track should have frames");
}
