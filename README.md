# LESS

**LESS** ("LESS editor") is a calm, minimal code editor for macOS. Less noise, more code.

It is a single app: a rebranded fork of the [Ghostty](https://github.com/ghostty-org/ghostty) terminal that
opens straight into a curated [Neovim](https://neovim.io) setup built on [LazyVim](https://www.lazyvim.org).
You get IDE-style panels, mouse support and Cmd shortcuts without the crowded UI of a full IDE.
Main languages: Java (Spring Boot), JavaScript/TypeScript, SQL and Markdown.

<!-- Screenshot: docs/screenshot.png -->

> Turkish docs: the full shortcut list is in [nvim/KISAYOLLAR.md](nvim/KISAYOLLAR.md), design decisions and
> technical notes in [CLAUDE.md](CLAUDE.md).

## Features

- **Panels on Cmd+0..7**, IntelliJ style (open, focus, or close when focused): Git changes (0), Files (1),
  Code (2), Claude Code (3), Terminal (4), Database (5), Run (6), Git History (7). The right column and the
  bottom strip keep a fixed, predictable layout.
- **Claude Code tabs**: several Claude sessions side by side in one panel, named after the session, with a
  summary tab (status, tokens, last task). The terminal panel has tabs too (Cmd+T).
- **Run panel** for Spring Boot (Maven/Gradle) and Node scripts: Run / Stop / Rerun / **Debug** per item,
  debugger attaches automatically (jdtls / js-debug, localhost only).
- **Git panel** (stage, unstage, discard) and **Git History** with per-commit and per-file history;
  `:GitBlame` annotations.
- **Diff tabs**: diffs open as normal editor tabs, panels stay in place.
- **Preview tabs**: single click in Files/Git opens a preview (italic) that the next one replaces; edit, save
  or double click to keep it.
- **Workspaces**: open folders from different places as one project (search, Run and Git cover all roots).
- **Autorestore**: LESS reopens your last project or workspace with its tabs, pins and panels.
- **`:Font`**: switch the editor font (JetBrains Mono, Geist Mono, Fira Code, Monaspace, ...).
- Pinned tabs (Cmd+P), Cmd+F / Cmd+Shift+F search, problem markers on the scrollbar.

## Install

### Release download (macOS 13+)

1. Download `LESS-<version>-macos-universal.dmg` (or `.zip`) from
   [Releases](https://github.com/oguzmuay/less-editor/releases).
2. Run `./install.sh` from it: copies `LESS.app` to `/Applications` and `lessed` to `~/.local/bin`.
   (Or drag `LESS.app` to Applications.)

Releases are ad-hoc signed until LESS has an Apple Developer ID, so macOS may say the app "is damaged" or is
from an unidentified developer. `install.sh` handles it; after a manual copy run once:

```bash
xattr -dr com.apple.quarantine /Applications/LESS.app
```

Linux tarballs are **experimental** (Ghostty's GTK build, see [Releases](#releasing)); there is no Windows build
because Ghostty has none.

### From source

```bash
git clone https://github.com/oguzmuay/less-editor.git
cd less-editor
make install    # builds build/LESS.app, installs /Applications/LESS.app and ~/.local/bin/lessed
```

`make install` links `~/.config/less/nvim` to the repo's `nvim/` (development mode: config changes apply
immediately). Other targets: `make check`, `make app`, `make run`, `make dist` (zip + dmg in `build/dist`),
`make uninstall`, `make clean`.

### Prerequisites

- **Neovim** (0.12+), **git**, **ripgrep**, **fd**: `brew install neovim git ripgrep fd`
- Language tools are installed by Mason on first run; they need **Node.js** (npm based tools) and, for Java,
  a **JDK 25+** to run jdtls (`brew install openjdk`) plus your project's JDK and Maven/Gradle.
- Everything the author uses (fonts, lazygit, mysql-client, JDKs) is in the [Brewfile](Brewfile):
  `brew bundle`.

### First run

The first window shows "Setting up LESS": it installs the plugins, then language tools in the background
(about 800 MB, a few minutes, needs network). LESS uses its own config and data folders, so an existing
Ghostty or Neovim setup is never touched.

## `lessed`

```bash
lessed                 # new LESS window
lessed ~/code/my-app   # open a project (restores its session)
lessed src/Main.java   # open a file in a new window
```

It talks to the running LESS (or starts it). The first use asks for macOS Automation permission for your
terminal app. `LESS_APP_PATH` overrides the app location.

## Config and data

| What | Where |
|---|---|
| App settings (Ghostty options, keybinds) | `~/.config/less/config.ghostty` |
| Font chosen with `:Font` | `~/.config/less/font.ghostty` |
| Neovim config (copied on first run, yours to edit) | `~/.config/less/nvim` |
| Plugins, Mason tools, sessions | `~/.local/share/less/nvim`, `~/.local/state/less/nvim` |
| Cache | `~/.cache/less/nvim` |

Bundled defaults live in `LESS.app/Contents/Resources/less/`; your `config.ghostty` overrides them. Ghostty's
option reference applies: <https://ghostty.org/docs/config>.

## Building from source

- **Zig 0.17.x** (`brew install zig`; the exact minor comes from `app/ghostty/build.zig.zon`)
- **Full Xcode** (not only the Command Line Tools) with the **Metal Toolchain**:
  `xcodebuild -downloadComponent MetalToolchain`
- `make check` verifies all of this. `make app XCFW=universal` builds an arm64 + x86_64 app.
- The version comes from `LESS_VERSION`, else the latest `v*` git tag, else `0.1.0`.

Layout: `app/ghostty/` is Ghostty v1.4.0 as a git subtree (small upstream patches are listed in
[app/PATCHES.md](app/PATCHES.md)), `app/scripts/brand.sh` turns the Ghostty build into `LESS.app`,
`app/less/` holds the bundled defaults, launcher and icon, `nvim/` is the Neovim config.

## Releasing

Push a tag: `git tag v0.1.0 && git push origin v0.1.0`. GitHub Actions
([.github/workflows/release.yml](.github/workflows/release.yml)) builds the universal macOS zip + dmg and the
experimental Linux tarballs, then publishes a release with `SHA256SUMS.txt`. Developer ID signing and
notarization run only when the `APPLE_*` secrets are set (see the workflow header).

## Credits and license

- [Ghostty](https://github.com/ghostty-org/ghostty) by Mitchell Hashimoto and contributors, MIT. LESS's app is
  a fork of it; its license ships in the app (`Contents/Resources/less/GHOSTTY-LICENSE`).
- [Neovim](https://neovim.io) (Apache 2.0 / Vim license), [LazyVim](https://www.lazyvim.org) (Apache 2.0) and
  the plugins listed in `nvim/lazy-lock.json`, each under its own license.

LESS's own code (Neovim modules, scripts, branding) is released under the [MIT License](LICENSE), the same license
as Ghostty. LESS is not affiliated with Ghostty.
