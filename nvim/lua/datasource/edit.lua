--- Veri tab'ında satır düzenleme (yalnız SQL türleri): hücre değiştirme, NULL yapma, satır kopyalama (yeni satır).
--- Değişiklikler önce bekler (vurgulu hücreler, buffer `modified`); Cmd+S / `:w` üretilen SQL'i onay penceresinde
--- gösterir, onaylanınca tek transaction'da çalıştırır ve tabloyu yeniden yükler. UPDATE birincil anahtarla yapılır;
--- anahtarı olmayan tabloda yalnız yeni satır eklenebilir.
local M = {}

local grid = require("datasource.grid")
local types = require("datasource.types")

---@class DatasourceEditState
---@field conn DatasourceConn
---@field t DatasourceType
---@field item string tablo adı ("şema.tablo")
---@field reload fun() tabloyu veritabanından yeniden yükler
---@field base? DatasourceGrid veritabanından gelen satırlar
---@field info string
---@field edits table<integer, table<integer, string|table>> satır → kolon → yeni değer (`grid.NULL` dahil)
---@field inserts (string|table)[][] kopyalanan / yeni satırlar
---@field committing? boolean

---@type table<integer, DatasourceEditState>
local states = {}

---@param st DatasourceEditState
---@return integer
local function pending(st)
  local n = #st.inserts
  for _, cols in pairs(st.edits) do
    n = n + vim.tbl_count(cols)
  end
  return n
end

---@param v string|table
---@return string?
local function raw(v)
  return v ~= grid.NULL and v or nil
end

--- Bekleyen değişiklikler uygulanmış tabloyu çizer; imleç aynı hücrede kalır.
---@param buf integer
local function redraw(buf)
  local st = states[buf]
  if not (st and st.base) then
    return
  end
  local rows = {}
  for r, row in ipairs(st.base.rows) do
    local copy = vim.list_slice(row)
    for c, v in pairs(st.edits[r] or {}) do
      copy[c] = v
    end
    rows[r] = copy
  end
  vim.list_extend(rows, st.inserts)
  local n = pending(st)
  local info = st.info
    .. (n > 0 and (" · %d pending · Cmd+S commit · u revert"):format(n) or " · e edit · Cmd+D clone row")
  local nbase = #st.base.rows
  local wins = vim.fn.win_findbuf(buf)
  local cursor = {}
  for _, w in ipairs(wins) do
    vim.api.nvim_win_call(w, function()
      local r, c = grid.cell(buf)
      cursor[w] = r and { r, c }
    end)
  end
  grid.render(buf, { columns = st.base.columns, rows = rows }, info, function(r, c)
    if r > nbase then
      return "DatasourceGridAdded"
    elseif st.edits[r] and st.edits[r][c] ~= nil then
      return "DatasourceGridChanged"
    end
  end)
  vim.bo[buf].modified = n > 0
  for w, pos in pairs(cursor) do
    vim.api.nvim_win_call(w, function()
      grid.goto_cell(buf, pos[1], pos[2])
    end)
  end
end

--- Hücreye yeni değer koyar (orijinal değere dönülürse bekleyen değişiklik silinir).
---@param buf integer
---@param r integer
---@param c integer
---@param v string|table
local function set_cell(buf, r, c, v)
  local st = states[buf]
  local nbase = #st.base.rows
  if r > nbase then
    st.inserts[r - nbase][c] = v
  elseif st.base.rows[r][c] == v then
    if st.edits[r] then
      st.edits[r][c] = nil
      if next(st.edits[r]) == nil then
        st.edits[r] = nil
      end
    end
  else
    st.edits[r] = st.edits[r] or {}
    st.edits[r][c] = v
  end
  redraw(buf)
end

--- Çok satırlı değer için yüzen düzenleyici: Cmd+Enter / Ctrl+S kaydeder, normal modda `q` / Esc iptal.
---@param title string
---@param value string
---@param cb fun(v: string)
local function edit_multiline(title, value, cb)
  local ebuf = vim.api.nvim_create_buf(false, true)
  vim.bo[ebuf].bufhidden = "wipe"
  vim.api.nvim_buf_set_lines(ebuf, 0, -1, false, vim.split(value, "\n"))
  local width = math.floor(vim.o.columns * 0.6)
  local height = math.min(math.max(5, vim.api.nvim_buf_line_count(ebuf) + 1), math.floor(vim.o.lines * 0.6))
  local win = vim.api.nvim_open_win(ebuf, true, {
    relative = "editor",
    width = width,
    height = height,
    row = math.floor((vim.o.lines - height) / 2) - 1,
    col = math.floor((vim.o.columns - width) / 2),
    border = "rounded",
    title = " " .. title .. " ",
    title_pos = "center",
    footer = " Cmd+Enter / Ctrl+S apply · q cancel ",
    footer_pos = "center",
  })
  local function apply()
    local text = table.concat(vim.api.nvim_buf_get_lines(ebuf, 0, -1, false), "\n")
    vim.cmd.stopinsert()
    pcall(vim.api.nvim_win_close, win, true)
    cb(text)
  end
  for _, lhs in ipairs({ "<D-CR>", "<C-s>" }) do
    vim.keymap.set({ "n", "i" }, lhs, apply, { buffer = ebuf })
  end
  for _, lhs in ipairs({ "q", "<Esc>" }) do
    vim.keymap.set("n", lhs, function()
      pcall(vim.api.nvim_win_close, win, true)
    end, { buffer = ebuf, nowait = true })
  end
end

--- İmleçteki hücreyi düzenler: tek satırlık değer `vim.ui.input`, çok satırlı değer yüzen düzenleyici.
---@param buf integer
function M.edit_cell(buf)
  local st = states[buf]
  local r, c, column, value = grid.cell(buf)
  if not (st and st.base and r) then
    return
  end
  local current = value == grid.NULL and "" or value
  local function done(v)
    if v ~= nil and vim.api.nvim_buf_is_valid(buf) then
      set_cell(buf, r, c, v)
    end
  end
  if current:find("\n") then
    return edit_multiline(column, current, done)
  end
  vim.ui.input({ prompt = column .. (value == grid.NULL and " (NULL)" or ""), default = current }, done)
end

---@param buf integer
function M.set_null(buf)
  local st = states[buf]
  local r, c = grid.cell(buf)
  if st and st.base and r then
    set_cell(buf, r, c, grid.NULL)
  end
end

--- İmleçteki satırın kopyasını yeni satır olarak ekler (birincil anahtar kolonları düzenlenmezse INSERT'e girmez;
--- veritabanı üretir).
---@param buf integer
function M.clone_row(buf)
  local st = states[buf]
  local r = grid.cell(buf)
  if not (st and st.base and r) then
    return
  end
  local nbase = #st.base.rows
  local src = r > nbase and st.inserts[r - nbase] or vim.list_slice(st.base.rows[r])
  if r <= nbase then
    for c, v in pairs(st.edits[r] or {}) do
      src[c] = v
    end
  end
  local copy = vim.list_slice(src)
  copy.cloned = vim.deepcopy(copy)
  table.insert(st.inserts, copy)
  redraw(buf)
  grid.goto_cell(buf, nbase + #st.inserts, select(2, grid.cell(buf)) or 1)
end

--- İmleçteki hücrenin değişikliğini geri alır; eklenmiş satırda satırı siler, değişmemiş hücrede satırın tüm
--- değişikliklerini geri alır.
---@param buf integer
function M.revert(buf)
  local st = states[buf]
  local r, c = grid.cell(buf)
  if not (st and st.base and r) then
    return
  end
  local nbase = #st.base.rows
  if r > nbase then
    table.remove(st.inserts, r - nbase)
  elseif st.edits[r] and st.edits[r][c] ~= nil then
    st.edits[r][c] = nil
    if next(st.edits[r]) == nil then
      st.edits[r] = nil
    end
  else
    st.edits[r] = nil
  end
  redraw(buf)
end

---@param buf integer
function M.revert_all(buf)
  local st = states[buf]
  if st and pending(st) > 0 then
    st.edits, st.inserts = {}, {}
    redraw(buf)
    vim.notify("Discarded pending changes")
  end
end

--- Bekleyen değişikliklerin SQL ifadeleri.
---@param st DatasourceEditState
---@param pk string[] birincil anahtar kolonları
---@return string[]? statements, string? err
local function statements(st, pk)
  local id, cols = st.t.id, st.base.columns
  local tbl = types.quote(id, st.item)
  local index = {}
  for c, name in ipairs(cols) do
    index[name] = c
  end
  local out = {}
  local rows = vim.tbl_keys(st.edits)
  table.sort(rows)
  if #rows > 0 then
    if #pk == 0 then
      return nil, ("%s has no primary key, so rows can't be updated safely (new rows can still be added)"):format(st.item)
    end
    for _, k in ipairs(pk) do
      if not index[k] then
        return nil, ("Primary key column %s is not in the result"):format(k)
      end
    end
  end
  for _, r in ipairs(rows) do
    local sets, where = {}, {}
    local changed = vim.tbl_keys(st.edits[r])
    table.sort(changed)
    for _, c in ipairs(changed) do
      table.insert(sets, ("%s = %s"):format(types.quote_ident(id, cols[c]), types.value(id, raw(st.edits[r][c]))))
    end
    for _, k in ipairs(pk) do
      local v = raw(st.base.rows[r][index[k]])
      table.insert(where, types.quote_ident(id, k) .. (v == nil and " IS NULL" or (" = " .. types.value(id, v))))
    end
    table.insert(out, ("UPDATE %s SET %s WHERE %s;"):format(tbl, table.concat(sets, ", "), table.concat(where, " AND ")))
  end
  local is_pk = {}
  for _, k in ipairs(pk) do
    is_pk[k] = true
  end
  for _, row in ipairs(st.inserts) do
    local names, values = {}, {}
    for c, name in ipairs(cols) do
      local keep = not is_pk[name] or (row.cloned and row[c] ~= row.cloned[c])
      if keep then
        table.insert(names, types.quote_ident(id, name))
        table.insert(values, types.value(id, raw(row[c])))
      end
    end
    table.insert(out, ("INSERT INTO %s (%s) VALUES (%s);"):format(tbl, table.concat(names, ", "), table.concat(values, ", ")))
  end
  return out
end

--- Bekleyen değişiklikleri onaya sunar; onaylanınca tek transaction'da çalıştırır ve tabloyu yeniden yükler.
---@param buf integer
function M.commit(buf)
  local st = states[buf]
  if not st or pending(st) == 0 then
    vim.notify("No pending changes", vim.log.levels.INFO)
    return
  end
  if st.committing then
    return
  end
  local ds = require("datasource")
  local url = ds.url(st.conn)
  local function with_pk(cb)
    if next(st.edits) == nil then
      return cb({})
    end
    ds.run(types.pk_cmd(st.t.id, url, st.item), function(ok, lines, err)
      if not ok then
        return vim.notify("Could not read the primary key: " .. err, vim.log.levels.ERROR)
      end
      cb(vim.tbl_filter(function(l)
        return vim.trim(l) ~= ""
      end, vim.tbl_map(vim.trim, lines)))
    end)
  end
  with_pk(function(pk)
    local list, err = statements(st, pk)
    if not list then
      return vim.notify(err, vim.log.levels.ERROR)
    end
    local title = ("Commit %d change%s to %s › %s"):format(#list, #list == 1 and "" or "s", st.conn.name, st.item)
    require("lessmcp").approve(title, list, function(yes)
      if not yes then
        return
      end
      st.committing = true
      local cmd, stdin = types.query_cmd(st.t.id, url, types.transaction(st.t.id, list))
      ds.run(cmd, function(ok, _, run_err)
        st.committing = false
        if not ok then
          return vim.notify("Commit failed, nothing was changed:\n" .. run_err, vim.log.levels.ERROR)
        end
        st.edits, st.inserts = {}, {}
        vim.notify(("Committed %d change%s to %s"):format(#list, #list == 1 and "" or "s", st.item))
        if vim.api.nvim_buf_is_valid(buf) then
          vim.bo[buf].modified = false
          st.reload()
        end
      end, { stdin = stdin, timeout = 120000 })
    end, "sql")
  end)
end

--- Bekleyen değişiklik varsa atılmasını onaylatır, sonra `fn`'i çalıştırır (yenileme gibi).
---@param buf integer
---@param fn fun()
function M.confirm_discard(buf, fn)
  local st = states[buf]
  if not st or pending(st) == 0 then
    return fn()
  end
  vim.ui.select({ "Cancel", "Discard changes" }, {
    prompt = ("%d pending change(s) will be lost"):format(pending(st)),
  }, function(choice)
    if choice == "Discard changes" then
      st.edits, st.inserts = {}, {}
      vim.bo[buf].modified = false
      fn()
    end
  end)
end

---@param buf integer
---@return boolean
function M.active(buf)
  return states[buf] ~= nil
end

--- Veritabanından gelen tabloyu düzenlenebilir olarak çizer (bekleyen değişiklikler yeniden yüklemede atılır).
---@param buf integer
---@param data DatasourceGrid
---@param info string
function M.load(buf, data, info)
  local st = states[buf]
  st.base, st.info, st.edits, st.inserts = data, info, {}, {}
  redraw(buf)
end

--- Veri tab'ını düzenlemeye açar ve tuşları bağlar.
---@param buf integer
---@param conn DatasourceConn
---@param t DatasourceType
---@param item string
---@param reload fun()
function M.attach(buf, conn, t, item, reload)
  if states[buf] then
    states[buf].reload = reload
    return
  end
  states[buf] = { conn = conn, t = t, item = item, reload = reload, info = "", edits = {}, inserts = {} }
  local function map(lhs, fn, desc)
    vim.keymap.set("n", lhs, function()
      fn(buf)
    end, { buffer = buf, nowait = true, desc = desc })
  end
  map("e", M.edit_cell, "Edit cell")
  map("i", M.edit_cell, "Edit cell")
  map("<Del>", M.set_null, "Set cell to NULL")
  map("<D-d>", M.clone_row, "Clone row")
  map("c", M.clone_row, "Clone row")
  map("u", M.revert, "Revert change")
  map("U", M.revert_all, "Revert all changes")
  vim.api.nvim_create_autocmd("BufWipeout", { buffer = buf, once = true, callback = function() states[buf] = nil end })
end

return M
