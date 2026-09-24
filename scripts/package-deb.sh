#!/usr/bin/env sh
set -eu
version="${ELSEWHEN_VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)}"
out_dir="${1:-dist}"
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
case "$out_dir" in /*) output_dir="$out_dir";; *) output_dir="$root/$out_dir";; esac
command -v dpkg-deb >/dev/null 2>&1 || { echo "dpkg-deb is required" >&2; exit 1; }
mkdir -p "$output_dir"
if [ -x "$root/ui/.fvm/flutter_sdk/bin/flutter" ]; then flutter="$root/ui/.fvm/flutter_sdk/bin/flutter"; else flutter="$(command -v flutter || { echo 'flutter is required' >&2; exit 1; })"; fi
(cd "$root" && cargo build --release)
(cd "$root/ui" && "$flutter" build linux --release)
bundle="$root/ui/build/linux/x64/release/bundle"
[ -x "$bundle/elsewhen_ui" ] || { echo "未找到 $bundle/elsewhen_ui" >&2; exit 1; }
cp "$root/target/release/libelsewhen.so" "$bundle/lib/"
stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/DEBIAN" "$stage/usr/bin" "$stage/usr/lib/elsewhen" "$stage/usr/share/applications" "$stage/usr/share/icons/hicolor/256x256/apps"
cp -a "$bundle/." "$stage/usr/lib/elsewhen/"
printf '%s\n' '#!/bin/sh' 'export LD_LIBRARY_PATH="/usr/lib/elsewhen/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"' 'exec /usr/lib/elsewhen/elsewhen_ui "$@"' > "$stage/usr/bin/elsewhen"
chmod 0755 "$stage/usr/bin/elsewhen"
install -m 0644 "$root/packaging/linux/io.github.elsewhen.Elsewhen.desktop" "$stage/usr/share/applications/"
if [ -f "$root/assets/brand/elsewhen-icon-v2-256.png" ]; then
  install -m 0644 "$root/assets/brand/elsewhen-icon-v2-256.png" "$stage/usr/share/icons/hicolor/256x256/apps/io.github.elsewhen.Elsewhen.png"
fi
cat > "$stage/DEBIAN/control" <<EOF
Package: elsewhen
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Maintainer: Elsewhen contributors
Description: Local-first personal event recorder with global hotkey capture
EOF
dpkg-deb --root-owner-group --build "$stage" "$output_dir/elsewhen_${version}_amd64.deb" >/dev/null
echo "Created $output_dir/elsewhen_${version}_amd64.deb"
