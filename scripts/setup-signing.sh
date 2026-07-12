#!/bin/bash
# Monet 本机签名身份基建（幂等，可重复执行）
#
# 背景：macOS TCC 权限授权钉在应用的 designated requirement 上。adhoc 签名的
# DR 退化为 cdhash（二进制内容哈希），每次重新编译授权即失效。本脚本创建一张
# 长效自签代码签名证书，让所有构建（主 app / routine-runner / mcp / widget）
# 共享稳定的 DR（identifier + 证书），重新构建不再丢权限。
#
# 信任边界：证书仅作本机签名身份，不是分发信任凭据（分发仍需 Developer ID +
# 公证）。钥匙串密码明文存本机、钥匙串永不自动锁定——本地同用户进程可用该
# 证书签任意二进制并继承已授予的 TCC 权限；能读到密码文件即已同权，可接受。
#
# 产物：
#   - 钥匙串 ~/Library/Keychains/monet-signing.keychain-db（专用，免交互解锁）
#   - 钥匙串密码 ~/.monet/signing/keychain-password（0600，运行时解锁用）
#   - 证书副本 ~/.monet/signing/monet-signing.pem（公开部分，重挂信任用）
#   - 私钥备份 ~/.monet/signing/backup.p12（防钥匙串文件损坏；密码即上面的
#     keychain-password。防不了整个 signing 目录被删——那要靠 Time Machine）
set -euo pipefail

IDENTITY="${MONET_SIGN_ID:-Monet Signing}"
KEYCHAIN="$HOME/Library/Keychains/monet-signing.keychain-db"
SIGNING_DIR="$HOME/.monet/signing"
PASS_FILE="$SIGNING_DIR/keychain-password"
CERT_PEM="$SIGNING_DIR/monet-signing.pem"
BACKUP_P12="$SIGNING_DIR/backup.p12"
CERT_DAYS=5475  # 15 年

SUDO="sudo"
[ -n "${SUDO_ASKPASS:-}" ] && SUDO="sudo -A"

unlock() {
    [ -f "$PASS_FILE" ] && [ -f "$KEYCHAIN" ] \
        && security unlock-keychain -p "$(cat "$PASS_FILE")" "$KEYCHAIN" 2>/dev/null
}

# 私钥+证书对是否在钥匙串中（不带 -v：信任丢失时依然能查到，与 -v 区分开）
identity_in_keychain() {
    security find-identity -p codesigning "$KEYCHAIN" 2>/dev/null | grep -Fq "$IDENTITY"
}

# --- 分支 1：身份有效且钥匙串可非交互解锁 → 全部就绪 ---
if security find-identity -v -p codesigning 2>/dev/null | grep -Fq "$IDENTITY"; then
    if unlock; then
        echo "=> 签名身份 \"$IDENTITY\" 已就绪，无需操作"
        exit 0
    fi
    echo "!! 身份存在但专用钥匙串无法非交互解锁（密码文件丢失/不符），需要重建"
fi

# --- 分支 2：私钥完好、仅系统信任丢失（OS 重装/迁移不带 System.keychain 信任项）
#             → 只重挂信任，绝不销毁私钥（换私钥 = DR 变化 = TCC 授权全丢） ---
if unlock && identity_in_keychain; then
    echo "=> 私钥完好，仅信任设置缺失，重挂代码签名信任（需要管理员权限）"
    if [ ! -f "$CERT_PEM" ]; then
        security find-certificate -c "$IDENTITY" -p "$KEYCHAIN" > "$CERT_PEM"
    fi
    $SUDO security add-trusted-cert -d -r trustRoot -p codeSign \
        -k /Library/Keychains/System.keychain "$CERT_PEM"
    security find-identity -v -p codesigning | grep -Fq "$IDENTITY" \
        && echo "=> 信任已恢复，签名身份 \"$IDENTITY\" 就绪" && exit 0
    echo "!! 重挂信任后身份仍无效，转入重建"
fi

# --- 分支 3：推倒重建 ---
if [ -f "$KEYCHAIN" ] || [ -f "$PASS_FILE" ]; then
    echo "!! 警告：即将更换签名证书。新证书的 designated requirement 与旧授权"
    echo "!!       不再匹配，所有已授予的 TCC 权限（自动化/辅助功能等）需要重新授权。"
fi

echo "=> 创建签名身份 \"$IDENTITY\"（有效期 ${CERT_DAYS} 天）"

if [ -f "$KEYCHAIN" ]; then
    echo "=> 清理残留钥匙串"
    security delete-keychain "$KEYCHAIN" 2>/dev/null || rm -f "$KEYCHAIN"
fi
# 清理 System.keychain 中历次重建累积的旧同名证书（连带其信任设置）
while $SUDO security delete-certificate -c "$IDENTITY" \
    /Library/Keychains/System.keychain 2>/dev/null; do :; done

mkdir -p "$SIGNING_DIR"
chmod 700 "$SIGNING_DIR"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# --- 生成自签证书（codeSigning EKU 必需，缺了 codesign 不认） ---
cat > "$TMP_DIR/cert.conf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $IDENTITY
[ext]
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
basicConstraints = critical,CA:false
subjectKeyIdentifier = hash
EOF
openssl req -x509 -newkey rsa:2048 -sha256 -nodes \
    -keyout "$TMP_DIR/key.pem" -out "$TMP_DIR/cert.pem" \
    -days "$CERT_DAYS" -config "$TMP_DIR/cert.conf" 2>/dev/null

KC_PASS=$(openssl rand -hex 16)
openssl pkcs12 -export -out "$TMP_DIR/cert.p12" \
    -inkey "$TMP_DIR/key.pem" -in "$TMP_DIR/cert.pem" \
    -passout "pass:$KC_PASS"

# --- 专用钥匙串（独立密码 → set-key-partition-list 可脚本化，签名免交互） ---
security create-keychain -p "$KC_PASS" "$KEYCHAIN"
security set-keychain-settings "$KEYCHAIN"   # 不带超时参数 = 永不自动锁定
security unlock-keychain -p "$KC_PASS" "$KEYCHAIN"
security import "$TMP_DIR/cert.p12" -k "$KEYCHAIN" -P "$KC_PASS" \
    -f pkcs12 -T /usr/bin/codesign -T /usr/bin/security
security set-key-partition-list -S "apple-tool:,apple:,codesign:" \
    -s -k "$KC_PASS" "$KEYCHAIN" > /dev/null

# 加入用户钥匙串搜索列表（数组承接，路径含空格也安全；已在列则跳过）
KEYCHAIN_LIST=()
while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"   # 去前导空白
    line="${line%\"}"
    line="${line#\"}"
    [ -n "$line" ] && KEYCHAIN_LIST+=("$line")
done < <(security list-keychains -d user)
IN_LIST=0
for kc in "${KEYCHAIN_LIST[@]}"; do
    [ "$kc" = "$KEYCHAIN" ] && IN_LIST=1
done
if [ "$IN_LIST" = 0 ]; then
    security list-keychains -d user -s "${KEYCHAIN_LIST[@]}" "$KEYCHAIN"
fi

printf '%s' "$KC_PASS" > "$PASS_FILE"
chmod 600 "$PASS_FILE"
cp "$TMP_DIR/cert.pem" "$CERT_PEM"
cp "$TMP_DIR/cert.p12" "$BACKUP_P12"
chmod 600 "$BACKUP_P12"

# --- 信任设置（codeSign 策略，admin 域）：codesign 拒绝用不受信任的证书签名 ---
echo "=> 写入代码签名信任（需要管理员权限）"
$SUDO security add-trusted-cert -d -r trustRoot -p codeSign \
    -k /Library/Keychains/System.keychain "$CERT_PEM"

# --- 冒烟验证：非交互签名 + DR 形态 ---
echo "=> 冒烟验证"
security find-identity -v -p codesigning | grep -F "$IDENTITY"
cp /bin/ls "$TMP_DIR/smoke"
codesign --force --sign "$IDENTITY" --identifier io.github.zenolab124.monet.smoke "$TMP_DIR/smoke"
codesign --verify --strict "$TMP_DIR/smoke"
echo "=> designated requirement："
codesign -d -r- "$TMP_DIR/smoke" 2>&1 | grep designated

echo "=> 完成。签名身份 \"$IDENTITY\" 就绪。"
