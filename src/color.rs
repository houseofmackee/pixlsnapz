//! Color formats for copying, and the average color of an area.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::measure::{Frame, PxRect};
use crate::settings::Settings;

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

    /// A compact form for the loupe: the code formats are too long to show
    /// in full, so they show just their three values.
    pub fn short(self, rgb: [u8; 3]) -> String {
        match self {
            ColorFormat::SwiftUI | ColorFormat::AppKit => {
                let unit = |v: u8| format!("{:.3}", f64::from(v) / 255.0);
                format!("{} {}, {}, {}", self.name(), unit(rgb[0]), unit(rgb[1]), unit(rgb[2]))
            }
            _ => self.format(rgb),
        }
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

fn to_linear(v: u8) -> f64 {
    let c = f64::from(v) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(l: f64) -> u8 {
    let l = l.clamp(0.0, 1.0);
    let c = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (c * 255.0).round() as u8
}

/// A color vision deficiency to simulate in the loupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Vision {
    #[default]
    Normal,
    /// No red cones.
    Protanopia,
    /// No green cones.
    Deuteranopia,
    /// No blue cones.
    Tritanopia,
    /// No color at all.
    Achromatopsia,
}

impl Vision {
    pub const ALL: [Vision; 5] =
        [Vision::Normal, Vision::Protanopia, Vision::Deuteranopia, Vision::Tritanopia, Vision::Achromatopsia];

    pub fn name(self) -> &'static str {
        match self {
            Vision::Normal => n!("Normal vision"),
            Vision::Protanopia => n!("Protanopia (red-blind)"),
            Vision::Deuteranopia => n!("Deuteranopia (green-blind)"),
            Vision::Tritanopia => n!("Tritanopia (blue-blind)"),
            Vision::Achromatopsia => n!("Achromatopsia (no color)"),
        }
    }

    pub fn next(self) -> Vision {
        let i = Vision::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Vision::ALL[(i + 1) % Vision::ALL.len()]
    }

    /// How `rgb` looks with this vision: Machado, Oliveira and Fernandes
    /// (2009) at full severity, applied in linear RGB.
    pub fn simulate(self, rgb: [u8; 3]) -> [u8; 3] {
        let m: [[f64; 3]; 3] = match self {
            Vision::Normal => return rgb,
            Vision::Achromatopsia => {
                let y = to_srgb(luminance(rgb));
                return [y, y, y];
            }
            Vision::Protanopia => {
                [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]]
            }
            Vision::Deuteranopia => {
                [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]]
            }
            Vision::Tritanopia => {
                [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]]
            }
        };
        let lin = rgb.map(to_linear);
        let row = |r: [f64; 3]| to_srgb(r[0] * lin[0] + r[1] * lin[1] + r[2] * lin[2]);
        [row(m[0]), row(m[1]), row(m[2])]
    }
}

/// Recently copied colors, newest first, kept between launches.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ColorHistory {
    #[serde(default)]
    pub colors: Vec<[u8; 3]>,
}

impl ColorHistory {
    pub const MAX: usize = 20;

    pub fn path() -> PathBuf {
        Settings::path().with_file_name("colors.toml")
    }

    pub fn load_from(path: &Path) -> ColorHistory {
        std::fs::read_to_string(path).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(path, toml::to_string(self)?).with_context(|| format!("writing {}", path.display()))
    }

    /// Puts `rgb` first, without duplicates, keeping at most `MAX` colors.
    pub fn push(&mut self, rgb: [u8; 3]) {
        self.colors.retain(|c| *c != rgb);
        self.colors.insert(0, rgb);
        self.colors.truncate(ColorHistory::MAX);
    }

    /// Adds `rgb` to the saved history.
    pub fn remember(rgb: [u8; 3]) {
        let path = ColorHistory::path();
        let mut history = ColorHistory::load_from(&path);
        history.push(rgb);
        if let Err(e) = history.save_to(&path) {
            tracing::warn!("saving the color history: {e:#}");
        }
    }
}

/// Relative luminance of an sRGB color (WCAG 2).
pub fn luminance(rgb: [u8; 3]) -> f64 {
    0.2126 * to_linear(rgb[0]) + 0.7152 * to_linear(rgb[1]) + 0.0722 * to_linear(rgb[2])
}

/// The WCAG contrast ratio of two colors, from 1 to 21.
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The best WCAG level a ratio passes: "AAA", "AA", "AA large" (only for
/// large text), or "Fail".
pub fn contrast_grade(ratio: f64) -> &'static str {
    // Compare the shown (truncated) value, so "4.49:1" never reads as AA.
    let r = (ratio * 100.0).floor() / 100.0;
    if r >= 7.0 {
        "AAA"
    } else if r >= 4.5 {
        "AA"
    } else if r >= 3.0 {
        "AA large"
    } else {
        "Fail"
    }
}

/// "4.52:1  AA": the ratio truncated to two decimals (WCAG says not to
/// round up) and its grade.
pub fn contrast_text(a: [u8; 3], b: [u8; 3]) -> String {
    let ratio = contrast_ratio(a, b);
    format!("{:.2}:1  {}", (ratio * 100.0).floor() / 100.0, contrast_grade(ratio))
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
    fn short_formats() {
        assert_eq!(ColorFormat::SwiftUI.short([26, 43, 60]), "SwiftUI 0.102, 0.169, 0.235");
        assert_eq!(ColorFormat::Rgb.short([26, 43, 60]), "rgb(26, 43, 60)");
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
    fn contrast() {
        assert!((contrast_ratio([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio([10, 20, 30], [10, 20, 30]) - 1.0).abs() < 1e-9);
        // #767676 on white is the classic 4.54:1.
        assert_eq!(contrast_text([0x76, 0x76, 0x76], [255, 255, 255]), "4.54:1  AA");
        assert_eq!(contrast_text([255, 255, 255], [0, 0, 0]), "21.00:1  AAA");
        assert_eq!(contrast_grade(3.2), "AA large");
        assert_eq!(contrast_grade(4.499), "AA large");
        assert_eq!(contrast_grade(2.9), "Fail");
    }

    #[test]
    fn simulates_vision() {
        for v in Vision::ALL {
            // White, black and grays look the same to everyone.
            for g in [0u8, 128, 255] {
                let out = v.simulate([g, g, g]);
                assert!(out.iter().all(|c| c.abs_diff(g) <= 2), "{v:?} changed gray {g} to {out:?}");
            }
        }
        assert_eq!(Vision::Normal.simulate([200, 10, 30]), [200, 10, 30]);
        // Without color, pure red is the gray of its luminance.
        assert_eq!(Vision::Achromatopsia.simulate([255, 0, 0]), [127, 127, 127]);
        // Red and green are hard to tell apart without red cones.
        let red = Vision::Protanopia.simulate([255, 0, 0]);
        let green = Vision::Protanopia.simulate([0, 255, 0]);
        assert!(red[0].abs_diff(red[1]) < 40 && green[0].abs_diff(green[1]) < 40, "{red:?} {green:?}");
        assert_eq!(Vision::Achromatopsia.next(), Vision::Normal);
    }

    #[test]
    fn color_history() {
        let mut h = ColorHistory::default();
        for i in 0..25u8 {
            h.push([i, 0, 0]);
        }
        assert_eq!(h.colors.len(), ColorHistory::MAX);
        assert_eq!(h.colors[0], [24, 0, 0]);
        h.push([10, 0, 0]);
        assert_eq!(h.colors[0], [10, 0, 0]);
        assert_eq!(h.colors.iter().filter(|c| **c == [10, 0, 0]).count(), 1);

        let dir = std::env::temp_dir().join(format!("pixlsnapz-colors-{}", std::process::id()));
        let path = dir.join("colors.toml");
        h.save_to(&path).unwrap();
        assert_eq!(ColorHistory::load_from(&path), h);
        let _ = std::fs::remove_dir_all(dir);
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
