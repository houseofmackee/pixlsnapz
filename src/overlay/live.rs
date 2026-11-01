//! Live mode (⇧F): the screen is captured again and again while measuring,
//! so edges follow animations, video, and scrolling instead of the snapshot
//! taken when measuring started. Captures run on a background thread; a
//! main-thread timer swaps each new set of screenshots in.

use std::ptr::NonNull;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2_app_kit::NSImage;
use objc2_foundation::NSTimer;

use super::*;
use crate::capture::{self, Shot};

/// How often the timer checks for new screenshots.
const TICK: f64 = 0.05;
/// The least time between the start of two captures.
const INTERVAL: Duration = Duration::from_millis(250);

/// Screenshots moving from the capture thread to the main thread. A `Shot`
/// holds pixels and an immutable `CGImage`, both safe to hand over.
struct Shots(anyhow::Result<Vec<Shot>>);
// SAFETY: see the type's documentation.
unsafe impl Send for Shots {}

/// The capture in progress, and when it started.
pub(super) struct Capturing {
    rx: Receiver<Shots>,
    started: Instant,
}

/// ⇧F: live mode on or off.
pub(super) fn toggle_live() {
    let on = SESSION.with(|s| s.borrow().as_ref().is_some_and(|s| !s.live));
    update(|s| {
        s.live = on;
        s.capturing = None;
        s.toast(if on {
            t!("Live: edges follow the screen as it changes")
        } else {
            t!("Live off: the screen as it was")
        });
    });
    if on {
        let block = RcBlock::new(|timer: NonNull<NSTimer>| tick(timer));
        // SAFETY: a repeating timer on the main run loop; it invalidates
        // itself when live mode ends.
        let _ = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(TICK, true, &block) };
    }
}

fn start_capture() -> Capturing {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = tx.send(Shots(capture::capture_all()));
    });
    Capturing { rx, started: Instant::now() }
}

fn tick(timer: NonNull<NSTimer>) {
    let live = SESSION.with(|s| s.borrow().as_ref().is_some_and(|s| s.live && !s.kept));
    if !live {
        // SAFETY: the timer calling this block.
        unsafe { timer.as_ref() }.invalidate();
        return;
    }
    update(|s| {
        let result = match &s.capturing {
            // Not more often than every INTERVAL, to keep the load low.
            None if s.last_capture.is_none_or(|t| t.elapsed() >= INTERVAL) => {
                s.capturing = Some(start_capture());
                return;
            }
            None => return,
            Some(c) => match c.rx.try_recv() {
                Ok(Shots(result)) => result,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => Err(anyhow::anyhow!("the capture thread stopped")),
            },
        };
        s.last_capture = s.capturing.take().map(|c| c.started);
        match result {
            Ok(shots) => s.apply_shots(shots),
            Err(e) => tracing::warn!("live capture: {e:#}"),
        }
    });
}

impl Session {
    /// Swaps in new screenshots, matched to the overlays by display.
    pub(super) fn apply_shots(&mut self, shots: Vec<Shot>) {
        tracing::trace!("live: {} new screenshots", shots.len());
        for shot in shots {
            let Some(o) = self.overlays.iter_mut().find(|o| o.shot.display_id == shot.display_id) else { continue };
            if shot.frame.width != o.shot.frame.width || shot.frame.height != o.shot.frame.height {
                continue;
            }
            o.image = NSImage::initWithCGImage_size(NSImage::alloc(), &shot.image, o.size);
            o.shot = shot;
            o.smoothed = std::cell::OnceCell::new();
        }
    }
}
