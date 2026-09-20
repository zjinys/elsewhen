#!/usr/bin/env sh
set -eu

data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
desktop_dir="$data_dir/applications"
icon_dir="$data_dir/icons/hicolor/256x256/apps"

mkdir -p "$desktop_dir"
install -m 0644 packaging/linux/io.github.elsewhen.Elsewhen.desktop \
  "$desktop_dir/io.github.elsewhen.Elsewhen.desktop"

# Install the canonical v2 brand icon used by the desktop UI.
if [ -f assets/brand/elsewhen-icon-v2-256.png ]; then
  mkdir -p "$icon_dir"
  install -m 0644 assets/brand/elsewhen-icon-v2-256.png \
    "$icon_dir/io.github.elsewhen.Elsewhen.png"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$desktop_dir"
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$data_dir/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
  kbuildsycoca6 >/dev/null 2>&1 || true
elif command -v kbuildsycoca5 >/dev/null 2>&1; then
  kbuildsycoca5 >/dev/null 2>&1 || true
fi

echo "Installed Elsewhen desktop metadata in $data_dir"
