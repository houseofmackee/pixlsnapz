# Changelog

All notable changes to PixlSnapz. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). The section for a version is what
the GitHub release shows.

## [Unreleased]

## [0.3.2] - 2026-11-04

### Added

- Floating rulers turn to any angle: drag the far end with ⌥ (⇧ snaps to
  15°), press ⌥ and an arrow key to turn by a degree (⇧: 15°), or choose
  Straighten in a ruler's menu. Clicks beside a turned ruler go through to
  what is underneath.
- ⌃-drag from one window, button, or field to another keeps a box around
  both (⌃-click still keeps just the one under the pointer).
- VoiceOver reads a ruler's length, angle, the pointer's position along it,
  and its keys; the Colors window reads each color's codes and announces
  copies and contrast.

### Changed

- A ruler running down the screen now has its ticks on its right edge and
  its numbers turned with it, like a turned physical ruler.

### Fixed

- A right-click while macOS still reports ⌃ held (as just after a ⌃
  shortcut) measures or ends measuring, instead of keeping a window.

## [0.3.1] - 2026-11-01

### Added

- Palettes can be edited in the Colors window: right-click a color to add
  it to a palette (or a new one), or to remove it or move it up or down
  within a palette; Rename… renames the palette shown.
- Rulers come back where they were after a restart, and a clicked ruler
  takes the arrow keys (⇧: ten points), R to rotate, and ⌫ to close.
- The Shortcuts actions are in German, French, and Spanish too.

### Changed

- The name of a set (⇧S) is typed in a real text field: paste, input
  methods, and the usual editing keys work.

## [0.3.0] - 2026-11-01

### Added

Measuring:

- Hold ⌥ over a kept box to see its spacing to the nearest box, guide, or
  screen edge on every side, with equal gaps marked "=".
- Smart guides: while drawing or editing a box, dashed lines show where it
  lines up with other kept boxes, and "=" marks boxes of the same width or
  height.
- Kept boxes, lines, circles, guides, and points can be edited: hover one
  to see its handles, drag a handle to resize or the outline to move.
  ⌫ removes the item under the pointer.
- Hold ⌃ over a window to see its frame; ⌃-click keeps it as a box. With
  Snap to Buttons and Fields on (Settings, Accessibility permission), ⌃
  picks out the button, field, or image under the pointer instead.
- Protractor: a line started or ended at the end of a kept line shows the
  angle between the two, and ⌘C copies it.
- Live mode (⇧F): edges follow the screen as it changes (animations,
  video, scrolling).
- Measurement sets: ⇧S saves what is on screen under a name, ⇧R cycles
  through saved sets, and Measurement Sets in the menu starts measuring
  with one or deletes it.
- Hold to measure: hold a chosen pair of modifier keys to see distances,
  and let go to stop (off by default; in Settings).
- A design overlay: lay a design image over the real app at 25% to 100%
  opacity, or as a difference (I, ⇧I, ⌘ arrows, ⌘I; Design Overlay in
  the menu).
- T switches what ⌘C copies for sizes: plain, CSS, Tailwind, SwiftUI, or
  CGSize (also in Settings and the menu).

Colors:

- A color picker shortcut (⇧⌘2, or Pick Color in the menu): just the
  loupe, and a click copies the color.
- A Colors window (Colors… in the menu): recent colors with their codes,
  click to copy, the contrast between the last two clicked, palettes, and
  export as CSS custom properties, JSON, or Adobe swatches (ASE).

Rulers and other apps:

- Floating rulers in px, pt, mm, or in that stay on screen while you work,
  with a marker that follows the pointer (Rulers in the menu).
- Shortcuts actions: Measure (optionally with a saved set), Pick Color,
  New Ruler, and Show Colors.
- `pixlsnapz://` links start measuring (optionally with a saved set), pick
  a color, open a ruler, the Colors window, or a Settings tab.
- ⌘E opens the last area, with its measurements, in CleanShotZ to
  annotate.

### Changed

- The shortcut hints are four rows.
- While ⌥ shows a box's spacing, the other boxes show just their outlines,
  so the gap labels stay readable.

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

[Unreleased]: https://github.com/houseofmackee/pixlsnapz/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/houseofmackee/pixlsnapz/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/houseofmackee/pixlsnapz/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/houseofmackee/pixlsnapz/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/houseofmackee/pixlsnapz/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/houseofmackee/pixlsnapz/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/houseofmackee/pixlsnapz/releases/tag/v0.1.0
