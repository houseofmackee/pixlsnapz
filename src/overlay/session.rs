//! A measuring session's state: displays, kept items, and what the pointer
//! is doing, with the logic that does not draw.

use super::*;

use std::time::Instant;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_app_kit::NSImage;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSTimer};

use crate::capture::Shot;
use crate::items::{Display, Item, SavedItem, SavedSession};
use crate::measure::{
    Cross, Direction, Frame, PxRect, Scale, clip_cross, columns, format_angle, line_angle, line_length, next_edge,
    snap_to,
};

/// The least tolerance smooth edges use: neighboring pixels of a blurred
/// screen differ a little even inside one surface.
const SMOOTH_MIN_TOLERANCE: u8 = 8;
use crate::settings::{Axes, Settings, Units};

/// What dragging measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tool {
    Area,
    /// Length and angle (L, or ⌥-drag).
    Line,
    /// Radius and diameter from the center (E).
    Circle,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Drag {
    pub(super) start: NSPoint,
    pub(super) end: NSPoint,
    pub(super) tool: Tool,
}

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
}

impl Session {
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
        self.toast(format!("Edge tolerance: {}", self.settings.tolerance_name()));
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
        let last = self.items.iter().rev().find_map(|(i, item)| {
            let scale = &self.overlays[*i].measure;
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
        });
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
        self.toast = Some((text.into(), Instant::now()));
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
