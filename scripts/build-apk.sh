#!/usr/bin/env bash
# 一键编译 Android APK：先校验桥接 hash，再编 Rust 核心（arm64），最后 flutter build apk。
#
# 用法：
#   ./scripts/build-apk.sh              # release APK
#   ./scripts/build-apk.sh --debug      # debug APK
#   ./scripts/build-apk.sh --split-per-abi / 其他 flutter build apk 参数原样透传
#
# 注意：本脚本不跑 regen.sh。改了 Rust 公开 API 时 hash 预检会直接报错，
# 按提示先跑 ./scripts/regen.sh 即可。
set -euo pipefail

cd "$(dirname "$0")/.."

# ---------- [0/3] 桥接代码 hash 预检 ----------
# 改了 Rust 公开 API 却只跑本脚本时，Dart 侧桥接代码还是旧的：编译能过，
# 装到手机上要到 RustLib.init() 才报 hash mismatch（且报错现象与根因相距
# 很远）。这里提前拦下，指向 regen.sh。
DART_FRB="ui/lib/bridge/generated.dart/frb_generated.dart"
RUST_FRB="src/frb_generated.rs"
# 注意 `-?`：frb 的 content hash 是 i32，负数很常见（当前就是 -854730017）。
# 原来的 `[0-9]+` 会把 `=> -854730017` 里 `854730017` 之后的位置匹配不上/只取绝对值，
# 表现为「读不到桥接 hash」，于是不管桥对不对都直接退出，这个预检从来没真正放行过负 hash 的仓库。
DART_HASH="$(grep -oE 'rustContentHash => -?[0-9]+' "$DART_FRB" 2>/dev/null | grep -oE -- '-?[0-9]+$' | head -1 || true)"
RUST_HASH="$(grep -oE 'FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH: i32 = -?[0-9]+' "$RUST_FRB" 2>/dev/null | grep -oE -- '-?[0-9]+$' | head -1 || true)"
if [[ -z "$DART_HASH" || -z "$RUST_HASH" ]]; then
  echo "error: 读不到桥接 hash（Dart='${DART_HASH}' Rust='${RUST_HASH}'）。" >&2
  echo "       先跑 ./scripts/regen.sh 重新生成两侧桥接代码。" >&2
  exit 1
fi
if [[ "$DART_HASH" != "$RUST_HASH" ]]; then
  cat >&2 <<EOF
error: 桥接代码与 Rust 不一致（Dart rustContentHash=$DART_HASH，Rust=$RUST_HASH）。
       改了 Rust 公开 API 后必须先重生成两侧桥接，再重跑本脚本：
         ./scripts/regen.sh
       否则 APK 装上去会在 RustLib.init() 报 hash mismatch。
EOF
  exit 1
fi
echo "==> [0/3] 桥接 hash 一致（$RUST_HASH）"

FLUTTER_ARGS=("$@")
BUILD_MODE=release
for arg in "$@"; do
  case "$arg" in
    --debug|--profile) BUILD_MODE="${arg#--}" ;;
  esac
done

echo "==> [1/3] 编译 Rust 核心 (android arm64)"
./scripts/build-android-rust.sh

echo "==> [2/3] flutter build apk (仅 android-arm64)"
cd ui
fvm flutter pub get
# 只编 arm64：Rust 核心只提供 arm64-v8a 的 libelsewhen.so，且裁掉多余 ABI
# 可把 APK 从 ~72MB 降到 ~31MB。main.dart 是默认目标（无 nativeapi，避免
# Android release AOT 崩溃），这里无需 --target。
DEFAULT_ARGS=(--release --target-platform android-arm64)
if [[ ${#FLUTTER_ARGS[@]} -gt 0 ]]; then
  fvm flutter build apk --target-platform android-arm64 "${FLUTTER_ARGS[@]}"
else
  fvm flutter build apk "${DEFAULT_ARGS[@]}"
fi

echo
echo "==> 完成，APK 位置："
find build/app/outputs/flutter-apk -name "*.apk" -newer ../scripts/build-apk.sh -exec ls -lh {} \; 2>/dev/null \
  || ls -lh build/app/outputs/flutter-apk/*.apk 2>/dev/null || true
