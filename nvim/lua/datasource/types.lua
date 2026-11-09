--- Desteklenen veri kaynağı türleri ve türe özgü komutlar. Bağlantı ve çalıştırma vim-dadbod adaptörleri üzerinden
--- (`db#adapter#dispatch`); burada yalnızca tablo/anahtar listeleme, veri, yapı sorguları ve ikonlar tanımlıdır.
local M = {}

local g = vim.fn.nr2char
local dispatch = function(url, fn)
  return vim.fn["db#adapter#dispatch"](url, fn)
end

---@class DatasourceType
---@field id string dadbod şeması
---@field label string
---@field icon string
---@field color string
---@field kind "sql"|"redis"|"mongo"
---@field port? integer varsayılan port (dosya tabanlı türlerde yok)
---@field file? boolean SQLite gibi dosya tabanlı
---@field clients string[] gereken komut satırı istemcisi (biri yeterli)
---@field brew string istemciyi kuran Homebrew paketi
---@field ext string konsol dosyası uzantısı
---@field ft string konsol filetype'ı

---@type DatasourceType[]
M.list = {
  { id = "postgresql", label = "PostgreSQL", icon = g(0xe76e), color = "#6699cc", kind = "sql", port = 5432,
    clients = { "psql" }, brew = "libpq", ext = "sql", ft = "sql" },
  { id = "mysql", label = "MySQL", icon = g(0xe704), color = "#4d9fbf", kind = "sql", port = 3306,
    clients = { "mysql" }, brew = "mysql-client", ext = "sql", ft = "sql" },
  { id = "mariadb", label = "MariaDB", icon = g(0xe704), color = "#c0765a", kind = "sql", port = 3306,
    clients = { "mariadb", "mysql" }, brew = "mariadb", ext = "sql", ft = "sql" },
  { id = "sqlserver", label = "SQL Server", icon = g(0xe77c), color = "#d04a3d", kind = "sql", port = 1433,
    clients = { "sqlcmd" }, brew = "sqlcmd", ext = "sql", ft = "sql" },
  { id = "sqlite", label = "SQLite", icon = g(0xe7c4), color = "#5ba4d6", kind = "sql", file = true,
    clients = { "sqlite3" }, brew = "sqlite", ext = "sql", ft = "sql" },
  { id = "redis", label = "Redis", icon = g(0xe76d), color = "#e0503f", kind = "redis", port = 6379,
    clients = { "redis-cli" }, brew = "redis", ext = "redis", ft = "redis" },
  { id = "mongodb", label = "MongoDB", icon = g(0xe7a4), color = "#4faa45", kind = "mongo", port = 27017,
    clients = { "mongosh", "mongo" }, brew = "mongosh", ext = "js", ft = "javascript" },
}

M.by_id = {}
for _, t in ipairs(M.list) do
  M.by_id[t.id] = t
end

--- URL şeması takma adları → tür kimliği.
M.aliases = { postgres = "postgresql", mongo = "mongodb", rediss = "redis", mssql = "sqlserver", sqlite3 = "sqlite" }

M.icons = { table = g(0xf0ce), key = g(0xf084), collection = g(0xf1b2), unknown = g(0xf1c0) }

---@param t DatasourceType
---@return string? hata (istemci kurulu değilse)
function M.check_client(t)
  for _, c in ipairs(t.clients) do
    if vim.fn.executable(c) == 1 then
      return nil
    end
  end
  return ("%s not found. Install it with: brew install %s"):format(t.clients[1], t.brew)
end

--- SQL dize sabiti.
local function lit(s)
  return "'" .. s:gsub("'", "''") .. "'"
end

--- "şema.tablo" adını parçalarına ayırır (şemasız adlarda şema nil).
---@return string? schema, string table
local function split(name)
  local schema, tbl = name:match("^([^.]+)%.(.+)$")
  return schema, tbl or name
end

--- Türün lehçesinde tırnaklanmış tek tanımlayıcı (kolon adı gibi; nokta bölünmez).
---@param id string
---@param s string
---@return string
function M.quote_ident(id, s)
  if id == "mysql" or id == "mariadb" then
    return "`" .. s:gsub("`", "``") .. "`"
  elseif id == "sqlserver" then
    return "[" .. s:gsub("%]", "]]") .. "]"
  end
  return '"' .. s:gsub('"', '""') .. '"'
end

--- Türün lehçesinde tırnaklanmış tablo adı.
---@param id string
---@param name string
function M.quote(id, name)
  local schema, tbl = split(name)
  local function q(s)
    return M.quote_ident(id, s)
  end
  if id == "sqlite" or not schema then
    return q(name)
  end
  return q(schema) .. "." .. q(tbl)
end

local SYSTEM = {
  postgresql = "table_schema NOT IN ('pg_catalog', 'information_schema')",
  mysql = "table_schema NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')",
  sqlserver = "1 = 1",
}
SYSTEM.mariadb = SYSTEM.mysql

--- Tablo/koleksiyon/anahtar listesini döndüren komut (her satır bir öğe).
---@param id string
---@param url string
---@return string[]
function M.list_cmd(id, url)
  local q = "SELECT table_schema, table_name FROM information_schema.tables WHERE %s ORDER BY table_schema, table_name"
  if id == "postgresql" then
    return vim.list_extend(dispatch(url, "interactive"), { "-At", "-F", ".", "-c", q:format(SYSTEM[id]) })
  elseif id == "mysql" or id == "mariadb" then
    local sql = q:format(SYSTEM[id]):gsub("table_schema, table_name", "CONCAT(table_schema, '.', table_name)")
    return vim.list_extend(dispatch(url, "interactive"), { "-N", "-B", "-e", sql })
  elseif id == "sqlserver" then
    local sql = "SET NOCOUNT ON; SELECT TABLE_SCHEMA + '.' + TABLE_NAME FROM INFORMATION_SCHEMA.TABLES ORDER BY 1"
    return vim.list_extend(dispatch(url, "interactive"), { "-h", "-1", "-W", "-Q", sql })
  elseif id == "sqlite" then
    local sql = "SELECT name FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY 1"
    return vim.list_extend(dispatch(url, "command"), { "-noheader", sql })
  elseif id == "redis" then
    return vim.list_extend(dispatch(url, "interactive"), { "--scan", "--count", "1000" })
  else
    return vim.list_extend(dispatch(url, "filter"), { "--eval", "db.getCollectionNames().sort().join('\\n')" })
  end
end

--- İlk satırları ham biçimde (tablo görünümü ayrıştırır, `datasource.grid.parse`) getiren komut ve çıktı biçimi.
--- Redis'te önce anahtar türü gerekir (`redis_type` yokken komut `TYPE`, biçim `redis:type`).
---@param id string
---@param url string
---@param name string
---@param redis_type? string
---@return string[] cmd, string fmt
function M.data_cmd(id, url, name, redis_type)
  local limit = M.LIMIT
  if id == "postgresql" then
    return vim.list_extend(dispatch(url, "interactive"), {
      "--csv", "-P", "pager=off", "-c", ("SELECT * FROM %s LIMIT %d"):format(M.quote(id, name), limit) }), "csv"
  elseif id == "mysql" or id == "mariadb" then
    return vim.list_extend(dispatch(url, "interactive"), {
      "-B", "-e", ("SELECT * FROM %s LIMIT %d"):format(M.quote(id, name), limit) }), "mysql"
  elseif id == "sqlserver" then
    return vim.list_extend(dispatch(url, "interactive"), {
      "-W", "-s", "\t", "-Q", ("SET NOCOUNT ON; SELECT TOP %d * FROM %s"):format(limit, M.quote(id, name)) }), "sqlcmd"
  elseif id == "sqlite" then
    return vim.list_extend(dispatch(url, "command"), {
      "-csv", "-header", ("SELECT * FROM %s LIMIT %d"):format(M.quote(id, name), limit) }), "csv"
  elseif id == "redis" then
    local by_type = {
      string = { "GET", name },
      list = { "LRANGE", name, "0", tostring(limit - 1) },
      set = { "SMEMBERS", name },
      zset = { "ZRANGE", name, "0", tostring(limit - 1), "WITHSCORES" },
      hash = { "HGETALL", name },
      stream = { "XRANGE", name, "-", "+", "COUNT", tostring(limit) },
    }
    return vim.list_extend(dispatch(url, "interactive"), by_type[redis_type] or { "TYPE", name }),
      "redis:" .. (redis_type or "type")
  else
    local js = ("const d = db.getCollection(%s).find().limit(%d).toArray(); "
      .. "print(typeof EJSON !== 'undefined' ? EJSON.stringify(d) : tojson(d))"):format(vim.json.encode(name), limit)
    return vim.list_extend(dispatch(url, "filter"), { "--eval", js }), "json"
  end
end

--- Konsoldan çalıştırılan sorgu: komut, stdin'e verilecek metin ve çıktı biçimi (`datasource.grid.parse`).
--- Sonuç kümesi döndürmeyen ifadeler (INSERT, UPDATE…) sessiz çalışır; çıktı yoksa "çalıştı" mesajı gösterilir.
---@param id string
---@param url string
---@param text string
---@return string[] cmd, string? stdin, string fmt
function M.query_cmd(id, url, text)
  if id == "postgresql" then
    return vim.list_extend(dispatch(url, "interactive"), { "--csv", "-q", "-P", "pager=off", "-v", "ON_ERROR_STOP=1" }),
      text, "csv"
  elseif id == "mysql" or id == "mariadb" then
    return vim.list_extend(dispatch(url, "interactive"), { "-B" }), text, "mysql"
  elseif id == "sqlserver" then
    return vim.list_extend(dispatch(url, "interactive"), { "-W", "-s", "\t", "-b" }),
      "SET NOCOUNT ON;\n" .. text .. "\nGO\n", "sqlcmd"
  elseif id == "sqlite" then
    return vim.list_extend(dispatch(url, "command"), { "-csv", "-header", "-bail" }), text, "csv"
  elseif id == "redis" then
    return dispatch(url, "interactive"), text, "text"
  end
  local js = ("const __r = eval(%s); const __v = (__r && typeof __r.toArray === 'function') ? __r.toArray() : __r; "
    .. "print(typeof EJSON !== 'undefined' ? EJSON.stringify(__v) : tojson(__v))"):format(vim.json.encode(text))
  return vim.list_extend(dispatch(url, "filter"), { "--eval", js }), nil, "json"
end

--- Tablo yapısını (kolon, tür, null, varsayılan) gösteren komut; şemasız türlerde nil.
---@param id string
---@param url string
---@param name string
---@return string[]?
function M.structure_cmd(id, url, name)
  local schema, tbl = split(name)
  local q = "SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns "
    .. "WHERE table_name = %s%s ORDER BY ordinal_position"
  local where_schema = schema and (" AND table_schema = " .. lit(schema)) or ""
  if id == "postgresql" then
    return vim.list_extend(dispatch(url, "interactive"), { "-P", "pager=off", "-c", q:format(lit(tbl), where_schema) })
  elseif id == "mysql" or id == "mariadb" then
    return vim.list_extend(dispatch(url, "interactive"), { "-t", "-e", q:format(lit(tbl), where_schema) })
  elseif id == "sqlserver" then
    return vim.list_extend(dispatch(url, "interactive"), { "-W", "-s", " | ", "-Q", q:format(lit(tbl), where_schema) })
  elseif id == "sqlite" then
    return vim.list_extend(dispatch(url, "interactive"), { ("PRAGMA table_info(%s)"):format(lit(name)) })
  elseif id == "redis" then
    return vim.list_extend(dispatch(url, "interactive"), { "TYPE", name })
  end
end

--- Yapı düzenleme buffer'ına konan şablon (kullanıcı düzenleyip Cmd+Enter ile çalıştırır).
---@param id string
---@param name string
---@return string[]
function M.edit_template(id, name)
  local t = M.by_id[id]
  if t.kind == "redis" then
    return {
      "SET " .. name .. " value",
      "EXPIRE " .. name .. " 3600",
      "PERSIST " .. name,
      "RENAME " .. name .. " " .. name .. "_new",
      "DEL " .. name,
    }
  elseif t.kind == "mongo" then
    local c = ("db.getCollection(%s)"):format(vim.json.encode(name))
    return {
      "// Run one statement at a time (Cmd+Enter runs the block under the cursor).",
      c .. ".createIndex({ field: 1 })",
      "",
      c .. ".updateMany({}, { $set: { newField: null } })",
      "",
      c .. ".updateMany({}, { $rename: { oldField: 'newField' } })",
      "",
      c .. ".renameCollection('" .. name .. "_new')",
    }
  end
  local q = M.quote(id, name)
  if id == "sqlite" then
    return {
      "ALTER TABLE " .. q .. " ADD COLUMN new_column TEXT;",
      "",
      "ALTER TABLE " .. q .. " RENAME COLUMN old_name TO new_name;",
      "",
      "ALTER TABLE " .. q .. " DROP COLUMN column_name;",
      "",
      "ALTER TABLE " .. q .. " RENAME TO new_table_name;",
    }
  end
  local modify = ({
    postgresql = "ALTER TABLE %s ALTER COLUMN column_name TYPE text;",
    mysql = "ALTER TABLE %s MODIFY COLUMN column_name TEXT;",
    mariadb = "ALTER TABLE %s MODIFY COLUMN column_name TEXT;",
    sqlserver = "ALTER TABLE %s ALTER COLUMN column_name NVARCHAR(255);",
  })[id]
  local rename = id == "sqlserver" and ("EXEC sp_rename '%s.old_name', 'new_name', 'COLUMN';"):format(name)
    or ("ALTER TABLE " .. q .. " RENAME COLUMN old_name TO new_name;")
  return {
    "ALTER TABLE " .. q .. " ADD " .. (id == "sqlserver" and "" or "COLUMN ") .. "new_column TEXT;",
    "",
    modify:format(q),
    "",
    rename,
    "",
    "ALTER TABLE " .. q .. " DROP COLUMN column_name;",
    "",
    ("CREATE INDEX idx_%s_column ON %s (column_name);"):format((name:gsub("%W", "_")), q),
  }
end

--- Hücre değerinin SQL sabiti (`nil` = NULL). Sayılar da dize olarak yazılır, veritabanı kolon türüne çevirir.
--- MySQL/MariaDB'de ters bölü de kaçırılır (varsayılan `sql_mode`'da kaçış karakteri), SQL Server'da `N'…'`.
---@param id string
---@param v string?
---@return string
function M.value(id, v)
  if v == nil then
    return "NULL"
  end
  local s = v:gsub("'", "''")
  if id == "mysql" or id == "mariadb" then
    s = s:gsub("\\", "\\\\")
  end
  return (id == "sqlserver" and "N'" or "'") .. s .. "'"
end

--- İfadeleri tek transaction'da çalışacak metne sarar; istemciler ilk hatada durur (psql ON_ERROR_STOP, mysql batch,
--- sqlite -bail, sqlcmd -b + XACT_ABORT), COMMIT'e ulaşılmadan bağlantı kapanınca değişiklikler geri alınır.
---@param id string
---@param statements string[]
---@return string
function M.transaction(id, statements)
  local body = table.concat(statements, "\n")
  if id == "sqlserver" then
    return "SET XACT_ABORT ON;\nBEGIN TRANSACTION;\n" .. body .. "\nCOMMIT TRANSACTION;"
  elseif id == "mysql" or id == "mariadb" then
    return "START TRANSACTION;\n" .. body .. "\nCOMMIT;"
  end
  return "BEGIN;\n" .. body .. "\nCOMMIT;"
end

--- Tablonun birincil anahtar kolonlarını (her satırda bir ad, anahtar sırasıyla) döndüren komut; SQL dışı türlerde nil.
---@param id string
---@param url string
---@param name string
---@return string[]?
function M.pk_cmd(id, url, name)
  local schema, tbl = split(name)
  if id == "postgresql" then
    local q = "SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid "
      .. "AND a.attnum = ANY(i.indkey) WHERE i.indisprimary AND i.indrelid = %s::regclass "
      .. "ORDER BY array_position(i.indkey::int2[], a.attnum)"
    return vim.list_extend(dispatch(url, "interactive"), { "-At", "-c", q:format(lit(M.quote(id, name))) })
  elseif id == "mysql" or id == "mariadb" then
    local q = "SELECT column_name FROM information_schema.key_column_usage WHERE constraint_name = 'PRIMARY' "
      .. "AND table_schema = %s AND table_name = %s ORDER BY ordinal_position"
    return vim.list_extend(dispatch(url, "interactive"), {
      "-N", "-B", "-e", q:format(schema and lit(schema) or "DATABASE()", lit(tbl)) })
  elseif id == "sqlserver" then
    local q = "SET NOCOUNT ON; SELECT k.COLUMN_NAME FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS c "
      .. "JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE k ON k.CONSTRAINT_NAME = c.CONSTRAINT_NAME "
      .. "AND k.TABLE_SCHEMA = c.TABLE_SCHEMA AND k.TABLE_NAME = c.TABLE_NAME "
      .. "WHERE c.CONSTRAINT_TYPE = 'PRIMARY KEY' AND c.TABLE_NAME = %s%s ORDER BY k.ORDINAL_POSITION"
    return vim.list_extend(dispatch(url, "interactive"), {
      "-h", "-1", "-W", "-Q", q:format(lit(tbl), schema and (" AND c.TABLE_SCHEMA = " .. lit(schema)) or "") })
  elseif id == "sqlite" then
    return vim.list_extend(dispatch(url, "command"), {
      "-noheader", ("SELECT name FROM pragma_table_info(%s) WHERE pk > 0 ORDER BY pk"):format(lit(name)) })
  end
end

M.LIMIT = 200

return M
