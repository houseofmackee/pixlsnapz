# Changelog

All notable changes to PixlSnapz. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). The section for a version is what
the GitHub release shows.

## [Unreleased]

### Added

- Hold ⌥ over a kept box to see its spacing to the nearest box, guide, or
  screen edge on every side.
- Kept boxes, lines, circles, guides, and points can be edited: hover one
  to see its handles, drag a handle to resize or the outline to move.
  ⌫ removes the item under the pointer.
- T switches what ⌘C copies for sizes: plain, CSS, Tailwind, SwiftUI, or
  CGSize (also in Settings and the menu).
- A design overlay: lay a design image over the real app at 25% to 100%
  opacity, or as a difference (I, ⇧I, ⌘ arrows, ⌘I; Design Overlay in
  the menu).
- ⌘E opens the last area, with its measurements, in CleanShotZ to
  annotate.
- Smart guides: while drawing or editing a box, dashed lines show where it
  lines up with other kept boxes, and "=" marks boxes of the same width or
  height and equal gaps in the spacing view.
- Hold ⌃ over a window to see its frame; ⌃-click keeps it as a box.
- Measurement sets: ⇧S saves what is on screen under a name, ⇧R cycles
  through saved sets, and Measurement Sets in the menu starts measuring
  with one or deletes it.
- `pixlsnapz://` links start measuring (optionally with a saved set),
  pick a color, or open a Settings tab or the Colors window.
- Protractor: a line started or ended at the end of a kept line shows the
  angle between the two, and ⌘C copies it.
- A color picker shortcut (⇧⌘2, or Pick Color in the menu): just the loupe,
  and a click copies the color.
- A Colors window: recent colors with their codes, click to copy, contrast
  between the last two clicked, palettes, and export as CSS, JSON, or ASE.
- Hold to measure: hold a chosen pair of modifier keys to see distances,
  and let go to stop (off by default; in Settings).
- Element snapping: with Snap to Buttons and Fields on (Accessibility
  permission), ⌃ highlights and keeps the button, field, or image under
  the pointer, not just the window.
- Live mode (⇧F): edges follow the screen as it changes.
- Floating rulers in px, pt, mm, or in that stay on screen, with a marker
  that follows the pointer (Rulers in the menu, or `pixlsnapz://ruler`).
- Shortcuts actions: Measure, Pick Color, New Ruler, and Show Colors.

### Changed

- The shortcut hints are four rows.

## [0.2.0] - 2026-11-01

### Added

- PixlSnapz speaks German, French, and Spanish, following the system
  language (or the language set for PixlSnapz in System Settings >
  General > Language & Region): the menu, Settings, alerts, messages, the
  shortcut hints, and VoiceOver announcements. Measurements, color codes,
  and anything copied stay as they are.
- VoiceOver: every Settings control has a name; measuring announces kept
  measurements, copies, and mode changes ("Kept 140 by 38"); and the
  measurement under the pointer can be read on request.
- With Increase Contrast or Reduce Transparency on, lines are twice as
  thick and labels are opaque with an outline.
- Homebrew: `brew install --cask houseofmackee/tap/pixlsnapz`, kept up to
  date with each release.

### Changed

- Settings controls grow to fit their text, so longer translations are
  never cut off.

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

[Unreleased]: https://github.com/houseofmackee/pixlsnapz/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/houseofmackee/pixlsnapz/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/houseofmackee/pixlsnapz/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/houseofmackee/pixlsnapz/releases/tag/v0.1.0
