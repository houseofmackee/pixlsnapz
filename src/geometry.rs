//! Geometry for lines, angles, grids, snapping, spacing, and editing kept
//! measurements, in plain numbers.

use crate::measure::PxRect;

/// Lines every `step` from `start` up to `end` (inclusive of `start`).
pub fn grid_lines(start: f64, end: f64, step: f64) -> Vec<f64> {
    if step <= 0.0 || end <= start {
        return Vec::new();
    }
    let n = ((end - start) / step).floor() as usize;
    (0..=n).map(|i| start + i as f64 * step).collect()
}

/// `count` columns in `start..start + width`, with `gutter` between them and
/// `margin` on both sides, as (left, right) pairs. Empty when they do not fit.
pub fn columns(start: f64, width: f64, count: u32, gutter: f64, margin: f64) -> Vec<(f64, f64)> {
    if count == 0 {
        return Vec::new();
    }
    let n = f64::from(count);
    let column = (width - 2.0 * margin - gutter * (n - 1.0)) / n;
    if column <= 0.0 {
        return Vec::new();
    }
    (0..count)
        .map(|i| {
            let left = start + margin + f64::from(i) * (column + gutter);
            (left, left + column)
        })
        .collect()
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

/// The angle at `vertex` between the arms to `a` and `b`, in degrees
/// (`0..=180`).
pub fn angle_between(vertex: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (ax, ay) = (a.0 - vertex.0, a.1 - vertex.1);
    let (bx, by) = (b.0 - vertex.0, b.1 - vertex.1);
    let cross = ax * by - ay * bx;
    let dot = ax * bx + ay * by;
    cross.abs().atan2(dot).to_degrees()
}

/// Two lines meeting at a shared end: the vertex and the far ends of both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub first: usize,
    pub second: usize,
    pub vertex: (f64, f64),
    pub a: (f64, f64),
    pub b: (f64, f64),
}

impl Joint {
    pub fn angle(&self) -> f64 {
        angle_between(self.vertex, self.a, self.b)
    }
}

/// Pairs of `lines` (x0, y0, x1, y1) whose ends are within `tolerance` of
/// each other: the angles a protractor shows.
pub fn joints(lines: &[(f64, f64, f64, f64)], tolerance: f64) -> Vec<Joint> {
    let ends = |l: &(f64, f64, f64, f64)| [((l.0, l.1), (l.2, l.3)), ((l.2, l.3), (l.0, l.1))];
    let mut out = Vec::new();
    for (i, first) in lines.iter().enumerate() {
        for (j, second) in lines.iter().enumerate().skip(i + 1) {
            let hit = ends(first)
                .into_iter()
                .flat_map(|e| ends(second).map(move |f| (e, f)))
                .find(|((v, _), (w, _))| line_length(v.0 - w.0, v.1 - w.1) <= tolerance);
            if let Some(((vertex, a), (_, b))) = hit {
                out.push(Joint { first: i, second: j, vertex, a, b });
            }
        }
    }
    out
}

/// The nearest of `points` within `max` of `p`, or `p`.
pub fn snap_point(p: (f64, f64), points: &[(f64, f64)], max: f64) -> (f64, f64) {
    points
        .iter()
        .copied()
        .map(|q| (q, line_length(q.0 - p.0, q.1 - p.1)))
        .filter(|(_, d)| *d <= max)
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map_or(p, |(q, _)| q)
}

/// The nearest of `candidates` within `max` of `at`, or `at`.
pub fn snap_to(at: usize, candidates: &[usize], max: usize) -> usize {
    candidates.iter().copied().filter(|c| c.abs_diff(at) <= max).min_by_key(|c| c.abs_diff(at)).unwrap_or(at)
}

/// The distance from point `p` to the segment from `a` to `b`.
pub fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    line_length(p.0 - (a.0 + t * dx), p.1 - (a.1 + t * dy))
}

/// The eight handles of a box, clockwise from the top-left corner: corners
/// at even indexes, edge midpoints at odd ones.
pub fn rect_handles(r: PxRect) -> [(f64, f64); 8] {
    let (x0, y0, x1, y1) = (r.x0 as f64, r.y0 as f64, r.x1 as f64, r.y1 as f64);
    let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    [(x0, y0), (mx, y0), (x1, y0), (x1, my), (x1, y1), (mx, y1), (x0, y1), (x0, my)]
}

/// `r` with the edges that handle `handle` (see [`rect_handles`]) controls
/// moved to `(x, y)`, kept at least one pixel wide and high and inside
/// `width` by `height`. Dragging past the opposite edge flips the box.
pub fn resize_rect(r: PxRect, handle: usize, x: f64, y: f64, width: usize, height: usize) -> PxRect {
    let x = x.round().clamp(0.0, width as f64) as usize;
    let y = y.round().clamp(0.0, height as f64) as usize;
    let (mut x0, mut y0, mut x1, mut y1) = (r.x0, r.y0, r.x1, r.y1);
    match handle {
        0 => (x0, y0) = (x, y),
        1 => y0 = y,
        2 => (x1, y0) = (x, y),
        3 => x1 = x,
        4 => (x1, y1) = (x, y),
        5 => y1 = y,
        6 => (x0, y1) = (x, y),
        _ => x0 = x,
    }
    let (x0, x1) = (x0.min(x1), x0.max(x1).max(x0.min(x1) + 1));
    let (y0, y1) = (y0.min(y1), y0.max(y1).max(y0.min(y1) + 1));
    PxRect { x0, y0, x1: x1.min(width.max(1)), y1: y1.min(height.max(1)) }
}

/// `r` moved by `(dx, dy)` pixels, kept inside `width` by `height`.
pub fn move_rect(r: PxRect, dx: f64, dy: f64, width: usize, height: usize) -> PxRect {
    let (w, h) = (r.width() as f64, r.height() as f64);
    let x0 = (r.x0 as f64 + dx).round().clamp(0.0, (width as f64 - w).max(0.0));
    let y0 = (r.y0 as f64 + dy).round().clamp(0.0, (height as f64 - h).max(0.0));
    PxRect { x0: x0 as usize, y0: y0 as usize, x1: (x0 + w) as usize, y1: (y0 + h) as usize }
}

/// What lies next to a box on each side: the gap from the box's edge to the
/// nearest other box, guide, or screen edge, as (from, to) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spacing {
    pub left: (usize, usize),
    pub right: (usize, usize),
    pub top: (usize, usize),
    pub bottom: (usize, usize),
}

/// The spacing around `area` to `others` (boxes level with it on that
/// side), guides, and the edges of a `width` by `height` screen.
pub fn spacing(
    area: PxRect,
    others: &[PxRect],
    vertical: &[usize],
    horizontal: &[usize],
    width: usize,
    height: usize,
) -> Spacing {
    let level_x = |o: &&PxRect| o.y0 < area.y1 && area.y0 < o.y1;
    let level_y = |o: &&PxRect| o.x0 < area.x1 && area.x0 < o.x1;
    let left = others
        .iter()
        .filter(level_x)
        .map(|o| o.x1)
        .chain(vertical.iter().copied())
        .filter(|&e| e <= area.x0)
        .max()
        .unwrap_or(0);
    let right = others
        .iter()
        .filter(level_x)
        .map(|o| o.x0)
        .chain(vertical.iter().copied())
        .filter(|&e| e >= area.x1)
        .min()
        .unwrap_or(width);
    let top = others
        .iter()
        .filter(level_y)
        .map(|o| o.y1)
        .chain(horizontal.iter().copied())
        .filter(|&e| e <= area.y0)
        .max()
        .unwrap_or(0);
    let bottom = others
        .iter()
        .filter(level_y)
        .map(|o| o.y0)
        .chain(horizontal.iter().copied())
        .filter(|&e| e >= area.y1)
        .min()
        .unwrap_or(height);
    Spacing { left: (left, area.x0), right: (area.x1, right), top: (top, area.y0), bottom: (area.y1, bottom) }
}

/// Which line of a box lines up with another box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    CenterX,
    Top,
    Bottom,
    CenterY,
}

impl Edge {
    /// Runs top to bottom (a shared x coordinate).
    pub fn vertical(self) -> bool {
        matches!(self, Edge::Left | Edge::Right | Edge::CenterX)
    }
}

/// A line of `target` that lines up with `others[other]` at coordinate `at`
/// (pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alignment {
    pub edge: Edge,
    pub at: f64,
    pub other: usize,
}

/// Every line (edges and centers) of `target` within `tolerance` pixels of
/// the same kind of line, or the opposite edge, of another box.
pub fn alignments(target: PxRect, others: &[PxRect], tolerance: f64) -> Vec<Alignment> {
    let xs = |r: &PxRect| {
        [(Edge::Left, r.x0 as f64), (Edge::Right, r.x1 as f64), (Edge::CenterX, (r.x0 + r.x1) as f64 / 2.0)]
    };
    let ys = |r: &PxRect| {
        [(Edge::Top, r.y0 as f64), (Edge::Bottom, r.y1 as f64), (Edge::CenterY, (r.y0 + r.y1) as f64 / 2.0)]
    };
    let mut out = Vec::new();
    for (i, o) in others.iter().enumerate() {
        for (lines, theirs) in [(xs(&target), xs(o)), (ys(&target), ys(o))] {
            for (edge, at) in lines {
                let center = matches!(edge, Edge::CenterX | Edge::CenterY);
                // Centers line up with centers; edges with either edge.
                let hit = theirs.iter().find(|(e, v)| {
                    (center == matches!(e, Edge::CenterX | Edge::CenterY)) && (v - at).abs() <= tolerance
                });
                if let Some((_, v)) = hit {
                    out.push(Alignment { edge, at: *v, other: i });
                }
            }
        }
    }
    out
}

/// The other boxes as wide as `target`, and those as high, within
/// `tolerance` pixels.
pub fn same_sizes(target: PxRect, others: &[PxRect], tolerance: usize) -> (Vec<usize>, Vec<usize>) {
    let close = |a: usize, b: usize| a.abs_diff(b) <= tolerance;
    let wide = others.iter().enumerate().filter(|(_, o)| close(o.width(), target.width())).map(|(i, _)| i).collect();
    let high = others.iter().enumerate().filter(|(_, o)| close(o.height(), target.height())).map(|(i, _)| i).collect();
    (wide, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_alignments() {
        let target = rect(100, 100, 200, 150);
        let others = [
            rect(100, 300, 160, 340), // left edges
            rect(0, 150, 50, 180),    // target's bottom meets its top
            rect(125, 0, 175, 20),    // centered above
            rect(400, 400, 450, 450), // nothing in common
        ];
        let found = alignments(target, &others, 1.0);
        assert!(found.contains(&Alignment { edge: Edge::Left, at: 100.0, other: 0 }));
        assert!(found.contains(&Alignment { edge: Edge::Bottom, at: 150.0, other: 1 }));
        assert!(found.contains(&Alignment { edge: Edge::CenterX, at: 150.0, other: 2 }));
        assert!(!found.iter().any(|a| a.other == 3));
        assert!(Edge::CenterX.vertical() && !Edge::Top.vertical());
        // Within the tolerance.
        assert_eq!(alignments(rect(0, 0, 10, 10), &[rect(1, 50, 9, 60)], 1.0).len(), 3);
    }

    #[test]
    fn finds_same_sizes() {
        let (wide, high) =
            same_sizes(rect(0, 0, 100, 40), &[rect(5, 5, 105, 30), rect(0, 0, 60, 41), rect(0, 0, 3, 3)], 1);
        assert_eq!(wide, vec![0]);
        assert_eq!(high, vec![1]);
    }

    fn rect(x0: usize, y0: usize, x1: usize, y1: usize) -> PxRect {
        PxRect { x0, y0, x1, y1 }
    }

    #[test]
    fn distances_to_segments() {
        assert_eq!(distance_to_segment((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)), 3.0);
        assert_eq!(distance_to_segment((-4.0, 3.0), (0.0, 0.0), (10.0, 0.0)), 5.0);
        assert_eq!(distance_to_segment((1.0, 1.0), (0.0, 0.0), (0.0, 0.0)), 2f64.sqrt());
    }

    #[test]
    fn handles_resize_and_move_boxes() {
        let r = rect(10, 20, 50, 60);
        assert_eq!(rect_handles(r)[0], (10.0, 20.0));
        assert_eq!(rect_handles(r)[3], (50.0, 40.0));
        // The bottom-right corner, and the left edge.
        assert_eq!(resize_rect(r, 4, 70.0, 90.0, 100, 100), rect(10, 20, 70, 90));
        assert_eq!(resize_rect(r, 7, 0.0, 999.0, 100, 100), rect(0, 20, 50, 60));
        // Past the opposite edge, the box flips; it stays inside the screen.
        assert_eq!(resize_rect(r, 3, 5.0, 0.0, 100, 100), rect(5, 20, 10, 60));
        assert_eq!(resize_rect(r, 4, 500.0, 500.0, 100, 100), rect(10, 20, 100, 100));
        // Never thinner than a pixel.
        assert_eq!(resize_rect(r, 3, 10.0, 0.0, 100, 100), rect(10, 20, 11, 60));
        assert_eq!(move_rect(r, 5.0, -30.0, 100, 100), rect(15, 0, 55, 40));
        assert_eq!(move_rect(r, 90.0, 0.0, 100, 100), rect(60, 20, 100, 60));
    }

    #[test]
    fn spacing_to_neighbors() {
        let area = rect(40, 40, 60, 60);
        let others = [rect(0, 45, 10, 50), rect(80, 0, 90, 100), rect(45, 0, 55, 30), rect(0, 0, 5, 5)];
        let s = spacing(area, &others, &[20], &[], 200, 100);
        // Left: the guide at 20 is nearer than the box ending at 10.
        assert_eq!(s.left, (20, 40));
        assert_eq!(s.right, (60, 80));
        assert_eq!(s.top, (30, 40));
        // Nothing below: the screen edge.
        assert_eq!(s.bottom, (60, 100));
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
    fn grids() {
        assert_eq!(grid_lines(0.0, 20.0, 8.0), vec![0.0, 8.0, 16.0]);
        assert!(grid_lines(0.0, 20.0, 0.0).is_empty());
        let c = columns(0.0, 100.0, 4, 10.0, 5.0);
        assert_eq!(c, vec![(5.0, 20.0), (30.0, 45.0), (55.0, 70.0), (80.0, 95.0)]);
        assert!(columns(0.0, 10.0, 12, 20.0, 0.0).is_empty());
    }

    #[test]
    fn snaps_to_nearby_candidates() {
        assert_eq!(snap_to(50, &[44, 53, 90], 5), 53);
        assert_eq!(snap_to(50, &[40, 60], 5), 50);
    }
    #[test]
    fn protractor_angles() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(angle_between((0.0, 0.0), (10.0, 0.0), (0.0, 10.0)), 90.0));
        assert!(close(angle_between((0.0, 0.0), (10.0, 0.0), (-10.0, 0.0)), 180.0));
        assert!(close(angle_between((0.0, 0.0), (10.0, 0.0), (10.0, -10.0)), 45.0));
        // Line 0 ends where line 1 starts (within 1.5); line 2 is apart.
        let lines = [(0.0, 0.0, 100.0, 0.0), (101.0, 1.0, 101.0, 80.0), (300.0, 300.0, 400.0, 400.0)];
        let j = joints(&lines, 1.5);
        assert_eq!(j.len(), 1);
        assert_eq!((j[0].first, j[0].second), (0, 1));
        assert_eq!((j[0].vertex, j[0].a, j[0].b), ((100.0, 0.0), (0.0, 0.0), (101.0, 80.0)));
        assert!(joints(&lines, 0.5).is_empty());
        assert_eq!(snap_point((98.0, 3.0), &[(0.0, 0.0), (100.0, 0.0)], 5.0), (100.0, 0.0));
        assert_eq!(snap_point((90.0, 3.0), &[(100.0, 0.0)], 5.0), (90.0, 3.0));
    }
}
