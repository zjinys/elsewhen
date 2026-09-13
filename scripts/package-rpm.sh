#!/usr/bin/env sh
set -eu
version="${ELSEWHEN_VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)}"
out_dir="${1:-dist}"
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
case "$out_dir" in /*) output_dir="$out_dir";; *) output_dir="$root/$out_dir";; esac
command -v rpmbuild >/dev/null 2>&1 || { echo "rpmbuild is required" >&2; exit 1; }
mkdir -p "$output_dir"; cargo build --release --bin elsewhen
top="$(mktemp -d)"; trap 'rm -rf "$top"' EXIT
mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
sed "s/@VERSION@/$version/g; s|@ROOT@|$root|g" "$root/packaging/linux/elsewhen.spec.in" > "$top/SPECS/elsewhen.spec"
rpmbuild --define "_topdir $top" -bb "$top/SPECS/elsewhen.spec" >/dev/null
find "$top/RPMS" -name '*.rpm' -exec cp {} "$output_dir/" \;
echo "Created RPM in $output_dir"
