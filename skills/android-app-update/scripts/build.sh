#!/usr/bin/env bash
# 编译 Android APK（release，仅 arm64-v8a），并打印「这次编出来的到底是什么」。
#
# 本脚本不重复实现构建逻辑：它调用仓库自己的 scripts/build-apk.sh
#   —— 那里面有桥接 hash 预检（改了 Rust 公开 API 却没 regen 时直接拦下）、
#      Rust arm64 编译、以及 flutter build apk。
# 这里只加两件事：把 extra 配置透传过去，以及把构建产物与仓库状态记下来。
#
# 用法：
#   ./skills/android-app-update/scripts/build.sh
#   APK_BUILD_ARGS="--debug" ./skills/android-app-update/scripts/build.sh
#   ./skills/android-app-update/scripts/build.sh --no-fvm   # 用系统 flutter（临时）
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/common.sh
. "$HERE/../lib/common.sh"

BUILD_ARGS=()
for arg in "$@"; do
  case "$arg" in
    --no-fvm) APK_FLUTTER_CMD=flutter ;;   # 只影响 build-apk.sh 里的 fvm 调用
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) BUILD_ARGS+=("$arg") ;;
  esac
done
if [ -n "$APK_BUILD_ARGS" ]; then
  # shellcheck disable=SC2206
  BUILD_ARGS+=($APK_BUILD_ARGS)
fi

# 构建前先看仓库状态：APK 对应的提交不明确时，后面「设备上是不是这份代码」
# 就没法回答。这里只提示，不阻断（本地验证构建很正常）。
DIRTY="$(apk_repo_dirty)"
if [ -n "$DIRTY" ]; then
  apk_say "注意：仓库有未提交改动，这个 APK 不对应任何一次提交"
  printf '%s\n' "$DIRTY" | sed 's/^/    /'
fi

SKIP=0
if [ "$APK_ALWAYS_BUILD" != 1 ] && [ -f "$APK_MAIN_APK" ]; then
  # 已有 APK 比构建脚本更新 ⇒ 认为它已经包含当前构建逻辑。
  if [ "$APK_MAIN_APK" -nt "$APK_REPO_ROOT/scripts/build-apk.sh" ]; then
    SKIP=1
    apk_say "已有更新的 APK，跳过构建（APK_ALWAYS_BUILD=0）"
  fi
fi

if [ "$SKIP" != 1 ]; then
  apk_say "构建（scripts/build-apk.sh ${BUILD_ARGS[*]:-}）"
  echo "    桥接 hash：$(apk_repo_bridge_hash)"
  echo "    仓库 HEAD：$(apk_repo_head)"
  # 让 build-apk.sh 用配置里指定的 flutter 命令（它自己写死 fvm flutter）。
  if [ "$APK_FLUTTER_CMD" != "fvm flutter" ]; then
    echo "    ⚠ 注意：scripts/build-apk.sh 内部固定用 fvm flutter；"
    echo "      APK_FLUTTER_CMD=$APK_FLUTTER_CMD 只在本脚本这条路径上生效。"
  fi
  # `set -- "${BUILD_ARGS[@]:-}"` 是错的：空数组会展开出**一个空字符串参数**，
  # 于是 build-apk.sh 收到 "" 并把它透传给 flutter，表现为
  # `Target file "" not found.`（构建在一个完全无关的报错上卡住）。
  # `${arr[@]+...}` 只在真的非空时才展开。
  set -- ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}
  if [ $# -gt 0 ]; then
    "$APK_REPO_ROOT/scripts/build-apk.sh" "$@"
  else
    "$APK_REPO_ROOT/scripts/build-apk.sh"
  fi
fi

[ -f "$APK_MAIN_APK" ] || apk_die "构建结束但找不到 $APK_MAIN_APK"

apk_say "产物"
ls -lh "$APK_MAIN_APK"
echo
echo "    最大迁移版本：$(apk_max_migration "$APK_MAIN_APK")（仓库：$(apk_repo_latest_migration)）"
echo "    APK 整包 md5：$(md5sum "$APK_MAIN_APK" | cut -d' ' -f1)"
echo "    libelsewhen.so md5：$(TMPD="$(mktemp -d)"; SO="$(apk_extract_so "$APK_MAIN_APK" "$TMPD")"; md5sum "$SO" | cut -d' ' -f1; rm -rf "$TMPD")"
echo "    签名证书：$(apk_cert_sha256 "$APK_MAIN_APK")"
