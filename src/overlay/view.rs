//! The overlay's window and view classes, and mouse and scroll input.

use super::*;

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSPanel, NSTrackingArea, NSTrackingAreaOptions, NSView};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect};

use crate::measure::snap_45;

define_class!(
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "PixlSnapzOverlayWindow"]
    pub struct OverlayWindow;

    impl OverlayWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool {
            true
        }
    }

    unsafe impl NSObjectProtocol for OverlayWindow {}
);

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "PixlSnapzOverlayView"]
    #[ivars = Cell<usize>]
    pub struct OverlayView;

    impl OverlayView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            draw(self.index());
        }

        // The pointer stays hidden while measuring, like PixelSnap: the
        // lines cross exactly where it is. `NSCursor::hide` alone only works
        // while the app is active, and the overlay is a non-activating
        // panel, so the view also shows an invisible cursor.
        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            if is_measuring() {
                self.addCursorRect_cursor(self.bounds(), &blank_cursor());
            }
        }

        #[unsafe(method(cursorUpdate:))]
        fn cursor_update(&self, _event: &NSEvent) {
            if is_measuring() {
                blank_cursor().set();
            }
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            if let Some(window) = self.window() {
                window.makeKeyWindow();
                window.makeFirstResponder(Some(self));
            }
            self.moved(event);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.moved(event);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.drag_started(event);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.dragged(event);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.drag_ended(event, false);
        }

        // Right-drag selects an area without snapping; a right click ends.
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.drag_started(event);
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            self.dragged(event);
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.drag_ended(event, true);
        }

        // Scrolling changes the edge tolerance, like PowerToys Screen Ruler.
        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let mut dy = event.scrollingDeltaY();
            if event.isDirectionInvertedFromDevice() {
                dy = -dy;
            }
            let per_step = if event.hasPreciseScrollingDeltas() { SCROLL_STEP } else { 1.0 };
            update(|s| s.scrolled(dy / per_step));
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if is_measuring() {
                key_down(event);
            }
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            if event.keyCode() == KEY_SPACE {
                update(|s| s.moving = false);
            }
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            let flags = event.modifierFlags();
            let shift = flags.contains(NSEventModifierFlags::Shift);
            let option = flags.contains(NSEventModifierFlags::Option);
            let control = flags.contains(NSEventModifierFlags::Control);
            update(|s| {
                s.shift = shift;
                s.option = option;
                s.control = control;
            });
        }
    }

    unsafe impl NSObjectProtocol for OverlayView {}
);

impl OverlayView {
    pub(super) fn new(index: usize, frame: NSRect, mtm: MainThreadMarker) -> Retained<OverlayView> {
        let this = OverlayView::alloc(mtm).set_ivars(Cell::new(index));
        // SAFETY: NSView's designated initializer.
        let view: Retained<OverlayView> = unsafe { msg_send![super(this), initWithFrame: frame] };
        let options = NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::CursorUpdate
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        // SAFETY: the view owns the tracking area and outlives it.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                frame,
                options,
                Some(&view),
                None,
            )
        };
        view.addTrackingArea(&area);
        view
    }

    pub(super) fn index(&self) -> usize {
        self.ivars().get()
    }

    pub(super) fn location(&self, event: &NSEvent) -> NSPoint {
        self.convertPoint_fromView(event.locationInWindow(), None)
    }

    pub(super) fn moved(&self, event: &NSEvent) {
        if !is_measuring() {
            return;
        }
        blank_cursor().set();
        let p = self.location(event);
        let index = self.index();
        let flags = event.modifierFlags();
        let shift = flags.contains(NSEventModifierFlags::Shift);
        let option = flags.contains(NSEventModifierFlags::Option);
        let control = flags.contains(NSEventModifierFlags::Control);
        update(|s| {
            s.active = index;
            s.hover = Some(p);
            s.shift = shift;
            s.option = option;
            s.control = control;
        });
    }

    pub(super) fn drag_started(&self, event: &NSEvent) {
        if !is_measuring() {
            return;
        }
        blank_cursor().set();
        let p = self.location(event);
        let index = self.index();
        let option = event.modifierFlags().contains(NSEventModifierFlags::Option);
        // ⌃-click keeps the window under the pointer.
        if event.modifierFlags().contains(NSEventModifierFlags::Control) {
            update(|s| {
                s.active = index;
                s.keep_window(index, p);
            });
            return;
        }
        update(|s| {
            s.active = index;
            s.hover = Some(p);
            // ⌥ held draws a line whatever the tool; otherwise a drag that
            // starts on a kept item edits it.
            let tool = if option { Tool::Line } else { s.tool };
            let edit = if option { None } else { s.hit_test(index, p) };
            s.drag = Some(Drag { start: p, end: p, tool, edit });
        });
    }

    pub(super) fn dragged(&self, event: &NSEvent) {
        if !is_measuring() {
            return;
        }
        blank_cursor().set();
        let p = self.location(event);
        let shift = event.modifierFlags().contains(NSEventModifierFlags::Shift);
        update(|s| {
            s.shift = shift;
            let moving = s.moving;
            if let Some(target) = s.drag.and_then(|d| d.edit) {
                let from = s.drag.map_or(p, |d| d.end);
                s.apply_edit(target, from, p);
                if let Some(d) = s.drag.as_mut() {
                    d.end = p;
                }
            } else if let Some(d) = s.drag.as_mut() {
                if moving {
                    // Space held: move the whole rectangle.
                    let (dx, dy) = (p.x - d.end.x, p.y - d.end.y);
                    d.start = NSPoint::new(d.start.x + dx, d.start.y + dy);
                    d.end = p;
                } else if d.tool == Tool::Line {
                    d.end = if shift { snap_line(d.start, p) } else { p };
                } else if d.tool == Tool::Circle {
                    d.end = p;
                } else {
                    d.end = if shift { square(d.start, p) } else { p };
                }
            }
            s.hover = Some(p);
        });
    }

    pub(super) fn drag_ended(&self, event: &NSEvent, right: bool) {
        if !is_measuring() {
            return;
        }
        let p = self.location(event);
        let no_snap = right || event.modifierFlags().contains(NSEventModifierFlags::Command);
        let index = self.index();
        let mut close = false;
        update(|s| {
            s.hover = Some(p);
            if let Some(d) = s.drag.take() {
                let moved = (d.end.x - d.start.x).abs().max((d.end.y - d.start.y).abs());
                if let Some(target) = d.edit {
                    // A click on an item without moving it keeps a distance
                    // there, as anywhere else; an edit is announced.
                    if moved < CLICK_SLOP {
                        s.finish_drag(index, d, no_snap);
                    } else if let Some(text) =
                        s.items.get(target.item).and_then(|(i, item)| s.text_of(item, &s.overlays[*i].measure))
                    {
                        announce(&text);
                    }
                } else if right && moved < CLICK_SLOP {
                    close = true;
                } else {
                    s.finish_drag(index, d, no_snap);
                }
            }
        });
        if close {
            end();
        }
    }
}

/// The end point that makes a square with `start`, towards `p`.
pub(super) fn square(start: NSPoint, p: NSPoint) -> NSPoint {
    let side = (p.x - start.x).abs().max((p.y - start.y).abs());
    NSPoint::new(start.x + side.copysign(p.x - start.x), start.y + side.copysign(p.y - start.y))
}

/// The end point that turns the line from `start` to `p` to the nearest
/// multiple of 45 degrees.
pub(super) fn snap_line(start: NSPoint, p: NSPoint) -> NSPoint {
    let (dx, dy) = snap_45(p.x - start.x, p.y - start.y);
    NSPoint::new(start.x + dx, start.y + dy)
}
