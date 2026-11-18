#!/usr/bin/env bash
# Build the real (egui) HebnixMac into build/HebnixMac.app.
#
# The old build/HebnixMac.app is the SwiftUI test harness from the first
# phase; this script builds the actual Hebnix UI from hebnix_rs.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/build/HebnixMac.app"
BIN="${1:-release}"

echo "== hebnix-app ($BIN) =="
cd "$ROOT/hebnix_rs"
if [ "$BIN" = "release" ]; then
    cargo build -p hebnix-app --release
    SRC="$ROOT/hebnix_rs/target/release/hebnix"
else
    cargo build -p hebnix-app
    SRC="$ROOT/hebnix_rs/target/debug/hebnix"
fi

echo "== bundle =="
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/macos/Hebnix/Info.plist" "$APP/Contents/Info.plist"
cp "$SRC" "$APP/Contents/MacOS/hebnixmac"
chmod +x "$APP/Contents/MacOS/hebnixmac"

# Locales live next to the binary's data dir, but ship a copy so a fresh
# machine has them before first run creates the support folder.
if [ -d "$ROOT/hebnix_rs/crates/hebnix-app/locales" ]; then
    cp -R "$ROOT/hebnix_rs/crates/hebnix-app/locales" "$APP/Contents/Resources/locales"
fi

if [ -f "$ROOT/macos/Hebnix/AppIcon.icns" ]; then
    cp "$ROOT/macos/Hebnix/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
fi

plutil -lint "$APP/Contents/Info.plist" >/dev/null

# Ad-hoc signature over the whole bundle: enough for local use. Re-signing
# from scratch (instead of patching an existing signature) is what stops
# macOS from calling the app "outdated"/damaged.
codesign --force --deep --sign - --timestamp=none "$APP" >/dev/null
codesign --verify --deep --strict "$APP"
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

echo
echo "built: $APP"
echo "run:   open \"$APP\""
