#!/usr/bin/env sh
# package-appimage.sh - 把 Flutter UI 桌面端打成 Linux AppImage
#
# 产物：dist/Elsewhen-<version>-x86_64.AppImage
#
# 依赖：cargo、flutter（ui/.fvm 优先）、appimagetool（缺失时自动下载到缓存目录）、
#       FUSE2（无 FUSE 的环境自动退回 APPIMAGE_EXTRACT_AND_RUN=1）。
#
# 用法：./scripts/package-appimage.sh [输出目录，默认 dist]
set -eu

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

echo "[1/4] 构建 Rust 桥接库 (cargo build --release) ..."
(cd "$root" && cargo build --release)

echo "[2/4] 构建 Flutter Linux release ..."
(cd "$root/ui" && "$flutter" build linux --release)

bundle="$root/ui/build/linux/x64/release/bundle"
[ -x "$bundle/elsewhen_ui" ] || { echo "未找到 $bundle/elsewhen_ui" >&2; exit 1; }

# frb 默认按裸名 dlopen('libelsewhen.so')（glibc 搜 RPATH/LD_LIBRARY_PATH）：
# 把桥接库放进 bundle/lib/，AppRun 再兜一层 LD_LIBRARY_PATH。
cp "$root/target/release/libelsewhen.so" "$bundle/lib/"

echo "[3/4] 组装 AppDir ..."
stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
appdir="$stage/Elsewhen.AppDir"
mkdir -p "$appdir/usr/bin"
cp -r "$bundle/." "$appdir/usr/bin/"

cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
APPDIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export LD_LIBRARY_PATH="$APPDIR/usr/bin/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$APPDIR/usr/bin/elsewhen_ui" "$@"
EOF
chmod +x "$appdir/AppRun"

cat > "$appdir/elsewhen.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Elsewhen
Comment=Local-first personal event recorder with global hotkey capture
Exec=elsewhen_ui
Icon=elsewhen
Terminal=false
Categories=Utility;
StartupNotify=true
StartupWMClass=io.github.elsewhen.Elsewhen
EOF

icon="$root/assets/brand/elsewhen-icon-v2-256.png"
[ -f "$icon" ] || { echo "缺少图标 $icon" >&2; exit 1; }
cp "$icon" "$appdir/elsewhen.png"
ln -sf elsewhen.png "$appdir/.DirIcon"

echo "[4/4] appimagetool 打包 ..."
cache="${XDG_CACHE_HOME:-$HOME/.cache}/elsewhen"
tool="$cache/appimagetool-x86_64.AppImage"
if command -v appimagetool >/dev/null 2>&1; then
  tool="$(command -v appimagetool)"
elif [ ! -x "$tool" ]; then
  mkdir -p "$cache"
  echo "     下载 appimagetool → $tool"
  curl -fL --retry 2 -o "$tool" \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
  chmod +x "$tool"
fi

out="$output_dir/Elsewhen-${version}-x86_64.AppImage"
rm -f "$out"
# 容器/无 FUSE 环境：解包运行 appimagetool（不影响产物格式）。
if [ -e /dev/fuse ]; then
  ARCH=x86_64 "$tool" "$appdir" "$out" >/dev/null
else
  ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" "$appdir" "$out" >/dev/null
fi
[ -f "$out" ] || { echo "appimagetool 打包失败" >&2; exit 1; }
echo "Created $out"
