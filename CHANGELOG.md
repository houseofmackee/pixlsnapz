# Changelog

All notable changes to PixlSnapz. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). The section for a version is what
the GitHub release shows.

## [Unreleased]

### Added

- Homebrew: `brew install --cask houseofmackee/tap/pixlsnapz`, kept up to
  date by each release.
- `PIXLSNAPZ_SHOW_SETTINGS=<tab>` opens Settings at a tab on launch (for
  screenshots and tests).
- VoiceOver: every Settings control has a name, measuring announces kept
  measurements, copies, and mode changes, and the measurement under the
  pointer can be read on request.
- With Increase Contrast or Reduce Transparency on, lines are thicker and
  labels opaque with an outline.

## [0.1.1] - 2026-11-01

### Added

- PixlSnapz updates itself: Check for Updates… in the menu bar icon's
  menu, and automatic checks in Settings > General. Updates are offered
  once a release is marked as a regular release. 0.1.0 cannot update
  itself, so download this version once by hand.
- Smooth edges (N, or Settings > Measuring) for photos, compressed images,
  and gradients: edges are found on a smoothed copy of the screen, between
  neighboring pixels, so noise and gentle color changes no longer stop the
  lines.
- D and ⇧D change the design zoom while measuring, with more steps (33%,
  67%, 75%, 125%, and 150%).

### Fixed

- The shortcut hints sit above the Dock instead of under it.
- The loupe shows SwiftUI and AppKit colors as their values instead of
  falling back to hex, and Recent Colors keeps short hex titles (clicking
  one still copies the chosen format).
- The Add App… and Clear buttons in Settings no longer touch.

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

[Unreleased]: https://github.com/houseofmackee/pixlsnapz/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/houseofmackee/pixlsnapz/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/houseofmackee/pixlsnapz/releases/tag/v0.1.0
