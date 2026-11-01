//! The parts of PixlSnapz that do not need AppKit: edge detection, colors,
//! kept items, settings, and shortcuts. The app itself is in `main.rs` and
//! its modules.

pub mod color;
pub mod items;
pub mod measure;
pub mod settings;
pub mod shortcut;
