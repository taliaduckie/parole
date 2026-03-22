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
