//! Formant extraction via LPC + polynomial roots.
//! Root-finding via companion matrix eigenvalues (TODO: implement).

use crate::audio::loader::AudioBuffer;

#[derive(Clone)]
pub struct FormantFrame {
    pub f1: Option<f32>,
    pub f2: Option<f32>,
    pub f3: Option<f32>,
}

pub struct FormantTrack {
    pub frames:      Vec<FormantFrame>,
    pub hop_size:    usize,
    pub sample_rate: u32,
}

pub fn extract(buf: &AudioBuffer) -> FormantTrack {
    let mono      = buf.mono();
    let sr        = buf.sample_rate;
    let window    = 512usize;
    let hop       = 128usize;
    let lpc_order = (2 + sr / 1000) as usize;

    let frames: Vec<FormantFrame> = mono
        .windows(window).step_by(hop)
        .map(|frame| { let _c = lpc(frame, lpc_order); FormantFrame { f1: None, f2: None, f3: None } })
        .collect();

    FormantTrack { frames, hop_size: hop, sample_rate: sr }
}

fn lpc(frame: &[f32], order: usize) -> Vec<f32> {
    let n = frame.len();
    let r: Vec<f32> = (0..=order)
        .map(|lag| frame[..n-lag].iter().zip(&frame[lag..]).map(|(&a,&b)| a*b).sum())
        .collect();
    let mut a = vec![0.0f32; order + 1];
    let mut e = r[0]; a[0] = 1.0;
    for i in 1..=order {
        let lam = -r[1..=i].iter().zip(a[0..i].iter().rev()).map(|(&ri,&ai)| ri*ai).sum::<f32>() / e;
        let prev = a.clone();
        for j in 1..=i { a[j] = prev[j] + lam * prev[i-j]; }
        a[i] = lam; e *= 1.0 - lam * lam;
    }
    a
}
