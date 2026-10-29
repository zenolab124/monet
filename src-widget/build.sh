#!/bin/bash
# 构建 Widget Extension + widget-updater，嵌入 Tauri .app bundle，签名，打 DMG
# 用法：build.sh [Configuration] [App 路径]；build.sh --notary-preflight 只做公证凭据预检
set -euo pipefail

# 长期私钥只保留为当前 shell 的非导出变量。后续 xcodebuild、cargo、codesign、
# diskutil 等子进程不应继承它们；仅在实际调用 notarytool / Tauri signer 时注入。
NOTARY_PRIVATE_KEY="${APPLE_API_PRIVATE_KEY:-}"
NOTARY_EXTERNAL_KEY_PATH="${APPLE_API_KEY_PATH:-}"
NOTARY_KEY_ID="${APPLE_API_KEY:-}"
NOTARY_ISSUER_ID="${APPLE_API_ISSUER:-}"
UPDATER_SIGNING_PRIVATE_KEY="${TAURI_SIGNING_PRIVATE_KEY:-}"
UPDATER_SIGNING_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
unset APPLE_API_PRIVATE_KEY APPLE_API_KEY_PATH APPLE_API_KEY APPLE_API_ISSUER
unset TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD
export -n NOTARY_PRIVATE_KEY NOTARY_EXTERNAL_KEY_PATH NOTARY_KEY_ID NOTARY_ISSUER_ID
export -n UPDATER_SIGNING_PRIVATE_KEY UPDATER_SIGNING_PASSWORD

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# --notary-preflight：只用公证 API key 查询一次提交历史，在任何构建开始前
# 识别协议过期、key 失效等账号层面故障；不构建、不签名、不提交产物。
MODE="package"
if [ "${1:-}" = "--notary-preflight" ]; then
    MODE="notary-preflight"
    shift
fi

# 预检与正式公证共用同一份凭据校验、密钥落地与清理逻辑。
ENTITLEMENTS_DIR=""
NOTARY_KEY_FILE=""
NOTARY_ACTIVE_KEY_PATH=""
release_notary_key() {
    if [ -n "$NOTARY_KEY_FILE" ]; then
        rm -f "$NOTARY_KEY_FILE"
        NOTARY_KEY_FILE=""
    fi
    NOTARY_ACTIVE_KEY_PATH=""
}
cleanup_sensitive_files() {
    release_notary_key
    if [ -n "$ENTITLEMENTS_DIR" ]; then
        rm -rf "$ENTITLEMENTS_DIR"
    fi
}
trap cleanup_sensitive_files EXIT

# 私钥只在调用 notarytool 的这一刻写入 0600 临时文件，调用结束立即删除。
materialize_notary_key() {
    NOTARY_ACTIVE_KEY_PATH="$NOTARY_EXTERNAL_KEY_PATH"
    if [ -n "$NOTARY_PRIVATE_KEY" ]; then
        NOTARY_KEY_FILE=$(mktemp "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/monet-notary-key.XXXXXX")
        chmod 600 "$NOTARY_KEY_FILE"
        printf '%s' "$NOTARY_PRIVATE_KEY" > "$NOTARY_KEY_FILE"
        NOTARY_ACTIVE_KEY_PATH="$NOTARY_KEY_FILE"
    fi
}

NOTARY_KEY_SOURCE="${NOTARY_PRIVATE_KEY:-$NOTARY_EXTERNAL_KEY_PATH}"
NOTARY_ENV_COUNT=0
for VALUE in "$NOTARY_KEY_ID" "$NOTARY_ISSUER_ID" "$NOTARY_KEY_SOURCE"; do
    [ -n "$VALUE" ] && NOTARY_ENV_COUNT=$((NOTARY_ENV_COUNT + 1))
done

NOTARY_AGREEMENT_REASON="Apple Developer agreement is missing or expired. Sign it at https://developer.apple.com/account as the Account Holder, then re-run this job."
NOTARY_KEY_REASON="Notary API key is invalid or revoked. Check APPLE_API_PRIVATE_KEY / APPLE_API_KEY / APPLE_API_ISSUER."

# 识别规则：协议缺失或 HTTP 403 → agreement；HTTP 401 → key 失效；
# 其余（超时、网络、未知错误）不在此定论，交给正式公证步骤给出最终结果。
classify_notary_preflight() {
    local status="$1"
    local output="$2"
    if [ "$status" -eq 0 ]; then
        echo "ok"
    elif grep -qiE 'agreement' <<< "$output" || grep -qE '(^|[^0-9])403([^0-9]|$)' <<< "$output"; then
        echo "agreement"
    elif grep -qE '(^|[^0-9])401([^0-9]|$)' <<< "$output"; then
        echo "invalid-key"
    else
        echo "inconclusive"
    fi
}

# GitHub Actions 中输出注解；本地只输出纯文本。
preflight_annotate() {
    local level="$1"
    local message="$2"
    if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
        echo "::$level title=Notary preflight::$message"
    else
        echo "$message"
    fi
}

print_preflight_output() {
    local output="$1"
    [ -n "$output" ] || return 0
    if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
        echo "::group::notarytool output"
        sed -n '1,40p' <<< "$output"
        echo "::endgroup::"
    else
        sed -n '1,40p' <<< "$output"
    fi
}

run_notary_preflight() {
    # 三项任一为空即跳过，与未配置时的打包行为保持一致；
    # 部分配置仍由打包步骤的完整性校验拒绝。
    if [ "$NOTARY_ENV_COUNT" -eq 0 ]; then
        preflight_annotate notice "Notary credentials are not configured; skipping notary preflight."
        return 0
    fi
    if [ "$NOTARY_ENV_COUNT" -ne 3 ]; then
        preflight_annotate notice "Notary credentials are incomplete ($NOTARY_ENV_COUNT of 3 set); skipping notary preflight."
        return 0
    fi

    local timeout_seconds="${NOTARY_PREFLIGHT_TIMEOUT:-25}"
    local output_file status=0 output verdict pid deadline
    output_file=$(mktemp "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/monet-notary-preflight.XXXXXX")

    materialize_notary_key
    xcrun notarytool history \
        --key "$NOTARY_ACTIVE_KEY_PATH" \
        --key-id "$NOTARY_KEY_ID" \
        --issuer "$NOTARY_ISSUER_ID" > "$output_file" 2>&1 &
    pid=$!
    # 轮询而非后台 sleep 看门狗：超时后不留下孤儿进程拖住 job 日志流。
    deadline=$((SECONDS + timeout_seconds))
    while kill -0 "$pid" 2>/dev/null; do
        if [ "$SECONDS" -ge "$deadline" ]; then
            kill -TERM "$pid" 2>/dev/null || true
            echo "notarytool history timed out after ${timeout_seconds}s" >> "$output_file"
            break
        fi
        sleep 0.2
    done
    wait "$pid" || status=$?
    release_notary_key

    output=$(cat "$output_file")
    rm -f "$output_file"
    verdict=$(classify_notary_preflight "$status" "$output")

    local reason=""
    case "$verdict" in
        ok)
            echo "Notary preflight passed."
            return 0
            ;;
        agreement) reason="$NOTARY_AGREEMENT_REASON" ;;
        invalid-key) reason="$NOTARY_KEY_REASON" ;;
        *)
            preflight_annotate warning "Notary preflight was inconclusive (exit $status); continuing, notarization will report the final result."
            print_preflight_output "$output"
            return 0
            ;;
    esac

    # 首行必须是原因句；随后才是注解与原始输出。
    echo "$reason"
    if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
        echo "::error title=Notary preflight::$reason"
    fi
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
        echo "reason=$reason" >> "$GITHUB_OUTPUT"
    fi
    print_preflight_output "$output"
    return 1
}

if [ "$MODE" = "notary-preflight" ]; then
    run_notary_preflight
    exit $?
fi

# 公证必须发生在 Widget、helper 与主 App 全部完成最终签名之后。若只提供了
# 部分凭据，宁可停止构建，也不能静默发布一个仅签名、未公证的安装包。
if [ -n "$NOTARY_PRIVATE_KEY" ] && [ -n "$NOTARY_EXTERNAL_KEY_PATH" ]; then
    echo "Error: provide APPLE_API_PRIVATE_KEY or APPLE_API_KEY_PATH, not both" >&2
    exit 1
fi
if [ "$NOTARY_ENV_COUNT" -ne 0 ] && [ "$NOTARY_ENV_COUNT" -ne 3 ]; then
    echo "Error: incomplete App Store Connect notarization credentials" >&2
    exit 1
fi
NOTARIZE=0
if [ "$NOTARY_ENV_COUNT" -eq 3 ]; then
    if [ -n "$NOTARY_EXTERNAL_KEY_PATH" ] && [ ! -f "$NOTARY_EXTERNAL_KEY_PATH" ]; then
        echo "Error: APPLE_API_KEY_PATH does not exist" >&2
        exit 1
    fi
    NOTARIZE=1
fi

submit_for_notarization() {
    local artifact="$1"
    local submit_status=0

    materialize_notary_key
    xcrun notarytool submit "$artifact" \
        --key "$NOTARY_ACTIVE_KEY_PATH" \
        --key-id "$NOTARY_KEY_ID" \
        --issuer "$NOTARY_ISSUER_ID" \
        --wait || submit_status=$?
    release_notary_key
    return "$submit_status"
}

# --- 参数 ---
SIGN_ID="${SIGN_ID:-Monet Signing}"
APP_IDENTIFIER="io.github.zenolab124.monet"
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

if [ -f "$SIGNING_PASS_FILE" ] && [ -f "$SIGNING_KEYCHAIN" ]; then
    security unlock-keychain -p "$(cat "$SIGNING_PASS_FILE")" "$SIGNING_KEYCHAIN"
fi
IDENTITIES=$(security find-identity -v -p codesigning)
IDENTITY_LINE=$(grep -F "$SIGN_ID" <<< "$IDENTITIES" | head -1 || true)
if [ -z "$IDENTITY_LINE" ]; then
    echo "Error: signing identity '$SIGN_ID' not found" >&2
    exit 1
fi
SIGNING_IDENTITY_NAME=$(sed -nE 's/.*"([^"]+)".*/\1/p' <<< "$IDENTITY_LINE")
if [ -z "$SIGNING_IDENTITY_NAME" ]; then
    echo "Error: cannot resolve signing identity name" >&2
    exit 1
fi
CODESIGN_ARGS=(--force --options runtime --sign "$SIGN_ID")
if [[ "$SIGNING_IDENTITY_NAME" == "Developer ID Application:"* ]]; then
    CODESIGN_ARGS+=(--timestamp)
fi

# macOS 支持 Team ID 前缀的 App Group，无需注册 group.* 标识符或嵌入
# provisioning profile。Developer ID / Apple Development 身份均可从名称末尾提取 Team ID；
# 自签身份没有 Apple Team ID，因此安全地关闭共享写入，避免再次触发容器授权弹窗。
resolve_app_group_identifier() {
    if [ -n "${MONET_APP_GROUP_ID:-}" ]; then
        echo "$MONET_APP_GROUP_ID"
        return
    fi

    local identity_name="$SIGNING_IDENTITY_NAME"
    if [[ "$identity_name" =~ ^(Developer\ ID\ Application|Apple\ Development):.*\(([A-Z0-9]{10})\)$ ]]; then
        echo "${BASH_REMATCH[2]}.io.github.zenolab124.monet"
    fi
}

APP_GROUP_ID=$(resolve_app_group_identifier)
if [ -n "$APP_GROUP_ID" ] && \
   [[ ! "$APP_GROUP_ID" =~ ^[A-Z0-9]{10}\.io\.github\.zenolab124\.monet$ ]]; then
    echo "Error: invalid MONET_APP_GROUP_ID format" >&2
    exit 1
fi

ENTITLEMENTS_DIR=$(mktemp -d)
prepare_entitlements() {
    local source="$1"
    local destination="$2"
    cp "$source" "$destination"
    if [ -n "$APP_GROUP_ID" ]; then
        /usr/libexec/PlistBuddy \
            -c "Add :com.apple.security.application-groups array" \
            -c "Add :com.apple.security.application-groups:0 string $APP_GROUP_ID" \
            "$destination"
    fi
}

MAIN_ENTITLEMENTS="$ENTITLEMENTS_DIR/Monet.entitlements"
WIDGET_ENTITLEMENTS="$ENTITLEMENTS_DIR/MonetWidgetExtension.entitlements"
UPDATER_ENTITLEMENTS="$ENTITLEMENTS_DIR/WidgetUpdater.entitlements"
prepare_entitlements ../src-tauri/Monet.entitlements "$MAIN_ENTITLEMENTS"
prepare_entitlements MonetWidgetExtension.entitlements "$WIDGET_ENTITLEMENTS"
prepare_entitlements WidgetUpdater.entitlements "$UPDATER_ENTITLEMENTS"

# --- 构建 Widget Extension ---
echo "=> Building widget extension..."
DEVELOPER_DIR="$XCODE" xcodegen generate --quiet 2>/dev/null || DEVELOPER_DIR="$XCODE" xcodegen generate
DEVELOPER_DIR="$XCODE" xcodebuild build \
    -project MonetWidget.xcodeproj \
    -target MonetWidgetExtension \
    -configuration "$CONFIG" \
    CODE_SIGNING_ALLOWED=NO \
    MONET_APP_GROUP_ID="$APP_GROUP_ID" \
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
LAUNCH_AGENTS_DIR="$APP_BUNDLE/Contents/Library/LaunchAgents"
mkdir -p "$LAUNCH_AGENTS_DIR"
cp io.github.zenolab124.monet.widget-updater.plist "$LAUNCH_AGENTS_DIR/"
if [ -n "$APP_GROUP_ID" ]; then
    /usr/libexec/PlistBuddy \
        -c "Set :MonetAppGroupIdentifier $APP_GROUP_ID" \
        "$APP_BUNDLE/Contents/Info.plist"
    echo "=> App Group enabled for this Apple team"
else
    echo "=> App Group disabled (signing identity has no Apple Team ID)"
fi

# --- 签名 ---
echo "=> Signing..."

codesign "${CODESIGN_ARGS[@]}" \
    --entitlements "$WIDGET_ENTITLEMENTS" \
    "$PLUGINS_DIR/MonetWidgetExtension.appex"
for BIN in "$APP_BUNDLE/Contents/MacOS/"*; do
    NAME=$(basename "$BIN")
    [ "$NAME" = "app" ] && continue
    # routine-runner 发 Apple Events（定时任务里的自动化操作），hardened
    # runtime 下必须随签名授予 apple-events entitlement，否则 tccd 直接
    # 拒绝授权弹窗，用户无法完成授权
    if [ "$NAME" = "monet-routine-runner" ]; then
        codesign "${CODESIGN_ARGS[@]}" \
            --entitlements ../src-tauri/runner-entitlements.plist \
            --identifier "$APP_IDENTIFIER.$NAME" "$BIN"
    elif [ "$NAME" = "widget-updater" ]; then
        # SMAppService 将 LaunchAgent 的受保护数据访问归属到主应用。
        # updater 还必须自己携带 App Group entitlement，子进程不继承父进程能力。
        codesign "${CODESIGN_ARGS[@]}" \
            --entitlements "$UPDATER_ENTITLEMENTS" \
            --identifier "$APP_IDENTIFIER" "$BIN"
    else
        codesign "${CODESIGN_ARGS[@]}" \
            --identifier "$APP_IDENTIFIER.$NAME" "$BIN"
    fi
done
# Helper App（独立 menubar 进程）：嵌套 bundle 必须先签内层再签外层
TRAY_APP="$APP_BUNDLE/Contents/Library/LoginItems/MonetTray.app"
if [ -d "$TRAY_APP" ]; then
    codesign "${CODESIGN_ARGS[@]}" \
        --identifier "io.github.zenolab124.monet.tray" "$TRAY_APP"
fi
# Routine Runner 也必须是完整签名的 Helper App。旧版只有裸二进制，TCC 以
# 路径记账；签名身份切换后设置开关仍指向旧 code requirement，造成假授权。
RUNNER_APP="$APP_BUNDLE/Contents/Helpers/MonetRoutineRunner.app"
if [ -d "$RUNNER_APP" ]; then
    codesign "${CODESIGN_ARGS[@]}" \
        --entitlements ../src-tauri/runner-entitlements.plist \
        --identifier "io.github.zenolab124.monet.monet-routine-runner" "$RUNNER_APP"
fi
codesign "${CODESIGN_ARGS[@]}" \
    --entitlements "$MAIN_ENTITLEMENTS" "$APP_BUNDLE"
codesign --verify --deep --strict "$APP_BUNDLE"
SIGNATURE_DETAILS=$(codesign -dvv "$APP_BUNDLE" 2>&1)
EXPECTED_AUTHORITY="$SIGN_ID"
if [[ "$SIGN_ID" =~ ^[A-Fa-f0-9]{40}$ ]]; then
    EXPECTED_AUTHORITY="$SIGNING_IDENTITY_NAME"
fi
if [ -z "$EXPECTED_AUTHORITY" ] || \
   ! grep -F "Authority=$EXPECTED_AUTHORITY" <<< "$SIGNATURE_DETAILS" >/dev/null; then
    echo "Error: final app was not signed by the selected identity" >&2
    exit 1
fi
if grep -F 'Signature=adhoc' <<< "$SIGNATURE_DETAILS" >/dev/null; then
    echo "Error: unexpected ad-hoc signature" >&2
    exit 1
fi

# Updater 直接分发 .app.tar.gz，因此不能只公证外层 DMG；先公证最终 App，
# staple 后再从同一份 App 生成 DMG 与 updater 工件。
if [ "$NOTARIZE" -eq 1 ]; then
    echo "=> Notarizing app..."
    NOTARY_DIR=$(mktemp -d)
    NOTARY_ZIP="$NOTARY_DIR/$(basename "$APP_BUNDLE").zip"
    ditto -c -k --keepParent "$APP_BUNDLE" "$NOTARY_ZIP"
    submit_for_notarization "$NOTARY_ZIP"
    xcrun stapler staple "$APP_BUNDLE"
    xcrun stapler validate "$APP_BUNDLE"
    rm -rf "$NOTARY_DIR"
fi

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

if [ "$NOTARIZE" -eq 1 ]; then
    echo "=> Notarizing DMG..."
    submit_for_notarization "$DMG_PATH"
    xcrun stapler staple "$DMG_PATH"
    xcrun stapler validate "$DMG_PATH"
fi

# --- Updater 产物（.app.tar.gz + minisign 签名 + latest.json） ---
# 仅在提供 TAURI_SIGNING_PRIVATE_KEY 时生成（发版链路:CI 经 secrets 注入;
# 日常本地打包无密钥自动跳过,不阻塞）。私钥对应 tauri.conf plugins.updater.pubkey
UPDATER_DIR="$(dirname "$APP_BUNDLE")/../updater"
if [ -n "$UPDATER_SIGNING_PRIVATE_KEY" ]; then
    echo "=> Creating updater artifacts..."
    mkdir -p "$UPDATER_DIR"
    TARBALL="$UPDATER_DIR/${APP_NAME}_${VERSION}_aarch64.app.tar.gz"
    rm -f "$TARBALL" "$TARBALL.sig"
    tar czf "$TARBALL" -C "$(dirname "$APP_BUNDLE")" "$(basename "$APP_BUNDLE")"
    (
        cd ..
        TAURI_SIGNING_PRIVATE_KEY="$UPDATER_SIGNING_PRIVATE_KEY" \
            TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$UPDATER_SIGNING_PASSWORD" \
            pnpm tauri signer sign "$TARBALL"
    )
    node "$SCRIPT_DIR/../scripts/create-latest-json.mjs" "$VERSION" "$TARBALL" "$UPDATER_DIR/latest.json"
    echo "   Updater: $TARBALL (+.sig, latest.json)"
else
    echo "=> Skipping updater artifacts (TAURI_SIGNING_PRIVATE_KEY not set)"
fi

echo "=> Done!"
echo "   App: $APP_BUNDLE"
echo "   DMG: $DMG_PATH"
