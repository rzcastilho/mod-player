// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Ticker`: a background OS thread that requests a repaint roughly every
//! 33 ms while playback intent is `Playing` (research R12, FR-008/SC-010).
//! `PlaybackController::tick()` only runs from inside `App::ui`, which
//! eframe only calls on a repaint; some platforms stop delivering repaints
//! to a minimized/hidden/occluded window on their own schedule, which would
//! otherwise stall event draining (and so, eventually, transport reactions
//! and position bookkeeping) while playback continues in the background.
//! Audio itself never depends on this thread — only UI-side bookkeeping
//! does.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use egui::Context;

/// Repaint cadence while playing (research R12).
const TICK_INTERVAL: Duration = Duration::from_millis(33);

pub struct Ticker {
    playing: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Ticker {
    /// Spawn the ticker thread bound to `ctx`. Starts idle (`set_playing`
    /// is the caller's job, once per frame).
    pub fn spawn(ctx: Context) -> Self {
        let playing = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_playing = Arc::clone(&playing);
        let thread_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("ui-ticker".to_string())
            .spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    if thread_playing.load(Ordering::Relaxed) {
                        ctx.request_repaint();
                    }
                    thread::sleep(TICK_INTERVAL);
                }
            })
            .ok();
        Self {
            playing,
            stop,
            handle,
        }
    }

    /// Tell the ticker whether playback intent is currently `Playing` —
    /// call once per frame from `App::ui`.
    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Relaxed);
    }
}

impl Drop for Ticker {
    /// Signal the thread to stop and join it, so it never outlives the
    /// `App` (and, with it, the `egui::Context` it holds a clone of).
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_playing_updates_the_flag_the_thread_reads() {
        let ticker = Ticker::spawn(Context::default());
        assert!(!ticker.playing.load(Ordering::Relaxed));
        ticker.set_playing(true);
        assert!(ticker.playing.load(Ordering::Relaxed));
        ticker.set_playing(false);
        assert!(!ticker.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn drop_stops_the_thread_promptly() {
        let ticker = Ticker::spawn(Context::default());
        ticker.set_playing(true);
        drop(ticker);
        // If `drop` didn't join, this test would still pass (it doesn't
        // observe the thread directly) — its real value is under Miri/TSan
        // in CI, which would flag a leaked/still-running thread. Kept as a
        // smoke test that construction+drop never panics or hangs.
    }
}
