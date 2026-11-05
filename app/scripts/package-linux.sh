#!/bin/sh
# package-linux.sh <zig-prefix> <outdir>: turns a Ghostty GTK install prefix (`zig build -p <zig-prefix>`)
# into less-editor-<version>-linux-<arch>.tar.gz. The real binary goes to lib/less-editor/ghostty (it finds
# its resources via ../../share/terminfo), bin/less-editor is the wrapper that adds the LESS defaults.
# Ghostty's own desktop/D-Bus/systemd/file-manager files are left out. Needs rsvg-convert for the icons.
set -eu
src=$1 out=$2
repo=$(cd "$(dirname "$0")/../.." && pwd)
version=${LESS_VERSION:-$(git -C "$repo" describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null | sed 's/^v//')}
version=${version:-0.1.0}
name="less-editor-$version-linux-$(uname -m)"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage="$work/$name"
mkdir -p "$stage/bin" "$stage/lib/less-editor" "$stage/share/less" "$stage/share/applications" "$out"

install -m 755 "$src/bin/ghostty" "$stage/lib/less-editor/ghostty"
cp -R "$src/share/ghostty" "$src/share/terminfo" "$stage/share/"
[ ! -d "$src/share/locale" ] || cp -R "$src/share/locale" "$stage/share/"
install -m 755 "$repo/app/less/linux/less-editor" "$stage/bin/less-editor"
install -m 755 "$repo/app/less/linux/install.sh" "$stage/install.sh"
cp "$repo/app/less/linux/com.oguz-dev.less-editor.desktop" "$stage/share/applications/"

{
  grep -v '^config-file' "$repo/ghostty/config.ghostty"
  echo
  cat "$repo/app/less/defaults.ghostty"
  echo
  cat "$repo/app/less/linux/linux.ghostty"
} >"$stage/share/less/defaults.ghostty"
install -m 755 "$repo/app/less/launch" "$stage/share/less/launch"
rsync -a --exclude .lazy.lua --exclude '*.backup-*' "$repo/nvim/" "$stage/share/less/nvim/"
cp "$repo/app/ghostty/LICENSE" "$stage/share/less/GHOSTTY-LICENSE"

for s in 16 32 64 128 256 512; do
  d="$stage/share/icons/hicolor/${s}x${s}/apps"
  mkdir -p "$d"
  rsvg-convert -w $s -h $s "$repo/app/less/icon.svg" -o "$d/com.oguz-dev.less-editor.png"
done

tar -C "$work" -czf "$out/$name.tar.gz" "$name"
echo "Built $out/$name.tar.gz"
