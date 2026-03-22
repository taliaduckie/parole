//! Audio playback via cpal.
//! "playback" is generous — this module bravely holds state while waiting for
//! a cpal output stream to be wired up. it's doing great. we love it here.

use parking_lot::Mutex;
use std::sync::Arc;

pub struct AudioPlayer {
    pub position: Arc<Mutex<usize>>,
    pub samples:  Arc<Mutex<Vec<f32>>>,
    pub playing:  bool,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            position: Arc::new(Mutex::new(0)),
            samples:  Arc::new(Mutex::new(vec![])),
            playing:  false,
        }
    }

    pub fn play(&mut self, samples: Vec<f32>, sample_rate: u32) {
        *self.samples.lock()  = samples;
        *self.position.lock() = 0;
        self.playing = true;
        // TODO: wire up cpal output stream
        // (I will wire this up. I'm going to wire this up. Look at me, about to wire this up.)
        log::info!("playback start @ {}Hz", sample_rate);
    }

    // this function has one job and handles it with complete dignity
    pub fn stop(&mut self) { self.playing = false; }

    // unused and it knows it. patiently waiting for the playback cursor to matter.
    pub fn position_secs(&self, sr: u32) -> f64 {
        *self.position.lock() as f64 / sr as f64
    }
}
