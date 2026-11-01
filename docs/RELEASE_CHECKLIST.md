# Release checklist

Work through this for every release, on an Apple silicon Mac.

## 1. Automated checks

- [ ] `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`, and `python3 -I scripts/strings.py --check` are clean.
- [ ] README and screenshots are current (`scripts/readme_screenshots.sh` after visible changes).
- [ ] CI is green on main (it also runs `cargo deny check`).
- [ ] `CHANGELOG.md`: the Unreleased notes moved into a new `## [X.Y.Z] - YYYY-MM-DD` section, plus its compare link at the bottom.
- [ ] Version bumped in `Cargo.toml` and `CFBundleVersion` in `macos/Info.plist`.
- [ ] Push the tag; the Release workflow passes and the release has the DMG, zip, `SHA256SUMS.txt`, and `appcast.xml`.
- [ ] `Casks/pixlsnapz.rb` in houseofmackee/homebrew-tap has the new version and the DMG's checksum.
- [ ] The release is marked as regular (`gh release edit vX.Y.Z --prerelease=false --latest`), and https://github.com/houseofmackee/pixlsnapz/releases/latest/download/appcast.xml offers the new version.
- [ ] `spctl -a -vv /Applications/PixlSnapz.app` says "accepted, source=Notarized Developer ID", and `codesign -dr - /Applications/PixlSnapz.app` shows `certificate leaf[subject.OU] = ZD5VPNGP3T`.

## 2. Install

- [ ] The DMG opens and shows PixlSnapz, the Applications link, and "Read Me First.txt".
- [ ] After dragging to Applications, PixlSnapz opens without a Gatekeeper warning and shows the welcome message once.
- [ ] Measuring the first time asks for Screen Recording; after allowing it and reopening, measuring works.
- [ ] Upgrading over the previous release keeps the Screen Recording permission and the settings.
- [ ] Launch at login works after a restart.
- [ ] After marking the release as regular, the previous version offers it with Check for Updates…, shows the changelog notes, installs it, and keeps the Screen Recording permission.
- [ ] `brew install --cask houseofmackee/tap/pixlsnapz` installs the new version.

## 3. Measuring

- [ ] The shortcut and the menu's Measure item both start measuring, with keyboard focus in the overlay (press U and see the toast) while the previous app stays frontmost.
- [ ] Distances in a window, between two windows, and to the screen edge are right on a Retina and a non-Retina display.
- [ ] Dragging over a button snaps to its bounds; ⌘ drag and right-drag do not snap; ⇧ makes a square; Space moves the area.
- [ ] Guides (V, H) snap to edges, move with the arrow keys, and show distances.
- [ ] ⌘C, Return, P, ⌘S, and ⇧⌘C copy or save the right thing.
- [ ] Freeze, loupe, units, aspect ratio, tolerance, restore, and clear work, and the toggles persist.
- [ ] Works over a full-screen app, in another Space, and with two displays; Esc, right click, and ⌘Tab end measuring.
- [ ] ⌃ highlights a window (and, with Snap to Buttons and Fields and Accessibility allowed, a button); ⇧S and ⇧R save and load a set; ⇧F follows a scrolling page.
- [ ] ⇧⌘2 picks a color; the Colors window copies, compares, saves a palette, and exports; rulers move, resize, and rotate.
- [ ] Hold to measure starts and stops with the chosen keys, and the Shortcuts actions (Measure, Pick Color, New Ruler, Show Colors) run from the Shortcuts app.

## 4. Languages and accessibility

- [ ] With PixlSnapz set to German, French, or Spanish (System Settings > General > Language & Region > Applications), the menu, Settings, the hints, and messages are translated, and nothing is cut off.
- [ ] VoiceOver reads the Settings controls by name and announces kept measurements while measuring.
- [ ] With Increase Contrast on, lines are thicker and labels outlined.
