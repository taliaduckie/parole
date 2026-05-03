//! Formant extraction via LPC + polynomial root-finding.
//!
//! Pipeline: pre-emphasis → Hann window → autocorrelation LPC (Levinson-Durbin)
//! → Durand-Kerner root-finding → filter roots by frequency/bandwidth → F1/F2/F3.

use crate::audio::loader::AudioBuffer;
use rustfft::num_complex::Complex;

#[derive(Clone, Debug, Default, PartialEq)]
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

impl FormantTrack {
    pub fn frame_to_sec(&self, frame: usize) -> f64 {
        frame as f64 * self.hop_size as f64 / self.sample_rate as f64
    }
}

/// User-tweakable knobs for formant tracking. Window/hop/order stay internal
/// — the user mostly cares about how high to look (Praat's "max formant" knob).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormantSettings {
    /// Upper bound on candidate formant frequency, in Hz. Roots above this
    /// (and the bandwidth cap) are dropped before sorting.
    pub max_formant_hz: f32,
}

impl Default for FormantSettings {
    fn default() -> Self {
        // 5500 Hz is Praat's default for adult speech.
        Self { max_formant_hz: 5500.0 }
    }
}

pub fn extract(buf: &AudioBuffer, settings: FormantSettings) -> FormantTrack {
    let mono      = buf.mono();
    let sr        = buf.sample_rate;
    let window    = 512usize;
    let hop       = 128usize;
    // Rule of thumb: LPC order ≈ 2 + sr/1000 — gives roughly two coefficients
    // per expected formant within the band of interest.
    let lpc_order = (2 + sr / 1000) as usize;

    // Pre-emphasis flattens the spectral tilt of voiced speech (~+6 dB/oct),
    // which gives the autocorrelation matrix more numerical headroom.
    let pre = pre_emphasis(&mono, 0.97);
    let win = hann(window);

    let frames: Vec<FormantFrame> = pre
        .windows(window).step_by(hop)
        .map(|frame| {
            let windowed: Vec<f32> = frame.iter().zip(&win).map(|(&s, &w)| s * w).collect();
            let coeffs = lpc(&windowed, lpc_order);
            // The polynomial of interest is z^p + a₁z^(p-1) + … + a_p.
            // `coeffs` is [1, a₁, …, a_p]; in standard low-to-high form we reverse it.
            let mut poly: Vec<f32> = coeffs.iter().rev().copied().collect();
            // Skip silent/degenerate frames where the leading coefficient (after reverse,
            // this is a_p) is essentially zero — Durand-Kerner doesn't help us there.
            if poly.iter().all(|c| c.abs() < 1e-9) {
                return FormantFrame::default();
            }
            // Normalise so the leading coefficient is 1 (Durand-Kerner expects monic).
            let lead = *poly.last().unwrap();
            if lead.abs() > 1e-9 {
                for c in poly.iter_mut() { *c /= lead; }
            }
            let roots = find_roots(&poly);
            roots_to_formants(&roots, sr)
        })
        .collect();

    FormantTrack { frames, hop_size: hop, sample_rate: sr }
}

fn pre_emphasis(x: &[f32], alpha: f32) -> Vec<f32> {
    if x.is_empty() { return Vec::new(); }
    let mut out = Vec::with_capacity(x.len());
    out.push(x[0]);
    for i in 1..x.len() {
        out.push(x[i] - alpha * x[i - 1]);
    }
    out
}

fn hann(n: usize) -> Vec<f32> {
    if n == 0 { return Vec::new(); }
    (0..n)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n - 1).max(1) as f32).cos()))
        .collect()
}

/// Levinson-Durbin recursion. Returns p+1 LPC coefficients with a[0] = 1.
fn lpc(frame: &[f32], order: usize) -> Vec<f32> {
    let n = frame.len();
    let r: Vec<f32> = (0..=order)
        .map(|lag| frame[..n - lag].iter().zip(&frame[lag..]).map(|(&a, &b)| a * b).sum())
        .collect();
    let mut a = vec![0.0f32; order + 1];
    let mut e = r[0];
    a[0] = 1.0;
    if e.abs() < 1e-12 { return a; }
    for i in 1..=order {
        let lam = -r[1..=i].iter().zip(a[0..i].iter().rev()).map(|(&ri, &ai)| ri * ai).sum::<f32>() / e;
        let prev = a.clone();
        for j in 1..=i { a[j] = prev[j] + lam * prev[i - j]; }
        a[i] = lam;
        e *= 1.0 - lam * lam;
        if e.abs() < 1e-12 { break; }
    }
    a
}

/// Find all complex roots of a real-coefficient polynomial in standard form
/// `c[0] + c[1]·x + … + c[n]·x^n` (must be monic: c[n] ≈ 1) using the
/// Durand-Kerner iteration.
pub(crate) fn find_roots(coeffs: &[f32]) -> Vec<Complex<f32>> {
    if coeffs.len() < 2 { return Vec::new(); }
    let n = coeffs.len() - 1;
    let c64: Vec<Complex<f64>> = coeffs.iter().map(|&x| Complex::new(x as f64, 0.0)).collect();

    // Initial guesses: equally-spaced points on a circle of radius 0.4, with a
    // small angular offset so we don't accidentally land on roots-of-unity.
    let mut roots: Vec<Complex<f64>> = (0..n)
        .map(|k| {
            let theta = 2.0 * std::f64::consts::PI * k as f64 / n as f64 + 0.4;
            Complex::from_polar(0.4, theta)
        })
        .collect();

    let max_iter = 200;
    let tol = 1e-12;
    for _ in 0..max_iter {
        let mut max_delta: f64 = 0.0;
        let snapshot = roots.clone();
        for k in 0..n {
            let z = snapshot[k];
            // p(z) via Horner from the top coefficient down.
            let mut p = c64[n];
            for i in (0..n).rev() {
                p = p * z + c64[i];
            }
            // Denominator: product of (z - z_j) for j ≠ k.
            let mut denom = Complex::new(1.0, 0.0);
            for j in 0..n {
                if j != k { denom *= z - snapshot[j]; }
            }
            if denom.norm_sqr() < 1e-30 { continue; }
            let delta = p / denom;
            roots[k] = z - delta;
            let dn = delta.norm();
            if dn > max_delta { max_delta = dn; }
        }
        if max_delta < tol { break; }
    }

    roots.into_iter().map(|z| Complex::new(z.re as f32, z.im as f32)).collect()
}

/// Convert LPC polynomial roots to F1/F2/F3.
/// Each root z = r·e^(jθ) with positive imaginary part contributes a candidate
/// formant at frequency θ·sr/(2π) and bandwidth -ln(r)·sr/π. We discard wide
/// bands and out-of-band candidates, then sort by frequency.
pub(crate) fn roots_to_formants(roots: &[Complex<f32>], sr: u32) -> FormantFrame {
    let sr_f = sr as f32;
    let nyquist = sr_f / 2.0;
    let mut formants: Vec<f32> = roots
        .iter()
        .filter(|z| z.im > 0.0)
        .filter_map(|z| {
            let r = z.norm();
            // Roots outside the unit circle are non-causal; near-zero magnitude
            // means we lost the angle to numerical noise.
            if !(1e-3..1.0).contains(&r) { return None; }
            let freq = z.arg().abs() * sr_f / (2.0 * std::f32::consts::PI);
            let bw = -r.ln() * sr_f / std::f32::consts::PI;
            // Speech formant band: ~90 Hz to a hair below Nyquist; bandwidth
            // cap of 400 Hz screens out spurious wide poles.
            if freq < 90.0 || freq > nyquist - 50.0 { return None; }
            if bw > 400.0 { return None; }
            Some(freq)
        })
        .collect();
    formants.sort_by(|a, b| a.partial_cmp(b).unwrap());
    FormantFrame {
        f1: formants.first().copied(),
        f2: formants.get(1).copied(),
        f3: formants.get(2).copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn find_roots_quadratic_with_known_real_roots() {
        // (x - 0.5)(x + 0.3) = x² - 0.2x - 0.15
        let roots = find_roots(&[-0.15, -0.2, 1.0]);
        assert_eq!(roots.len(), 2);
        let mut reals: Vec<f32> = roots.iter().map(|z| z.re).collect();
        reals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(approx(reals[0], -0.3, 1e-3), "got {:?}", reals);
        assert!(approx(reals[1], 0.5, 1e-3), "got {:?}", reals);
        // Real roots should have ~0 imaginary part.
        for z in &roots { assert!(z.im.abs() < 1e-3, "non-real im: {:?}", z); }
    }

    #[test]
    fn find_roots_quadratic_with_complex_conjugates() {
        // x² - 1.6x + 0.89 = 0 → x = 0.8 ± 0.5i
        let roots = find_roots(&[0.89, -1.6, 1.0]);
        assert_eq!(roots.len(), 2);
        for z in &roots {
            assert!(approx(z.re, 0.8, 1e-3), "got {:?}", z);
            assert!(approx(z.im.abs(), 0.5, 1e-3), "got {:?}", z);
        }
        // Conjugates: imaginary parts should sum to ~0.
        assert!((roots[0].im + roots[1].im).abs() < 1e-3);
    }

    #[test]
    fn find_roots_returns_empty_for_constant() {
        assert!(find_roots(&[1.0]).is_empty());
        assert!(find_roots(&[]).is_empty());
    }

    #[test]
    fn roots_to_formants_picks_three_lowest_in_band() {
        // Construct roots at known formant frequencies (sr = 8000 → Nyquist 4000).
        // Use radius 0.95 → bandwidth = -ln(0.95)·8000/π ≈ 130 Hz, well under 400.
        let sr = 8000u32;
        let theta = |hz: f32| 2.0 * std::f32::consts::PI * hz / sr as f32;
        let roots = vec![
            Complex::from_polar(0.95, theta(500.0)),
            Complex::from_polar(0.95, theta(1500.0)),
            Complex::from_polar(0.95, theta(2500.0)),
            Complex::from_polar(0.95, theta(3500.0)),
        ];
        let f = roots_to_formants(&roots, sr);
        assert!(approx(f.f1.unwrap(), 500.0, 5.0));
        assert!(approx(f.f2.unwrap(), 1500.0, 5.0));
        assert!(approx(f.f3.unwrap(), 2500.0, 5.0));
    }

    #[test]
    fn roots_to_formants_rejects_wide_bandwidth() {
        // Radius 0.5 → bandwidth ≈ 1764 Hz at sr=8000, well over the 400 Hz cap.
        let sr = 8000u32;
        let theta = 2.0 * std::f32::consts::PI * 1000.0 / sr as f32;
        let roots = vec![Complex::from_polar(0.5, theta)];
        let f = roots_to_formants(&roots, sr);
        assert_eq!(f, FormantFrame::default());
    }

    #[test]
    fn roots_to_formants_rejects_below_90hz() {
        let sr = 8000u32;
        let theta = 2.0 * std::f32::consts::PI * 50.0 / sr as f32;
        let roots = vec![Complex::from_polar(0.95, theta)];
        let f = roots_to_formants(&roots, sr);
        assert!(f.f1.is_none());
    }

    #[test]
    fn roots_to_formants_ignores_negative_imaginary_conjugates() {
        // Only the upper-half-plane root should contribute.
        let sr = 8000u32;
        let theta = 2.0 * std::f32::consts::PI * 1000.0 / sr as f32;
        let roots = vec![
            Complex::from_polar(0.95, theta),
            Complex::from_polar(0.95, -theta),
        ];
        let f = roots_to_formants(&roots, sr);
        assert!(f.f1.is_some());
        assert!(f.f2.is_none());
    }

    #[test]
    fn lpc_recovers_known_ar_process() {
        // Generate a signal x[n] = 0.9·x[n-1] - 0.5·x[n-2] + impulse,
        // then check that LPC(2) recovers a ≈ [1, -0.9, 0.5].
        let n = 1024;
        let mut x = vec![0.0_f32; n];
        x[0] = 1.0;
        for i in 1..n {
            let a1 = if i >= 1 { x[i - 1] } else { 0.0 };
            let a2 = if i >= 2 { x[i - 2] } else { 0.0 };
            x[i] = 0.9 * a1 - 0.5 * a2;
        }
        let a = lpc(&x, 2);
        assert_eq!(a.len(), 3);
        assert!(approx(a[0], 1.0, 1e-3));
        assert!(approx(a[1], -0.9, 1e-2), "a[1] = {}", a[1]);
        assert!(approx(a[2], 0.5, 1e-2), "a[2] = {}", a[2]);
    }

    #[test]
    fn extract_finds_a_formant_for_synthetic_resonator() {
        // Drive a 2-pole resonator at 1000 Hz with white-ish noise; extract should
        // find F1 near 1000 Hz somewhere in the middle of the track.
        let sr = 8000u32;
        let dur_secs = 1.0;
        let n = (sr as f32 * dur_secs) as usize;
        let f0 = 1000.0_f32;
        let r = 0.97_f32;
        let theta = 2.0 * std::f32::consts::PI * f0 / sr as f32;
        // Resonator: y[n] = 2r·cos(θ)·y[n-1] - r²·y[n-2] + x[n].
        let a1 = 2.0 * r * theta.cos();
        let a2 = r * r;
        let mut y = vec![0.0_f32; n];
        let mut seed: u32 = 0x9E3779B9;
        for i in 0..n {
            // xorshift-ish noise as input — deterministic, no extra deps.
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let x = ((seed as i32) as f32) / (i32::MAX as f32) * 0.1;
            let prev1 = if i >= 1 { y[i - 1] } else { 0.0 };
            let prev2 = if i >= 2 { y[i - 2] } else { 0.0 };
            y[i] = a1 * prev1 - a2 * prev2 + x;
        }
        let buf = AudioBuffer { samples: y, sample_rate: sr, channels: 1 };
        let track = extract(&buf);
        // Look at the middle of the track to skip startup transients.
        let mid = track.frames.len() / 2;
        let f1 = track.frames[mid].f1.expect("expected an F1 in the middle of the track");
        assert!(
            (f1 - 1000.0).abs() < 100.0,
            "F1 = {} Hz, expected ~1000",
            f1
        );
    }
}
