--- Veri tab'ı için tablo görünümü: istemcilerin ham çıktısını (CSV, TSV, sqlcmd, JSON, Redis) satır/kolona ayrıştırır,
--- çerçeveli tablo olarak çizer. Başlık yukarı kayınca winbar'da sabit kalır (yatay kaydırmayla birlikte kayar).
--- Hücrede Enter/çift tık tam değeri gösterir, `y` panoya kopyalar.
local M = {}

local ns = vim.api.nvim_create_namespace("datasource_grid")
local MAX_WIDTH = 40
local HEADER_LINES = 3

--- NULL hücre işareti (boş dizeden ayırmak için).
M.NULL = setmetatable({}, { __tostring = function() return "NULL" end })

---@class DatasourceGrid
---@field columns string[]
---@field rows (string|table)[][]

---@type table<integer, { header: string, info: string, grid: DatasourceGrid, first: integer }>
local views = {}

--- RFC 4180 CSV; tırnaksız boş alan NULL sayılır (psql `--csv` ve sqlite `-csv` NULL'u böyle yazar).
---@param text string
---@return string[][]
function M.parse_csv(text)
  local rows, row, field = {}, {}, {}
  local i, n = 1, #text
  local quoted, in_quotes = false, false
  local function end_field()
    local s = table.concat(field)
    table.insert(row, (s == "" and not quoted) and M.NULL or s)
    field, quoted = {}, false
  end
  while i <= n do
    local c = text:sub(i, i)
    if in_quotes then
      if c == '"' and text:sub(i + 1, i + 1) == '"' then
        table.insert(field, '"')
        i = i + 1
      elseif c == '"' then
        in_quotes = false
      else
        table.insert(field, c)
      end
    elseif c == '"' then
      in_quotes, quoted = true, true
    elseif c == "," then
      end_field()
    elseif c == "\n" or c == "\r" then
      if c == "\r" and text:sub(i + 1, i + 1) == "\n" then
        i = i + 1
      end
      end_field()
      table.insert(rows, row)
      row = {}
    else
      table.insert(field, c)
    end
    i = i + 1
  end
  if #field > 0 or #row > 0 or quoted then
    end_field()
    table.insert(rows, row)
  end
  return rows
end

--- Ayraçla bölünmüş satırlar; `NULL` yazan hücre NULL. mysql `-B` kaçışları (`\n`, `\t`, `\\`) çözülür.
---@param lines string[]
---@param sep string
---@param mysql_escapes? boolean
---@return string[][]
local function parse_separated(lines, sep, mysql_escapes)
  local esc = { n = "\n", t = "\t", ["\\"] = "\\", ["0"] = "\0" }
  return vim.tbl_map(function(line)
    return vim.tbl_map(function(cell)
      if cell == "NULL" then
        return M.NULL
      end
      return mysql_escapes and (cell:gsub("\\(.)", function(ch)
        return esc[ch] or ("\\" .. ch)
      end)) or cell
    end, vim.split(line, sep, { plain = true }))
  end, lines)
end

--- JSON değerini hücre metnine çevirir: `{"$oid": x}` gibi tek anahtarlı EJSON sarmalayıcıları açılır.
---@param v any
---@return string|table
local function json_cell(v)
  if v == vim.NIL or v == nil then
    return M.NULL
  elseif type(v) == "table" then
    local keys = vim.tbl_keys(v)
    if #keys == 1 and type(keys[1]) == "string" and keys[1]:sub(1, 1) == "$" and type(v[keys[1]]) ~= "table" then
      return tostring(v[keys[1]])
    end
    return vim.json.encode(v)
  end
  return tostring(v)
end

--- MongoDB belge dizisi (EJSON) → tablo; kolonlar ilk görülme sırasıyla tüm belgelerin üst düzey anahtarları.
---@param text string
---@return DatasourceGrid?
local function from_json(text)
  local ok, docs = pcall(vim.json.decode, text, { luanil = { object = false, array = false } })
  if not ok or type(docs) ~= "table" then
    return nil
  end
  if not vim.islist(docs) then
    docs = { docs }
  end
  if #docs > 0 and vim.iter(docs):any(function(d) return type(d) ~= "table" or vim.islist(d) end) then
    return { columns = { "value" }, rows = vim.tbl_map(function(d) return { json_cell(d) } end, docs) }
  end
  local columns, seen = {}, {}
  for _, doc in ipairs(docs) do
    for k in pairs(doc) do
      if not seen[k] then
        seen[k] = true
        table.insert(columns, k)
      end
    end
  end
  local id = vim.tbl_contains(columns, "_id")
  columns = vim.tbl_filter(function(c)
    return c ~= "_id"
  end, columns)
  if id then
    table.insert(columns, 1, "_id")
  end
  local rows = vim.tbl_map(function(doc)
    return vim.tbl_map(function(c)
      return json_cell(doc[c])
    end, columns)
  end, docs)
  return { columns = columns, rows = rows }
end

--- Redis çıktısı (redis-cli, tty değilken ham satırlar) → türüne göre kolonlar.
---@param kind string TYPE yanıtı
---@param lines string[]
---@return DatasourceGrid
local function from_redis(kind, lines)
  local function pairs_of(a, b)
    local rows = {}
    for i = 1, #lines - 1, 2 do
      table.insert(rows, { lines[i], lines[i + 1] })
    end
    return { columns = { a, b }, rows = rows }
  end
  if kind == "hash" then
    return pairs_of("field", "value")
  elseif kind == "zset" then
    return pairs_of("member", "score")
  elseif kind == "list" then
    local rows = {}
    for i, l in ipairs(lines) do
      table.insert(rows, { tostring(i - 1), l })
    end
    return { columns = { "index", "value" }, rows = rows }
  elseif kind == "set" then
    table.sort(lines)
    return { columns = { "member" }, rows = vim.tbl_map(function(l) return { l } end, lines) }
  elseif kind == "string" then
    return { columns = { "value" }, rows = { { table.concat(lines, "\n") } } }
  end
  return { columns = { kind }, rows = vim.tbl_map(function(l) return { l } end, lines) }
end

--- İstemci çıktısını tabloya çevirir; biçim tanınmazsa nil.
---@param fmt string "csv" | "mysql" | "sqlcmd" | "json" | "redis:<tür>" | "text" (ayrıştırılmaz)
---@param lines string[]
---@return DatasourceGrid?
function M.parse(fmt, lines)
  local rows
  if fmt == "csv" then
    rows = M.parse_csv(table.concat(lines, "\n"))
  elseif fmt == "mysql" then
    rows = parse_separated(lines, "\t", true)
  elseif fmt == "sqlcmd" then
    -- 2. satır `----` ayırıcısı
    rows = parse_separated(vim.list_extend({ lines[1] }, vim.list_slice(lines, 3)), "\t")
  elseif fmt == "json" then
    return from_json(table.concat(lines, "\n"))
  elseif fmt:match("^redis:") then
    return from_redis(fmt:sub(7), lines)
  end
  if not rows or #rows == 0 then
    return nil
  end
  local columns = vim.tbl_map(tostring, table.remove(rows, 1))
  return { columns = columns, rows = rows }
end

--- Hücre metninin tek satırlık görünümü (satır sonu `↵`, sekme boşluk).
---@param v string|table
---@return string
local function display(v)
  return v == M.NULL and "NULL" or (v:gsub("\r?\n", "↵"):gsub("\t", " "))
end

--- Metni görüntü genişliğine göre keser (`…` ile) ve `width`'e doldurur; `right` ise sağa yaslar.
---@param s string
---@param width integer
---@param right? boolean
---@return string
local function fit(s, width, right)
  local w = vim.fn.strdisplaywidth(s)
  if w > width then
    local out, acc = {}, 0
    for ch in s:gmatch("[%z\1-\127\194-\244][\128-\191]*") do
      local cw = vim.fn.strdisplaywidth(ch)
      if acc + cw > width - 1 then
        break
      end
      table.insert(out, ch)
      acc = acc + cw
    end
    s, w = table.concat(out) .. "…", acc + 1
  end
  local pad = (" "):rep(width - w)
  return right and (pad .. s) or (s .. pad)
end

--- Tabloyu buffer'a çizer. `info` winbar'da başlık görünürken gösterilir (tablo adı, satır sayısı, tuşlar).
---@param buf integer
---@param grid DatasourceGrid
---@param info string
function M.render(buf, grid, info)
  local ncol = #grid.columns
  local widths, numeric = {}, {}
  for c = 1, ncol do
    widths[c] = math.min(MAX_WIDTH, vim.fn.strdisplaywidth(grid.columns[c]))
    local any = false
    numeric[c] = true
    for _, row in ipairs(grid.rows) do
      local v = row[c] or ""
      widths[c] = math.min(MAX_WIDTH, math.max(widths[c], vim.fn.strdisplaywidth(display(v))))
      if v ~= M.NULL and v ~= "" then
        any = true
        numeric[c] = numeric[c] and v:match("^%-?%d+%.?%d*$") ~= nil
      end
    end
    numeric[c] = numeric[c] and any
  end
  local function border(l, m, r)
    return l .. table.concat(vim.tbl_map(function(w) return ("─"):rep(w + 2) end, widths), m) .. r
  end
  local lines, marks = { border("┌", "┬", "┐") }, {}
  --- Bir tablo satırı kurar; hücre vurguları (bayt aralığı) `marks`'a eklenir.
  local function add_row(cells, header)
    local parts, col = { "│" }, #"│"
    for c = 1, ncol do
      local v = cells[c] or ""
      local text = " " .. fit(display(v), widths[c], not header and numeric[c]) .. " "
      local hl = header and "DatasourceGridHeader" or (v == M.NULL and "Comment" or (numeric[c] and "Number" or nil))
      if hl then
        table.insert(marks, { #lines, col, col + #text, hl })
      end
      table.insert(parts, text)
      table.insert(parts, "│")
      col = col + #text + #"│"
    end
    table.insert(lines, table.concat(parts))
  end
  add_row(grid.columns, true)
  local header = lines[2]
  table.insert(lines, border("├", "┼", "┤"))
  for _, row in ipairs(grid.rows) do
    add_row(row)
  end
  table.insert(lines, border("└", "┴", "┘"))

  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.bo[buf].modified = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  for i = 0, #lines - 1 do
    vim.api.nvim_buf_set_extmark(buf, ns, i, 0, { end_col = #lines[i + 1], hl_group = "DatasourceGridBorder", priority = 100 })
  end
  for _, m in ipairs(marks) do
    vim.api.nvim_buf_set_extmark(buf, ns, m[1], m[2], { end_col = m[3], hl_group = m[4], priority = 110 })
  end
  views[buf] = { header = header, info = info, grid = grid, first = HEADER_LINES + 1 }
  for _, win in ipairs(vim.fn.win_findbuf(buf)) do
    vim.wo[win][0].wrap = false
  end
end

--- Satır metnini (hücre vurgusu olmadan) gösterir; hata ya da tanınmayan çıktı için.
---@param buf integer
---@param lines string[]
---@param info string
---@param hl? string tüm satırların vurgusu (ör. hata için `DiagnosticError`)
function M.render_text(buf, lines, info, hl)
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.bo[buf].modified = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  for i, l in ipairs(hl and lines or {}) do
    vim.api.nvim_buf_set_extmark(buf, ns, i - 1, 0, { end_col = #l, hl_group = hl })
  end
  views[buf] = { info = info }
end

--- Görüntü sütunu `from`'dan başlayan kısım (yatay kaydırmada sabit başlığı tabloyla hizalamak için).
---@param s string
---@param from integer
---@return string
local function drop_cols(s, from)
  local acc = 0
  for start, ch in s:gmatch("()([%z\1-\127\194-\244][\128-\191]*)") do
    if acc >= from then
      return s:sub(start)
    end
    acc = acc + vim.fn.strdisplaywidth(ch)
  end
  return ""
end

--- Veri tab'ı winbar'ı: tablonun başında bilgi satırı, başlık yukarı kaydıysa başlık satırı (sütunlar hizalı).
---@return string
function M.winbar()
  local win = vim.g.statusline_winid or vim.api.nvim_get_current_win()
  local view = views[vim.api.nvim_win_get_buf(win)]
  if not view then
    return ""
  end
  local wv = vim.api.nvim_win_call(win, vim.fn.winsaveview)
  local pad = (" "):rep(vim.fn.getwininfo(win)[1].textoff)
  if not view.header or wv.topline <= 2 then
    return pad .. "%#Comment#" .. view.info:gsub("%%", "%%%%")
  end
  return pad .. "%#DatasourceGridHeader#" .. drop_cols(view.header, wv.leftcol):gsub("%%", "%%%%") .. "%*"
end

--- İmleçteki hücre: kolon adı ve değeri; tablo dışındaysa nil.
---@param buf integer
---@return string? column, (string|table)? value
local function cell_at_cursor(buf)
  local view = views[buf]
  if not (view and view.grid) then
    return nil
  end
  local row, col = unpack(vim.api.nvim_win_get_cursor(0))
  local r = view.grid.rows[row - view.first + 1]
  if not r then
    return nil
  end
  local before = vim.api.nvim_get_current_line():sub(1, col + 1)
  local _, bars = before:gsub("│", "")
  local c = math.max(1, math.min(bars, #view.grid.columns))
  return view.grid.columns[c], r[c]
end

--- Hücrenin tam değerini yüzen pencerede gösterir (JSON düzgün girintilenir).
---@param buf integer
function M.show_cell(buf)
  local column, value = cell_at_cursor(buf)
  if not column then
    return
  end
  local text = value == M.NULL and "NULL" or value
  local ft = "text"
  local ok, decoded = pcall(vim.json.decode, text)
  if ok and type(decoded) == "table" and vim.fn.executable("jq") == 1 then
    local res = vim.system({ "jq", "." }, { stdin = text, text = true }):wait()
    if res.code == 0 then
      text, ft = vim.trim(res.stdout), "json"
    end
  end
  vim.lsp.util.open_floating_preview(vim.split(text, "\n"), ft, { border = "rounded", title = " " .. column .. " ",
    max_width = math.floor(vim.o.columns * 0.6), max_height = math.floor(vim.o.lines * 0.6), focus_id = "datasource_cell" })
end

--- Hücre değerini panoya (`+` ve `"`) kopyalar.
---@param buf integer
function M.yank_cell(buf)
  local column, value = cell_at_cursor(buf)
  if not column then
    return
  end
  local text = value == M.NULL and "" or value
  vim.fn.setreg("+", text)
  vim.fn.setreg('"', text)
  vim.notify(("Copied %s (%d chars)"):format(column, #text))
end

--- Veri buffer'ına hücre tuşlarını bağlar ve buffer kapanınca durumu siler.
---@param buf integer
function M.attach(buf)
  if vim.b[buf].datasource_grid then
    return
  end
  vim.b[buf].datasource_grid = true
  for _, lhs in ipairs({ "<CR>", "<2-LeftMouse>", "K" }) do
    vim.keymap.set("n", lhs, function() M.show_cell(buf) end, { buffer = buf, nowait = true, desc = "Show cell value" })
  end
  vim.keymap.set("n", "y", function() M.yank_cell(buf) end, { buffer = buf, nowait = true, desc = "Copy cell value" })
  vim.api.nvim_create_autocmd("BufWipeout", { buffer = buf, once = true, callback = function() views[buf] = nil end })
end

local function highlights()
  vim.api.nvim_set_hl(0, "DatasourceGridHeader", { link = "Title", default = true })
  vim.api.nvim_set_hl(0, "DatasourceGridBorder", { link = "NonText", default = true })
end
highlights()
local group = vim.api.nvim_create_augroup("datasource_grid", { clear = true })
vim.api.nvim_create_autocmd("ColorScheme", { group = group, callback = highlights })
-- Sabit başlık topline/leftcol'a bağlı; kaydırınca winbar yeniden çizilsin
vim.api.nvim_create_autocmd("WinScrolled", {
  group = group,
  callback = function()
    for id in pairs(vim.v.event) do
      local win = tonumber(id)
      if win and vim.api.nvim_win_is_valid(win) and views[vim.api.nvim_win_get_buf(win)] then
        vim.cmd.redrawstatus({ bang = true })
        return
      end
    end
  end,
})

return M
