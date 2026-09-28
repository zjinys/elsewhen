#!/usr/bin/env bash
# 一键编译 Android APK：先编 Rust 核心（arm64），再 flutter build apk。
#
# 用法：
#   ./scripts/build-apk.sh              # release APK
#   ./scripts/build-apk.sh --debug      # debug APK
#   ./scripts/build-apk.sh --split-per-abi / 其他 flutter build apk 参数原样透传
set -euo pipefail

cd "$(dirname "$0")/.."

FLUTTER_ARGS=("$@")
BUILD_MODE=release
for arg in "$@"; do
  case "$arg" in
    --debug|--profile) BUILD_MODE="${arg#--}" ;;
  esac
done

echo "==> [1/2] 编译 Rust 核心 (android arm64)"
./scripts/build-android-rust.sh

echo "==> [2/2] flutter build apk (仅 android-arm64)"
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
