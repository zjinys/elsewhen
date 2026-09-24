#!/usr/bin/env sh
set -eu
version="${ELSEWHEN_VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)}"
out_dir="${1:-dist}"
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
case "$out_dir" in /*) output_dir="$out_dir";; *) output_dir="$root/$out_dir";; esac
command -v rpmbuild >/dev/null 2>&1 || { echo "rpmbuild is required" >&2; exit 1; }
mkdir -p "$output_dir"
if [ -x "$root/ui/.fvm/flutter_sdk/bin/flutter" ]; then flutter="$root/ui/.fvm/flutter_sdk/bin/flutter"; else flutter="$(command -v flutter || { echo 'flutter is required' >&2; exit 1; })"; fi
(cd "$root" && cargo build --release)
(cd "$root/ui" && "$flutter" build linux --release)
bundle="$root/ui/build/linux/x64/release/bundle"
[ -x "$bundle/elsewhen_ui" ] || { echo "未找到 $bundle/elsewhen_ui" >&2; exit 1; }
cp "$root/target/release/libelsewhen.so" "$bundle/lib/"
top="$(mktemp -d)"; trap 'rm -rf "$top"' EXIT
mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
sed "s/@VERSION@/$version/g; s|@ROOT@|$root|g" "$root/packaging/linux/elsewhen.spec.in" > "$top/SPECS/elsewhen.spec"
rpmbuild --define "_topdir $top" -bb "$top/SPECS/elsewhen.spec" >/dev/null
find "$top/RPMS" -name '*.rpm' -exec cp {} "$output_dir/" \;
echo "Created RPM in $output_dir"
