--- Notlar: Cmd+N ile ekranın ortasında açılan yüzen pencere, tab'lı (her tab bir Markdown dosyası).
--- Dosyalar projede `.less/notes/*.md` (klasörde `.gitignore` = `*`, git'e girmez); kendiliğinden kaydedilir
--- (`noautocmd update`, conform biçimlendirmesin). Tab sırası dosya oluşturulma zamanı. Pencere dışına geçilince
--- (yüzen olmayan bir pencereye) kapanır; vim.ui.input gibi başka yüzen pencereler kapatmaz.
local M = {}

local state = {
  win = nil, ---@type integer?
  current = {}, ---@type table<string, string> proje klasörü → son açık not dosyası
}

--- Proje not klasörü (yoksa oluşturur, `.gitignore` yazar).
---@return string
function M.dir()
  local dir = vim.fn.getcwd() .. "/.less/notes"
  if vim.fn.isdirectory(dir) == 0 then
    vim.fn.mkdir(dir, "p")
  end
  if vim.fn.filereadable(dir .. "/.gitignore") == 0 then
    vim.fn.writefile({ "*" }, dir .. "/.gitignore")
  end
  return dir
end

--- Notlar oluşturulma sırasıyla: `{ path, name }`.
---@return { path: string, name: string }[]
function M.list()
  local dir = M.dir()
  local items = {}
  for _, path in ipairs(vim.fn.glob(dir .. "/*.md", false, true)) do
    local st = vim.uv.fs_stat(path)
    local born = st and (st.birthtime.sec > 0 and st.birthtime or st.mtime)
    table.insert(items, { path = path, name = vim.fn.fnamemodify(path, ":t:r"), t = born and (born.sec * 1e9 + born.nsec) or 0 })
  end
  table.sort(items, function(a, b)
    return a.t == b.t and a.name < b.name or a.t < b.t
  end)
  return items
end

--- Ad → dosya yolu (dosya adına uygun olmayan karakterler `-`).
---@param name string
---@return string
local function path_for(name)
  local safe = vim.trim(name):gsub("[/\\:%c]", "-")
  return M.dir() .. "/" .. safe .. ".md"
end

--- Not buffer'ı: kendiliğinden kaydetme ve tab/kapatma tuşları bir kez kurulur.
---@param buf integer
local function setup_buf(buf)
  if vim.b[buf].less_note then
    return
  end
  vim.b[buf].less_note = true
  vim.bo[buf].bufhidden = "hide"
  vim.bo[buf].buflisted = false
  local function save()
    if vim.api.nvim_buf_is_valid(buf) and vim.bo[buf].modified then
      vim.api.nvim_buf_call(buf, function()
        vim.cmd("silent! noautocmd update")
      end)
    end
  end
  local timer = vim.uv.new_timer()
  vim.api.nvim_create_autocmd({ "TextChanged", "InsertLeave", "BufLeave", "BufHidden", "FocusLost", "QuitPre" }, {
    buffer = buf,
    callback = save,
  })
  vim.api.nvim_create_autocmd("TextChangedI", {
    buffer = buf,
    callback = function()
      timer:stop()
      timer:start(300, 0, vim.schedule_wrap(save))
    end,
  })
  vim.api.nvim_create_autocmd("BufWipeout", { buffer = buf, once = true, callback = function() timer:close() end })
  local function map(modes, lhs, fn, desc)
    vim.keymap.set(modes, lhs, fn, { buffer = buf, nowait = true, desc = desc })
  end
  local all = { "n", "i", "v" }
  map(all, "<D-n>", M.close, "Close notes")
  map(all, "<D-s>", save, "Save note")
  map(all, "<D-t>", M.new, "New note")
  map(all, "<D-w>", M.close, "Close notes")
  for _, lhs in ipairs({ "<S-D-Right>", "<M-D-Right>" }) do
    map(all, lhs, function() M.cycle(1) end, "Next note")
  end
  for _, lhs in ipairs({ "<S-D-Left>", "<M-D-Left>" }) do
    map(all, lhs, function() M.cycle(-1) end, "Previous note")
  end
  map("n", "<Esc>", M.close, "Close notes")
  map("n", "q", M.close, "Close notes")
end

--- Ekranın ortasındaki pencere boyutu ve konumu.
local function geometry()
  local width = math.min(120, math.max(50, math.floor(vim.o.columns * 0.6)))
  local height = math.min(40, math.max(12, math.floor(vim.o.lines * 0.6)))
  return {
    relative = "editor",
    width = width,
    height = height,
    row = math.floor((vim.o.lines - height) / 2) - 1,
    col = math.floor((vim.o.columns - width) / 2),
  }
end

---@return boolean
function M.is_open()
  return state.win ~= nil and vim.api.nvim_win_is_valid(state.win)
end

--- Notu açık pencerede gösterir (pencere yoksa açar).
---@param path string
function M.show(path)
  local buf = vim.fn.bufadd(path)
  vim.fn.bufload(buf)
  setup_buf(buf)
  vim.bo[buf].filetype = "markdown"
  state.current[vim.fn.getcwd()] = path
  if M.is_open() then
    vim.api.nvim_win_set_buf(state.win, buf)
  else
    local cfg = vim.tbl_extend("force", geometry(), { border = "rounded", title = " Notes ", title_pos = "center",
      footer = " Cmd+T new · Cmd+Shift+←/→ switch · Cmd+N / Esc close ", footer_pos = "center", zindex = 60 })
    state.win = vim.api.nvim_open_win(buf, true, cfg)
    vim.wo[state.win].winbar = "%!v:lua.require'notes'.winbar()"
    vim.wo[state.win].wrap = true
    vim.wo[state.win].linebreak = true
    vim.wo[state.win].number = false
    vim.wo[state.win].relativenumber = false
    vim.wo[state.win].signcolumn = "no"
    vim.wo[state.win].winfixbuf = false
  end
  vim.cmd("redrawstatus!")
end

--- Yeni not: ad sorulur (varsayılan "Note N"), boş dosya oluşturulup açılır.
function M.new()
  local n = #M.list() + 1
  vim.ui.input({ prompt = "Note name", default = "Note " .. n }, function(name)
    if not name or vim.trim(name) == "" then
      return
    end
    local path = path_for(name)
    if vim.fn.filereadable(path) == 0 then
      vim.fn.writefile({}, path)
    end
    M.show(path)
  end)
end

--- Notlar penceresini açar: projenin son açık notu, yoksa ilk not, hiç not yoksa "Notes" oluşturulur.
function M.open()
  if M.is_open() then
    vim.api.nvim_set_current_win(state.win)
    return
  end
  local items = M.list()
  local last = state.current[vim.fn.getcwd()]
  if not (last and vim.fn.filereadable(last) == 1) then
    last = items[1] and items[1].path
  end
  if not last then
    last = path_for("Notes")
    vim.fn.writefile({}, last)
  end
  M.show(last)
end

function M.close()
  if M.is_open() then
    vim.cmd("stopinsert")
    pcall(vim.api.nvim_win_close, state.win, false)
  end
  state.win = nil
end

--- Cmd+N: açıksa kapatır, kapalıysa açar.
function M.toggle()
  if M.is_open() then
    M.close()
  else
    M.open()
  end
end

---@return string?
local function current_path()
  return M.is_open() and vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(state.win)) or nil
end

--- Sonraki / önceki not tab'ı (uçlarda başa / sona döner).
---@param step integer
function M.cycle(step)
  local items, cur = M.list(), current_path()
  for i, item in ipairs(items) do
    if item.path == cur then
      M.show(items[(i - 1 + step) % #items + 1].path)
      return
    end
  end
end

--- Açık notu yeniden adlandırır (dosya da taşınır).
function M.rename()
  local cur = current_path()
  if not cur then
    return
  end
  vim.ui.input({ prompt = "Rename note", default = vim.fn.fnamemodify(cur, ":t:r") }, function(name)
    if not name or vim.trim(name) == "" then
      return
    end
    local target = path_for(name)
    if target == cur then
      return
    elseif vim.fn.filereadable(target) == 1 then
      vim.notify("A note with that name already exists", vim.log.levels.WARN)
      return
    end
    local buf = vim.fn.bufnr(cur)
    vim.api.nvim_buf_call(buf, function()
      vim.cmd("silent! noautocmd update")
    end)
    vim.fn.rename(cur, target)
    M.show(target)
    pcall(vim.api.nvim_buf_delete, buf, { force = true })
  end)
end

--- Notu siler (onaylı; ilk seçenek Cancel).
---@param path? string
function M.delete(path)
  path = path or current_path()
  if not path then
    return
  end
  local name = vim.fn.fnamemodify(path, ":t:r")
  vim.ui.select({ "Cancel", "Delete" }, { prompt = ("Delete note '%s'?"):format(name) }, function(choice)
    if choice ~= "Delete" then
      return
    end
    local items = M.list()
    local next
    for i, item in ipairs(items) do
      if item.path == path then
        next = items[i + 1] or items[i - 1]
      end
    end
    local buf = vim.fn.bufnr(path)
    vim.fn.delete(path)
    if next then
      M.show(next.path)
    else
      M.close()
    end
    if buf ~= -1 then
      pcall(vim.api.nvim_buf_delete, buf, { force = true })
    end
  end)
end

--- Tab çubuğu (winbar): tıklanınca o not, ✕ siler, + yeni not.
---@return string
function M.winbar()
  local cur = current_path()
  local parts = {}
  for i, item in ipairs(M.list()) do
    local hl = item.path == cur and "%#TabLineSel#" or "%#TabLine#"
    local name = item.name:gsub("%%", "%%%%")
    table.insert(parts, ("%s%%%d@v:lua.NotesClick@ %s %%X%%%d@v:lua.NotesClose@✕ %%X"):format(hl, i, name, i))
  end
  return table.concat(parts, "%#TabLineFill# ") .. "%#TabLineFill# %@v:lua.NotesNew@ + %X%=%#Comment#.less/notes "
end

--- Tab tıklaması: sol tık o nota geçer, çift tık / sağ tık yeniden adlandırma / silme menüsü.
function _G.NotesClick(i, clicks, button)
  local item = M.list()[i]
  if not item then
    return
  end
  M.show(item.path)
  if button == "r" or clicks > 1 then
    vim.schedule(function()
      vim.ui.select({ "Rename", "Delete" }, { prompt = item.name }, function(choice)
        if choice == "Rename" then
          M.rename()
        elseif choice == "Delete" then
          M.delete(item.path)
        end
      end)
    end)
  end
end

function _G.NotesClose(i)
  local item = M.list()[i]
  if item then
    M.delete(item.path)
  end
end

function _G.NotesNew()
  M.new()
end

--- `:Notes [new|rename|delete]` komutu ve yüzen olmayan bir pencereye geçilince kapanma.
function M.setup()
  vim.api.nvim_create_user_command("Notes", function(opts)
    local sub = opts.fargs[1]
    if sub == "new" then
      M.open()
      M.new()
    elseif sub == "rename" then
      M.rename()
    elseif sub == "delete" then
      M.delete()
    else
      M.toggle()
    end
  end, {
    nargs = "?",
    complete = function()
      return { "new", "rename", "delete" }
    end,
    desc = "Notes (centered window, .less/notes)",
  })
  vim.api.nvim_create_autocmd("WinEnter", {
    group = vim.api.nvim_create_augroup("less_notes", { clear = true }),
    callback = function()
      local win = vim.api.nvim_get_current_win()
      if M.is_open() and win ~= state.win and vim.api.nvim_win_get_config(win).relative == "" then
        M.close()
      end
    end,
  })
  vim.api.nvim_create_autocmd("VimResized", {
    group = "less_notes",
    callback = function()
      if M.is_open() then
        vim.api.nvim_win_set_config(state.win, geometry())
      end
    end,
  })
end

return M
