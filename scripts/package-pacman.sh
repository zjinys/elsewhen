#!/usr/bin/env sh
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
out_dir="${1:-dist}"
case "$out_dir" in /*) output_dir="$out_dir";; *) output_dir="$root/$out_dir";; esac
command -v makepkg >/dev/null 2>&1 || { echo "makepkg is required" >&2; exit 1; }
if [ -x "$root/ui/.fvm/flutter_sdk/bin/flutter" ]; then flutter="$root/ui/.fvm/flutter_sdk/bin/flutter"; else flutter="$(command -v flutter || { echo 'flutter is required' >&2; exit 1; })"; fi
(cd "$root" && cargo build --release)
(cd "$root/ui" && "$flutter" build linux --release)
bundle="$root/ui/build/linux/x64/release/bundle"
[ -x "$bundle/elsewhen_ui" ] || { echo "未找到 $bundle/elsewhen_ui" >&2; exit 1; }
cp "$root/target/release/libelsewhen.so" "$bundle/lib/"
mkdir -p "$output_dir"
cd "$root/packaging/linux"
PKGDEST="$output_dir" makepkg -C
