//! Floating rulers: panels with tick marks in the chosen units that stay on
//! screen while you work. Drag one to move it, drag its far end to change
//! its length, and right-click it to rotate it, change units, or close it.
//! A clicked ruler takes the arrow keys (⇧: ten points), R to rotate, and ⌫
//! to close. A line follows the pointer along each ruler, with its
//! position. Rulers are saved in `rulers.toml` and come back at launch.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSBezierPath, NSColor, NSEvent, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSMenu, NSMenuItem, NSPanel, NSScreen, NSStringDrawing, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSDictionary, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer};

use serde::{Deserialize, Serialize};

use crate::settings::{Settings, Units};
use crate::units::Scale;

/// A ruler as saved: its frame in Cocoa screen points (bottom-left origin).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedRuler {
    pub vertical: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedRulers {
    #[serde(default)]
    pub rulers: Vec<SavedRuler>,
}

impl SavedRulers {
    pub fn path() -> std::path::PathBuf {
        Settings::path().with_file_name("rulers.toml")
    }

    pub fn load_from(path: &std::path::Path) -> SavedRulers {
        std::fs::read_to_string(path).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml::to_string(self)?)?;
        Ok(())
    }
}

/// How tall (or wide, when vertical) a ruler is, in points.
const THICKNESS: f64 = 34.0;
const DEFAULT_LENGTH: f64 = 600.0;
const MIN_LENGTH: f64 = 120.0;
/// The grip at the far end that changes the length.
const GRIP: f64 = 14.0;

/// How big a tick is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickSize {
    Minor,
    Middle,
    Major,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    /// Distance from the start of the ruler, in points.
    pub at: f64,
    pub size: TickSize,
    /// The value at major ticks.
    pub label: Option<String>,
}

/// "Nice" steps: 1, 2, and 5 times a power of ten.
fn nice_steps() -> impl Iterator<Item = f64> {
    (-3..7).flat_map(|e| [1.0, 2.0, 5.0].map(|m| m * 10f64.powi(e)))
}

/// The ticks of a ruler `length` points long, in `units` on a display with
/// `scale`: minor ticks at least 4 points apart, and labeled major ticks
/// at least 40 points apart.
pub fn ticks(length: f64, scale: &Scale, units: Units) -> Vec<Tick> {
    // Points per unit.
    let per_unit = 1.0 / scale.value(scale.px_per_pt, units);
    if !per_unit.is_finite() || per_unit <= 0.0 {
        return Vec::new();
    }
    let Some(minor) = nice_steps().find(|s| s * per_unit >= 4.0) else { return Vec::new() };
    let Some(major) = nice_steps().find(|s| *s >= minor * 2.0 && s * per_unit >= 40.0) else { return Vec::new() };
    let ratio = (major / minor).round() as i64;
    let count = (length / (minor * per_unit)).floor() as i64;
    (0..=count)
        .map(|i| {
            let value = i as f64 * minor;
            let size = if i % ratio == 0 {
                TickSize::Major
            } else if ratio % 2 == 0 && i % (ratio / 2) == 0 {
                TickSize::Middle
            } else {
                TickSize::Minor
            };
            let label = (size == TickSize::Major).then(|| number(value));
            Tick { at: value * per_unit, size, label }
        })
        .collect()
}

/// A value without needless decimals: "12", "2.5", "0.25".
fn number(v: f64) -> String {
    let s = format!("{:.3}", (v * 1000.0).round() / 1000.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The pointer's position along a ruler, as its label shows it (with the
/// unit when `suffix`; vertical rulers are too narrow for it).
pub fn position_text(at_pt: f64, scale: &Scale, units: Units, suffix: bool) -> String {
    scale.format(at_pt * scale.px_per_pt, units, suffix)
}

#[derive(Debug, Default)]
pub struct RulerIvars {
    vertical: Cell<bool>,
    /// Dragging the far end: the length when the drag started, and where.
    resizing: Cell<Option<(f64, NSPoint)>>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "PixlSnapzRulerView"]
    #[ivars = RulerIvars]
    pub struct RulerView;

    impl RulerView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let p = self.convertPoint_fromView(event.locationInWindow(), None);
            let length = self.length();
            let along = if self.ivars().vertical.get() { p.y } else { p.x };
            if along >= length - GRIP {
                self.ivars().resizing.set(Some((length, NSEvent::mouseLocation())));
            } else if let Some(window) = self.window() {
                window.performWindowDragWithEvent(event);
            }
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {
            let Some((start, from)) = self.ivars().resizing.get() else { return };
            let Some(window) = self.window() else { return };
            let now = NSEvent::mouseLocation();
            let vertical = self.ivars().vertical.get();
            // Screen coordinates grow upward: a vertical ruler grows down.
            let delta = if vertical { from.y - now.y } else { now.x - from.x };
            let length = (start + delta).max(MIN_LENGTH);
            let mut frame = window.frame();
            if vertical {
                let top = frame.origin.y + frame.size.height;
                frame.size.height = length;
                frame.origin.y = top - length;
            } else {
                frame.size.width = length;
            }
            window.setFrame_display(frame, true);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            self.ivars().resizing.set(None);
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.show_menu(event);
        }

        #[unsafe(method(rulerAction:))]
        fn ruler_action(&self, sender: &NSMenuItem) {
            self.action(sender.tag());
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        /// Arrow keys move the ruler (⇧: ten points), R rotates it, and ⌫
        /// closes it.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let step = if event.modifierFlags().contains(objc2_app_kit::NSEventModifierFlags::Shift) { 10.0 } else { 1.0 };
            let (dx, dy) = match event.keyCode() {
                123 => (-step, 0.0),
                124 => (step, 0.0),
                125 => (0.0, -step),
                126 => (0.0, step),
                51 | 117 => {
                    self.action(CLOSE);
                    return;
                }
                _ => {
                    let key = event.charactersIgnoringModifiers().map(|c| c.to_string().to_lowercase());
                    if key.as_deref() == Some("r") {
                        self.action(ROTATE);
                    }
                    return;
                }
            };
            if let Some(window) = self.window() {
                let mut frame = window.frame();
                frame.origin.x += dx;
                frame.origin.y += dy;
                window.setFrame_display(frame, true);
            }
        }
    }

    unsafe impl NSObjectProtocol for RulerView {}
);

define_class!(
    /// A ruler's panel: borderless, yet it can take the keyboard when
    /// clicked (without making PixlSnapz the active app).
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "PixlSnapzRulerPanel"]
    pub struct RulerPanel;

    impl RulerPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }
    }

    unsafe impl NSObjectProtocol for RulerPanel {}
);

const ROTATE: isize = 1;
const CLOSE: isize = 2;
const UNITS_BASE: isize = 100;

impl RulerView {
    fn length(&self) -> f64 {
        let size = self.bounds().size;
        if self.ivars().vertical.get() { size.height } else { size.width }
    }

    /// The display's scale, for pixels, millimeters, and inches.
    fn scale(&self, settings: &Settings) -> Scale {
        let screen = self.window().and_then(|w| w.screen()).or_else(|| NSScreen::mainScreen(self.mtm()));
        let Some(screen) = screen else { return Scale::new(2.0, 0, 0.0, settings.screen_ppi, 100) };
        let px_per_pt = screen.backingScaleFactor();
        let id = screen
            .deviceDescription()
            .objectForKey(&NSString::from_str("NSScreenNumber"))
            .and_then(|n| n.downcast::<NSNumber>().ok())
            .map_or(0, |n| n.unsignedIntValue());
        let width_mm = objc2_core_graphics::CGDisplayScreenSize(id).width;
        let width_px = (screen.frame().size.width * px_per_pt) as usize;
        Scale::new(px_per_pt, width_px, width_mm, settings.screen_ppi, 100)
    }

    fn draw(&self) {
        let settings = Settings::load();
        let scale = self.scale(&settings);
        let vertical = self.ivars().vertical.get();
        let bounds = self.bounds();
        let length = self.length();

        let body = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, 4.0, 4.0);
        NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 0.98, 0.9, 0.92).setFill();
        body.fill();
        NSColor::colorWithWhite_alpha(0.3, 0.6).setStroke();
        body.setLineWidth(1.0);
        body.stroke();

        // Ticks hang from the top edge (the left edge when vertical).
        let ink = NSColor::colorWithWhite_alpha(0.15, 1.0);
        let path = NSBezierPath::bezierPath();
        path.setLineWidth(1.0);
        let at =
            |along: f64, across: f64| if vertical { NSPoint::new(across, along) } else { NSPoint::new(along, across) };
        let font = NSFont::monospacedDigitSystemFontOfSize_weight(9.0, 0.0);
        let attrs = text_attributes(&font, &ink);
        for tick in ticks(length - GRIP, &scale, settings.units) {
            let x = tick.at.floor() + 0.5;
            let size = match tick.size {
                TickSize::Major => 14.0,
                TickSize::Middle => 9.0,
                TickSize::Minor => 5.0,
            };
            path.moveToPoint(at(x, 0.0));
            path.lineToPoint(at(x, size));
            if let Some(label) = tick.label {
                let text = NSString::from_str(&label);
                // SAFETY: drawing a string with valid attributes.
                unsafe { text.drawAtPoint_withAttributes(at(x + 2.0, 15.0), Some(&attrs)) };
            }
        }
        ink.setStroke();
        path.stroke();

        // The grip at the far end.
        let grip = NSBezierPath::bezierPath();
        for i in 0..3 {
            let a = length - GRIP + 4.0 + f64::from(i) * 3.0;
            grip.moveToPoint(at(a, THICKNESS * 0.3));
            grip.lineToPoint(at(a, THICKNESS * 0.7));
        }
        NSColor::colorWithWhite_alpha(0.4, 0.8).setStroke();
        grip.stroke();

        // The pointer's position along the ruler.
        if let Some(window) = self.window() {
            let p = window.convertPointFromScreen(NSEvent::mouseLocation());
            let p = self.convertPoint_fromView(p, None);
            let along = if vertical { p.y } else { p.x };
            let across = if vertical { p.x } else { p.y };
            if (0.0..length - GRIP).contains(&along) && (-400.0..400.0).contains(&across) {
                let red = NSColor::systemRedColor();
                let marker = NSBezierPath::bezierPath();
                marker.setLineWidth(1.0);
                marker.moveToPoint(at(along.floor() + 0.5, 0.0));
                marker.lineToPoint(at(along.floor() + 0.5, THICKNESS));
                red.setStroke();
                marker.stroke();
                let label = NSString::from_str(&position_text(along, &scale, settings.units, !vertical));
                let label_attrs = text_attributes(&font, &red);
                let place = if vertical { NSPoint::new(2.0, along + 2.0) } else { at(along + 3.0, THICKNESS - 12.0) };
                // SAFETY: as above.
                unsafe { label.drawAtPoint_withAttributes(place, Some(&label_attrs)) };
            }
        }
    }

    fn show_menu(&self, event: &NSEvent) {
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        let add = |menu: &NSMenu, title: &str, tag: isize, on: bool| {
            // SAFETY: the action exists on this view, which outlives the menu.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(&crate::i18n::tr(title)),
                    Some(sel!(rulerAction:)),
                    &NSString::from_str(""),
                )
            };
            item.setTag(tag);
            // SAFETY: as above.
            unsafe { item.setTarget(Some(self)) };
            if on {
                item.setState(objc2_app_kit::NSControlStateValueOn);
            }
            menu.addItem(&item);
        };
        add(&menu, n!("Rotate"), ROTATE, false);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let units = Settings::load().units;
        for (i, u) in Units::ALL.iter().enumerate() {
            add(&menu, u.name(), UNITS_BASE + i as isize, *u == units);
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add(&menu, n!("Close Ruler"), CLOSE, false);
        NSMenu::popUpContextMenu_withEvent_forView(&menu, event, self);
    }

    fn action(&self, tag: isize) {
        let Some(window) = self.window() else { return };
        match tag {
            ROTATE => {
                let vertical = !self.ivars().vertical.get();
                self.ivars().vertical.set(vertical);
                // Turn around the top-left corner.
                let f = window.frame();
                let top = f.origin.y + f.size.height;
                let size = NSSize::new(f.size.height, f.size.width);
                let frame = NSRect::new(NSPoint::new(f.origin.x, top - size.height), size);
                // Kept on the screen, which the turned ruler may run off.
                let frame = window.constrainFrameRect_toScreen(frame, window.screen().as_deref());
                window.setFrame_display(frame, true);
            }
            CLOSE => {
                window.close();
                let closed = Retained::as_ptr(&window).cast::<AnyObject>();
                RULERS.with(|r| r.borrow_mut().retain(|w| Retained::as_ptr(w).cast::<AnyObject>() != closed));
                save();
            }
            t if (UNITS_BASE..UNITS_BASE + Units::ALL.len() as isize).contains(&t) => {
                let mut s = Settings::load();
                s.units = Units::ALL[(t - UNITS_BASE) as usize];
                if let Err(e) = s.save() {
                    tracing::warn!("saving settings: {e:#}");
                }
                redraw_all();
            }
            _ => {}
        }
    }
}

fn text_attributes(font: &NSFont, color: &NSColor) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: static attribute name constants.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font, color];
    NSDictionary::from_slices(&keys, &values)
}

thread_local! {
    static RULERS: RefCell<Vec<Retained<RulerPanel>>> = const { RefCell::new(Vec::new()) };
    /// What `rulers.toml` holds, to save only changes.
    static SAVED: RefCell<Vec<SavedRuler>> = const { RefCell::new(Vec::new()) };
    static TIMER: RefCell<Option<Retained<NSTimer>>> = const { RefCell::new(None) };
}

/// Opens a ruler in the middle of the screen with the pointer.
pub fn new_ruler(vertical: bool, mtm: MainThreadMarker) {
    let screen = NSScreen::mainScreen(mtm);
    let visible = screen.map_or(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0)), |s| s.visibleFrame());
    let count = RULERS.with(|r| r.borrow().len()) as f64;
    let size = if vertical { NSSize::new(THICKNESS, DEFAULT_LENGTH) } else { NSSize::new(DEFAULT_LENGTH, THICKNESS) };
    // Each new ruler a little lower and to the right of the last.
    let origin = NSPoint::new(
        visible.origin.x + (visible.size.width - size.width) / 2.0 + count * 24.0,
        visible.origin.y + (visible.size.height - size.height) / 2.0 - count * 24.0,
    );
    open(vertical, NSRect::new(origin, size), mtm);
}

/// For screenshots: rulers from `spec`, such as `h:300,200,500;v:250,240,300`
/// (direction, then the top-left corner in screen points from the top-left
/// of the main screen, and the length).
pub fn open_from_spec(spec: &str, mtm: MainThreadMarker) {
    let height = NSScreen::mainScreen(mtm).map_or(0.0, |s| s.frame().size.height);
    for part in spec.split(';') {
        let Some((kind, numbers)) = part.split_once(':') else { continue };
        let n: Vec<f64> = numbers.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let [x, y, length] = n[..] else { continue };
        let vertical = kind.trim() == "v";
        let size = if vertical { NSSize::new(THICKNESS, length) } else { NSSize::new(length, THICKNESS) };
        open(vertical, NSRect::new(NSPoint::new(x, height - y - size.height), size), mtm);
    }
}

fn open(vertical: bool, frame: NSRect, mtm: MainThreadMarker) {
    let (origin, size) = (frame.origin, frame.size);
    // SAFETY: NSPanel's designated initializer.
    let panel: Retained<RulerPanel> = unsafe {
        msg_send![RulerPanel::alloc(mtm), initWithContentRect: NSRect::new(origin, size),
            styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            backing: NSBackingStoreType::Buffered, defer: false]
    };
    // SAFETY: the module keeps the panel.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(true);
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(false);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    let view = RulerView::alloc(mtm).set_ivars(RulerIvars::default());
    // SAFETY: NSView's designated initializer.
    let view: Retained<RulerView> =
        unsafe { msg_send![super(view), initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), size)] };
    view.ivars().vertical.set(vertical);
    view.setAutoresizingMask(
        objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable
            | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    view.setAccessibilityLabel(Some(&NSString::from_str(&t!("Ruler"))));
    panel.setContentView(Some(&view));
    panel.orderFrontRegardless();
    RULERS.with(|r| r.borrow_mut().push(panel));
    start_timer();
}

/// Closes every ruler.
pub fn close_all() {
    for panel in RULERS.with(|r| std::mem::take(&mut *r.borrow_mut())) {
        panel.close();
    }
    save();
}

/// The rulers on screen, as saved.
fn snapshot() -> Vec<SavedRuler> {
    RULERS.with(|r| {
        r.borrow()
            .iter()
            .map(|panel| {
                let f = panel.frame();
                let vertical = panel
                    .contentView()
                    .and_then(|v| v.downcast::<RulerView>().ok())
                    .is_some_and(|v| v.ivars().vertical.get());
                SavedRuler { vertical, x: f.origin.x, y: f.origin.y, width: f.size.width, height: f.size.height }
            })
            .collect()
    })
}

/// Saves the rulers if they changed since the last save.
fn save() {
    let now = snapshot();
    if SAVED.with(|s| *s.borrow() == now) {
        return;
    }
    if let Err(e) = (SavedRulers { rulers: now.clone() }).save_to(&SavedRulers::path()) {
        tracing::warn!("saving the rulers: {e:#}");
    }
    SAVED.with(|s| *s.borrow_mut() = now);
}

/// Opens the rulers saved last time (at launch).
pub fn restore(mtm: MainThreadMarker) {
    let saved = SavedRulers::load_from(&SavedRulers::path()).rulers;
    SAVED.with(|s| *s.borrow_mut() = saved.clone());
    for r in saved {
        let frame = NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.width, r.height));
        open(r.vertical, frame, mtm);
    }
    // Keep them on the screens there are now.
    RULERS.with(|r| {
        for panel in r.borrow().iter() {
            let frame = panel.constrainFrameRect_toScreen(panel.frame(), panel.screen().as_deref());
            panel.setFrame_display(frame, false);
        }
    });
}

pub fn count() -> usize {
    RULERS.with(|r| r.borrow().len())
}

fn redraw_all() {
    RULERS.with(|r| {
        for panel in r.borrow().iter() {
            if let Some(view) = panel.contentView() {
                view.setNeedsDisplay(true);
            }
        }
    });
}

/// Redraws the rulers 30 times a second while there are any, for the
/// pointer marker.
fn start_timer() {
    if TIMER.with(|t| t.borrow().is_some()) {
        return;
    }
    let last = Cell::new(NSPoint::new(f64::NAN, f64::NAN));
    let block = RcBlock::new(move |timer: NonNull<NSTimer>| {
        if count() == 0 {
            save();
            // SAFETY: the timer calling this block.
            unsafe { timer.as_ref() }.invalidate();
            TIMER.with(|t| *t.borrow_mut() = None);
            return;
        }
        // Moved, resized, or turned: saved once the mouse is up.
        if NSEvent::pressedMouseButtons() == 0 {
            save();
        }
        let p = NSEvent::mouseLocation();
        if p != last.get() {
            last.set(p);
            redraw_all();
        }
    });
    // SAFETY: a repeating timer on the main run loop; it stops itself.
    let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0 / 30.0, true, &block) };
    TIMER.with(|t| *t.borrow_mut() = Some(timer));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_ticks() {
        // Points on a 2x display: a minor tick every 5 pt, labels every 50.
        let scale = Scale::new(2.0, 3456, 0.0, 0.0, 100);
        let t = ticks(100.0, &scale, Units::Points);
        assert_eq!(t.len(), 21);
        assert_eq!((t[0].at, t[0].size, t[0].label.as_deref()), (0.0, TickSize::Major, Some("0")));
        assert_eq!((t[5].at, t[5].size), (25.0, TickSize::Middle));
        assert_eq!((t[10].at, t[10].size, t[10].label.as_deref()), (50.0, TickSize::Major, Some("50")));
        assert_eq!(t[1].size, TickSize::Minor);
        // Pixels: twice as many per point, so a minor tick every 10 px (5 pt).
        let t = ticks(100.0, &scale, Units::Pixels);
        assert_eq!(t[1].at, 5.0);
        assert_eq!(t[10].label.as_deref(), Some("100"));
        // Millimeters at 96 PPI: 1 mm is about 3.8 pt; ticks every 2 mm,
        // a middle one at 10, and labels every 20.
        let mm = Scale::new(1.0, 0, 0.0, 96.0, 100);
        let t = ticks(400.0, &mm, Units::Millimeters);
        assert_eq!(t[1].label, None);
        assert!((t[1].at - 2.0 * 96.0 / 25.4).abs() < 1e-9, "{}", t[1].at);
        assert_eq!(t[5].size, TickSize::Middle);
        assert_eq!(t[10].label.as_deref(), Some("20"));
        assert_eq!(number(0.25), "0.25");

        let saved =
            SavedRulers { rulers: vec![SavedRuler { vertical: true, x: 10.0, y: 20.5, width: 34.0, height: 300.0 }] };
        let dir = std::env::temp_dir().join(format!("pixlsnapz-rulers-{}", std::process::id()));
        let path = dir.join("rulers.toml");
        saved.save_to(&path).unwrap();
        assert_eq!(SavedRulers::load_from(&path), saved);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(number(12.0), "12");
    }
}
