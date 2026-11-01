//! Recording a new shortcut (to measure, or to pick a color): the next key
//! combination typed.

use super::*;

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSEventMask, NSEventModifierFlags};

use crate::settings::Settings;
use crate::shortcut::{CMD, CONTROL, OPTION, SHIFT, Shortcut};

/// What a recording ends with.
pub(super) enum Recorded {
    Cancelled,
    Shortcut(Shortcut),
    /// ⌫ for the color shortcut: none.
    Cleared,
}

thread_local! {
    /// The color shortcut is being recorded (not the measure shortcut).
    static FOR_COLOR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The button showing the shortcut being recorded.
fn recorder_button(c: &Controls) -> &NSButton {
    if FOR_COLOR.with(|f| f.get()) { &c.color_shortcut } else { &c.shortcut }
}

/// Waits for the next key combination and makes it the measure shortcut, or
/// the color shortcut when `color`.
pub(super) fn start_recording(color: bool) {
    if MONITOR.with(|m| m.borrow().is_some()) {
        return;
    }
    FOR_COLOR.with(|f| f.set(color));
    crate::app::suspend_hotkey();
    // The keys must come to this window: the click on the recorder lets the
    // app become active.
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).activate();
    }
    WINDOW.with(|w| {
        if let Some(w) = w.borrow().as_ref() {
            w.window.makeKeyAndOrderFront(None);
            let prompt =
                if color { t!("Type a shortcut (⌫ none, Esc cancels)") } else { t!("Type a shortcut (Esc cancels)") };
            recorder_button(&w.c).setTitle(&ns(&prompt));
            tracing::debug!("recording a shortcut; settings window key: {}", w.window.isKeyWindow());
        }
    });
    let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a valid event.
        let event = unsafe { event.as_ref() };
        let flags = event.modifierFlags();
        let mut modifiers = 0;
        for (flag, bit) in [
            (NSEventModifierFlags::Command, CMD),
            (NSEventModifierFlags::Shift, SHIFT),
            (NSEventModifierFlags::Option, OPTION),
            (NSEventModifierFlags::Control, CONTROL),
        ] {
            if flags.contains(flag) {
                modifiers |= bit;
            }
        }
        let code = event.keyCode();
        if code == KEY_ESCAPE && modifiers == 0 {
            finish_recording(Recorded::Cancelled);
        } else if code == KEY_DELETE && modifiers == 0 && FOR_COLOR.with(|f| f.get()) {
            finish_recording(Recorded::Cleared);
        } else if let Some(shortcut) = Shortcut::from_key(code, modifiers) {
            finish_recording(Recorded::Shortcut(shortcut));
        } else {
            WINDOW.with(|w| {
                if let Some(w) = w.borrow().as_ref() {
                    recorder_button(&w.c).setTitle(&ns(&t!("Add ⌘, ⌥, ⌃ or ⇧ (Esc cancels)")));
                }
            });
        }
        // Swallow the key either way.
        std::ptr::null_mut()
    });
    // SAFETY: the handler has the expected signature; the monitor is removed in `finish_recording`.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &handler) };
    MONITOR.with(|m| *m.borrow_mut() = monitor);
}

pub(super) fn finish_recording(result: Recorded) {
    if let Some(monitor) = MONITOR.with(|m| m.borrow_mut().take()) {
        // SAFETY: the monitor came from addLocalMonitorForEventsMatchingMask.
        unsafe { NSEvent::removeMonitor(&monitor) };
    }
    let color = FOR_COLOR.with(|f| f.get());
    let value = match result {
        Recorded::Cancelled => None,
        Recorded::Shortcut(shortcut) => Some(shortcut.to_config()),
        Recorded::Cleared => Some(String::new()),
    };
    if let Some(value) = value {
        let mut s = Settings::load();
        if color {
            s.color_shortcut = value;
        } else {
            s.shortcut = value;
        }
        if let Err(e) = s.save() {
            tracing::warn!("saving settings: {e:#}");
        }
    }
    crate::app::resume_hotkey();
    refresh();
}
