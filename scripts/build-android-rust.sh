#!/usr/bin/env bash
# 编译 Rust 核心为 Android arm64 的 libelsewhen.so，放入 ui/rustLibs/arm64-v8a/
# 供 android/app/build.gradle.kts 的 jniLibs.srcDir 打进 APK。
#
# 依赖：
#   rustup target add aarch64-linux-android
#   Android SDK/NDK（ANDROID_HOME 或 ANDROID_SDK_ROOT，ndk 在 $ANDROID_HOME/ndk/<ver>/）
set -euo pipefail

cd "$(dirname "$0")/.."

ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
# 取已安装的最新 NDK
NDK_HOME="${ANDROID_NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)}"
if [[ -z "$NDK_HOME" || ! -d "$NDK_HOME" ]]; then
  echo "error: 找不到 Android NDK（$ANDROID_HOME/ndk/）。请用 SDK Manager 安装 NDK。" >&2
  exit 1
fi

# NDK clang 包装脚本所在目录（host tag 支持 linux/darwin）
HOST_TAG="$(uname -s | tr '[:upper:]' '[:lower:]')-x86_64"
TOOLCHAIN_BIN="$NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"
if [[ ! -d "$TOOLCHAIN_BIN" ]]; then
  echo "error: 找不到 NDK toolchain：$TOOLCHAIN_BIN" >&2
  exit 1
fi

# minSdk 21（flutter.minSdkVersion）；NDK r25+ 的 clang 前缀直接用 API level
API=21
TRIPLE=aarch64-linux-android

echo "==> 编译 arm64-v8a ($TRIPLE)"

# 配置 linker + ar（cargo 环境变量覆盖，不污染全局 config）
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN_BIN/${TRIPLE}${API}-clang"
export AR_aarch64_linux_android="$TOOLCHAIN_BIN/llvm-ar"
export CC_aarch64_linux_android="$TOOLCHAIN_BIN/${TRIPLE}${API}-clang"

cargo build --release --target "$TRIPLE"

mkdir -p ui/rustLibs/arm64-v8a
cp "target/$TRIPLE/release/libelsewhen.so" ui/rustLibs/arm64-v8a/
"$TOOLCHAIN_BIN/llvm-strip" ui/rustLibs/arm64-v8a/libelsewhen.so

echo "==> 完成："
ls -lh ui/rustLibs/arm64-v8a/libelsewhen.so
