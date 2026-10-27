#!/usr/bin/env bash
# 只读体检：这台 Android 设备上装的 Elsewhen 是不是当前仓库编出来的那一版？
#
# 只做三件事，绝不安装、绝不改动手机上的任何数据：
#   1. 从设备上把已安装的 base.apk 拉回来（只读）
#   2. 比对它和新编 APK（或本地最后一个 APK）里的迁移版本号、桥接 hash、
#      libelsewhen.so / libapp.so 的 md5
#   3. 打印 try 的结论
#
# 用法：
#   ./skills/android-app-update/scripts/check.sh              # 与本地已有 APK 比
#   ./skills/android-app-update/scripts/check.sh --build      # 先构建再比（改动后的常规做法）
#
# 退出码：0 = 已是最新；1 = 需要更新；2 = 环境/前置条件不满足（不是「需要更新」）
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/common.sh
. "$HERE/../lib/common.sh"

WANT_BUILD=0
for arg in "$@"; do
  case "$arg" in
    --build) WANT_BUILD=1 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) apk_die "未知参数：$arg（支持 --build；构建用 scripts/build.sh）" ;;
  esac
done

apk_pick_device

# ── 1. 本地 APK ──────────────────────────────────────────────────────────
if [ "$WANT_BUILD" = 1 ]; then
  "$HERE/build.sh"
fi

LOCAL_APK="$APK_MAIN_APK"
if [ ! -f "$LOCAL_APK" ]; then
  apk_die "本地没有 APK：$LOCAL_APK
       先跑 ./skills/android-app-update/scripts/build.sh（或本脚本加 --build）。"
fi

# ── 2. 设备上已装的 APK ──────────────────────────────────────────────────
apk_say "从设备拉回已安装的 APK（只读）"
if ! apk_adb shell pm path "$APK_APP_ID" >/dev/null 2>&1 \
   || [ -z "$(apk_adb shell pm path "$APK_APP_ID" 2>/dev/null | tr -d '\r')" ]; then
  apk_say "设备上没有装 $APK_APP_ID —— 这是首次安装，直接跑 install.sh"
  exit 1
fi
REMOTE_PATH="$(apk_adb shell pm path "$APK_APP_ID" | sed 's/^package://' | tr -d '\r' | head -1)"
INSTALLED_APK="$(mktemp -t elsewhen-installed-XXXXXX.apk)"
apk_adb pull "$REMOTE_PATH" "$INSTALLED_APK" >/dev/null 2>&1 \
  || apk_die "拉不出 $REMOTE_PATH（adb 断了吗？）"

echo "    已装 APK：$REMOTE_PATH → $INSTALLED_APK ($(stat -c%s "$INSTALLED_APK") 字节)"
echo "    本地 APK：$LOCAL_APK ($(stat -c%s "$LOCAL_APK") 字节)"

# ── 3. 判定 ──────────────────────────────────────────────────────────────
apk_say "对比"
INSTALLED_MIG="$(apk_max_migration "$INSTALLED_APK")"
LOCAL_MIG="$(apk_max_migration "$LOCAL_APK")"
INSTALLED_CERT="$(apk_cert_sha256 "$INSTALLED_APK")"
LOCAL_CERT="$(apk_cert_sha256 "$LOCAL_APK")"

# 解包两侧一次，复用给 md5 与迁移名检查。
TMP_I="$(mktemp -d)"; TMP_L="$(mktemp -d)"
trap 'rm -rf "$TMP_I" "$TMP_L" "$INSTALLED_APK"' EXIT
unzip -q -o "$INSTALLED_APK" 'lib/arm64-v8a/*' -d "$TMP_I"
unzip -q -o "$LOCAL_APK" 'lib/arm64-v8a/*' -d "$TMP_L"

printf '  %-26s %-22s %-22s\n' '项目' '设备上已装' '本地 APK'
printf '  %-26s %-22s %-22s\n' '仓库最新迁移' "$(apk_repo_latest_migration)" '← 两边都应到此'
printf '  %-26s %-22s %-22s\n' '最高迁移（仓库清单）' "${INSTALLED_MIG:-（无）}" "${LOCAL_MIG:-（无）}"

SO_I="$TMP_I/lib/arm64-v8a/libelsewhen.so"
SO_L="$TMP_L/lib/arm64-v8a/libelsewhen.so"
APP_I="$TMP_I/lib/arm64-v8a/libapp.so"
APP_L="$TMP_L/lib/arm64-v8a/libapp.so"
MD5_I="$(md5sum "$SO_I" | cut -d' ' -f1)"
MD5_L="$(md5sum "$SO_L" | cut -d' ' -f1)"
APP_MD5_I="$(md5sum "$APP_I" 2>/dev/null | cut -d' ' -f1)"
APP_MD5_L="$(md5sum "$APP_L" 2>/dev/null | cut -d' ' -f1)"
printf '  %-26s %-22s %-22s\n' 'libelsewhen.so md5' "${MD5_I:0:12}…" "${MD5_L:0:12}…"
printf '  %-26s %-22s %-22s\n' 'libapp.so md5' "${APP_MD5_I:0:12}…" "${APP_MD5_L:0:12}…"
printf '  %-26s %-22s %-22s\n' '签名证书' "${INSTALLED_CERT:0:16}…" "${LOCAL_CERT:0:16}…"

verdict=0
notes=()

[ "$MD5_I" = "$MD5_L" ] || { verdict=1; notes+=("libelsewhen.so 不同：设备上不是这份 Rust 内核"); }
if [ -n "$APP_MD5_I" ] && [ -n "$APP_MD5_L" ]; then
  [ "$APP_MD5_I" = "$APP_MD5_L" ] || { verdict=1; notes+=("libapp.so 不同：设备上不是这份 Dart 代码"); }
fi
[ "$INSTALLED_CERT" = "$LOCAL_CERT" ] || { verdict=1; notes+=("签名证书不同：装不上去（INSTALL_FAILED_UPDATE_INCOMPATIBLE）"); }

# 本地 APK 认得的迁移，设备上那版是不是也认得？缺哪个就报哪个 ——
# 这比 md5 更能说明「差在哪」。
if [ "$verdict" = 1 ] && [ -n "$SO_L" ] && [ -n "$SO_I" ]; then
  # 只拿「本地 APK 认得、设备上那版不认得」的迁移报差异 —— 集合来自仓库
  # migrations/ 清单，而不是从二进制里正则猜名字。
  while IFS= read -r m; do
    [ -n "$m" ] || continue
    notes+=("设备上那版不认识迁移 $m（升级后首次启动会先跑它）")
  done < <(comm -23 <(apk_so_known_migrations "$SO_L") <(apk_so_known_migrations "$SO_I"))
fi

# ── 4. 结论 ──────────────────────────────────────────────────────────────
apk_say "结论"
DUMPSYS="$(apk_adb shell dumpsys package "$APK_APP_ID" | tr -d '\r' || true)"
INSTALLED_WHEN="$(printf '%s\n' "$DUMPSYS" | sed -n 's/.*lastUpdateTime=//p' | head -1)"
[ -n "$INSTALLED_WHEN" ] && echo "    设备上最后更新：$INSTALLED_WHEN"
echo "    仓库 HEAD：$(apk_repo_head)"

BRIDGE="$(apk_repo_bridge_hash)"
echo "    仓库桥接 hash：Dart=${BRIDGE%% *} Rust=${BRIDGE##* }"
case "$BRIDGE" in
  " "*) echo "    ⚠ 读不到桥接 hash，先跑 scripts/regen.sh" ;;
  *)  set -- $BRIDGE
      [ "$1" = "$2" ] || echo "    ⚠ 仓库两侧桥接 hash 不一致（Dart=$1 Rust=$2）——先跑 scripts/regen.sh" ;;
esac

if [ "$verdict" = 0 ]; then
  echo
  echo "✓ 设备上装的就是这份代码，不需要更新。"
  exit 0
fi

echo
echo "✗ 需要更新："
for n in "${notes[@]}"; do echo "    - $n"; done
echo
echo "  更新：./skills/android-app-update/scripts/install.sh"
exit 1
