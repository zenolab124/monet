#!/bin/bash
# npm "version" lifecycle hook 自动调用：
# 把 package.json 里的版本号同步到 tauri.conf.json 和 Cargo.toml，
# 然后 stage 这两个文件（npm/pnpm version 会自动 commit + tag）。

set -euo pipefail

VERSION="${npm_package_version:?}"
DIR="$(cd "$(dirname "$0")/.." && pwd)"

# tauri.conf.json
sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"${VERSION}\"/" "$DIR/src-tauri/tauri.conf.json"

# Cargo.toml（只改首个 version = 即 [package] 区的，不动依赖版本）。
# 不用 sed "0,/re/"——那是 GNU 语法，BSD sed 静默不作为且退出码 0，
# 曾让 Cargo.toml 版本在 0.1.0 潜伏 16 个版本（tray plist 重装判定因此失效）
python3 - "$DIR/src-tauri/Cargo.toml" "$VERSION" <<'PYEOF'
import re, sys
path, version = sys.argv[1], sys.argv[2]
s = open(path).read()
s = re.sub(r'^version = ".*?"', f'version = "{version}"', s, count=1, flags=re.M)
open(path, 'w').write(s)
PYEOF

# 防呆校验：同步失败必须炸在当场，不许再静默潜伏
grep -q "^version = \"${VERSION}\"" "$DIR/src-tauri/Cargo.toml" \
  || { echo "✗ Cargo.toml 版本同步失败（期望 ${VERSION}）" >&2; exit 1; }
grep -q "\"version\": \"${VERSION}\"" "$DIR/src-tauri/tauri.conf.json" \
  || { echo "✗ tauri.conf.json 版本同步失败（期望 ${VERSION}）" >&2; exit 1; }

# Cargo.lock 的 app 版本随 Cargo.toml 联动，避免下次构建产生漂移的未提交改动
(cd "$DIR/src-tauri" && cargo update --workspace --quiet 2>/dev/null) || true

git add "$DIR/src-tauri/tauri.conf.json" "$DIR/src-tauri/Cargo.toml" "$DIR/src-tauri/Cargo.lock"
