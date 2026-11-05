local M = {}

local PREFIX = "gitdiff://"
local STAT = "--stat=200,180"
local group = vim.api.nvim_create_augroup("gitdiff", { clear = true })

--- `git` komutunu verilen klasörde çalıştırır; yollar tırnaksız (Türkçe karakterli dosya adları okunur kalsın).
---@param args string[]
---@param cwd string
---@return vim.SystemCompleted
local function git(args, cwd)
  return vim.system(vim.list_extend({ "git", "-c", "core.quotepath=false" }, args), { cwd = cwd, text = true }):wait()
end

--- Diff'in çalışacağı repo kökü: `opts.cwd`, yoksa mutlak `opts.path`'in klasörü, o da yoksa Neovim cwd'si.
--- Silinmiş dosyanın klasörü olmayabilir; var olan ilk üst klasörden aranır.
---@param opts table
---@return string?
local function repo_root(opts)
  local dir = opts.cwd or (opts.path and opts.path:sub(1, 1) == "/" and vim.fs.dirname(opts.path)) or vim.fn.getcwd()
  while vim.fn.isdirectory(dir) == 0 and dir ~= "/" do
    dir = vim.fs.dirname(dir)
  end
  local res = git({ "rev-parse", "--show-toplevel" }, dir)
  return res.code == 0 and vim.trim(res.stdout) or nil
end

--- `opts`'a göre unified diff metnini üretir. Çalışma ağacı modunda git'in bilmediği (untracked) dosya boş
--- yerine `/dev/null`'a karşı tamamen eklenmiş olarak gösterilir.
---@param opts table
---@param root string
---@return string? text, string? err
local function diff_text(opts, root)
  local paths = opts.path and { "--", opts.path } or {}
  local args
  if opts.rev then
    args = { "show", "--diff-merges=first-parent", "--date=format:%Y-%m-%d %H:%M", STAT, "-p", opts.rev }
  elseif opts.staged then
    args = { "diff", "--cached", STAT, "-p" }
  else
    args = { "diff", STAT, "-p" }
  end
  local res = git(vim.list_extend(args, paths), root)
  if res.code ~= 0 then
    return nil, vim.trim(res.stderr)
  end
  if res.stdout == "" and opts.path and not opts.rev and not opts.staged then
    local untracked = git({ "ls-files", "--others", "--exclude-standard", "--", opts.path }, root).stdout
    if untracked ~= "" then
      res = git({ "diff", "--no-index", STAT, "-p", "--", "/dev/null", opts.path }, root)
    end
  end
  return res.stdout
end

--- Tab adı (bufferline'da görünen kısım `Foo.java (abc1234)`); aynı diff ikinci kez açılınca aynı buffer bulunur.
---@param opts table
---@param root string
---@return string
local function buf_name(opts, root)
  local label = opts.rev and vim.trim(git({ "rev-parse", "--short", opts.rev }, root).stdout)
    or (opts.staged and "staged" or "working tree")
  local rel = opts.path and (vim.fs.relpath(root, opts.path) or opts.path) or (opts.rev and "commit" or "changes")
  return ("%s%s/%s (%s)"):format(PREFIX, root, rel, label)
end

--- Unified diff satırının karşılık geldiği dosya ve satır: `@@ -a,b +c,d @@` başlığından yeni taraftaki satır
--- sayılır, dosya yolu yukarıdaki `+++ b/<yol>` satırından alınır. Hunk dışındaysa nil.
---@param lines string[]
---@param row integer
---@return string? path, integer? lnum
local function source_at(lines, row)
  local lnum
  for i = row, 1, -1 do
    local l = lines[i]
    local start = l:match("^@@ %-%S+ %+(%d+)")
    if start then
      lnum = tonumber(start) + (lnum or 0)
      for j = i - 1, 1, -1 do
        local path = lines[j]:match("^%+%+%+ b/(.*)$")
        if path then
          return path, math.max(lnum, 1)
        end
      end
      return nil
    elseif l:match("^diff ") then
      return nil
    elseif i < row and not l:match("^[-\\]") then
      lnum = (lnum or 0) + 1
    end
  end
end

--- Enter / çift tık: `--stat` satırında o dosyanın diff bölümüne iner; hunk içinde dosyayı (çalışma ağacındaki
--- haliyle) o satırda aynı pencerede açar. Diff tab'ı bufferline'da kalır.
---@param buf integer
local function jump(buf)
  local row = vim.api.nvim_win_get_cursor(0)[1]
  local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local first_diff = vim.iter(ipairs(lines)):find(function(_, l)
    return l:match("^diff ")
  end)
  local stat = row < (first_diff or 0) and lines[row]:match("^ (.-)%s+|")
  if stat then
    for i = row + 1, #lines do
      if lines[i] == "+++ b/" .. stat or lines[i] == "--- a/" .. stat then
        vim.api.nvim_win_set_cursor(0, { i, 0 })
        return
      end
    end
    return
  end
  local path, lnum = source_at(lines, row)
  local file = path and vim.b[buf].gitdiff_root .. "/" .. path
  if not (file and vim.uv.fs_stat(file)) then
    return
  end
  vim.cmd.edit(vim.fn.fnameescape(file))
  vim.api.nvim_win_set_cursor(0, { math.min(lnum, vim.api.nvim_buf_line_count(0)), 0 })
end

--- Diff buffer'ını oluşturur. `buftype=""` bilerek: workbench onu kod penceresi sayar (Cmd+W, Cmd+P, tab gezme
--- çalışır). Diske yazılmaması `BufWriteCmd`, `:e` ile yenilenmesi `BufReadCmd` ile sağlanır.
---@param name string
---@param render fun(buf: integer)
---@return integer
local function create_buf(name, render)
  local buf = vim.api.nvim_create_buf(true, false)
  vim.api.nvim_buf_set_name(buf, name)
  vim.bo[buf].swapfile = false
  vim.bo[buf].filetype = "diff"
  vim.api.nvim_create_autocmd("BufWriteCmd", {
    group = group,
    buffer = buf,
    callback = function()
      vim.notify("Diff tabs are read-only", vim.log.levels.WARN)
    end,
  })
  vim.api.nvim_create_autocmd("BufReadCmd", {
    group = group,
    buffer = buf,
    callback = function()
      render(buf)
    end,
  })
  for _, lhs in ipairs({ "<CR>", "<2-LeftMouse>" }) do
    vim.keymap.set("n", lhs, function()
      jump(buf)
    end, { buffer = buf, nowait = true, desc = "Diff: go to file / section" })
  end
  vim.keymap.set("n", "q", function()
    Snacks.bufdelete(buf)
  end, { buffer = buf, nowait = true, desc = "Close diff tab" })
  return buf
end

--- Bir git diff'ini kod editöründe tab olarak açar (bufferline buffer'ı, kod penceresinde; yeni tabpage ya da
--- pencere açılmaz, paneller yerinde kalır). Aynı diff zaten açıksa içeriği yenilenip o tab gösterilir.
--- Tab'da: Enter/çift tık `--stat` satırında dosyanın bölümüne, hunk'ta dosyanın o satırına gider; `q`/Cmd+W kapatır.
---
--- opts:
--- - `rev`: commit (sha/ref). Verilirse o commit'in ebeveynine göre değişikliği (merge'de ilk ebeveyn):
---   `path` varsa yalnız o dosya, yoksa tüm commit (başlık + dosya listesi + tüm dosyalar).
--- - `staged = true` (rev yokken): index ↔ HEAD (`git diff --cached`).
--- - `staged` falsy (rev yokken): çalışma ağacı ↔ index (`git diff`); untracked dosya tamamen eklenmiş görünür.
--- - `path`: mutlak ya da repo köküne göre göreli dosya yolu. rev'siz modlarda verilmezse tüm repo.
--- - `cwd`: repo içindeki bir klasör; yoksa mutlak `path`'in klasörü, o da yoksa Neovim cwd'si.
---
--- Değişiklik yoksa ya da git hata verirse bildirim gösterir ve nil döner.
---@param opts? { path?: string, rev?: string, staged?: boolean, cwd?: string }
---@return integer? buf
function M.open(opts)
  opts = opts or {}
  local root = repo_root(opts)
  if not root then
    vim.notify("Not a git repository", vim.log.levels.WARN)
    return nil
  end
  local text, err = diff_text(opts, root)
  if not text or text == "" then
    vim.notify(err or "No changes to show", err and vim.log.levels.ERROR or vim.log.levels.INFO)
    return nil
  end
  local function render(buf, s)
    s = s or diff_text(opts, root) or ""
    vim.bo[buf].modifiable = true
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(s, "\n", { trimempty = true }))
    vim.bo[buf].modifiable = false
    vim.bo[buf].modified = false
  end
  local name = buf_name(opts, root)
  local buf = vim.iter(vim.api.nvim_list_bufs()):find(function(b)
    return vim.api.nvim_buf_get_name(b) == name
  end)
  buf = buf or create_buf(name, render)
  vim.b[buf].gitdiff_root = root
  render(buf, text)
  vim.bo[buf].buflisted = true

  local wb = require("workbench")
  local win = wb.code_win()
  if not win then
    wb.code({ pinned = false })
    win = vim.api.nvim_get_current_win()
  end
  vim.api.nvim_win_set_buf(win, buf)
  vim.api.nvim_set_current_win(win)
  vim.cmd.stopinsert()
  return buf
end

-- Diff tab'ları oturuma girmesin: geri yüklenince içeriksiz `gitdiff://` dosyası olarak açılırdı.
vim.api.nvim_create_autocmd("User", {
  group = group,
  pattern = "PersistenceSavePre",
  callback = function()
    for _, b in ipairs(vim.api.nvim_list_bufs()) do
      if vim.startswith(vim.api.nvim_buf_get_name(b), PREFIX) then
        pcall(vim.api.nvim_buf_delete, b, { force = true })
      end
    end
  end,
})

return M
