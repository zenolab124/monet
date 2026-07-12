#!/bin/bash
# 构建 Widget Extension + widget-updater，嵌入 Tauri .app bundle，签名，打 DMG
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# --- 参数 ---
# 签名身份默认本机自签长效证书（scripts/setup-signing.sh 创建）：
# TCC 授权钉在 identifier+证书 的 designated requirement 上，重新构建不丢权限
SIGN_ID="${SIGN_ID:-Monet Signing}"
SIGNING_KEYCHAIN="$HOME/Library/Keychains/monet-signing.keychain-db"
SIGNING_PASS_FILE="$HOME/.monet/signing/keychain-password"
CONFIG="${1:-Release}"
APP_BUNDLE="${2:-../src-tauri/target/release/bundle/macos/Monet.app}"
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
# 自签证书钥匙串可能处于锁定状态，签名前解锁
if [ -f "$SIGNING_PASS_FILE" ] && [ -f "$SIGNING_KEYCHAIN" ]; then
    security unlock-keychain -p "$(cat "$SIGNING_PASS_FILE")" "$SIGNING_KEYCHAIN"
fi
codesign --force --options runtime --sign "$SIGN_ID" \
    --entitlements MonetWidgetExtension.entitlements \
    "$PLUGINS_DIR/MonetWidgetExtension.appex"
# 辅助二进制单独签固定 identifier：被安装到 ~/.monet/bin 后 DR 依旧稳定
for BIN in "$APP_BUNDLE/Contents/MacOS/"*; do
    NAME=$(basename "$BIN")
    [ "$NAME" = "app" ] && continue
    codesign --force --options runtime --sign "$SIGN_ID" \
        --identifier "io.github.zenolab124.monet.$NAME" "$BIN"
done
codesign --force --options runtime --sign "$SIGN_ID" \
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
hdiutil create -volname "$APP_NAME" -srcfolder "$APP_BUNDLE" -ov -format UDZO "$DMG_PATH" -quiet

echo "=> Done!"
echo "   App: $APP_BUNDLE"
echo "   DMG: $DMG_PATH"
