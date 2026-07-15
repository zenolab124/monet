#!/bin/bash
# 把 monet-tray 二进制包装为独立 Helper App，放到主应用的 LoginItems 下。
# macOS 把 Contents/Library/LoginItems/ 下的 .app 视为完全独立进程。

set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP="$DIR/src-tauri/target/release/bundle/macos/Monet.app"

if [ ! -d "$APP" ]; then
  echo "error: Monet.app not found at $APP" >&2
  exit 1
fi

TRAY_BIN="$APP/Contents/MacOS/monet-tray"
if [ ! -f "$TRAY_BIN" ]; then
  echo "error: monet-tray binary not found" >&2
  exit 1
fi

# 构建 Helper App bundle
HELPER="$APP/Contents/Library/LoginItems/MonetTray.app"
mkdir -p "$HELPER/Contents/MacOS"

# 移动二进制（不再留在主 MacOS 目录里）
mv "$TRAY_BIN" "$HELPER/Contents/MacOS/monet-tray"

# 复制 Info.plist
cp "$DIR/src-tray/Info.plist" "$HELPER/Contents/Info.plist"

echo "✓ MonetTray.app bundled at $HELPER"
