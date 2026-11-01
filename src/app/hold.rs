//! Hold to measure: holding the chosen modifier keys (and nothing else) for
//! a moment measures until they are released. The keys are read about 20
//! times a second, which needs no permission (unlike watching key events).

use std::cell::Cell;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2_core_graphics::{CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventType};
use objc2_foundation::NSTimer;

use super::*;
use crate::settings::HoldKeys;
use crate::shortcut::{CMD, CONTROL, OPTION, SHIFT};

/// How long the keys must be held before measuring starts, so shortcuts
/// with the same modifiers (typed quickly) do not start it.
const DELAY: Duration = Duration::from_millis(350);
const INTERVAL: f64 = 0.05;

thread_local! {
    /// When the keys started being held, and whether measuring started.
    static HELD: Cell<(Option<Instant>, bool)> = const { Cell::new((None, false)) };
}

/// The modifier keys down right now, as `shortcut` bits.
fn modifiers_down() -> u32 {
    let flags = CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState);
    [
        (CGEventFlags::MaskCommand, CMD),
        (CGEventFlags::MaskShift, SHIFT),
        (CGEventFlags::MaskAlternate, OPTION),
        (CGEventFlags::MaskControl, CONTROL),
    ]
    .into_iter()
    .filter(|(flag, _)| flags.contains(*flag))
    .fold(0, |bits, (_, bit)| bits | bit)
}

/// Starts watching the keys; the setting is read on every check.
pub(super) fn start() {
    let block = RcBlock::new(|_timer: NonNull<NSTimer>| tick());
    // SAFETY: a repeating timer on the main run loop; the block is copied.
    let _ = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(INTERVAL, true, &block) };
}

fn tick() {
    let (keys, enabled_here) = APP.with(|a| {
        let a = a.borrow();
        let Some(a) = a.as_ref() else { return (HoldKeys::Off, false) };
        let enabled = !a.suspended && a.front_app.as_ref().is_none_or(|(id, _)| a.settings.shortcut_enabled_in(id));
        (a.settings.hold_keys, enabled)
    });
    let (since, measuring) = HELD.with(Cell::get);
    let held = keys != HoldKeys::Off && modifiers_down() == keys.modifiers();
    if !held {
        if measuring {
            overlay::end_peek();
        }
        HELD.with(|h| h.set((None, false)));
        return;
    }
    let since = since.unwrap_or_else(Instant::now);
    let mut started = measuring;
    if !measuring && enabled_here && since.elapsed() >= DELAY && !overlay::is_measuring() {
        // Not when a key was typed with the modifiers (a shortcut).
        let typed = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            CGEventType::KeyDown,
        ) < since.elapsed().as_secs_f64();
        if !typed {
            started = true;
            if let Err(StartError::Capture(e)) = overlay::peek(mtm()) {
                tracing::warn!("hold to measure: {e:#}");
            }
        }
    }
    HELD.with(|h| h.set((Some(since), started)));
}
