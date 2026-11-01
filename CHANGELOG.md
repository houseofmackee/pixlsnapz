# Changelog

All notable changes to PixlSnapz. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). The section for a version is what
the GitHub release shows.

## [Unreleased]

### Added

- A line ruler: ⌥-drag, or press L for the line tool, to measure the
  length and angle of any line, with a protractor arc. ⇧ snaps it to 45°.
- ⌥ plus an arrow key jumps the pointer to the next edge.
- Scrolling changes the edge tolerance.
- Press X to measure only horizontally or only vertically.
- Colors can be copied as hex, `rgb()`, `hsl()`, SwiftUI, or AppKit code
  (⇧P switches), and ⌥P copies the average color of the last area.
- The loupe shows the pointer's coordinates and zooms from 4× to 32×
  (- and +).
- R brings back the previous session's measurements and guides even
  after PixlSnapz restarts.
- A contrast checker: K on two colors copies their WCAG contrast ratio
  and grade, with a live readout in between.
- Millimeters and inches, from the size each display reports or a PPI
  you set, and a design zoom (25% to 400%) to measure designs in their
  own units.
- Square and column layout grids (O), over the screen or inside the last
  area.
- B keeps the measurements on screen after measuring, letting clicks
  through; the next session picks them up again.
- A Settings window (⌘, in the menu) with every option and a shortcut
  recorder, and `pixlsnapz settings` to open it.
- A circle tool (E): radius and diameter from the center.
- Screenshots of areas include the measurements (M turns that off).
- Recent Colors in the menu bar icon's menu: the last 20 copied colors.
- The loupe can show how people with color vision deficiencies see the
  screen (⇧Z).
- The shortcut can be turned off in chosen apps, from the menu or
  Settings, so they get the keys.

### Fixed

- The Settings window is split into tabs (General, Measuring, Colors &
  Loupe, Grid, Screenshots), so it fits on small screens.
- Measuring no longer ends when PixlSnapz itself becomes active.
- Screenshots for measuring leave out PixlSnapz's own windows.

## [0.1.0] - 2026-11-01

First release: distances to the nearest edges, areas that snap to their
content, kept measurements, guides with distances, a crosshair for
alignment, a loupe with color picking, area screenshots, freeze screen,
points or pixels, aspect ratios, adjustable edge tolerance, restoring the
previous session, multiple displays, and a customizable global shortcut.
The pointer is hidden while measuring.

[Unreleased]: https://github.com/houseofmackee/pixlsnapz/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/houseofmackee/pixlsnapz/releases/tag/v0.1.0
