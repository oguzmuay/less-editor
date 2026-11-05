#!/usr/bin/env bash
# Kullanım: ./install.sh            -> Brewfile paketlerini kurar + config'leri symlink'ler + eklentileri lock'a göre kurar
#           ./install.sh --no-brew  -> sadece symlink + eklentiler
set -euo pipefail

REPO="$(cd "$(dirname "$0")" && pwd)"

link() {
  local src="$1" dst="$2"
  if [ -L "$dst" ] && [ "$(readlink "$dst")" = "$src" ]; then
    echo "✓ $dst zaten bağlı"
    return
  fi
  if [ -e "$dst" ] || [ -L "$dst" ]; then
    local backup="$dst.backup-$(date +%Y%m%d%H%M%S)"
    mv "$dst" "$backup"
    echo "↪ mevcut $dst -> $backup"
  fi
  mkdir -p "$(dirname "$dst")"
  ln -s "$src" "$dst"
  echo "✓ $dst -> $src"
}

if [ "${1:-}" != "--no-brew" ]; then
  brew bundle --file="$REPO/Brewfile"
fi

link "$REPO/nvim" "$HOME/.config/nvim"
link "$REPO/ghostty/config.ghostty" "$HOME/.config/ghostty/config.ghostty"

echo "Eklentiler lazy-lock.json'a göre kuruluyor..."
nvim --headless "+Lazy! restore" +qa
echo "Bitti. Ghostty'yi açıp bir proje klasöründe 'nvim' çalıştır. Dil sunucuları ilk açılışta Mason ile kurulur."
