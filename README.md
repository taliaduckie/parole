# Parole

A phonetic analysis workbench built in Rust.
Basically spiritual successor to Praat: same capabilities, w dramatically better UX. at least that's the intent.

## Features (in progress)

- Waveform viewer — zoomable, scrollable, click-drag selection
- Spectrogram — STFT with configurable window/overlap, viridis colour map
- Pitch tracking — autocorrelation F0 extraction
- Formant analysis — LPC-based F1/F2/F3 with overlay on spectrogram
- TextGrid — in-progress data model and read-only interval renderer
- Audio playback — play selection or full file

Planned: interactive TextGrid editing and user-facing Lua scripting.

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
cargo build --locked --release
cargo run --locked --release -- path/to/audio.wav
```

To run the checks locally (also run by CI):

```bash
cargo test --locked
cargo clippy --locked --all-targets
```

On Ubuntu, install `libasound2-dev` and `pkg-config` first. CI runs the locked checks on stable Rust on Linux and macOS.

## Why Rust?

Praat's DSP is fast because it's C++. Python-based alternatives
(parselmouth, librosa) are too slow for interactive real-time rendering.
Rust gives the same performance with memory safety and a modern
ecosystem. Yay Rust! 
