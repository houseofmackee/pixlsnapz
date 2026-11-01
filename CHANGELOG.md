# Changelog

All notable changes to PixlSnapz. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). The section for a version is what
the GitHub release shows.

## [Unreleased]

## [0.1.0] - 2026-11-01

First release of PixlSnapz, a screen ruler for Apple silicon Macs
(macOS 14 or later). Press ⇧⌘1 anywhere to measure.

### Measuring

- Distances from the pointer to the nearest edges, with the pointer
  hidden so nothing covers the spot the lines cross.
- Areas that snap to their content when you drag over them (⌘ or a
  right-drag measures exactly what you drag; ⇧ makes a square, Space
  moves it).
- A line ruler for length and angle (⌥-drag, or L), with ⇧ snapping to
  45°, and a circle tool for radius and diameter (E).
- Kept measurements (click), guides with the distances between them
  (V and H), and a crosshair for checking alignment (hold ⇧).
- ⌥ plus an arrow key jumps to the next edge; X measures one direction at
  a time.
- Points, pixels, millimeters, or inches, with a design zoom to measure
  designs in their own units, and adjustable edge tolerance (scroll, or
  [ and ]).
- Square and column layout grids (O).
- B keeps the measurements on screen while you work, letting clicks
  through; R brings back the previous session, even after a restart.

### Colors

- A loupe with the pointer's color and coordinates, zooming from 4× to
  32×, and a preview of how people with color vision deficiencies see
  the screen (⇧Z).
- Colors copy as hex, `rgb()`, `hsl()`, SwiftUI, or AppKit code, including
  the average color of an area, and the last 20 are under Recent Colors
  in the menu bar icon's menu.
- A contrast checker (K) that copies the WCAG ratio and grade.

### Everything else

- Screenshots of areas with the measurements drawn in (⌘S saves, ⇧⌘C
  copies).
- A Settings window with every option in tabs and a shortcut recorder;
  the shortcut can be turned off in chosen apps.
- Freeze screen, aspect ratios, multiple displays, launch at login, and
  `pixlsnapz measure` and `pixlsnapz settings` for scripts and launchers.

[Unreleased]: https://github.com/houseofmackee/pixlsnapz/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/houseofmackee/pixlsnapz/releases/tag/v0.1.0
