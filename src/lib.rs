//! PixlSnapz: a screen ruler for macOS.
//!
//! The logic that needs no AppKit (edge detection, units, colors, kept
//! items, settings, shortcuts) is plain Rust and unit tested. The app
//! (menu bar item, overlay, Settings window, updates) is AppKit through
//! `objc2`; it lives here too, so the UI tests in `tests/` can build it.
//! `main.rs` is only the command line entry point.

#[cfg(not(target_os = "macos"))]
compile_error!("PixlSnapz only builds for macOS");

#[macro_use]
pub mod i18n;

pub mod color;
pub mod elements;
pub mod geometry;
pub mod integrations;
pub mod items;
pub mod measure;
pub mod settings;
pub mod shortcut;
pub mod units;

pub mod app;
pub mod capture;
mod hotkeys;
mod login_item;
pub mod overlay;
pub mod rulers;
pub mod settings_window;
mod updates;
