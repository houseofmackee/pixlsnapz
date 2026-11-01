//! Recording a new measure shortcut: the next key combination typed.

use super::*;

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSEventMask, NSEventModifierFlags};

use crate::settings::Settings;
use crate::shortcut::{CMD, CONTROL, OPTION, SHIFT, Shortcut};

/// Waits for the next key combination and makes it the shortcut.
pub(super) fn start_recording() {
    if MONITOR.with(|m| m.borrow().is_some()) {
        return;
    }
    crate::app::suspend_hotkey();
    // The keys must come to this window: the click on the recorder lets the
    // app become active.
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).activate();
    }
    WINDOW.with(|w| {
        if let Some(w) = w.borrow().as_ref() {
            w.window.makeKeyAndOrderFront(None);
            w.c.shortcut.setTitle(&ns("Type a shortcut (Esc cancels)"));
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
            finish_recording(None);
        } else if let Some(shortcut) = Shortcut::from_key(code, modifiers) {
            finish_recording(Some(shortcut));
        } else {
            WINDOW.with(|w| {
                if let Some(w) = w.borrow().as_ref() {
                    w.c.shortcut.setTitle(&ns("Add ⌘, ⌥, ⌃ or ⇧ (Esc cancels)"));
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

pub(super) fn finish_recording(shortcut: Option<Shortcut>) {
    if let Some(monitor) = MONITOR.with(|m| m.borrow_mut().take()) {
        // SAFETY: the monitor came from addLocalMonitorForEventsMatchingMask.
        unsafe { NSEvent::removeMonitor(&monitor) };
    }
    if let Some(shortcut) = shortcut {
        let mut s = Settings::load();
        s.shortcut = shortcut.to_config();
        if let Err(e) = s.save() {
            tracing::warn!("saving settings: {e:#}");
        }
    }
    crate::app::resume_hotkey();
    refresh();
}
