#!/usr/bin/env bash
# Regenerate every committed brand raster from the shared vector sources.
#
# Sources:
#   assets/brand/elsewhen-icon-v2.svg          rounded dark tile (desktop, macOS, taskbar)
#   ui/assets/brand/elsewhen-icon-android.svg  full-bleed square (iOS, Android legacy)
#   ui/assets/brand/elsewhen-splash.svg        rounded tile fitted to Android 12's safe circle
#   ui/assets/brand/elsewhen-adaptive-foreground.svg  transparent mark (Android adaptive layer)
#
# Requires: rsvg-convert, ImageMagick (magick or convert).

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

svg="rsvg-convert"
rounded="assets/brand/elsewhen-icon-v2.svg"
square="ui/assets/brand/elsewhen-icon-android.svg"
splash="ui/assets/brand/elsewhen-splash.svg"
adaptive_fg="ui/assets/brand/elsewhen-adaptive-foreground.svg"

if ! command -v "$svg" >/dev/null 2>&1; then
  echo "error: rsvg-convert is required" >&2
  exit 1
fi
if command -v magick >/dev/null 2>&1; then
  im=(magick)
elif command -v convert >/dev/null 2>&1; then
  im=(convert)
else
  echo "error: ImageMagick (magick or convert) is required" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

render() {
  local source="$1" size="$2" out="$3"
  mkdir -p "$(dirname "$out")"
  "$svg" -w "$size" -h "$size" "$source" -o "$out"
}

make_ico() {
  local source="$1" out="$2"
  local frames=()
  local size
  for size in 16 24 32 48 64 128 256; do
    render "$source" "$size" "$tmp/ico-$size.png"
    frames+=("$tmp/ico-$size.png")
  done
  mkdir -p "$(dirname "$out")"
  "${im[@]}" "${frames[@]}" "$out"
}

# Root exports used by packaging (Linux launcher, AppImage, DMG).
render "$rounded" 1024 assets/brand/elsewhen-icon-v2-1024.png
render "$rounded" 256 assets/brand/elsewhen-icon-v2-256.png

# macOS app icon set.
for size in 16 32 64 128 256 512 1024; do
  render "$rounded" "$size" "ui/macos/Runner/Assets.xcassets/AppIcon.appiconset/app_icon_${size}.png"
done

# iOS app icon set. iOS supplies its own corner mask, so this source is
# deliberately full-bleed and opaque.
ios_dir="ui/ios/Runner/Assets.xcassets/AppIcon.appiconset"
render "$square" 20 "$ios_dir/Icon-App-20x20@1x.png"
render "$square" 40 "$ios_dir/Icon-App-20x20@2x.png"
render "$square" 60 "$ios_dir/Icon-App-20x20@3x.png"
render "$square" 29 "$ios_dir/Icon-App-29x29@1x.png"
render "$square" 58 "$ios_dir/Icon-App-29x29@2x.png"
render "$square" 87 "$ios_dir/Icon-App-29x29@3x.png"
render "$square" 40 "$ios_dir/Icon-App-40x40@1x.png"
render "$square" 80 "$ios_dir/Icon-App-40x40@2x.png"
render "$square" 120 "$ios_dir/Icon-App-40x40@3x.png"
render "$square" 120 "$ios_dir/Icon-App-60x60@2x.png"
render "$square" 180 "$ios_dir/Icon-App-60x60@3x.png"
render "$square" 76 "$ios_dir/Icon-App-76x76@1x.png"
render "$square" 152 "$ios_dir/Icon-App-76x76@2x.png"
render "$square" 167 "$ios_dir/Icon-App-83.5x83.5@2x.png"
render "$square" 1024 "$ios_dir/Icon-App-1024x1024@1x.png"

# Android legacy launcher icons (API < 26) and adaptive foreground layers.
android_res="ui/android/app/src/main/res"
for spec in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
  density="${spec%%:*}"
  size="${spec##*:}"
  render "$square" "$size" "$android_res/mipmap-$density/ic_launcher.png"
done
for spec in mdpi:108 hdpi:162 xhdpi:216 xxhdpi:324 xxxhdpi:432; do
  density="${spec%%:*}"
  size="${spec##*:}"
  render "$adaptive_fg" "$size" "$android_res/mipmap-$density/ic_launcher_foreground.png"
done

# Android 12 splash icon (288dp canvas, 192dp safe circle).
for spec in mdpi:288 hdpi:432 xhdpi:576 xxhdpi:864 xxxhdpi:1152; do
  density="${spec%%:*}"
  size="${spec##*:}"
  render "$splash" "$size" "$android_res/mipmap-$density/ic_splash.png"
done

# Linux window/taskbar icon and Windows taskbar/MSI icon.
render "$rounded" 256 ui/linux/runner/app_icon.png
make_ico "$rounded" ui/windows/runner/resources/app_icon.ico

# System tray: same artwork as the taskbar icon, exported smaller. A separate
# transparent/brightened mark made the tray and taskbar read as two brands.
# The -v2 suffix also busts desktop-shell icon caches keyed by file path.
render "$rounded" 64 ui/assets/brand/elsewhen-tray-v2.png
make_ico "$rounded" ui/assets/brand/elsewhen-tray-v2.ico

echo "Brand assets regenerated from the shared mark."
