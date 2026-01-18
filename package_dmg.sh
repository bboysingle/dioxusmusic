#!/bin/bash
set -e

APP_NAME="DioxusMusic"
EXECUTABLE_NAME="dioxusmusic"
VERSION="0.1.0"
DMG_NAME="${APP_NAME}_${VERSION}.dmg"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Building release binary..."
cargo build --release

echo "Creating App Bundle..."
BUNDLE_DIR="target/release/bundle/osx/${APP_NAME}.app"
mkdir -p "${BUNDLE_DIR}/Contents/MacOS"
mkdir -p "${BUNDLE_DIR}/Contents/Resources"

# Copy binary
cp "target/release/${EXECUTABLE_NAME}" "${BUNDLE_DIR}/Contents/MacOS/${APP_NAME}"
chmod +x "${BUNDLE_DIR}/Contents/MacOS/${APP_NAME}"

# Copy icon
if [ -f "${SCRIPT_DIR}/assets/DioxusMusic.icns" ]; then
    cp "${SCRIPT_DIR}/assets/DioxusMusic.icns" "${BUNDLE_DIR}/Contents/Resources/"
    echo "Copied app icon"
fi

# Copy resources
if [ -d "${SCRIPT_DIR}/assets" ]; then
    for item in "${SCRIPT_DIR}/assets"/*; do
        if [ "$(basename "$item")" != "DioxusMusic.icns" ]; then
            cp -r "$item" "${BUNDLE_DIR}/Contents/Resources/" 2>/dev/null || true
        fi
    done
fi

# Create Info.plist with icon reference
cat > "${BUNDLE_DIR}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.dioxusmusic.app</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIconFile</key>
    <string>DioxusMusic</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

echo "Creating DMG..."
rm -f "${DMG_NAME}"
hdiutil create -volname "${APP_NAME}" -srcfolder "${BUNDLE_DIR}" -ov -format UDZO "${DMG_NAME}"

echo "Done! DMG created at ${DMG_NAME}"
