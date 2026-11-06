local M = {}

local ns = vim.api.nvim_create_namespace("gitstatus")
local FT = "gitstatus"
local WIDTH = 40
local HINT = " s stage · u unstage · d diff · X discard"

local SECTIONS = {
  { id = "conflicts", title = "Conflicts" },
  { id = "staged", title = "Staged" },
  { id = "changes", title = "Changes" },
  { id = "untracked", title = "Untracked" },
}
local CONFLICT = { DD = true, AU = true, UD = true, UA = true, DU = true, AA = true, UU = true }
local CODE_HL = {
  A = "Added",
  M = "Changed",
  R = "Changed",
  C = "Changed",
  T = "Changed",
  D = "Removed",
  ["?"] = "Added",
  U = "DiagnosticError",
}

---@class GitStatusEntry
---@field repo string repo kökü (mutlak)
---@field section "conflicts"|"staged"|"changes"|"untracked"
---@field path string repo köküne göre yol
---@field orig string? yeniden adlandırmada eski yol
---@field code string tek harflik durum (A, M, D, R, ?, U ...)

local state = {
  buf = nil,
  ---@type string[]
  repos = {},
  line_map = {},
  collapsed = {},
  watchers = {},
  timer = nil,
}

--- git'i repo kökünde çalıştırır; başarısızsa hatayı bildirir.
---@param repo string
---@param args string[]
---@return boolean ok, string stdout
function M.git(repo, args)
  local res = vim.system(vim.list_extend({ "git" }, args), { cwd = repo, text = true }):wait()
  if res.code ~= 0 then
    local msg = vim.trim((res.stderr or "") ~= "" and res.stderr or (res.stdout or ""))
    vim.notify(msg ~= "" and msg or ("git " .. args[1] .. " failed"), vim.log.levels.ERROR, { title = "Git" })
  end
  return res.code == 0, res.stdout or ""
end
local git = M.git

--- `git status --porcelain=v1 -z` çıktısını bölümlere ayırır. Hem index'te hem çalışma ağacında değişen dosya
--- (ör. `MM`) hem Staged hem Changes altında görünür. `--no-optional-locks`: status index'i yazmasın, yoksa
--- `.git` izleyicisi kendi yenilemesiyle tekrar tetiklenirdi.
---@param repo string
---@return { branch: string, conflicts: GitStatusEntry[], staged: GitStatusEntry[], changes: GitStatusEntry[], untracked: GitStatusEntry[] }
function M.status(repo)
  local out = { branch = "", conflicts = {}, staged = {}, changes = {}, untracked = {} }
  local ok, stdout = git(repo, { "--no-optional-locks", "status", "--porcelain=v1", "-z", "--branch", "--untracked-files=all" })
  if not ok then
    return out
  end
  local items = vim.split(stdout, "\0", { plain = true })
  local i = 1
  while i <= #items do
    local item = items[i]
    if item:sub(1, 3) == "## " then
      out.branch = item:sub(4):gsub("%.%.%.%S+", "")
    elseif #item > 3 then
      local x, y, path = item:sub(1, 1), item:sub(2, 2), item:sub(4)
      local function add(section, code, orig)
        table.insert(out[section], { repo = repo, section = section, path = path, orig = orig, code = code })
      end
      if x == "?" then
        add("untracked", "?")
      elseif CONFLICT[x .. y] then
        add("conflicts", "U")
      else
        local orig
        if x == "R" or x == "C" then
          i = i + 1
          orig = items[i]
        end
        if x ~= " " then
          add("staged", x, orig)
        end
        if y ~= " " then
          add("changes", y)
        end
      end
    end
    i = i + 1
  end
  return out
end
local status = M.status

--- Dosya ikonu ve vurgusu (mini.icons; yoksa düz ikon).
---@param path string
---@return string icon, string hl
local function file_icon(path)
  local ok, icons = pcall(require, "mini.icons")
  if ok then
    local icon, hl = icons.get("file", path)
    return icon, hl
  end
  return "", "Normal"
end

--- Satır girdisinin yenilemeden sonra aynı satırı bulmak için anahtarı.
---@param entry table?
---@return string?
local function line_key(entry)
  if not entry then
    return nil
  end
  if entry.kind == "file" then
    return table.concat({ entry.entry.repo, entry.entry.section, entry.entry.path }, "\0")
  end
  return entry.key or entry.repo
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

--- Paneli tüm repoların güncel durumuyla yeniden çizer: repo başlığı (ad + dal), altında boş olmayan
--- Conflicts / Staged / Changes / Untracked bölümleri. İmleç aynı girdide kalır (girdi gittiyse aynı satırda).
function M.refresh()
  local buf = state.buf
  if not (buf and vim.api.nvim_buf_is_valid(buf)) then
    return
  end
  local win = M.find_panel()
  local cursor = win and vim.api.nvim_win_get_cursor(win)[1]
  local key = cursor and line_key(state.line_map[cursor])
  local lines, marks, map = { HINT }, { { 0, 0, #HINT, "Comment" } }, {}
  local function add(parts, entry)
    local row, text = #lines, ""
    for _, p in ipairs(parts) do
      table.insert(marks, { row, #text, #text + #p[1], p[2] })
      text = text .. p[1]
    end
    table.insert(lines, text)
    map[row + 1] = entry
  end
  for _, repo in ipairs(state.repos) do
    local st = status(repo)
    add({ { " " .. vim.fs.basename(repo), "Title" }, { "  " .. st.branch, "Comment" } }, { kind = "repo", repo = repo })
    local empty = true
    for _, s in ipairs(SECTIONS) do
      local list = st[s.id]
      if #list > 0 then
        empty = false
        local k = repo .. "\0" .. s.id
        local open = not state.collapsed[k]
        add(
          { { ("  %s %s"):format(open and "▾" or "▸", s.title), "Directory" }, { (" (%d)"):format(#list), "Comment" } },
          { kind = "section", repo = repo, section = s.id, entries = list, key = k }
        )
        for _, e in ipairs(open and list or {}) do
          local icon, icon_hl = file_icon(e.path)
          local dir = vim.fs.dirname(e.path)
          local detail = e.orig and ("  ← " .. e.orig) or (dir ~= "." and ("  " .. dir) or "")
          add({
            { "    " .. e.code .. " ", CODE_HL[e.code] or "Normal" },
            { icon .. " ", icon_hl },
            { vim.fs.basename(e.path), "Normal" },
            { detail, "Comment" },
          }, { kind = "file", entry = e })
        end
      end
    end
    if empty then
      add({ { "    No changes", "Comment" } }, { kind = "repo", repo = repo })
    end
  end
  state.line_map = map
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  for _, m in ipairs(marks) do
    if m[3] > m[2] then
      vim.api.nvim_buf_set_extmark(buf, ns, m[1], m[2], { end_row = m[1], end_col = m[3], hl_group = m[4] })
    end
  end
  if win then
    local target = math.min(math.max(cursor, 2), #lines)
    for lnum, entry in pairs(map) do
      if key and line_key(entry) == key then
        target = lnum
      end
    end
    vim.api.nvim_win_set_cursor(win, { target, 0 })
  end
end

--- Yenilemeyi 300 ms erteler (art arda gelen dosya olaylarını birleştirir); panel görünür değilse iş yapmaz,
--- açılışta zaten yenilenir.
local function schedule_refresh()
  state.timer = state.timer or vim.uv.new_timer()
  state.timer:start(
    300,
    0,
    vim.schedule_wrap(function()
      if M.find_panel() then
        M.refresh()
      end
    end)
  )
end

--- Her repo kökünü özyinelemeli izler (macOS FSEvents): çalışma ağacındaki dış değişiklikler (Claude, başka araç)
--- ve `.git/index` / `HEAD` / refs değişimleri (stage, commit, checkout) paneli yeniler. `.git` altındaki diğer
--- dosyalar (kilitler, nesneler) yok sayılır.
--- shortcut: özyinelemeli fs_event Linux'ta desteklenmez (orada sadece kök klasör izlenir); gerekirse BufWritePost /
--- FocusGained yeter, değilse zamanlayıcıya geçilir.
local function watch()
  for _, w in ipairs(state.watchers) do
    w:stop()
  end
  state.watchers = {}
  for _, repo in ipairs(state.repos) do
    local w = vim.uv.new_fs_event()
    local ok = w and w:start(repo, { recursive = true }, function(err, file)
      file = file or ""
      if err or (file:match("^%.git/") and not (file == ".git/index" or file == ".git/HEAD" or file:match("^%.git/refs/"))) then
        return
      end
      schedule_refresh()
    end)
    if ok then
      table.insert(state.watchers, w)
    end
  end
end

--- Girdilerin yolları (yeniden adlandırmada eski yol da; unstage ikisini birden geri almalı).
---@param list GitStatusEntry[]
---@return string[]
local function paths(list)
  local out = {}
  for _, e in ipairs(list) do
    table.insert(out, e.path)
    if e.orig then
      table.insert(out, e.orig)
    end
  end
  return out
end

--- Dosyaları index'e ekler (silinmiş dosyada silmeyi stage eder; conflict'te çözüldü işaretler).
---@param repo string
---@param list GitStatusEntry[]
local function stage(repo, list)
  git(repo, vim.list_extend({ "add", "-A", "--" }, paths(list)))
  M.refresh()
end

--- Dosyaları index'ten çıkarır; çalışma ağacı değişmez. `reset` ilk commit'ten önce (HEAD yokken) de çalışır.
---@param repo string
---@param list GitStatusEntry[]
local function unstage(repo, list)
  git(repo, vim.list_extend({ "reset", "-q", "--" }, paths(list)))
  M.refresh()
end

--- Reponun tüm değişikliklerini (untracked dahil) stage eder.
---@param repo string
local function stage_all(repo)
  git(repo, { "add", "-A" })
  M.refresh()
end

--- Reponun tüm stage'lenmiş değişikliklerini geri alır (çalışma ağacı değişmez).
---@param repo string
local function unstage_all(repo)
  git(repo, { "reset", "-q" })
  M.refresh()
end

local open_file

--- Dosyanın diff'ini kod editöründe tab olarak açar: Staged → index ↔ HEAD, diğerleri çalışma ağacı ↔ index.
--- `gitdiff` modülü yoksa (ya da conflict ise) dosyanın kendisi açılır.
---@param e GitStatusEntry
local function show_diff(e)
  local ok, gitdiff = pcall(require, "gitdiff")
  if not ok or e.section == "conflicts" then
    return open_file(e, true)
  end
  gitdiff.open({ path = e.repo .. "/" .. e.path, staged = e.section == "staged", cwd = e.repo })
end

--- Dosyayı son kullanılan kod penceresinde açar. Çalışma ağacında silinmişse diff'i gösterir.
---@param e GitStatusEntry
---@param no_diff boolean?
function open_file(e, no_diff)
  local path = e.repo .. "/" .. e.path
  if not no_diff and not vim.uv.fs_stat(path) then
    return show_diff(e)
  end
  require("previewtab").open(path, function()
    require("workbench").code({ pinned = false })
    vim.cmd.edit(vim.fn.fnameescape(path))
  end)
end

--- Çalışma ağacındaki değişikliği geri alır (Changes: index'teki hale döner) ya da untracked dosyayı siler.
--- Geri dönüşü olmadığı için her zaman onay ister (varsayılan seçenek "Cancel"). Staged ve conflict
--- girdilerinde yapılmaz (önce unstage / çözüm).
---@param e GitStatusEntry
local function discard(e)
  if e.section == "staged" or e.section == "conflicts" then
    local msg = e.section == "staged" and "Unstage the file first to discard its changes" or "Resolve the conflict first"
    vim.notify(msg, vim.log.levels.WARN, { title = "Git" })
    return
  end
  local untracked = e.section == "untracked"
  local action = untracked and "Delete file" or "Discard changes"
  local prompt = (untracked and "Delete untracked file %s?" or "Discard changes to %s?") .. " This cannot be undone."
  vim.ui.select({ "Cancel", action }, { prompt = prompt:format(e.path) }, function(choice)
    if choice ~= action then
      return
    end
    M.discard_entry(e)
  end)
end

--- Onaysız geri alma (çağıran onayı almış olmalı: panel menüsü ya da `lessmcp.approve`).
---@param e GitStatusEntry Changes ya da Untracked girdisi
function M.discard_entry(e)
  git(e.repo, e.section == "untracked" and { "clean", "-f", "-q", "--", e.path } or { "checkout", "-q", "--", e.path })
  M.refresh()
  vim.cmd.checktime()
end

--- İmlecin (ya da verilen satırın) girdisi.
---@param lnum integer?
---@return table?
local function current(lnum)
  return state.line_map[lnum or vim.api.nvim_win_get_cursor(0)[1]]
end

--- Satırda `s`: dosya / bölüm / repo başlığına göre stage eder.
---@param entry table?
local function stage_line(entry)
  if not entry then
    return
  end
  if entry.kind == "repo" then
    stage_all(entry.repo)
  elseif entry.kind == "section" and entry.section ~= "staged" then
    stage(entry.repo, entry.entries)
  elseif entry.kind == "file" and entry.entry.section ~= "staged" then
    stage(entry.entry.repo, { entry.entry })
  end
end

--- Satırda `u`: Staged dosyasını / Staged bölümünü / repo başlığında tüm repoyu unstage eder.
---@param entry table?
local function unstage_line(entry)
  if not entry then
    return
  end
  if entry.kind == "repo" then
    unstage_all(entry.repo)
  elseif entry.kind == "section" and entry.section == "staged" then
    unstage(entry.repo, entry.entries)
  elseif entry.kind == "file" and entry.entry.section == "staged" then
    unstage(entry.entry.repo, { entry.entry })
  end
end

--- Bölümü açar/kapatır; `open` verilirse o duruma getirir.
---@param entry table bölüm girdisi
---@param open boolean?
local function set_open(entry, open)
  if open == nil then
    open = state.collapsed[entry.key] == true
  end
  state.collapsed[entry.key] = not open or nil
  M.refresh()
end

--- Enter / çift tık: dosyayı açar, bölüm başlığını açıp kapatır.
---@param entry table?
local function activate(entry)
  if entry and entry.kind == "file" then
    open_file(entry.entry)
  elseif entry and entry.kind == "section" then
    set_open(entry)
  end
end

--- Satırın eylem menüsü (sağ tık / `m`): girdiye uygun eylemler + repo geneli işlemler.
---@param entry table?
function M.menu(entry)
  if not entry then
    return
  end
  local repo = entry.repo or entry.entry.repo
  local list = {}
  local function add(label, fn)
    table.insert(list, { label = label, fn = fn })
  end
  if entry.kind == "file" then
    local e = entry.entry
    add("Open file", function() open_file(e) end)
    add("Show diff", function() show_diff(e) end)
    if e.section == "staged" then
      add("Unstage", function() unstage(repo, { e }) end)
    else
      add(e.section == "conflicts" and "Mark as resolved (stage)" or "Stage", function() stage(repo, { e }) end)
    end
    if e.section == "changes" then
      add("Discard changes…", function() discard(e) end)
    elseif e.section == "untracked" then
      add("Delete file…", function() discard(e) end)
    end
  elseif entry.kind == "section" then
    if entry.section == "staged" then
      add("Unstage section", function() unstage(repo, entry.entries) end)
    else
      add("Stage section", function() stage(repo, entry.entries) end)
    end
  end
  add("Stage all", function() stage_all(repo) end)
  add("Unstage all", function() unstage_all(repo) end)
  add("Refresh", M.refresh)
  vim.ui.select(list, {
    prompt = entry.kind == "file" and entry.entry.path or vim.fs.basename(repo),
    format_item = function(a)
      return a.label
    end,
  }, function(choice)
    if choice then
      choice.fn()
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
    vim.keymap.set("n", lhs, function()
      fn(current())
    end, { buffer = buf, nowait = true })
  end
  local function on_file(fn)
    return function(entry)
      if entry and entry.kind == "file" then
        fn(entry.entry)
      end
    end
  end
  map("<CR>", activate)
  map("<2-LeftMouse>", activate)
  map("o", on_file(open_file))
  map("d", on_file(show_diff))
  map("s", stage_line)
  map("u", unstage_line)
  map("S", function(entry)
    if entry then
      stage_all(entry.repo or entry.entry.repo)
    end
  end)
  map("U", function(entry)
    if entry then
      unstage_all(entry.repo or entry.entry.repo)
    end
  end)
  map("X", on_file(discard))
  map("m", M.menu)
  map("<Right>", function(entry)
    if entry and entry.kind == "section" then
      set_open(entry, true)
    end
  end)
  map("<Left>", function(entry)
    if entry and entry.kind == "section" then
      set_open(entry, false)
    end
  end)
  map("R", M.refresh)
  map("q", function()
    M.close_panel(vim.api.nvim_get_current_win())
  end)
  require("workbench").on_right_click(FT, function(pos)
    if not (state.buf and pos.winid ~= 0 and vim.api.nvim_win_get_buf(pos.winid) == state.buf) then
      return false
    end
    local entry = current(pos.line)
    vim.schedule(function()
      M.menu(entry)
    end)
    return true
  end)
  return buf
end

--- Paneli sol kenarda tam boy açar (workbench alt şeridi onun da altına alır). Workspace'te tüm repolar
--- gösterilir; repo listesi değiştiyse izleyiciler yeniden kurulur.
function M.open_panel()
  local repos = require("githistory").repos()
  if #repos == 0 then
    vim.notify("This folder is not a git repository", vim.log.levels.WARN)
    return
  end
  if table.concat(repos, "\n") ~= table.concat(state.repos, "\n") or #state.watchers == 0 then
    state.repos = repos
    watch()
  end
  if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
    state.buf = create_buf()
  end
  local win = vim.api.nvim_open_win(state.buf, true, { split = "left", win = -1, width = WIDTH })
  for opt, val in pairs({
    winfixwidth = true,
    number = false,
    relativenumber = false,
    signcolumn = "no",
    foldcolumn = "0",
    statuscolumn = "",
    cursorline = true,
    wrap = false,
    spell = false,
    list = false,
  }) do
    vim.wo[win][opt] = val
  end
  M.refresh()
end

--- Panel penceresini kapatır.
---@param win integer
function M.close_panel(win)
  if vim.api.nvim_win_is_valid(win) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
    vim.api.nvim_win_hide(win)
  end
end

vim.api.nvim_create_autocmd({ "BufWritePost", "FocusGained" }, {
  group = vim.api.nvim_create_augroup("gitstatus", { clear = true }),
  callback = schedule_refresh,
})

return M
