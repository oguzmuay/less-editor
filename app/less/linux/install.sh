#!/bin/sh
# install.sh (shipped in the LESS Linux tarball): copies LESS to ~/.local/opt/less-editor, links
# ~/.local/bin/less-editor and registers the desktop entry + icons. Override with PREFIX= / BINDIR=.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
dest=${PREFIX:-$HOME/.local/opt/less-editor}
bindir=${BINDIR:-$HOME/.local/bin}
data=${XDG_DATA_HOME:-$HOME/.local/share}
app=com.oguz-dev.less-editor

if [ -e "$dest" ] && [ ! -x "$dest/bin/less-editor" ]; then
  echo "install.sh: $dest exists and is not a LESS install; set PREFIX" >&2
  exit 1
fi
if [ "$here" != "$dest" ]; then
  rm -rf "$dest"
  mkdir -p "$(dirname "$dest")"
  cp -R "$here" "$dest"
fi
mkdir -p "$bindir" "$data/applications" "$data/icons"
ln -sf "$dest/bin/less-editor" "$bindir/less-editor"
sed "s|^Exec=less-editor|Exec=$dest/bin/less-editor|; s|^TryExec=less-editor|TryExec=$dest/bin/less-editor|" \
  "$dest/share/applications/$app.desktop" >"$data/applications/$app.desktop"
cp -R "$dest/share/icons/." "$data/icons/"
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" 2>/dev/null || true
echo "Installed $dest ($bindir/less-editor)"
for dep in nvim zsh git rg fd; do
  command -v $dep >/dev/null || echo "missing: $dep (LESS needs neovim, zsh, git, ripgrep, fd)"
done
