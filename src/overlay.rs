//! The measuring overlay: one transparent, borderless window per display,
//! above everything else, while a measuring session runs.
//!
//! The session lives in a thread local. AppKit calls the view's methods on
//! the main thread; none of them keeps the session borrowed while calling
//! into AppKit in a way that could draw synchronously.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSBezierPath, NSBitmapImageFileType, NSBitmapImageRep, NSColor,
    NSCompositingOperation, NSCursor, NSEvent, NSEventModifierFlags, NSFont, NSFontAttributeName, NSFontWeightMedium,
    NSFontWeightRegular, NSForegroundColorAttributeName, NSGraphicsContext, NSImage, NSImageInterpolation, NSPanel,
    NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString, NSScreen, NSStringDrawing, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSDictionary, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer};

use crate::capture::{self, Shot};
use crate::color::{ColorFormat, average};
use crate::items::{Display, Item, SavedItem, SavedSession};
use crate::measure::{
    Cross, Direction, PxRect, aspect_ratio, clip_cross, format_angle, format_length, line_angle, line_length,
    next_edge, snap_45, snap_to,
};
use crate::settings::{Axes, Settings, Units};

/// `NSScreenSaverWindowLevel`: above menus, the Dock and full-screen apps.
const LEVEL: isize = 1000;
const TOAST_TIME: Duration = Duration::from_millis(1400);
/// A drag shorter than this (in points) counts as a click.
const CLICK_SLOP: f64 = 3.0;
/// New guides snap to an edge this close (in points).
const GUIDE_SNAP: f64 = 6.0;
/// About how big the magnifier is, in points.
const LOUPE_SIZE: f64 = 136.0;
/// Scroll distance (in points of precise, trackpad-style scrolling) per
/// tolerance step; a mouse wheel notch is one step.
const SCROLL_STEP: f64 = 12.0;
/// How much one scroll step changes the edge tolerance.
const TOLERANCE_PER_SCROLL: i32 = 4;
/// Padding around area screenshots taken with Option held (points).
const SCREENSHOT_PADDING: f64 = 20.0;

const KEY_RETURN: u16 = 36;
const KEY_ENTER: u16 = 76;
const KEY_ESCAPE: u16 = 53;
const KEY_DELETE: u16 = 51;
const KEY_FORWARD_DELETE: u16 = 117;
const KEY_SPACE: u16 = 49;
const KEY_LEFT: u16 = 123;
const KEY_RIGHT: u16 = 124;
const KEY_DOWN: u16 = 125;
const KEY_UP: u16 = 126;

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
            self.addCursorRect_cursor(self.bounds(), &blank_cursor());
        }

        #[unsafe(method(cursorUpdate:))]
        fn cursor_update(&self, _event: &NSEvent) {
            blank_cursor().set();
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
            key_down(event);
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            if event.keyCode() == KEY_SPACE {
                update(|s| s.moving = false);
            }
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            let shift = event.modifierFlags().contains(NSEventModifierFlags::Shift);
            update(|s| s.shift = shift);
        }
    }

    unsafe impl NSObjectProtocol for OverlayView {}
);

impl OverlayView {
    fn new(index: usize, frame: NSRect, mtm: MainThreadMarker) -> Retained<OverlayView> {
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

    fn index(&self) -> usize {
        self.ivars().get()
    }

    fn location(&self, event: &NSEvent) -> NSPoint {
        self.convertPoint_fromView(event.locationInWindow(), None)
    }

    fn moved(&self, event: &NSEvent) {
        blank_cursor().set();
        let p = self.location(event);
        let index = self.index();
        let shift = event.modifierFlags().contains(NSEventModifierFlags::Shift);
        update(|s| {
            s.active = index;
            s.hover = Some(p);
            s.shift = shift;
        });
    }

    fn drag_started(&self, event: &NSEvent) {
        blank_cursor().set();
        let p = self.location(event);
        let index = self.index();
        let option = event.modifierFlags().contains(NSEventModifierFlags::Option);
        update(|s| {
            s.active = index;
            s.hover = Some(p);
            // The line tool, or ⌥ held, draws a line instead of an area.
            s.drag = Some(Drag { start: p, end: p, line: s.line_tool || option });
        });
    }

    fn dragged(&self, event: &NSEvent) {
        blank_cursor().set();
        let p = self.location(event);
        let shift = event.modifierFlags().contains(NSEventModifierFlags::Shift);
        update(|s| {
            s.shift = shift;
            let moving = s.moving;
            if let Some(d) = s.drag.as_mut() {
                if moving {
                    // Space held: move the whole rectangle.
                    let (dx, dy) = (p.x - d.end.x, p.y - d.end.y);
                    d.start = NSPoint::new(d.start.x + dx, d.start.y + dy);
                    d.end = p;
                } else if d.line {
                    d.end = if shift { snap_line(d.start, p) } else { p };
                } else {
                    d.end = if shift { square(d.start, p) } else { p };
                }
            }
            s.hover = Some(p);
        });
    }

    fn drag_ended(&self, event: &NSEvent, right: bool) {
        let p = self.location(event);
        let no_snap = right || event.modifierFlags().contains(NSEventModifierFlags::Command);
        let index = self.index();
        let mut close = false;
        update(|s| {
            s.hover = Some(p);
            if let Some(d) = s.drag.take() {
                let moved = (d.end.x - d.start.x).abs().max((d.end.y - d.start.y).abs());
                if right && moved < CLICK_SLOP {
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
fn square(start: NSPoint, p: NSPoint) -> NSPoint {
    let side = (p.x - start.x).abs().max((p.y - start.y).abs());
    NSPoint::new(start.x + side.copysign(p.x - start.x), start.y + side.copysign(p.y - start.y))
}

/// The end point that turns the line from `start` to `p` to the nearest
/// multiple of 45 degrees.
fn snap_line(start: NSPoint, p: NSPoint) -> NSPoint {
    let (dx, dy) = snap_45(p.x - start.x, p.y - start.y);
    NSPoint::new(start.x + dx, start.y + dy)
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    start: NSPoint,
    end: NSPoint,
    /// Draws a line rather than an area.
    line: bool,
}

struct Overlay {
    window: Retained<OverlayWindow>,
    view: Retained<OverlayView>,
    shot: Shot,
    /// The screenshot in points, for freezing and the magnifier.
    image: Retained<NSImage>,
    /// Pixels per point.
    scale: f64,
    /// View size in points.
    size: NSSize,
}

impl Overlay {
    fn px(&self, p: NSPoint) -> (usize, usize) {
        let f = &self.shot.frame;
        let x = (p.x * self.scale).floor().clamp(0.0, (f.width - 1) as f64) as usize;
        let y = (p.y * self.scale).floor().clamp(0.0, (f.height - 1) as f64) as usize;
        (x, y)
    }

    fn pt(&self, px: usize) -> f64 {
        px as f64 / self.scale
    }

    fn rect_px(&self, a: NSPoint, b: NSPoint) -> PxRect {
        let v = |n: f64| (n * self.scale).round().max(0.0) as usize;
        self.shot.frame.clamp(PxRect {
            x0: v(a.x.min(b.x)),
            y0: v(a.y.min(b.y)),
            x1: v(a.x.max(b.x)),
            y1: v(a.y.max(b.y)),
        })
    }

    fn rect_pt(&self, r: PxRect) -> NSRect {
        NSRect::new(NSPoint::new(self.pt(r.x0), self.pt(r.y0)), NSSize::new(self.pt(r.width()), self.pt(r.height())))
    }
}

struct Session {
    overlays: Vec<Overlay>,
    settings: Settings,
    /// The overlay the cursor is on.
    active: usize,
    /// Cursor position in the active overlay's view (points, top-left origin).
    hover: Option<NSPoint>,
    drag: Option<Drag>,
    /// Space is held: dragging moves the rectangle.
    moving: bool,
    /// Shift is held: crosshair mode.
    shift: bool,
    /// Kept items with the index of their overlay, oldest first.
    items: Vec<(usize, Item)>,
    toast: Option<(String, Instant)>,
    /// The line tool is on (L).
    line_tool: bool,
    /// Scrolling not yet turned into tolerance steps.
    scroll: f64,
    /// Settings changed by scrolling, saved when measuring ends.
    settings_dirty: bool,
}

impl Session {
    fn in_pixels(&self) -> bool {
        self.settings.units == Units::Pixels
    }

    fn len_text(&self, px: usize, scale: f64) -> String {
        let n = format_length(px, scale, self.in_pixels());
        if self.settings.show_units { format!("{n} {}", if self.in_pixels() { "px" } else { "pt" }) } else { n }
    }

    /// A length given in (possibly fractional) pixels.
    fn len_text_f64(&self, px: f64, scale: f64) -> String {
        let v = if self.in_pixels() { px } else { px / scale };
        let rounded = (v * 10.0).round() / 10.0;
        let n = if rounded.fract() == 0.0 { format!("{}", rounded as i64) } else { format!("{rounded:.1}") };
        if self.settings.show_units { format!("{n} {}", if self.in_pixels() { "px" } else { "pt" }) } else { n }
    }

    /// What a cross measures in the given directions.
    fn cross_text(&self, c: &Cross, axes: Axes, scale: f64) -> String {
        match axes {
            Axes::Both => self.size_text(c.width(), c.height(), scale),
            Axes::Horizontal => self.len_text(c.width(), scale),
            Axes::Vertical => self.len_text(c.height(), scale),
        }
    }

    /// Length and angle of a line in pixels: ("120.5", "32.5°").
    fn line_texts(&self, (x0, y0, x1, y1): (f64, f64, f64, f64), scale: f64) -> (String, String) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        (self.len_text_f64(line_length(dx, dy), scale), format_angle(line_angle(dx, dy)))
    }

    fn scrolled(&mut self, steps: f64) {
        self.scroll += steps;
        let whole = self.scroll.trunc();
        if whole == 0.0 {
            return;
        }
        self.scroll -= whole;
        self.settings.adjust_tolerance(whole as i32 * TOLERANCE_PER_SCROLL);
        self.settings_dirty = true;
        self.toast(format!("Edge tolerance: {}", self.settings.tolerance_name()));
    }

    fn size_text(&self, w: usize, h: usize, scale: f64) -> String {
        let px = self.in_pixels();
        let mut text = format!("{} × {}", format_length(w, scale, px), format_length(h, scale, px));
        if self.settings.show_units {
            text.push_str(if px { " px" } else { " pt" });
        }
        text
    }

    fn guides(&self, index: usize) -> (Vec<usize>, Vec<usize>) {
        let mut v = Vec::new();
        let mut h = Vec::new();
        for (_, item) in self.items.iter().filter(|(i, _)| *i == index) {
            match item {
                Item::VerticalGuide { x } => v.push(*x),
                Item::HorizontalGuide { y } => h.push(*y),
                _ => {}
            }
        }
        (v, h)
    }

    fn areas(&self, index: usize) -> Vec<PxRect> {
        self.items
            .iter()
            .filter_map(|(i, item)| match item {
                Item::Area(r) if *i == index => Some(*r),
                _ => None,
            })
            .collect()
    }

    /// The cross through pixel `(x, y)`, stopped at guides and kept areas.
    fn cross_at(&self, index: usize, x: usize, y: usize) -> Cross {
        let o = &self.overlays[index];
        let (v, h) = self.guides(index);
        clip_cross(o.shot.frame.cross(x, y, self.settings.tolerance), x, y, &v, &h, &self.areas(index))
    }

    fn hover_px(&self) -> Option<(usize, usize)> {
        Some(self.overlays.get(self.active)?.px(self.hover?))
    }

    fn finish_drag(&mut self, index: usize, d: Drag, no_snap: bool) {
        let Some(o) = self.overlays.get(index) else { return };
        let (start, end) = (d.start, d.end);
        let moved = (end.x - start.x).abs().max((end.y - start.y).abs());
        let item = if moved < CLICK_SLOP {
            let (x, y) = o.px(end);
            Item::Cross { x, y, cross: self.cross_at(index, x, y), axes: self.settings.axes }
        } else if d.line {
            Item::Line { x0: start.x * o.scale, y0: start.y * o.scale, x1: end.x * o.scale, y1: end.y * o.scale }
        } else {
            let r = o.rect_px(start, end);
            let r =
                if self.settings.snap_areas && !no_snap { o.shot.frame.snap(r, self.settings.tolerance) } else { r };
            if r.width() == 0 || r.height() == 0 {
                return;
            }
            Item::Area(r)
        };
        self.items.push((index, item));
    }

    fn add_guide(&mut self, vertical: bool) {
        let Some((x, y)) = self.hover_px() else { return };
        let o = &self.overlays[self.active];
        let c = o.shot.frame.cross(x, y, self.settings.tolerance);
        let max = (GUIDE_SNAP * o.scale) as usize;
        let item = if vertical {
            Item::VerticalGuide { x: snap_to(x, &[c.left, c.right], max) }
        } else {
            Item::HorizontalGuide { y: snap_to(y, &[c.top, c.bottom], max) }
        };
        self.items.push((self.active, item));
    }

    /// The text to copy: the last kept measurement, or the one under the cursor.
    fn copy_text(&self) -> Option<String> {
        let last = self.items.iter().rev().find_map(|(i, item)| {
            let scale = self.overlays[*i].scale;
            match item {
                Item::Area(r) => Some(self.size_text(r.width(), r.height(), scale)),
                Item::Cross { cross, axes, .. } => Some(self.cross_text(cross, *axes, scale)),
                Item::Line { x0, y0, x1, y1 } => {
                    let (len, angle) = self.line_texts((*x0, *y0, *x1, *y1), scale);
                    Some(format!("{len}, {angle}"))
                }
                _ => None,
            }
        });
        if last.is_some() {
            return last;
        }
        let (x, y) = self.hover_px()?;
        let c = self.cross_at(self.active, x, y);
        Some(self.cross_text(&c, self.settings.axes, self.overlays[self.active].scale))
    }

    /// The last kept area, if any.
    fn last_area(&self) -> Option<(usize, PxRect)> {
        self.items.iter().rev().find_map(|(i, item)| match item {
            Item::Area(r) => Some((*i, *r)),
            _ => None,
        })
    }

    fn displays(&self) -> Vec<Display> {
        self.overlays
            .iter()
            .map(|o| Display { id: o.shot.display_id, width: o.shot.frame.width, height: o.shot.frame.height })
            .collect()
    }

    /// Option-arrow: jumps the pointer just past the next edge.
    fn jump(&mut self, dir: Direction) {
        let Some((x, y)) = self.hover_px() else { return };
        let o = &self.overlays[self.active];
        let c = self.cross_at(self.active, x, y);
        let (nx, ny) = next_edge(c, x, y, dir, o.shot.frame.width, o.shot.frame.height);
        self.hover = Some(NSPoint::new((nx as f64 + 0.5) / o.scale, (ny as f64 + 0.5) / o.scale));
    }

    /// The last kept area, or the whole display under the cursor.
    fn screenshot_area(&self) -> Option<(usize, PxRect)> {
        self.items
            .iter()
            .rev()
            .find_map(|(i, item)| match item {
                Item::Area(r) => Some((*i, *r)),
                _ => None,
            })
            .or_else(|| {
                let o = self.overlays.get(self.active)?;
                let f = &o.shot.frame;
                Some((self.active, PxRect { x0: 0, y0: 0, x1: f.width, y1: f.height }))
            })
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
        let block = RcBlock::new(|_timer: std::ptr::NonNull<NSTimer>| redraw_all());
        // SAFETY: a one-shot timer on the main run loop; the block is copied.
        let _ = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(TOAST_TIME.as_secs_f64() + 0.05, false, &block)
        };
    }

    fn save_settings(&self) {
        if let Err(e) = self.settings.save() {
            tracing::warn!("saving settings: {e:#}");
        }
    }

    fn saved(&self) -> SavedSession {
        let items = self
            .items
            .iter()
            .map(|(i, item)| {
                let shot = &self.overlays[*i].shot;
                SavedItem { display: shot.display_id, width: shot.frame.width, height: shot.frame.height, item: *item }
            })
            .collect();
        SavedSession { items }
    }

    /// Brings back what the last session kept (also after a restart).
    fn restore(&mut self) -> usize {
        let placed = SavedSession::load_from(&SavedSession::path()).place(&self.displays());
        let n = placed.len();
        self.items.extend(placed);
        n
    }
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

pub fn is_active() -> bool {
    SESSION.with(|s| s.borrow().is_some())
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
    if is_active() {
        end();
        return Ok(());
    }
    if !capture::has_permission(true) {
        return Err(StartError::NoPermission);
    }
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
        overlays.push(Overlay { window, view, shot, image, scale, size });
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
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            overlays,
            settings,
            active,
            hover: Some(hover),
            drag: None,
            moving: false,
            shift: false,
            items: Vec::new(),
            toast: None,
            line_tool: false,
            scroll: 0.0,
            settings_dirty: false,
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
    NSCursor::unhide();
    NSCursor::arrowCursor().set();
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

fn key_down(event: &NSEvent) {
    let code = event.keyCode();
    let flags = event.modifierFlags();
    let command = flags.contains(NSEventModifierFlags::Command);
    let shift = flags.contains(NSEventModifierFlags::Shift);
    let option = flags.contains(NSEventModifierFlags::Option);
    let chars = event.charactersIgnoringModifiers().map(|c| c.to_string().to_lowercase()).unwrap_or_default();

    match code {
        KEY_ESCAPE => {
            let dragging = SESSION.with(|s| s.borrow().as_ref().is_some_and(|s| s.drag.is_some()));
            if dragging {
                update(|s| s.drag = None)
            } else {
                end()
            }
            return;
        }
        KEY_SPACE => {
            update(|s| s.moving = s.drag.is_some());
            return;
        }
        KEY_RETURN | KEY_ENTER => {
            if copy_size() {
                end();
            }
            return;
        }
        KEY_DELETE | KEY_FORWARD_DELETE => {
            update(|s| {
                s.items.pop();
            });
            return;
        }
        KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN if option => {
            let dir = match code {
                KEY_LEFT => Direction::Left,
                KEY_RIGHT => Direction::Right,
                KEY_UP => Direction::Up,
                _ => Direction::Down,
            };
            update(|s| s.jump(dir));
            return;
        }
        KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN => {
            update(|s| nudge(s, code, shift));
            return;
        }
        _ => {}
    }

    match (command, chars.as_str()) {
        (true, "c") if shift => screenshot(false, option),
        (true, "c") => {
            copy_size();
        }
        (true, "s") => screenshot(true, option),
        (true, "z") => update(|s| {
            s.items.pop();
        }),
        (true, "w") | (true, "q") | (true, ".") => end(),
        (false, "v") => update(|s| s.add_guide(true)),
        (false, "h") => update(|s| s.add_guide(false)),
        (false, "g") => update(|s| {
            s.items.retain(|(_, item)| !item.is_guide());
            s.toast("Guides cleared");
        }),
        (false, "c") => update(|s| {
            s.items.clear();
            s.toast("Cleared");
        }),
        (false, "r") => update(|s| {
            let n = s.restore();
            s.toast(if n == 0 { "Nothing to restore".to_string() } else { format!("Restored {n} items") });
        }),
        (false, "p") if shift => update(|s| {
            s.settings.color_format = s.settings.color_format.next();
            let name = s.settings.color_format.name();
            s.toast(format!("Color format: {name}"));
            s.save_settings();
        }),
        (false, "p") if option => copy_average_color(),
        (false, "p") => copy_color(),
        (false, "l") => update(|s| {
            s.line_tool = !s.line_tool;
            s.toast(if s.line_tool { "Line tool: drag to measure length and angle" } else { "Area tool" });
        }),
        (false, "x") => update(|s| {
            s.settings.axes = s.settings.axes.next();
            let name = s.settings.axes.name();
            s.toast(format!("Measure: {name}"));
            s.save_settings();
        }),
        (false, "-") | (false, "=") | (false, "+") => update(|s| {
            let zoom_in = chars != "-";
            if s.settings.loupe {
                s.settings.step_loupe_zoom(zoom_in);
            }
            s.settings.loupe = true;
            s.toast(format!("Loupe: {}×", s.settings.loupe_zoom));
            s.save_settings();
        }),
        (false, "u") => update(|s| {
            s.settings.units = if s.in_pixels() { Units::Points } else { Units::Pixels };
            s.toast(if s.in_pixels() { "Units: pixels" } else { "Units: points" });
            s.save_settings();
        }),
        (false, "[") | (false, "]") => update(|s| {
            s.settings.step_tolerance(chars == "]");
            s.toast(format!("Edge tolerance: {}", s.settings.tolerance_name()));
            s.save_settings();
        }),
        (false, "s") => toggle_setting("Snap areas to content", |st| &mut st.snap_areas),
        (false, "z") => toggle_setting("Loupe", |st| &mut st.loupe),
        (false, "f") => toggle_setting("Freeze screen", |st| &mut st.freeze),
        (false, "a") => toggle_setting("Aspect ratio", |st| &mut st.show_aspect_ratio),
        (false, "/") | (false, "?") => update(|s| {
            s.settings.show_hints = !s.settings.show_hints;
            s.save_settings();
        }),
        _ => {}
    }
}

fn toggle_setting(name: &str, field: impl Fn(&mut Settings) -> &mut bool) {
    update(|s| {
        let value = field(&mut s.settings);
        *value = !*value;
        let on = *value;
        s.toast(format!("{name}: {}", if on { "on" } else { "off" }));
        s.save_settings();
    });
}

/// Arrow keys: move the cursor by one pixel (ten points with Shift), or
/// the last guide when one was just added.
fn nudge(s: &mut Session, code: u16, shift: bool) {
    let Some(o) = s.overlays.get(s.active) else { return };
    let step_pt = if shift { 10.0 } else { 1.0 / o.scale };
    let step_px = (step_pt * o.scale).round().max(1.0) as isize;
    let (dx, dy) = match code {
        KEY_LEFT => (-1.0, 0.0),
        KEY_RIGHT => (1.0, 0.0),
        KEY_UP => (0.0, -1.0),
        _ => (0.0, 1.0),
    };
    let (w, h) = (o.shot.frame.width as isize, o.shot.frame.height as isize);
    let (size, active) = (o.size, s.active);
    let moved_guide = match s.items.last_mut() {
        Some((i, Item::VerticalGuide { x })) if *i == active && dx != 0.0 => {
            *x = (*x as isize + dx as isize * step_px).clamp(0, w) as usize;
            true
        }
        Some((i, Item::HorizontalGuide { y })) if *i == active && dy != 0.0 => {
            *y = (*y as isize + dy as isize * step_px).clamp(0, h) as usize;
            true
        }
        _ => false,
    };
    if !moved_guide && let Some(p) = s.hover {
        let x = (p.x + dx * step_pt).clamp(0.0, size.width - 0.01);
        let y = (p.y + dy * step_pt).clamp(0.0, size.height - 0.01);
        s.hover = Some(NSPoint::new(x, y));
    }
}

fn set_clipboard_string(text: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: a static constant.
    pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
}

/// Copies the current measurement as "W × H". Returns whether there was one.
fn copy_size() -> bool {
    let text = SESSION.with(|s| s.borrow().as_ref().and_then(Session::copy_text));
    let Some(text) = text else { return false };
    set_clipboard_string(&text);
    update(|s| s.toast(format!("Copied {text}")));
    true
}

/// Copies the color under the cursor in the chosen format.
fn copy_color() {
    let text = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (x, y) = s.hover_px()?;
        Some(s.settings.color_format.format(s.overlays[s.active].shot.frame.pixel(x, y)))
    });
    if let Some(text) = text {
        set_clipboard_string(&text);
        update(|s| s.toast(format!("Copied {text}")));
    }
}

/// Copies the average color of the last area in the chosen format.
fn copy_average_color() {
    let text = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (i, r) = s.last_area()?;
        average(&s.overlays[i].shot.frame, r).map(|c| s.settings.color_format.format(c))
    });
    match text {
        Some(text) => {
            set_clipboard_string(&text);
            update(|s| s.toast(format!("Copied average {text}")));
        }
        None => update(|s| s.toast("Drag over an area first")),
    }
}

/// Saves (or copies) a PNG of the last area, or of the display when there is
/// none. Option toggles padding around the area.
fn screenshot(save: bool, padded: bool) {
    let result: Option<(Vec<u8>, PathBuf)> = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (i, r) = s.screenshot_area()?;
        let o = &s.overlays[i];
        let pad = if padded { (SCREENSHOT_PADDING * o.scale) as usize } else { 0 };
        let r = o.shot.frame.clamp(PxRect {
            x0: r.x0.saturating_sub(pad),
            y0: r.y0.saturating_sub(pad),
            x1: r.x1 + pad,
            y1: r.y1 + pad,
        });
        let png = png_of(&o.shot.image, r)?;
        Some((png, s.settings.screenshot_dir()))
    });
    let Some((png, dir)) = result else {
        update(|s| s.toast("Could not take the screenshot"));
        return;
    };
    if save {
        let name = format!("PixlSnapz {}.png", chrono_like_now());
        let path = dir.join(name);
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &png)) {
            Ok(()) => update(|s| s.toast(format!("Saved to {}", dir.display()))),
            Err(e) => {
                tracing::warn!("saving {}: {e}", path.display());
                update(|s| s.toast("Could not save the screenshot"));
            }
        }
    } else {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let data = objc2_foundation::NSData::with_bytes(&png);
        // SAFETY: a static constant.
        pasteboard.setData_forType(Some(&data), unsafe { NSPasteboardTypePNG });
        update(|s| s.toast("Screenshot copied"));
    }
}

fn png_of(image: &CGImage, r: PxRect) -> Option<Vec<u8>> {
    let rect = CGRect::new(CGPoint::new(r.x0 as f64, r.y0 as f64), CGSize::new(r.width() as f64, r.height() as f64));
    let cropped = CGImage::with_image_in_rect(Some(image), rect)?;
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cropped);
    // SAFETY: an empty properties dictionary.
    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some(data.to_vec())
}

/// Local date and time as "2026-11-01 at 16.42.05", like macOS screenshots.
fn chrono_like_now() -> String {
    // SAFETY: localtime_r fills the given struct.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02} at {:02}.{:02}.{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

// Drawing ------------------------------------------------------------------

fn color(rgb: [f64; 3], alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(rgb[0], rgb[1], rgb[2], alpha)
}

fn line(path: &NSBezierPath, a: (f64, f64), b: (f64, f64)) {
    path.moveToPoint(NSPoint::new(a.0, a.1));
    path.lineToPoint(NSPoint::new(b.0, b.1));
}

fn attributes(size: f64, medium: bool) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: static constants.
    let weight = unsafe { if medium { NSFontWeightMedium } else { NSFontWeightRegular } };
    let font = NSFont::monospacedDigitSystemFontOfSize_weight(size, weight);
    let fg = NSColor::whiteColor();
    // SAFETY: static constants.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font.as_ref(), fg.as_ref()];
    NSDictionary::from_slices(&keys, &values)
}

fn pill_size(text: &str, size: f64) -> NSSize {
    // SAFETY: the attributes have the expected types.
    let t = unsafe { NSString::from_str(text).sizeWithAttributes(Some(&attributes(size, true))) };
    NSSize::new(t.width + 14.0, t.height + 6.0)
}

/// Draws `text` in a rounded label filled with `fill`. `at` is the label's
/// top-left corner, moved as needed to stay inside `bounds`.
fn pill(text: &str, at: NSPoint, bounds: NSSize, size: f64, fill: &NSColor) -> NSRect {
    let attrs = attributes(size, true);
    let s = NSString::from_str(text);
    let ps = pill_size(text, size);
    let x = at.x.clamp(2.0, (bounds.width - ps.width - 2.0).max(2.0));
    let y = at.y.clamp(2.0, (bounds.height - ps.height - 2.0).max(2.0));
    let r = NSRect::new(NSPoint::new(x, y), ps);
    fill.setFill();
    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(r, 4.0, 4.0).fill();
    // SAFETY: the attributes have the expected types.
    unsafe { s.drawAtPoint_withAttributes(NSPoint::new(x + 7.0, y + 3.0), Some(&attrs)) };
    r
}

/// A label centered on `center`.
fn pill_centered(text: &str, center: NSPoint, bounds: NSSize, size: f64, fill: &NSColor) {
    let ps = pill_size(text, size);
    pill(text, NSPoint::new(center.x - ps.width / 2.0, center.y - ps.height / 2.0), bounds, size, fill);
}

fn dark() -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(0.09, 0.09, 0.11, 0.9)
}

/// Lines from edge to edge through the point, with end ticks and a label
/// for each direction.
fn draw_cross(s: &Session, o: &Overlay, x: usize, y: usize, c: &Cross, axes: Axes, rgb: [f64; 3]) {
    let (cx, cy) = ((x as f64 + 0.5) / o.scale, (y as f64 + 0.5) / o.scale);
    let (l, r) = (o.pt(c.left), o.pt(c.right));
    let (t, b) = (o.pt(c.top), o.pt(c.bottom));
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(1.0);
    let tick = 5.0;
    if axes.horizontal() {
        line(&path, (l, cy), (r, cy));
        for ex in [l, r] {
            line(&path, (ex, cy - tick), (ex, cy + tick));
        }
    }
    if axes.vertical() {
        line(&path, (cx, t), (cx, b));
        for ey in [t, b] {
            line(&path, (cx - tick, ey), (cx + tick, ey));
        }
    }
    color(rgb, 1.0).setStroke();
    path.stroke();

    let accent = color(rgb, 1.0);
    pill(&s.cross_text(c, axes, o.scale), NSPoint::new(cx + 10.0, cy + 10.0), o.size, 12.0, &accent);
}

/// A measured line with end ticks, a protractor arc from the horizontal at
/// its start, and its length and angle. Coordinates in pixels.
fn draw_line(s: &Session, o: &Overlay, (x0, y0, x1, y1): (f64, f64, f64, f64), rgb: [f64; 3]) {
    let (ax, ay, bx, by) = (x0 / o.scale, y0 / o.scale, x1 / o.scale, y1 / o.scale);
    let len_pt = line_length(bx - ax, by - ay);
    let accent = color(rgb, 1.0);
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(1.5);
    line(&path, (ax, ay), (bx, by));
    if len_pt > 0.0 {
        // Ticks across both ends.
        let (nx, ny) = (-(by - ay) / len_pt * 5.0, (bx - ax) / len_pt * 5.0);
        for (px, py) in [(ax, ay), (bx, by)] {
            line(&path, (px - nx, py - ny), (px + nx, py + ny));
        }
    }
    accent.setStroke();
    path.stroke();

    // Protractor: a dashed horizontal reference and an arc to the line.
    let angle = line_angle(bx - ax, by - ay);
    let radius = 28.0f64.min(len_pt * 0.6);
    if radius > 8.0 && angle.abs() > 0.5 && (angle.abs() - 180.0).abs() > 0.5 {
        let reference = NSBezierPath::bezierPath();
        reference.setLineWidth(1.0);
        let dash = [3.0f64, 3.0];
        // SAFETY: `dash` holds two values.
        unsafe { reference.setLineDash_count_phase(dash.as_ptr().cast_mut().cast(), 2, 0.0) };
        let dir = if angle.abs() > 90.0 { -1.0 } else { 1.0 };
        line(&reference, (ax, ay), (ax + dir * (radius + 10.0), ay));
        color(rgb, 0.7).setStroke();
        reference.stroke();
        let arc = NSBezierPath::bezierPath();
        arc.setLineWidth(1.0);
        let start = if dir > 0.0 { 0.0 } else { 180.0f64.copysign(angle) };
        let steps = 24;
        for i in 0..=steps {
            let a = (start + (angle - start) * i as f64 / steps as f64).to_radians();
            let p = NSPoint::new(ax + radius * a.cos(), ay - radius * a.sin());
            if i == 0 { arc.moveToPoint(p) } else { arc.lineToPoint(p) }
        }
        accent.setStroke();
        arc.stroke();
    }

    let (len, angle_text) = s.line_texts((x0, y0, x1, y1), o.scale);
    let text = format!("{len}  ∠ {angle_text}");
    let mid = NSPoint::new((ax + bx) / 2.0, (ay + by) / 2.0);
    pill_centered(&text, NSPoint::new(mid.x, mid.y - 16.0), o.size, 12.0, &accent);
}

fn draw_area(s: &Session, o: &Overlay, r: PxRect, rgb: [f64; 3]) {
    let rect = o.rect_pt(r);
    color(rgb, 0.12).setFill();
    NSBezierPath::fillRect(rect);
    let path = NSBezierPath::bezierPathWithRect(rect);
    path.setLineWidth(1.0);
    color(rgb, 1.0).setStroke();
    path.stroke();

    // Width above the top edge and height beside the right edge, like
    // dimension lines on a drawing; the full size below.
    let accent = color(rgb, 1.0);
    let (x0, y0) = (rect.origin.x, rect.origin.y);
    let (x1, y1) = (x0 + rect.size.width, y0 + rect.size.height);
    if rect.size.width > 40.0 {
        pill_centered(&s.len_text(r.width(), o.scale), NSPoint::new((x0 + x1) / 2.0, y0 - 13.0), o.size, 11.0, &dark());
    }
    if rect.size.height > 30.0 {
        let label = s.len_text(r.height(), o.scale);
        let w = pill_size(&label, 11.0).width;
        pill_centered(&label, NSPoint::new(x1 + 6.0 + w / 2.0, (y0 + y1) / 2.0), o.size, 11.0, &dark());
    }
    let mut text = s.size_text(r.width(), r.height(), o.scale);
    if s.settings.show_aspect_ratio {
        text = format!("{text}  ({})", aspect_ratio(r.width(), r.height()));
    }
    let below = y1 + 16.0;
    let y = if below + 16.0 < o.size.height { below } else { y1 - 16.0 };
    pill_centered(&text, NSPoint::new((x0 + x1) / 2.0, y), o.size, 12.0, &accent);
}

/// Guides as full lines, with the distances between them (and to the
/// screen edges) along the top and left.
fn draw_guides(s: &Session, o: &Overlay, index: usize, rgb: [f64; 3]) {
    let (mut v, mut h) = s.guides(index);
    if v.is_empty() && h.is_empty() {
        return;
    }
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(1.0);
    for &x in &v {
        let px = o.pt(x);
        line(&path, (px, 0.0), (px, o.size.height));
    }
    for &y in &h {
        let py = o.pt(y);
        line(&path, (0.0, py), (o.size.width, py));
    }
    color(rgb, 0.85).setStroke();
    path.stroke();

    let fill = dark();
    if !v.is_empty() {
        v.extend([0, o.shot.frame.width]);
        v.sort_unstable();
        v.dedup();
        for pair in v.windows(2) {
            let (a, b) = (o.pt(pair[0]), o.pt(pair[1]));
            if b - a > 30.0 {
                pill_centered(
                    &s.len_text(pair[1] - pair[0], o.scale),
                    NSPoint::new((a + b) / 2.0, 52.0),
                    o.size,
                    11.0,
                    &fill,
                );
            }
        }
    }
    if !h.is_empty() {
        h.extend([0, o.shot.frame.height]);
        h.sort_unstable();
        h.dedup();
        for pair in h.windows(2) {
            let (a, b) = (o.pt(pair[0]), o.pt(pair[1]));
            if b - a > 24.0 {
                pill_centered(
                    &s.len_text(pair[1] - pair[0], o.scale),
                    NSPoint::new(40.0, (a + b) / 2.0),
                    o.size,
                    11.0,
                    &fill,
                );
            }
        }
    }
}

/// Shift held: lines across the whole screen through the cursor, for
/// checking alignment.
fn draw_alignment(o: &Overlay, x: usize, y: usize, rgb: [f64; 3]) {
    let (cx, cy) = ((x as f64 + 0.5) / o.scale, (y as f64 + 0.5) / o.scale);
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(1.0);
    let dash = [4.0f64, 3.0];
    // SAFETY: `dash` holds two values.
    unsafe { path.setLineDash_count_phase(dash.as_ptr().cast_mut().cast(), 2, 0.0) };
    line(&path, (0.0, cy), (o.size.width, cy));
    line(&path, (cx, 0.0), (cx, o.size.height));
    color(rgb, 0.6).setStroke();
    path.stroke();
}

fn draw_loupe(s: &Session, o: &Overlay, x: usize, y: usize, cursor: NSPoint, rgb: [f64; 3]) {
    let zoom = f64::from(s.settings.loupe_zoom.max(1));
    // An odd number of pixels, so the one under the pointer is in the middle.
    let radius = ((LOUPE_SIZE / zoom / 2.0).floor() as usize).max(1);
    let n = radius * 2 + 1;
    let side = n as f64 * zoom;
    let mut origin = NSPoint::new(cursor.x + 24.0, cursor.y + 36.0);
    if origin.x + side > o.size.width - 4.0 {
        origin.x = cursor.x - 24.0 - side;
    }
    if origin.y + side + 30.0 > o.size.height - 4.0 {
        origin.y = cursor.y - 24.0 - side - 30.0;
    }
    let dest = NSRect::new(origin, NSSize::new(side, side));

    // Source rectangle in the image's points (NSImage: bottom-left origin).
    let src_x = x as f64 - radius as f64;
    let src_y = y as f64 - radius as f64;
    let src = NSRect::new(
        NSPoint::new(src_x / o.scale, o.size.height - (src_y + n as f64) / o.scale),
        NSSize::new(n as f64 / o.scale, n as f64 / o.scale),
    );

    let Some(ctx) = NSGraphicsContext::currentContext() else { return };
    ctx.saveGraphicsState();
    let clip = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(dest, 10.0, 10.0);
    NSColor::blackColor().setFill();
    clip.fill();
    clip.addClip();
    ctx.setImageInterpolation(NSImageInterpolation::None);
    // SAFETY: plain drawing; `respectFlipped` keeps the image upright in the flipped view.
    unsafe {
        o.image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
            dest,
            src,
            NSCompositingOperation::Copy,
            1.0,
            true,
            None,
        )
    };
    // Pixel grid, when the pixels are big enough to see it.
    let grid = NSBezierPath::bezierPath();
    grid.setLineWidth(0.5);
    for i in (1..n).filter(|_| zoom >= 6.0) {
        let d = i as f64 * zoom;
        line(&grid, (origin.x + d, origin.y), (origin.x + d, origin.y + side));
        line(&grid, (origin.x, origin.y + d), (origin.x + side, origin.y + d));
    }
    NSColor::colorWithWhite_alpha(0.5, 0.25).setStroke();
    grid.stroke();
    // The pixel under the cursor.
    let at = radius as f64 * zoom;
    let mark = NSBezierPath::bezierPathWithRect(NSRect::new(
        NSPoint::new(origin.x + at, origin.y + at),
        NSSize::new(zoom, zoom),
    ));
    mark.setLineWidth(1.5);
    color(rgb, 1.0).setStroke();
    mark.stroke();
    ctx.restoreGraphicsState();
    let border = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(dest, 10.0, 10.0);
    border.setLineWidth(1.0);
    NSColor::colorWithWhite_alpha(1.0, 0.75).setStroke();
    border.stroke();

    // The color in the chosen format, and where the pointer is.
    let format = match s.settings.color_format {
        // The long code formats would not fit; they show as hex here.
        ColorFormat::SwiftUI | ColorFormat::AppKit => ColorFormat::Hex,
        f => f,
    };
    let at = |px: usize| format_length(px, o.scale, s.in_pixels());
    let text = format!("{}   {}, {}", format.format(o.shot.frame.pixel(x, y)), at(x), at(y));
    pill(&text, NSPoint::new(origin.x, origin.y + side + 5.0), o.size, 11.0, &dark());
}

const HINTS: [&str; 2] = [
    "Drag: area  ·  ⌘/right-drag: no snap  ·  ⌥-drag or L: line  ·  ⇧: crosshair, square, 45°  ·  Click: keep  ·  \
     V H: guides  ·  ⌥ arrows: next edge  ·  X: directions  ·  Scroll or [ ]: tolerance",
    "⌘C copy  ·  ⌘S ⇧⌘C screenshot  ·  P color  ·  ⇧P format  ·  ⌥P average  ·  Z loupe  ·  - + zoom  ·  \
     F freeze  ·  U units  ·  ⌫ undo  ·  C clear  ·  R restore  ·  ? hints  ·  Esc done",
];

fn draw(index: usize) {
    SESSION.with(|s| {
        let s = s.borrow();
        let Some(s) = s.as_ref() else { return };
        let Some(o) = s.overlays.get(index) else { return };
        let rgb = s.settings.rgb();

        if s.settings.freeze {
            o.image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), o.size));
        }

        draw_guides(s, o, index, rgb);
        for (_, item) in s.items.iter().filter(|(i, _)| *i == index) {
            match item {
                Item::Area(r) => draw_area(s, o, *r, rgb),
                Item::Cross { x, y, cross, axes } => draw_cross(s, o, *x, *y, cross, *axes, rgb),
                Item::Line { x0, y0, x1, y1 } => draw_line(s, o, (*x0, *y0, *x1, *y1), rgb),
                Item::VerticalGuide { .. } | Item::HorizontalGuide { .. } => {}
            }
        }

        if index == s.active {
            if let Some(d) = s.drag {
                if d.line {
                    let px = |v: f64| v * o.scale;
                    draw_line(s, o, (px(d.start.x), px(d.start.y), px(d.end.x), px(d.end.y)), rgb);
                } else {
                    draw_area(s, o, o.rect_px(d.start, d.end), rgb);
                }
            } else if let Some(p) = s.hover {
                let (x, y) = o.px(p);
                if s.shift {
                    draw_alignment(o, x, y, rgb);
                }
                let c = s.cross_at(index, x, y);
                draw_cross(s, o, x, y, &c, s.settings.axes, rgb);
                if s.settings.loupe {
                    draw_loupe(s, o, x, y, p, rgb);
                }
            }
        }

        if s.settings.show_hints {
            for (row, hint) in HINTS.iter().enumerate() {
                let w = pill_size(hint, 11.0).width;
                let y = o.size.height - 58.0 + row as f64 * 22.0;
                pill(hint, NSPoint::new((o.size.width - w) / 2.0, y), o.size, 11.0, &dark());
            }
        }

        if index == s.active
            && let Some((text, at)) = &s.toast
            && at.elapsed() < TOAST_TIME
        {
            pill_centered(text, NSPoint::new(o.size.width / 2.0, o.size.height / 2.0), o.size, 15.0, &dark());
        }
    });
}
