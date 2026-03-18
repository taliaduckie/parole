//! Audio playback via cpal.

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
        log::info!("playback start @ {}Hz", sample_rate);
    }

    pub fn stop(&mut self) { self.playing = false; }

    pub fn position_secs(&self, sr: u32) -> f64 {
        *self.position.lock() as f64 / sr as f64
    }
}
