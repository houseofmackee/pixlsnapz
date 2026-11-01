# PixlSnapz

Measure anything on your screen. PixlSnapz is a native macOS screen ruler written in Rust, inspired by [PixelSnap](https://pixelsnap.com): press a shortcut, move the pointer, and see the distances to the nearest edges in every direction. Drag over a button, an icon, or a block of text and the box snaps to its exact bounds.

Everything happens on your Mac: no accounts, no telemetry, and no network access at all.

Apple silicon, macOS 14 Sonoma or later.

## Features

- **Distances**: lines run from the pointer to the nearest edge left, right, up, and down, with the size as `W × H`; the pointer itself is hidden while measuring, so nothing covers the spot the lines cross
- **Area snapping**: drag over something and the box shrinks to its exact bounds; several objects in one drag snap to all of them. Hold ⌘ (or drag with the right mouse button) to measure exactly what you drag
- **Keep measurements**: click to keep a distance on screen, and drag as many areas as you like. New distances stop at kept areas and guides, so you can read the gap between two objects
- **Guides**: press V or H for a vertical or horizontal guide at the pointer; guides snap to nearby edges, show the distances between each other and to the screen edges, and move with the arrow keys
- **Crosshair**: hold ⇧ for lines across the whole screen, to check alignment
- **Loupe**: a pixel grid magnifier next to the pointer, with the hex color of the pixel under it. Press P to copy that color
- **Screenshots**: ⌘S saves the last area as a PNG on the Desktop (⇧⌘C copies it); hold ⌥ for padding around it
- **Freeze screen**: measure a still copy of the screen, so menus and animations stay put
- **Points or pixels**, optional units (`pt`, `px`), and the aspect ratio of areas (`16:9`)
- **Adjustable edge tolerance** for shadows, gradients, and low-contrast edges
- **Restore** the measurements and guides of the previous session (R)
- Multiple displays with different scales, including over full-screen apps
- Six guide colors, launch at login, and a customizable global shortcut

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
| ⇧ while dragging | Square area |
| Space while dragging | Move the area |
| Click | Keep the distances at that point |
| ⇧ (hold) | Crosshair across the whole screen |
| V / H | Add a vertical / horizontal guide (arrow keys then move it) |
| G | Remove all guides |
| Arrow keys | Move the pointer by one pixel (⇧: ten points) |
| ⌘C | Copy the last size, such as `120 × 48` |
| Return | Copy the last size and finish |
| P | Copy the color under the pointer |
| ⌘S / ⇧⌘C | Save / copy a screenshot of the last area (⌥: with padding) |
| Z | Loupe on or off |
| F | Freeze screen on or off |
| U | Points or pixels |
| A | Aspect ratio on or off |
| S | Snapping on or off |
| [ and ] | Lower or raise the edge tolerance |
| ⌫, ⌘Z | Remove the last measurement or guide |
| C | Clear everything |
| R | Restore the previous session |
| ? | Show or hide the shortcut hints |
| Esc, right click | Finish |

From a script or a launcher such as Raycast or Alfred, `PixlSnapz.app/Contents/MacOS/pixlsnapz measure` starts measuring in the running app.

## Settings

The menu bar icon's menu has the common settings. All of them live in `~/Library/Application Support/PixlSnapz/settings.toml` (Open Settings File… in the menu); changes apply the next time you measure or open the menu.

```toml
shortcut = "shift+cmd+1"     # modifiers: cmd, shift, opt, ctrl; keys: a to z, 0 to 9, f1 to f12, punctuation, space
units = "points"             # or "pixels"
tolerance = 8                # 0 to 255; how different a color must be to count as an edge
snap_areas = true
show_hints = true
loupe = false
color = "#FF2D78"            # guide color
show_units = false
show_aspect_ratio = false
freeze = false
screenshot_folder = ""       # empty for the Desktop; "~/Pictures/Measurements" works too
```

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
cargo test                     # edge detection, snapping, guides, settings, and shortcut parsing
cargo clippy --all-targets
```

The edge detection (`src/measure.rs`) works on a plain RGBA buffer and has no AppKit dependency, so it is fully unit tested. The rest is AppKit through `objc2`: `src/capture.rs` takes a screenshot of each display with ScreenCaptureKit, `src/overlay.rs` shows one transparent, non-activating panel per display and draws the measurements, and `src/app.rs` owns the menu bar item and the global shortcut (Carbon `RegisterEventHotKey`, which needs no extra permission).

GitHub Actions runs `cargo fmt --check`, `cargo clippy -D warnings` (debug and release), `cargo test`, a bundle build, and `cargo deny check` on every push to main and every pull request.

### Releasing

Releases are built and published by GitHub Actions. Move the Unreleased notes in [CHANGELOG.md](CHANGELOG.md) into a section for the new version, bump the version in `Cargo.toml` (and `CFBundleVersion` in `macos/Info.plist`), commit, then push a matching tag:

```sh
git tag vX.Y.Z && git push origin main vX.Y.Z
```

`.github/workflows/release.yml` checks the tag matches `Cargo.toml`, signs with the Developer ID from the repository secrets `DEVELOPER_ID_P12_BASE64` and `DEVELOPER_ID_P12_PASSWORD`, notarizes the app and the DMG with the App Store Connect API key (`NOTARY_API_KEY_BASE64` secret; `APPLE_TEAM_ID`, `NOTARY_KEY_ID`, and `NOTARY_ISSUER_ID` variables), refuses to publish anything Gatekeeper would block, and publishes the DMG, zip, and `SHA256SUMS.txt` as a pre-release. Mark it as a regular release once the checks in [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md) are done.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
