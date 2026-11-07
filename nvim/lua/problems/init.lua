local M = {}

local WINBAR = "%!v:lua.require'problems'.winbar()"
local BLANK = " "

--- Problem sayacının gösterilmeyeceği (panel) filetype'ları. scrollview'in hariç tutma listesi de buradan gelir.
M.excluded_filetypes = {
  "neo-tree",
  "neo-tree-popup",
  "runner",
  "githistory",
  "githistorybranches",
  "gitstatus",
  "gitblame",
  "termtabs-summary",
  "datasource",
  "dsdata",
  "dbout",
  "qf",
  "help",
  "lazy",
  "mason",
  "trouble",
  "noice",
  "notify",
  "snacks_dashboard",
  "snacks_picker_list",
  "snacks_picker_input",
  "snacks_picker_preview",
  "snacks_notif",
  "snacks_terminal",
  "snacks_win",
  "which-key",
  "diff",
  "image",
  "dap-repl",
  "dapui_scopes",
  "dapui_breakpoints",
  "dapui_stacks",
  "dapui_watches",
  "dapui_console",
  "neotest-summary",
  "neotest-output-panel",
  "grug-far",
  "checkhealth",
}

--- Kod penceresine hizalama için boş winbar verilen (sayaç göstermeyen) filetype'lar.
--- GitBlame sütunu kod penceresiyle scrollbind'lı; winbar yüksekliği farklı olursa satırlar bir kayar.
local ALIGN_FILETYPES = { gitblame = true }

--- Sayaç yerine kendi winbar'ını gösteren filetype'lar (Database veri tab'ının sabit tablo başlığı).
local CUSTOM_WINBARS = { dsdata = "%!v:lua.require'datasource.grid'.winbar()" }

local SEVERITIES = {
  { sev = vim.diagnostic.severity.ERROR, name = "Error", hl = "DiagnosticError" },
  { sev = vim.diagnostic.severity.WARN, name = "Warn", hl = "DiagnosticWarn" },
  { sev = vim.diagnostic.severity.INFO, name = "Info", hl = "DiagnosticInfo" },
  { sev = vim.diagnostic.severity.HINT, name = "Hint", hl = "DiagnosticHint" },
}

--- Severity ikonları: LazyVim'in diagnostik ikonları (Nerd Font), yoksa Unicode yedek.
---@return table<string, string>
local function icons()
  local ok, lazy_icons = pcall(function()
    return LazyVim.config.icons.diagnostics
  end)
  if ok and type(lazy_icons) == "table" and lazy_icons.Error then
    return lazy_icons
  end
  return { Error = "\u{2716} ", Warn = "\u{26a0} ", Info = "\u{2139} ", Hint = "\u{2022} " }
end

--- Pencere bir kod penceresi mi: normal (float olmayan) pencere, dosya buffer'ı (buftype ""), panel filetype'ı değil,
--- diff modunda değil (diff modunda scrollbind'lı karşı pencereyle satır hizası korunsun).
---@param win integer
---@return boolean
function M.is_code_window(win)
  if not vim.api.nvim_win_is_valid(win) or vim.api.nvim_win_get_config(win).relative ~= "" then
    return false
  end
  local buf = vim.api.nvim_win_get_buf(win)
  local bo = vim.bo[buf]
  return bo.buftype == ""
    and not vim.tbl_contains(M.excluded_filetypes, bo.filetype)
    and not vim.wo[win].diff
    and not vim.b[buf].termtabs
    and not vim.b[buf].workbench_bottom
end

--- Buffer'daki diagnostikleri önem derecesine göre sayar.
---@param buf integer
---@return table<integer, integer>
function M.counts(buf)
  local counts = vim.diagnostic.count(buf)
  local out = {}
  for _, s in ipairs(SEVERITIES) do
    out[s.sev] = counts[s.sev] or 0
  end
  return out
end

--- Gezinmede kullanılacak en yüksek önem derecesi (error varsa ERROR, yoksa WARN, ...); problem yoksa nil.
---@param buf integer
---@return integer|nil
function M.top_severity(buf)
  local counts = M.counts(buf)
  for _, s in ipairs(SEVERITIES) do
    if counts[s.sev] > 0 then
      return s.sev
    end
  end
end

--- Verilen penceredeki bir sonraki/önceki probleme gider ve satırın diagnostik float'ını açar.
--- `severity` verilmezse en yüksek önem derecesindeki problemler arasında gezilir. Dosya sonunda başa sarar.
---@param win integer
---@param count integer 1 sonraki, -1 önceki
---@param severity? integer
---@return boolean jumped
function M.jump(win, count, severity)
  if not M.is_code_window(win) then
    return false
  end
  local buf = vim.api.nvim_win_get_buf(win)
  severity = severity or M.top_severity(buf)
  if not severity then
    return false
  end
  vim.api.nvim_set_current_win(win)
  local diag = vim.diagnostic.jump({
    winid = win,
    count = count,
    severity = severity,
    wrap = true,
    on_jump = function(_, bufnr)
      vim.diagnostic.open_float({ bufnr = bufnr, scope = "cursor", focus = false })
    end,
  })
  return diag ~= nil
end

--- Winbar tıklama işleyicisi: `minwid` pencere no, `button` "l"/"r"/"m".
--- Sağ tık buffer'ın diagnostik listesini (Trouble) açar.
---@param minwid integer
---@param button string
---@param count integer
---@param severity? integer
local function on_click(minwid, button, count, severity)
  if button == "r" then
    if vim.api.nvim_win_is_valid(minwid) then
      vim.api.nvim_set_current_win(minwid)
    end
    pcall(vim.cmd, "Trouble diagnostics toggle filter.buf=0")
    return
  end
  M.jump(minwid, count, severity)
end

--- Global tıklama işleyicileri (winbar `%@v:lua.X@` sözdizimi global fonksiyon ister).
function _G.ProblemsNext(minwid, _, button)
  on_click(minwid, button, 1)
end

function _G.ProblemsPrev(minwid, _, button)
  on_click(minwid, button, -1)
end

function _G.ProblemsNextError(minwid, _, button)
  on_click(minwid, button, 1, vim.diagnostic.severity.ERROR)
end

function _G.ProblemsNextWarn(minwid, _, button)
  on_click(minwid, button, 1, vim.diagnostic.severity.WARN)
end

function _G.ProblemsNextInfo(minwid, _, button)
  on_click(minwid, button, 1, vim.diagnostic.severity.INFO)
end

function _G.ProblemsNextHint(minwid, _, button)
  on_click(minwid, button, 1, vim.diagnostic.severity.HINT)
end

--- Bir winbar parçasını tıklanabilir yapar.
---@param win integer
---@param fn string global fonksiyon adı
---@param text string
---@return string
local function click(win, fn, text)
  return ("%%%d@v:lua.%s@%s%%X"):format(win, fn, text)
end

--- Winbar içeriği: sağa yaslı `✖ 1  ⚠ 3  ↑ ↓` ya da temizse `✓`. Sondaki boşluklar scrollbar sütununa yer bırakır.
---@return string
function M.winbar()
  local win = vim.g.statusline_winid or vim.api.nvim_get_current_win()
  if not M.is_code_window(win) then
    return ""
  end
  local counts = M.counts(vim.api.nvim_win_get_buf(win))
  local icon = icons()
  local parts = {}
  for _, s in ipairs(SEVERITIES) do
    local n = counts[s.sev]
    if n > 0 then
      parts[#parts + 1] = click(win, "ProblemsNext" .. s.name, ("%%#%s#%s%d"):format(s.hl, icon[s.name], n))
    end
  end
  if #parts == 0 then
    return "%=%#DiagnosticOk# ✓ %*  "
  end
  local arrows = click(win, "ProblemsPrev", "%#WinBarProblemsArrow# ↑ ")
    .. click(win, "ProblemsNext", "%#WinBarProblemsArrow# ↓ ")
  return "%=" .. table.concat(parts, "  ") .. " " .. arrows .. "%*  "
end

--- Pencere-yerel winbar'ı pencere türüne göre ayarlar. Sadece kendi koyduğu değerleri değiştirir;
--- termtabs gibi başka modüllerin winbar'ına dokunmaz.
---@param win integer
local function apply(win)
  if not vim.api.nvim_win_is_valid(win) or vim.api.nvim_win_get_config(win).relative ~= "" then
    return
  end
  local current = vim.api.nvim_get_option_value("winbar", { win = win, scope = "local" })
  local ft = vim.bo[vim.api.nvim_win_get_buf(win)].filetype
  local want = ""
  if M.is_code_window(win) then
    want = WINBAR
  elseif ALIGN_FILETYPES[ft] then
    want = BLANK
  elseif CUSTOM_WINBARS[ft] then
    want = CUSTOM_WINBARS[ft]
  end
  local ours = current == "" or current == WINBAR or current == BLANK or vim.tbl_contains(vim.tbl_values(CUSTOM_WINBARS), current)
  if current == want or not ours then
    return
  end
  vim.api.nvim_set_option_value("winbar", want, { win = win, scope = "local" })
end

--- Tüm sekmelerdeki pencerelerin winbar'ını günceller.
function M.refresh()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    apply(win)
  end
end

local scheduled = false

--- Aynı olay döngüsündeki çok sayıda tetiklemeyi tek `refresh()`'e birleştirir.
local function schedule_refresh()
  if scheduled then
    return
  end
  scheduled = true
  vim.schedule(function()
    scheduled = false
    M.refresh()
  end)
end

--- Vurguları TokyoNight/diagnostik renkleriyle tanımlar (renk şeması değişince yeniden).
local function set_highlights()
  vim.api.nvim_set_hl(0, "WinBarProblemsArrow", { default = true, link = "Comment" })
end

--- Autocmd'leri kurar: pencere/buffer değişiminde winbar ataması, `DiagnosticChanged`'de yeniden çizim; hata tooltip'i
--- (`problems.hover`).
function M.setup()
  set_highlights()
  local group = vim.api.nvim_create_augroup("problems", { clear = true })
  vim.api.nvim_create_autocmd({ "BufWinEnter", "WinNew", "WinEnter", "FileType", "TermOpen", "VimEnter" }, {
    group = group,
    callback = schedule_refresh,
  })
  vim.api.nvim_create_autocmd(
    "OptionSet",
    { group = group, pattern = { "diff", "buftype" }, callback = schedule_refresh }
  )
  vim.api.nvim_create_autocmd("DiagnosticChanged", {
    group = group,
    callback = function()
      vim.cmd.redrawstatus({ bang = true })
    end,
  })
  vim.api.nvim_create_autocmd("ColorScheme", { group = group, callback = set_highlights })
  require("problems.hover").setup()
  M.refresh()
end

return M
