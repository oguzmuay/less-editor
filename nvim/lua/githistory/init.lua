local M = {}

local ns = vim.api.nvim_create_namespace("githistory")
local FT = "githistory"
local SEP = "\31"

local state = {
  buf = nil,
  root = nil,
  file = nil,
  all = false,
  ---@type string[]
  repos = {},
  line_map = {},
  watcher = nil,
}

--- Klasörün (varsayılan: Neovim'in çalışma klasörü) git kökünü döndürür.
---@param dir string?
---@return string?
local function git_root(dir)
  dir = dir or vim.fn.getcwd()
  if vim.fn.isdirectory(dir) == 0 then
    return nil
  end
  local res = vim.system({ "git", "rev-parse", "--show-toplevel" }, { cwd = dir, text = true }):wait()
  return res.code == 0 and vim.trim(res.stdout) or nil
end

--- Projedeki git repoları: workspace açıksa her kökün reposu (tekrarsız), değilse çalışma klasörünün reposu.
---@return string[]
function M.repos()
  local ok, ws = pcall(require, "workspaces")
  local repos = {}
  for _, dir in ipairs(ok and ws.roots() or { vim.fn.getcwd() }) do
    local root = git_root(dir)
    if root and not vim.tbl_contains(repos, root) then
      table.insert(repos, root)
    end
  end
  return repos
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
  local mode = state.file and ("file: " .. vim.fn.fnamemodify(state.file, ":t")) or (state.all and "all branches" or "current branch")
  local multi = #state.repos > 1
  local header = " Git History · " .. (multi and (vim.fs.basename(state.root) .. " · ") or "") .. mode
  local hint = "   ⏎ diff · f file/repo · a all branches · y copy hash · R refresh"
    .. (multi and " · g choose repo" or "")
    .. " · q close"
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

--- Commit'in değişikliklerini kod editöründe diff tab'ı olarak açar. Dosya modunda yalnız o dosya gösterilir;
--- dosya o commit'te eski adıyla değiştiyse (`--follow`) tüm commit'e düşülür.
---@param hash string
local function open_commit(hash)
  local gitdiff = require("gitdiff")
  if not (state.file and gitdiff.open({ rev = hash, path = state.file, cwd = state.root })) then
    gitdiff.open({ rev = hash, cwd = state.root })
  end
end

--- Paneli başka bir repoya geçirir (dosya modu sıfırlanır, izleyici yeni repoya taşınır).
---@param root string
local function set_root(root)
  if root ~= state.root then
    state.root, state.file = root, nil
    watch()
  end
end

--- Birden fazla repo varsa (workspace) hangisinin geçmişinin gösterileceğini seçtirir.
local function choose_repo()
  if #state.repos < 2 then
    vim.notify("No other git repository to choose", vim.log.levels.INFO)
    return
  end
  vim.ui.select(state.repos, {
    prompt = "Git repository",
    format_item = function(r)
      return (r == state.root and "● " or "  ") .. vim.fn.fnamemodify(r, ":~")
    end,
  }, function(root)
    if root then
      set_root(root)
      M.refresh()
    end
  end)
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
    vim.notify("Copied: " .. hash)
  end))
  map("f", function()
    local file = not state.file and require("workbench").code_file() or nil
    local root = file and git_root(vim.fs.dirname(file))
    if root and root ~= state.root then
      set_root(root)
    end
    state.file = file
    M.refresh()
  end)
  map("g", choose_repo)
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

--- Paneli alt şeride açar. Workspace'te birden fazla repo varsa önce kod penceresindeki dosyanın reposu,
--- yoksa ilk repo gösterilir; `g` ile değiştirilir.
function M.open_panel()
  state.repos = M.repos()
  if #state.repos == 0 then
    vim.notify("This folder is not a git repository", vim.log.levels.WARN)
    return
  end
  if not vim.tbl_contains(state.repos, state.root) then
    local file = require("workbench").code_file()
    local root = file and git_root(vim.fs.dirname(file))
    set_root(vim.tbl_contains(state.repos, root) and root or state.repos[1])
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
