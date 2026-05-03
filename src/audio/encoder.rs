//! File encoders for recorded audio. Pure I/O: take samples + sample rate,
//! produce a file on disk. No UI, no app state.

use anyhow::{anyhow, Context, Result};
use std::path::Path;

/// Write mono f32 samples to a 32-bit float WAV file.
pub fn write_wav_mono_f32(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("Could not create WAV file at {:?}", path))?;
    for &s in samples {
        writer.write_sample(s).context("WAV write error")?;
    }
    writer.finalize().context("WAV finalize error")?;
    Ok(())
}

/// Encode mono f32 samples as MP3 (CBR 128 kbps) and write to disk.
pub fn write_mp3_mono(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    use mp3lame_encoder::{Builder, FlushNoGap, MonoPcm};

    let mut builder = Builder::new().ok_or_else(|| anyhow!("Failed to create MP3 encoder"))?;
    builder
        .set_num_channels(1)
        .map_err(|e| anyhow!("MP3 channels: {:?}", e))?;
    builder
        .set_sample_rate(sample_rate)
        .map_err(|e| anyhow!("MP3 sample rate: {:?}", e))?;
    builder
        .set_brate(mp3lame_encoder::Bitrate::Kbps128)
        .map_err(|e| anyhow!("MP3 bitrate: {:?}", e))?;
    builder
        .set_quality(mp3lame_encoder::Quality::Best)
        .map_err(|e| anyhow!("MP3 quality: {:?}", e))?;

    let mut encoder = builder
        .build()
        .map_err(|e| anyhow!("Failed to build MP3 encoder: {:?}", e))?;

    let pcm: Vec<i16> = samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect();

    // The encoder writes into the Vec's spare capacity — it does NOT grow it.
    // Reserve enough for both the encode and the final flush (which needs ≥7200 B).
    let mut mp3_bytes: Vec<u8> =
        Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(pcm.len()));
    encoder
        .encode_to_vec(MonoPcm(&pcm), &mut mp3_bytes)
        .map_err(|e| anyhow!("MP3 encode failed: {:?}", e))?;
    mp3_bytes.reserve(7200);
    encoder
        .flush_to_vec::<FlushNoGap>(&mut mp3_bytes)
        .map_err(|e| anyhow!("MP3 flush failed: {:?}", e))?;

    std::fs::write(path, &mp3_bytes)
        .with_context(|| format!("Could not write MP3 to {:?}", path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::loader::load_audio;

    fn sine(freq: f32, sample_rate: u32, secs: f32) -> Vec<f32> {
        let n = (sample_rate as f32 * secs) as usize;
        (0..n)
            .map(|i| (i as f32 * freq * 2.0 * std::f32::consts::PI / sample_rate as f32).sin() * 0.5)
            .collect()
    }

    #[test]
    fn wav_roundtrip_preserves_samples_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.wav");
        let samples = sine(440.0, 44100, 0.1);

        write_wav_mono_f32(&path, &samples, 44100).unwrap();
        let loaded = load_audio(&path).unwrap();

        assert_eq!(loaded.sample_rate, 44100);
        assert_eq!(loaded.channels, 1);
        assert_eq!(loaded.samples.len(), samples.len());
        // 32-bit float WAV is exact.
        for (a, b) in samples.iter().zip(loaded.samples.iter()) {
            assert!((a - b).abs() < 1e-6, "{} vs {}", a, b);
        }
    }

    #[test]
    fn wav_roundtrip_preserves_sample_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rate.wav");
        write_wav_mono_f32(&path, &sine(220.0, 48000, 0.05), 48000).unwrap();
        let loaded = load_audio(&path).unwrap();
        assert_eq!(loaded.sample_rate, 48000);
    }

    #[test]
    fn wav_roundtrip_handles_empty_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.wav");
        write_wav_mono_f32(&path, &[], 44100).unwrap();
        let loaded = load_audio(&path).unwrap();
        assert!(loaded.samples.is_empty());
        assert_eq!(loaded.sample_rate, 44100);
    }

    #[test]
    fn wav_clamping_is_caller_concern_not_encoder() {
        // Float WAV stores out-of-range values verbatim — sanity-check that we
        // don't silently clamp inside write_wav_mono_f32.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("loud.wav");
        write_wav_mono_f32(&path, &[2.0, -2.0, 0.5], 44100).unwrap();
        let loaded = load_audio(&path).unwrap();
        assert!((loaded.samples[0] - 2.0).abs() < 1e-6);
        assert!((loaded.samples[1] + 2.0).abs() < 1e-6);
    }

    #[test]
    fn mp3_writes_a_real_mp3_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.mp3");
        write_mp3_mono(&path, &sine(440.0, 44100, 0.2), 44100).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 100, "MP3 file suspiciously small: {} bytes", bytes.len());

        // Find an MP3 frame sync marker (0xFFE_) somewhere in the file.
        // ID3 tags or filler may precede the first frame, so don't insist on offset 0.
        let has_sync = bytes
            .windows(2)
            .any(|w| w[0] == 0xFF && (w[1] & 0xE0) == 0xE0);
        assert!(has_sync, "no MP3 frame sync byte pair found");
    }

    #[test]
    fn mp3_handles_clipping_input_without_panicking() {
        // Inputs outside [-1, 1] must be clamped before i16 conversion to avoid wrap.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.mp3");
        let samples: Vec<f32> = (0..4410).map(|i| if i % 2 == 0 { 5.0 } else { -5.0 }).collect();
        write_mp3_mono(&path, &samples, 44100).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }

    #[test]
    fn mp3_decodes_via_symphonia_probe() {
        // Lighter-weight than full decode: just confirm symphonia recognises the format.
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;
        use symphonia::core::probe::Hint;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("probe.mp3");
        write_mp3_mono(&path, &sine(440.0, 44100, 0.3), 44100).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .expect("symphonia could not probe our MP3");
        let track = probed.format.default_track().expect("no track in MP3");
        assert_eq!(track.codec_params.sample_rate.unwrap(), 44100);
    }
}
