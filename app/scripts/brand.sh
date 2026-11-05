#!/bin/sh
# brand.sh <Ghostty.app> <LESS.app>: turns the upstream Ghostty build into LESS.app
# (bundle id, names, icon, bundled defaults/launcher/Neovim config) and ad-hoc signs it.
set -eu
src=$1 out=$2
repo=$(cd "$(dirname "$0")/../.." && pwd)
# Version: LESS_VERSION (CI sets it from the tag), else the latest v* tag, else 0.1.0.
version=${LESS_VERSION:-$(git -C "$repo" describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null | sed 's/^v//')}
version=${version:-0.1.0}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

rm -rf "$out"
mkdir -p "$(dirname "$out")"
ditto "$src" "$out"
res="$out/Contents/Resources"
plist="$out/Contents/Info.plist"

pb() { /usr/libexec/PlistBuddy -c "$1" "$plist" 2>/dev/null || true; }
plutil -convert xml1 "$plist"
# Human-readable strings only ("... within Ghostty would ...", "New Ghostty Tab Here"); ids stay.
sed -i '' -e 's/\(<string>[^<]* \)Ghostty/\1LESS/' -e 's/<string>Ghostty /<string>LESS /' "$plist"
pb "Set :CFBundleIdentifier com.oguz-dev.less-editor"
# Executable name is the process name (ps, System Events, `tell application`).
mv "$out/Contents/MacOS/ghostty" "$out/Contents/MacOS/LESS"
pb "Set :CFBundleExecutable LESS"
pb "Set :CFBundleName LESS"
pb "Delete :CFBundleDisplayName"; pb "Add :CFBundleDisplayName string LESS"
pb "Set :CFBundleShortVersionString $version"
pb "Delete :NSHumanReadableCopyright"
pb "Add :NSHumanReadableCopyright string LESS editor. Built on Ghostty, (c) Mitchell Hashimoto, Ghostty contributors (MIT)."
pb "Set :MDItemKeywords Editor"
pb "Delete :CFBundleIconName"; pb "Delete :CFBundleIconFile"; pb "Add :CFBundleIconFile string LESS"
# Dock tile plugin reads Ghostty's own defaults (custom icons); LESS has a fixed icon.
pb "Delete :NSDockTilePlugIn"
rm -rf "$out/Contents/PlugIns/DockTilePlugin.plugin"

mkdir "$work/LESS.iconset"
sips -s format png "$repo/app/less/icon.svg" --out "$work/icon.png" >/dev/null
for s in 16 32 128 256 512; do
  sips -z $s $s "$work/icon.png" --out "$work/LESS.iconset/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$work/icon.png" --out "$work/LESS.iconset/icon_${s}x${s}@2x.png" >/dev/null
done
# AppleScript dictionary: titles/descriptions only; class/command codes and cocoa classes stay.
sed -i '' -E 's/(title|name|description)="([^"]*)Ghostty/\1="\2LESS/g' "$res/Ghostty.sdef"

iconutil -c icns "$work/LESS.iconset" -o "$res/LESS.icns"

mkdir -p "$res/less"
{
  grep -v '^config-file' "$repo/ghostty/config.ghostty"
  echo
  cat "$repo/app/less/defaults.ghostty"
} >"$res/less/defaults.ghostty"
cp "$repo/app/less/launch" "$res/less/launch"
rsync -a --exclude .lazy.lua --exclude '*.backup-*' "$repo/nvim/" "$res/less/nvim/"
cp "$repo/app/ghostty/LICENSE" "$res/less/GHOSTTY-LICENSE"

codesign --force --deep --sign - --preserve-metadata=entitlements,flags "$out"
codesign --verify --deep "$out"
echo "Built $out"
