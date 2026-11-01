#!/bin/sh
# Builds everything a GitHub release needs, in dist/:
#
#   PixlSnapz-<version>-arm64.dmg   from scripts/make_dmg.sh (signed, notarized
#                                   when the notary key is given, and checked)
#   PixlSnapz-<version>-arm64.zip   the same PixlSnapz.app, zipped with ditto,
#                                   which keeps the signature intact
#   SHA256SUMS.txt                  checksums of both
#   appcast.xml                     Sparkle's update feed for the zip, signed
#                                   with the EdDSA key in the file
#                                   PIXLSNAPZ_SPARKLE_KEY (only when that is
#                                   set), with the version's CHANGELOG.md
#                                   section as its notes
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

rm -f "dist/$ZIP" dist/SHA256SUMS.txt dist/appcast.xml
ditto -c -k --sequesterRsrc --keepParent "$APP" "dist/$ZIP"
(cd dist && shasum -a 256 "$DMG" "$ZIP" > SHA256SUMS.txt)
cat dist/SHA256SUMS.txt

if [ -n "${PIXLSNAPZ_SPARKLE_KEY:-}" ]; then
    . scripts/lib.sh
    SIGN_UPDATE="$(sparkle_dir)/bin/sign_update"
    BUILD=$(plutil -extract CFBundleVersion raw "$APP/Contents/Info.plist")
    SIGNATURE=$("$SIGN_UPDATE" --ed-key-file "$PIXLSNAPZ_SPARKLE_KEY" -p "dist/$ZIP")
    LENGTH=$(stat -f %z "dist/$ZIP")
    DATE=$(LC_ALL=C date -u "+%a, %d %b %Y %H:%M:%S +0000")
    # PIXLSNAPZ_UPDATE_BASE_URL serves a test update from elsewhere.
    BASE=${PIXLSNAPZ_UPDATE_BASE_URL:-https://github.com/houseofmackee/pixlsnapz/releases/download/v$VERSION}
    # What changed, from CHANGELOG.md, shown in Sparkle's update window.
    NOTES=$(python3 -I scripts/release-notes.py --html "$VERSION" | sed 's/]]>/]]]]><![CDATA[>/g')
    # Sparkle compares sparkle:version with CFBundleVersion (the build number).
    cat > dist/appcast.xml <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>PixlSnapz</title>
    <link>https://github.com/houseofmackee/pixlsnapz</link>
    <item>
      <title>PixlSnapz $VERSION</title>
      <pubDate>$DATE</pubDate>
      <sparkle:version>$BUILD</sparkle:version>
      <sparkle:shortVersionString>$VERSION</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>14.0</sparkle:minimumSystemVersion>
      <sparkle:fullReleaseNotesLink>https://github.com/houseofmackee/pixlsnapz/releases/tag/v$VERSION</sparkle:fullReleaseNotesLink>
      <description><![CDATA[$NOTES]]></description>
      <enclosure url="$BASE/$ZIP" length="$LENGTH" type="application/octet-stream" sparkle:edSignature="$SIGNATURE"/>
    </item>
  </channel>
</rss>
EOF
    # The app requires a signed feed.
    "$SIGN_UPDATE" --ed-key-file "$PIXLSNAPZ_SPARKLE_KEY" dist/appcast.xml
    "$SIGN_UPDATE" --ed-key-file "$PIXLSNAPZ_SPARKLE_KEY" --verify dist/appcast.xml
    echo "Wrote dist/appcast.xml"
fi
