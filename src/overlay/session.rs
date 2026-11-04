//! A measuring session's state: displays, kept items, and what the pointer
//! is doing, with the logic that does not draw.

use super::*;

use std::time::Instant;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_app_kit::NSImage;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSTimer};

use crate::capture::Shot;
use crate::capture::WindowFrame;
use crate::geometry::{
    Joint, Spacing, distance_to_segment, joints, move_rect, rect_handles, resize_rect, snap_point, spacing,
};
use crate::items::SavedSets;
use crate::items::{Display, Item, SavedItem, SavedSession};
use crate::measure::{
    Cross, Direction, Frame, PxRect, Scale, clip_cross, columns, format_angle, line_angle, line_length, next_edge,
    snap_to,
};

/// The least tolerance smooth edges use: neighboring pixels of a blurred
/// screen differ a little even inside one surface.
const SMOOTH_MIN_TOLERANCE: u8 = 8;
use crate::i18n::tr;
use crate::settings::{Axes, Settings, Units};

/// What dragging measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tool {
    Area,
    /// Length and angle (L, or ⌥-drag).
    Line,
    /// Radius and diameter from the center (E).
    Circle,
    /// ⌃-drag: a box around the elements (or windows) under the start and
    /// the end of the drag.
    Span,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Drag {
    pub(super) start: NSPoint,
    pub(super) end: NSPoint,
    pub(super) tool: Tool,
    /// Editing a kept item instead of measuring something new.
    pub(super) edit: Option<Target>,
}

/// The part of a kept item a drag takes hold of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Grip {
    /// One of a box's eight handles (see `geometry::rect_handles`).
    AreaHandle(usize),
    /// A box's outline: moves the box.
    AreaMove,
    /// A line's start (false) or end (true).
    LineEnd(bool),
    LineMove,
    CircleCenter,
    CircleRim,
    Guide,
    /// A kept distance's point.
    CrossMove,
}

/// A kept item (its index in `Session::items`) and the grip on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) item: usize,
    pub(super) grip: Grip,
}

/// How near (in points) the pointer must be to take hold of a kept item.
const GRIP_TOLERANCE: f64 = 6.0;
/// How close (in points) a new line's end snaps to a kept line's end.
const JOINT_SNAP: f64 = 8.0;

pub(super) struct Overlay {
    pub(super) window: Retained<OverlayWindow>,
    pub(super) view: Retained<OverlayView>,
    pub(super) shot: Shot,
    /// The screenshot in points, for freezing and the magnifier.
    pub(super) image: Retained<NSImage>,
    /// Pixels per point.
    pub(super) scale: f64,
    /// Conversion of this display's pixels to the chosen units.
    pub(super) measure: Scale,
    /// View size in points.
    pub(super) size: NSSize,
    /// Where the Dock (or the bottom of the screen) begins, in view points
    /// from the top.
    pub(super) safe_bottom: f64,
    /// The screenshot smoothed for smooth edges (N), made when first needed.
    pub(super) smoothed: std::cell::OnceCell<Frame>,
    /// This display's top-left corner in global Core Graphics points.
    pub(super) cg_origin: (f64, f64),
}

impl Overlay {
    /// The frame edges are found in.
    pub(super) fn edges(&self, smooth: bool) -> &Frame {
        if smooth { self.smoothed.get_or_init(|| self.shot.frame.blurred()) } else { &self.shot.frame }
    }

    pub(super) fn px(&self, p: NSPoint) -> (usize, usize) {
        let f = &self.shot.frame;
        let x = (p.x * self.scale).floor().clamp(0.0, (f.width - 1) as f64) as usize;
        let y = (p.y * self.scale).floor().clamp(0.0, (f.height - 1) as f64) as usize;
        (x, y)
    }

    pub(super) fn pt(&self, px: usize) -> f64 {
        px as f64 / self.scale
    }

    pub(super) fn rect_px(&self, a: NSPoint, b: NSPoint) -> PxRect {
        let v = |n: f64| (n * self.scale).round().max(0.0) as usize;
        self.shot.frame.clamp(PxRect {
            x0: v(a.x.min(b.x)),
            y0: v(a.y.min(b.y)),
            x1: v(a.x.max(b.x)),
            y1: v(a.y.max(b.y)),
        })
    }

    pub(super) fn rect_pt(&self, r: PxRect) -> NSRect {
        NSRect::new(NSPoint::new(self.pt(r.x0), self.pt(r.y0)), NSSize::new(self.pt(r.width()), self.pt(r.height())))
    }
}

pub(super) struct Session {
    pub(super) overlays: Vec<Overlay>,
    pub(super) settings: Settings,
    /// The overlay the cursor is on.
    pub(super) active: usize,
    /// Cursor position in the active overlay's view (points, top-left origin).
    pub(super) hover: Option<NSPoint>,
    pub(super) drag: Option<Drag>,
    /// Space is held: dragging moves the rectangle.
    pub(super) moving: bool,
    /// Shift is held: crosshair mode.
    pub(super) shift: bool,
    /// Kept items with the index of their overlay, oldest first.
    pub(super) items: Vec<(usize, Item)>,
    pub(super) toast: Option<(String, Instant)>,
    /// The line tool is on (L).
    pub(super) tool: Tool,
    /// Drawing for a screenshot: the screen and kept items only.
    pub(super) exporting: bool,
    /// Scrolling not yet turned into tolerance steps.
    pub(super) scroll: f64,
    /// Settings changed by scrolling, saved when measuring ends.
    pub(super) settings_dirty: bool,
    /// The first color picked for a contrast check (K).
    pub(super) contrast: Option<[u8; 3]>,
    /// Measuring ended but the measurements stay on screen (B); the
    /// overlay lets clicks through.
    pub(super) kept: bool,
    /// Option is held: spacing around the box under the pointer.
    pub(super) option: bool,
    /// The design overlay's image and its size in pixels.
    pub(super) overlay_image: Option<(Retained<NSImage>, f64, f64)>,
    /// Control is held: the window under the pointer.
    pub(super) control: bool,
    /// Other apps' windows when measuring started, front to back.
    pub(super) windows: Vec<WindowFrame>,
    /// The saved set last loaded with ⇧R, to load the next one.
    pub(super) current_set: Option<String>,
    /// ⇧S: a set's name is being typed (in a field on the overlay).
    pub(super) naming: bool,
    /// Started with the color shortcut: only the loupe shows, and a click
    /// (or Return or P) copies the color under the pointer and ends.
    pub(super) picking: bool,
    /// Started by holding the hold-to-measure keys: those keys do not count
    /// as ⌃ or ⌥ here, and releasing them ends measuring.
    pub(super) peek: bool,
    /// What ⌃ highlights at the pointer: an element or window frame.
    pub(super) control_frame: Option<PxRect>,
    /// ⌃-drag: the element or window under the start of the drag.
    pub(super) span_anchor: Option<PxRect>,
    /// Live mode (⇧F): edges follow the screen.
    pub(super) live: bool,
    pub(super) capturing: Option<super::live::Capturing>,
    pub(super) last_capture: Option<Instant>,
}

impl Session {
    pub(super) fn new(
        overlays: Vec<Overlay>,
        settings: Settings,
        active: usize,
        hover: Option<NSPoint>,
        items: Vec<(usize, Item)>,
    ) -> Session {
        Session {
            overlays,
            settings,
            active,
            hover,
            drag: None,
            moving: false,
            shift: false,
            items,
            toast: None,
            tool: Tool::Area,
            exporting: false,
            scroll: 0.0,
            settings_dirty: false,
            contrast: None,
            kept: false,
            option: false,
            overlay_image: None,
            control: false,
            windows: Vec::new(),
            current_set: None,
            naming: false,
            picking: false,
            peek: false,
            control_frame: None,
            span_anchor: None,
            live: false,
            capturing: None,
            last_capture: None,
        }
    }

    /// The frontmost other window under `p` (points) on overlay `index`, as
    /// a box in that display's pixels.
    /// ⌃: the element under `p` (with element snapping on and permission),
    /// or else the window.
    pub(super) fn window_at(&self, index: usize, p: NSPoint) -> Option<PxRect> {
        let o = self.overlays.get(index)?;
        let (gx, gy) = (p.x + o.cg_origin.0, p.y + o.cg_origin.1);
        let window =
            self.windows.iter().find(|w| gx >= w.x && gx < w.x + w.width && gy >= w.y && gy < w.y + w.height)?;
        let element = (self.settings.element_snapping && crate::elements::trusted(false))
            .then(|| crate::elements::element_at(window.pid, gx, gy, window))
            .flatten();
        let w = element.as_ref().unwrap_or(window);
        let px = |v: f64| (v * o.scale).round().max(0.0) as usize;
        let (x0, y0) = (w.x - o.cg_origin.0, w.y - o.cg_origin.1);
        Some(o.shot.frame.clamp(PxRect { x0: px(x0), y0: px(y0), x1: px(x0 + w.width), y1: px(y0 + w.height) }))
    }

    /// ⌃-click: keeps the window under `p` as a box.
    /// Looks up what ⌃ highlights at the pointer (an Accessibility call for
    /// elements), for drawing to use without asking again.
    pub(super) fn update_control_frame(&mut self) {
        self.control_frame = match self.hover.filter(|_| self.control && !self.picking) {
            Some(p) => self.window_at(self.active, p),
            None => None,
        };
    }

    /// ⌃-drag: the box around the element or window where the drag started
    /// and the one under `p`.
    pub(super) fn span_frame(&self, index: usize, p: NSPoint) -> Option<PxRect> {
        let a = self.span_anchor?;
        let Some(b) = self.window_at(index, p) else { return Some(a) };
        Some(PxRect { x0: a.x0.min(b.x0), y0: a.y0.min(b.y0), x1: a.x1.max(b.x1), y1: a.y1.max(b.y1) })
    }

    /// The end of a ⌃-drag: a click keeps the element or window under it,
    /// a drag the box around both ends'.
    pub(super) fn finish_span(&mut self, index: usize, d: Drag) {
        let moved = (d.end.x - d.start.x).abs().max((d.end.y - d.start.y).abs());
        if moved < CLICK_SLOP {
            self.keep_window(index, d.start);
        } else if let Some(r) = self.span_frame(index, d.end).filter(|r| r.width() > 0 && r.height() > 0) {
            self.items.push((index, Item::Area(r)));
            if let Some(text) = self.copy_text() {
                announce(&t!("Kept {text}", text));
            }
        }
        self.span_anchor = None;
        self.update_control_frame();
    }

    pub(super) fn keep_window(&mut self, index: usize, p: NSPoint) {
        if let Some(r) = self.window_at(index, p).filter(|r| r.width() > 0 && r.height() > 0) {
            self.items.push((index, Item::Area(r)));
            if let Some(text) = self.copy_text() {
                announce(&t!("Kept {text}", text));
            }
        }
    }

    /// Replaces what is on screen with the saved set `name`, or the set
    /// after the last one loaded when `name` is `None`.
    pub(super) fn load_set(&mut self, name: Option<&str>) {
        let sets = SavedSets::load_from(&SavedSets::path());
        let set = match name {
            Some(name) => sets.get(name),
            None => sets.next_after(self.current_set.as_deref()),
        };
        let Some(set) = set else {
            self.toast(match name {
                Some(name) => t!("No saved set called {name}", name),
                None => t!("No saved sets yet: ⇧S saves one"),
            });
            return;
        };
        let placed = set.session().place(&self.displays());
        let position = sets.sets.iter().position(|s| s.name == set.name).unwrap_or(0) + 1;
        self.items = placed;
        self.current_set = Some(set.name.clone());
        self.toast(t!("Set: {name} ({n} of {count})", name = &set.name, n = position, count = sets.sets.len()));
    }

    /// The kept item under `p` (points) on overlay `index`, and the grip a
    /// drag there would take: the topmost (latest) item first.
    pub(super) fn hit_test(&self, index: usize, p: NSPoint) -> Option<Target> {
        let o = self.overlays.get(index)?;
        let at = (p.x * o.scale, p.y * o.scale);
        let tol = GRIP_TOLERANCE * o.scale;
        let near = |q: (f64, f64)| line_length(q.0 - at.0, q.1 - at.1) <= tol;
        for (n, (i, item)) in self.items.iter().enumerate().rev() {
            if *i != index {
                continue;
            }
            let grip = match *item {
                Item::Area(r) => {
                    if let Some(h) = rect_handles(r).iter().position(|h| near(*h)) {
                        Some(Grip::AreaHandle(h))
                    } else {
                        let c = rect_handles(r);
                        let corners = [c[0], c[2], c[4], c[6], c[0]];
                        corners.windows(2).any(|e| distance_to_segment(at, e[0], e[1]) <= tol).then_some(Grip::AreaMove)
                    }
                }
                Item::Line { x0, y0, x1, y1 } => {
                    if near((x1, y1)) {
                        Some(Grip::LineEnd(true))
                    } else if near((x0, y0)) {
                        Some(Grip::LineEnd(false))
                    } else {
                        (distance_to_segment(at, (x0, y0), (x1, y1)) <= tol).then_some(Grip::LineMove)
                    }
                }
                Item::Circle { cx, cy, r } => {
                    let d = line_length(at.0 - cx, at.1 - cy);
                    if d <= tol { Some(Grip::CircleCenter) } else { ((d - r).abs() <= tol).then_some(Grip::CircleRim) }
                }
                Item::VerticalGuide { x } => ((x as f64 - at.0).abs() <= tol).then_some(Grip::Guide),
                Item::HorizontalGuide { y } => ((y as f64 - at.1).abs() <= tol).then_some(Grip::Guide),
                Item::Cross { x, y, .. } => near((x as f64 + 0.5, y as f64 + 0.5)).then_some(Grip::CrossMove),
            };
            if let Some(grip) = grip {
                return Some(Target { item: n, grip });
            }
        }
        None
    }

    /// The kept item whose handles to show: the one under the pointer, or
    /// the box the pointer is inside.
    pub(super) fn hovered_item(&self) -> Option<usize> {
        let p = self.hover?;
        if let Some(t) = self.hit_test(self.active, p) {
            return Some(t.item);
        }
        let (x, y) = self.hover_px()?;
        self.area_under(self.active, x, y).map(|(n, _)| n)
    }

    /// The latest kept box on overlay `index` that contains pixel (x, y).
    pub(super) fn area_under(&self, index: usize, x: usize, y: usize) -> Option<(usize, PxRect)> {
        self.items.iter().enumerate().rev().find_map(|(n, (i, item))| match item {
            Item::Area(r) if *i == index && (r.x0..r.x1).contains(&x) && (r.y0..r.y1).contains(&y) => Some((n, *r)),
            _ => None,
        })
    }

    /// The spacing around kept box `n` to its neighbors, guides, and the
    /// screen edges.
    pub(super) fn spacing_of(&self, index: usize, n: usize, area: PxRect) -> Spacing {
        let o = &self.overlays[index];
        let others: Vec<PxRect> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(m, (i, item))| match item {
                Item::Area(r) if *i == index && m != n => Some(*r),
                _ => None,
            })
            .collect();
        let (v, h) = self.guides(index);
        spacing(area, &others, &v, &h, o.shot.frame.width, o.shot.frame.height)
    }

    /// Applies a drag on a kept item from `from` to `to` (points).
    pub(super) fn apply_edit(&mut self, target: Target, from: NSPoint, to: NSPoint) {
        let Some(&(index, item)) = self.items.get(target.item) else { return };
        let o = &self.overlays[index];
        let (w, h) = (o.shot.frame.width, o.shot.frame.height);
        let s = o.scale;
        let (tx, ty) = ((to.x * s).clamp(0.0, w as f64), (to.y * s).clamp(0.0, h as f64));
        let (dx, dy) = ((to.x - from.x) * s, (to.y - from.y) * s);
        let edited = match (item, target.grip) {
            (Item::Area(r), Grip::AreaHandle(k)) => Item::Area(resize_rect(r, k, tx, ty, w, h)),
            (Item::Area(r), Grip::AreaMove) => Item::Area(move_rect(r, dx, dy, w, h)),
            (Item::Line { x0, y0, .. }, Grip::LineEnd(true)) => Item::Line { x0, y0, x1: tx, y1: ty },
            (Item::Line { x1, y1, .. }, Grip::LineEnd(false)) => Item::Line { x0: tx, y0: ty, x1, y1 },
            (Item::Line { x0, y0, x1, y1 }, Grip::LineMove) => {
                Item::Line { x0: x0 + dx, y0: y0 + dy, x1: x1 + dx, y1: y1 + dy }
            }
            (Item::Circle { cx, cy, r }, Grip::CircleCenter) => Item::Circle { cx: cx + dx, cy: cy + dy, r },
            (Item::Circle { cx, cy, .. }, Grip::CircleRim) => Item::Circle { cx, cy, r: line_length(tx - cx, ty - cy) },
            (Item::VerticalGuide { .. }, Grip::Guide) => Item::VerticalGuide { x: tx.round() as usize },
            (Item::HorizontalGuide { .. }, Grip::Guide) => Item::HorizontalGuide { y: ty.round() as usize },
            (Item::Cross { axes, .. }, Grip::CrossMove) => {
                let (x, y) = (tx.min(w as f64 - 1.0) as usize, ty.min(h as f64 - 1.0) as usize);
                // Measured without the item itself, which may stop it.
                let kept = self.items.remove(target.item);
                let cross = self.cross_at(index, x, y);
                self.items.insert(target.item, kept);
                Item::Cross { x, y, cross, axes }
            }
            (other, _) => other,
        };
        self.items[target.item].1 = edited;
    }

    /// Removes the kept item under the pointer, or else the latest one.
    pub(super) fn remove_item(&mut self) {
        match self.hovered_item() {
            Some(n) => {
                self.items.remove(n);
            }
            None => {
                self.items.pop();
            }
        }
    }

    /// What ⌘C copies: like `copy_text`, with sizes in the chosen size
    /// format.
    /// The kept lines on display `index`, in pixels, with their item numbers.
    fn lines_on(&self, index: usize) -> Vec<(usize, (f64, f64, f64, f64))> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, (i, _))| *i == index)
            .filter_map(|(n, (_, item))| match *item {
                Item::Line { x0, y0, x1, y1 } => Some((n, (x0, y0, x1, y1))),
                _ => None,
            })
            .collect()
    }

    /// The angles between lines that meet on display `index` (protractor),
    /// counting `extra` (a line being drawn, in pixels) last. The joints'
    /// `first` and `second` are item numbers (`usize::MAX` for `extra`).
    pub(super) fn joints_on(&self, index: usize, extra: Option<(f64, f64, f64, f64)>) -> Vec<Joint> {
        let mut lines = self.lines_on(index);
        lines.extend(extra.map(|l| (usize::MAX, l)));
        let plain: Vec<_> = lines.iter().map(|(_, l)| *l).collect();
        joints(&plain, 1.0)
            .into_iter()
            .map(|j| Joint { first: lines[j.first].0, second: lines[j.second].0, ..j })
            .collect()
    }

    /// `p` moved onto the end of a kept line nearby, so a new line starting
    /// or ending there measures the angle between the two.
    pub(super) fn snap_to_line_end(&self, index: usize, p: NSPoint) -> NSPoint {
        let Some(o) = self.overlays.get(index) else { return p };
        let ends: Vec<(f64, f64)> = self
            .lines_on(index)
            .into_iter()
            .flat_map(|(_, l)| [(l.0 / o.scale, l.1 / o.scale), (l.2 / o.scale, l.3 / o.scale)])
            .collect();
        let (x, y) = snap_point((p.x, p.y), &ends, JOINT_SNAP);
        NSPoint::new(x, y)
    }

    /// When the latest item is a line meeting another: the angle between
    /// them, which ⌘C copies.
    fn last_joint_angle(&self) -> Option<String> {
        let n = self.items.len().checked_sub(1)?;
        let (index, Item::Line { .. }) = self.items[n] else { return None };
        let joint = self.joints_on(index, None).into_iter().rev().find(|j| j.first == n || j.second == n)?;
        Some(crate::geometry::format_angle(joint.angle()))
    }

    pub(super) fn clipboard_text(&self) -> Option<String> {
        if let Some(angle) = self.last_joint_angle() {
            return Some(angle);
        }
        let format = self.settings.size_format;
        let units = self.settings.units;
        let size = |w: f64, h: f64, scale: &Scale| {
            format.format(&scale.format(w, units, false), &scale.format(h, units, false), units)
        };
        let last = self.items.iter().rev().find_map(|(i, item)| {
            let scale = &self.overlays[*i].measure;
            match item {
                Item::Area(r) => Some(size(r.width() as f64, r.height() as f64, scale)),
                Item::Cross { cross, axes: Axes::Both, .. } => {
                    Some(size(cross.width() as f64, cross.height() as f64, scale))
                }
                Item::Circle { r, .. } => Some(size(2.0 * r, 2.0 * r, scale)),
                Item::Cross { .. } | Item::Line { .. } => self.text_of(item, scale),
                _ => None,
            }
        });
        if last.is_some() {
            return last;
        }
        let (x, y) = self.hover_px()?;
        let c = self.cross_at(self.active, x, y);
        let scale = &self.overlays[self.active].measure;
        Some(match self.settings.axes {
            Axes::Both => size(c.width() as f64, c.height() as f64, scale),
            axes => self.cross_text(&c, axes, scale),
        })
    }

    /// An item's measurement in plain text, as shown on screen.
    pub(super) fn text_of(&self, item: &Item, scale: &Scale) -> Option<String> {
        match item {
            Item::Area(r) => Some(self.size_text(r.width(), r.height(), scale)),
            Item::Cross { cross, axes, .. } => Some(self.cross_text(cross, *axes, scale)),
            Item::Line { x0, y0, x1, y1 } => {
                let (len, angle) = self.line_texts((*x0, *y0, *x1, *y1), scale);
                Some(format!("{len}, {angle}"))
            }
            Item::Circle { r, .. } => Some(self.circle_text(*r, scale)),
            _ => None,
        }
    }

    pub(super) fn in_pixels(&self) -> bool {
        self.settings.units == Units::Pixels
    }

    pub(super) fn len_text(&self, px: usize, scale: &Scale) -> String {
        self.len_text_f64(px as f64, scale)
    }

    /// A length given in (possibly fractional) pixels.
    pub(super) fn len_text_f64(&self, px: f64, scale: &Scale) -> String {
        scale.format(px, self.settings.units, self.settings.show_units)
    }

    /// What a cross measures in the given directions.
    pub(super) fn cross_text(&self, c: &Cross, axes: Axes, scale: &Scale) -> String {
        match axes {
            Axes::Both => self.size_text(c.width(), c.height(), scale),
            Axes::Horizontal => self.len_text(c.width(), scale),
            Axes::Vertical => self.len_text(c.height(), scale),
        }
    }

    /// Length and angle of a line in pixels: ("120.5", "32.5°").
    pub(super) fn line_texts(&self, (x0, y0, x1, y1): (f64, f64, f64, f64), scale: &Scale) -> (String, String) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        (self.len_text_f64(line_length(dx, dy), scale), format_angle(line_angle(dx, dy)))
    }

    /// "r 50 · ⌀ 100" for a circle of radius `r` pixels.
    pub(super) fn circle_text(&self, r: f64, scale: &Scale) -> String {
        format!("r {}  ·  ⌀ {}", self.len_text_f64(r, scale), self.len_text_f64(2.0 * r, scale))
    }

    pub(super) fn scrolled(&mut self, steps: f64) {
        self.scroll += steps;
        let whole = self.scroll.trunc();
        if whole == 0.0 {
            return;
        }
        self.scroll -= whole;
        self.settings.adjust_tolerance(whole as i32 * TOLERANCE_PER_SCROLL);
        self.settings_dirty = true;
        self.toast(t!("Edge tolerance: {name}", name = tr(&self.settings.tolerance_name())));
    }

    pub(super) fn size_text(&self, w: usize, h: usize, scale: &Scale) -> String {
        let units = self.settings.units;
        let mut text = format!("{} × {}", scale.format(w as f64, units, false), scale.format(h as f64, units, false));
        if self.settings.show_units {
            text.push(' ');
            text.push_str(units.suffix());
        }
        text
    }

    pub(super) fn guides(&self, index: usize) -> (Vec<usize>, Vec<usize>) {
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

    pub(super) fn areas(&self, index: usize) -> Vec<PxRect> {
        self.items
            .iter()
            .filter_map(|(i, item)| match item {
                Item::Area(r) if *i == index => Some(*r),
                _ => None,
            })
            .collect()
    }

    /// The cross through pixel `(x, y)`, stopped at guides and kept areas.
    pub(super) fn cross_at(&self, index: usize, x: usize, y: usize) -> Cross {
        let (v, h) = self.guides(index);
        clip_cross(self.raw_cross(index, x, y), x, y, &v, &h, &self.areas(index))
    }

    /// The cross through `(x, y)` as the screen alone has it.
    pub(super) fn raw_cross(&self, index: usize, x: usize, y: usize) -> Cross {
        let o = &self.overlays[index];
        let tolerance = self.settings.tolerance;
        if self.settings.smooth_edges {
            o.edges(true).cross_local(x, y, tolerance.max(SMOOTH_MIN_TOLERANCE))
        } else {
            o.shot.frame.cross(x, y, tolerance)
        }
    }

    pub(super) fn hover_px(&self) -> Option<(usize, usize)> {
        Some(self.overlays.get(self.active)?.px(self.hover?))
    }

    pub(super) fn finish_drag(&mut self, index: usize, d: Drag, no_snap: bool) {
        let Some(o) = self.overlays.get(index) else { return };
        let (start, end) = (d.start, d.end);
        let moved = (end.x - start.x).abs().max((end.y - start.y).abs());
        let item = if moved < CLICK_SLOP {
            let (x, y) = o.px(end);
            Item::Cross { x, y, cross: self.cross_at(index, x, y), axes: self.settings.axes }
        } else if d.tool == Tool::Line {
            Item::Line { x0: start.x * o.scale, y0: start.y * o.scale, x1: end.x * o.scale, y1: end.y * o.scale }
        } else if d.tool == Tool::Circle {
            let r = line_length(end.x - start.x, end.y - start.y) * o.scale;
            Item::Circle { cx: start.x * o.scale, cy: start.y * o.scale, r }
        } else {
            let r = o.rect_px(start, end);
            let r = if self.settings.snap_areas && !no_snap {
                o.edges(self.settings.smooth_edges).snap(r, self.settings.tolerance)
            } else {
                r
            };
            if r.width() == 0 || r.height() == 0 {
                return;
            }
            Item::Area(r)
        };
        self.items.push((index, item));
        if let Some(text) = self.copy_text() {
            announce(&t!("Kept {text}", text));
        }
    }

    pub(super) fn add_guide(&mut self, vertical: bool) {
        let Some((x, y)) = self.hover_px() else { return };
        let o = &self.overlays[self.active];
        let c = self.raw_cross(self.active, x, y);
        let max = (GUIDE_SNAP * o.scale) as usize;
        let item = if vertical {
            Item::VerticalGuide { x: snap_to(x, &[c.left, c.right], max) }
        } else {
            Item::HorizontalGuide { y: snap_to(y, &[c.top, c.bottom], max) }
        };
        self.items.push((self.active, item));
    }

    /// The text to copy: the last kept measurement, or the one under the cursor.
    pub(super) fn copy_text(&self) -> Option<String> {
        if let Some(angle) = self.last_joint_angle() {
            return Some(angle);
        }
        let last = self.items.iter().rev().find_map(|(i, item)| self.text_of(item, &self.overlays[*i].measure));
        if last.is_some() {
            return last;
        }
        let (x, y) = self.hover_px()?;
        let c = self.cross_at(self.active, x, y);
        Some(self.cross_text(&c, self.settings.axes, &self.overlays[self.active].measure))
    }

    /// The area the grid is drawn in: the last kept area on `index`, or the
    /// whole display.
    pub(super) fn grid_area(&self, index: usize) -> NSRect {
        let o = &self.overlays[index];
        self.items
            .iter()
            .rev()
            .find_map(|(i, item)| match item {
                Item::Area(r) if *i == index => Some(o.rect_pt(*r)),
                _ => None,
            })
            .unwrap_or(NSRect::new(NSPoint::new(0.0, 0.0), o.size))
    }

    /// Whether the column grid fits in the active display's grid area.
    pub(super) fn columns_fit(&self) -> bool {
        let o = &self.overlays[self.active];
        let st = &self.settings;
        let zoom = o.measure.zoom;
        let area = self.grid_area(self.active);
        !columns(area.origin.x, area.size.width, st.grid_columns, st.grid_gutter * zoom, st.grid_margin * zoom)
            .is_empty()
    }

    /// The last kept area, if any.
    pub(super) fn last_area(&self) -> Option<(usize, PxRect)> {
        self.items.iter().rev().find_map(|(i, item)| match item {
            Item::Area(r) => Some((*i, *r)),
            _ => None,
        })
    }

    pub(super) fn displays(&self) -> Vec<Display> {
        self.overlays
            .iter()
            .map(|o| Display { id: o.shot.display_id, width: o.shot.frame.width, height: o.shot.frame.height })
            .collect()
    }

    /// Option-arrow: jumps the pointer just past the next edge.
    pub(super) fn jump(&mut self, dir: Direction) {
        let Some((x, y)) = self.hover_px() else { return };
        let o = &self.overlays[self.active];
        let c = self.cross_at(self.active, x, y);
        let (nx, ny) = next_edge(c, x, y, dir, o.shot.frame.width, o.shot.frame.height);
        self.hover = Some(NSPoint::new((nx as f64 + 0.5) / o.scale, (ny as f64 + 0.5) / o.scale));
    }

    /// The last kept area, or the whole display under the cursor.
    pub(super) fn screenshot_area(&self) -> Option<(usize, PxRect)> {
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

    pub(super) fn toast(&mut self, text: impl Into<String>) {
        let text = text.into();
        announce(&text);
        self.toast = Some((text, Instant::now()));
        let block = RcBlock::new(|_timer: std::ptr::NonNull<NSTimer>| redraw_all());
        // SAFETY: a one-shot timer on the main run loop; the block is copied.
        let _ = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(TOAST_TIME.as_secs_f64() + 0.05, false, &block)
        };
    }

    pub(super) fn save_settings(&self) {
        if let Err(e) = self.settings.save() {
            tracing::warn!("saving settings: {e:#}");
        }
    }

    pub(super) fn saved(&self) -> SavedSession {
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
    pub(super) fn restore(&mut self) -> usize {
        let placed = SavedSession::load_from(&SavedSession::path()).place(&self.displays());
        let n = placed.len();
        self.items.extend(placed);
        n
    }
}
