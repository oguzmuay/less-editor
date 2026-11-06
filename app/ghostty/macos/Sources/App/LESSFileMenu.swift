import AppKit

/// LESS: File menu entries for opening projects in a new window.
/// "Open Folder…" picks a folder, "Open Recent" lists the projects/workspaces Neovim recorded in
/// `<XDG_STATE_HOME or ~/.local/state>/less/nvim/recents.json` (written by `nvim/lua/autorestore`).
/// A new window starts in that folder; Neovim's autorestore then opens the project (or workspace).
final class LESSFileMenu: NSObject, NSMenuDelegate {
    static let shared = LESSFileMenu()

    private weak var ghostty: Ghostty.App?
    private let recentMenu = NSMenu(title: "Open Recent")

    private struct Recent: Decodable {
        let kind: String
        let dir: String
        let name: String?
    }

    private struct Recents: Decodable {
        let items: [Recent]
    }

    /// Inserts the items after "New Tab" in the File menu (once).
    func install(ghostty: Ghostty.App) {
        self.ghostty = ghostty
        guard let file = NSApp.mainMenu?.item(withTitle: "File")?.submenu,
              file.item(withTitle: "Open Folder…") == nil else { return }
        var index = (file.items.firstIndex { $0.action == #selector(AppDelegate.newTab(_:)) } ?? 1) + 1

        let open = NSMenuItem(title: "Open Folder…", action: #selector(openFolder(_:)), keyEquivalent: "")
        open.target = self
        file.insertItem(open, at: index)
        index += 1

        recentMenu.delegate = self
        let recent = NSMenuItem(title: "Open Recent", action: nil, keyEquivalent: "")
        recent.submenu = recentMenu
        file.insertItem(recent, at: index)
    }

    private static var recentsURL: URL {
        let env = ProcessInfo.processInfo.environment
        let state = env["XDG_STATE_HOME"].map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".local/state")
        return state.appendingPathComponent("less/nvim/recents.json")
    }

    /// Existing folders from the recents file, newest first; empty if the file is missing or invalid.
    private func recents() -> [Recent] {
        guard let data = try? Data(contentsOf: Self.recentsURL),
              let decoded = try? JSONDecoder().decode(Recents.self, from: data) else { return [] }
        return decoded.items.filter { FileManager.default.fileExists(atPath: $0.dir) }
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let items = recents()
        if items.isEmpty {
            let empty = NSMenuItem(title: "No Recent Projects", action: nil, keyEquivalent: "")
            empty.isEnabled = false
            menu.addItem(empty)
            return
        }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        for r in items {
            let base = r.kind == "workspace" ? (r.name ?? (r.dir as NSString).lastPathComponent)
                : (r.dir as NSString).lastPathComponent
            let item = NSMenuItem(title: base, action: #selector(openRecent(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = r.dir
            let detail = r.kind == "workspace" ? "workspace"
                : (r.dir.hasPrefix(home) ? "~" + r.dir.dropFirst(home.count) : r.dir)
            let title = NSMutableAttributedString(string: base)
            title.append(NSAttributedString(string: "  " + detail, attributes: [
                .foregroundColor: NSColor.secondaryLabelColor,
                .font: NSFont.menuFont(ofSize: NSFont.smallSystemFontSize),
            ]))
            item.attributedTitle = title
            item.image = NSImage(
                systemSymbolName: r.kind == "workspace" ? "square.stack" : "folder",
                accessibilityDescription: nil)
            menu.addItem(item)
        }
    }

    @objc private func openRecent(_ sender: NSMenuItem) {
        guard let dir = sender.representedObject as? String else { return }
        openWindow(in: dir)
    }

    @objc private func openFolder(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Open"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        openWindow(in: url.path)
    }

    private func openWindow(in dir: String) {
        guard let ghostty else { return }
        var config = Ghostty.SurfaceConfiguration()
        config.workingDirectory = dir
        // Tells Neovim this window should open the folder even when other LESS windows are open.
        config.environmentVariables["LESS_OPEN_DIR"] = dir
        _ = TerminalController.newWindow(ghostty, withBaseConfig: config)
    }
}
