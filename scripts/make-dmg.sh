#!/bin/sh
# Packs the built (and locally signed) PushTalks.app into a drag-to-install .dmg for sharing
# inside PUSH. Run after `npm run bundle:mac`. Apple Silicon only.
set -e
APP="src-tauri/target/release/bundle/macos/PushTalks.app"
VERSION=$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist")
OUT="src-tauri/target/release/bundle/dmg/PushTalks_${VERSION}_apple-silicon.dmg"
STAGE=$(mktemp -d)
mkdir -p "$(dirname "$OUT")"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$OUT"
hdiutil create -volname "PushTalks" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$OUT" >/dev/null
rm -rf "$STAGE"
hdiutil verify "$OUT" >/dev/null
echo "make-dmg: $OUT ($(du -h "$OUT" | cut -f1))"
