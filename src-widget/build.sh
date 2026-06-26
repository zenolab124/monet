#!/bin/bash
# 构建 Widget Extension + widget-updater，嵌入 Tauri .app bundle，签名，打 DMG
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# --- 参数 ---
TEAM_ID="${TEAM_ID:-BKJ3T6HL57}"
SIGN_ID="${SIGN_ID:-Apple Development: 1255996058@qq.com (5P3LX4CJR7)}"
CONFIG="${1:-Release}"
APP_BUNDLE="${2:-../src-tauri/target/release/bundle/macos/CC Space.app}"
XCODE="${DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}"

if [ ! -d "$APP_BUNDLE" ]; then
    echo "Error: App bundle not found: $APP_BUNDLE"
    echo "Run 'pnpm tauri build' first."
    exit 1
fi

# --- 构建 Widget Extension ---
echo "=> Building widget extension..."
DEVELOPER_DIR="$XCODE" xcodegen generate --quiet 2>/dev/null || DEVELOPER_DIR="$XCODE" xcodegen generate
DEVELOPER_DIR="$XCODE" xcodebuild build \
    -project CCSpaceWidget.xcodeproj \
    -target CCSpaceWidgetExtension \
    -configuration "$CONFIG" \
    DEVELOPMENT_TEAM="$TEAM_ID" \
    CODE_SIGN_STYLE=Automatic \
    -allowProvisioningUpdates \
    CONFIGURATION_BUILD_DIR=build/"$CONFIG" \
    -quiet

# --- 构建 widget-updater ---
echo "=> Building widget-updater..."
(cd ../src-tauri && cargo build --release --bin widget-updater 2>&1 | tail -1)

# --- 嵌入 ---
echo "=> Embedding into app bundle..."
PLUGINS_DIR="$APP_BUNDLE/Contents/PlugIns"
mkdir -p "$PLUGINS_DIR"
rm -rf "$PLUGINS_DIR/CCSpaceWidgetExtension.appex"
cp -R "build/$CONFIG/CCSpaceWidgetExtension.appex" "$PLUGINS_DIR/"
cp ../src-tauri/target/release/widget-updater "$APP_BUNDLE/Contents/MacOS/widget-updater"

# --- 签名 ---
echo "=> Signing..."
codesign --force --options runtime --sign "$SIGN_ID" \
    --entitlements CCSpaceWidgetExtension.entitlements \
    "$PLUGINS_DIR/CCSpaceWidgetExtension.appex"
codesign --force --options runtime --sign "$SIGN_ID" "$APP_BUNDLE"
codesign --verify --deep --strict "$APP_BUNDLE"

# --- 打 DMG ---
APP_NAME=$(basename "$APP_BUNDLE" .app)
VERSION=$(plutil -extract CFBundleShortVersionString raw "$APP_BUNDLE/Contents/Info.plist" 2>/dev/null || echo "0.0.0")
DMG_DIR=$(dirname "$APP_BUNDLE")/../dmg
DMG_PATH="$DMG_DIR/${APP_NAME}_${VERSION}_aarch64.dmg"
mkdir -p "$DMG_DIR"
rm -f "$DMG_PATH"

echo "=> Creating DMG..."
hdiutil create -volname "$APP_NAME" -srcfolder "$APP_BUNDLE" -ov -format UDZO "$DMG_PATH" -quiet

echo "=> Done!"
echo "   App: $APP_BUNDLE"
echo "   DMG: $DMG_PATH"
