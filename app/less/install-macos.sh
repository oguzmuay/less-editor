#!/bin/sh
# install.sh (shipped in the LESS release archive): copies LESS.app to /Applications and lessed to ~/.local/bin,
# and clears the download quarantine flag (the app is not notarized yet).
# Override with PREFIX=~/Applications or BINDIR=/usr/local/bin.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
prefix=${PREFIX:-/Applications}
bindir=${BINDIR:-$HOME/.local/bin}

rm -rf "$prefix/LESS.app"
ditto "$here/LESS.app" "$prefix/LESS.app"
xattr -dr com.apple.quarantine "$prefix/LESS.app" 2>/dev/null || true
mkdir -p "$bindir"
install -m 755 "$here/lessed" "$bindir/lessed"
echo "Installed $prefix/LESS.app and $bindir/lessed"
case ":$PATH:" in *":$bindir:"*) ;; *) echo "Add $bindir to your PATH to use 'lessed'." ;; esac
command -v nvim >/dev/null || echo "LESS needs Neovim: brew install neovim git ripgrep fd"
