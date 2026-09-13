#!/usr/bin/env sh
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
out_dir="${1:-dist}"
case "$out_dir" in /*) output_dir="$out_dir";; *) output_dir="$root/$out_dir";; esac
command -v makepkg >/dev/null 2>&1 || { echo "makepkg is required" >&2; exit 1; }
cargo build --release --bin elsewhen
mkdir -p "$output_dir"
cd "$root/packaging/linux"
PKGDEST="$output_dir" makepkg -C
