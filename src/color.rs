//! Color formats for copying, and the average color of an area.

use serde::{Deserialize, Serialize};

use crate::measure::{Frame, PxRect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColorFormat {
    /// `#1A2B3C`
    #[default]
    Hex,
    /// `rgb(26, 43, 60)`
    Rgb,
    /// `hsl(210, 40%, 17%)`
    Hsl,
    /// `Color(red: 0.102, green: 0.169, blue: 0.235)`
    SwiftUI,
    /// `NSColor(srgbRed: 0.102, green: 0.169, blue: 0.235, alpha: 1)`
    AppKit,
}

impl ColorFormat {
    pub const ALL: [ColorFormat; 5] =
        [ColorFormat::Hex, ColorFormat::Rgb, ColorFormat::Hsl, ColorFormat::SwiftUI, ColorFormat::AppKit];

    pub fn name(self) -> &'static str {
        match self {
            ColorFormat::Hex => "Hex",
            ColorFormat::Rgb => "RGB",
            ColorFormat::Hsl => "HSL",
            ColorFormat::SwiftUI => "SwiftUI",
            ColorFormat::AppKit => "AppKit",
        }
    }

    pub fn next(self) -> ColorFormat {
        let i = ColorFormat::ALL.iter().position(|f| *f == self).unwrap_or(0);
        ColorFormat::ALL[(i + 1) % ColorFormat::ALL.len()]
    }

    pub fn format(self, [r, g, b]: [u8; 3]) -> String {
        let unit = |v: u8| {
            let s = format!("{:.3}", f64::from(v) / 255.0);
            // "1.000" and "0.000" read better as "1" and "0".
            match s.as_str() {
                "1.000" => "1".to_string(),
                "0.000" => "0".to_string(),
                _ => s,
            }
        };
        match self {
            ColorFormat::Hex => format!("#{r:02X}{g:02X}{b:02X}"),
            ColorFormat::Rgb => format!("rgb({r}, {g}, {b})"),
            ColorFormat::Hsl => {
                let (h, s, l) = hsl([r, g, b]);
                format!("hsl({h}, {s}%, {l}%)")
            }
            ColorFormat::SwiftUI => format!("Color(red: {}, green: {}, blue: {})", unit(r), unit(g), unit(b)),
            ColorFormat::AppKit => {
                format!("NSColor(srgbRed: {}, green: {}, blue: {}, alpha: 1)", unit(r), unit(g), unit(b))
            }
        }
    }
}

/// Hue in degrees, saturation and lightness in percent, rounded.
pub fn hsl([r, g, b]: [u8; 3]) -> (u32, u32, u32) {
    let (r, g, b) = (f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    ((h.round() as u32) % 360, (s * 100.0).round() as u32, (l * 100.0).round() as u32)
}

/// The average color of the pixels in `r`.
pub fn average(frame: &Frame, r: PxRect) -> Option<[u8; 3]> {
    let r = frame.clamp(r);
    let n = (r.width() * r.height()) as u64;
    if n == 0 {
        return None;
    }
    let mut sum = [0u64; 3];
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let p = frame.pixel(x, y);
            for c in 0..3 {
                sum[c] += u64::from(p[c]);
            }
        }
    }
    Some(sum.map(|s| ((s + n / 2) / n) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let c = [26, 43, 60];
        assert_eq!(ColorFormat::Hex.format(c), "#1A2B3C");
        assert_eq!(ColorFormat::Rgb.format(c), "rgb(26, 43, 60)");
        assert_eq!(ColorFormat::Hsl.format(c), "hsl(210, 40%, 17%)");
        assert_eq!(ColorFormat::SwiftUI.format(c), "Color(red: 0.102, green: 0.169, blue: 0.235)");
        assert_eq!(ColorFormat::AppKit.format([255, 0, 0]), "NSColor(srgbRed: 1, green: 0, blue: 0, alpha: 1)");
    }

    #[test]
    fn hsl_values() {
        assert_eq!(hsl([255, 0, 0]), (0, 100, 50));
        assert_eq!(hsl([0, 255, 0]), (120, 100, 50));
        assert_eq!(hsl([0, 0, 255]), (240, 100, 50));
        assert_eq!(hsl([128, 128, 128]), (0, 0, 50));
        assert_eq!(hsl([255, 0, 128]), (330, 100, 50));
    }

    #[test]
    fn cycles_formats() {
        assert_eq!(ColorFormat::Hex.next(), ColorFormat::Rgb);
        assert_eq!(ColorFormat::AppKit.next(), ColorFormat::Hex);
    }

    #[test]
    fn averages() {
        let mut f = Frame::solid(4, 2, [0, 0, 0]);
        f.fill(PxRect { x0: 0, y0: 0, x1: 2, y1: 2 }, [200, 100, 50]);
        assert_eq!(average(&f, PxRect { x0: 0, y0: 0, x1: 4, y1: 2 }), Some([100, 50, 25]));
        assert_eq!(average(&f, PxRect { x0: 0, y0: 0, x1: 1, y1: 1 }), Some([200, 100, 50]));
        assert_eq!(average(&f, PxRect { x0: 3, y0: 0, x1: 3, y1: 2 }), None);
    }
}
