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
use objc2::runtime::AnyObject;
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
use crate::measure::{Cross, PxRect, aspect_ratio, clip_cross, format_length, snap_to};
use crate::settings::{Settings, Units};

/// `NSScreenSaverWindowLevel`: above menus, the Dock and full-screen apps.
const LEVEL: isize = 1000;
const TOAST_TIME: Duration = Duration::from_millis(1400);
/// A drag shorter than this (in points) counts as a click.
const CLICK_SLOP: f64 = 3.0;
/// New guides snap to an edge this close (in points).
const GUIDE_SNAP: f64 = 6.0;
/// Pixels each side of the cursor shown in the magnifier.
const LOUPE_RADIUS: usize = 8;
const LOUPE_ZOOM: f64 = 8.0;
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
        let p = self.location(event);
        let index = self.index();
        update(|s| {
            s.active = index;
            s.hover = Some(p);
            s.drag = Some(Drag { start: p, end: p });
        });
    }

    fn dragged(&self, event: &NSEvent) {
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
                    s.finish_drag(index, d.start, d.end, no_snap);
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

#[derive(Debug, Clone, Copy)]
struct Drag {
    start: NSPoint,
    end: NSPoint,
}

/// Something kept on screen, all in pixels.
#[derive(Debug, Clone, Copy)]
enum Item {
    /// Distances around a clicked point.
    Cross {
        x: usize,
        y: usize,
        cross: Cross,
    },
    Area(PxRect),
    VerticalGuide(usize),
    HorizontalGuide(usize),
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
}

/// Items of the last session per display, for restoring.
type Saved = Vec<(u32, Item)>;

impl Session {
    fn in_pixels(&self) -> bool {
        self.settings.units == Units::Pixels
    }

    fn len_text(&self, px: usize, scale: f64) -> String {
        let n = format_length(px, scale, self.in_pixels());
        if self.settings.show_units { format!("{n} {}", if self.in_pixels() { "px" } else { "pt" }) } else { n }
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
                Item::VerticalGuide(x) => v.push(*x),
                Item::HorizontalGuide(y) => h.push(*y),
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

    fn finish_drag(&mut self, index: usize, start: NSPoint, end: NSPoint, no_snap: bool) {
        let Some(o) = self.overlays.get(index) else { return };
        let moved = (end.x - start.x).abs().max((end.y - start.y).abs());
        let item = if moved < CLICK_SLOP {
            let (x, y) = o.px(end);
            Item::Cross { x, y, cross: self.cross_at(index, x, y) }
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
            Item::VerticalGuide(snap_to(x, &[c.left, c.right], max))
        } else {
            Item::HorizontalGuide(snap_to(y, &[c.top, c.bottom], max))
        };
        self.items.push((self.active, item));
    }

    /// The text to copy: the last kept measurement, or the one under the cursor.
    fn copy_text(&self) -> Option<String> {
        let last = self.items.iter().rev().find_map(|(i, item)| match item {
            Item::Area(r) => Some((*i, r.width(), r.height())),
            Item::Cross { cross, .. } => Some((*i, cross.width(), cross.height())),
            _ => None,
        });
        let (i, w, h) = match last {
            Some(m) => m,
            None => {
                let (x, y) = self.hover_px()?;
                let c = self.cross_at(self.active, x, y);
                (self.active, c.width(), c.height())
            }
        };
        Some(self.size_text(w, h, self.overlays[i].scale))
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

    fn saved_items(&self) -> Saved {
        self.items.iter().map(|(i, item)| (self.overlays[*i].shot.display_id, *item)).collect()
    }

    fn restore(&mut self, saved: &Saved) -> usize {
        let mut n = 0;
        for (display, item) in saved {
            if let Some(i) = self.overlays.iter().position(|o| o.shot.display_id == *display) {
                self.items.push((i, *item));
                n += 1;
            }
        }
        n
    }
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static LAST: RefCell<Saved> = const { RefCell::new(Vec::new()) };
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
    if !session.items.is_empty() {
        LAST.with(|l| *l.borrow_mut() = session.saved_items());
    }
    for o in &session.overlays {
        o.window.orderOut(None);
    }
    NSCursor::unhide();
    drop(session);
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
            s.items.retain(|(_, item)| !matches!(item, Item::VerticalGuide(_) | Item::HorizontalGuide(_)));
            s.toast("Guides cleared");
        }),
        (false, "c") => update(|s| {
            s.items.clear();
            s.toast("Cleared");
        }),
        (false, "r") => {
            let saved = LAST.with(|l| l.borrow().clone());
            update(|s| {
                let n = s.restore(&saved);
                s.toast(if n == 0 { "Nothing to restore".to_string() } else { format!("Restored {n} items") });
            });
        }
        (false, "p") => copy_color(),
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
        Some((i, Item::VerticalGuide(x))) if *i == active && dx != 0.0 => {
            *x = (*x as isize + dx as isize * step_px).clamp(0, w) as usize;
            true
        }
        Some((i, Item::HorizontalGuide(y))) if *i == active && dy != 0.0 => {
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

/// Copies the color under the cursor as "#RRGGBB".
fn copy_color() {
    let hex = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (x, y) = s.hover_px()?;
        let [r, g, b] = s.overlays[s.active].shot.frame.pixel(x, y);
        Some(format!("#{r:02X}{g:02X}{b:02X}"))
    });
    if let Some(hex) = hex {
        set_clipboard_string(&hex);
        update(|s| s.toast(format!("Copied {hex}")));
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
fn draw_cross(s: &Session, o: &Overlay, x: usize, y: usize, c: &Cross, rgb: [f64; 3]) {
    let (cx, cy) = ((x as f64 + 0.5) / o.scale, (y as f64 + 0.5) / o.scale);
    let (l, r) = (o.pt(c.left), o.pt(c.right));
    let (t, b) = (o.pt(c.top), o.pt(c.bottom));
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(1.0);
    line(&path, (l, cy), (r, cy));
    line(&path, (cx, t), (cx, b));
    let tick = 5.0;
    for ex in [l, r] {
        line(&path, (ex, cy - tick), (ex, cy + tick));
    }
    for ey in [t, b] {
        line(&path, (cx - tick, ey), (cx + tick, ey));
    }
    color(rgb, 1.0).setStroke();
    path.stroke();

    let accent = color(rgb, 1.0);
    let text = s.size_text(c.width(), c.height(), o.scale);
    pill(&text, NSPoint::new(cx + 10.0, cy + 10.0), o.size, 12.0, &accent);
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

fn draw_loupe(o: &Overlay, x: usize, y: usize, cursor: NSPoint, rgb: [f64; 3]) {
    let n = LOUPE_RADIUS * 2 + 1;
    let side = n as f64 * LOUPE_ZOOM;
    let mut origin = NSPoint::new(cursor.x + 24.0, cursor.y + 36.0);
    if origin.x + side > o.size.width - 4.0 {
        origin.x = cursor.x - 24.0 - side;
    }
    if origin.y + side + 30.0 > o.size.height - 4.0 {
        origin.y = cursor.y - 24.0 - side - 30.0;
    }
    let dest = NSRect::new(origin, NSSize::new(side, side));

    // Source rectangle in the image's points (NSImage: bottom-left origin).
    let src_x = x as f64 - LOUPE_RADIUS as f64;
    let src_y = y as f64 - LOUPE_RADIUS as f64;
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
    // Pixel grid.
    let grid = NSBezierPath::bezierPath();
    grid.setLineWidth(0.5);
    for i in 1..n {
        let d = i as f64 * LOUPE_ZOOM;
        line(&grid, (origin.x + d, origin.y), (origin.x + d, origin.y + side));
        line(&grid, (origin.x, origin.y + d), (origin.x + side, origin.y + d));
    }
    NSColor::colorWithWhite_alpha(0.5, 0.25).setStroke();
    grid.stroke();
    // The pixel under the cursor.
    let at = LOUPE_RADIUS as f64 * LOUPE_ZOOM;
    let mark = NSBezierPath::bezierPathWithRect(NSRect::new(
        NSPoint::new(origin.x + at, origin.y + at),
        NSSize::new(LOUPE_ZOOM, LOUPE_ZOOM),
    ));
    mark.setLineWidth(1.5);
    color(rgb, 1.0).setStroke();
    mark.stroke();
    ctx.restoreGraphicsState();
    let border = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(dest, 10.0, 10.0);
    border.setLineWidth(1.0);
    NSColor::colorWithWhite_alpha(1.0, 0.75).setStroke();
    border.stroke();

    let [r, g, b] = o.shot.frame.pixel(x, y);
    pill(&format!("#{r:02X}{g:02X}{b:02X}"), NSPoint::new(origin.x, origin.y + side + 5.0), o.size, 11.0, &dark());
}

const HINTS: &str = "Drag: area  ·  ⌘/right-drag: no snap  ·  ⇧: crosshair  ·  Click: keep  ·  V H: guides  ·  \
                     ⌘C copy  ·  ⌘S ⇧⌘C screenshot  ·  P color  ·  Z loupe  ·  F freeze  ·  U units  ·  [ ] tolerance  ·  \
                     ⌫ undo  ·  C clear  ·  R restore  ·  ? hints  ·  Esc done";

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
                Item::Cross { x, y, cross } => draw_cross(s, o, *x, *y, cross, rgb),
                Item::VerticalGuide(_) | Item::HorizontalGuide(_) => {}
            }
        }

        if index == s.active {
            if let Some(d) = s.drag {
                draw_area(s, o, o.rect_px(d.start, d.end), rgb);
            } else if let Some(p) = s.hover {
                let (x, y) = o.px(p);
                if s.shift {
                    draw_alignment(o, x, y, rgb);
                }
                let c = s.cross_at(index, x, y);
                draw_cross(s, o, x, y, &c, rgb);
                if s.settings.loupe {
                    draw_loupe(o, x, y, p, rgb);
                }
            }
        }

        if s.settings.show_hints {
            let w = pill_size(HINTS, 11.0).width;
            pill(HINTS, NSPoint::new((o.size.width - w) / 2.0, o.size.height - 36.0), o.size, 11.0, &dark());
        }

        if index == s.active
            && let Some((text, at)) = &s.toast
            && at.elapsed() < TOAST_TIME
        {
            pill_centered(text, NSPoint::new(o.size.width / 2.0, o.size.height / 2.0), o.size, 15.0, &dark());
        }
    });
}
