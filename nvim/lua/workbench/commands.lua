local M = {}

--- Komut adı → { workbench panel adı, açıklama }. `Code` toggle değil, kod penceresine odaklanır.
local commands = {
  Files = { "files", "Toggle file explorer" },
  Code = { nil, "Focus code window" },
  Claude = { "claude", "Toggle Claude Code panel" },
  Terminal = { "shell", "Toggle Terminal panel" },
  Datasource = { "db", "Toggle Database panel" },
  Run = { "runner", "Toggle Run panel" },
  Git = { "git", "Toggle Git panel (uncommitted changes)" },
  GitHistory = { "git_history", "Toggle Git History panel" },
}

--- Panelleri saf toggle eden Ex komutlarını (`:Files`, `:Git` ...) tanımlar.
--- Aynı adla başka bir eklentinin tanımladığı komut varsa ezmez, uyarı verir.
function M.setup()
  local wb = require("workbench")
  for name, spec in pairs(commands) do
    local existing = vim.api.nvim_get_commands({ builtin = false })[name]
    if existing and existing.definition ~= spec[2] then
      vim.notify(("workbench: :%s is already defined, skipped"):format(name), vim.log.levels.WARN)
    else
      vim.api.nvim_create_user_command(name, function()
        if spec[1] then
          wb.toggle(spec[1])
        else
          wb.code()
        end
      end, { nargs = 0, desc = spec[2], force = true })
    end
  end
end

return M
