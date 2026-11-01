#!/bin/sh
# Builds everything a GitHub release needs, in dist/:
#
#   PixlSnapz-<version>-arm64.dmg   from scripts/make_dmg.sh (signed, notarized
#                                   when the notary key is given, and checked)
#   PixlSnapz-<version>-arm64.zip   the same PixlSnapz.app, zipped with ditto,
#                                   which keeps the signature intact
#   SHA256SUMS.txt                  checksums of both
#
# The release workflow (.github/workflows/release.yml) runs this with the
# Developer ID imported and the notary key set. Run locally, it signs the
# same way as scripts/bundle.sh (see scripts/lib.sh).
set -eu
cd "$(dirname "$0")/.."

scripts/make_dmg.sh
APP=dist/PixlSnapz.app
VERSION=$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist")
DMG="PixlSnapz-$VERSION-arm64.dmg"
ZIP="PixlSnapz-$VERSION-arm64.zip"

rm -f "dist/$ZIP" dist/SHA256SUMS.txt
ditto -c -k --sequesterRsrc --keepParent "$APP" "dist/$ZIP"
(cd dist && shasum -a 256 "$DMG" "$ZIP" > SHA256SUMS.txt)
cat dist/SHA256SUMS.txt
