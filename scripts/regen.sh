#!/bin/bash
# Regen.sh - flutter_rust_bridge 一键重新生成 + 重编 Rust release 库
#
# 用法: scripts/regen.sh （任何 cwd 均可，脚本自己切到仓库根目录）
#
# 背景（血泪史）:
# 1. codegen 依赖 ffigen/libclang 解析 C 系统头文件；如果找不到 stdbool.h，
#    ffigen 会把 cbindgen 的 `typedef bool (*DartPostCObjectFnType)(...)` 解析成
#    `typedef bool = ffi.NativeFunction<...>`, 直接遮蔽 Dart 的 bool 类型,
#    导致构建报错（本仓库曾踩过这个坑）。
#    解法: 把 clang 内置头文件目录放进 CPATH。
# 2. 重新生成 Dart 侧代码后, target/release/libelsewhen.so 里的
#    FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH 会与 Dart 侧的 rustContentHash
#    不一致, 运行时 RustLib.init() 直接报 hash mismatch。
#    解法: 重新 cargo build --release（脚本最后一步自动做）。
set -euo pipefail
cd "$(dirname "$0")/.."

# ---------- 1. 给 ffigen 准备 stdbool.h（隔离目录，只放这一个头文件） ----------
# 血泪史续：CPATH 若指向整个 clang 内置头文件目录，codegen 内部的
# `cargo expand` 会让 GCC 误用 clang 的 stddef.h（__has_feature 等 clang 专有
# 写法），导致 ring 等 C 依赖编译失败。ffigen 实际只需要 stdbool.h，
# 故只隔离复制这一个头文件：GCC 照常用自己的系统头，libclang 又能找到 bool 定义。
CLANG_STDBOOL=""
for p in /usr/lib/clang/*/include /usr/local/lib/clang/*/include; do
  if [ -f "$p/stdbool.h" ]; then
    CLANG_STDBOOL="$p/stdbool.h"
    break
  fi
done
if [ -z "$CLANG_STDBOOL" ]; then
  echo "ERROR: 找不到 clang 内置头文件 (stdbool.h)。请安装 clang: sudo apt install clang" >&2
  exit 1
fi
FRB_CPATH_SHIM="${TMPDIR:-/tmp}/elsewhen-frb-cpath"
mkdir -p "$FRB_CPATH_SHIM"
cp -u "$CLANG_STDBOOL" "$FRB_CPATH_SHIM/stdbool.h"
export CPATH="$FRB_CPATH_SHIM${CPATH:+:$CPATH}"
echo "[1/3] CPATH=$FRB_CPATH_SHIM (stdbool.h from $CLANG_STDBOOL)"

# ---------- 2. 重新生成 bridge 代码 ----------
echo "[2/3] flutter_rust_bridge_codegen generate ..."
flutter_rust_bridge_codegen generate
echo "     生成完成"

# ---------- 3. 重编 Rust release 库（保持 hash 一致） ----------
echo "[3/3] cargo build --release ..."
cargo build --release
echo ""
echo "✔ 完成: 生成的 Dart/Rust bridge 与 target/release/libelsewhen.so 已保持一致。"