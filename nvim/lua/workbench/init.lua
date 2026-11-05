local M = {}

local state = {
  code_win = nil,
  shell_buf = nil,
}

--- Pencere yüzen (floating) değil ve içinde normal bir dosya buffer'ı varsa "kod penceresi" sayılır.
---@param win integer
---@return boolean
local function is_code_win(win)
  if not vim.api.nvim_win_is_valid(win) then
    return false
  end
  if vim.api.nvim_win_get_config(win).relative ~= "" then
    return false
  end
  local buf = vim.api.nvim_win_get_buf(win)
  return vim.bo[buf].buftype == "" and vim.bo[buf].filetype ~= "neo-tree"
end

--- Mevcut sekmede, verilen koşulu sağlayan ilk pencereyi döndürür.
---@param pred fun(win: integer, buf: integer): boolean
---@return integer?
local function find_win(pred)
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.api.nvim_win_get_config(win).relative == "" and pred(win, vim.api.nvim_win_get_buf(win)) then
      return win
    end
  end
end

--- Verilen filetype'a sahip ilk pencereyi bulur.
---@param ft string|string[]
---@return integer?
local function find_ft(ft)
  local fts = type(ft) == "table" and ft or { ft }
  return find_win(function(_, buf)
    return vim.tbl_contains(fts, vim.bo[buf].filetype)
  end)
end

--- Pencereye odaklanır; terminal ise doğrudan yazmaya hazır olsun diye insert moduna geçer.
---@param win integer
local function focus(win)
  vim.api.nvim_set_current_win(win)
  if vim.bo[vim.api.nvim_win_get_buf(win)].buftype == "terminal" then
    vim.cmd.startinsert()
  else
    vim.cmd.stopinsert()
  end
end

--- IntelliJ tarzı panel davranışı: panel kapalıysa açar, açık ama odakta değilse odaklar,
--- zaten odaktaysa kapatıp kod penceresine döner.
---@param find fun(): integer?
---@param open fun()
---@param close fun(win: integer)
local function panel(find, open, close)
  local win = find()
  if not win then
    open()
    return
  end
  if win == vim.api.nvim_get_current_win() then
    close(win)
    M.code()
  else
    focus(win)
  end
end

--- Son kullanılan kod penceresini takip eden autocmd'leri ve terminal davranışlarını kurar.
function M.setup()
  local group = vim.api.nvim_create_augroup("workbench", { clear = true })
  vim.api.nvim_create_autocmd("WinEnter", {
    group = group,
    callback = function()
      local win = vim.api.nvim_get_current_win()
      if is_code_win(win) then
        state.code_win = win
      end
    end,
  })
  vim.api.nvim_create_autocmd({ "WinEnter", "BufWinEnter" }, {
    group = group,
    pattern = "term://*",
    callback = function()
      vim.cmd.startinsert()
    end,
  })
end

--- Cmd+1: Dosya gezgini (neo-tree).
function M.files()
  panel(function()
    return find_ft("neo-tree")
  end, function()
    vim.cmd("Neotree focus reveal")
  end, function()
    vim.cmd("Neotree close")
  end)
end

--- Cmd+2: Son kullanılan kod penceresi. Yoksa ilk uygun pencere, o da yoksa yeni bir pencere.
function M.code()
  local win = state.code_win
  if not (win and is_code_win(win) and vim.api.nvim_win_get_tabpage(win) == vim.api.nvim_get_current_tabpage()) then
    win = find_win(function(w)
      return is_code_win(w)
    end)
  end
  if win then
    focus(win)
    return
  end
  vim.cmd("topleft vnew")
end

--- Claude Code terminalinin penceresini döndürür.
---@return integer?
local function claude_win()
  local ok, term = pcall(require, "claudecode.terminal")
  local buf = ok and term.get_active_terminal_bufnr() or nil
  if not buf then
    return
  end
  return find_win(function(_, b)
    return b == buf
  end)
end

--- Cmd+3: Sağdaki Claude Code terminali.
function M.claude()
  panel(claude_win, function()
    vim.cmd("ClaudeCodeFocus")
  end, function()
    vim.cmd("ClaudeCode")
  end)
end

--- Sağ taraftaki shell terminalinin penceresini döndürür.
---@return integer?
local function shell_win()
  if not (state.shell_buf and vim.api.nvim_buf_is_valid(state.shell_buf)) then
    return
  end
  return find_win(function(_, b)
    return b == state.shell_buf
  end)
end

--- Shell terminalini açar: Claude açıksa onun altına, değilse ekranın sağına yerleşir.
local function open_shell()
  local fresh = not (state.shell_buf and vim.api.nvim_buf_is_valid(state.shell_buf))
  if fresh then
    state.shell_buf = vim.api.nvim_create_buf(false, false)
  end
  local anchor = claude_win()
  local win = anchor and vim.api.nvim_open_win(state.shell_buf, true, { split = "below", win = anchor })
    or vim.api.nvim_open_win(state.shell_buf, true, {
      split = "right",
      win = -1,
      width = math.floor(vim.o.columns * 0.35),
    })
  vim.wo[win].winfixwidth = true
  vim.wo[win].number = false
  vim.wo[win].relativenumber = false
  vim.wo[win].signcolumn = "no"
  if fresh then
    vim.fn.jobstart(vim.o.shell, {
      term = true,
      cwd = vim.fn.getcwd(),
      on_exit = function()
        state.shell_buf = nil
      end,
    })
  end
  vim.cmd.startinsert()
end

--- Cmd+4: Sağdaki shell terminali.
function M.shell()
  panel(shell_win, open_shell, function(win)
    vim.api.nvim_win_hide(win)
  end)
end

--- Pencereleri ekrandaki konumuna göre (önce satır, sonra sütun) sıralar.
---@param wins integer[]
---@return integer[]
local function sort_by_position(wins)
  table.sort(wins, function(a, b)
    local pa, pb = vim.api.nvim_win_get_position(a), vim.api.nvim_win_get_position(b)
    return pa[1] == pb[1] and pa[2] < pb[2] or pa[1] < pb[1]
  end)
  return wins
end

--- Alt şeritteki (Spring, Git geçmişi, loglar) pencereleri soldan sağa döndürür.
---@return integer[]
function M.bottom_wins()
  local wins = {}
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.api.nvim_win_get_config(win).relative == "" and vim.b[vim.api.nvim_win_get_buf(win)].workbench_bottom then
      table.insert(wins, win)
    end
  end
  return sort_by_position(wins)
end

--- Sağ sütunu (Claude + terminal) ekranın tam boyunda en sağa sabitler.
--- Alt şerit tam genişlikte açıldıktan sonra çağrılır; böylece şerit sağ sütunun altına uzanmaz.
local function pin_right_column()
  local wins = vim.tbl_filter(function(w)
    return w ~= nil
  end, { claude_win(), shell_win() })
  if #wins == 0 then
    return
  end
  sort_by_position(wins)
  local width = vim.api.nvim_win_get_width(wins[1])
  local cur = vim.api.nvim_get_current_win()
  vim.cmd(("noautocmd call win_gotoid(%d) | wincmd L | noautocmd call win_gotoid(%d)"):format(wins[1], cur))
  for i = 2, #wins do
    vim.fn.win_splitmove(wins[i], wins[i - 1], { vertical = false, rightbelow = true })
  end
  vim.api.nvim_win_set_width(wins[1], width)
end

--- Buffer'ı alt şeritte açar. Şerit yoksa ekranın altına (sol paneller dahil, sağ sütun hariç) yeni şerit kurar;
--- varsa şeritteki son pencerenin (ya da `after` penceresinin) sağına ekler.
---@param buf integer
---@param opts? { enter?: boolean, width?: integer, after?: integer }
---@return integer
function M.open_bottom(buf, opts)
  opts = opts or {}
  vim.b[buf].workbench_bottom = true
  local strip = M.bottom_wins()
  local enter = opts.enter ~= false
  local win
  if #strip > 0 then
    win = vim.api.nvim_open_win(buf, enter, { split = "right", win = opts.after or strip[#strip], width = opts.width })
  else
    win = vim.api.nvim_open_win(buf, enter, {
      split = "below",
      win = -1,
      height = math.max(10, math.floor(vim.o.lines * 0.3)),
    })
    pin_right_column()
  end
  for opt, val in pairs({ winfixheight = true, number = false, relativenumber = false, signcolumn = "no" }) do
    vim.wo[win][opt] = val
  end
  return win
end

--- Cmd+5: Veritabanı paneli (vim-dadbod-ui).
function M.db()
  panel(function()
    return find_ft("dbui")
  end, function()
    vim.cmd("DBUI")
  end, function()
    vim.cmd("DBUIClose")
  end)
end

--- Cmd+6: Spring Boot uygulamaları paneli.
function M.spring()
  local spring = require("spring")
  panel(spring.find_panel, spring.open_panel, spring.close_panel)
end

--- Cmd+7: Git geçmişi paneli.
function M.git()
  local git = require("githistory")
  panel(git.find_panel, git.open_panel, git.close_panel)
end

--- Git geçmişinde "sadece bu dosya" modu için son kullanılan kod penceresindeki dosyanın yolunu döndürür.
---@return string?
function M.code_file()
  local win = state.code_win
  if not (win and is_code_win(win)) then
    win = find_win(function(w)
      return is_code_win(w)
    end)
  end
  local name = win and vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(win)) or ""
  return name ~= "" and name or nil
end

return M
