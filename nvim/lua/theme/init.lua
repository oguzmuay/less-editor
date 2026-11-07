--- Editör teması: `:Theme` seçicisi (listede gezerken tema canlı uygulanır, Esc geri alır), seçim kalıcı.
--- Seçim `stdpath("state")/theme` dosyasına tema id'si olarak yazılır (testte `NVIM_THEME_FILE`); açılışta LazyVim'in
--- `colorscheme` seçeneği `load()`'u çağırır. Tema değişince Ghostty'nin pencere kenarı (padding) da OSC 11 ile
--- editörün arka planına boyanır, çıkışta OSC 111 ile terminalin kendi rengine döner.
local M = {}

---@class ThemeEntry
---@field id string kalıcı kimlik
---@field name string listede görünen ad
---@field colorscheme string
---@field background "dark"|"light" colorscheme'den önce `vim.o.background` (bazı şemalar bundan stil seçer)

M.default = "tokyonight-night"

---@type ThemeEntry[]
M.themes = {
  { id = "intellij-dark", name = "IntelliJ Dark", colorscheme = "intellij-dark", background = "dark" },
  { id = "intellij-light", name = "IntelliJ Light", colorscheme = "intellij-light", background = "light" },
  { id = "darcula", name = "Darcula", colorscheme = "darcula", background = "dark" },
  { id = "tokyonight-night", name = "Tokyo Night", colorscheme = "tokyonight-night", background = "dark" },
  { id = "tokyonight-storm", name = "Tokyo Night Storm", colorscheme = "tokyonight-storm", background = "dark" },
  { id = "tokyonight-moon", name = "Tokyo Night Moon", colorscheme = "tokyonight-moon", background = "dark" },
  { id = "tokyonight-day", name = "Tokyo Night Day", colorscheme = "tokyonight-day", background = "light" },
  { id = "catppuccin-mocha", name = "Catppuccin Mocha", colorscheme = "catppuccin-mocha", background = "dark" },
  { id = "catppuccin-macchiato", name = "Catppuccin Macchiato", colorscheme = "catppuccin-macchiato", background = "dark" },
  { id = "catppuccin-frappe", name = "Catppuccin Frappé", colorscheme = "catppuccin-frappe", background = "dark" },
  { id = "catppuccin-latte", name = "Catppuccin Latte", colorscheme = "catppuccin-latte", background = "light" },
  { id = "vscode-dark", name = "VS Code Dark", colorscheme = "vscode", background = "dark" },
  { id = "vscode-light", name = "VS Code Light", colorscheme = "vscode", background = "light" },
  { id = "github-dark", name = "GitHub Dark", colorscheme = "github_dark_default", background = "dark" },
  { id = "github-dark-dimmed", name = "GitHub Dark Dimmed", colorscheme = "github_dark_dimmed", background = "dark" },
  { id = "github-light", name = "GitHub Light", colorscheme = "github_light_default", background = "light" },
  { id = "onedark", name = "One Dark", colorscheme = "onedark", background = "dark" },
  { id = "kanagawa-wave", name = "Kanagawa Wave", colorscheme = "kanagawa-wave", background = "dark" },
  { id = "kanagawa-dragon", name = "Kanagawa Dragon", colorscheme = "kanagawa-dragon", background = "dark" },
  { id = "kanagawa-lotus", name = "Kanagawa Lotus", colorscheme = "kanagawa-lotus", background = "light" },
  { id = "rose-pine", name = "Rosé Pine", colorscheme = "rose-pine-main", background = "dark" },
  { id = "rose-pine-moon", name = "Rosé Pine Moon", colorscheme = "rose-pine-moon", background = "dark" },
  { id = "rose-pine-dawn", name = "Rosé Pine Dawn", colorscheme = "rose-pine-dawn", background = "light" },
  { id = "gruvbox-dark", name = "Gruvbox Dark", colorscheme = "gruvbox", background = "dark" },
  { id = "gruvbox-light", name = "Gruvbox Light", colorscheme = "gruvbox", background = "light" },
  { id = "nightfox", name = "Nightfox", colorscheme = "nightfox", background = "dark" },
  { id = "carbonfox", name = "Carbonfox", colorscheme = "carbonfox", background = "dark" },
  { id = "dayfox", name = "Dayfox", colorscheme = "dayfox", background = "light" },
}

---@return string
function M.file()
  return vim.env.NVIM_THEME_FILE or (vim.fn.stdpath("state") .. "/theme")
end

---@param id string
---@return ThemeEntry?
function M.get(id)
  return vim.iter(M.themes):find(function(t)
    return t.id == id
  end)
end

--- Kayıtlı tema id'si (yoksa ya da tanınmıyorsa varsayılan).
---@return string
function M.current()
  local ok, lines = pcall(vim.fn.readfile, M.file())
  local id = ok and lines[1] and vim.trim(lines[1]) or ""
  return M.get(id) and id or M.default
end

--- Temayı uygular (kaydetmez). Şema bulunamazsa false.
---@param t ThemeEntry
---@return boolean
function M.apply(t)
  vim.o.background = t.background
  return (pcall(vim.cmd.colorscheme, t.colorscheme))
end

--- Temayı uygulayıp kalıcı seçim yapar.
---@param id string
function M.set(id)
  local t = M.get(id)
  if not t then
    vim.notify("Unknown theme: " .. id, vim.log.levels.WARN)
    return
  end
  if not M.apply(t) then
    vim.notify("Could not load theme " .. t.name, vim.log.levels.ERROR)
    return
  end
  vim.fn.mkdir(vim.fs.dirname(M.file()), "p")
  pcall(vim.fn.writefile, { id }, M.file())
  vim.notify("Theme: " .. t.name)
end

--- Açılışta kayıtlı temayı yükler (LazyVim `opts.colorscheme`); olmazsa varsayılana düşer.
function M.load()
  if not M.apply(M.get(M.current()) --[[@as ThemeEntry]]) then
    M.apply(M.get(M.default) --[[@as ThemeEntry]])
  end
end

--- Seçici: listede gezerken tema canlı uygulanır; Enter kaydeder, Esc önceki temaya döner. İmleç açılışta önce
--- ilk satırda durduğu için önizleme imleç mevcut temaya bir kez ulaşana kadar başlamaz (yanıp sönme olmasın).
function M.pick()
  local cur, before = M.current(), { name = vim.g.colors_name, background = vim.o.background }
  local items, start, chosen, ready = {}, 1, false, false
  for i, t in ipairs(M.themes) do
    items[#items + 1] = { text = t.name, theme = t }
    if t.id == cur then
      start = i
    end
  end
  Snacks.picker.pick({
    title = "Theme",
    items = items,
    layout = { preset = "select" },
    format = function(item)
      local tag = item.theme.id == cur and "  (current)" or ""
      return { { item.text }, { tag, "Comment" } }
    end,
    on_show = function(picker)
      picker.list:view(start)
    end,
    on_change = function(_, item)
      ready = ready or (item ~= nil and item.theme.id == cur)
      if item and ready then
        vim.schedule(function()
          M.apply(item.theme)
        end)
      end
    end,
    confirm = function(picker, item)
      chosen = true
      picker:close()
      if item then
        vim.schedule(function()
          M.set(item.theme.id)
        end)
      end
    end,
    on_close = function()
      if not chosen and before.name then
        vim.schedule(function()
          vim.o.background = before.background
          pcall(vim.cmd.colorscheme, before.name)
        end)
      end
    end,
  })
end

--- Ghostty'nin pencere kenarını editörün arka planına boyar (OSC 11).
local function sync_terminal_bg()
  local bg = vim.api.nvim_get_hl(0, { name = "Normal", link = false }).bg
  if bg and vim.api.nvim_ui_send then
    pcall(vim.api.nvim_ui_send, ("\27]11;#%06x\7"):format(bg))
  end
end

function M.setup()
  vim.api.nvim_create_user_command("Theme", function(o)
    if o.args == "" then
      M.pick()
    else
      M.set(o.args)
    end
  end, {
    nargs = "?",
    desc = "Choose the editor theme",
    complete = function(lead)
      return vim.tbl_filter(function(id)
        return vim.startswith(id, lead)
      end, vim.tbl_map(function(t)
        return t.id
      end, M.themes))
    end,
  })
  local group = vim.api.nvim_create_augroup("theme", { clear = true })
  vim.api.nvim_create_autocmd({ "ColorScheme", "UIEnter" }, { group = group, callback = sync_terminal_bg })
  vim.api.nvim_create_autocmd("VimLeavePre", {
    group = group,
    callback = function()
      if vim.api.nvim_ui_send then
        pcall(vim.api.nvim_ui_send, "\27]111\7")
      end
    end,
  })
end

return M
