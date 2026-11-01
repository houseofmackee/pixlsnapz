# PixlSnapz

Measure anything on your screen. PixlSnapz is a native macOS screen ruler written in Rust, inspired by [PixelSnap](https://pixelsnap.com): press a shortcut, move the pointer, and see the distances to the nearest edges in every direction. Drag over a button, an icon, or a block of text and the box snaps to its exact bounds.

Everything happens on your Mac: no accounts, no telemetry, and no network access at all.

Apple silicon, macOS 14 Sonoma or later.

## Features

- **Distances**: lines run from the pointer to the nearest edge left, right, up, and down, with the size as `W × H`; the pointer itself is hidden while measuring, so nothing covers the spot the lines cross
- **Area snapping**: drag over something and the box shrinks to its exact bounds; several objects in one drag snap to all of them. Hold ⌘ (or drag with the right mouse button) to measure exactly what you drag
- **Keep measurements**: click to keep a distance on screen, and drag as many areas as you like. New distances stop at kept areas and guides, so you can read the gap between two objects
- **Guides**: press V or H for a vertical or horizontal guide at the pointer; guides snap to nearby edges, show the distances between each other and to the screen edges, and move with the arrow keys
- **Line ruler**: ⌥-drag (or press L for the line tool) to measure the length and angle of any line, with a protractor arc; hold ⇧ to snap to 45°
- **Circle tool**: press E and drag from the center to measure radius and diameter
- **Jump to the next edge**: ⌥ plus an arrow key moves the pointer just past the next edge, object by object
- **One direction at a time**: press X to measure only horizontally or only vertically
- **Crosshair**: hold ⇧ for lines across the whole screen, to check alignment
- **Loupe**: a pixel grid magnifier next to the pointer, with the color and coordinates of the pixel under it; zoom from 4× to 32× with - and +
- **Colors**: P copies the color under the pointer and ⌥P the average color of the last area, as hex, `rgb()`, `hsl()`, SwiftUI, or AppKit code (⇧P switches); the last 20 are under Recent Colors in the menu bar icon's menu
- **Color vision**: ⇧Z shows the loupe as people with protanopia, deuteranopia, tritanopia, or achromatopsia see it
- **Screenshots**: ⌘S saves the last area as a PNG on the Desktop (⇧⌘C copies it), with the measurements drawn in (M turns that off); hold ⌥ for padding around it
- **Freeze screen**: measure a still copy of the screen, so menus and animations stay put
- **Contrast checker**: press K on one color and K again on another to copy the WCAG contrast ratio and grade, such as `4.54:1  AA`; a live readout with both swatches follows the pointer in between
- **Units**: points, pixels, millimeters, or inches (real sizes from what each display reports, or a PPI you set), optional unit labels, and the aspect ratio of areas (`16:9`)
- **Design zoom**: measure a design shown at 200% (or 25% to 400%) in its own units
- **Grids**: press O for a square grid or a column layout grid (columns, gutter, and margin are adjustable), over the whole screen or inside the last area
- **Keep on screen**: press B to stop measuring but leave the measurements up while you keep working; clicks go through them, and the next measuring session picks them up again
- **Settings window** with every option in tabs, and a shortcut recorder
- **Adjustable edge tolerance** for shadows, gradients, and low-contrast edges: scroll, or press [ and ]
- **Restore** the measurements and guides of the previous session (R), even after a restart
- Multiple displays with different scales, including over full-screen apps
- Six guide colors, launch at login, and a customizable global shortcut that can be turned off in chosen apps (such as a design tool that uses it), from the menu bar icon's menu or Settings

## Install

Download `PixlSnapz-<version>-arm64.dmg` from the [releases page](https://github.com/houseofmackee/pixlsnapz/releases/latest), open it, and drag PixlSnapz into Applications. Check it against `SHA256SUMS.txt` from the same release if you like (`shasum -a 256 -c --ignore-missing SHA256SUMS.txt`).

PixlSnapz lives in the menu bar. The first time you measure, macOS asks to allow **Screen Recording**: PixlSnapz looks at the pixels on screen to find edges. Turn it on in System Settings > Privacy & Security > Screen & System Audio Recording, then quit and reopen PixlSnapz. Nothing is saved or sent anywhere, except screenshots you save yourself.

## Usage

Press **⇧⌘1** (or choose Measure from the menu bar icon), measure, and press **Esc** when done.

| Key | Action |
| --- | --- |
| Move | Distances to the nearest edges |
| Drag | Measure an area, snapped to its content |
| ⌘ drag, right-drag | Measure an area without snapping |
| ⌥ drag | Measure a line: length and angle |
| L | Line tool on or off (drag draws lines) |
| E | Circle tool on or off (drag from the center) |
| ⇧ while dragging | Square area, or a line at a multiple of 45° |
| Space while dragging | Move the area |
| Click | Keep the distances at that point |
| ⇧ (hold) | Crosshair across the whole screen |
| V / H | Add a vertical / horizontal guide (arrow keys then move it) |
| G | Remove all guides |
| Arrow keys | Move the pointer by one pixel (⇧: ten points) |
| ⌥ arrow keys | Jump to the next edge |
| X | Measure both directions, horizontal only, or vertical only |
| ⌘C | Copy the last size, such as `120 × 48` |
| Return | Copy the last size and finish |
| P | Copy the color under the pointer |
| ⌥P | Copy the average color of the last area |
| ⇧P | Switch the color format: hex, RGB, HSL, SwiftUI, AppKit |
| ⌘S / ⇧⌘C | Save / copy a screenshot of the last area (⌥: with padding) |
| M | Measurements in screenshots on or off |
| Z | Loupe on or off |
| - and + | Loupe zoom out and in |
| ⇧Z | Loupe color vision: normal, protanopia, deuteranopia, tritanopia, achromatopsia |
| F | Freeze screen on or off |
| U | Points, pixels, millimeters, inches |
| A | Aspect ratio on or off |
| K | Contrast: pick the first color, then compare and copy |
| O | Grid: off, square, columns |
| B | Keep the measurements on screen and stop measuring |
| S | Snapping on or off |
| Scroll, [ and ] | Lower or raise the edge tolerance |
| ⌫, ⌘Z | Remove the last measurement or guide |
| C | Clear everything |
| R | Restore the previous session, also after a restart |
| ? | Show or hide the shortcut hints |
| Esc, right click | Finish |

From a script or a launcher such as Raycast or Alfred, `PixlSnapz.app/Contents/MacOS/pixlsnapz measure` starts measuring in the running app, and `pixlsnapz settings` opens Settings.

Measurements kept on screen with B stay until you measure again (they come back for editing) or choose Clear Kept Measurements in the menu bar icon's menu.

## Settings

Choose **Settings…** (⌘,) in the menu bar icon's menu for every option, in five tabs (General, Measuring, Colors & Loupe, Grid, Screenshots), including recording a new shortcut; the menu itself has the common ones. Everything is saved in `~/Library/Application Support/PixlSnapz/settings.toml` (Open Settings File in the Settings window), and changes made there by hand apply the next time you measure or open the menu.

```toml
shortcut = "shift+cmd+1"     # modifiers: cmd, shift, opt, ctrl; keys: a to z, 0 to 9, f1 to f12, punctuation, space
units = "points"             # or "pixels", "millimeters", "inches"
tolerance = 8                # 0 to 255; how different a color must be to count as an edge
snap_areas = true
show_hints = true
loupe = false
color = "#FF2D78"            # guide color
show_units = false
show_aspect_ratio = false
freeze = false
screenshot_folder = ""       # empty for the Desktop; "~/Pictures/Measurements" works too
axes = "both"                # or "horizontal", "vertical"
color_format = "hex"         # or "rgb", "hsl", "swiftui", "appkit"
loupe_zoom = 8               # 4, 6, 8, 12, 16, 24, or 32
design_zoom = 100            # percent; lengths are divided by it
screen_ppi = 0.0             # 0 uses the size each display reports
grid = "off"                 # or "square", "columns"
grid_size = 8.0              # points
grid_columns = 12
grid_gutter = 20.0           # points
grid_margin = 0.0            # points
stamp_screenshots = true     # draw the measurements into screenshots
vision = "normal"            # or "protanopia", "deuteranopia", "tritanopia", "achromatopsia"
disabled_apps = []           # bundle identifiers, such as ["com.figma.Desktop"]
```

The last session's measurements and guides are kept in `last-session.toml` next to the settings, for R, and recent colors in `colors.toml`.

## Build and run

Requires an Apple silicon Mac with macOS 14 or later and a Rust toolchain.

```sh
cargo run --release            # run from the terminal (Screen Recording then applies to your terminal)
scripts/bundle.sh              # build and sign dist/PixlSnapz.app (Developer ID if present; see scripts/lib.sh)
scripts/make_dmg.sh            # build dist/PixlSnapz-<version>-arm64.dmg and its .sha256, then mount and check it
scripts/dist.sh                # make_dmg.sh, plus a zip and SHA256SUMS.txt: everything a release uploads
scripts/make_icon.sh           # rebuild macos/AppIcon.icns after editing macos/AppIcon.svg (needs librsvg)
open dist/PixlSnapz.app
```

Environment variables for testing: `PIXLSNAPZ_HOME` uses a different settings folder, and `PIXLSNAPZ_LOG=pixlsnapz=debug` enables verbose logging.

## Development

```sh
cargo test                     # edge detection, snapping, edge jumps, lines, units, grids, colors, contrast, saved sessions, settings, and shortcuts
cargo clippy --all-targets
```

The edge detection (`src/measure.rs`) works on a plain RGBA buffer and has no AppKit dependency, so it is fully unit tested. The rest is AppKit through `objc2`: `src/capture.rs` takes a screenshot of each display with ScreenCaptureKit, `src/overlay/` shows one transparent, non-activating panel per display: `view.rs` (window, view, and mouse input), `session.rs` (session state and logic), `keys.rs` (keyboard commands), `actions.rs` (copying and screenshots), and `draw.rs` (drawing), `src/settings_window.rs` is the Settings window, and `src/app.rs` owns the menu bar item and the global shortcut (Carbon `RegisterEventHotKey`, which needs no extra permission).

GitHub Actions runs `cargo fmt --check`, `cargo clippy -D warnings` (debug and release), `cargo test`, a bundle build, and `cargo deny check` on every push to main and every pull request.

### Releasing

Releases are built and published by GitHub Actions. Move the Unreleased notes in [CHANGELOG.md](CHANGELOG.md) into a section for the new version, bump the version in `Cargo.toml` (and `CFBundleVersion` in `macos/Info.plist`), commit, then push a matching tag:

```sh
git tag vX.Y.Z && git push origin main vX.Y.Z
```

`.github/workflows/release.yml` checks the tag matches `Cargo.toml`, signs with the Developer ID from the repository secrets `DEVELOPER_ID_P12_BASE64` and `DEVELOPER_ID_P12_PASSWORD`, notarizes the app and the DMG with the App Store Connect API key (`NOTARY_API_KEY_BASE64` secret; `APPLE_TEAM_ID`, `NOTARY_KEY_ID`, and `NOTARY_ISSUER_ID` variables), refuses to publish anything Gatekeeper would block, and publishes the DMG, zip, and `SHA256SUMS.txt` as a pre-release. Mark it as a regular release once the checks in [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md) are done.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
