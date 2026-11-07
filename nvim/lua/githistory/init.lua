local M = {}

local ns = vim.api.nvim_create_namespace("githistory")
local FT = "githistory"
local BRANCH_FT = "githistorybranches"
local BRANCH_WIDTH = 34
local SEP = "\31"

local state = {
  buf = nil,
  root = nil,
  file = nil,
  all = false,
  ---@type string? geçmişi gösterilen dal (nil = HEAD)
  branch = nil,
  bbuf = nil,
  ---@type table<integer, { kind: "group"|"branch"|"head", group?: string, branch?: GitBranch }>
  branch_map = {},
  ---@type table<string, boolean>
  collapsed = {},
  ---@type GitBranch[]
  locals = {},
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

--- git log çıktısını okuyup commit listesine çevirir (topo sırada, çocuk önce). Grafik `githistory.graph` ile
--- parent'lardan çizilir; dosya modunda (--follow) parent'lar listedeki sıradaki commit sayılır (tek şerit).
---@return { hash: string, parents: string[], date: string, author: string, subject: string, refs: string }[]
local function load()
  local cmd = {
    "git",
    "log",
    "--date=format:%Y-%m-%d %H:%M",
    "--pretty=format:%h" .. SEP .. "%p" .. SEP .. "%ad" .. SEP .. "%an" .. SEP .. "%s" .. SEP .. "%D",
    "-n",
    "500",
  }
  if state.branch and not state.all then
    table.insert(cmd, state.branch)
  end
  if state.file then
    vim.list_extend(cmd, { "--follow", "--", state.file })
  else
    table.insert(cmd, 3, "--topo-order")
    if state.all then
      table.insert(cmd, "--all")
    end
  end
  local res = vim.system(cmd, { cwd = state.root, text = true }):wait()
  local entries = {}
  for line in vim.gsplit(res.stdout or "", "\n", { trimempty = true }) do
    local f = vim.split(line, SEP, { plain = true })
    if #f >= 6 then
      table.insert(entries, {
        hash = f[1],
        parents = vim.split(f[2], " ", { trimempty = true }),
        date = f[3],
        author = f[4],
        subject = f[5],
        refs = f[6],
      })
    end
  end
  if state.file then
    for i, e in ipairs(entries) do
      e.parents = entries[i + 1] and { entries[i + 1].hash } or {}
    end
  end
  return entries
end

--- Şerit renkleri ve ref etiketi vurguları (renk şemasına bağlı; her çizimde tanımlanır, şema değişse de kalır).
local LANE_HL = { "DiagnosticInfo", "DiagnosticOk", "DiagnosticWarn", "Function", "Constant", "DiagnosticHint", "Special",
  "DiagnosticError" }
local win_of

--- Verilen filetype'ı gösteren panel penceresine odaklanır.
---@param ft string
local function focus_ft(ft)
  local win = win_of(ft)
  if win then
    vim.api.nvim_set_current_win(win)
  end
end

local function define_hl()
  for i, link in ipairs(LANE_HL) do
    vim.api.nvim_set_hl(0, "GitHistoryLane" .. i, { link = link, default = true })
  end
  vim.api.nvim_set_hl(0, "GitHistoryHead", { link = "Title", default = true })
  vim.api.nvim_set_hl(0, "GitHistoryLocal", { link = "DiagnosticOk", default = true })
  vim.api.nvim_set_hl(0, "GitHistoryRemote", { link = "DiagnosticWarn", default = true })
  vim.api.nvim_set_hl(0, "GitHistoryTag", { link = "Constant", default = true })
  vim.api.nvim_set_hl(0, "GitHistorySelected", { link = "Visual", default = true })
end

--- `%D` ref listesini renkli parçalara böler: HEAD, local branch, remote branch (`<remote>/…`), tag.
---@param refs string
---@param remotes string[]
---@return { [1]: string, [2]: string }[]
local function ref_parts(refs, remotes)
  local parts = {}
  local function add(text, hl)
    parts[#parts + 1] = { text, hl }
  end
  for i, ref in ipairs(vim.split(refs, ", ", { plain = true, trimempty = true })) do
    add(i == 1 and "  " or " ", "Normal")
    local head, branch = ref:match("^(HEAD) %-> (.+)$")
    if head then
      add("HEAD → ", "GitHistoryHead")
      ref = branch
    end
    local tag = ref:match("^tag: (.+)$")
    if tag then
      add("◆ " .. tag, "GitHistoryTag")
    elseif ref == "HEAD" then
      add("HEAD", "GitHistoryHead")
    elseif vim.iter(remotes):any(function(r) return vim.startswith(ref, r .. "/") end) then
      add("☁ " .. ref, "GitHistoryRemote")
    else
      add("⎇ " .. ref, "GitHistoryLocal")
    end
  end
  return parts
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

--- Satırları ve `{ satır, başlangıç, bitiş (-1 = satır sonu), vurgu }` işaretlerini salt okunur buffer'a yazar.
---@param buf integer
---@param lines string[]
---@param marks table[]
local function draw(buf, lines, marks)
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  for _, m in ipairs(marks) do
    vim.api.nvim_buf_set_extmark(buf, ns, m[1], m[2], {
      end_row = m[1],
      end_col = m[3] == -1 and #lines[m[1] + 1] or m[3],
      hl_group = m[4],
      line_hl_group = m[5],
    })
  end
end

--- Dal sütununu çizer: Local (HEAD'deki dal ●, upstream'e göre ↑↓) ve Remote grupları. Geçmişi gösterilen dal
--- (`state.branch`, yoksa HEAD) satır vurgusuyla işaretlenir; kapalı grubun dalları gizlenir.
local function render_branches()
  local buf = state.bbuf
  if not (buf and vim.api.nvim_buf_is_valid(buf)) or not state.root then
    return
  end
  local list = require("githistory.branches").list(state.root)
  state.locals = list.locals
  local lines, marks = { " Branches" }, { { 0, 0, -1, "Title" } }
  state.branch_map = {}
  local function add(text, entry, parts)
    local row = #lines
    lines[#lines + 1] = text
    state.branch_map[row + 1] = entry
    for _, m in ipairs(parts or {}) do
      table.insert(marks, { row, m[1], m[2], m[3] })
    end
  end
  local function selected(b)
    return state.branch and not state.all and b.name == state.branch
      or (not state.branch and not state.all and b.current)
  end
  local function group(id, title, items, icon, hl)
    local open = not state.collapsed[id]
    local head = string.format(" %s %s (%d)", open and "▾" or "▸", title, #items)
    add(head, { kind = "group", group = id }, { { 0, -1, "Directory" } })
    if not open then
      return
    end
    if id == "local" and list.detached then
      add("   ● (detached HEAD)", { kind = "head" }, { { 0, -1, "GitHistoryHead" } })
      if not state.branch and not state.all then
        marks[#marks + 1] = { #lines - 1, 0, 0, nil, "GitHistorySelected" }
      end
    end
    for _, b in ipairs(items) do
      local mark = b.current and "● " or (icon .. " ")
      local text = "   " .. mark .. b.name
      local parts = { { 3, 3 + #mark, b.current and "GitHistoryHead" or hl }, { 3 + #mark, #text, b.current and "GitHistoryHead" or "Normal" } }
      if b.track then
        parts[#parts + 1] = { #text, #text + 2 + #b.track, "Comment" }
        text = text .. "  " .. b.track
      end
      add(text, { kind = "branch", branch = b }, parts)
      if selected(b) then
        marks[#marks + 1] = { #lines - 1, 0, 0, nil, "GitHistorySelected" }
      end
    end
  end
  group("local", "Local", list.locals, "⎇", "GitHistoryLocal")
  group("remote", "Remote", list.remotes, "☁", "GitHistoryRemote")
  draw(buf, lines, marks)
end

--- Paneli git log'un güncel haliyle yeniden çizer.
function M.refresh()
  local buf = state.buf
  if not (buf and vim.api.nvim_buf_is_valid(buf)) or not state.root then
    return
  end
  local mode = state.all and "all branches" or (state.branch and ("⎇ " .. state.branch) or "current branch")
  if state.file then
    mode = mode .. " · file: " .. vim.fn.fnamemodify(state.file, ":t")
  end
  local multi = #state.repos > 1
  local header = " Git History · " .. (multi and (vim.fs.basename(state.root) .. " · ") or "") .. mode
  local hint = "   ⏎ diff · f file/repo · a all branches · y copy hash · Tab branches · R refresh"
    .. (multi and " · g choose repo" or "")
    .. " · q close"
  local lines, marks = { header .. hint }, { { 0, 0, #header, "Title" }, { 0, #header, -1, "Comment" } }
  state.line_map = {}
  define_hl()
  local entries = load()
  local rows = require("githistory.graph").layout(entries)
  local width = 0
  for _, cells in ipairs(rows) do
    width = math.max(width, #cells)
  end
  local remotes = vim.split(
    vim.system({ "git", "remote" }, { cwd = state.root, text = true }):wait().stdout or "",
    "\n",
    { trimempty = true }
  )
  for i, e in ipairs(entries) do
    local row = #lines
    local parts = { { " " } }
    for _, cell in ipairs(rows[i]) do
      parts[#parts + 1] = { cell[1], cell[2] and ("GitHistoryLane" .. cell[2]) }
    end
    parts[#parts + 1] = { string.rep(" ", width - #rows[i] + 1) }
    vim.list_extend(parts, {
      { e.hash .. "  ", "Identifier" },
      { e.date .. "  ", "Comment" },
      { fit(e.author, 16) .. "  ", "String" },
      { e.subject, "Normal" },
    })
    vim.list_extend(parts, ref_parts(e.refs, remotes))
    local text, col = "", 0
    for _, p in ipairs(parts) do
      if p[2] then
        table.insert(marks, { row, col, col + #p[1], p[2] })
      end
      text, col = text .. p[1], col + #p[1]
    end
    table.insert(lines, text)
    state.line_map[row + 1] = e.hash
  end
  draw(buf, lines, marks)
  render_branches()
end

--- `.git` klasörünü özyinelemeli izler; HEAD, ref'ler (commit, checkout, dal açma/silme, fetch) ve packed-refs
--- değişince paneli kendiliğinden yeniler. index/objects gibi sık değişen dosyalar yok sayılır.
local function watch()
  if state.watcher then
    state.watcher:stop()
  end
  local path = state.root .. "/.git"
  if vim.fn.isdirectory(path) == 0 then
    return
  end
  state.watcher = vim.uv.new_fs_event()
  local timer = vim.uv.new_timer()
  state.watcher:start(path, { recursive = true }, function(_, name)
    name = (name or ""):gsub("%.lock$", "")
    if name:match("^refs/") or name == "HEAD" or name == "logs/HEAD" or name == "packed-refs" or name == "FETCH_HEAD" then
      timer:start(200, 0, vim.schedule_wrap(M.refresh))
    end
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
    state.root, state.file, state.branch = root, nil, nil
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
  map("<Tab>", function()
    focus_ft(BRANCH_FT)
  end)
  map("q", function()
    M.close_panel(vim.api.nvim_get_current_win())
  end)
  return buf
end

--- Dal sütununda imlecin (ya da `line` satırının) girdisi.
---@param line? integer
local function branch_entry(line)
  return state.branch_map[line or vim.api.nvim_win_get_cursor(0)[1]]
end

--- Dalın geçmişini panelde gösterir (checkout yapmaz); HEAD'deki dal ya da detached HEAD seçilirse mevcut dal moduna
--- döner. "Tüm dallar" modu kapanır.
---@param entry? table
local function preview_branch(entry)
  if not entry or entry.kind == "group" then
    if entry then
      state.collapsed[entry.group] = not state.collapsed[entry.group]
      render_branches()
    end
    return
  end
  local b = entry.branch
  state.branch = b and not b.current and b.name or nil
  state.all = false
  M.refresh()
  local win = win_of(FT)
  if win and vim.api.nvim_buf_line_count(state.buf) > 1 then
    vim.api.nvim_win_set_cursor(win, { 2, 0 })
  end
end

--- Onay alıp dala geçer (remote dal → takip eden local dal). Başarılıysa panel mevcut dalı gösterir.
---@param entry? table
local function checkout(entry)
  local b = entry and entry.branch
  if not b or b.current then
    return
  end
  local target = b.remote and b.name:sub(#b.remote + 2) or b.name
  vim.ui.select({ "Checkout " .. target, "Cancel" }, { prompt = "Switch branch" }, function(choice)
    if not (choice and vim.startswith(choice, "Checkout")) then
      return
    end
    require("githistory.branches").checkout(state.root, b, state.locals, function(ok, msg)
      vim.notify(msg ~= "" and msg or ("Switched to " .. target), ok and vim.log.levels.INFO or vim.log.levels.ERROR,
        { title = "Git" })
      if ok then
        state.branch, state.all = nil, false
        M.refresh()
      end
    end)
  end)
end

--- Dal satırının sağ tık menüsü.
---@param entry? table
local function branch_menu(entry)
  local b = entry and entry.branch
  if not b then
    return
  end
  local items = { { "Show history", preview_branch } }
  if not b.current then
    items[#items + 1] = { "Checkout", checkout }
  end
  items[#items + 1] = { "Copy name", function()
    vim.fn.setreg("+", b.name)
    vim.notify("Copied: " .. b.name)
  end }
  vim.ui.select(items, {
    prompt = b.name,
    format_item = function(i)
      return i[1]
    end,
  }, function(i)
    if i then
      i[2](entry)
    end
  end)
end

--- Dal sütunu buffer'ını oluşturur: ⏎ / tek tık geçmişi gösterir, `c` / çift tık checkout, Space/⏎ grupta aç-kapa.
---@return integer
local function create_branch_buf()
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].filetype = BRANCH_FT
  vim.bo[buf].bufhidden = "hide"
  local function map(lhs, fn)
    vim.keymap.set("n", lhs, function()
      fn(branch_entry())
    end, { buffer = buf, nowait = true })
  end
  map("<CR>", preview_branch)
  map("<LeftRelease>", preview_branch)
  map("<Space>", function(entry)
    if entry and entry.kind == "group" then
      preview_branch(entry)
    end
  end)
  map("c", checkout)
  map("<2-LeftMouse>", checkout)
  map("y", function(entry)
    if entry and entry.branch then
      vim.fn.setreg("+", entry.branch.name)
      vim.notify("Copied: " .. entry.branch.name)
    end
  end)
  map("a", function()
    state.all = not state.all
    M.refresh()
  end)
  map("g", choose_repo)
  map("R", M.refresh)
  map("<Tab>", function()
    focus_ft(FT)
  end)
  map("q", function()
    M.close_panel(vim.api.nvim_get_current_win())
  end)
  require("workbench").on_right_click(BRANCH_FT, function(pos)
    if not (state.bbuf and pos.winid ~= 0 and vim.api.nvim_win_get_buf(pos.winid) == state.bbuf) then
      return false
    end
    local entry = branch_entry(pos.line)
    vim.schedule(function()
      branch_menu(entry)
    end)
    return true
  end)
  return buf
end

--- Bu tabpage'de verilen filetype'ı gösteren pencere.
---@param ft string
---@return integer?
win_of = function(ft)
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == ft then
      return win
    end
  end
end

--- Açık panel penceresini döndürür: odak dal sütunundaysa o (Cmd+7 oradan da kapatsın), değilse geçmiş penceresi.
---@return integer?
function M.find_panel()
  local cur = vim.api.nvim_get_current_win()
  if vim.bo[vim.api.nvim_win_get_buf(cur)].filetype == BRANCH_FT then
    return cur
  end
  return win_of(FT) or win_of(BRANCH_FT)
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
  if not (state.bbuf and vim.api.nvim_buf_is_valid(state.bbuf)) then
    state.bbuf = create_branch_buf()
  end
  local workbench = require("workbench")
  local bwin, win = win_of(BRANCH_FT), win_of(FT)
  if not bwin then
    if win then
      bwin = vim.api.nvim_open_win(state.bbuf, false, { split = "left", win = win, width = BRANCH_WIDTH })
      workbench.mark_bottom(bwin, BRANCH_WIDTH)
    else
      bwin = workbench.open_bottom(state.bbuf, { enter = false, width = BRANCH_WIDTH })
    end
  end
  if not win then
    win = workbench.open_bottom(state.buf, { after = bwin })
    vim.api.nvim_win_set_width(bwin, BRANCH_WIDTH)
  end
  vim.api.nvim_set_current_win(win)
  for w, opts in pairs({ [bwin] = { cursorline = true, wrap = false }, [win] = { cursorline = true, wrap = false } }) do
    for k, v in pairs(opts) do
      vim.wo[w][k] = v
    end
  end
  M.refresh()
  if vim.api.nvim_buf_line_count(state.buf) > 1 then
    vim.api.nvim_win_set_cursor(win, { 2, 0 })
  end
end

--- Panel penceresini (geçmiş ve dal sütunu birlikte) kapatır.
---@param win integer
function M.close_panel(win)
  for _, w in ipairs({ win, win_of(BRANCH_FT), win_of(FT) }) do
    if w and vim.api.nvim_win_is_valid(w) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
      vim.api.nvim_win_hide(w)
    end
  end
end

return M
