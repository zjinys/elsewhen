#!/usr/bin/env bash
# 把当前仓库编出来的 APK 装到 / 更新到 Android 设备上，并证明装上去的确实是它。
#
# 用法：
#   ./skills/android-app-update/scripts/install.sh                 # 需要时先构建，再安装
#   ./skills/android-app-update/scripts/install.sh --no-build      # 只装本地已有 APK
#   ./skills/android-app-update/scripts/install.sh --dry-run       # 只打印将要做什么
#   ./skills/android-app-update/scripts/install.sh --relaunch      # 安装后冷启动一次
#
# 它不会做的事（重要）：
#   * 不 `adb uninstall`、不 `pm clear` —— 覆盖安装保留应用数据。
#     手机上的库会在首次启动自己的迁移里从旧版本升到新版本。
#   * 不动 `/data/data/<pkg>`，不做任何备份。设备未 root、app 非 debuggable
#     （`run-as` 不可用、`adb root` 也不可用），这里拿不到应用私有目录 ——
#     需要备份就用应用内的导出功能，别指望 adb。
#   * 不装到多台设备：目标不唯一时必须显式指定（见 config.env 的 APK_DEVICE）。
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/common.sh
. "$HERE/../lib/common.sh"

DO_BUILD=1
DRY_RUN=0
RELAUNCH=0
for arg in "$@"; do
  case "$arg" in
    --no-build) DO_BUILD=0 ;;
    --dry-run) DRY_RUN=1 ;;
    --relaunch) RELAUNCH=1 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) apk_die "未知参数：$arg" ;;
  esac
done

apk_pick_device

# ── 1. 构建 ──────────────────────────────────────────────────────────────
if [ "$DO_BUILD" = 1 ]; then
  "$HERE/build.sh"
else
  [ -f "$APK_MAIN_APK" ] || apk_die "--no-build 但本地没有 $APK_MAIN_APK"
fi
LOCAL_APK="$APK_MAIN_APK"

# ── 2. 前置检查（在动手机之前全部做完）──────────────────────────────────
apk_say "前置检查"
LOCAL_CERT="$(apk_cert_sha256 "$LOCAL_APK")"
echo "    本地 APK 签名：$LOCAL_CERT"

if [ -n "$APK_EXPECTED_CERT_SHA256" ]; then
  # 多证书时逐个比对（用 grep -F 避免把指纹当正则）。
  if printf '%s\n' "$APK_EXPECTED_CERT_SHA256" | tr ' ' '\n' | grep -qxF "$LOCAL_CERT"; then
    echo "    ✓ 与 config.env 允许的签名一致"
  else
    echo "    ✗ 与 config.env 里 APK_EXPECTED_CERT_SHA256 不一致："
    printf '%s\n' "$APK_EXPECTED_CERT_SHA256" | tr ' ' '\n' | sed 's/^/        /'
    apk_die "本地 APK 是另一张证书签的。覆盖安装会以 INSTALL_FAILED_UPDATE_INCOMPATIBLE 失败。
       要么用对的签名重编（见 references/runbook.md「签名」节），
       要么确认要换签名后显式改 config.env（那会导致先卸载、数据全丢）。"
  fi
fi

DEVICE_INSTALLED=1
if ! apk_adb shell pm path "$APK_APP_ID" >/dev/null 2>&1 \
   || [ -z "$(apk_adb shell pm path "$APK_APP_ID" 2>/dev/null | tr -d '\r')" ]; then
  DEVICE_INSTALLED=0
  echo "    设备上还没装过 $APK_APP_ID（首次安装）"
fi

if [ "$DEVICE_INSTALLED" = 1 ]; then
  REMOTE_PATH="$(apk_adb shell pm path "$APK_APP_ID" | sed 's/^package://' | tr -d '\r' | head -1)"
  INSTALLED_APK="$(mktemp -t elsewhen-installed-XXXXXX.apk)"
  trap 'rm -f "$INSTALLED_APK"' EXIT
  apk_adb pull "$REMOTE_PATH" "$INSTALLED_APK" >/dev/null 2>&1
  DEVICE_CERT="$(apk_cert_sha256 "$INSTALLED_APK")"
  echo "    设备上那版签名：$DEVICE_CERT"
  if [ "$DEVICE_CERT" != "$LOCAL_CERT" ]; then
    apk_die "签名不同，覆盖安装必然失败（INSTALL_FAILED_UPDATE_INCOMPATIBLE）。
       要换签名只能先卸载（应用数据会全丢）—— 这属于破坏性操作，请人工确认后自己执行：
         adb -s $APK_DEVICE uninstall $APK_APP_ID
         adb -s $APK_DEVICE install \"$LOCAL_APK\""
  fi
  DEVICE_MIG="$(apk_max_migration "$INSTALLED_APK")"
  LOCAL_MIG="$(apk_max_migration "$LOCAL_APK")"
  echo "    迁移版本：设备上 $DEVICE_MIG → 本地 $LOCAL_MIG"
  case "$(printf '%s\n%s\n' "$DEVICE_MIG" "$LOCAL_MIG" | sort -V | tail -1)" in
    "$DEVICE_MIG")
      # 设备上那版比本地新：装回去等于降级，schema 已经前进过了。
      [ "$DEVICE_MIG" = "$LOCAL_MIG" ] || apk_die "设备上的迁移比本地 APK 更新（$DEVICE_MIG > $LOCAL_MIG）。
       迁移只前进不回退 —— 装旧版会让 app 遇到比它认识的更新的 schema。
       先 \`git pull\` 把仓库更新到对应版本再装。"
      echo "    （迁移版本相同；下面仍会按 md5 判断是否需要真的装）"
      ;;
  esac
fi

if [ "$DRY_RUN" = 1 ]; then
  apk_say "--dry-run：不做任何改动。将要执行："
  echo "    adb -s $APK_DEVICE install -r \"$LOCAL_APK\""
  [ "$RELAUNCH" = 1 ] && echo "    adb -s $APK_DEVICE shell am start -n $APK_APP_ID/.MainActivity"
  exit 0
fi

# ── 3. 安装 ──────────────────────────────────────────────────────────────
apk_say "安装（覆盖，保留应用数据）"
if ! apk_adb install -r "$LOCAL_APK" 2>&1 | tee /tmp/elsewhen-install.log | sed 's/^/    /'; then
  apk_die "adb install 失败，上面是原始输出（完整日志 /tmp/elsewhen-install.log）。
       常见原因：签名不同、设备空间不足、安装被安全策略拦下（MIUI 等需在手机上确认）。"
fi
grep -q "Success" /tmp/elsewhen-install.log || apk_die "adb 没报错但也没有 Success —— 安装结果未知，别当成功。见 /tmp/elsewhen-install.log"

# ── 4. 复核：从设备上把装好的拉回来，逐项比对 ────────────────────────────
apk_say "复核（把装好的 APK 拉回来重新比对）"
NEW_REMOTE="$(apk_adb shell pm path "$APK_APP_ID" | sed 's/^package://' | tr -d '\r' | head -1)"
VERIFY_APK="$(mktemp -t elsewhen-verify-XXXXXX.apk)"
trap 'rm -f "${INSTALLED_APK:-}" "$VERIFY_APK"' EXIT
apk_adb pull "$NEW_REMOTE" "$VERIFY_APK" >/dev/null 2>&1 || apk_die "复核时拉不出已装 APK"

TMP_L="$(mktemp -d)"; TMP_V="$(mktemp -d)"
trap 'rm -f "${INSTALLED_APK:-}" "$VERIFY_APK"; rm -rf "$TMP_L" "$TMP_V"' EXIT
SO_L="$(apk_extract_so "$LOCAL_APK" "$TMP_L")"
SO_V="$(apk_extract_so "$VERIFY_APK" "$TMP_V")"

MD5_L="$(md5sum "$SO_L" | cut -d' ' -f1)"
MD5_V="$(md5sum "$SO_V" | cut -d' ' -f1)"
LOCAL_MIG="$(apk_max_migration "$LOCAL_APK")"
VERIFY_MIG="$(apk_max_migration "$VERIFY_APK")"

echo "    libelsewhen.so  本地 $MD5_L"
echo "                    设备 $MD5_V"
echo "    最大迁移版本    本地 $LOCAL_MIG / 设备 $VERIFY_MIG"
# 逐条断言迁移名（md5 相同其实已经蕴含，但这里让「差在哪」可读）
missing=0
while IFS= read -r m; do
  [ -n "$m" ] || continue
  echo "    ✗ 设备上那版缺迁移 $m"; missing=1
done < <(comm -23 <(apk_so_known_migrations "$SO_L") <(apk_so_known_migrations "$SO_V"))

if [ "$MD5_L" != "$MD5_V" ] || [ "$LOCAL_MIG" != "$VERIFY_MIG" ] || [ "$missing" = 1 ]; then
  apk_die "装上去的不是这个 APK（md5/迁移对不上）。别继续，先查 references/runbook.md「复核失败」。
       （最可能：手机上有另一个包名相同、签名也相同的分身/克隆应用，或安装被系统的
         「安装后自动恢复旧版本」覆盖。）"
fi
echo "    ✓ 设备上运行的确实是这份 libelsewhen.so（md5 与迁移名逐项一致）"

apk_say "设备状态"
apk_adb shell dumpsys package "$APK_APP_ID" | tr -d '\r' \
  | grep -E "versionName=|versionCode=|lastUpdateTime=|firstInstallTime=" | sed 's/^ */    /' | head -4

if [ "$RELAUNCH" = 1 ]; then
  apk_say "冷启动"
  apk_adb shell am force-stop "$APK_APP_ID" >/dev/null 2>&1 || true
  # am start 用 monkey 之外更明确的方式；MainActivity 是唯一 LAUNCHER。
  apk_adb shell monkey -p "$APK_APP_ID" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1 \
    || apk_adb shell am start -n "$APK_APP_ID/.MainActivity" >/dev/null 2>&1 \
    || echo "    （启动失败，手动点开看看）"
  sleep 6
  PID="$(apk_adb shell pidof "$APK_APP_ID" 2>/dev/null | tr -d '\r' || true)"
  if [ -n "$PID" ]; then
    echo "    ✓ 进程在跑（pid $PID）"
  else
    echo "    ✗ 进程没起来 —— 首次启动迁移失败是最可能的原因。取证看："
    echo "        adb -s $APK_DEVICE logcat -d | grep -iE 'elsewhen|migration|panic' | tail -40"
    exit 1
  fi
fi

echo
echo "✓ 更新完成：$APK_APP_ID @ $APK_DEVICE"
echo "  手机首次启动会自己把本地库迁到 v$LOCAL_MIG（只追加，不会回退）。"
