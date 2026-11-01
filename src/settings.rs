//! Settings, stored as TOML in
//! `~/Library/Application Support/PixlSnapz/settings.toml`
//! (or `$PIXLSNAPZ_HOME/settings.toml`).

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::color::{ColorFormat, Vision};
use crate::shortcut::Shortcut;
use crate::units::SizeFormat;

/// Tolerance steps offered in the menu and with [ and ] in the overlay.
/// Color choices offered in the menu.
pub const COLORS: &[(&str, &str)] = &[
    (n!("Pink"), "#FF2D78"),
    (n!("Orange"), "#FF7A1A"),
    (n!("Cyan"), "#00C2FF"),
    (n!("Green"), "#2BD96B"),
    (n!("Purple"), "#9B5CFF"),
    (n!("Yellow"), "#FFD60A"),
];

pub const TOLERANCE_STEPS: &[(u8, &str)] =
    &[(0, n!("Exact")), (8, n!("Low")), (24, n!("Medium")), (48, n!("High")), (96, n!("Very High"))];

/// Loupe magnifications, in screen points per pixel.
pub const LOUPE_ZOOMS: &[u32] = &[4, 6, 8, 12, 16, 24, 32];

/// Which distances the pointer measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Axes {
    #[default]
    Both,
    Horizontal,
    Vertical,
}

impl Axes {
    pub fn next(self) -> Axes {
        match self {
            Axes::Both => Axes::Horizontal,
            Axes::Horizontal => Axes::Vertical,
            Axes::Vertical => Axes::Both,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Axes::Both => n!("Both directions"),
            Axes::Horizontal => n!("Horizontal only"),
            Axes::Vertical => n!("Vertical only"),
        }
    }

    pub fn horizontal(self) -> bool {
        self != Axes::Vertical
    }

    pub fn vertical(self) -> bool {
        self != Axes::Horizontal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Points,
    Pixels,
    /// Real-world size on the display.
    Millimeters,
    Inches,
}

impl Units {
    pub const ALL: [Units; 4] = [Units::Points, Units::Pixels, Units::Millimeters, Units::Inches];

    pub fn next(self) -> Units {
        let i = Units::ALL.iter().position(|u| *u == self).unwrap_or(0);
        Units::ALL[(i + 1) % Units::ALL.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Units::Points => n!("Points"),
            Units::Pixels => n!("Pixels"),
            Units::Millimeters => n!("Millimeters"),
            Units::Inches => n!("Inches"),
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Units::Points => "pt",
            Units::Pixels => "px",
            Units::Millimeters => "mm",
            Units::Inches => "in",
        }
    }
}

/// The layout grid drawn over the screen (or the last area).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Grid {
    #[default]
    Off,
    /// Squares of `grid_size` points.
    Square,
    /// `grid_columns` columns with gutters and margins.
    Columns,
}

impl Grid {
    pub const ALL: [Grid; 3] = [Grid::Off, Grid::Square, Grid::Columns];

    pub fn next(self) -> Grid {
        match self {
            Grid::Off => Grid::Square,
            Grid::Square => Grid::Columns,
            Grid::Columns => Grid::Off,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Grid::Off => n!("Off"),
            Grid::Square => n!("Square Grid"),
            Grid::Columns => n!("Column Grid"),
        }
    }
}

/// Design zoom levels offered in the menu, in percent.
/// Modifier keys that, held on their own, measure until released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HoldKeys {
    #[default]
    Off,
    ControlOption,
    OptionCommand,
    ControlCommand,
}

impl HoldKeys {
    pub const ALL: [HoldKeys; 4] =
        [HoldKeys::Off, HoldKeys::ControlOption, HoldKeys::OptionCommand, HoldKeys::ControlCommand];

    pub fn name(self) -> &'static str {
        match self {
            HoldKeys::Off => n!("Off"),
            HoldKeys::ControlOption => "⌃⌥",
            HoldKeys::OptionCommand => "⌥⌘",
            HoldKeys::ControlCommand => "⌃⌘",
        }
    }

    /// The modifiers as `shortcut` bits (`CONTROL`, `OPTION`, `CMD`).
    pub fn modifiers(self) -> u32 {
        use crate::shortcut::{CMD, CONTROL, OPTION};
        match self {
            HoldKeys::Off => 0,
            HoldKeys::ControlOption => CONTROL | OPTION,
            HoldKeys::OptionCommand => OPTION | CMD,
            HoldKeys::ControlCommand => CONTROL | CMD,
        }
    }
}

pub const DESIGN_ZOOMS: &[u32] = &[25, 33, 50, 67, 75, 100, 125, 150, 200, 300, 400];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Global shortcut that starts measuring, for example "shift+cmd+1".
    pub shortcut: String,
    pub units: Units,
    /// How different a color must be (0 to 255 per channel) to count as an edge.
    pub tolerance: u8,
    /// Shrink dragged areas to the content inside them.
    pub snap_areas: bool,
    /// Show the shortcut hints at the bottom of the screen.
    pub show_hints: bool,
    /// Show a magnifier next to the cursor.
    pub loupe: bool,
    /// Color of the guides, as "#RRGGBB".
    pub color: String,
    /// Add "pt" or "px" to dimensions.
    pub show_units: bool,
    /// Add the aspect ratio to area dimensions.
    pub show_aspect_ratio: bool,
    /// Show the screen as it was when measuring started (a frozen copy).
    pub freeze: bool,
    /// Where screenshots of areas are saved; empty for the Desktop.
    pub screenshot_folder: String,
    /// Which distances the pointer measures.
    pub axes: Axes,
    /// How colors are copied.
    pub color_format: ColorFormat,
    /// Loupe magnification (one of `LOUPE_ZOOMS`).
    pub loupe_zoom: u32,
    /// Zoom of the design being measured, in percent: lengths are divided by
    /// it, so a Figma frame at 200% measures in its own units.
    pub design_zoom: u32,
    /// Pixels per inch of the displays, for millimeters and inches; 0 uses
    /// the size each display reports.
    pub screen_ppi: f64,
    pub grid: Grid,
    /// Square grid size, in points.
    pub grid_size: f64,
    pub grid_columns: u32,
    /// Space between columns, in points.
    pub grid_gutter: f64,
    /// Space left and right of the columns, in points.
    pub grid_margin: f64,
    /// Draw the measurements into screenshots of areas.
    pub stamp_screenshots: bool,
    /// Color vision simulated in the loupe.
    pub vision: Vision,
    /// Bundle identifiers of apps in which the shortcut is left to the app.
    pub disabled_apps: Vec<String>,
    /// Find edges on a smoothed screen, comparing neighboring pixels: for
    /// photos, compressed images, and gradients.
    pub smooth_edges: bool,
    /// How ⌘C copies sizes.
    pub size_format: SizeFormat,
    /// The design overlay: an image file shown over the screen while
    /// measuring, to compare a design with the real thing.
    pub overlay_image: String,
    pub overlay_visible: bool,
    /// From 0 to 1.
    pub overlay_opacity: f64,
    /// Show the difference between the image and the screen instead.
    pub overlay_difference: bool,
    /// The image's top-left corner, in screen pixels.
    pub overlay_x: f64,
    pub overlay_y: f64,
    /// Global shortcut that picks a color (with the loupe) and copies it;
    /// empty for none.
    pub color_shortcut: String,
    /// Hold these modifier keys to measure until they are released.
    pub hold_keys: HoldKeys,
    /// ⌃ snaps to buttons, fields, and other elements inside windows, not
    /// just windows (needs Accessibility permission).
    pub element_snapping: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            shortcut: "shift+cmd+1".into(),
            units: Units::Points,
            tolerance: 8,
            snap_areas: true,
            show_hints: true,
            loupe: false,
            color: "#FF2D78".into(),
            show_units: false,
            show_aspect_ratio: false,
            freeze: false,
            screenshot_folder: String::new(),
            axes: Axes::Both,
            color_format: ColorFormat::Hex,
            loupe_zoom: 8,
            design_zoom: 100,
            screen_ppi: 0.0,
            grid: Grid::Off,
            grid_size: 8.0,
            grid_columns: 12,
            grid_gutter: 20.0,
            grid_margin: 0.0,
            stamp_screenshots: true,
            vision: Vision::Normal,
            disabled_apps: Vec::new(),
            smooth_edges: false,
            size_format: SizeFormat::Plain,
            overlay_image: String::new(),
            overlay_visible: false,
            overlay_opacity: 0.5,
            overlay_difference: false,
            overlay_x: 0.0,
            overlay_y: 0.0,
            color_shortcut: "shift+cmd+2".into(),
            hold_keys: HoldKeys::Off,
            element_snapping: false,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        let dir = std::env::var_os("PIXLSNAPZ_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("PixlSnapz"));
        dir.join("settings.toml")
    }

    /// Loads the settings, falling back to the defaults for a missing file
    /// or missing keys. A broken file is reported and ignored.
    pub fn load() -> Settings {
        let path = Settings::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => Settings::from_toml(&text).unwrap_or_else(|e| {
                tracing::warn!("ignoring {}: {e:#}", path.display());
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn from_toml(text: &str) -> Result<Settings> {
        Ok(toml::from_str(text)?)
    }

    pub fn save(&self) -> Result<()> {
        let path = Settings::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let text = toml::to_string_pretty(self)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// The shortcut, or the default one when the setting is invalid.
    pub fn shortcut(&self) -> Shortcut {
        Shortcut::parse(&self.shortcut).unwrap_or_else(|e| {
            tracing::warn!("invalid shortcut setting: {e}; using the default");
            Shortcut::parse(&Settings::default().shortcut).expect("the default shortcut is valid")
        })
    }

    /// The color picking shortcut, if there is a valid one.
    pub fn color_shortcut(&self) -> Option<Shortcut> {
        if self.color_shortcut.trim().is_empty() {
            return None;
        }
        Shortcut::parse(&self.color_shortcut).inspect_err(|e| tracing::warn!("invalid color shortcut: {e}")).ok()
    }

    /// The guide color as RGB components from 0 to 1.
    pub fn rgb(&self) -> [f64; 3] {
        parse_hex(&self.color).unwrap_or_else(|| parse_hex(&Settings::default().color).expect("valid default color"))
    }

    /// The next tolerance step up (`up`) or down from the current value.
    pub fn step_tolerance(&mut self, up: bool) {
        let current = self.tolerance;
        let next = if up {
            TOLERANCE_STEPS.iter().map(|s| s.0).find(|&t| t > current)
        } else {
            TOLERANCE_STEPS.iter().rev().map(|s| s.0).find(|&t| t < current)
        };
        if let Some(t) = next {
            self.tolerance = t;
        }
    }

    pub fn screenshot_dir(&self) -> PathBuf {
        if self.screenshot_folder.trim().is_empty() {
            dirs::desktop_dir().unwrap_or_else(|| PathBuf::from("."))
        } else {
            let folder = self.screenshot_folder.trim();
            match folder.strip_prefix("~/") {
                Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
                None => PathBuf::from(folder),
            }
        }
    }

    /// Whether the shortcut is on while the app `bundle_id` is in front.
    pub fn shortcut_enabled_in(&self, bundle_id: &str) -> bool {
        !self.disabled_apps.iter().any(|a| a.eq_ignore_ascii_case(bundle_id))
    }

    /// Turns the shortcut off in `bundle_id`, or back on.
    pub fn toggle_disabled_app(&mut self, bundle_id: &str) {
        if self.shortcut_enabled_in(bundle_id) {
            self.disabled_apps.push(bundle_id.to_string());
            self.disabled_apps.sort();
        } else {
            self.disabled_apps.retain(|a| !a.eq_ignore_ascii_case(bundle_id));
        }
    }

    /// Moves the tolerance by `delta`, within 0 to 255.
    pub fn adjust_tolerance(&mut self, delta: i32) {
        self.tolerance = (i32::from(self.tolerance) + delta).clamp(0, 255) as u8;
    }

    /// The next overlay look: 25, 50, 75, and 100% opacity, then the
    /// difference view, then 25% again.
    pub fn step_overlay_look(&mut self) {
        if self.overlay_difference {
            self.overlay_difference = false;
            self.overlay_opacity = 0.25;
        } else if self.overlay_opacity >= 0.99 {
            self.overlay_difference = true;
        } else {
            self.overlay_opacity = ((self.overlay_opacity * 4.0).floor() + 1.0).min(4.0) / 4.0;
        }
    }

    /// The next design zoom preset up or down.
    pub fn step_design_zoom(&mut self, up: bool) {
        let current = self.design_zoom;
        let next = if up {
            DESIGN_ZOOMS.iter().copied().find(|&z| z > current)
        } else {
            DESIGN_ZOOMS.iter().rev().copied().find(|&z| z < current)
        };
        if let Some(z) = next {
            self.design_zoom = z;
        }
    }

    /// The next loupe zoom step in or out.
    pub fn step_loupe_zoom(&mut self, zoom_in: bool) {
        let current = self.loupe_zoom;
        let next = if zoom_in {
            LOUPE_ZOOMS.iter().copied().find(|&z| z > current)
        } else {
            LOUPE_ZOOMS.iter().rev().copied().find(|&z| z < current)
        };
        if let Some(z) = next {
            self.loupe_zoom = z;
        }
    }

    pub fn tolerance_name(&self) -> String {
        TOLERANCE_STEPS
            .iter()
            .find(|s| s.0 == self.tolerance)
            .map_or_else(|| format!("{}", self.tolerance), |s| s.1.to_string())
    }
}

fn parse_hex(text: &str) -> Option<[f64; 3]> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|v| f64::from(v) / 255.0);
    Some([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_use_defaults() {
        let s = Settings::from_toml("units = \"pixels\"\ntolerance = 24\n").unwrap();
        assert_eq!(s.units, Units::Pixels);
        assert_eq!(s.tolerance, 24);
        assert_eq!(s.shortcut, Settings::default().shortcut);
        assert!(s.snap_areas);
    }

    #[test]
    fn round_trips() {
        let s = Settings { loupe: true, color: "#00ff00".into(), ..Settings::default() };
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::from_toml(&text).unwrap(), s);
    }

    #[test]
    fn colors() {
        let s = Settings { color: "#00FF80".into(), ..Settings::default() };
        assert_eq!(s.rgb(), [0.0, 1.0, 128.0 / 255.0]);
        let s = Settings { color: "teal".into(), ..Settings::default() };
        assert_eq!(s.rgb(), Settings::default().rgb());
    }

    #[test]
    fn tolerance_steps() {
        let mut s = Settings { tolerance: 8, ..Settings::default() };
        s.step_tolerance(true);
        assert_eq!(s.tolerance, 24);
        s.step_tolerance(false);
        s.step_tolerance(false);
        s.step_tolerance(false);
        assert_eq!(s.tolerance, 0);
        s.tolerance = 30;
        s.step_tolerance(false);
        assert_eq!(s.tolerance, 24);
        assert_eq!(s.tolerance_name(), "Medium");
    }

    #[test]
    fn adjusts_tolerance_and_zoom() {
        let mut s = Settings { tolerance: 250, ..Settings::default() };
        s.adjust_tolerance(15);
        assert_eq!(s.tolerance, 255);
        s.adjust_tolerance(-300);
        assert_eq!(s.tolerance, 0);
        s.loupe_zoom = 8;
        s.step_loupe_zoom(true);
        assert_eq!(s.loupe_zoom, 12);
        s.loupe_zoom = 4;
        s.step_loupe_zoom(false);
        assert_eq!(s.loupe_zoom, 4);
        s.loupe_zoom = 10;
        s.step_loupe_zoom(false);
        assert_eq!(s.loupe_zoom, 8);
    }

    #[test]
    fn units_and_grid_cycle() {
        assert_eq!(Units::Points.next(), Units::Pixels);
        assert_eq!(Units::Inches.next(), Units::Points);
        assert_eq!(Grid::Off.next().next().next(), Grid::Off);
        let s = Settings::from_toml("units = \"millimeters\"\ngrid = \"columns\"\n").unwrap();
        assert_eq!((s.units, s.grid, s.design_zoom), (Units::Millimeters, Grid::Columns, 100));
    }

    #[test]
    fn disabled_apps() {
        let mut s = Settings::default();
        assert!(s.shortcut_enabled_in("com.figma.Desktop"));
        s.toggle_disabled_app("com.figma.Desktop");
        assert!(!s.shortcut_enabled_in("com.figma.desktop"));
        assert!(s.shortcut_enabled_in("com.apple.Safari"));
        s.toggle_disabled_app("com.figma.Desktop");
        assert!(s.disabled_apps.is_empty());
        let s = Settings::from_toml("disabled_apps = [\"com.a\", \"com.b\"]\nvision = \"tritanopia\"\n").unwrap();
        assert!(!s.shortcut_enabled_in("com.b"));
        assert_eq!(s.vision, Vision::Tritanopia);
        assert!(s.stamp_screenshots);
    }

    #[test]
    fn design_zoom_steps() {
        let mut s = Settings::default();
        s.step_design_zoom(true);
        assert_eq!(s.design_zoom, 125);
        s.design_zoom = 90;
        s.step_design_zoom(false);
        assert_eq!(s.design_zoom, 75);
        s.design_zoom = 400;
        s.step_design_zoom(true);
        assert_eq!(s.design_zoom, 400);
    }

    #[test]
    fn overlay_looks_cycle() {
        let mut s = Settings::default();
        assert_eq!(s.overlay_opacity, 0.5);
        s.step_overlay_look();
        assert_eq!(s.overlay_opacity, 0.75);
        s.step_overlay_look();
        assert_eq!(s.overlay_opacity, 1.0);
        s.step_overlay_look();
        assert!(s.overlay_difference);
        s.step_overlay_look();
        assert!(!s.overlay_difference);
        assert_eq!(s.overlay_opacity, 0.25);
        s.overlay_opacity = 0.3;
        s.step_overlay_look();
        assert_eq!(s.overlay_opacity, 0.5);
    }

    #[test]
    fn axes_cycle() {
        assert_eq!(Axes::Both.next().next().next(), Axes::Both);
        assert!(Axes::Horizontal.horizontal() && !Axes::Horizontal.vertical());
    }

    #[test]
    fn invalid_shortcut_falls_back() {
        let s = Settings { shortcut: "nonsense".into(), ..Settings::default() };
        assert_eq!(s.shortcut().to_string(), "⇧⌘1");
    }
}
