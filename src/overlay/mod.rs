//! The measuring overlay: one transparent, borderless window per display,
//! above everything else, while a measuring session runs.
//!
//! The session lives in a thread local. AppKit calls the view's methods on
//! the main thread; none of them keeps the session borrowed while calling
//! into AppKit in a way that could draw synchronously.

mod actions;
mod draw;
mod keys;
mod session;
mod view;

use std::cell::RefCell;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSCursor, NSEvent, NSImage, NSScreen, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, NSString};

use crate::capture::{self, Shot};
use crate::items::{Display, SavedSession};
use crate::measure::Scale;
use crate::settings::Settings;

use actions::*;
use draw::*;
use keys::*;
use session::*;
use view::*;

/// `NSScreenSaverWindowLevel`: above menus, the Dock and full-screen apps.
pub(super) const LEVEL: isize = 1000;
/// `NSStatusWindowLevel`, for measurements kept on screen: above normal
/// windows, below menus.
pub(super) const KEPT_LEVEL: isize = 25;
pub(super) const TOAST_TIME: Duration = Duration::from_millis(1400);
/// A drag shorter than this (in points) counts as a click.
pub(super) const CLICK_SLOP: f64 = 3.0;
/// New guides snap to an edge this close (in points).
pub(super) const GUIDE_SNAP: f64 = 6.0;
/// About how big the magnifier is, in points.
pub(super) const LOUPE_SIZE: f64 = 136.0;
/// Scroll distance (in points of precise, trackpad-style scrolling) per
/// tolerance step; a mouse wheel notch is one step.
pub(super) const SCROLL_STEP: f64 = 12.0;
/// How much one scroll step changes the edge tolerance.
pub(super) const TOLERANCE_PER_SCROLL: i32 = 4;
/// Padding around area screenshots taken with Option held (points).
pub(super) const SCREENSHOT_PADDING: f64 = 20.0;

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// Measuring is under way (not just measurements kept on screen).
pub fn is_measuring() -> bool {
    SESSION.with(|s| s.borrow().as_ref().is_some_and(|s| !s.kept))
}

/// Measurements are kept on screen after measuring ended.
pub fn has_kept() -> bool {
    SESSION.with(|s| s.borrow().as_ref().is_some_and(|s| s.kept))
}

/// Ends measuring, leaving kept measurements alone.
pub fn end_measuring() {
    if is_measuring() {
        end();
    }
}

/// Takes measurements kept on screen off it, returning them.
fn take_kept() -> Option<SavedSession> {
    let session = SESSION.with(|s| {
        let mut s = s.borrow_mut();
        if s.as_ref().is_some_and(|s| s.kept) { s.take() } else { None }
    })?;
    for o in &session.overlays {
        o.window.orderOut(None);
    }
    Some(session.saved())
}

/// B: ends measuring but keeps the measurements on screen, letting clicks
/// through, until the next measuring session or Clear Kept Measurements.
fn keep_on_screen() {
    let windows = SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut()?;
        if s.items.is_empty() {
            return None;
        }
        s.kept = true;
        s.drag = None;
        s.hover = None;
        s.contrast = None;
        s.toast = None;
        Some(s.overlays.iter().map(|o| o.window.clone()).collect::<Vec<_>>())
    });
    let Some(windows) = windows else {
        update(|s| s.toast("Measure something first"));
        return;
    };
    for w in &windows {
        w.setIgnoresMouseEvents(true);
        w.setLevel(KEPT_LEVEL);
        w.disableCursorRects();
        w.resignKeyWindow();
    }
    NSCursor::unhide();
    NSCursor::arrowCursor().set();
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).deactivate();
    }
    redraw_all();
}

/// Runs `f` on the session (if any) and redraws every overlay.
fn update(f: impl FnOnce(&mut Session)) {
    let ran = SESSION.with(|s| s.borrow_mut().as_mut().map(f).is_some());
    if ran {
        redraw_all();
    }
}

fn redraw_all() {
    let views: Vec<Retained<OverlayView>> =
        SESSION.with(|s| s.borrow().iter().flat_map(|s| s.overlays.iter().map(|o| o.view.clone())).collect());
    for v in views {
        v.setNeedsDisplay(true);
    }
}

/// Why a session could not start.
pub enum StartError {
    NoPermission,
    Capture(anyhow::Error),
}

/// Starts measuring on every display (or ends a running session).
pub fn toggle(mtm: MainThreadMarker) -> Result<(), StartError> {
    if is_measuring() {
        end();
        return Ok(());
    }
    if !capture::has_permission(true) {
        return Err(StartError::NoPermission);
    }
    // Measurements kept on screen come back into the new session.
    let kept = take_kept();
    let started = Instant::now();
    let shots = capture::capture_all().map_err(StartError::Capture)?;
    let settings = Settings::load();

    let mut overlays = Vec::new();
    let mut shots: Vec<Option<Shot>> = shots.into_iter().map(Some).collect();
    for screen in NSScreen::screens(mtm).iter() {
        let Some(id) = display_id(&screen) else { continue };
        let Some(shot) =
            shots.iter_mut().find(|s| s.as_ref().is_some_and(|s| s.display_id == id)).and_then(Option::take)
        else {
            continue;
        };
        let frame = screen.frame();
        let index = overlays.len();
        // A non-activating panel takes key events without activating the
        // app, so the app you measure stays frontmost.
        // SAFETY: NSWindow's designated initializer.
        let window: Retained<OverlayWindow> = unsafe {
            msg_send![OverlayWindow::alloc(mtm), initWithContentRect: frame,
                styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel, backing: NSBackingStoreType::Buffered, defer: false]
        };
        // SAFETY: the session owns the window.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setHidesOnDeactivate(false);
        window.setLevel(LEVEL);
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setAcceptsMouseMovedEvents(true);
        // Without this, clicks on fully transparent areas pass through.
        window.setIgnoresMouseEvents(false);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        window.setFrame_display(frame, false);
        let size = frame.size;
        let view = OverlayView::new(index, NSRect::new(NSPoint::new(0.0, 0.0), size), mtm);
        window.setContentView(Some(&view));
        let scale = shot.frame.width as f64 / size.width;
        let image = NSImage::initWithCGImage_size(NSImage::alloc(), &shot.image, size);
        let measure = Scale::new(scale, shot.frame.width, shot.width_mm, settings.screen_ppi, settings.design_zoom);
        overlays.push(Overlay { window, view, shot, image, scale, measure, size });
    }
    if overlays.is_empty() {
        return Err(StartError::Capture(anyhow::anyhow!("no screen matched a captured display")));
    }

    // The overlay under the mouse, and where the mouse is on it.
    let mouse = NSEvent::mouseLocation();
    let active = overlays.iter().position(|o| contains(o.window.frame(), mouse)).unwrap_or(0);
    let hover = {
        let o = &overlays[active];
        o.view.convertPoint_fromView(o.window.convertPointFromScreen(mouse), None)
    };
    let windows: Vec<(Retained<OverlayWindow>, Retained<OverlayView>)> =
        overlays.iter().map(|o| (o.window.clone(), o.view.clone())).collect();
    let displays: Vec<Display> = overlays
        .iter()
        .map(|o| Display { id: o.shot.display_id, width: o.shot.frame.width, height: o.shot.frame.height })
        .collect();
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            overlays,
            settings,
            active,
            hover: Some(hover),
            drag: None,
            moving: false,
            shift: false,
            items: kept.map(|k| k.place(&displays)).unwrap_or_default(),
            toast: None,
            line_tool: false,
            scroll: 0.0,
            settings_dirty: false,
            contrast: None,
            kept: false,
        })
    });

    let app = NSApplication::sharedApplication(mtm);
    for (i, (window, view)) in windows.iter().enumerate() {
        if i == active {
            window.makeKeyAndOrderFront(None);
            window.makeFirstResponder(Some(view));
        } else {
            window.orderFrontRegardless();
        }
    }
    blank_cursor().set();
    NSCursor::hide();
    tracing::debug!(
        "overlay ready in {:?}; key: {}, app active: {}",
        started.elapsed(),
        windows[active].0.isKeyWindow(),
        app.isActive()
    );
    Ok(())
}

/// Ends the session.
pub fn end() {
    let Some(session) = SESSION.with(|s| s.borrow_mut().take()) else { return };
    if !session.items.is_empty()
        && let Err(e) = session.saved().save_to(&SavedSession::path())
    {
        tracing::warn!("saving the session: {e:#}");
    }
    if session.settings_dirty {
        session.save_settings();
    }
    for o in &session.overlays {
        o.window.orderOut(None);
    }
    if !session.kept {
        NSCursor::unhide();
        NSCursor::arrowCursor().set();
    }
    drop(session);
}

/// A fully transparent cursor, made once.
fn blank_cursor() -> Retained<NSCursor> {
    thread_local! {
        static BLANK: std::cell::OnceCell<Retained<NSCursor>> = const { std::cell::OnceCell::new() };
    }
    BLANK.with(|b| {
        b.get_or_init(|| {
            // An image that draws nothing.
            let draw = RcBlock::new(|_rect: NSRect| Bool::YES);
            let image = NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(16.0, 16.0), false, &draw);
            NSCursor::initWithImage_hotSpot(NSCursor::alloc(), &image, NSPoint::new(8.0, 8.0))
        })
        .clone()
    })
}

fn contains(r: NSRect, p: NSPoint) -> bool {
    p.x >= r.origin.x && p.x < r.origin.x + r.size.width && p.y >= r.origin.y && p.y < r.origin.y + r.size.height
}

fn display_id(screen: &NSScreen) -> Option<u32> {
    let key = NSString::from_str("NSScreenNumber");
    let value = screen.deviceDescription().objectForKey(&key)?;
    value.downcast::<NSNumber>().ok().map(|n| n.unsignedIntValue())
}
