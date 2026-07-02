#!/bin/bash
# Build mx-gestures as a signed .app bundle and install it to /Applications.
# A stable signed identity means macOS TCC permissions (Accessibility) survive
# rebuilds — re-run this script after code changes.
set -euo pipefail

cd "$(dirname "$0")"
APP="/Applications/MX Gestures.app"
# Signing: use MXG_SIGN_IDENTITY if set, else the first Apple codesigning
# identity in the keychain, else ad-hoc ("-"). A real identity keeps macOS
# permissions (Accessibility) across rebuilds; ad-hoc works but re-prompts.
IDS="$(security find-identity -v -p codesigning 2>/dev/null)"
IDENTITY="${MXG_SIGN_IDENTITY:-$(echo "$IDS" | awk -F'"' '/Developer ID Application/ {print $2; exit}')}"
IDENTITY="${IDENTITY:-$(echo "$IDS" | awk -F'"' '/Apple Development/ {print $2; exit}')}"
IDENTITY="${IDENTITY:--}"
echo "signing identity: $IDENTITY"

cargo build --release

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/mx-gestures "$APP/Contents/MacOS/"

# App icon (same glyph as the menu-bar icon), regenerated only when missing
if [ ! -f AppIcon.icns ]; then
    python3 gen-icon.py
    rm -rf AppIcon.iconset && mkdir AppIcon.iconset
    for sz in 16 32 128 256 512; do
        sips -z $sz $sz icon_1024.png --out "AppIcon.iconset/icon_${sz}x${sz}.png" >/dev/null
        sips -z $((sz*2)) $((sz*2)) icon_1024.png --out "AppIcon.iconset/icon_${sz}x${sz}@2x.png" >/dev/null
    done
    iconutil -c icns AppIcon.iconset
    rm -rf AppIcon.iconset icon_1024.png
fi
cp AppIcon.icns "$APP/Contents/Resources/"
cat > "$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>lt.ovi.mx-gestures</string>
    <key>CFBundleName</key><string>MX Gestures</string>
    <key>CFBundleDisplayName</key><string>MX Gestures</string>
    <key>CFBundleExecutable</key><string>mx-gestures</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.2.1</string>
    <key>CFBundleVersion</key><string>0.2.1</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHumanReadableCopyright</key><string>MIT</string>
</dict>
</plist>
EOF

codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
codesign --verify --deep "$APP" && echo "signed OK: $APP"

# ./make-app.sh --notarize  — upload to Apple's notary service and staple.
# One-time setup: xcrun notarytool store-credentials mx-gestures \
#   --apple-id <you> --team-id <team> --password <app-specific password>
if [ "${1:-}" = "--notarize" ]; then
    ZIP="$(mktemp -d)/MX.Gestures.zip"
    ditto -c -k --keepParent "$APP" "$ZIP"
    xcrun notarytool submit "$ZIP" --keychain-profile mx-gestures --wait
    xcrun stapler staple "$APP"
    spctl -a -vv "$APP" && echo "notarized + stapled OK"
fi
