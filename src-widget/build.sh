#!/bin/bash
# 构建 Widget Extension + widget-updater，嵌入 Tauri .app bundle，签名，打 DMG
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# --- 参数 ---
SIGN_ID="${SIGN_ID:-Monet Signing}"
SIGNING_KEYCHAIN="$HOME/Library/Keychains/monet-signing.keychain-db"
SIGNING_PASS_FILE="$HOME/.monet/signing/keychain-password"
CONFIG="${1:-Release}"
_RAW_BUNDLE="${2:-../src-tauri/target/release/bundle/macos/Monet.app}"
APP_BUNDLE="$(cd "$(dirname "$_RAW_BUNDLE")" && pwd)/$(basename "$_RAW_BUNDLE")"
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
    -project MonetWidget.xcodeproj \
    -target MonetWidgetExtension \
    -configuration "$CONFIG" \
    CODE_SIGNING_ALLOWED=NO \
    CONFIGURATION_BUILD_DIR=build/"$CONFIG" \
    -quiet

# --- 构建 widget-updater ---
echo "=> Building widget-updater..."
(cd ../src-tauri && cargo build --release --bin widget-updater 2>&1 | tail -1)

# --- 嵌入 ---
echo "=> Embedding into app bundle..."
PLUGINS_DIR="$APP_BUNDLE/Contents/PlugIns"
mkdir -p "$PLUGINS_DIR"
rm -rf "$PLUGINS_DIR/MonetWidgetExtension.appex"
cp -R "build/$CONFIG/MonetWidgetExtension.appex" "$PLUGINS_DIR/"
cp ../src-tauri/target/release/widget-updater "$APP_BUNDLE/Contents/MacOS/widget-updater"

# --- 签名 ---
echo "=> Signing..."
if security find-identity -v -p codesigning | grep -q "$SIGN_ID"; then
    if [ -f "$SIGNING_PASS_FILE" ] && [ -f "$SIGNING_KEYCHAIN" ]; then
        security unlock-keychain -p "$(cat "$SIGNING_PASS_FILE")" "$SIGNING_KEYCHAIN"
    fi
    CODESIGN_ARGS=(--force --options runtime --sign "$SIGN_ID")
else
    echo "   identity '$SIGN_ID' not found, falling back to adhoc signing"
    echo "   (run scripts/setup-signing.sh to create a stable signing identity)"
    CODESIGN_ARGS=(--force --sign -)
fi

codesign "${CODESIGN_ARGS[@]}" \
    --entitlements MonetWidgetExtension.entitlements \
    "$PLUGINS_DIR/MonetWidgetExtension.appex"
for BIN in "$APP_BUNDLE/Contents/MacOS/"*; do
    NAME=$(basename "$BIN")
    [ "$NAME" = "app" ] && continue
    codesign "${CODESIGN_ARGS[@]}" \
        --identifier "io.github.zenolab124.monet.$NAME" "$BIN"
done
# Helper App（独立 menubar 进程）：嵌套 bundle 必须先签内层再签外层
TRAY_APP="$APP_BUNDLE/Contents/Library/LoginItems/MonetTray.app"
if [ -d "$TRAY_APP" ]; then
    codesign "${CODESIGN_ARGS[@]}" \
        --identifier "io.github.zenolab124.monet.tray" "$TRAY_APP"
fi
codesign "${CODESIGN_ARGS[@]}" \
    --entitlements ../src-tauri/Monet.entitlements "$APP_BUNDLE"
codesign --verify --deep --strict "$APP_BUNDLE"

# --- 打 DMG ---
APP_NAME=$(basename "$APP_BUNDLE" .app)
VERSION=$(plutil -extract CFBundleShortVersionString raw "$APP_BUNDLE/Contents/Info.plist" 2>/dev/null || echo "0.0.0")
DMG_DIR=$(dirname "$APP_BUNDLE")/../dmg
DMG_PATH="$DMG_DIR/${APP_NAME}_${VERSION}_aarch64.dmg"
mkdir -p "$DMG_DIR"
rm -f "$DMG_PATH"

echo "=> Creating DMG..."
DMG_STAGE=$(mktemp -d)
cp -R "$APP_BUNDLE" "$DMG_STAGE/"
ln -s /Applications "$DMG_STAGE/Applications"
if diskutil image create from --help &>/dev/null; then
    diskutil image create from --format UDZO --volumeName "$APP_NAME" "$DMG_STAGE" "$DMG_PATH"
else
    hdiutil create -volname "$APP_NAME" -srcfolder "$DMG_STAGE" -ov -format UDZO "$DMG_PATH" -quiet
fi
rm -rf "$DMG_STAGE"

echo "=> Done!"
echo "   App: $APP_BUNDLE"
echo "   DMG: $DMG_PATH"
