//! Geometry for lines, angles, grids, and snapping, in plain numbers.

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

/// The nearest of `candidates` within `max` of `at`, or `at`.
pub fn snap_to(at: usize, candidates: &[usize], max: usize) -> usize {
    candidates.iter().copied().filter(|c| c.abs_diff(at) <= max).min_by_key(|c| c.abs_diff(at)).unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
