#!/bin/bash
# 发版：提交未暂存改动 → 升版本号 → 打包 + Widget
# 用法：
#   pnpm release            → patch (0.1.0 → 0.1.1)
#   pnpm release -- minor   → minor (0.1.0 → 0.2.0)
#   pnpm release -- major   → major (0.1.0 → 1.0.0)

set -euo pipefail

BUMP=${1:-patch}

# 有未提交改动则先提交
if ! git diff --quiet || ! git diff --cached --quiet || [ -n "$(git ls-files --others --exclude-standard)" ]; then
  git add -A
  git commit -m "chore: pre-release changes"
fi

pnpm version "$BUMP"
pnpm tauri build --bundles app
bash scripts/bundle-tray.sh
src-widget/build.sh
