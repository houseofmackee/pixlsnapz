//! Settings, stored as TOML in
//! `~/Library/Application Support/PixlSnapz/settings.toml`
//! (or `$PIXLSNAPZ_HOME/settings.toml`).

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::shortcut::Shortcut;

/// Tolerance steps offered in the menu and with [ and ] in the overlay.
/// Color choices offered in the menu.
pub const COLORS: &[(&str, &str)] = &[
    ("Pink", "#FF2D78"),
    ("Orange", "#FF7A1A"),
    ("Cyan", "#00C2FF"),
    ("Green", "#2BD96B"),
    ("Purple", "#9B5CFF"),
    ("Yellow", "#FFD60A"),
];

pub const TOLERANCE_STEPS: &[(u8, &str)] = &[(0, "Exact"), (8, "Low"), (24, "Medium"), (48, "High"), (96, "Very High")];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Points,
    Pixels,
}

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
    fn invalid_shortcut_falls_back() {
        let s = Settings { shortcut: "nonsense".into(), ..Settings::default() };
        assert_eq!(s.shortcut().to_string(), "⇧⌘1");
    }
}
