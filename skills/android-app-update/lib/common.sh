#!/usr/bin/env bash
# 公共部分：读配置、挑设备、找 apksigner、解析 APK。
# 用法：source 本文件后可直接用 apk_pick_device / apk_apksigner / apk_info ...
# 本文件不含任何常量（设备序列号、包名、证书指纹全部来自 config.env）。

set -euo pipefail

APK_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APK_SKILL_DIR="$(cd "$APK_LIB_DIR/.." && pwd)"
APK_REPO_ROOT="$(cd "$APK_SKILL_DIR/../.." && pwd)"

apk_die() { printf '\n✗ %s\n' "$*" >&2; exit 1; }
apk_say() { printf '\n── %s\n' "$*"; }

# 纯 shell 子串判断。**不要**写成 `cmd | grep -q`：grep -q 命中即退出，
# 上游（strings/printf）收到 SIGPIPE，配合 `set -o pipefail` 整条管道会被判
# 失败 —— 于是「明明存在」被读成「不存在」。这个坑在本 skill 里踩过：
# 迁移名用 `strings | grep -qF` 判断，一直返回未命中。
apk_substr() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac; }

# ── 配置 ─────────────────────────────────────────────────────────────────
# 优先级：环境变量 > config.local.env > config.env。
# 必须先把环境变量摘出来：`. config.env` 是**无条件赋值**，配置文件里的
# `APK_DEVICE=`（空值）会直接把 `APK_DEVICE=xxx ./install.sh` 覆盖掉 ——
# 那样「用环境变量指定设备」就是个假承诺（这在本 skill 里踩过一次：
# APK_DEVICE=bogus 本该报错，结果照样装到真机上）。
_apk_env_APK_DEVICE="${APK_DEVICE:-}"
_apk_env_APK_APP_ID="${APK_APP_ID:-}"
_apk_env_APK_EXPECTED_CERT_SHA256="${APK_EXPECTED_CERT_SHA256:-}"
_apk_env_APK_BUILD_ARGS="${APK_BUILD_ARGS:-}"
_apk_env_APK_FLUTTER_CMD="${APK_FLUTTER_CMD:-}"
_apk_env_APK_ALWAYS_BUILD="${APK_ALWAYS_BUILD:-}"
_apk_env_APK_ANDROID_SDK="${APK_ANDROID_SDK:-}"

APK_CONFIG="$APK_SKILL_DIR/config.env"
[ -f "$APK_CONFIG" ] || apk_die "缺配置文件：$APK_CONFIG"
# shellcheck source=/dev/null
. "$APK_CONFIG"
if [ -f "$APK_SKILL_DIR/config.local.env" ]; then
  # shellcheck source=/dev/null
  . "$APK_SKILL_DIR/config.local.env"
fi

[ -n "$_apk_env_APK_DEVICE" ] && APK_DEVICE="$_apk_env_APK_DEVICE"
[ -n "$_apk_env_APK_APP_ID" ] && APK_APP_ID="$_apk_env_APK_APP_ID"
[ -n "$_apk_env_APK_EXPECTED_CERT_SHA256" ] && APK_EXPECTED_CERT_SHA256="$_apk_env_APK_EXPECTED_CERT_SHA256"
[ -n "$_apk_env_APK_BUILD_ARGS" ] && APK_BUILD_ARGS="$_apk_env_APK_BUILD_ARGS"
[ -n "$_apk_env_APK_FLUTTER_CMD" ] && APK_FLUTTER_CMD="$_apk_env_APK_FLUTTER_CMD"
[ -n "$_apk_env_APK_ALWAYS_BUILD" ] && APK_ALWAYS_BUILD="$_apk_env_APK_ALWAYS_BUILD"
[ -n "$_apk_env_APK_ANDROID_SDK" ] && APK_ANDROID_SDK="$_apk_env_APK_ANDROID_SDK"

: "${APK_APP_ID:?config.env 缺 APK_APP_ID}"
APK_DEVICE="${APK_DEVICE:-}"
APK_EXPECTED_CERT_SHA256="${APK_EXPECTED_CERT_SHA256:-}"
APK_BUILD_ARGS="${APK_BUILD_ARGS:-}"
APK_FLUTTER_CMD="${APK_FLUTTER_CMD:-fvm flutter}"
APK_ALWAYS_BUILD="${APK_ALWAYS_BUILD:-1}"
APK_ANDROID_SDK="${APK_ANDROID_SDK:-}"

APK_APK_DIR="$APK_REPO_ROOT/ui/build/app/outputs/flutter-apk"
APK_MAIN_APK="$APK_APK_DIR/app-release.apk"

# ── 工具：adb / 设备 ──────────────────────────────────────────────────────
apk_adb() { adb -s "$APK_DEVICE" "$@"; }

# 选定唯一目标设备。多设备且未显式配置时必须报错，不能猜 —— 猜错就是把
# 别人的 APK 装到另一台机器上。
apk_pick_device() {
  command -v adb >/dev/null 2>&1 || apk_die "找不到 adb（Android platform-tools 不在 PATH 里）"
  local devices
  devices="$(adb devices | awk 'NR>1 && NF>=2 && $2=="device" {print $1}')"
  local want_found=0 d
  if [ -n "$APK_DEVICE" ]; then
    while IFS= read -r d; do
      [ "$d" = "$APK_DEVICE" ] && want_found=1
    done <<< "$devices"
    if [ "$want_found" = 0 ]; then
      printf '%s\n' "$devices" | sed 's/^/    /' >&2
      apk_die "配置里指定的设备 '$APK_DEVICE' 不在 adb 设备列表（上面是当前可用设备）。
       刚插上/刚连无线时先跑一次 \`adb devices\`；unauthorized 时先在手机上点「允许调试」。"
    fi
  else
    local count=0
    while IFS= read -r d; do [ -n "$d" ] && count=$((count+1)); done <<< "$devices"
    [ "$count" -ge 1 ] || apk_die "adb 上看不到设备。手机没插、没开无线调试，或驱动/授权没通过。
       `adb devices` 目前输出：
$(adb devices | sed 's/^/        /')"
    [ "$count" -eq 1 ] || apk_die "adb 上有 $count 台设备，必须显式指定一台：
       在 skills/android-app-update/config.env 里设 APK_DEVICE，或用环境变量
       APK_DEVICE=<serial> ./skill 脚本 ...
       当前可用：
$(printf '%s\n' "$devices" | sed 's/^/        /')"
    APK_DEVICE="$devices"
  fi
  export APK_DEVICE
  apk_say "目标设备：$APK_DEVICE"
}

# ── 工具：apksigner ───────────────────────────────────────────────────────
apk_apksigner() {
  if [ -n "${APK_APKSIGNER:-}" ] && [ -x "${APK_APKSIGNER:-}" ]; then
    printf '%s\n' "$APK_APKSIGNER"; return
  fi
  local candidates=()
  local sdk_root=""
  if [ -n "$APK_ANDROID_SDK" ]; then
    sdk_root="$APK_ANDROID_SDK"
  elif [ -n "${ANDROID_HOME:-}" ]; then
    sdk_root="$ANDROID_HOME"
  elif [ -n "${ANDROID_SDK_ROOT:-}" ]; then
    sdk_root="$ANDROID_SDK_ROOT"
  elif [ -d "$HOME/Android/Sdk" ]; then
    sdk_root="$HOME/Android/Sdk"
  elif [ -d "$HOME/Android/sdk" ]; then
    sdk_root="$HOME/Android/sdk"
  fi
  if [ -n "$sdk_root" ] && [ -d "$sdk_root/build-tools" ]; then
    # 版本号排序取最高（build-tools/36.1.0-rc1 这类带后缀的排在前面也认，
    # 因为同一目录里不会有多个同号变体）。
    while IFS= read -r d; do candidates+=("$d/apksigner"); done \
      < <(ls -1d "$sdk_root"/build-tools/*/ 2>/dev/null | sort -Vr)
  fi
  local c
  for c in "${candidates[@]}"; do
    [ -x "$c" ] && { printf '%s\n' "$c"; return; }
  done
  apk_die "找不到 apksigner。装上 Android build-tools，或在
      skills/android-app-update/config.env 里设 APK_ANDROID_SDK=<SDK 根目录>，
      或用环境变量 APK_APKSIGNER=/path/to/apksigner 直接指定。"
}

# 取 APK 的签名证书 SHA-256（小写、无冒号）。失败时打印原因并退出。
apk_cert_sha256() {
  local apk="$1" signer out
  [ -f "$apk" ] || apk_die "APK 不存在：$apk"
  signer="$(apk_apksigner)"
  out="$("$signer" verify --print-certs "$apk" 2>&1)" || {
    printf '%s\n' "$out" >&2
    apk_die "apksigner 验不了 $apk（APK 损坏、或它根本没有签名 —— 未签名的构建装不上手机）"
  }
  printf '%s\n' "$out" | sed -n 's/^Signer #1 certificate SHA-256 digest: //p' | head -1 | tr 'A-Z' 'a-z'
}

# ── 工具：APK 自省 ───────────────────────────────────────────────────────
# 迁移名会以字符串常量形式嵌进 libelsewhen.so（rusqlite_migration 的
# from-directory 把 migrations/ 编进库）。所以「这个 APK 认得哪些迁移」
# 可以在不装、不解包 libapp.so 的情况下从二进制里读出来 —— 这是本 skill
# 判断「该不该更新」的硬证据来源。
#
# ⚠ 不要拿裸正则 `^[0-9]{2}-[a-z-]+$` 去 grep 二进制：`strings` 里到处是
# 恰好长成 `67-s`、`35-w` 的杂串（`strings` 会把任意 ≥4 字符的可打印序列
# 切出来），照那样取到的「最大版本」是垃圾。唯一可信的集合是仓库
# migrations/ 目录本身，所以这里只认清单里的完整名字。
APK_REPO_MIGRATIONS_CACHE=""

apk_repo_migrations() {
  # 输出仓库全部迁移目录名，按版本号升序。
  [ -n "$APK_REPO_MIGRATIONS_CACHE" ] ||     APK_REPO_MIGRATIONS_CACHE="$(ls -1 "$APK_REPO_ROOT/migrations" 2>/dev/null       | grep -E '^[0-9]{2}-' | sort -V)"
  printf '%s
' "$APK_REPO_MIGRATIONS_CACHE"
}

apk_repo_latest_migration() { apk_repo_migrations | tail -1; }

# 把 APK 里的 libelsewhen.so 取出来（调用方负责清理 tmp）。
apk_extract_so() {
  local apk="$1" into="$2"
  unzip -q -o "$apk" 'lib/arm64-v8a/libelsewhen.so' -d "$into" 2>/dev/null     || apk_die "APK 里没有 lib/arm64-v8a/libelsewhen.so：$apk"
  printf '%s/lib/arm64-v8a/libelsewhen.so\n' "$into"
}

# 一个 .so 里是否含有某个迁移名。
# 用「子串」而不是整行匹配：`strings` 切出来的是连续可打印串，迁移名通常
# 嵌在更长的一行里（例如 `...02-goals...` 这种连排常量池），整行相等会漏。
# 反过来「子串命中的一定不是巧合」——名字带版本号前缀，够长够特指。
apk_so_has() {
  local all
  all="$(strings "$1")"
  apk_substr "$all" "$2"
}

# 「这个 APK 认得仓库清单里的哪些迁移」——按版本升序的全部命中项。
# 取一次 strings 再逐名比对，避免对同一个 .so 反复解压/扫描。
apk_so_known_migrations() {
  local so="$1" m all
  all="$(strings "$so")"
  while IFS= read -r m; do
    [ -n "$m" ] || continue
    apk_substr "$all" "$m" && printf '%s\n' "$m"
  done < <(apk_repo_migrations)
}

# 该 APK 认得的最高迁移（按仓库清单）。清单里一个都没有时输出空。
apk_max_migration() {
  local apk="$1" tmp so
  tmp="$(mktemp -d)"
  so="$(apk_extract_so "$apk" "$tmp")"
  apk_so_known_migrations "$so" | tail -1
  rm -rf "$tmp"
}

# 兼容旧名（install.sh 里用它做过断言）。
apk_has_migration() { apk_so_has "$1" "$2"; }

# APK 的 versionName（解析 aapt2 太脆弱，直接读 AndroidManifest 的二进制
# 字符串池：versionName 会以明文字符串出现，配合 versionCode 那行 dumpsys 用）。
apk_version_info() {
  local apk="$1" tmp
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' RETURN
  unzip -q -o "$apk" 'AndroidManifest.xml' -d "$tmp" 2>/dev/null || true
  aapt2_dump "$apk"
}

# 优先用 aapt2 读（准确）；没有就退回 dumpsys（需要设备上已装该包）。
aapt2_dump() {
  local apk="$1" aapt2 sdk_root=""
  if [ -n "${APK_ANDROID_SDK:-}" ]; then sdk_root="$APK_ANDROID_SDK"
  elif [ -n "${ANDROID_HOME:-}" ]; then sdk_root="$ANDROID_HOME"
  elif [ -d "$HOME/Android/Sdk" ]; then sdk_root="$HOME/Android/Sdk"; fi
  if [ -n "$sdk_root" ]; then
    aapt2="$(ls -1d "$sdk_root"/build-tools/*/aapt2 2>/dev/null | sort -Vr | head -1 || true)"
  fi
  if [ -n "${aapt2:-}" ] && [ -x "$aapt2" ]; then
    "$aapt2" dump badging "$apk" 2>/dev/null \
      | sed -n 's/^package: name=\([^ ]*\).*versionCode=\([^ ]*\).*versionName=\([^ ]*\).*/\1 versionCode=\2 versionName=\3/p'
  else
    printf '（没找到 aapt2，跳过 versionName 解析）\n'
  fi
}

# ── 工具：仓库侧信息 ─────────────────────────────────────────────────────
apk_repo_bridge_hash() {
  local dart="$APK_REPO_ROOT/ui/lib/bridge/generated.dart/frb_generated.dart"
  local rust="$APK_REPO_ROOT/src/frb_generated.rs"
  local d r
  # 注意 `-?`：content hash 是 i32，负数很常见。
  d="$(grep -oE 'rustContentHash => -?[0-9]+' "$dart" 2>/dev/null | grep -oE -- '-?[0-9]+$' | head -1 || true)"
  r="$(grep -oE 'FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH: i32 = -?[0-9]+' "$rust" 2>/dev/null | grep -oE -- '-?[0-9]+$' | head -1 || true)"
  printf '%s %s\n' "$d" "$r"
}

apk_repo_head() {
  git -C "$APK_REPO_ROOT" log --oneline -1 2>/dev/null || printf '（不是 git 仓库）\n'
}

# 仓库里有没有还没提交/未跟踪的改动。用于提示「这个 APK 对应的不是任何一次提交」。
apk_repo_dirty() {
  git -C "$APK_REPO_ROOT" status --porcelain 2>/dev/null | grep -v '^?? skills/android-app-update/' | head -20
}
