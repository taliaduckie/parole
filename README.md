# Praatly

A modern phonetic analysis workbench built in Rust.
Spiritual successor to Praat: same capabilities, dramatically better UX.

## Features (in progress)

- Waveform viewer — zoomable, scrollable, click-drag selection
- Spectrogram — STFT with configurable window/overlap, viridis colour map
- Pitch tracking — autocorrelation F0 extraction
- Formant analysis — LPC-based F1/F2/F3 with overlay on spectrogram
- TextGrid annotation — interval and point tiers, keyboard-driven
- Audio playback — play selection or full file
- Scripting — Lua 5.4 scripting layer (replacing Praat's bespoke language)

## Tech stack

| Concern        | Crate          |
|----------------|----------------|
| GUI            | egui + eframe  |
| Audio I/O      | cpal           |
| Audio decoding | symphonia      |
| FFT / DSP      | rustfft        |
| Scripting      | mlua (Lua 5.4) |
| Serialization  | serde_json     |

## Build

```bash
cargo build --release
cargo run --release -- path/to/audio.wav
```

## Why Rust?

Praat's DSP is fast because it's C++. Python-based alternatives
(parselmouth, librosa) are too slow for interactive real-time rendering.
Rust gives us the same performance with memory safety and a modern
ecosystem.
