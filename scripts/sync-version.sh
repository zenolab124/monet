#!/bin/bash
# npm "version" lifecycle hook 自动调用：
# 把 package.json 里的版本号同步到 tauri.conf.json 和 Cargo.toml，
# 然后 stage 这两个文件（npm/pnpm version 会自动 commit + tag）。

set -euo pipefail

VERSION="${npm_package_version:?}"
DIR="$(cd "$(dirname "$0")/.." && pwd)"

# tauri.conf.json
sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"${VERSION}\"/" "$DIR/src-tauri/tauri.conf.json"

# Cargo.toml（只改 [package] 区的 version，不动依赖版本）
sed -i '' "0,/^version = \".*\"/s/^version = \".*\"/version = \"${VERSION}\"/" "$DIR/src-tauri/Cargo.toml"

# Cargo.lock 里的版本由 cargo 在下次 build 时自动更新，不需要手动改

git add "$DIR/src-tauri/tauri.conf.json" "$DIR/src-tauri/Cargo.toml"
