--- Database paneli (Cmd+5): kayıtlı bağlantılar, altlarında tablolar / koleksiyonlar / anahtarlar. Bağlantılar ve
--- sorgular vim-dadbod ile çalışır; dadbod-ui kullanılmaz. Panel sağ sütunda açılır (yerleşim `workbench.db`).
--- Bağlantı: Enter/Space/tık → sorgu konsolu (kod editöründe tab), `e` düzenle. Tablo: Enter/Space/tık → veri
--- tab'ı, `e` yapı düzenleme tab'ı.
local M = {}

local types = require("datasource.types")

local FT = "datasource"
local DATA_FT = "dsdata"
local PREFIX = "datasource://"
local HINT = " a add · e edit · ⏎ open · d delete · R refresh"
local ns = vim.api.nvim_create_namespace("datasource")
local group = vim.api.nvim_create_augroup("datasource", { clear = true })

local state = {
  buf = nil,
  ---@type DatasourceConn[]?
  saved = nil,
  expanded = {},
  ---@type table<string, { status: "loading"|"ok"|"error", items: string[], err?: string }>
  children = {},
  line_map = {},
}

---@class DatasourceConn
---@field name string
---@field type string
---@field host? string
---@field port? string
---@field user? string
---@field password? string
---@field database? string
---@field options? string `a=b&c=d`
---@field file? string
---@field url? string türü tanınmayan ya da projeden gelen bağlantının ham URL'i
---@field project? boolean `vim.g.dbs`'ten (düzenlenmez)

--- Kayıt dosyası (`NVIM_DATASOURCE_FILE` testler için).
---@return string
local function store_file()
  return vim.env.NVIM_DATASOURCE_FILE or (vim.fn.stdpath("data") .. "/datasource/connections.json")
end

---@return string
local function consoles_dir()
  return vim.fs.dirname(store_file()) .. "/consoles"
end

---@param path string
---@return any?
local function read_json(path)
  local fd = io.open(path, "r")
  if not fd then
    return nil
  end
  local text = fd:read("*a")
  fd:close()
  local ok, data = pcall(vim.json.decode, text)
  return ok and data or nil
end

--- `key=value&...` metni ↔ tablo.
---@param s? string
---@return table
local function parse_options(s)
  local out = {}
  for pair in (s or ""):gmatch("[^&]+") do
    local k, v = pair:match("^%s*([^=]+)=(.*)$")
    if k then
      out[vim.trim(k)] = vim.trim(v)
    end
  end
  return out
end

--- URL'den bağlantı kaydı. `jdbc:` öneki atılır (eski panelde girilen JDBC URL'leri dadbod'da çalışmıyordu).
---@param name string
---@param url string
---@return DatasourceConn
function M.from_url(name, url)
  url = url:gsub("^jdbc:", "")
  local ok, u = pcall(vim.fn["db#url#parse"], url)
  local id = ok and (types.aliases[u.scheme] or u.scheme)
  if not (id and types.by_id[id]) then
    return { name = name, type = id or "unknown", url = url }
  end
  if id == "sqlite" then
    return { name = name, type = id, file = vim.fn["db#url#file_path"](url) }
  end
  local opts = {}
  for k, v in pairs(u.params or {}) do
    table.insert(opts, k .. "=" .. v)
  end
  table.sort(opts)
  return {
    name = name,
    type = id,
    host = u.host,
    port = u.port,
    user = u.user,
    password = u.password,
    database = (u.path or ""):gsub("^/", ""),
    options = table.concat(opts, "&"),
  }
end

--- Bağlantının dadbod URL'i.
---@param conn DatasourceConn
---@return string
function M.url(conn)
  if conn.url then
    return conn.url
  end
  local function v(s)
    return (s and s ~= "") and s or nil
  end
  if conn.type == "sqlite" then
    return vim.fn["db#url#format"]({ scheme = "sqlite", path = vim.fn.fnamemodify(vim.fn.expand(conn.file), ":p") })
  end
  local params = parse_options(conn.options)
  return vim.fn["db#url#format"]({
    scheme = conn.type,
    user = v(conn.user),
    password = v(conn.password),
    host = v(conn.host) or "localhost",
    port = v(conn.port),
    path = v(conn.database) and ("/" .. conn.database) or nil,
    params = next(params) and params or vim.empty_dict(),
  })
end

--- Eski dadbod-ui kaydındaki bağlantıları (bir kerelik) içe aktarır.
---@return DatasourceConn[]
local function migrate()
  local old = read_json(vim.fn.stdpath("data") .. "/dadbod_ui/connections.json")
  if vim.env.NVIM_DATASOURCE_FILE or type(old) ~= "table" then
    return {}
  end
  return vim.tbl_map(function(c)
    return M.from_url(c.name, c.url)
  end, vim.tbl_filter(function(c)
    return type(c) == "table" and c.name and c.url
  end, old))
end

--- Kayıtlı bağlantıları diske yazar (parolalar içerdiği için yalnız kullanıcı okuyabilir).
local function persist()
  local file = store_file()
  vim.fn.mkdir(vim.fs.dirname(file), "p")
  local ok, err = pcall(vim.fn.writefile, { vim.json.encode({ version = 1, connections = state.saved }) }, file)
  if not ok then
    vim.notify("Could not save data sources: " .. tostring(err), vim.log.levels.ERROR)
    return
  end
  vim.uv.fs_chmod(file, 384)
end

---@return DatasourceConn[]
local function saved()
  if not state.saved then
    local data = read_json(store_file())
    if data then
      state.saved = data.connections or {}
    else
      state.saved = migrate()
      if #state.saved > 0 then
        persist()
        vim.notify(("Imported %d connection(s) from the old Database panel"):format(#state.saved))
      end
    end
  end
  return state.saved
end

--- Kayıtlı bağlantılar + projenin `.lazy.lua`'sındaki `vim.g.dbs` (liste ya da ad → URL sözlüğü).
---@return DatasourceConn[]
function M.connections()
  local out = vim.deepcopy(saved())
  local dbs = vim.g.dbs
  if type(dbs) == "table" then
    local list = vim.islist(dbs) and dbs
      or vim.tbl_map(function(k)
        return { name = k, url = dbs[k] }
      end, vim.tbl_keys(dbs))
    for _, c in ipairs(list) do
      if c.name and c.url then
        local conn = M.from_url(c.name, c.url)
        conn.url, conn.project = c.url, true
        table.insert(out, conn)
      end
    end
  end
  return out
end

---@param name string
---@return DatasourceConn?
function M.get(name)
  return vim.iter(M.connections()):find(function(c)
    return c.name == name
  end)
end

--- Komutu arka planda çalıştırır; `cb(ok, satırlar, hata)` ana döngüde çağrılır.
---@param cmd string[]
---@param cb fun(ok: boolean, lines: string[], err: string)
---@param opts? { stdin?: string, timeout?: integer, env?: table<string, string> } varsayılan zaman aşımı 20 sn
local function run(cmd, cb, opts)
  opts = opts or {}
  local sys = { text = true, timeout = opts.timeout or 20000, stdin = opts.stdin, env = opts.env }
  local ok, err = pcall(vim.system, cmd, sys, vim.schedule_wrap(function(res)
    local out = vim.split(((res.stdout or ""):gsub("%s+$", "")), "\n")
    local msg = vim.trim(res.stderr or "")
    if res.code ~= 0 and msg == "" then
      msg = res.signal == 15 and "Timed out" or ("Exited with code " .. res.code)
    end
    cb(res.code == 0, out[1] == "" and #out == 1 and {} or out, msg)
  end))
  if not ok then
    cb(false, {}, tostring(err))
  end
end

---@type table<string, { [1]: fun(), [2]: fun(err: string)? }[]>
local waiting = {}

--- Türün komut satırı istemcisi kuruluysa `on_ready`'yi hemen çağırır. Değilse Homebrew ile kurmayı sorar; kurulum
--- bitince bekleyen tüm istekler devam eder (aynı tür için ikinci soru sorulmaz), iptal ya da hatada `on_missing`.
---@param t DatasourceType
---@param on_ready fun()
---@param on_missing? fun(err: string)
local function with_client(t, on_ready, on_missing)
  local err = types.check_client(t)
  if not err then
    return on_ready()
  end
  if waiting[t.id] then
    table.insert(waiting[t.id], { on_ready, on_missing })
    return
  end
  waiting[t.id] = { { on_ready, on_missing } }
  local function finish(msg)
    local list = waiting[t.id]
    waiting[t.id] = nil
    for _, cb in ipairs(list) do
      if not msg then
        cb[1]()
      elseif cb[2] then
        cb[2](msg)
      end
    end
  end
  if vim.fn.executable("brew") == 0 then
    vim.notify(err .. " (Homebrew not found: https://brew.sh)", vim.log.levels.WARN)
    return finish(err)
  end
  vim.ui.select({ "Install", "Cancel" }, {
    prompt = ("%s needs %s, which is not installed. Run `brew install %s`?"):format(t.label, t.clients[1], t.brew),
  }, function(choice)
    if choice ~= "Install" then
      return finish(err)
    end
    vim.notify(("Installing %s… (brew install %s)"):format(t.clients[1], t.brew))
    local ok, spawn_err = pcall(vim.system, { "brew", "install", t.brew }, { text = true }, vim.schedule_wrap(function(res)
      local still = types.check_client(t)
      if res.code ~= 0 or still then
        local lines = vim.split(vim.trim(res.stderr or ""), "\n")
        local msg = res.code ~= 0 and ("brew install %s failed: %s"):format(t.brew, lines[#lines])
          or (still .. " (installed, but not on PATH)")
        vim.notify(msg, vim.log.levels.ERROR)
        return finish(msg)
      end
      vim.notify(t.clients[1] .. " installed")
      finish()
    end))
    if not ok then
      vim.notify(tostring(spawn_err), vim.log.levels.ERROR)
      finish(tostring(spawn_err))
    end
  end)
end

--- Bağlantının türünü bulur, istemcisi hazır olunca (gerekirse kurdurarak) `fn(t)`'yi çağırır.
---@param conn DatasourceConn
---@param fn fun(t: DatasourceType)
local function with_type(conn, fn)
  local t = types.by_id[conn.type]
  if not t then
    vim.notify(("Unsupported data source type: %s"):format(conn.type), vim.log.levels.WARN)
    return
  end
  with_client(t, function()
    fn(t)
  end)
end

---@param name string
---@return string
local function slug(name)
  return (name:gsub("[^%w%-_.]", "_"))
end

--- Buffer'ı kod penceresinde gösterir (yoksa açar) ve odaklar.
---@param buf integer
local function show_in_code(buf)
  local wb = require("workbench")
  local win = wb.code_win()
  if not win then
    wb.code({ pinned = false })
    win = vim.api.nvim_get_current_win()
  end
  vim.api.nvim_win_set_buf(win, buf)
  vim.api.nvim_set_current_win(win)
  vim.cmd.stopinsert()
end

---@type table<string, integer>
local results = {}

--- Bağlantının sonuç buffer'ı (alt şeritte tablo görünümü; bufferline'da görünmez).
---@param name string
---@return integer
local function result_buf(name)
  local buf = results[name]
  if buf and vim.api.nvim_buf_is_valid(buf) then
    return buf
  end
  buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].bufhidden = "hide"
  vim.bo[buf].filetype = DATA_FT
  vim.api.nvim_buf_set_name(buf, PREFIX .. name .. " (result)")
  require("datasource.grid").attach(buf)
  vim.keymap.set("n", "q", function()
    for _, w in ipairs(vim.fn.win_findbuf(buf)) do
      pcall(vim.api.nvim_win_hide, w)
    end
  end, { buffer = buf, nowait = true, desc = "Close results" })
  results[name] = buf
  return buf
end

--- Sonuç buffer'ını alt şeritte gösterir (şeritteki diğer araçlar gizlenir, odak konsolda kalır).
---@param buf integer
local function show_result(buf)
  for _, w in ipairs(vim.fn.win_findbuf(buf)) do
    if vim.api.nvim_win_get_tabpage(w) == vim.api.nvim_get_current_tabpage() then
      return
    end
  end
  local wb = require("workbench")
  for _, w in ipairs(wb.bottom_wins()) do
    if #vim.api.nvim_tabpage_list_wins(0) > 1 then
      pcall(vim.api.nvim_win_hide, w)
    end
  end
  wb.open_bottom(buf, { enter = false })
end

--- Sorgu metnini bağlantıda çalıştırır; sonuç alt şeritte tablo (sonuç kümesi yoksa mesaj, hatada hata metni).
---@param conn DatasourceConn
---@param t DatasourceType
---@param text string
---@param on_done? fun(ok: boolean, lines: string[], err: string) ham çıktı (MCP)
---@param env? table<string, string> istemciye ek ortam
local function execute(conn, t, text, on_done, env)
  local grid = require("datasource.grid")
  local buf = result_buf(conn.name)
  show_result(buf)
  grid.render_text(buf, { "Running…" }, conn.name .. " · running…")
  local cmd, stdin, fmt = types.query_cmd(t.id, M.url(conn), text)
  local start = vim.uv.hrtime()
  run(cmd, function(ok, lines, err)
    if on_done then
      on_done(ok, lines, err)
    end
    if not vim.api.nvim_buf_is_valid(buf) then
      return
    end
    show_result(buf)
    local head = ("%s · %.2fs"):format(conn.name, (vim.uv.hrtime() - start) / 1e9)
    if not ok then
      return grid.render_text(buf, vim.split(err, "\n"), head .. " · error · q close", "DiagnosticError")
    end
    local data = #lines > 0 and grid.parse(fmt, lines)
    if data and #data.columns > 0 then
      local n = #data.rows
      grid.render(buf, data, ("%s · %d row%s · ⏎ cell · y copy · q close"):format(head, n, n == 1 and "" or "s"))
    else
      grid.render_text(buf, #lines > 0 and lines or { "Statement executed (no rows returned)." }, head .. " · q close")
    end
  end, { stdin = stdin, timeout = 300000, env = env })
end

--- Konsol buffer'ının bağlantısında metni çalıştırır (istemci yoksa kurmayı sorar).
---@param buf integer
---@param text string
local function run_text(buf, text)
  local conn = M.get(vim.b[buf].datasource or "")
  if not conn then
    vim.notify("This buffer is not connected to a data source (open it from the Database panel)", vim.log.levels.WARN)
    return
  end
  if vim.trim(text) == "" then
    vim.notify("Nothing to run", vim.log.levels.INFO)
    return
  end
  with_type(conn, function(t)
    execute(conn, t, text)
  end)
end

--- İmleçteki ifade bloğunu (boş satırlarla ayrılan paragraf) çalıştırır.
local function run_block()
  local buf = vim.api.nvim_get_current_buf()
  local row = vim.api.nvim_win_get_cursor(0)[1]
  local last = vim.api.nvim_buf_line_count(buf)
  local function blank(l)
    return vim.trim(vim.api.nvim_buf_get_lines(buf, l - 1, l, false)[1] or "") == ""
  end
  if blank(row) then
    vim.notify("Put the cursor on a statement (or select it) to run it", vim.log.levels.INFO)
    return
  end
  local first, stop = row, row
  while first > 1 and not blank(first - 1) do
    first = first - 1
  end
  while stop < last and not blank(stop + 1) do
    stop = stop + 1
  end
  run_text(buf, table.concat(vim.api.nvim_buf_get_lines(buf, first - 1, stop, false), "\n"))
end

--- Görsel moddaki seçimi çalıştırır (karakter, satır ya da blok seçimi).
local function run_selection()
  local buf = vim.api.nvim_get_current_buf()
  local lines = vim.fn.getregion(vim.fn.getpos("v"), vim.fn.getpos("."), { type = vim.fn.mode() })
  vim.api.nvim_feedkeys(vim.api.nvim_replace_termcodes("<Esc>", true, false, true), "nx", false)
  run_text(buf, table.concat(lines, "\n"))
end

--- Konsol / düzenleme buffer'ını bağlantıya bağlar: `b:db` (dadbod-completion tamamlama için) ve çalıştırma tuşları
--- (Cmd+Enter, `Space D`): normal/ekleme modunda imleçteki blok, görsel modda seçim. Sonuç alt şeritte tablo.
---@param buf integer
---@param conn DatasourceConn
function M.attach(buf, conn)
  vim.b[buf].db = M.url(conn)
  vim.b[buf].datasource = conn.name
  for _, lhs in ipairs({ "<D-CR>", "<leader>D" }) do
    vim.keymap.set("n", lhs, run_block, { buffer = buf, desc = "Run statement under cursor" })
    vim.keymap.set("x", lhs, run_selection, { buffer = buf, desc = "Run selection" })
  end
  vim.keymap.set("i", "<D-CR>", function()
    vim.cmd.stopinsert()
    vim.schedule(run_block)
  end, { buffer = buf, desc = "Run statement under cursor" })
end

--- Bağlantının sorgu konsolunu (kalıcı dosya, `<veri>/datasource/consoles/<ad>.<uzantı>`) kod editöründe tab olarak açar.
---@param conn DatasourceConn
local function open_console(conn, t)
  local dir = consoles_dir()
  vim.fn.mkdir(dir, "p")
  local path = ("%s/%s.%s"):format(dir, slug(conn.name), t.ext)
  if vim.fn.filereadable(path) == 0 then
    local head = ({
      sql = "-- %s console · Cmd+Enter runs the statement under the cursor or the selection",
      mongo = "// %s console · Cmd+Enter runs the block under the cursor or the selection",
    })[t.kind]
    vim.fn.writefile(head and { head:format(conn.name), "" } or {}, path)
  end
  local wb = require("workbench")
  local win = wb.code_win()
  if not win then
    wb.code({ pinned = false })
    win = vim.api.nvim_get_current_win()
  end
  vim.api.nvim_set_current_win(win)
  vim.cmd.edit(vim.fn.fnameescape(path))
  vim.bo.filetype = t.ft
  M.attach(vim.api.nvim_get_current_buf(), conn)
  vim.cmd.stopinsert()
end

---@param conn DatasourceConn
function M.open_console(conn)
  with_type(conn, function(t)
    open_console(conn, t)
  end)
end

--- `datasource://` buffer'ı (veri / yapı tab'ı) oluşturur ya da var olanı döndürür. `buftype=""` bilerek
--- (gitdiff gibi): workbench onu kod penceresi sayar; yazma `BufWriteCmd` ile engellenir, `:e` yeniler.
---@param name string
---@param ft string
---@param render fun(buf: integer)
---@param editable? boolean
---@return integer
local function tab_buf(name, ft, render, editable)
  local full = PREFIX .. name
  local buf = vim.iter(vim.api.nvim_list_bufs()):find(function(b)
    return vim.api.nvim_buf_get_name(b) == full
  end)
  if buf then
    return buf
  end
  buf = vim.api.nvim_create_buf(true, false)
  vim.api.nvim_buf_set_name(buf, full)
  vim.bo[buf].swapfile = false
  vim.bo[buf].filetype = ft
  vim.api.nvim_create_autocmd("BufWriteCmd", {
    group = group,
    buffer = buf,
    callback = function()
      if editable then
        vim.bo[buf].modified = false
      else
        vim.notify("Data tabs are read-only", vim.log.levels.WARN)
      end
    end,
  })
  vim.api.nvim_create_autocmd("BufReadCmd", {
    group = group,
    buffer = buf,
    callback = function()
      render(buf)
    end,
  })
  if not editable then
    vim.keymap.set("n", "R", function()
      render(buf)
    end, { buffer = buf, nowait = true, desc = "Refresh" })
    vim.keymap.set("n", "q", function()
      Snacks.bufdelete(buf)
    end, { buffer = buf, nowait = true, desc = "Close tab" })
  end
  return buf
end

---@param buf integer
---@param lines string[]
---@param modifiable? boolean
local function set_lines(buf, lines, modifiable)
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = modifiable == true
  vim.bo[buf].modified = false
end

--- Tablonun / koleksiyonun / anahtarın ilk satırlarını kod editöründe salt okunur tab olarak, tablo görünümünde
--- açar (`R` yeniler, hücrede ⏎ tam değer, `y` kopyala). Çıktı ayrıştırılamazsa ham metin gösterilir.
---@param conn DatasourceConn
---@param item string
---@param t DatasourceType
local function open_data(conn, item, t)
  local grid = require("datasource.grid")
  local url = M.url(conn)
  local function render(buf)
    local head = ("%s › %s"):format(conn.name, item)
    grid.render_text(buf, { "Loading…" }, head)
    local function done(fmt)
      return function(ok, lines, err)
        if not vim.api.nvim_buf_is_valid(buf) then
          return
        elseif not ok then
          return grid.render_text(buf, vim.split(err, "\n"), head .. " · error · R retry")
        end
        local data = grid.parse(fmt, lines)
        local info = ("%s · %d row%s%s · ⏎ cell · y copy · R refresh"):format(head, data and #data.rows or #lines,
          (data and #data.rows or #lines) == 1 and "" or "s",
          data and #data.rows >= types.LIMIT and (" (first " .. types.LIMIT .. ")") or "")
        if data and #data.columns > 0 then
          grid.render(buf, data, info)
        else
          grid.render_text(buf, #lines > 0 and lines or { "(empty)" }, info)
        end
      end
    end
    if t.kind == "redis" then
      local cmd = types.data_cmd(t.id, url, item)
      run(cmd, function(ok, lines, err)
        local kind = vim.trim(lines[1] or "")
        if not ok or kind == "none" then
          return done("")(ok and kind ~= "none", {}, err ~= "" and err or "Key not found")
        end
        head = head .. " · " .. kind
        local data_cmd, fmt = types.data_cmd(t.id, url, item, kind)
        run(data_cmd, done(fmt))
      end)
    else
      local cmd, fmt = types.data_cmd(t.id, url, item)
      run(cmd, done(fmt))
    end
  end
  local buf = tab_buf(conn.name .. "/" .. item, DATA_FT, render)
  grid.attach(buf)
  show_in_code(buf)
  render(buf)
end

---@param conn DatasourceConn
---@param item string
function M.open_data(conn, item)
  with_type(conn, function(t)
    open_data(conn, item, t)
  end)
end

--- Tablo yapısı düzenleme tab'ı: üstte mevcut yapı (yorum olarak), altında çalıştırılabilir ALTER şablonları.
---@param conn DatasourceConn
---@param item string
local function open_structure(conn, item, t)
  local url = M.url(conn)
  local comment = t.kind == "sql" and "-- " or (t.kind == "mongo" and "// " or nil)
  local function render(buf)
    local head = ("Edit %s › %s · edit a statement, then Cmd+Enter runs it"):format(conn.name, item)
    local template = types.edit_template(t.id, item)
    local function fill(info)
      if not vim.api.nvim_buf_is_valid(buf) then
        return
      end
      local lines = comment and { comment .. head } or {}
      for _, l in ipairs(comment and info or {}) do
        table.insert(lines, comment .. l)
      end
      table.insert(lines, "")
      set_lines(buf, vim.list_extend(lines, template), true)
    end
    local cmd = types.structure_cmd(t.id, url, item)
    if not cmd then
      return fill({})
    end
    fill({ "Loading structure…" })
    run(cmd, function(ok, lines, err)
      fill(ok and (t.kind == "redis" and { "type: " .. (lines[1] or "?") } or lines) or vim.split(err, "\n"))
    end)
  end
  local buf = tab_buf(conn.name .. "/" .. item .. " (edit)", t.ft, render, true)
  M.attach(buf, conn)
  show_in_code(buf)
  render(buf)
end

---@param conn DatasourceConn
---@param item string
function M.open_structure(conn, item)
  with_type(conn, function(t)
    open_structure(conn, item, t)
  end)
end

--- Alt öğeleri listeler (istemci hazır).
---@param conn DatasourceConn
---@param t DatasourceType
local function list_children(conn, t)
  local ok, cmd = pcall(types.list_cmd, t.id, M.url(conn))
  if not ok then
    state.children[conn.name] = { status = "error", items = {}, err = tostring(cmd) }
    return M.render()
  end
  run(cmd, function(success, lines, msg)
    local items = vim.tbl_filter(function(l)
      return vim.trim(l) ~= ""
    end, lines)
    if t.kind == "redis" then
      table.sort(items)
      items = vim.list_slice(items, 1, 500)
    end
    state.children[conn.name] = success and { status = "ok", items = items }
      or { status = "error", items = {}, err = (vim.split(msg, "\n")[1]) }
    M.render()
  end)
end

--- Bağlantının alt öğelerini (tablolar, koleksiyonlar, anahtarlar) yükler; istemci eksikse kurmayı sorar.
---@param conn DatasourceConn
function M.load_children(conn)
  local t = types.by_id[conn.type]
  local function fail(err)
    state.children[conn.name] = { status = "error", items = {}, err = err }
    M.render()
  end
  if not t then
    return fail("Unsupported type: " .. conn.type)
  end
  state.children[conn.name] = { status = "loading", items = {} }
  M.render()
  with_client(t, function()
    list_children(conn, t)
  end, fail)
end

--- Bağlantıyı açar (alt öğeler gerekirse yüklenir) ya da kapatır.
---@param conn DatasourceConn
---@param open? boolean nil → tersine çevir
local function set_open(conn, open)
  if open == nil then
    open = not state.expanded[conn.name]
  end
  state.expanded[conn.name] = open or nil
  if open and not state.children[conn.name] then
    return M.load_children(conn)
  end
  M.render()
end

--- Panel satırlarını çizer.
function M.render()
  local buf = state.buf
  if not (buf and vim.api.nvim_buf_is_valid(buf)) then
    return
  end
  local lines, marks, map = { HINT, "" }, {}, {}
  local function add(text, entry, hls)
    table.insert(lines, text)
    map[#lines] = entry
    for _, h in ipairs(hls or {}) do
      table.insert(marks, { #lines - 1, h[1], h[2], h[3] })
    end
  end
  local conns = M.connections()
  if #conns == 0 then
    add("  No data sources yet. Press a to add one.", nil, { { 0, -1, "Comment" } })
  end
  for _, conn in ipairs(conns) do
    local t = types.by_id[conn.type]
    local open = state.expanded[conn.name]
    local icon = t and t.icon or types.icons.unknown
    local arrow = open and "▾ " or "▸ "
    local detail = conn.file and vim.fn.fnamemodify(conn.file, ":~")
      or conn.url and conn.project and "project"
      or ((conn.host or "localhost") .. (conn.port and conn.port ~= "" and (":" .. conn.port) or "")
        .. (conn.database and conn.database ~= "" and ("/" .. conn.database) or ""))
    local prefix = " " .. arrow .. icon .. " "
    local text = prefix .. conn.name .. "  " .. detail
    add(text, { kind = "conn", conn = conn }, {
      { #(" " .. arrow), #(" " .. arrow) + #icon, "DatasourceIcon_" .. (t and t.id or "unknown") },
      { #prefix, #prefix + #conn.name, "Directory" },
      { #prefix + #conn.name, -1, "Comment" },
    })
    local ch = state.children[conn.name]
    if open and ch then
      if ch.status == "loading" then
        add("      Loading…", nil, { { 0, -1, "Comment" } })
      elseif ch.status == "error" then
        add("      ✖ " .. (ch.err or "error"), nil, { { 0, -1, "DiagnosticError" } })
      elseif #ch.items == 0 then
        add("      (empty)", nil, { { 0, -1, "Comment" } })
      end
      local child_icon = t and (t.kind == "redis" and types.icons.key or t.kind == "mongo" and types.icons.collection)
        or types.icons.table
      for _, item in ipairs(ch.status == "ok" and ch.items or {}) do
        local p = "     " .. child_icon .. " "
        add(p .. item, { kind = "item", conn = conn, item = item }, { { 5, 5 + #child_icon, "Type" } })
      end
    end
  end
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  vim.api.nvim_buf_set_extmark(buf, ns, 0, 0, { end_col = #HINT, hl_group = "Comment" })
  for _, m in ipairs(marks) do
    vim.api.nvim_buf_set_extmark(buf, ns, m[1], m[2], { end_col = m[3] < 0 and #lines[m[1] + 1] or m[3], hl_group = m[4] })
  end
  state.line_map = map
end

--- İmleçteki (ya da verilen satırdaki) panel girdisi.
---@param row? integer
---@return table?
local function current(row)
  return state.line_map[row or vim.api.nvim_win_get_cursor(0)[1]]
end

--- Enter / Space / tık: bağlantıda konsolu açar ve tabloları gösterir, öğede veri tab'ını açar.
---@param entry? table
local function activate(entry)
  if not entry then
    return
  end
  if entry.kind == "conn" then
    if not state.expanded[entry.conn.name] then
      set_open(entry.conn, true)
    end
    M.open_console(entry.conn)
  else
    M.open_data(entry.conn, entry.item)
  end
end

--- Bağlantı formu için alan listesi (türe göre).
---@param t DatasourceType
---@param c DatasourceConn
---@return DatasourceField[]
local function form_fields(t, c)
  if t.file then
    return {
      { key = "name", label = "Name", value = c.name },
      { key = "file", label = "File", value = c.file, hint = "path to the database file" },
    }
  end
  local fields = {
    { key = "name", label = "Name", value = c.name },
    { key = "host", label = "Host", value = c.host or "localhost" },
    { key = "port", label = "Port", value = c.port or tostring(t.port) },
    { key = "user", label = "User", value = c.user },
    { key = "password", label = "Password", value = c.password, secret = true },
    { key = "database", label = t.kind == "redis" and "DB index" or "Database", value = c.database,
      hint = t.kind == "redis" and "0" or nil },
  }
  if t.kind ~= "redis" then
    table.insert(fields, { key = "options", label = "Options", value = c.options, hint = "e.g. sslmode=require" })
  end
  return fields
end

--- Bağlantı ekleme/düzenleme formu. `old` verilirse düzenler (adı değişirse açık durum yeni ada taşınır).
---@param t DatasourceType
---@param old? DatasourceConn
local function edit_form(t, old)
  local base = old or { name = "", type = t.id }
  require("datasource.form").open({
    title = (old and "Edit " or "New ") .. t.label .. " connection",
    fields = form_fields(t, base),
    submit = function(v)
      if v.name == "" then
        return "Name is required"
      elseif v.name ~= base.name and M.get(v.name) then
        return "A connection named “" .. v.name .. "” already exists"
      elseif t.file and v.file == "" then
        return "File is required"
      elseif not t.file and v.port ~= "" and not v.port:match("^%d+$") then
        return "Port must be a number"
      end
      local conn = vim.tbl_extend("force", { type = t.id }, v)
      for k, val in pairs(conn) do
        if val == "" then
          conn[k] = nil
        end
      end
      local list = saved()
      local idx = old and vim.iter(ipairs(list)):find(function(_, c)
        return c.name == old.name
      end)
      if idx then
        list[idx] = conn
      else
        table.insert(list, conn)
      end
      persist()
      if old then
        state.expanded[old.name], state.children[old.name] = nil, nil
      end
      state.expanded[conn.name] = true
      vim.schedule(function()
        M.load_children(conn)
      end)
    end,
  })
end

--- Yeni bağlantı: önce tür seçilir (ikonlarıyla), sonra tek pencerede tüm alanlar.
function M.add()
  vim.ui.select(types.list, {
    prompt = "New data source",
    format_item = function(t)
      return t.icon .. "  " .. t.label
    end,
  }, function(t)
    if t then
      edit_form(t)
    end
  end)
end

--- Girdiye göre düzenle: bağlantı → form, öğe → yapı düzenleme tab'ı.
---@param entry? table
function M.edit(entry)
  if not entry then
    return
  elseif entry.kind == "item" then
    return M.open_structure(entry.conn, entry.item)
  end
  local conn = entry.conn
  if conn.project then
    vim.notify("This connection comes from the project's .lazy.lua (vim.g.dbs); edit it there", vim.log.levels.INFO)
    return
  end
  local t = types.by_id[conn.type]
  if not t then
    vim.notify("Unsupported data source type: " .. conn.type, vim.log.levels.WARN)
    return
  end
  edit_form(t, conn)
end

--- Bağlantıyı onay alarak siler (konsol dosyası kalır).
---@param entry? table
function M.delete(entry)
  local conn = entry and entry.conn
  if not conn then
    return
  elseif conn.project then
    vim.notify("This connection comes from the project's .lazy.lua (vim.g.dbs)", vim.log.levels.INFO)
    return
  end
  vim.ui.select({ "Cancel", "Delete" }, { prompt = "Delete connection “" .. conn.name .. "”?" }, function(choice)
    if choice ~= "Delete" then
      return
    end
    state.saved = vim.tbl_filter(function(c)
      return c.name ~= conn.name
    end, saved())
    persist()
    state.expanded[conn.name], state.children[conn.name] = nil, nil
    M.render()
  end)
end

--- Satır eylem menüsü (sağ tık / `m`).
---@param entry? table
function M.menu(entry)
  local items = {}
  local function add(label, fn)
    table.insert(items, { label = label, fn = fn })
  end
  if entry and entry.kind == "conn" then
    add("Open query console", function() M.open_console(entry.conn) end)
    add("Edit connection…", function() M.edit(entry) end)
    add("Refresh", function() M.load_children(entry.conn) end)
    add("Delete connection…", function() M.delete(entry) end)
  elseif entry and entry.kind == "item" then
    add("Open data", function() M.open_data(entry.conn, entry.item) end)
    add("Edit structure", function() M.edit(entry) end)
  end
  add("New data source…", M.add)
  vim.ui.select(items, {
    prompt = entry and (entry.item or entry.conn.name) or "Database",
    format_item = function(i)
      return i.label
    end,
  }, function(choice)
    if choice then
      choice.fn()
    end
  end)
end

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
  map("<CR>", activate)
  map("<Space>", activate)
  vim.keymap.set("n", "<LeftRelease>", function()
    local pos = vim.fn.getmousepos()
    local entry = pos.winid == vim.api.nvim_get_current_win() and current(pos.line)
    if entry and entry.kind == "conn" and pos.column <= 3 then
      set_open(entry.conn)
    elseif entry then
      activate(entry)
    end
  end, { buffer = buf, nowait = true })
  map("<Right>", function(e)
    if e and e.kind == "conn" then
      set_open(e.conn, true)
    end
  end)
  map("<Left>", function(e)
    if e and e.kind == "conn" then
      set_open(e.conn, false)
    elseif e then
      for row = vim.api.nvim_win_get_cursor(0)[1], 1, -1 do
        local p = state.line_map[row]
        if p and p.kind == "conn" then
          vim.api.nvim_win_set_cursor(0, { row, 0 })
          return
        end
      end
    end
  end)
  map("e", M.edit)
  map("a", function() M.add() end)
  map("d", M.delete)
  map("m", M.menu)
  map("R", function(e)
    if e then
      M.load_children(e.conn)
    else
      state.children = {}
      M.render()
    end
  end)
  map("q", function()
    require("workbench").toggle("db")
  end)
  require("workbench").on_right_click(FT, function(pos)
    if not (state.buf and pos.winid ~= 0 and vim.api.nvim_win_get_buf(pos.winid) == state.buf) then
      return false
    end
    local entry = state.line_map[pos.line]
    vim.schedule(function()
      M.menu(entry)
    end)
    return true
  end)
  return buf
end

--- Panel penceresi (mevcut sekmede görünüyorsa).
---@return integer?
function M.find_panel()
  if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
    return nil
  end
  for _, win in ipairs(vim.fn.win_findbuf(state.buf)) do
    if vim.api.nvim_win_get_tabpage(win) == vim.api.nvim_get_current_tabpage() then
      return win
    end
  end
end

--- Paneli sağ kenarda açar ve odaklar; yerleşimi (sağ sütun) workbench düzeltir.
---@param width integer
---@return integer win
function M.open_panel(width)
  if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
    state.buf = create_buf()
  end
  local win = vim.api.nvim_open_win(state.buf, true, { split = "right", win = -1, width = width })
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
    winbar = "%#Title# Database",
  }) do
    vim.wo[win][opt] = val
  end
  M.render()
  return win
end

---@param win integer
function M.close_panel(win)
  if vim.api.nvim_win_is_valid(win) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
    vim.api.nvim_win_hide(win)
  end
end

--- Konsol dosyaları oturumdan geri gelince bağlantıya yeniden bağlanır; veri/yapı tab'ları oturuma girmez.
--- Tür ikonu renkleri tanımlanır.
--- MCP için: sorgu çalıştırma (sonuç panelde de görünür), komut çalıştırma, istemci kontrolü, konsol.
M.execute, M.run, M.with_client, M.open_console_raw = execute, run, with_client, open_console

function M.setup()
  vim.api.nvim_create_autocmd({ "BufReadPost", "BufNewFile" }, {
    group = group,
    pattern = consoles_dir() .. "/*",
    callback = function(ev)
      local stem = vim.fn.fnamemodify(ev.file, ":t:r")
      local conn = vim.iter(M.connections()):find(function(c)
        return slug(c.name) == stem
      end)
      if conn then
        M.attach(ev.buf, conn)
      end
    end,
  })
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
  local function hl()
    for _, t in ipairs(types.list) do
      vim.api.nvim_set_hl(0, "DatasourceIcon_" .. t.id, { fg = t.color, default = true })
    end
    vim.api.nvim_set_hl(0, "DatasourceIcon_unknown", { link = "Comment", default = true })
  end
  hl()
  vim.api.nvim_create_autocmd("ColorScheme", { group = group, callback = hl })
end

return M
