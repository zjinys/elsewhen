#!/bin/sh
# package-dmg.sh - 把 Flutter UI 桌面端打成 macOS DMG（需在 macOS 上运行）
#
# 产物：dist/Elsewhen-<version>.dmg
#
# 依赖：cargo、flutter（ui/.fvm 优先）、Xcode 命令行工具（sips/iconutil/codesign/hdiutil）。
#
# 签名：默认 ad-hoc（codesign --sign -），本机可用；对外分发请设置
#       CODESIGN_IDENTITY="Developer ID Application: ..." 并重跑，随后还需公证
#       （xcrun notarytool submit）。
#
# 用法：./scripts/package-dmg.sh [输出目录，默认 dist]
set -eu

[ "$(uname -s)" = "Darwin" ] || { echo "本脚本需在 macOS 上运行" >&2; exit 1; }

version="${ELSEWHEN_VERSION:-$(sed -n 's/^version = \"\([^\"]*\)\"/\1/p' Cargo.toml | head -n 1)}"
out_dir="${1:-dist}"
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
case "$out_dir" in /*) output_dir="$out_dir";; *) output_dir="$root/$out_dir";; esac
mkdir -p "$output_dir"

if [ -x "$root/ui/.fvm/flutter_sdk/bin/flutter" ]; then
  flutter="$root/ui/.fvm/flutter_sdk/bin/flutter"
else
  flutter="$(command -v flutter || { echo 'flutter 不在 PATH 且 ui/.fvm 不存在' >&2; exit 1; })"
fi

echo "[1/5] 构建 Rust 桥接库 (cargo build --release) ..."
(cd "$root" && cargo build --release)

echo "[2/5] 构建 Flutter macOS release ..."
(cd "$root/ui" && "$flutter" build macos --release)

app="$root/ui/build/macos/Build/Products/Release/Elsewhen.app"
[ -d "$app" ] || { echo "未找到 $app" >&2; exit 1; }

echo "[3/5] 放入 Rust 桥接库 + 品牌图标 ..."
# frb 在 macOS 的默认加载链（CWD 相对的 ioDirectory / *.framework）在 Finder
# 启动下找不到 dylib；rust_bridge_repository._bundledRustLibrary() 会显式从
# Contents/Frameworks/libelsewhen.dylib 加载，拷到这里即可。
mkdir -p "$app/Contents/Frameworks"
cp "$root/target/release/libelsewhen.dylib" "$app/Contents/Frameworks/"

# 用品牌 PNG 生成 AppIcon.icns（iconutil 为 macOS 自带工具）。
icon_src="$root/assets/brand/elsewhen-icon-v2-1024.png"
if [ -f "$icon_src" ]; then
  iconset="$(mktemp -d)/AppIcon.iconset"
  mkdir -p "$iconset"
  for spec in "16 16x16" "32 16x16@2x" "32 32x32" "64 32x32@2x" \
              "128 128x128" "256 128x128@2x" "256 256x256" "512 256x256@2x" \
              "512 512x512" "1024 512x512@2x"; do
    set -- $spec
    sips -z "$1" "$1" "$icon_src" --out "$iconset/icon_$2.png" >/dev/null
  done
  iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"
fi

echo "[4/5] 签名（${CODESIGN_IDENTITY:-ad-hoc}）..."
codesign --force --deep --sign "${CODESIGN_IDENTITY:--}" "$app"

echo "[5/5] hdiutil 制作 DMG ..."
stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"

out="$output_dir/Elsewhen-${version}.dmg"
rm -f "$out"
hdiutil create -volname "Elsewhen" -srcfolder "$stage" -ov -format UDZO "$out" >/dev/null
echo "Created $out"
