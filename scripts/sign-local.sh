#!/bin/sh
# Re-signs the built macOS app with a local certificate so macOS permissions survive rebuilds.
# Ad-hoc signatures change with every build, and macOS then treats each build as a new app.
# Needs rcodesign (`cargo install apple-codesign`) and ~/.pushtalks-signing/signing.pem
# (a self-signed code-signing key + certificate). Skips quietly if either is missing.
# Not for distribution: a PUSH Apple Developer ID replaces this (PRD §12).
set -e
APP="src-tauri/target/release/bundle/macos/PushTalks.app"
PEM="$HOME/.pushtalks-signing/signing.pem"
if [ ! -f "$PEM" ] || ! command -v rcodesign >/dev/null 2>&1; then
  echo "sign-local: no local certificate or rcodesign; leaving the ad-hoc signature"
  exit 0
fi
rcodesign sign --pem-file "$PEM" --code-signature-flags runtime \
  --entitlements-xml-file src-tauri/Entitlements.plist "$APP" >/dev/null
codesign --verify --deep --strict "$APP"
echo "sign-local: signed $APP with the local certificate"
