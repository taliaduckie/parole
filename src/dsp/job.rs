//! A "do this DSP work somewhere else, and tell me when you're done" handle.
//!
//! Spawns a worker thread, gives you back a receiver to poll, and pings egui
//! for a repaint when the result lands. The worker is fire-and-forget — if
//! the job is dropped before completion, the result silently goes to the void.
//! (cancellation would require the DSP functions to poll a flag — saving that
//! ambition for a future me with more time and fewer feelings about it)

use std::sync::mpsc::{channel, Receiver, TryRecvError};

pub struct DspJob<T> {
    rx: Receiver<T>,
}

impl<T: Send + 'static> DspJob<T> {
    /// Run `f` on a worker thread. When it finishes, the result is sent back
    /// and `ctx` is poked so the UI wakes up to consume it.
    pub fn spawn<F>(ctx: eframe::egui::Context, f: F) -> Self
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = f();
            // If the receiver has been dropped (user loaded another file),
            // send fails silently. That's fine — the worker's quiet way of
            // accepting that nobody wanted its output anymore.
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        Self { rx }
    }

    /// Non-blocking. Returns Some(result) on the first successful poll;
    /// after that the channel is empty and this returns None forever.
    pub fn poll(&self) -> Option<T> {
        match self.rx.try_recv() {
            Ok(v) => Some(v),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    #[test]
    fn job_returns_result_eventually() {
        let ctx = egui::Context::default();
        let job = DspJob::spawn(ctx, || 42_i32);
        // poll until ready — should be near-instant for trivial work, but no
        // sleep loop because that's how flakes are born. small bounded retries.
        for _ in 0..1000 {
            if let Some(v) = job.poll() {
                assert_eq!(v, 42);
                return;
            }
            std::thread::yield_now();
        }
        panic!("job never produced a result");
    }

    #[test]
    fn job_poll_returns_none_after_consumed() {
        let ctx = egui::Context::default();
        let job = DspJob::spawn(ctx, || "done".to_string());
        for _ in 0..1000 {
            if job.poll().is_some() { break; }
            std::thread::yield_now();
        }
        // After the first successful poll the channel is drained.
        assert!(job.poll().is_none());
    }

    #[test]
    fn dropping_job_does_not_panic_worker() {
        let ctx = egui::Context::default();
        // Worker runs, tries to send into a dropped channel, send fails silently.
        // We can't observe the worker directly but we can verify no panic
        // propagates here and the test exits cleanly.
        for _ in 0..50 {
            let _job = DspJob::spawn(ctx.clone(), || vec![0_u8; 1024]);
            // immediately drop — receiver gone before sender writes
        }
    }
}
