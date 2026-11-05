local M = {}

local ns = vim.api.nvim_create_namespace("githistory")
local FT = "githistory"
local SEP = "\31"

local state = {
  buf = nil,
  root = nil,
  file = nil,
  all = false,
  line_map = {},
  watcher = nil,
}

--- Neovim'in çalışma klasörünün git kökünü döndürür.
---@return string?
local function git_root()
  local res = vim.system({ "git", "rev-parse", "--show-toplevel" }, { cwd = vim.fn.getcwd(), text = true }):wait()
  return res.code == 0 and vim.trim(res.stdout) or nil
end

--- git log çıktısını okuyup satır satır commit bilgisine çevirir.
--- Dosya modunda (--follow) git grafik çizmediği için grafik sadece repo modunda gösterilir.
---@return { graph: string, hash: string?, date: string?, author: string?, subject: string?, refs: string? }[]
local function load()
  local cmd = {
    "git",
    "log",
    "--date=format:%Y-%m-%d %H:%M",
    "--pretty=format:" .. SEP .. "%h" .. SEP .. "%ad" .. SEP .. "%an" .. SEP .. "%s" .. SEP .. "%D",
    "-n",
    "500",
  }
  if state.file then
    vim.list_extend(cmd, { "--follow", "--", state.file })
  else
    table.insert(cmd, 3, "--graph")
    if state.all then
      table.insert(cmd, "--all")
    end
  end
  local res = vim.system(cmd, { cwd = state.root, text = true }):wait()
  local entries = {}
  for line in vim.gsplit(res.stdout or "", "\n", { trimempty = true }) do
    local graph, rest = line:match("^(.-)" .. SEP .. "(.*)$")
    if graph then
      local f = vim.split(rest, SEP, { plain = true })
      table.insert(entries, { graph = graph, hash = f[1], date = f[2], author = f[3], subject = f[4], refs = f[5] })
    else
      table.insert(entries, { graph = line })
    end
  end
  return entries
end

--- Metni verilen genişliğe kırpar veya boşlukla doldurur.
---@param s string
---@param n integer
---@return string
local function fit(s, n)
  local w = vim.fn.strdisplaywidth(s)
  if w > n then
    return vim.fn.strcharpart(s, 0, n - 1) .. "…"
  end
  return s .. string.rep(" ", n - w)
end

--- Paneli git log'un güncel haliyle yeniden çizer.
function M.refresh()
  local buf = state.buf
  if not (buf and vim.api.nvim_buf_is_valid(buf)) or not state.root then
    return
  end
  local mode = state.file and ("dosya: " .. vim.fn.fnamemodify(state.file, ":t")) or (state.all and "tüm dallar" or "mevcut dal")
  local header = " Git History · " .. mode
  local hint = "   ⏎ diff · f dosya/repo · a tüm dallar · y hash kopyala · R yenile · q kapat"
  local lines, marks = { header .. hint }, { { 0, 0, #header, "Title" }, { 0, #header, -1, "Comment" } }
  state.line_map = {}
  for _, e in ipairs(load()) do
    local row = #lines
    if not e.hash then
      table.insert(lines, " " .. e.graph)
      table.insert(marks, { row, 0, -1, "Comment" })
    else
      local parts = {
        { " " .. e.graph, "Comment" },
        { e.hash .. "  ", "Identifier" },
        { e.date .. "  ", "Comment" },
        { fit(e.author, 16) .. "  ", "String" },
        { e.subject, "Normal" },
        { e.refs ~= "" and ("  (" .. e.refs .. ")") or "", "Special" },
      }
      local text, col = "", 0
      for _, p in ipairs(parts) do
        table.insert(marks, { row, col, col + #p[1], p[2] })
        text, col = text .. p[1], col + #p[1]
      end
      table.insert(lines, text)
      state.line_map[row + 1] = e.hash
    end
  end
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  for _, m in ipairs(marks) do
    vim.api.nvim_buf_set_extmark(buf, ns, m[1], m[2], {
      end_row = m[1],
      end_col = m[3] == -1 and #lines[m[1] + 1] or m[3],
      hl_group = m[4],
    })
  end
end

--- .git/logs/HEAD dosyasını izler; commit, checkout, reset gibi her işlemde paneli kendiliğinden yeniler.
local function watch()
  if state.watcher then
    state.watcher:stop()
  end
  local path = state.root .. "/.git/logs/HEAD"
  if not vim.uv.fs_stat(path) then
    return
  end
  state.watcher = vim.uv.new_fs_event()
  local timer = vim.uv.new_timer()
  state.watcher:start(path, {}, function()
    timer:start(200, 0, vim.schedule_wrap(M.refresh))
  end)
end

--- İmlecin bulunduğu satırdaki commit hash'ini döndürür.
---@return string?
local function current_hash()
  return state.line_map[vim.api.nvim_win_get_cursor(0)[1]]
end

--- Commit'in değişikliklerini diffview ile yeni bir sekmede açar.
---@param hash string
local function open_commit(hash)
  vim.cmd(("DiffviewOpen %s^!"):format(hash))
end

--- Panel buffer'ını oluşturur ve tuş/mouse eşlemelerini bağlar.
---@return integer
local function create_buf()
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].filetype = FT
  vim.bo[buf].bufhidden = "hide"
  local function map(lhs, fn)
    vim.keymap.set("n", lhs, fn, { buffer = buf, nowait = true })
  end
  local function on_commit(fn)
    return function()
      local hash = current_hash()
      if hash then
        fn(hash)
      end
    end
  end
  map("<CR>", on_commit(open_commit))
  map("<2-LeftMouse>", on_commit(open_commit))
  map("y", on_commit(function(hash)
    vim.fn.setreg("+", hash)
    vim.notify("Kopyalandı: " .. hash)
  end))
  map("f", function()
    state.file = not state.file and require("workbench").code_file() or nil
    M.refresh()
  end)
  map("a", function()
    state.all = not state.all
    M.refresh()
  end)
  map("R", M.refresh)
  map("q", function()
    M.close_panel(vim.api.nvim_get_current_win())
  end)
  return buf
end

--- Açık panel penceresini döndürür.
---@return integer?
function M.find_panel()
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == FT then
      return win
    end
  end
end

--- Paneli alt şeride açar.
function M.open_panel()
  local root = git_root()
  if not root then
    vim.notify("Bu klasör bir git reposu değil", vim.log.levels.WARN)
    return
  end
  if root ~= state.root then
    state.root, state.file = root, nil
    watch()
  end
  if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
    state.buf = create_buf()
  end
  local win = require("workbench").open_bottom(state.buf)
  vim.wo[win].cursorline = true
  vim.wo[win].wrap = false
  M.refresh()
  if vim.api.nvim_buf_line_count(state.buf) > 1 then
    vim.api.nvim_win_set_cursor(win, { 2, 0 })
  end
end

--- Panel penceresini kapatır.
---@param win integer
function M.close_panel(win)
  if vim.api.nvim_win_is_valid(win) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
    vim.api.nvim_win_hide(win)
  end
end

return M
