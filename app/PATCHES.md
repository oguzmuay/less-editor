# LESS patches on upstream Ghostty

`app/ghostty/` is a `git subtree` of https://github.com/ghostty-org/ghostty (squashed), currently **v1.4.0**.

Update:

```bash
git subtree pull --prefix=app/ghostty https://github.com/ghostty-org/ghostty <tag> --squash
```

Most branding is done **after** the build by `app/scripts/brand.sh` (bundle id, names, executable
`Contents/MacOS/LESS`, version, icon, AppleScript dictionary titles, Info.plist texts, dock tile plugin removed,
bundled files, ad-hoc signing), so upstream files stay almost untouched.
Ghostty is MIT licensed; its LICENSE stays in `app/ghostty/LICENSE` and is copied into the app
(`Contents/Resources/less/GHOSTTY-LICENSE`). The About window credits Ghostty.

## Touched upstream files

| File | Change | Why |
|---|---|---|
| `src/config/file_load.zig` | XDG config subdir `ghostty/` → `less/` (2 strings) | LESS reads `~/.config/less/config.ghostty`, never the user's Ghostty config |
| `macos/Sources/Ghostty/Ghostty.Config.swift` | Load `Resources/less/defaults.ghostty` before the default files (4 lines) | Bundled LESS defaults (keybinds, Neovim command); user config overrides them |
| `macos/Sources/Features/About/AboutView.swift` | Name "LESS", tagline crediting Ghostty, app icon instead of the cycling Ghostty icons, links (LESS repo + Ghostty), no Ghostty release-notes link | Branding |
| `src/os/macos.zig` | App Support/Caches bundle id `com.mitchellh.ghostty` → `com.oguz-dev.less-editor` (+ its tests) | LESS must not load or write the user's real Ghostty files |
| `src/config/Config.zig` | First-run template written to the XDG path (`~/.config/less/config.ghostty`) instead of App Support (1 line + comment) | The template points users to `~/.config/less` |
| `src/config/config-template` | Rewritten for LESS (short, points to `~/.config/less`, credits Ghostty docs) | First-run file must not say Ghostty |
| 6 window xibs: `macos/Sources/Features/Terminal/Window Styles/Terminal*.xib`, `macos/Sources/Features/QuickTerminal/QuickTerminal.xib` | Initial window title `👻 Ghostty` → `LESS` | Visible before Neovim sets the title |
| `macos/Sources/Features/Terminal/Window Styles/TitlebarTabsTahoeTerminalWindow.swift`, `TitlebarTabsVenturaTerminalWindow.swift` | Same initial title `LESS` (tabs titlebar style) | Same |
| `macos/Sources/App/AppDelegate.swift` | Alert texts: "Allow LESS to execute…", "Quit LESS?" (×2), hidden-control-characters warning; one line in `applicationDidFinishLaunching` installing `LESSFileMenu` | Visible strings; File menu |
| `macos/Sources/Features/Terminal/BaseTerminalController.swift`, `macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift` | Fallback title `👻` → `LESS` (3 strings); `window.representedURL` falls back to the app bundle (LESS icon next to the title) | No ghost in the title bar |
| `macos/Sources/App/LESSFileMenu.swift` (new file, LESS-only) | File > "Open Folder…" and "Open Recent" (projects/workspaces from `~/.local/state/less/nvim/recents.json`, written by `nvim/lua/autorestore`); each opens a new window in that folder | Switch projects in new windows from the menu bar |
| `macos/Sources/Helpers/UntrustedURLAlert.swift` | "LESS Blocked This Link" | Visible string |
| `macos/Sources/Features/App Intents/IntentPermission.swift`, `GhosttyIntentError.swift` | Shortcuts permission prompt / errors say LESS | Visible strings |
| `macos/Sources/App/MainMenu.xib` | App menu titled LESS (About/Hide/Quit LESS); `hidden="YES"` on: Check for Updates, Make Default Terminal, Split Right/Left/Down/Up, Change Terminal Title, Terminal Read-only, Quick Terminal, Terminal Inspector, Show/Hide All Terminals, split items of Window menu, Help menu | Calm, minimal menus |

All string patches are plain replacements; on a subtree pull conflict re-apply them (e.g.
`grep -rn '👻 Ghostty\|"Quit Ghostty' app/ghostty/macos/Sources`). Nothing else upstream is modified.

## Not patched (known leftovers)

- Internal identifiers: Swift module/classes `Ghostty*`, `com.mitchellh.ghostty.*` notification/UTType ids,
  AppleScript codes and cocoa classes, App Intents type names, `Resources/ghostty/` (themes, shell integration),
  `Ghostty.sdef`/`Ghostty.icns` file names, `TERM_PROGRAM=ghostty`, `xterm-ghostty` terminfo, `GHOSTTY_*` env vars.
- Embedded config/CLI documentation (`+show-config --docs`, `+help`) and CLI stderr messages talk about Ghostty.
- Command palette (Cmd+Shift+P) still has Ghostty's "Ghostty" joke entry and update entries (updates are off).
- Hidden menu items keep their Ghostty titles in `MainMenu.xib` (never shown).
- About window and `Resources/less/GHOSTTY-LICENSE` credit Ghostty on purpose.

## Linux (experimental, no extra upstream patches)

The Linux tarball is branded after `zig build -p <prefix>` by `app/scripts/package-linux.sh`; nothing upstream
changes beyond the table above (the `less/` XDG dir in `file_load.zig` applies to Linux too).

- Real binary `lib/less-editor/ghostty` (finds its resources through `share/terminfo`), wrapper `bin/less-editor`
  passes `--config-file=share/less/defaults.ghostty` (Linux has no `Ghostty.Config.swift` hook).
- `app/less/linux/linux.ghostty` sets `class = com.oguz-dev.less-editor` (GTK app id, WM class, D-Bus name) and loads
  `~/.config/less/config.ghostty` again so the user's file wins over the CLI-passed defaults.
- Own desktop file / icons (`com.oguz-dev.less-editor`); Ghostty's desktop, D-Bus, systemd, Nautilus/Dolphin files are
  not shipped.
- Leftovers: GTK About dialog, window/menu texts and `+help` still say Ghostty; `super+…` keybinds use the Super key
  (desktop shortcuts may take some); `:Font` reload and `lessed` are macOS only.
