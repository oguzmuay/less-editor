local M = {}

--- Komut adı → { workbench panel adı, açıklama }. `Code` toggle değil, kod penceresine odaklanır.
local commands = {
  Files = { "files", "Toggle file explorer" },
  Code = { nil, "Focus code window" },
  AI = { "claude", "Toggle AI panel (Claude, Codex, Gemini, Ollama)" },
  Claude = { "claude", "Toggle AI panel" },
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
      vim.api.nvim_create_user_command(name, function(args)
        local sub = args.args
        if sub == "account" then
          return require("termtabs.accounts").pick()
        elseif sub == "new" then
          return require("termtabs.agents").pick()
        elseif sub ~= "" then
          if not require("termtabs.agents").kinds[sub] then
            return vim.notify("Unknown AI: " .. sub, vim.log.levels.WARN)
          end
          return require("termtabs.agents").open(sub)
        end
        if spec[1] then
          wb.toggle(spec[1])
        else
          wb.code()
        end
      end, {
        nargs = spec[1] == "claude" and "?" or 0,
        complete = spec[1] == "claude" and function()
          return vim.list_extend({ "account", "new" }, vim.deepcopy(require("termtabs.agents").order))
        end or nil,
        desc = spec[2],
        force = true,
      })
    end
  end
  vim.api.nvim_create_user_command("Shortcuts", function()
    require("shortcuts").open()
  end, { nargs = 0, desc = "Search keyboard shortcuts" })
  vim.api.nvim_create_user_command("UIReset", wb.reset_ui, { nargs = 0, desc = "Reset panel sizes to defaults" })
end

return M
