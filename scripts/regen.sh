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

# ---------- 1. Clang gets its resource headers; C builds keep their own headers ----------
FRB_CLANG_INCLUDE=""
for p in /usr/lib/clang/*/include /usr/local/lib/clang/*/include; do
  if [ -f "$p/stdbool.h" ] && [ -f "$p/stddef.h" ]; then
    FRB_CLANG_INCLUDE="$p"
    break
  fi
done
if [ -z "$FRB_CLANG_INCLUDE" ]; then
  echo "ERROR: 找不到 clang 内置头文件。请安装 clang。" >&2
  exit 1
fi
FRB_SHIM_DIR="${TMPDIR:-/tmp}/elsewhen-frb-tools"
mkdir -p "$FRB_SHIM_DIR"
cat > "$FRB_SHIM_DIR/cc" <<'WRAPPER'
#!/bin/sh
exec env -u CPATH "$FRB_ORIGINAL_CC" "$@"
WRAPPER
cat > "$FRB_SHIM_DIR/cxx" <<'WRAPPER'
#!/bin/sh
exec env -u CPATH "$FRB_ORIGINAL_CXX" "$@"
WRAPPER
chmod +x "$FRB_SHIM_DIR/cc" "$FRB_SHIM_DIR/cxx"
export FRB_ORIGINAL_CC="${CC:-cc}"
export FRB_ORIGINAL_CXX="${CXX:-c++}"
export CC="$FRB_SHIM_DIR/cc" CXX="$FRB_SHIM_DIR/cxx"
export CPATH="$FRB_CLANG_INCLUDE${CPATH:+:$CPATH}"
echo "[1/3] Clang headers: $FRB_CLANG_INCLUDE; C compiler header paths isolated"

# ---------- 2. 重新生成 bridge 代码 ----------
echo "[2/3] flutter_rust_bridge_codegen generate ..."
flutter_rust_bridge_codegen generate
echo "     生成完成"

# ---------- 3. 重编 Rust release 库（保持 hash 一致） ----------
echo "[3/3] cargo build --release ..."
cargo build --release
echo ""
echo "✔ 完成: 生成的 Dart/Rust bridge 与 target/release/libelsewhen.so 已保持一致。"