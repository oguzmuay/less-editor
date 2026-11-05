--- LESS açılış ekranı (snacks dashboard): LazyVim başlığı yerine sade "LESS" yazısı + tek satır slogan, LESS'e uyan
--- kısa giriş listesi (Workspaces `w` dahil) ve "plugins loaded" yerine yalnız açılış süresi.
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
      -- stylua: ignore
      preset.keys = {
        { icon = " ", key = "f", desc = "Find File", action = ":lua Snacks.dashboard.pick('files')" },
        { icon = " ", key = "n", desc = "New File", action = ":ene | startinsert" },
        { icon = " ", key = "p", desc = "Projects", action = ":lua Snacks.picker.projects()" },
        { icon = " ", key = "w", desc = "Workspaces", action = function() require("workspaces.ui").pick() end },
        { icon = " ", key = "s", desc = "Restore Session", section = "session" },
        { icon = " ", key = "r", desc = "Recent Files", action = ":lua Snacks.dashboard.pick('oldfiles')" },
        { icon = " ", key = "g", desc = "Find Text", action = ":lua Snacks.dashboard.pick('live_grep')" },
        { icon = " ", key = "c", desc = "Config", action = ":lua Snacks.dashboard.pick('files', {cwd = vim.fn.stdpath('config')})" },
        { icon = "󰒲 ", key = "l", desc = "Plugins", action = ":Lazy" },
        { icon = " ", key = "q", desc = "Quit", action = ":qa" },
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
