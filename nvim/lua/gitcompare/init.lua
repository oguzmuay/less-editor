--- Git paneli için IntelliJ tarzı karşılaştırma ekranları (kod alanında, paneller yerinde kalır).
---
--- Diff (`M.diff`): solda HEAD (salt okunur), sağda çalışma ağacındaki dosyanın kendisi; ikisi diff modunda.
--- Her değişikliğin sağ dosyadaki ilk satırında ⟲ işareti: tıkla ya da `Space r` → o değişiklik HEAD'deki haline
--- döner (buffer'da, `u` ile geri alınır, kaydedince geçerli). Winbar'da "Rollback all" ve "Close".
---
--- Merge (`M.merge`): solda bizim taraf (`:2:`), ortada conflict işaretli dosyanın kendisi (sonuç), sağda gelen
--- taraf (`:3:`). Ortadaki her conflict bloğunun başında « Left / Right » / Both butonları ve `Space m l/r/b`.
--- "Accept all left/right" yalnız henüz işaretleri duran bloklara uygulanır: tek tek çözülmüş bloklar değişmez.
--- Bloklar kalmayınca "Mark resolved" dosyayı kaydedip `git add` yapar.
---
--- Aynı anda tek ekran açık olur. Kapanma: `q`, winbar'da ✕, yan pencerelerden birinin kapanması ya da kod
--- penceresinde başka dosyaya geçmek.
local M = {}

local ns = vim.api.nvim_create_namespace("gitcompare")
local MARK = { start = "^<<<<<<<", base = "^|||||||", sep = "^=======$", finish = "^>>>>>>>" }

---@class GitCompareSession
---@field kind "diff"|"merge"
---@field repo string
---@field path string repo-göreli yol
---@field buf integer gerçek dosyanın buffer'ı
---@field win integer gerçek dosyanın penceresi
---@field sides { buf: integer, win: integer }[]
---@field base string[]? diff: HEAD satırları
---@field hunks table[]? diff: vim.diff indeksleri
---@field blocks table[]? merge: conflict blokları
---@field labels string[]? merge: { ours, theirs }
---@field group integer
---@field closing boolean?

---@type GitCompareSession?
local S

--- Vurgular (renk şeması değişince yeniden tanımlanır).
local function set_highlights()
  local hl = function(name, link)
    vim.api.nvim_set_hl(0, name, { default = true, link = link })
  end
  hl("GitCompareTitle", "Title")
  hl("GitCompareButton", "DiagnosticInfo")
  hl("GitCompareDanger", "DiagnosticWarn")
  hl("GitCompareRollback", "DiagnosticWarn")
  hl("GitMergeOurs", "DiffAdd")
  hl("GitMergeTheirs", "DiffChange")
  hl("GitMergeBase", "DiffDelete")
  hl("GitMergeMarker", "NonText")
end
set_highlights()
vim.api.nvim_create_autocmd("ColorScheme", {
  group = vim.api.nvim_create_augroup("gitcompare.hl", { clear = true }),
  callback = set_highlights,
})

---@param args string[]
---@param cwd string
---@return boolean ok, string stdout
local function git(args, cwd)
  local res = vim.system(vim.list_extend({ "git", "-c", "core.quotepath=false" }, args), { cwd = cwd, text = true }):wait()
  return res.code == 0, res.stdout or ""
end

--- `git show <spec>` içeriğini satırlar olarak döndürür (yoksa nil).
---@return string[]?
local function show(repo, spec)
  local ok, out = git({ "show", spec }, repo)
  if not ok then
    return nil
  end
  local lines = vim.split(out, "\n", { plain = true })
  if lines[#lines] == "" then
    table.remove(lines)
  end
  return lines
end

--- Winbar'da tıklanabilir parça (`GitCompareClick`, minwid = eylem no).
local ACTIONS = { "rollback_all", "close", "accept_all_ours", "accept_all_theirs", "resolve" }
local function button(action, text, hl)
  local id = assert(vim.tbl_contains(ACTIONS, action) and vim.fn.index(ACTIONS, action) + 1)
  return ("%%#%s#%%%d@v:lua.GitCompareClick@%s%%X%%*"):format(hl or "GitCompareButton", id, text)
end

--- Yan pencere (salt okunur, scratch) oluşturur.
---@param lines string[]
---@param name string
---@param ft string
---@param split "left"|"right"
---@param title string
---@return { buf: integer, win: integer }
local function side(lines, name, ft, split, title)
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].bufhidden = "wipe"
  vim.bo[buf].modifiable = false
  vim.b[buf].gitcompare = true
  pcall(vim.api.nvim_buf_set_name, buf, name)
  vim.bo[buf].filetype = ft
  local win = vim.api.nvim_open_win(buf, false, { split = split, win = S.win })
  vim.wo[win].winbar = title
  vim.wo[win].winfixbuf = true
  for _, lhs in ipairs({ "q" }) do
    vim.keymap.set("n", lhs, M.close, { buffer = buf, nowait = true, desc = "Close compare view" })
  end
  return { buf = buf, win = win }
end

-- Diff ---------------------------------------------------------------------------------------------------------

--- HEAD ile buffer'ın güncel hali arasındaki değişiklikleri hesaplar, ⟲ işaretlerini koyar.
local function diff_update()
  if not (S and S.kind == "diff" and vim.api.nvim_buf_is_valid(S.buf)) then
    return
  end
  local cur = vim.api.nvim_buf_get_lines(S.buf, 0, -1, false)
  local a, b = table.concat(S.base, "\n") .. "\n", table.concat(cur, "\n") .. "\n"
  S.hunks = vim.diff(a, b, { result_type = "indices", algorithm = "histogram" }) --[[@as table[] ]]
  vim.api.nvim_buf_clear_namespace(S.buf, ns, 0, -1)
  for _, h in ipairs(S.hunks) do
    local line = math.max(1, math.min(h[4] > 0 and h[3] or h[3] + 1, #cur))
    vim.api.nvim_buf_set_extmark(S.buf, ns, line - 1, 0, {
      sign_text = "⟲",
      sign_hl_group = "GitCompareRollback",
      priority = 200,
    })
  end
  vim.cmd("redrawstatus!")
end

--- Satıra (sağ dosyada `b`, sol HEAD'de `a` tarafı) denk gelen değişiklik.
---@param line integer
---@param left boolean
---@return table?
local function hunk_at(line, left)
  for _, h in ipairs(S.hunks or {}) do
    local start, count = h[3], h[4]
    if left then
      start, count = h[1], h[2]
    end
    if count == 0 and (line == start or line == start + 1) or line >= start and line < start + count then
      return h
    end
  end
end

--- Değişikliği HEAD'deki haline döndürür (buffer'da; kaydedilmez).
---@param h table vim.diff indeksi { a_start, a_count, b_start, b_count }
local function rollback(h)
  local repl = vim.list_slice(S.base, h[1], h[1] + h[2] - 1)
  local first = h[4] == 0 and h[3] or h[3] - 1
  vim.api.nvim_buf_set_lines(S.buf, first, first + h[4], false, h[2] == 0 and {} or repl)
  diff_update()
end

--- İmlecin üstündeki değişikliği geri alır (sol ya da sağ pencereden).
function M.rollback()
  if not (S and S.kind == "diff") then
    return
  end
  diff_update()
  local win = vim.api.nvim_get_current_win()
  local left = win ~= S.win
  local h = hunk_at(vim.api.nvim_win_get_cursor(win)[1], left)
  if not h then
    vim.notify("No change under the cursor", vim.log.levels.INFO)
    return
  end
  rollback(h)
end

--- Tüm değişiklikleri HEAD'deki haline döndürür (buffer'da, `u` ile geri alınır).
function M.rollback_all()
  if S and S.kind == "diff" then
    vim.api.nvim_buf_set_lines(S.buf, 0, -1, false, S.base)
    diff_update()
  end
end

--- Diff ekranının winbar'ı (sağ pencere).
---@return string
function M.diff_winbar()
  if not S then
    return ""
  end
  local n = #(S.hunks or {})
  return table.concat({
    "%#GitCompareTitle# Working tree%* ",
    ("%%#Comment#%d change%s%%*"):format(n, n == 1 and "" or "s"),
    "%=",
    "%#Comment#]c [c move · Space r rollback · ⟲ click%*  ",
    button("rollback_all", " ⟲ Rollback all ", "GitCompareDanger"),
    " ",
    button("close", " ✕ Close "),
  })
end

--- Dosyanın HEAD ile çalışma ağacı arasındaki farkını yan yana açar.
---@param opts { repo: string, path: string } path repo-göreli
function M.diff(opts)
  M.close()
  local abs = opts.repo .. "/" .. opts.path
  local win = require("workbench").code_win() or vim.api.nvim_get_current_win()
  vim.api.nvim_set_current_win(win)
  vim.cmd.edit(vim.fn.fnameescape(abs))
  local buf = vim.api.nvim_get_current_buf()
  S = {
    kind = "diff",
    repo = opts.repo,
    path = opts.path,
    buf = buf,
    win = win,
    sides = {},
    base = show(opts.repo, "HEAD:" .. opts.path) or {},
  }
  local left = side(
    S.base,
    "gitcompare://HEAD/" .. opts.path,
    vim.bo[buf].filetype,
    "left",
    "%#GitCompareTitle# HEAD%* %#Comment#(read-only)%*"
  )
  S.sides = { left }
  vim.wo[win].winbar = "%!v:lua.require'gitcompare'.diff_winbar()"
  local width = math.floor((vim.api.nvim_win_get_width(win) + vim.api.nvim_win_get_width(left.win)) / 2)
  vim.api.nvim_win_set_width(left.win, width)
  for _, w in ipairs({ left.win, win }) do
    vim.api.nvim_win_call(w, function()
      vim.cmd("diffthis")
    end)
  end
  vim.keymap.set("n", "<leader>r", M.rollback, { buffer = left.buf, desc = "Rollback change" })
  M.attach({ "<leader>r", M.rollback, "Rollback change" })
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI", "BufWritePost" }, {
    group = S.group,
    buffer = buf,
    callback = function()
      vim.schedule(diff_update)
    end,
  })
  diff_update()
  vim.api.nvim_win_set_cursor(win, { S.hunks[1] and math.max(1, S.hunks[1][3]) or 1, 0 })
end

-- Merge --------------------------------------------------------------------------------------------------------

--- Ortadaki dosyadaki conflict bloklarını bulur (1 tabanlı satırlar; `base` diff3 stilinde).
---@param lines string[]
---@return { start: integer, base: integer?, sep: integer, finish: integer }[]
local function parse(lines)
  local blocks, cur = {}, nil
  for i, l in ipairs(lines) do
    if l:find(MARK.start) then
      cur = { start = i }
    elseif cur and not cur.sep and l:find(MARK.base) then
      cur.base = i
    elseif cur and not cur.sep and l:find(MARK.sep) then
      cur.sep = i
    elseif cur and cur.sep and l:find(MARK.finish) then
      cur.finish = i
      blocks[#blocks + 1] = cur
      cur = nil
    end
  end
  return blocks
end

local MERGE_BUTTONS = {
  { "ours", "« Accept left" },
  { "theirs", "Accept right »" },
  { "both", "Both" },
}

--- Blokları yeniden bulur, vurgular ve butonları çizer.
local function merge_update()
  if not (S and S.kind == "merge" and vim.api.nvim_buf_is_valid(S.buf)) then
    return
  end
  local lines = vim.api.nvim_buf_get_lines(S.buf, 0, -1, false)
  S.blocks = parse(lines)
  vim.api.nvim_buf_clear_namespace(S.buf, ns, 0, -1)
  local function hl_lines(from, to, group)
    for l = from, to do
      vim.api.nvim_buf_set_extmark(S.buf, ns, l - 1, 0, { line_hl_group = group, priority = 50 })
    end
  end
  for _, b in ipairs(S.blocks) do
    local ours_end = (b.base or b.sep) - 1
    hl_lines(b.start + 1, ours_end, "GitMergeOurs")
    if b.base then
      hl_lines(b.base + 1, b.sep - 1, "GitMergeBase")
    end
    hl_lines(b.sep + 1, b.finish - 1, "GitMergeTheirs")
    for _, l in ipairs({ b.start, b.base, b.sep, b.finish }) do
      hl_lines(l, l, "GitMergeMarker")
    end
    local chunks = { { "  " } }
    for i, btn in ipairs(MERGE_BUTTONS) do
      chunks[#chunks + 1] = { " " .. btn[2] .. " ", "GitCompareButton" }
      if i < #MERGE_BUTTONS then
        chunks[#chunks + 1] = { "│", "Comment" }
      end
    end
    vim.api.nvim_buf_set_extmark(S.buf, ns, b.start - 1, 0, {
      virt_text = chunks,
      virt_text_pos = "eol",
      sign_text = "⚡",
      sign_hl_group = "GitCompareDanger",
    })
  end
  vim.cmd("redrawstatus!")
end

--- Bloğu seçilen tarafla değiştirir: "ours" (sol), "theirs" (sağ), "both" (önce sol sonra sağ).
---@param b table
---@param which "ours"|"theirs"|"both"
local function accept(b, which)
  local lines = vim.api.nvim_buf_get_lines(S.buf, b.start - 1, b.finish, false)
  local rel = function(l)
    return l - b.start + 1
  end
  local ours = vim.list_slice(lines, 2, rel(b.base or b.sep) - 1)
  local theirs = vim.list_slice(lines, rel(b.sep) + 1, rel(b.finish) - 1)
  local repl = which == "ours" and ours or which == "theirs" and theirs or vim.list_extend(vim.list_extend({}, ours), theirs)
  vim.api.nvim_buf_set_lines(S.buf, b.start - 1, b.finish, false, repl)
end

--- İmleçteki (yoksa imleçten sonraki ilk) bloğu kabul eder.
---@param which "ours"|"theirs"|"both"
function M.accept(which)
  if not (S and S.kind == "merge") then
    return
  end
  merge_update()
  local line = vim.api.nvim_win_get_cursor(S.win)[1]
  for _, b in ipairs(S.blocks) do
    if line <= b.finish then
      accept(b, which)
      merge_update()
      return
    end
  end
  vim.notify("No conflict at or below the cursor", vim.log.levels.INFO)
end

--- Henüz çözülmemiş tüm blokları aynı tarafla kabul eder (alttan üste; satır numaraları kaymasın).
---@param which "ours"|"theirs"
function M.accept_all(which)
  if not (S and S.kind == "merge") then
    return
  end
  merge_update()
  for i = #S.blocks, 1, -1 do
    accept(S.blocks[i], which)
  end
  merge_update()
end

M.accept_all_ours = function()
  M.accept_all("ours")
end
M.accept_all_theirs = function()
  M.accept_all("theirs")
end

--- Sonraki / önceki conflict bloğuna gider.
---@param dir 1|-1
function M.jump(dir)
  if not (S and S.kind == "merge") then
    return
  end
  local line = vim.api.nvim_win_get_cursor(S.win)[1]
  local list = dir > 0 and S.blocks or vim.fn.reverse(vim.deepcopy(S.blocks))
  for _, b in ipairs(list) do
    if dir > 0 and b.start > line or dir < 0 and b.start < line then
      vim.api.nvim_win_set_cursor(S.win, { b.start, 0 })
      return
    end
  end
end

--- Conflict kalmadıysa dosyayı kaydeder, `git add` ile çözüldü işaretler ve ekranı kapatır.
function M.resolve()
  if not (S and S.kind == "merge") then
    return
  end
  merge_update()
  if #S.blocks > 0 then
    vim.notify(("%d conflict(s) left"):format(#S.blocks), vim.log.levels.WARN)
    return
  end
  local repo, path, buf = S.repo, S.path, S.buf
  vim.api.nvim_buf_call(buf, function()
    vim.cmd("silent write")
  end)
  local ok = git({ "add", "--", path }, repo)
  M.close()
  if ok then
    vim.notify(path .. " marked as resolved")
  else
    vim.notify("git add failed for " .. path, vim.log.levels.ERROR)
  end
  local gs = package.loaded["gitstatus"]
  if gs then
    gs.refresh()
  end
end

--- Merge ekranının winbar'ı (orta pencere).
---@return string
function M.merge_winbar()
  if not S then
    return ""
  end
  local n = #(S.blocks or {})
  local parts = { "%#GitCompareTitle# Result%* " }
  if n > 0 then
    parts[#parts + 1] = ("%%#GitCompareDanger#%d conflict%s left%%*"):format(n, n == 1 and "" or "s")
    parts[#parts + 1] = "%="
    parts[#parts + 1] = button("accept_all_ours", " « Accept all left ")
    parts[#parts + 1] = " "
    parts[#parts + 1] = button("accept_all_theirs", " Accept all right » ")
  else
    parts[#parts + 1] = "%#DiagnosticOk#all conflicts resolved%*"
    parts[#parts + 1] = "%="
    parts[#parts + 1] = button("resolve", " ✓ Save & mark resolved ", "DiagnosticOk")
  end
  parts[#parts + 1] = " "
  parts[#parts + 1] = button("close", " ✕ Close ")
  return table.concat(parts)
end

--- Conflict'li dosya için üç bölmeli merge ekranını açar.
---@param opts { repo: string, path: string } path repo-göreli
function M.merge(opts)
  M.close()
  local abs = opts.repo .. "/" .. opts.path
  local win = require("workbench").code_win() or vim.api.nvim_get_current_win()
  vim.api.nvim_set_current_win(win)
  vim.cmd.edit(vim.fn.fnameescape(abs))
  local buf = vim.api.nvim_get_current_buf()
  S = { kind = "merge", repo = opts.repo, path = opts.path, buf = buf, win = win, sides = {} }
  local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local ours_label, theirs_label = "ours", "theirs"
  for _, l in ipairs(lines) do
    ours_label = l:find(MARK.start) and vim.trim(l:sub(8)) or ours_label
    if l:find(MARK.finish) then
      theirs_label = vim.trim(l:sub(8))
      break
    end
  end
  local ft = vim.bo[buf].filetype
  local ours = show(opts.repo, ":2:" .. opts.path)
  local theirs = show(opts.repo, ":3:" .. opts.path)
  local left = side(
    ours or {},
    "gitcompare://ours/" .. opts.path,
    ft,
    "left",
    ("%%#GitCompareTitle# « Left%%* %%#Comment#%s%s%%*"):format(ours_label, ours and "" or " (deleted)")
  )
  local right = side(
    theirs or {},
    "gitcompare://theirs/" .. opts.path,
    ft,
    "right",
    ("%%#GitCompareTitle# Right »%%* %%#Comment#%s%s%%*"):format(theirs_label, theirs and "" or " (deleted)")
  )
  S.sides = { left, right }
  local total = vim.api.nvim_win_get_width(win) + vim.api.nvim_win_get_width(left.win) + vim.api.nvim_win_get_width(right.win)
  vim.api.nvim_win_set_width(left.win, math.floor(total / 3))
  vim.api.nvim_win_set_width(right.win, math.floor(total / 3))
  for _, w in ipairs({ left.win, right.win }) do
    vim.api.nvim_win_call(w, function()
      vim.cmd("diffthis")
    end)
  end
  vim.wo[win].winbar = "%!v:lua.require'gitcompare'.merge_winbar()"
  M.attach(
    { "<leader>ml", function() M.accept("ours") end, "Merge: accept left" },
    { "<leader>mr", function() M.accept("theirs") end, "Merge: accept right" },
    { "<leader>mb", function() M.accept("both") end, "Merge: accept both" },
    { "<leader>mL", M.accept_all_ours, "Merge: accept left for all unresolved" },
    { "<leader>mR", M.accept_all_theirs, "Merge: accept right for all unresolved" },
    { "<leader>mm", M.resolve, "Merge: save & mark resolved" },
    { "]x", function() M.jump(1) end, "Next conflict" },
    { "[x", function() M.jump(-1) end, "Previous conflict" }
  )
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI" }, {
    group = S.group,
    buffer = buf,
    callback = function()
      vim.schedule(merge_update)
    end,
  })
  merge_update()
  if S.blocks[1] then
    vim.api.nvim_win_set_cursor(win, { S.blocks[1].start, 0 })
  end
end

-- Ortak --------------------------------------------------------------------------------------------------------

--- Gerçek dosya buffer'ına oturum boyunca geçerli eşlemeleri, tıklama işleyicisini ve kapanış autocmd'lerini bağlar.
---@param ... { [1]: string, [2]: function, [3]: string }
function M.attach(...)
  S.group = vim.api.nvim_create_augroup("gitcompare.session", { clear = true })
  S.maps = { "q", "<LeftRelease>" }
  vim.keymap.set("n", "q", M.close, { buffer = S.buf, nowait = true, desc = "Close compare view" })
  for _, m in ipairs({ ... }) do
    vim.keymap.set("n", m[1], m[2], { buffer = S.buf, desc = m[3] })
    S.maps[#S.maps + 1] = m[1]
  end
  vim.keymap.set("n", "<LeftRelease>", function()
    vim.schedule(M.on_click)
    return "<LeftRelease>"
  end, { buffer = S.buf, expr = true })
  for _, s in ipairs(S.sides) do
    vim.api.nvim_create_autocmd("WinClosed", {
      group = S.group,
      pattern = tostring(s.win),
      callback = function()
        vim.schedule(M.close)
      end,
    })
  end
  vim.api.nvim_create_autocmd("WinClosed", {
    group = S.group,
    pattern = tostring(S.win),
    callback = function()
      vim.schedule(M.close)
    end,
  })
  vim.api.nvim_create_autocmd("BufWinLeave", {
    group = S.group,
    buffer = S.buf,
    callback = function()
      vim.schedule(M.close)
    end,
  })
end

--- Gerçek dosya penceresinde fare bırakma: oluk (işaret sütunu) üstünde ⟲ → rollback; merge'de blok başı satırının
--- sonundaki butonlar → kabul.
function M.on_click()
  if not S then
    return
  end
  local pos = vim.fn.getmousepos()
  if pos.winid ~= S.win or pos.line == 0 then
    return
  end
  local info = vim.fn.getwininfo(S.win)[1]
  if S.kind == "diff" then
    diff_update()
    if pos.wincol <= info.textoff then
      local h = hunk_at(pos.line, false)
      if h then
        rollback(h)
      end
    end
    return
  end
  merge_update()
  for _, b in ipairs(S.blocks or {}) do
    if b.start == pos.line then
      local text = vim.api.nvim_buf_get_lines(S.buf, pos.line - 1, pos.line, false)[1] or ""
      local col = pos.wincol - info.textoff - 1 + vim.fn.winsaveview().leftcol - vim.fn.strdisplaywidth(text) - 1 - 2
      for _, btn in ipairs(MERGE_BUTTONS) do
        local w = vim.fn.strdisplaywidth(btn[2]) + 2
        if col >= 0 and col < w then
          accept(b, btn[1])
          merge_update()
          return
        end
        col = col - w - 1
      end
      return
    end
  end
end

--- Ekranı kapatır: yan pencereler kapanır, gerçek dosya diff modundan çıkar, eşlemeler ve işaretler silinir.
function M.close()
  local s = S
  if not s or s.closing then
    return
  end
  s.closing = true
  S = nil
  pcall(vim.api.nvim_del_augroup_by_id, s.group or -1)
  for _, side_win in ipairs(s.sides) do
    if vim.api.nvim_win_is_valid(side_win.win) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
      pcall(vim.api.nvim_win_close, side_win.win, true)
    end
  end
  if vim.api.nvim_buf_is_valid(s.buf) then
    vim.api.nvim_buf_clear_namespace(s.buf, ns, 0, -1)
    for _, lhs in ipairs(s.maps or {}) do
      pcall(vim.keymap.del, "n", lhs, { buffer = s.buf })
    end
  end
  if vim.api.nvim_win_is_valid(s.win) then
    vim.api.nvim_win_call(s.win, function()
      vim.cmd("diffoff")
    end)
    vim.wo[s.win].winbar = ""
  end
  local ok, problems = pcall(require, "problems")
  if ok and problems.refresh then
    problems.refresh()
  end
end

--- Açık ekranın türü (yoksa nil).
---@return "diff"|"merge"|nil
function M.active()
  return S and S.kind
end

---@diagnostic disable-next-line: duplicate-set-field
function _G.GitCompareClick(id)
  local action = ACTIONS[id]
  if action and M[action] then
    M[action]()
  end
end

return M
