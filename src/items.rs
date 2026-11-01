//! What a measuring session keeps on screen, and saving it so the next
//! session (even after a restart) can bring it back.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::measure::{Cross, PxRect};
use crate::settings::{Axes, Settings};

/// Something kept on screen, in pixels of its display.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// Distances around a clicked point.
    Cross {
        x: usize,
        y: usize,
        cross: Cross,
        #[serde(default)]
        axes: Axes,
    },
    Area(PxRect),
    VerticalGuide {
        x: usize,
    },
    HorizontalGuide {
        y: usize,
    },
    /// A line from one point to another, with its length and angle.
    Line {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
    /// A circle around a center, with its radius and diameter.
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
    },
}

impl Item {
    pub fn is_guide(&self) -> bool {
        matches!(self, Item::VerticalGuide { .. } | Item::HorizontalGuide { .. })
    }
}

/// An item and the display it was on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedItem {
    pub display: u32,
    /// The display's size in pixels, to recognize it when its id changed.
    pub width: usize,
    pub height: usize,
    pub item: Item,
}

/// A display of the current session: its id and size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Display {
    pub id: u32,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedSession {
    #[serde(default)]
    pub items: Vec<SavedItem>,
}

impl SavedSession {
    pub fn path() -> PathBuf {
        Settings::path().with_file_name("last-session.toml")
    }

    pub fn load_from(path: &Path) -> SavedSession {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).map_err(|e| tracing::warn!("ignoring {}: {e}", path.display())).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string(self)?).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// The saved items placed on `displays`, as (display index, item): on
    /// the display with the same id and size, or else on one with the same
    /// size. Items for displays that are gone are left out.
    pub fn place(&self, displays: &[Display]) -> Vec<(usize, Item)> {
        self.items
            .iter()
            .filter_map(|s| {
                let same_size = |d: &Display| d.width == s.width && d.height == s.height;
                let index = displays
                    .iter()
                    .position(|d| d.id == s.display && same_size(d))
                    .or_else(|| displays.iter().position(same_size))?;
                Some((index, s.item))
            })
            .collect()
    }
}

/// A named set of measurements and guides, saved for later (⇧S).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedSet {
    pub name: String,
    #[serde(default)]
    pub items: Vec<SavedItem>,
}

impl NamedSet {
    /// The set as a saved session, to place on the current displays.
    pub fn session(&self) -> SavedSession {
        SavedSession { items: self.items.clone() }
    }
}

/// All saved sets, in the order they were first saved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedSets {
    #[serde(default)]
    pub sets: Vec<NamedSet>,
}

impl SavedSets {
    pub fn path() -> PathBuf {
        Settings::path().with_file_name("sets.toml")
    }

    pub fn load_from(path: &Path) -> SavedSets {
        std::fs::read_to_string(path).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string(self)?).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// The set called `name` (ignoring case).
    pub fn get(&self, name: &str) -> Option<&NamedSet> {
        self.sets.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Saves `items` as `name`, replacing a set of that name.
    pub fn put(&mut self, name: &str, items: Vec<SavedItem>) {
        let name = name.trim();
        match self.sets.iter_mut().find(|s| s.name.eq_ignore_ascii_case(name)) {
            Some(set) => set.items = items,
            None => self.sets.push(NamedSet { name: name.to_string(), items }),
        }
    }

    /// Removes the set called `name`; whether there was one.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.sets.len();
        self.sets.retain(|s| !s.name.eq_ignore_ascii_case(name));
        self.sets.len() != before
    }

    /// The set after the one called `current` (the first one when there is
    /// none, or after the last one).
    pub fn next_after(&self, current: Option<&str>) -> Option<&NamedSet> {
        let i = current.and_then(|c| self.sets.iter().position(|s| s.name.eq_ignore_ascii_case(c)));
        match i {
            Some(i) => self.sets.get((i + 1) % self.sets.len()),
            None => self.sets.first(),
        }
    }

    /// "Set 1", "Set 2", ...: the first name not taken.
    pub fn unused_name(&self, base: &str) -> String {
        (1..).map(|n| format!("{base} {n}")).find(|name| self.get(name).is_none()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(display: u32, width: usize, item: Item) -> SavedItem {
        SavedItem { display, width, height: 100, item }
    }

    #[test]
    fn round_trips_every_kind() {
        let session = SavedSession {
            items: vec![
                saved(1, 200, Item::Area(PxRect { x0: 1, y0: 2, x1: 3, y1: 4 })),
                saved(1, 200, Item::VerticalGuide { x: 7 }),
                saved(2, 300, Item::HorizontalGuide { y: 9 }),
                saved(
                    1,
                    200,
                    Item::Cross {
                        x: 5,
                        y: 6,
                        cross: Cross { left: 0, right: 10, top: 1, bottom: 8 },
                        axes: Axes::Horizontal,
                    },
                ),
                saved(1, 200, Item::Line { x0: 1.5, y0: 2.0, x1: 30.0, y1: 40.5 }),
                saved(1, 200, Item::Circle { cx: 50.0, cy: 60.5, r: 25.25 }),
            ],
        };
        let dir = tempdir();
        let path = dir.join("last-session.toml");
        session.save_to(&path).unwrap();
        assert_eq!(SavedSession::load_from(&path), session);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_or_broken_files_are_empty() {
        let dir = tempdir();
        assert_eq!(SavedSession::load_from(&dir.join("nope.toml")), SavedSession::default());
        let path = dir.join("broken.toml");
        std::fs::write(&path, "items = 5").unwrap();
        assert_eq!(SavedSession::load_from(&path), SavedSession::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn places_items_on_matching_displays() {
        let session = SavedSession {
            items: vec![
                saved(1, 200, Item::VerticalGuide { x: 1 }),
                saved(9, 300, Item::VerticalGuide { x: 2 }),
                saved(5, 999, Item::VerticalGuide { x: 3 }),
            ],
        };
        let displays = [Display { id: 4, width: 300, height: 100 }, Display { id: 1, width: 200, height: 100 }];
        // Same id and size, then same size under a new id; the third display is gone.
        assert_eq!(
            session.place(&displays),
            vec![(1, Item::VerticalGuide { x: 1 }), (0, Item::VerticalGuide { x: 2 })]
        );
    }

    #[test]
    fn saved_sets() {
        let guide = |x| vec![saved(1, 200, Item::VerticalGuide { x })];
        let mut sets = SavedSets::default();
        assert_eq!(sets.unused_name("Set"), "Set 1");
        sets.put("Header", guide(1));
        sets.put(" Footer ", guide(2));
        sets.put("header", guide(3));
        assert_eq!(sets.sets.len(), 2);
        assert_eq!(sets.get("HEADER").unwrap().items, guide(3));
        assert_eq!(sets.get("footer").unwrap().name, "Footer");
        assert_eq!(sets.next_after(None).unwrap().name, "Header");
        assert_eq!(sets.next_after(Some("Header")).unwrap().name, "Footer");
        assert_eq!(sets.next_after(Some("Footer")).unwrap().name, "Header");
        assert_eq!(sets.unused_name("Set"), "Set 1");
        sets.put("Set 1", Vec::new());
        assert_eq!(sets.unused_name("Set"), "Set 2");
        assert!(sets.remove("footer"));
        assert!(!sets.remove("footer"));

        let dir = tempdir();
        let path = dir.join("sets.toml");
        sets.save_to(&path).unwrap();
        assert_eq!(SavedSets::load_from(&path), sets);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pixlsnapz-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
