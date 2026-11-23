# Parole

A phonetic analysis workbench built in Rust.
The goal is a faster, easier annotation and analysis workflow than Praat.
Parole is still a prototype; see the [roadmap](docs/ROADMAP.md) for what's missing.

## Features (in progress)

- Waveform viewer — zoomable, scrollable, click-drag selection
- Spectrogram — STFT with configurable window/overlap, viridis colour map
- Pitch tracking — autocorrelation F0 extraction
- Formant analysis — LPC-based F1/F2/F3 with overlay on spectrogram
- TextGrid — import Praat text files, view interval and point tiers, edit labels, and export a copy
- Audio playback — play selection or full file

## Working with a TextGrid

1. Open the associated audio file.
2. Choose **Open TextGrid…** and select the annotation file. Both long and
   short Praat text formats are supported, with UTF-8 or BOM-marked UTF-16.
3. Edit labels in **TextGrid annotations**. Times, boundaries and tier
   definitions are read-only for now.
4. Choose **Save TextGrid copy…** and give it a new name. The copy uses Praat's
   long text format; existing files are never overwritten.

If you have a binary TextGrid, open it in Praat and save it as text first.

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

Rust lets Parole run signal processing off the UI thread and ship as a native
desktop app. Performance and measurement agreement with Praat still need
direct benchmarks.
