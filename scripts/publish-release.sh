#!/usr/bin/env bash
# 发布一个应用版本：构建桌面 so + Android APK → 上传 R2 → 写升级清单。
#
# 前置（一次性）：npx wrangler r2 bucket create elsewhen-releases
# 认证与 deploy-relay.sh 相同（relay/.env 或 wrangler login）。
# 上传走 api.cloudflare.com，直连质量差时预置 HTTPS_PROXY（脚本自动探测
# 本机常见代理端口，与桌面启动脚本同规则）。
#
# 用法：
#   scripts/publish-release.sh "修复同步配对不稳定"   # changelog 必填
#   scripts/publish-release.sh "..." --skip-build     # 复用现有产物
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CHANGELOG=""
SKIP_BUILD=0
for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=1 ;;
    *) if [ -z "$CHANGELOG" ]; then CHANGELOG="$arg"; else echo "多余参数: $arg" >&2; exit 1; fi ;;
  esac
done
[ -n "$CHANGELOG" ] || { echo "用法: $0 \"changelog 文案\" [--skip-build]" >&2; exit 1; }

# 代理探测（同 debug-run.sh：reqwest/wrangler 都不读系统代理）
if [ -z "${HTTPS_PROXY:-}" ] && [ -z "${https_proxy:-}" ] && [ -z "${ALL_PROXY:-}" ] && [ -z "${all_proxy:-}" ]; then
    for port in 7897 7890 7891 1080; do
        if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
            exec 3>&- 3<&- || true
            export HTTPS_PROXY="http://127.0.0.1:$port"
            echo "已探测到本机代理端口 $port，上传将走该代理"
            break
        fi
    done
fi

if [ ! -d relay/node_modules ]; then (cd relay && npm install); fi
WRANGLER="npx --prefix relay wrangler"
BUCKET="elsewhen-releases"

# 干净工作树：hash 才能唯一对应源码（dirty 时照发但显著警告）。
HASH="$(git rev-parse --short HEAD)"
SEQ="$(git rev-list --count HEAD)"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "⚠ 工作树有未提交改动，发布的 hash=$HASH 将与源码不完全对应。10 秒内 Ctrl-C 可中止。"
    sleep 10
fi
VERSION="$(grep -m1 '^version' ui/pubspec.yaml | sed 's/.*version: *//;s/+.*//')"

echo "==> [1/3] 构建（版本 $VERSION，hash $HASH，序号 $SEQ）"
if [ "$SKIP_BUILD" -eq 0 ]; then
    ./scripts/build-android-rust.sh >/dev/null
    ./scripts/regen.sh >/dev/null
    (cd ui && flutter build apk --release >/dev/null)
fi
APK="ui/build/app/outputs/flutter-apk/app-release.apk"
[ -f "$APK" ] || { echo "APK 不存在: $APK" >&2; exit 1; }

echo "==> [2/3] 上传 APK 到 R2（$BUCKET/apk/$HASH.apk）"
$WRANGLER r2 object put "$BUCKET/apk/$HASH.apk" --file "$APK" --remote --content-type "application/vnd.android.package-archive" >/dev/null

SHA="$(sha256sum "$APK" | cut -d' ' -f1)"
SIZE="$(stat -c %s "$APK")"
MANIFEST=$(printf '{"version":"%s","buildHash":"%s","buildSeq":%s,"changelog":"%s","apkSha256":"%s","apkSize":%s,"publishedAt":%s}' \
    "$VERSION" "$HASH" "$SEQ" "$CHANGELOG" "$SHA" "$SIZE" "$(date +%s)000")

echo "==> [3/3] 写升级清单（release/manifest.json）"
printf '%s' "$MANIFEST" | $WRANGLER r2 object put "$BUCKET/release/manifest.json" --pipe --remote --content-type "application/json" >/dev/null

echo
echo "✔ 发布完成：v$VERSION · $HASH（序号 $SEQ）"
echo "  客户端「设置 → 检查更新」即可看到；changelog：$CHANGELOG"
