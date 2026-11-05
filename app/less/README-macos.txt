LESS editor for macOS
=====================

Install:   ./install.sh        (or drag LESS.app to Applications)
Needs:     brew install neovim git ripgrep fd

If the build is not notarized, macOS says LESS "is damaged" or "cannot be opened".
install.sh already handles this; after a manual drag-and-drop run once:

    xattr -dr com.apple.quarantine /Applications/LESS.app

Open a project from a terminal:  lessed ~/code/my-project
Settings: ~/.config/less/config.ghostty   Neovim config: ~/.config/less/nvim
The first window installs plugins and language tools (a few minutes, needs network).

https://github.com/oguzmuay/less-editor
LESS is built on Ghostty (MIT, see LESS.app/Contents/Resources/less/GHOSTTY-LICENSE).
