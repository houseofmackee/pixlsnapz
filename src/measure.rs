//! Edge detection on a screenshot, independent of AppKit so it can be tested.
//!
//! All coordinates here are in pixels with a top-left origin, matching the
//! rows of the captured image.

use serde::{Deserialize, Serialize};

/// An RGBA8 screenshot of one display.
#[derive(Clone)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    /// Row-major RGBA, 4 bytes per pixel, no row padding.
    pub pixels: Vec<u8>,
}

/// A rectangle in pixels: `x0..x1` by `y0..y1` (end exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PxRect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl PxRect {
    pub fn width(&self) -> usize {
        self.x1 - self.x0
    }

    pub fn height(&self) -> usize {
        self.y1 - self.y0
    }
}

/// The horizontal and vertical extent of the uniform region around a point.
/// `left..right` and `top..bottom` are end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cross {
    pub left: usize,
    pub right: usize,
    pub top: usize,
    pub bottom: usize,
}

impl Cross {
    pub fn width(&self) -> usize {
        self.right - self.left
    }

    pub fn height(&self) -> usize {
        self.bottom - self.top
    }
}

impl Frame {
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Frame {
        assert_eq!(pixels.len(), width * height * 4, "pixel buffer does not match the frame size");
        Frame { width, height, pixels }
    }

    /// A frame filled with one color, for tests and as a fallback.
    pub fn solid(width: usize, height: usize, rgb: [u8; 3]) -> Frame {
        let pixels = std::iter::repeat_n([rgb[0], rgb[1], rgb[2], 255], width * height).flatten().collect();
        Frame { width, height, pixels }
    }

    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.width + x) * 4;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2]]
    }

    pub fn fill(&mut self, r: PxRect, rgb: [u8; 3]) {
        for y in r.y0..r.y1.min(self.height) {
            for x in r.x0..r.x1.min(self.width) {
                let i = (y * self.width + x) * 4;
                self.pixels[i..i + 3].copy_from_slice(&rgb);
            }
        }
    }

    fn differs(&self, x: usize, y: usize, from: [u8; 3], tolerance: u8) -> bool {
        distance(self.pixel(x, y), from) > tolerance
    }

    /// Scans from `(x, y)` in all four directions while the color stays
    /// within `tolerance` of the color under the point.
    pub fn cross(&self, x: usize, y: usize, tolerance: u8) -> Cross {
        let x = x.min(self.width - 1);
        let y = y.min(self.height - 1);
        let start = self.pixel(x, y);
        let left = (0..x).rev().find(|&i| self.differs(i, y, start, tolerance)).map_or(0, |i| i + 1);
        let right = (x + 1..self.width).find(|&i| self.differs(i, y, start, tolerance)).unwrap_or(self.width);
        let top = (0..y).rev().find(|&j| self.differs(x, j, start, tolerance)).map_or(0, |j| j + 1);
        let bottom = (y + 1..self.height).find(|&j| self.differs(x, j, start, tolerance)).unwrap_or(self.height);
        Cross { left, right, top, bottom }
    }

    /// Shrinks `r` to the bounding box of whatever stands out from its
    /// background (the most common color on its border). Returns `r`
    /// unchanged when nothing does.
    pub fn snap(&self, r: PxRect, tolerance: u8) -> PxRect {
        let r = self.clamp(r);
        if r.width() < 2 || r.height() < 2 {
            return r;
        }
        let background = self.border_color(r);
        let row_has = |y: usize| (r.x0..r.x1).any(|x| self.differs(x, y, background, tolerance));
        let col_has = |x: usize, y0: usize, y1: usize| (y0..y1).any(|y| self.differs(x, y, background, tolerance));
        let Some(y0) = (r.y0..r.y1).find(|&y| row_has(y)) else {
            return r;
        };
        let y1 = (y0..r.y1).rev().find(|&y| row_has(y)).map_or(y0 + 1, |y| y + 1);
        let x0 = (r.x0..r.x1).find(|&x| col_has(x, y0, y1)).unwrap_or(r.x0);
        let x1 = (x0..r.x1).rev().find(|&x| col_has(x, y0, y1)).map_or(x0 + 1, |x| x + 1);
        PxRect { x0, y0, x1, y1 }
    }

    pub fn clamp(&self, r: PxRect) -> PxRect {
        let x0 = r.x0.min(self.width);
        let y0 = r.y0.min(self.height);
        PxRect { x0, y0, x1: r.x1.clamp(x0, self.width), y1: r.y1.clamp(y0, self.height) }
    }

    /// The most common color along the edge of `r`.
    fn border_color(&self, r: PxRect) -> [u8; 3] {
        let mut counts: std::collections::HashMap<[u8; 3], usize> = std::collections::HashMap::new();
        let mut add = |x: usize, y: usize| *counts.entry(self.pixel(x, y)).or_default() += 1;
        for x in r.x0..r.x1 {
            add(x, r.y0);
            add(x, r.y1 - 1);
        }
        for y in r.y0..r.y1 {
            add(r.x0, y);
            add(r.x1 - 1, y);
        }
        // Ties go to the smallest color so the result does not depend on hashing.
        counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(c, _)| c).unwrap_or([0; 3])
    }
}

/// The largest difference between two colors' channels.
pub fn distance(a: [u8; 3], b: [u8; 3]) -> u8 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(y)).max().unwrap_or(0)
}

/// Formats a length given in pixels for display, in points (divided by
/// `scale`, with one decimal when needed) or in pixels.
pub fn format_length(px: usize, scale: f64, in_pixels: bool) -> String {
    if in_pixels || scale <= 0.0 {
        return px.to_string();
    }
    let pt = px as f64 / scale;
    let rounded = (pt * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 { format!("{}", rounded as i64) } else { format!("{rounded:.1}") }
}

/// "W × H" for a size in pixels.
pub fn format_size(w: usize, h: usize, scale: f64, in_pixels: bool) -> String {
    format!("{} × {}", format_length(w, scale, in_pixels), format_length(h, scale, in_pixels))
}

/// Stops the lines of `c` (through pixel `(x, y)`) at guides and at the
/// edges of kept areas, so the cross measures the gap to them.
/// `vertical` and `horizontal` are guide positions in pixels.
pub fn clip_cross(
    mut c: Cross,
    x: usize,
    y: usize,
    vertical: &[usize],
    horizontal: &[usize],
    areas: &[PxRect],
) -> Cross {
    let mut stop_x = |e: usize| {
        if e <= x && e > c.left {
            c.left = e;
        }
        if e > x && e < c.right {
            c.right = e;
        }
    };
    for &g in vertical {
        stop_x(g);
    }
    for a in areas.iter().filter(|a| (a.y0..a.y1).contains(&y)) {
        stop_x(a.x0);
        stop_x(a.x1);
    }
    let mut stop_y = |e: usize| {
        if e <= y && e > c.top {
            c.top = e;
        }
        if e > y && e < c.bottom {
            c.bottom = e;
        }
    };
    for &g in horizontal {
        stop_y(g);
    }
    for a in areas.iter().filter(|a| (a.x0..a.x1).contains(&x)) {
        stop_y(a.y0);
        stop_y(a.y1);
    }
    c
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// The pixel to jump to from `(x, y)` in `dir`: just past the edge of the
/// region `c` (the cross through that pixel), so repeated jumps move from
/// object to object. Stays inside a `width` by `height` frame.
pub fn next_edge(c: Cross, x: usize, y: usize, dir: Direction, width: usize, height: usize) -> (usize, usize) {
    match dir {
        Direction::Left => (c.left.saturating_sub(1), y),
        Direction::Right => (c.right.min(width - 1), y),
        Direction::Up => (x, c.top.saturating_sub(1)),
        Direction::Down => (x, c.bottom.min(height - 1)),
    }
}

/// The length of a line `dx` by `dy`.
pub fn line_length(dx: f64, dy: f64) -> f64 {
    dx.hypot(dy)
}

/// The angle of a line in degrees, counterclockwise from pointing right, in
/// `-180..=180`. `dy` grows downwards, as on screen.
pub fn line_angle(dx: f64, dy: f64) -> f64 {
    let a = (-dy).atan2(dx).to_degrees();
    if a <= -180.0 { a + 360.0 } else { a }
}

/// `(dx, dy)` turned to the nearest multiple of 45 degrees, keeping its length.
pub fn snap_45(dx: f64, dy: f64) -> (f64, f64) {
    let len = line_length(dx, dy);
    if len == 0.0 {
        return (0.0, 0.0);
    }
    let a = (line_angle(dx, dy) / 45.0).round() * 45.0;
    let r = a.to_radians();
    // Exact zeros for horizontal and vertical lines.
    let clean = |v: f64| if v.abs() < 1e-9 { 0.0 } else { v };
    (clean(len * r.cos()), clean(-len * r.sin()))
}

/// An angle for display: one decimal, without "-0".
pub fn format_angle(a: f64) -> String {
    let rounded = (a * 10.0).round() / 10.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    if rounded.fract() == 0.0 { format!("{}°", rounded as i64) } else { format!("{rounded:.1}°") }
}

/// The nearest of `candidates` within `max` of `at`, or `at`.
pub fn snap_to(at: usize, candidates: &[usize], max: usize) -> usize {
    candidates.iter().copied().filter(|c| c.abs_diff(at) <= max).min_by_key(|c| c.abs_diff(at)).unwrap_or(at)
}

/// The aspect ratio of a size, as small whole numbers ("16:9") when it is
/// exact, or as a decimal ratio ("1.78:1").
pub fn aspect_ratio(w: usize, h: usize) -> String {
    if w == 0 || h == 0 {
        return String::new();
    }
    fn gcd(a: usize, b: usize) -> usize {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let g = gcd(w, h);
    let (a, b) = (w / g, h / g);
    if a <= 32 && b <= 32 { format!("{a}:{b}") } else { format!("{:.2}:1", w as f64 / h as f64) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 3] = [255, 255, 255];
    const BLUE: [u8; 3] = [0, 90, 200];

    fn rect(x0: usize, y0: usize, x1: usize, y1: usize) -> PxRect {
        PxRect { x0, y0, x1, y1 }
    }

    /// A white 100 × 80 frame with a blue box at x 20..50, y 10..30.
    fn sample() -> Frame {
        let mut f = Frame::solid(100, 80, WHITE);
        f.fill(rect(20, 10, 50, 30), BLUE);
        f
    }

    #[test]
    fn cross_inside_a_box_measures_the_box() {
        let c = sample().cross(30, 20, 0);
        assert_eq!(c, Cross { left: 20, right: 50, top: 10, bottom: 30 });
        assert_eq!((c.width(), c.height()), (30, 20));
    }

    #[test]
    fn cross_outside_measures_the_gap_to_the_box() {
        // Left of the box on its rows: from the frame edge to the box.
        let c = sample().cross(5, 20, 0);
        assert_eq!((c.left, c.right), (0, 20));
        assert_eq!((c.top, c.bottom), (0, 80));
        // Below the box: from the box to the bottom edge.
        let c = sample().cross(30, 60, 0);
        assert_eq!((c.top, c.bottom), (30, 80));
    }

    #[test]
    fn cross_at_the_frame_corner() {
        let c = sample().cross(0, 0, 0);
        assert_eq!((c.left, c.top), (0, 0));
        assert_eq!(c.right, 100);
        assert_eq!(c.bottom, 80);
    }

    #[test]
    fn cross_clamps_points_outside_the_frame() {
        let c = sample().cross(500, 500, 0);
        assert_eq!((c.right, c.bottom), (100, 80));
    }

    #[test]
    fn tolerance_ignores_small_differences() {
        let mut f = Frame::solid(50, 10, WHITE);
        f.fill(rect(30, 0, 50, 10), [250, 250, 250]);
        assert_eq!(f.cross(5, 5, 0).right, 30);
        assert_eq!(f.cross(5, 5, 8).right, 50);
    }

    #[test]
    fn snap_shrinks_to_the_content() {
        let f = sample();
        assert_eq!(f.snap(rect(5, 2, 70, 60), 0), rect(20, 10, 50, 30));
    }

    #[test]
    fn snap_finds_several_objects() {
        let mut f = sample();
        f.fill(rect(60, 40, 65, 70), [200, 0, 0]);
        assert_eq!(f.snap(rect(0, 0, 100, 80), 0), rect(20, 10, 65, 70));
    }

    #[test]
    fn snap_inside_a_box_keeps_an_empty_selection() {
        let f = sample();
        assert_eq!(f.snap(rect(25, 12, 40, 25), 0), rect(25, 12, 40, 25));
    }

    #[test]
    fn snap_clamps_to_the_frame() {
        let f = sample();
        assert_eq!(f.snap(rect(10, 5, 400, 300), 0), rect(20, 10, 50, 30));
    }

    #[test]
    fn snap_of_a_tiny_rect_is_unchanged() {
        assert_eq!(sample().snap(rect(3, 3, 4, 9), 0), rect(3, 3, 4, 9));
    }

    #[test]
    fn formats_lengths() {
        assert_eq!(format_length(30, 2.0, false), "15");
        assert_eq!(format_length(31, 2.0, false), "15.5");
        assert_eq!(format_length(31, 2.0, true), "31");
        assert_eq!(format_length(10, 3.0, false), "3.3");
        assert_eq!(format_size(30, 20, 1.0, false), "30 × 20");
    }

    #[test]
    fn color_distance() {
        assert_eq!(distance([0, 0, 0], [0, 0, 0]), 0);
        assert_eq!(distance([10, 200, 0], [12, 190, 3]), 10);
    }
    #[test]
    fn clip_cross_stops_at_guides_and_areas() {
        let c = Cross { left: 0, right: 100, top: 0, bottom: 80 };
        let c2 = clip_cross(c, 50, 40, &[10, 70, 90], &[60], &[]);
        assert_eq!(c2, Cross { left: 10, right: 70, top: 0, bottom: 60 });
        // An area to the right on the cursor's row, and one above in its column.
        let areas = [PxRect { x0: 60, y0: 30, x1: 80, y1: 50 }, PxRect { x0: 40, y0: 5, x1: 55, y1: 20 }];
        let c3 = clip_cross(c, 50, 40, &[], &[], &areas);
        assert_eq!(c3, Cross { left: 0, right: 60, top: 20, bottom: 80 });
    }

    #[test]
    fn snaps_to_nearby_candidates() {
        assert_eq!(snap_to(50, &[44, 53, 90], 5), 53);
        assert_eq!(snap_to(50, &[40, 60], 5), 50);
    }

    #[test]
    fn jumps_to_the_next_edge() {
        let f = sample();
        // From the left margin, jumping right lands on the box, then past it.
        let (x, y) = next_edge(f.cross(5, 20, 0), 5, 20, Direction::Right, 100, 80);
        assert_eq!((x, y), (20, 20));
        let (x, _) = next_edge(f.cross(x, y, 0), x, y, Direction::Right, 100, 80);
        assert_eq!(x, 50);
        let (x, _) = next_edge(f.cross(x, y, 0), x, y, Direction::Right, 100, 80);
        assert_eq!(x, 99);
        // Back to the left: just outside the box.
        let (x, _) = next_edge(f.cross(30, 20, 0), 30, 20, Direction::Left, 100, 80);
        assert_eq!(x, 19);
        let (_, y) = next_edge(f.cross(30, 20, 0), 30, 20, Direction::Up, 100, 80);
        assert_eq!(y, 9);
        let (_, y) = next_edge(f.cross(30, 20, 0), 30, 20, Direction::Down, 100, 80);
        assert_eq!(y, 30);
        // At the frame edge it stays put.
        assert_eq!(next_edge(f.cross(0, 0, 0), 0, 0, Direction::Left, 100, 80), (0, 0));
    }

    #[test]
    fn line_geometry() {
        assert_eq!(line_length(3.0, 4.0), 5.0);
        assert_eq!(line_angle(10.0, 0.0), 0.0);
        assert_eq!(line_angle(0.0, -10.0), 90.0);
        assert_eq!(line_angle(-10.0, 0.0), 180.0);
        assert_eq!(line_angle(0.0, 10.0), -90.0);
        assert!((line_angle(10.0, -10.0) - 45.0).abs() < 1e-9);
        assert_eq!(format_angle(-0.01), "0°");
        assert_eq!(format_angle(32.46), "32.5°");
        assert_eq!(format_angle(-90.0), "-90°");
    }

    #[test]
    fn snaps_to_45_degrees() {
        assert_eq!(snap_45(10.0, 1.0), (line_length(10.0, 1.0), 0.0));
        assert_eq!(snap_45(1.0, -10.0), (0.0, -line_length(1.0, 10.0)));
        let (dx, dy) = snap_45(10.0, -9.0);
        assert!((dx - (-dy)).abs() < 1e-9 && dx > 0.0);
        assert_eq!(snap_45(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn aspect_ratios() {
        assert_eq!(aspect_ratio(1920, 1080), "16:9");
        assert_eq!(aspect_ratio(100, 100), "1:1");
        assert_eq!(aspect_ratio(997, 500), "1.99:1");
        assert_eq!(aspect_ratio(0, 5), "");
    }
}
