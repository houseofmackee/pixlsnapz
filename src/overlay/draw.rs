//! Drawing the measurements, guides, grid, loupe, and hints.

use super::*;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBezierPath, NSColor, NSCompositingOperation, NSFont, NSFontAttributeName, NSFontWeightMedium,
    NSFontWeightRegular, NSForegroundColorAttributeName, NSStringDrawing,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

use crate::color::contrast_text;
use crate::items::Item;
use crate::measure::{Cross, PxRect, aspect_ratio, columns, grid_lines, line_angle, line_length};
use crate::settings::{Axes, Grid};

pub(super) fn color(rgb: [f64; 3], alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(rgb[0], rgb[1], rgb[2], alpha)
}

pub(super) fn line(path: &NSBezierPath, a: (f64, f64), b: (f64, f64)) {
    path.moveToPoint(NSPoint::new(a.0, a.1));
    path.lineToPoint(NSPoint::new(b.0, b.1));
}

pub(super) fn attributes(size: f64, medium: bool) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: static constants.
    let weight = unsafe { if medium { NSFontWeightMedium } else { NSFontWeightRegular } };
    let font = NSFont::monospacedDigitSystemFontOfSize_weight(size, weight);
    let fg = NSColor::whiteColor();
    // SAFETY: static constants.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font.as_ref(), fg.as_ref()];
    NSDictionary::from_slices(&keys, &values)
}

pub(super) fn pill_size(text: &str, size: f64) -> NSSize {
    // SAFETY: the attributes have the expected types.
    let t = unsafe { NSString::from_str(text).sizeWithAttributes(Some(&attributes(size, true))) };
    NSSize::new(t.width + 14.0, t.height + 6.0)
}

/// Draws `text` in a rounded label filled with `fill`. `at` is the label's
/// top-left corner, moved as needed to stay inside `bounds`.
pub(super) fn pill(text: &str, at: NSPoint, bounds: NSSize, size: f64, fill: &NSColor) -> NSRect {
    let attrs = attributes(size, true);
    let s = NSString::from_str(text);
    let ps = pill_size(text, size);
    let x = at.x.clamp(2.0, (bounds.width - ps.width - 2.0).max(2.0));
    let y = at.y.clamp(2.0, (bounds.height - ps.height - 2.0).max(2.0));
    let r = NSRect::new(NSPoint::new(x, y), ps);
    fill.setFill();
    let shape = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(r, 4.0, 4.0);
    shape.fill();
    if high_contrast() {
        // An outline keeps labels apart from whatever is behind them.
        NSColor::whiteColor().setStroke();
        shape.setLineWidth(1.5);
        shape.stroke();
    }
    // SAFETY: the attributes have the expected types.
    unsafe { s.drawAtPoint_withAttributes(NSPoint::new(x + 7.0, y + 3.0), Some(&attrs)) };
    r
}

/// A label centered on `center`.
pub(super) fn pill_centered(text: &str, center: NSPoint, bounds: NSSize, size: f64, fill: &NSColor) {
    let ps = pill_size(text, size);
    pill(text, NSPoint::new(center.x - ps.width / 2.0, center.y - ps.height / 2.0), bounds, size, fill);
}

pub(super) fn dark() -> Retained<NSColor> {
    let alpha = if high_contrast() { 1.0 } else { 0.9 };
    NSColor::colorWithSRGBRed_green_blue_alpha(0.09, 0.09, 0.11, alpha)
}

/// Lines from edge to edge through the point, with end ticks and a label
/// for each direction.
pub(super) fn draw_cross(s: &Session, o: &Overlay, x: usize, y: usize, c: &Cross, axes: Axes, rgb: [f64; 3]) {
    let (cx, cy) = ((x as f64 + 0.5) / o.scale, (y as f64 + 0.5) / o.scale);
    let (l, r) = (o.pt(c.left), o.pt(c.right));
    let (t, b) = (o.pt(c.top), o.pt(c.bottom));
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(lw(1.0));
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
    pill(&s.cross_text(c, axes, &o.measure), NSPoint::new(cx + 10.0, cy + 10.0), o.size, 12.0, &accent);
}

/// A measured line with end ticks, a protractor arc from the horizontal at
/// its start, and its length and angle. Coordinates in pixels.
/// A measured circle: its outline, center, a radius line, and the radius
/// and diameter. Coordinates in pixels.
pub(super) fn draw_circle(s: &Session, o: &Overlay, (cx, cy, r): (f64, f64, f64), rgb: [f64; 3]) {
    let (x, y, rad) = (cx / o.scale, cy / o.scale, r / o.scale);
    let accent = color(rgb, 1.0);
    color(rgb, 0.1).setFill();
    let oval = NSBezierPath::bezierPathWithOvalInRect(NSRect::new(
        NSPoint::new(x - rad, y - rad),
        NSSize::new(2.0 * rad, 2.0 * rad),
    ));
    oval.fill();
    oval.setLineWidth(lw(1.5));
    accent.setStroke();
    oval.stroke();
    let radius = NSBezierPath::bezierPath();
    radius.setLineWidth(lw(1.0));
    line(&radius, (x, y), (x + rad, y));
    line(&radius, (x + rad, y - 4.0), (x + rad, y + 4.0));
    radius.stroke();
    accent.setFill();
    NSBezierPath::bezierPathWithOvalInRect(NSRect::new(NSPoint::new(x - 2.5, y - 2.5), NSSize::new(5.0, 5.0))).fill();
    pill_centered(&s.circle_text(r, &o.measure), NSPoint::new(x, y + 16.0), o.size, 12.0, &accent);
}

pub(super) fn draw_line(s: &Session, o: &Overlay, (x0, y0, x1, y1): (f64, f64, f64, f64), rgb: [f64; 3]) {
    let (ax, ay, bx, by) = (x0 / o.scale, y0 / o.scale, x1 / o.scale, y1 / o.scale);
    let len_pt = line_length(bx - ax, by - ay);
    let accent = color(rgb, 1.0);
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(lw(1.5));
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
        reference.setLineWidth(lw(1.0));
        let dash = [3.0f64, 3.0];
        // SAFETY: `dash` holds two values.
        unsafe { reference.setLineDash_count_phase(dash.as_ptr().cast_mut().cast(), 2, 0.0) };
        let dir = if angle.abs() > 90.0 { -1.0 } else { 1.0 };
        line(&reference, (ax, ay), (ax + dir * (radius + 10.0), ay));
        color(rgb, 0.7).setStroke();
        reference.stroke();
        let arc = NSBezierPath::bezierPath();
        arc.setLineWidth(lw(1.0));
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

    let (len, angle_text) = s.line_texts((x0, y0, x1, y1), &o.measure);
    let text = format!("{len}  ∠ {angle_text}");
    let mid = NSPoint::new((ax + bx) / 2.0, (ay + by) / 2.0);
    pill_centered(&text, NSPoint::new(mid.x, mid.y - 16.0), o.size, 12.0, &accent);
}

pub(super) fn draw_area(s: &Session, o: &Overlay, r: PxRect, rgb: [f64; 3]) {
    let rect = o.rect_pt(r);
    color(rgb, if high_contrast() { 0.22 } else { 0.12 }).setFill();
    NSBezierPath::fillRect(rect);
    let path = NSBezierPath::bezierPathWithRect(rect);
    path.setLineWidth(lw(1.0));
    color(rgb, 1.0).setStroke();
    path.stroke();

    // Width above the top edge and height beside the right edge, like
    // dimension lines on a drawing; the full size below.
    let accent = color(rgb, 1.0);
    let (x0, y0) = (rect.origin.x, rect.origin.y);
    let (x1, y1) = (x0 + rect.size.width, y0 + rect.size.height);
    if rect.size.width > 40.0 {
        pill_centered(
            &s.len_text(r.width(), &o.measure),
            NSPoint::new((x0 + x1) / 2.0, y0 - 13.0),
            o.size,
            11.0,
            &dark(),
        );
    }
    if rect.size.height > 30.0 {
        let label = s.len_text(r.height(), &o.measure);
        let w = pill_size(&label, 11.0).width;
        pill_centered(&label, NSPoint::new(x1 + 6.0 + w / 2.0, (y0 + y1) / 2.0), o.size, 11.0, &dark());
    }
    let mut text = s.size_text(r.width(), r.height(), &o.measure);
    if s.settings.show_aspect_ratio {
        text = format!("{text}  ({})", aspect_ratio(r.width(), r.height()));
    }
    let below = y1 + 16.0;
    let y = if below + 16.0 < o.size.height { below } else { y1 - 16.0 };
    pill_centered(&text, NSPoint::new((x0 + x1) / 2.0, y), o.size, 12.0, &accent);
}

/// Guides as full lines, with the distances between them (and to the
/// screen edges) along the top and left.
pub(super) fn draw_guides(s: &Session, o: &Overlay, index: usize, rgb: [f64; 3]) {
    let (mut v, mut h) = s.guides(index);
    if v.is_empty() && h.is_empty() {
        return;
    }
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(lw(1.0));
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
                    &s.len_text(pair[1] - pair[0], &o.measure),
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
                    &s.len_text(pair[1] - pair[0], &o.measure),
                    NSPoint::new(40.0, (a + b) / 2.0),
                    o.size,
                    11.0,
                    &fill,
                );
            }
        }
    }
}

/// The layout grid, in the last kept area or over the whole screen.
pub(super) fn draw_grid(s: &Session, o: &Overlay, index: usize, rgb: [f64; 3]) {
    let st = &s.settings;
    if st.grid == Grid::Off {
        return;
    }
    let area = s.grid_area(index);
    let (x0, y0) = (area.origin.x, area.origin.y);
    let (x1, y1) = (x0 + area.size.width, y0 + area.size.height);
    // Grid sizes are in design units: scale them like measurements.
    let zoom = o.measure.zoom;
    match st.grid {
        Grid::Square => {
            let path = NSBezierPath::bezierPath();
            path.setLineWidth(lw(0.5));
            for x in grid_lines(x0, x1, st.grid_size * zoom) {
                line(&path, (x, y0), (x, y1));
            }
            for y in grid_lines(y0, y1, st.grid_size * zoom) {
                line(&path, (x0, y), (x1, y));
            }
            color(rgb, 0.3).setStroke();
            path.stroke();
        }
        Grid::Columns => {
            let cols = columns(x0, area.size.width, st.grid_columns, st.grid_gutter * zoom, st.grid_margin * zoom);
            for (left, right) in cols {
                let r = NSRect::new(NSPoint::new(left, y0), NSSize::new(right - left, y1 - y0));
                color(rgb, 0.1).setFill();
                NSBezierPath::fillRect(r);
                let edge = NSBezierPath::bezierPath();
                edge.setLineWidth(lw(0.5));
                line(&edge, (left, y0), (left, y1));
                line(&edge, (right, y0), (right, y1));
                color(rgb, 0.4).setStroke();
                edge.stroke();
            }
        }
        Grid::Off => {}
    }
}

/// The contrast between the first picked color and the one under the
/// pointer, with both as swatches.
pub(super) fn draw_contrast(o: &Overlay, p: NSPoint, first: [u8; 3], here: [u8; 3]) {
    // Room on the left for the two swatches.
    let text = format!("         Contrast {}", contrast_text(first, here));
    let r = pill(&text, NSPoint::new(p.x + 12.0, p.y + 36.0), o.size, 12.0, &dark());
    for (i, c) in [first, here].iter().enumerate() {
        let sw = NSRect::new(
            NSPoint::new(r.origin.x + 7.0 + i as f64 * 14.0, r.origin.y + (r.size.height - 12.0) / 2.0),
            NSSize::new(12.0, 12.0),
        );
        NSColor::colorWithSRGBRed_green_blue_alpha(
            f64::from(c[0]) / 255.0,
            f64::from(c[1]) / 255.0,
            f64::from(c[2]) / 255.0,
            1.0,
        )
        .setFill();
        let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(sw, 2.0, 2.0);
        path.fill();
        NSColor::colorWithWhite_alpha(1.0, 0.8).setStroke();
        path.setLineWidth(lw(0.5));
        path.stroke();
    }
}

/// Shift held: lines across the whole screen through the cursor, for
/// checking alignment.
pub(super) fn draw_alignment(o: &Overlay, x: usize, y: usize, rgb: [f64; 3]) {
    let (cx, cy) = ((x as f64 + 0.5) / o.scale, (y as f64 + 0.5) / o.scale);
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(lw(1.0));
    let dash = [4.0f64, 3.0];
    // SAFETY: `dash` holds two values.
    unsafe { path.setLineDash_count_phase(dash.as_ptr().cast_mut().cast(), 2, 0.0) };
    line(&path, (0.0, cy), (o.size.width, cy));
    line(&path, (cx, 0.0), (cx, o.size.height));
    color(rgb, 0.6).setStroke();
    path.stroke();
}

pub(super) const HINTS: [&str; 4] = [
    n!("Drag: area  ·  ⌘/right-drag: no snap  ·  ⌥-drag or L: line  ·  E: circle  ·  ⇧: crosshair, square, 45°  ·  \
     Click: keep"),
    n!("Drag a handle: edit  ·  ⌥ over a box: spacing  ·  V H: guides  ·  ⌥ arrows: next edge  ·  X: directions  ·  \
     Scroll or [ ]: tolerance  ·  N smooth edges"),
    n!("U units  ·  O grid  ·  F freeze  ·  Z loupe  ·  - + zoom  ·  ⇧Z vision  ·  D ⇧D design zoom  ·  \
     I design overlay  ·  P color  ·  ⇧P format  ·  ⌥P average  ·  K contrast"),
    n!("⌘C copy  ·  T size format  ·  ⌘S ⇧⌘C screenshot  ·  ⌘E CleanShotZ  ·  M measurements in screenshots  ·  \
     B keep on screen  ·  ⌫ remove  ·  C clear  ·  R restore  ·  ? hints  ·  Esc done"),
];

/// The design image over the screen, at its opacity or as a difference.
fn draw_design_overlay(s: &Session, o: &Overlay) {
    let Some((image, w, h)) = s.overlay_image.as_ref().filter(|_| s.settings.overlay_visible) else { return };
    let st = &s.settings;
    let dest = NSRect::new(
        NSPoint::new(st.overlay_x / o.scale, st.overlay_y / o.scale),
        NSSize::new(w / o.scale, h / o.scale),
    );
    let (operation, fraction) = if st.overlay_difference {
        // The overlay window is transparent: to take the difference with
        // the screen, the screen must be drawn into it first.
        if !st.freeze {
            o.image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), o.size));
        }
        (NSCompositingOperation::Difference, 1.0)
    } else {
        (NSCompositingOperation::SourceOver, st.overlay_opacity.clamp(0.05, 1.0))
    };
    // SAFETY: plain drawing; `respectFlipped` keeps the image upright.
    unsafe {
        image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
            dest,
            NSRect::ZERO,
            operation,
            fraction,
            true,
            None,
        )
    };
}

/// Grab handles on kept item `n`, to show it can be edited.
fn draw_handles(s: &Session, o: &Overlay, n: usize, rgb: [f64; 3]) {
    let Some((_, item)) = s.items.get(n) else { return };
    let points: Vec<(f64, f64)> = match *item {
        Item::Area(r) => crate::geometry::rect_handles(r).to_vec(),
        Item::Line { x0, y0, x1, y1 } => vec![(x0, y0), (x1, y1)],
        Item::Circle { cx, cy, r } => vec![(cx, cy), (cx + r, cy)],
        Item::Cross { x, y, .. } => vec![(x as f64 + 0.5, y as f64 + 0.5)],
        Item::VerticalGuide { .. } | Item::HorizontalGuide { .. } => Vec::new(),
    };
    let size = lw(7.0);
    for (px, py) in points {
        let r =
            NSRect::new(NSPoint::new(px / o.scale - size / 2.0, py / o.scale - size / 2.0), NSSize::new(size, size));
        let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(r, 1.5, 1.5);
        NSColor::whiteColor().setFill();
        path.fill();
        path.setLineWidth(lw(1.0));
        color(rgb, 1.0).setStroke();
        path.stroke();
    }
}

/// ⌥ over a kept box: the box outlined and a dashed line with its length to
/// the nearest box, guide, or screen edge on every side.
fn draw_spacing(s: &Session, o: &Overlay, area: PxRect, sp: crate::geometry::Spacing, rgb: [f64; 3]) {
    let rect = o.rect_pt(area);
    color(rgb, if high_contrast() { 0.22 } else { 0.12 }).setFill();
    NSBezierPath::fillRect(rect);
    let outline = NSBezierPath::bezierPathWithRect(rect);
    outline.setLineWidth(lw(2.0));
    color(rgb, 1.0).setStroke();
    outline.stroke();
    let (cx, cy) = (rect.origin.x + rect.size.width / 2.0, rect.origin.y + rect.size.height / 2.0);
    let gaps = [(sp.left, true, cy), (sp.right, true, cy), (sp.top, false, cx), (sp.bottom, false, cx)];
    let accent = color(rgb, 1.0);
    for ((from, to), horizontal, across) in gaps {
        if to <= from {
            continue;
        }
        let (a, b) = (o.pt(from), o.pt(to));
        let path = NSBezierPath::bezierPath();
        path.setLineWidth(lw(1.0));
        let dash = [4.0f64, 3.0];
        // SAFETY: `dash` holds two values.
        unsafe { path.setLineDash_count_phase(dash.as_ptr().cast_mut().cast(), 2, 0.0) };
        let tick = 4.0;
        if horizontal {
            line(&path, (a, across), (b, across));
            line(&path, (a, across - tick), (a, across + tick));
            line(&path, (b, across - tick), (b, across + tick));
        } else {
            line(&path, (across, a), (across, b));
            line(&path, (across - tick, a), (across + tick, a));
            line(&path, (across - tick, b), (across + tick, b));
        }
        accent.setStroke();
        path.stroke();
        let mid = (a + b) / 2.0;
        let at = if horizontal { NSPoint::new(mid, across - 14.0) } else { NSPoint::new(across + 4.0, mid) };
        let label = s.len_text(to - from, &o.measure);
        if horizontal {
            pill_centered(&label, at, o.size, 11.0, &accent);
        } else {
            let w = pill_size(&label, 11.0).width;
            pill_centered(&label, NSPoint::new(at.x + w / 2.0, at.y), o.size, 11.0, &accent);
        }
    }
}

pub(super) fn draw(index: usize) {
    SESSION.with(|s| {
        let s = s.borrow();
        let Some(s) = s.as_ref() else { return };
        let Some(o) = s.overlays.get(index) else { return };
        let rgb = s.settings.rgb();

        // A screenshot with measurements has the screen behind them.
        if (s.settings.freeze && !s.kept) || s.exporting {
            o.image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), o.size));
        }
        // The design overlay, only while measuring (never in screenshots).
        if !s.kept && !s.exporting {
            draw_design_overlay(s, o);
        }

        draw_grid(s, o, index, rgb);
        draw_guides(s, o, index, rgb);
        // The box whose spacing shows (⌥) gets its outline from the spacing
        // view instead of its usual labels, which would collide with it.
        let spacing_box = (index == s.active && s.option && s.drag.is_none() && !s.kept && !s.exporting)
            .then(|| s.hover_px().and_then(|(x, y)| s.area_under(index, x, y)))
            .flatten()
            .map(|(n, _)| n);
        for (n, (_, item)) in s.items.iter().enumerate().filter(|(_, (i, _))| *i == index) {
            if Some(n) == spacing_box {
                continue;
            }
            match item {
                Item::Area(r) => draw_area(s, o, *r, rgb),
                Item::Cross { x, y, cross, axes } => draw_cross(s, o, *x, *y, cross, *axes, rgb),
                Item::Line { x0, y0, x1, y1 } => draw_line(s, o, (*x0, *y0, *x1, *y1), rgb),
                Item::Circle { cx, cy, r } => draw_circle(s, o, (*cx, *cy, *r), rgb),
                Item::VerticalGuide { .. } | Item::HorizontalGuide { .. } => {}
            }
        }

        if s.kept || s.exporting {
            return;
        }

        if index == s.active {
            if let Some(d) = s.drag.filter(|d| d.edit.is_some()) {
                // Editing a kept item: its handles, nothing new.
                if let Some(target) = d.edit {
                    draw_handles(s, o, target.item, rgb);
                }
            } else if let Some(d) = s.drag {
                let px = |v: f64| v * o.scale;
                match d.tool {
                    Tool::Line => draw_line(s, o, (px(d.start.x), px(d.start.y), px(d.end.x), px(d.end.y)), rgb),
                    Tool::Circle => {
                        let r = line_length(d.end.x - d.start.x, d.end.y - d.start.y);
                        draw_circle(s, o, (px(d.start.x), px(d.start.y), px(r)), rgb);
                    }
                    Tool::Area => draw_area(s, o, o.rect_px(d.start, d.end), rgb),
                }
            } else if let Some(p) = s.hover {
                let (x, y) = o.px(p);
                if s.shift {
                    draw_alignment(o, x, y, rgb);
                }
                // ⌥ over a kept box: its spacing to everything around it.
                if let Some((n, area)) = s.area_under(index, x, y).filter(|_| s.option) {
                    draw_spacing(s, o, area, s.spacing_of(index, n, area), rgb);
                } else {
                    let c = s.cross_at(index, x, y);
                    draw_cross(s, o, x, y, &c, s.settings.axes, rgb);
                }
                if let Some(n) = s.hovered_item() {
                    draw_handles(s, o, n, rgb);
                }
                if s.settings.loupe {
                    draw_loupe(s, o, x, y, p, rgb);
                }
                if let Some(first) = s.contrast {
                    draw_contrast(o, p, first, o.shot.frame.pixel(x, y));
                }
            }
        }

        if s.settings.show_hints {
            for (row, hint) in HINTS.iter().enumerate() {
                let hint = &crate::i18n::tr(hint);
                let w = pill_size(hint, 11.0).width;
                // Above the Dock, which covers the bottom of the screen.
                let y = o.safe_bottom - 8.0 - (HINTS.len() - row) as f64 * 22.0;
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
