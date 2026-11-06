--- LESS açılış ekranı (snacks dashboard): LazyVim başlığı yerine sade "LESS" yazısı + tek satır slogan, LESS'e uyan
--- kısa giriş listesi (Projects & Workspaces `p`, Open Folder `o`, New Workspace `w`) ve "plugins loaded" yerine yalnız açılış süresi.
local header = table.concat({
  "█   █▀▀ █▀▀ █▀▀",
  "█▄▄ ██▄ ▄▄█ ▄▄█",
}, "\n")

--- Alt satır: lazy.nvim'in ölçtüğü açılış süresi (UIEnter'dan önce 0 olur, o zaman boş kalır).
local function footer()
  local ms = require("lazy.stats").stats().startuptime
  return { align = "center", text = { { ms > 0 and ("ready in %d ms"):format(ms) or "", hl = "footer" } } }
end

return {
  {
    "folke/snacks.nvim",
    opts = function(_, opts)
      local preset = opts.dashboard.preset
      preset.header = header
      local g = vim.fn.nr2char
      -- stylua: ignore
      preset.keys = {
        { icon = g(0xf002) .. " ", key = "f", desc = "Find File", action = ":lua Snacks.dashboard.pick('files')" },
        { icon = g(0xf15b) .. " ", key = "n", desc = "New File", action = ":ene | startinsert" },
        { icon = g(0xf07b) .. " ", key = "p", desc = "Projects & Workspaces", action = function() require("workspaces.ui").projects() end },
        { icon = g(0xf07c) .. " ", key = "o", desc = "Open Folder", action = function() require("workspaces.ui").open_folder() end },
        { icon = g(0xf1b3) .. " ", key = "w", desc = "New Workspace", action = function() require("workspaces.ui").new() end },
        { icon = g(0xf1da) .. " ", key = "s", desc = "Restore Session", section = "session" },
        { icon = g(0xf017) .. " ", key = "r", desc = "Recent Files", action = ":lua Snacks.dashboard.pick('oldfiles')" },
        { icon = g(0xf0f6) .. " ", key = "g", desc = "Find Text", action = ":lua Snacks.dashboard.pick('live_grep')" },
        { icon = g(0xf013) .. " ", key = "c", desc = "Config", action = ":lua Snacks.dashboard.pick('files', {cwd = vim.fn.stdpath('config')})" },
        { icon = g(0xf1e6) .. " ", key = "l", desc = "Plugins", action = ":Lazy" },
        { icon = g(0xf08b) .. " ", key = "q", desc = "Quit", action = ":qa" },
      }
      opts.dashboard.sections = {
        { section = "header", padding = 1 },
        { text = { { "less noise, more code", hl = "footer" } }, align = "center", padding = 2 },
        { section = "keys", gap = 1, padding = 2 },
        footer,
      }
    end,
  },
}
