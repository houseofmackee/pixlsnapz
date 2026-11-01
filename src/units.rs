//! Converting lengths in pixels to points, pixels, millimeters, or inches,
//! and formatting them for display.

use crate::settings::Units;

/// How pixels of one display convert to other units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    /// Pixels per point (2 on Retina displays).
    pub px_per_pt: f64,
    /// Millimeters per pixel.
    pub mm_per_px: f64,
    /// Design zoom: lengths are divided by it (1 for none).
    pub zoom: f64,
}

impl Scale {
    /// `mm_per_px` from a manual PPI setting, or else from the display's
    /// reported width in millimeters; 96 PPI when neither is known.
    pub fn new(px_per_pt: f64, width_px: usize, width_mm: f64, ppi: f64, zoom_percent: u32) -> Scale {
        let mm_per_px = if ppi > 0.0 {
            25.4 / ppi
        } else if width_mm > 0.0 && width_px > 0 {
            width_mm / width_px as f64
        } else {
            25.4 / 96.0
        };
        let zoom = if zoom_percent == 0 { 1.0 } else { f64::from(zoom_percent) / 100.0 };
        Scale { px_per_pt, mm_per_px, zoom }
    }

    /// A length of `px` pixels in `units`.
    pub fn value(&self, px: f64, units: Units) -> f64 {
        let v = match units {
            Units::Pixels => px,
            Units::Points => px / self.px_per_pt,
            Units::Millimeters => px * self.mm_per_px,
            Units::Inches => px * self.mm_per_px / 25.4,
        };
        v / self.zoom
    }

    /// A length for display: one decimal (two for inches) when needed,
    /// with the unit's suffix when `suffix` is set.
    pub fn format(&self, px: f64, units: Units, suffix: bool) -> String {
        let decimals = if units == Units::Inches { 100.0 } else { 10.0 };
        let v = (self.value(px, units) * decimals).round() / decimals;
        // `Display` prints the shortest form: "20", "12.5", "0.5".
        let text = if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") };
        if suffix { format!("{text} {}", units.suffix()) } else { text }
    }
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

/// A measurement label as VoiceOver should read it: "140 by 38" for
/// "140 × 38", "radius 28, diameter 56" for "r 28  ·  ⌀ 56", and "12.5
/// degrees" for "12.5°".
pub fn spoken(text: &str) -> String {
    let mut out = text
        .replace(" × ", " by ")
        .replace("  ·  ", ", ")
        .replace(" · ", ", ")
        .replace("  ∠ ", ", angle ")
        .replace("⌀ ", "diameter ")
        .replace('°', " degrees");
    if let Some(rest) = out.strip_prefix("r ") {
        out = format!("radius {rest}");
    }
    for (short, long) in [(" pt", " points"), (" px", " pixels"), (" mm", " millimeters"), (" in", " inches")] {
        out = out.replace(&format!("{short},"), &format!("{long},"));
        if let Some(stripped) = out.strip_suffix(short) {
            out = format!("{stripped}{long}");
        }
    }
    // Squeeze runs of spaces left by the replacements.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_to_units() {
        // A 3456 px wide Retina display that is 345.6 mm wide: 0.1 mm per pixel.
        let s = Scale::new(2.0, 3456, 345.6, 0.0, 100);
        assert_eq!(s.format(200.0, Units::Points, false), "100");
        assert_eq!(s.format(200.0, Units::Pixels, true), "200 px");
        assert_eq!(s.format(200.0, Units::Millimeters, true), "20 mm");
        assert_eq!(s.format(254.0, Units::Inches, false), "1");
        assert_eq!(s.format(127.0, Units::Inches, false), "0.5");
        // Design zoom 200%: everything halves.
        let z = Scale::new(2.0, 3456, 345.6, 0.0, 200);
        assert_eq!(z.format(200.0, Units::Points, false), "50");
        // A manual PPI wins over the reported size; unknown sizes use 96 PPI.
        assert!((Scale::new(1.0, 1000, 500.0, 254.0, 100).mm_per_px - 0.1).abs() < 1e-12);
        assert_eq!(Scale::new(1.0, 1000, 0.0, 0.0, 100).format(96.0, Units::Inches, false), "1");
    }

    #[test]
    fn spoken_labels() {
        assert_eq!(spoken("140 × 38"), "140 by 38");
        assert_eq!(spoken("140 × 38 pt"), "140 by 38 points");
        assert_eq!(spoken("r 28  ·  ⌀ 56"), "radius 28, diameter 56");
        assert_eq!(spoken("r 28 pt  ·  ⌀ 56 pt"), "radius 28 points, diameter 56 points");
        assert_eq!(spoken("657.4  ∠ 7.3°"), "657.4, angle 7.3 degrees");
        assert_eq!(spoken("Copied 120 × 48"), "Copied 120 by 48");
        assert_eq!(spoken("31.9 × 12.8 mm"), "31.9 by 12.8 millimeters");
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
    fn aspect_ratios() {
        assert_eq!(aspect_ratio(1920, 1080), "16:9");
        assert_eq!(aspect_ratio(100, 100), "1:1");
        assert_eq!(aspect_ratio(997, 500), "1.99:1");
        assert_eq!(aspect_ratio(0, 5), "");
    }
}
