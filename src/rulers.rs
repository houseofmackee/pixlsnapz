//! Floating rulers: panels with tick marks in the chosen units that stay on
//! screen while you work, at any angle. Drag one to move it, drag its far
//! end to change its length (with ⌥ to turn it around its start; ⇧ snaps to
//! 15°), and right-click it to turn it, straighten it, change units, or
//! close it. A clicked ruler takes the arrow keys (⇧: ten points), ⌥ arrows
//! to turn it by 1° (⇧: 15°), R to turn it 90°, and ⌫ to close it. A line
//! follows the pointer along each ruler, with its position. Rulers are saved
//! in `rulers.toml` and come back at launch.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSEvent, NSEventModifierFlags, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSMenu, NSMenuItem, NSPanel, NSScreen, NSStringDrawing, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    NSAffineTransform, NSCopying, NSDictionary, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer,
};
use serde::{Deserialize, Serialize};

use crate::settings::{Settings, Units};
use crate::units::Scale;

/// How wide a ruler's body is, across its length, in points.
const THICKNESS: f64 = 34.0;
const DEFAULT_LENGTH: f64 = 600.0;
const MIN_LENGTH: f64 = 120.0;
/// The grip at the far end that changes the length (and, with ⌥, the angle).
const GRIP: f64 = 14.0;
/// Room around the ruler inside its panel, for antialiasing.
const MARGIN: f64 = 3.0;

/// Where a ruler is, in screen points (Cocoa: the origin at the bottom left,
/// y growing upward).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RulerGeometry {
    /// The start of the tick edge (where 0 is).
    pub start: (f64, f64),
    pub length: f64,
    /// Degrees counterclockwise from pointing right, in `-180..=180`.
    pub angle: f64,
}

impl RulerGeometry {
    /// The unit vector along the ruler.
    pub fn direction(&self) -> (f64, f64) {
        let a = self.angle.to_radians();
        // Exact zeros at right angles, so level and upright rulers get
        // whole-point frames.
        let clean = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v };
        (clean(a.cos()), clean(a.sin()))
    }

    /// The unit vector across it, toward the body (below the tick edge of a
    /// level ruler).
    pub fn normal(&self) -> (f64, f64) {
        let (dx, dy) = self.direction();
        (dy, -dx)
    }

    /// The screen point `u` along and `v` across the ruler.
    pub fn point(&self, u: f64, v: f64) -> (f64, f64) {
        let ((dx, dy), (nx, ny)) = (self.direction(), self.normal());
        (self.start.0 + u * dx + v * nx, self.start.1 + u * dy + v * ny)
    }

    /// How far along and across the ruler a screen point is.
    pub fn local(&self, p: (f64, f64)) -> (f64, f64) {
        let ((dx, dy), (nx, ny)) = (self.direction(), self.normal());
        let (x, y) = (p.0 - self.start.0, p.1 - self.start.1);
        (x * dx + y * dy, x * nx + y * ny)
    }

    /// The rectangle around the ruler, `margin` points larger on every side:
    /// (x, y, width, height).
    pub fn bounds(&self, margin: f64) -> (f64, f64, f64, f64) {
        let corners = [
            self.point(0.0, 0.0),
            self.point(self.length, 0.0),
            self.point(0.0, THICKNESS),
            self.point(self.length, THICKNESS),
        ];
        let x0 = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min) - margin;
        let y0 = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min) - margin;
        let x1 = corners.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max) + margin;
        let y1 = corners.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max) + margin;
        (x0, y0, x1 - x0, y1 - y0)
    }

    /// Whether a screen point is on the ruler (within `slop` points).
    pub fn contains(&self, p: (f64, f64), slop: f64) -> bool {
        let (u, v) = self.local(p);
        (-slop..=self.length + slop).contains(&u) && (-slop..=THICKNESS + slop).contains(&v)
    }
}

/// `angle` in `-180..=180`, turned to the nearest multiple of `step` degrees
/// when `step` is above 0.
pub fn normalize_angle(angle: f64, step: f64) -> f64 {
    let a = if step > 0.0 { (angle / step).round() * step } else { angle };
    let a = (a + 180.0).rem_euclid(360.0) - 180.0;
    let a = if a <= -180.0 { a + 360.0 } else { a };
    // Exact zeros, without "-0".
    if a.abs() < 1e-9 { 0.0 } else { a }
}

/// A ruler as saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedRuler {
    /// The start (see `RulerGeometry`), or in files from 0.3.1, the panel's
    /// bottom-left corner.
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    /// 0.3.1: the panel's size, and whether it ran down the screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub vertical: bool,
}

impl SavedRuler {
    pub fn from_geometry(g: RulerGeometry) -> SavedRuler {
        SavedRuler { x: g.start.0, y: g.start.1, length: Some(g.length), angle: Some(g.angle), ..SavedRuler::default() }
    }

    pub fn geometry(&self) -> RulerGeometry {
        if let (Some(length), Some(angle)) = (self.length, self.angle) {
            return RulerGeometry { start: (self.x, self.y), length, angle };
        }
        // 0.3.1 kept panels: a level ruler with its ticks along the top, or
        // one running down with its ticks along the left. The same rectangle
        // now: level from the top-left corner, or down from the top-right.
        let (w, h) = (self.width.unwrap_or(DEFAULT_LENGTH), self.height.unwrap_or(THICKNESS));
        if self.vertical {
            RulerGeometry { start: (self.x + w, self.y + h), length: h, angle: -90.0 }
        } else {
            RulerGeometry { start: (self.x, self.y + h), length: w, angle: 0.0 }
        }
    }
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

/// The pointer's position along a ruler, as its label shows it.
pub fn position_text(at_pt: f64, scale: &Scale, units: Units) -> String {
    scale.format(at_pt * scale.px_per_pt, units, true)
}

/// What a drag on a ruler's far end does.
#[derive(Debug, Clone, Copy)]
enum Gesture {
    /// The length (from where the drag started).
    Length { length: f64, from: (f64, f64) },
    /// With ⌥: the angle around the start.
    Turn,
}

#[derive(Debug)]
pub struct RulerIvars {
    /// The start of the tick edge, in view points (flipped: y grows down).
    start: Cell<(f64, f64)>,
    length: Cell<f64>,
    angle: Cell<f64>,
    gesture: Cell<Option<Gesture>>,
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

        /// Only the ruler itself takes clicks, not the empty corners of its
        /// panel around a turned ruler.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            // SAFETY: a plain property read.
            let p = match unsafe { self.superview() } {
                Some(superview) => self.convertPoint_fromView(point, Some(&superview)),
                None => point,
            };
            if self.view_geometry().contains((p.x, -p.y), 2.0) {
                (self as *const RulerView).cast_mut().cast()
            } else {
                std::ptr::null_mut()
            }
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let p = self.convertPoint_fromView(event.locationInWindow(), None);
            let g = self.view_geometry();
            let (u, _) = g.local((p.x, -p.y));
            if u >= g.length - GRIP {
                let gesture = if event.modifierFlags().contains(NSEventModifierFlags::Option) {
                    Gesture::Turn
                } else {
                    let m = NSEvent::mouseLocation();
                    Gesture::Length { length: g.length, from: (m.x, m.y) }
                };
                self.ivars().gesture.set(Some(gesture));
            } else if let Some(window) = self.window() {
                window.performWindowDragWithEvent(event);
            }
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let Some(gesture) = self.ivars().gesture.get() else { return };
            let Some(mut g) = self.screen_geometry() else { return };
            let m = NSEvent::mouseLocation();
            match gesture {
                Gesture::Length { length, from } => {
                    let (dx, dy) = g.direction();
                    g.length = (length + (m.x - from.0) * dx + (m.y - from.1) * dy).max(MIN_LENGTH);
                }
                Gesture::Turn => {
                    let step = if event.modifierFlags().contains(NSEventModifierFlags::Shift) { 15.0 } else { 0.0 };
                    g.angle = normalize_angle((m.y - g.start.1).atan2(m.x - g.start.0).to_degrees(), step);
                }
            }
            self.set_screen_geometry(g);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            self.ivars().gesture.set(None);
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            let menu = self.menu_for_ruler();
            NSMenu::popUpContextMenu_withEvent_forView(&menu, event, self);
        }

        #[unsafe(method(rulerAction:))]
        fn ruler_action(&self, sender: &NSMenuItem) {
            self.action(sender.tag());
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        /// Arrow keys move the ruler (⇧: ten points), ⌥ arrows turn it by
        /// 1° (⇧: 15°), R turns it 90°, and ⌫ closes it.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let flags = event.modifierFlags();
            let shift = flags.contains(NSEventModifierFlags::Shift);
            let option = flags.contains(NSEventModifierFlags::Option);
            let step = if shift { 10.0 } else { 1.0 };
            let code = event.keyCode();
            if option && matches!(code, 123 | 124) {
                let by = if shift { 15.0 } else { 1.0 };
                self.turn(if code == 123 { by } else { -by }, if shift { 15.0 } else { 0.0 });
                return;
            }
            let (dx, dy) = match code {
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

        #[unsafe(method(isAccessibilityElement))]
        fn is_accessibility_element(&self) -> bool {
            true
        }

        #[unsafe(method_id(accessibilityRole))]
        fn accessibility_role(&self) -> Option<Retained<NSString>> {
            // SAFETY: a static constant.
            Some(unsafe { objc2_app_kit::NSAccessibilityRulerRole }.copy())
        }

        /// "Ruler, 600 pt long, at 30°".
        #[unsafe(method_id(accessibilityLabel))]
        fn accessibility_label(&self) -> Option<Retained<NSString>> {
            let settings = Settings::load();
            let scale = self.scale(&settings);
            let g = self.view_geometry();
            let length = position_text(g.length, &scale, settings.units);
            let angle = crate::geometry::format_angle(g.angle);
            Some(NSString::from_str(&t!("Ruler, {length} long, at {angle}", length, angle)))
        }

        /// The pointer's position along it, when it is over the ruler.
        #[unsafe(method_id(accessibilityValue))]
        fn accessibility_value(&self) -> Option<Retained<AnyObject>> {
            let settings = Settings::load();
            let text = self.pointer_along().map(|u| position_text(u, &self.scale(&settings), settings.units));
            text.map(|t| Retained::into_super(Retained::into_super(NSString::from_str(&t))))
        }

        #[unsafe(method_id(accessibilityHelp))]
        fn accessibility_help(&self) -> Option<Retained<NSString>> {
            Some(NSString::from_str(&t!(
                "Arrow keys move it, Option arrows turn it, R turns it 90 degrees, Delete closes it; more in its menu"
            )))
        }

        #[unsafe(method(accessibilityPerformShowMenu))]
        fn accessibility_perform_show_menu(&self) -> bool {
            let menu = self.menu_for_ruler();
            let (x, y) = self.ivars().start.get();
            menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint::new(x, y), Some(self));
            true
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
const STRAIGHTEN: isize = 3;
const UNITS_BASE: isize = 100;

impl RulerView {
    fn new(mtm: MainThreadMarker) -> Retained<RulerView> {
        let view = RulerView::alloc(mtm).set_ivars(RulerIvars {
            start: Cell::new((MARGIN, MARGIN)),
            length: Cell::new(DEFAULT_LENGTH),
            angle: Cell::new(0.0),
            gesture: Cell::new(None),
        });
        // SAFETY: NSView's designated initializer.
        unsafe { msg_send![super(view), initWithFrame: NSRect::ZERO] }
    }

    /// The ruler in view points, as a geometry with y flipped (so the same
    /// math applies): points are (x, -y).
    fn view_geometry(&self) -> RulerGeometry {
        let (x, y) = self.ivars().start.get();
        RulerGeometry { start: (x, -y), length: self.ivars().length.get(), angle: self.ivars().angle.get() }
    }

    /// The ruler in screen points.
    fn screen_geometry(&self) -> Option<RulerGeometry> {
        let frame = self.window()?.frame();
        let (x, y) = self.ivars().start.get();
        Some(RulerGeometry {
            start: (frame.origin.x + x, frame.origin.y + frame.size.height - y),
            length: self.ivars().length.get(),
            angle: self.ivars().angle.get(),
        })
    }

    /// Moves, turns, or resizes the ruler: its panel becomes the rectangle
    /// around it.
    fn set_screen_geometry(&self, g: RulerGeometry) {
        let (x, y, w, h) = g.bounds(MARGIN);
        self.ivars().start.set((g.start.0 - x, y + h - g.start.1));
        self.ivars().length.set(g.length);
        self.ivars().angle.set(g.angle);
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
        match self.window() {
            Some(window) => window.setFrame_display(frame, true),
            None => self.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), frame.size)),
        }
        self.setNeedsDisplay(true);
    }

    /// Turns the ruler by `by` degrees around its start, snapping to `step`.
    fn turn(&self, by: f64, step: f64) {
        if let Some(mut g) = self.screen_geometry() {
            g.angle = normalize_angle(g.angle + by, step);
            self.set_screen_geometry(g);
            // Kept on the screen, which the turned ruler may run off.
            if let Some(window) = self.window() {
                let frame = window.constrainFrameRect_toScreen(window.frame(), window.screen().as_deref());
                window.setFrame_display(frame, true);
            }
        }
    }

    /// How far along the ruler the pointer is, when it is over or near it.
    fn pointer_along(&self) -> Option<f64> {
        let window = self.window()?;
        let p = window.convertPointFromScreen(NSEvent::mouseLocation());
        let p = self.convertPoint_fromView(p, None);
        let g = self.view_geometry();
        let (u, v) = g.local((p.x, -p.y));
        ((0.0..g.length - GRIP).contains(&u) && (-400.0..400.0).contains(&v)).then_some(u)
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
        let (sx, sy) = self.ivars().start.get();
        let (length, angle) = (self.ivars().length.get(), self.ivars().angle.get());

        // Draw as if level, from the start: x along, y across (down).
        let transform = NSAffineTransform::transform();
        transform.translateXBy_yBy(sx, sy);
        // In a flipped view, turning counterclockwise on screen is negative.
        transform.rotateByDegrees(-angle);
        // SAFETY: NSAffineTransform's AppKit method for the current context.
        let _: () = unsafe { msg_send![&transform, concat] };

        let body = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(length, THICKNESS)),
            4.0,
            4.0,
        );
        NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 0.98, 0.9, 0.92).setFill();
        body.fill();
        NSColor::colorWithWhite_alpha(0.3, 0.6).setStroke();
        body.setLineWidth(1.0);
        body.stroke();

        let ink = NSColor::colorWithWhite_alpha(0.15, 1.0);
        let path = NSBezierPath::bezierPath();
        path.setLineWidth(1.0);
        let font = NSFont::monospacedDigitSystemFontOfSize_weight(9.0, 0.0);
        let attrs = text_attributes(&font, &ink);
        for tick in ticks(length - GRIP, &scale, settings.units) {
            let x = tick.at.floor() + 0.5;
            let size = match tick.size {
                TickSize::Major => 14.0,
                TickSize::Middle => 9.0,
                TickSize::Minor => 5.0,
            };
            path.moveToPoint(NSPoint::new(x, 0.0));
            path.lineToPoint(NSPoint::new(x, size));
            if let Some(label) = tick.label {
                let text = NSString::from_str(&label);
                // SAFETY: drawing a string with valid attributes.
                unsafe { text.drawAtPoint_withAttributes(NSPoint::new(x + 2.0, 15.0), Some(&attrs)) };
            }
        }
        ink.setStroke();
        path.stroke();

        // The grip at the far end.
        let grip = NSBezierPath::bezierPath();
        for i in 0..3 {
            let a = length - GRIP + 4.0 + f64::from(i) * 3.0;
            grip.moveToPoint(NSPoint::new(a, THICKNESS * 0.3));
            grip.lineToPoint(NSPoint::new(a, THICKNESS * 0.7));
        }
        NSColor::colorWithWhite_alpha(0.4, 0.8).setStroke();
        grip.stroke();

        // A turned ruler shows its angle, before the grip.
        if angle.rem_euclid(90.0).abs() > 1e-9 {
            let text = NSString::from_str(&format!("∠ {}", crate::geometry::format_angle(angle)));
            // SAFETY: measuring and drawing a string with valid attributes.
            unsafe {
                let width = text.sizeWithAttributes(Some(&attrs)).width;
                text.drawAtPoint_withAttributes(
                    NSPoint::new(length - GRIP - 4.0 - width, THICKNESS - 12.0),
                    Some(&attrs),
                );
            }
        }

        // The pointer's position along the ruler.
        if let Some(u) = self.pointer_along() {
            let red = NSColor::systemRedColor();
            let marker = NSBezierPath::bezierPath();
            marker.setLineWidth(1.0);
            marker.moveToPoint(NSPoint::new(u.floor() + 0.5, 0.0));
            marker.lineToPoint(NSPoint::new(u.floor() + 0.5, THICKNESS));
            red.setStroke();
            marker.stroke();
            let label = NSString::from_str(&position_text(u, &scale, settings.units));
            let label_attrs = text_attributes(&font, &red);
            // SAFETY: as above.
            unsafe { label.drawAtPoint_withAttributes(NSPoint::new(u + 3.0, THICKNESS - 12.0), Some(&label_attrs)) };
        }
    }

    fn menu_for_ruler(&self) -> Retained<NSMenu> {
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
        if self.ivars().angle.get().abs() > 1e-9 {
            add(&menu, n!("Straighten"), STRAIGHTEN, false);
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let units = Settings::load().units;
        for (i, u) in Units::ALL.iter().enumerate() {
            add(&menu, u.name(), UNITS_BASE + i as isize, *u == units);
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add(&menu, n!("Close Ruler"), CLOSE, false);
        menu
    }

    fn action(&self, tag: isize) {
        let Some(window) = self.window() else { return };
        match tag {
            // Clockwise, as a level ruler turns to run down the screen.
            ROTATE => self.turn(-90.0, 0.0),
            STRAIGHTEN => {
                let angle = self.ivars().angle.get();
                self.turn(-angle, 0.0);
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

/// Opens a ruler in the middle of the screen, level or running down.
pub fn new_ruler(vertical: bool, mtm: MainThreadMarker) {
    let screen = NSScreen::mainScreen(mtm);
    let visible = screen.map_or(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0)), |s| s.visibleFrame());
    let count = RULERS.with(|r| r.borrow().len()) as f64;
    let center = (
        visible.origin.x + visible.size.width / 2.0 + count * 24.0,
        visible.origin.y + visible.size.height / 2.0 - count * 24.0,
    );
    let g = if vertical {
        RulerGeometry { start: (center.0, center.1 + DEFAULT_LENGTH / 2.0), length: DEFAULT_LENGTH, angle: -90.0 }
    } else {
        RulerGeometry {
            start: (center.0 - DEFAULT_LENGTH / 2.0, center.1 + THICKNESS / 2.0),
            length: DEFAULT_LENGTH,
            angle: 0.0,
        }
    };
    open(g, mtm);
}

/// For screenshots: rulers from `spec`, such as `h:300,200,500;v:250,240,300`
/// (level or running down, then the top-left corner of the ruler in screen
/// points from the top-left of the main screen, and the length).
pub fn open_from_spec(spec: &str, mtm: MainThreadMarker) {
    let height = NSScreen::mainScreen(mtm).map_or(0.0, |s| s.frame().size.height);
    for part in spec.split(';') {
        let Some((kind, numbers)) = part.split_once(':') else { continue };
        let n: Vec<f64> = numbers.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let [x, y, length] = n[..] else { continue };
        let top = height - y;
        let g = if kind.trim() == "v" {
            RulerGeometry { start: (x + THICKNESS, top), length, angle: -90.0 }
        } else {
            RulerGeometry { start: (x, top), length, angle: 0.0 }
        };
        open(g, mtm);
    }
}

fn open(g: RulerGeometry, mtm: MainThreadMarker) {
    let (x, y, w, h) = g.bounds(MARGIN);
    // SAFETY: NSPanel's designated initializer.
    let panel: Retained<RulerPanel> = unsafe {
        msg_send![RulerPanel::alloc(mtm), initWithContentRect: NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)),
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
    let view = RulerView::new(mtm);
    view.setAutoresizingMask(
        objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable
            | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    panel.setContentView(Some(&view));
    view.set_screen_geometry(g);
    panel.orderFrontRegardless();
    RULERS.with(|r| r.borrow_mut().push(panel));
    start_timer();
}

/// For tests: a ruler drawn at `angle`, `length` points long, as an image,
/// with the image's size in points and where the ruler starts in it (in
/// view points: y grows down).
pub fn render_for_tests(
    angle: f64,
    length: f64,
    mtm: MainThreadMarker,
) -> Option<(crate::measure::Frame, NSSize, (f64, f64))> {
    let view = RulerView::new(mtm);
    view.set_screen_geometry(RulerGeometry { start: (0.0, 0.0), length, angle });
    let rect = view.bounds();
    let rep = view.bitmapImageRepForCachingDisplayInRect(rect)?;
    view.cacheDisplayInRect_toBitmapImageRep(rect, &rep);
    let image = rep.CGImage()?;
    Some((crate::capture::to_frame(&image).ok()?, rect.size, view.ivars().start.get()))
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
            .filter_map(|panel| panel.contentView()?.downcast::<RulerView>().ok()?.screen_geometry())
            .map(SavedRuler::from_geometry)
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
    for r in &saved {
        open(r.geometry(), mtm);
    }
    // Keep them on the screens there are now.
    RULERS.with(|r| {
        for panel in r.borrow().iter() {
            let frame = panel.constrainFrameRect_toScreen(panel.frame(), panel.screen().as_deref());
            panel.setFrame_display(frame, false);
        }
    });
    SAVED.with(|s| *s.borrow_mut() = snapshot());
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
/// pointer marker, and saves them once they settle.
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
        // Moved, resized, or turned: saved once the mouse is up. Also then:
        // clicks go through a panel except on the ruler itself (the empty
        // corners around a turned ruler), which macOS does not do by itself.
        if NSEvent::pressedMouseButtons() == 0 {
            save();
            let m = NSEvent::mouseLocation();
            RULERS.with(|r| {
                for panel in r.borrow().iter() {
                    let on_ruler = panel
                        .contentView()
                        .and_then(|v| v.downcast::<RulerView>().ok())
                        .and_then(|v| v.screen_geometry())
                        .is_some_and(|g| g.contains((m.x, m.y), 2.0));
                    if panel.ignoresMouseEvents() == on_ruler {
                        panel.setIgnoresMouseEvents(!on_ruler);
                    }
                }
            });
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
        assert_eq!(number(12.0), "12");
    }

    #[test]
    fn ruler_geometry() {
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
        // Level: the body hangs below the tick edge.
        let level = RulerGeometry { start: (100.0, 500.0), length: 200.0, angle: 0.0 };
        assert!(close(level.point(200.0, 34.0), (300.0, 466.0)));
        assert_eq!(level.bounds(0.0), (100.0, 466.0, 200.0, 34.0));
        assert!(close(level.local((150.0, 480.0)), (50.0, 20.0)));
        assert!(level.contains((150.0, 480.0), 0.0) && !level.contains((150.0, 510.0), 2.0));
        // Running down (clockwise 90°): the body is left of the tick edge.
        let down = RulerGeometry { start: (100.0, 500.0), length: 200.0, angle: -90.0 };
        assert!(close(down.point(200.0, 0.0), (100.0, 300.0)));
        assert!(close(down.point(0.0, 34.0), (66.0, 500.0)));
        // Turned 30°: a point 100 along is (cos 30, sin 30) * 100 away.
        let turned = RulerGeometry { start: (0.0, 0.0), length: 100.0, angle: 30.0 };
        let (x, y) = turned.point(100.0, 0.0);
        assert!((x - 86.60254).abs() < 1e-4 && (y - 50.0).abs() < 1e-9);
        assert!(close(turned.local((x, y)), (100.0, 0.0)));
        // Angles.
        assert_eq!(normalize_angle(190.0, 0.0), -170.0);
        assert_eq!(normalize_angle(-180.0, 0.0), 180.0);
        assert_eq!(normalize_angle(37.0, 15.0), 30.0);
        assert_eq!(normalize_angle(-0.0, 0.0), 0.0);
    }

    #[test]
    fn saved_rulers() {
        // Saved by 0.3.1: a level ruler and one running down.
        let old: SavedRulers = toml::from_str(
            "[[rulers]]\nvertical = false\nx = 10.0\ny = 20.0\nwidth = 500.0\nheight = 34.0\n\
             [[rulers]]\nvertical = true\nx = 10.0\ny = 20.0\nwidth = 34.0\nheight = 300.0\n",
        )
        .unwrap();
        assert_eq!(old.rulers[0].geometry(), RulerGeometry { start: (10.0, 54.0), length: 500.0, angle: 0.0 });
        assert_eq!(old.rulers[1].geometry(), RulerGeometry { start: (44.0, 320.0), length: 300.0, angle: -90.0 });
        // Both cover the same rectangle as before.
        assert_eq!(old.rulers[1].geometry().bounds(0.0), (10.0, 20.0, 34.0, 300.0));

        let g = RulerGeometry { start: (12.5, 40.0), length: 250.0, angle: 30.0 };
        let saved = SavedRulers { rulers: vec![SavedRuler::from_geometry(g)] };
        let text = toml::to_string(&saved).unwrap();
        assert!(!text.contains("vertical") && !text.contains("width"), "{text}");
        let dir = std::env::temp_dir().join(format!("pixlsnapz-rulers-{}", std::process::id()));
        let path = dir.join("rulers.toml");
        saved.save_to(&path).unwrap();
        assert_eq!(SavedRulers::load_from(&path).rulers[0].geometry(), g);
        std::fs::remove_dir_all(&dir).ok();
    }
}
