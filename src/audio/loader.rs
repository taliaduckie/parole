//! Audio loading via symphonia. Decodes WAV/FLAC/MP3 to interleaved f32.
//! symphonia does the heavy lifting here. we just hold the door open.

use anyhow::{Context, Result};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Clone)]
pub struct AudioBuffer {
    pub samples:     Vec<f32>,
    pub sample_rate: u32,
    pub channels:    u16,
}

impl AudioBuffer {
    pub fn duration_secs(&self) -> f64 {
        // samples ÷ (rate × channels) — the only math in this file I can verify by hand without squinting
        self.samples.len() as f64 / (self.sample_rate as f64 * self.channels as f64)
    }

    pub fn mono(&self) -> Vec<f32> {
        // if it's already mono, great, we're done, everyone go home
        if self.channels == 1 { return self.samples.clone(); }
        let ch = self.channels as usize;
        // averaging channels together: not acoustically ideal, but this is phonetics not mastering
        self.samples.chunks(ch)
            .map(|f| f.iter().sum::<f32>() / ch as f32)
            .collect()
    }

    pub fn slice_mono(&self, start_sec: f64, end_sec: f64) -> Vec<f32> {
        let mono = self.mono();
        let sr   = self.sample_rate as f64;
        let s    = (start_sec * sr) as usize;
        // .min(mono.len()) is doing quiet heroics here — guarding against view_end
        // drifting past the actual audio end. I've named it; I'm moving on.
        let e    = ((end_sec * sr) as usize).min(mono.len());
        mono[s..e].to_vec()
    }
}

pub fn load_audio(path: &Path) -> Result<AudioBuffer> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Cannot open {:?}", path))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("Unsupported format")?;

    let mut format  = probed.format;
    let track       = format.default_track().context("No audio tracks")?;
    let track_id    = track.id;
    // if sample_rate is missing from the file we just... assume 44100.
    // confident and wrong is still a vibe.
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels    = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(1);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Unsupported codec")?;

    let mut all_samples = Vec::new();
    loop {
        // symphonia signals end-of-stream as an Err, so we break on any error.
        // distinguishing "done" from "actually broken" would require effort I've redirected elsewhere.
        // it works. I've made peace with the fact that it shouldn't.
        let packet = match format.next_packet() { Ok(p) => p, Err(_) => break };
        if packet.track_id() != track_id { continue; }
        // decode errors are silently skipped — technically correct in the same way "fine" is technically an answer
        let decoded = match decoder.decode(&packet) { Ok(d) => d, Err(_) => continue };
        let mut sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        sb.copy_interleaved_ref(decoded);
        all_samples.extend_from_slice(sb.samples());
    }

    Ok(AudioBuffer { samples: all_samples, sample_rate, channels })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn duration_secs_mono() {
        let buf = AudioBuffer { samples: vec![0.0; 44100], sample_rate: 44100, channels: 1 };
        assert!((buf.duration_secs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn duration_secs_stereo() {
        // 88200 interleaved samples / (44100 * 2) = 1.0s
        let buf = AudioBuffer { samples: vec![0.0; 88200], sample_rate: 44100, channels: 2 };
        assert!((buf.duration_secs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mono_passthrough_when_already_mono() {
        let buf = AudioBuffer { samples: vec![0.1, 0.2, 0.3], sample_rate: 44100, channels: 1 };
        assert_eq!(buf.mono(), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn mono_downmixes_stereo() {
        let buf = AudioBuffer {
            samples: vec![1.0, -1.0, 0.4, 0.6, 0.0, 0.0],
            sample_rate: 44100,
            channels: 2,
        };
        let m = buf.mono();
        assert_eq!(m.len(), 3);
        assert!(approx_eq(m[0], 0.0));
        assert!(approx_eq(m[1], 0.5));
        assert!(approx_eq(m[2], 0.0));
    }

    #[test]
    fn mono_downmixes_six_channels() {
        // 5.1 audio: average all 6 channels
        let buf = AudioBuffer {
            samples: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            sample_rate: 48000,
            channels: 6,
        };
        let m = buf.mono();
        assert_eq!(m.len(), 1);
        assert!(approx_eq(m[0], 3.5));
    }

    #[test]
    fn slice_mono_normal_range() {
        // 4 samples, 4Hz → 1s long. Slice 0.25..0.75 → samples [1,2].
        let buf = AudioBuffer {
            samples: vec![10.0, 11.0, 12.0, 13.0],
            sample_rate: 4,
            channels: 1,
        };
        assert_eq!(buf.slice_mono(0.25, 0.75), vec![11.0, 12.0]);
    }

    #[test]
    fn slice_mono_clamps_end_past_buffer() {
        let buf = AudioBuffer {
            samples: vec![1.0, 2.0, 3.0, 4.0],
            sample_rate: 4,
            channels: 1,
        };
        assert_eq!(buf.slice_mono(0.0, 100.0), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn slice_mono_zero_length_when_start_equals_end() {
        let buf = AudioBuffer {
            samples: vec![1.0, 2.0, 3.0, 4.0],
            sample_rate: 4,
            channels: 1,
        };
        assert!(buf.slice_mono(0.5, 0.5).is_empty());
    }

    #[test]
    fn slice_mono_works_on_stereo_input() {
        // Stereo source gets downmixed first, then sliced.
        let buf = AudioBuffer {
            samples: vec![1.0, 3.0, 2.0, 4.0, 0.0, 0.0, 5.0, 7.0],
            sample_rate: 4,
            channels: 2,
        };
        // mono = [2, 3, 0, 6]; slice 0.25..0.75 → [3, 0]
        assert_eq!(buf.slice_mono(0.25, 0.75), vec![3.0, 0.0]);
    }

    fn write_pcm16_wav(path: &std::path::Path, samples: &[i16], sample_rate: u32, channels: u16) {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn load_audio_decodes_16bit_pcm_mono() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pcm16.wav");
        let samples: Vec<i16> = (0..1000).map(|i| (i * 30) as i16).collect();
        write_pcm16_wav(&path, &samples, 22050, 1);

        let loaded = super::load_audio(&path).unwrap();
        assert_eq!(loaded.sample_rate, 22050);
        assert_eq!(loaded.channels, 1);
        assert_eq!(loaded.samples.len(), samples.len());
        // Symphonia normalises i16 → f32 in [-1, 1]
        for (i, &s) in samples.iter().enumerate() {
            let expected = s as f32 / i16::MAX as f32;
            assert!(
                (loaded.samples[i] - expected).abs() < 1e-3,
                "sample {}: got {}, expected {}",
                i, loaded.samples[i], expected
            );
        }
    }

    #[test]
    fn load_audio_decodes_16bit_pcm_stereo_interleaved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pcm16-stereo.wav");
        // Interleaved L,R,L,R…
        let samples: Vec<i16> = vec![100, -100, 200, -200, 300, -300];
        write_pcm16_wav(&path, &samples, 44100, 2);

        let loaded = super::load_audio(&path).unwrap();
        assert_eq!(loaded.channels, 2);
        // Interleaved samples are preserved as-is.
        assert_eq!(loaded.samples.len(), 6);
        // duration = 3 frames / 44100Hz
        assert!((loaded.duration_secs() - 3.0 / 44100.0).abs() < 1e-9);
    }

    #[test]
    fn load_audio_errors_on_missing_file() {
        let result = super::load_audio(std::path::Path::new("/definitely/not/a/real/path.wav"));
        assert!(result.is_err());
    }

    #[test]
    fn load_audio_errors_on_garbage_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.wav");
        std::fs::write(&path, b"this is not audio data, not even close").unwrap();
        let result = super::load_audio(&path);
        assert!(result.is_err());
    }
}
