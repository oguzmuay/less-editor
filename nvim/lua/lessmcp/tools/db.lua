--- MCP araçları: Database paneli (Cmd+5). Okuma sorguları onaysız ve salt okunur oturumda çalışır (PostgreSQL
--- `default_transaction_read_only`, MySQL/MariaDB `SET SESSION TRANSACTION READ ONLY`, SQLite `query_only`); okuma
--- olduğu kesin olmayan her şey (yazma, DDL, MongoDB betikleri, bilinmeyen Redis komutları) kullanıcı onayı bekler.
--- Bağlantı parolaları ve URL'ler AI'a verilmez.
local mcp = require("lessmcp")

local M = {}

local MAX_LINES, MAX_CHARS = 300, 40000

local READ_SQL = { select = true, show = true, describe = true, desc = true, explain = true, with = true,
  values = true, table = true, pragma = true }

local WRITE_SQL = {}
for w in ([[insert update delete merge upsert replace drop alter create truncate rename comment grant revoke call exec
  execute do copy into load lock unlock vacuum analyze reindex cluster refresh attach detach set reset begin commit
  rollback savepoint release prepare deallocate listen notify discard handler kill shutdown pg_terminate_backend
  pg_cancel_backend pg_reload_conf lo_import lo_export dblink dblink_exec load_file sleep benchmark xp_cmdshell
  sp_configure openrowset opendatasource bulk]]):gmatch("%S+") do
  WRITE_SQL[w] = true
end

local READ_REDIS = {}
for w in ([[get mget strlen getrange exists type ttl pttl keys scan dbsize info ping hget hmget hgetall hkeys hvals
  hlen hexists hstrlen hscan lrange llen lindex smembers scard sismember smismember srandmember sscan zrange
  zrangebyscore zrevrange zrevrangebyscore zcard zscore zrank zrevrank zcount zscan xrange xrevrange xlen
  bitcount getbit pfcount object memory]]):gmatch("%S+") do
  READ_REDIS[w] = true
end

--- Yorumları ve tırnak içlerini (string, tırnaklı ad, PostgreSQL $$ gövdesi) atar; anahtar kelime aramasında
--- yanlış pozitif olmasın, gizli kelime de kaçmasın diye içerik boş bırakılır.
---@param sql string
---@return string
local function strip(sql)
  return (sql:gsub("%-%-[^\n]*", " "):gsub("/%*.-%*/", " "):gsub("%$(%w*)%$.-%$%1%$", " '' ")
    :gsub("'[^']*'", " '' "):gsub('"[^"]*"', " x "):gsub("`[^`]*`", " x "):gsub("%[[^%]]*%]", " x "))
end

--- Sorgu onaysız (salt okunur) çalıştırılabilir mi? Şüphede hayır.
---@param kind "sql"|"redis"|"mongo"
---@param text string
---@return boolean
function M.is_read(kind, text)
  if kind == "redis" then
    local any = false
    for line in vim.gsplit(text, "\n", { trimempty = true }) do
      local cmd = vim.trim(line):match("^(%S+)")
      if cmd then
        any = true
        if not READ_REDIS[cmd:lower()] then
          return false
        end
      end
    end
    return any
  elseif kind ~= "sql" then
    return false
  end
  local clean = strip(text):lower()
  if clean:gsub("''", ""):find("'", 1, true) then
    return false -- kapanmamış tırnak
  end
  local any = false
  for stmt in vim.gsplit(clean, ";", { plain = true }) do
    local first = stmt:match("^%s*([%a_]+)")
    if first then
      any = true
      if not READ_SQL[first] or (first == "pragma" and stmt:find("=", 1, true)) then
        return false
      end
      for word in stmt:gmatch("[%a_][%w_]*") do
        if WRITE_SQL[word] then
          return false
        end
      end
    elseif vim.trim(stmt) ~= "" then
      return false
    end
  end
  return any
end

--- Okuma sorgusunu veritabanının kendi salt okunur moduna sokar.
---@param id string
---@param text string
---@return string text, table<string, string>? env
function M.guard(id, text)
  if id == "postgresql" then
    return text, { PGOPTIONS = vim.trim((vim.env.PGOPTIONS or "") .. " -c default_transaction_read_only=on") }
  elseif id == "mysql" or id == "mariadb" then
    return "SET SESSION TRANSACTION READ ONLY;\n" .. text
  elseif id == "sqlite" then
    return "PRAGMA query_only = ON;\n" .. text
  end
  return text
end

---@param lines string[]
---@return string
local function cap(lines)
  local out = table.concat(vim.list_slice(lines, 1, MAX_LINES), "\n")
  if #lines > MAX_LINES then
    out = out .. ("\n… %d more lines (narrow the query or add LIMIT)"):format(#lines - MAX_LINES)
  end
  return #out > MAX_CHARS and (out:sub(1, MAX_CHARS) .. "\n… truncated") or out
end

---@param name string?
---@param done fun(text: string, is_error?: boolean)
---@param fn fun(conn: DatasourceConn, t: DatasourceType)
local function with_conn(name, done, fn)
  local ds = require("datasource")
  local conn = name and ds.get(name)
  if not conn then
    local names = vim.tbl_map(function(c) return c.name end, ds.connections())
    return done(("No connection named %s. Connections: %s"):format(tostring(name), table.concat(names, ", ")), true)
  end
  local t = require("datasource.types").by_id[conn.type]
  if not t then
    return done("Unsupported data source type: " .. conn.type, true)
  end
  ds.with_client(t, function()
    fn(conn, t)
  end, function(err)
    done(err, true)
  end)
end

---@param done fun(text: string, is_error?: boolean)
local function reply(done)
  return function(ok, lines, err)
    if not ok then
      return done(err ~= "" and err or "Query failed", true)
    end
    done(#lines > 0 and cap(lines) or "Statement executed (no rows returned).")
  end
end

local conn_prop = { type = "string", description = "Connection name from db_connections" }

M.tools = {
  {
    name = "db_connections",
    description = "Database connections of the Database panel (name, type, host, database). No credentials.",
    schema = { type = "object", properties = vim.empty_dict() },
    run = function(_, done)
      local out = vim.tbl_map(function(c)
        return vim.json.encode({ name = c.name, type = c.type, host = c.host, port = c.port, database = c.database,
          file = c.file })
      end, require("datasource").connections())
      done(#out > 0 and table.concat(out, "\n") or "No connections. The user adds them in the Database panel (Cmd+5).")
    end,
  },
  {
    name = "db_tables",
    description = "Tables (SQL), collections (MongoDB) or keys (Redis, first 500) of a connection.",
    schema = { type = "object", properties = { connection = conn_prop }, required = { "connection" } },
    run = function(args, done)
      with_conn(args.connection, done, function(conn, t)
        local ds, types = require("datasource"), require("datasource.types")
        ds.run(types.list_cmd(t.id, ds.url(conn)), reply(done))
      end)
    end,
  },
  {
    name = "db_describe",
    description = "Columns of a table (name, type, nullable, default). SQL databases only.",
    schema = {
      type = "object",
      properties = { connection = conn_prop, table = { type = "string", description = "table or schema.table" } },
      required = { "connection", "table" },
    },
    run = function(args, done)
      with_conn(args.connection, done, function(conn, t)
        local ds, types = require("datasource"), require("datasource.types")
        local cmd = t.kind == "sql" and types.structure_cmd(t.id, ds.url(conn), args.table)
        if not cmd then
          return done("Not available for " .. t.label, true)
        end
        ds.run(cmd, reply(done))
      end)
    end,
  },
  {
    name = "db_query",
    description = "Run a query on a connection; the result also appears in the editor's result panel. Read-only "
      .. "statements (SELECT, SHOW, EXPLAIN, read Redis commands) run at once in a read-only session. Anything else "
      .. "(INSERT, UPDATE, DELETE, DDL, MongoDB scripts) is shown to the user and runs only after they approve. "
      .. "SQL: use the database's dialect; add LIMIT to large selects.",
    schema = {
      type = "object",
      properties = { connection = conn_prop, query = { type = "string" } },
      required = { "connection", "query" },
    },
    run = function(args, done, ctx)
      local text = args.query or ""
      if vim.trim(text) == "" then
        return done("Empty query", true)
      end
      with_conn(args.connection, done, function(conn, t)
        local ds = require("datasource")
        if M.is_read(t.kind, text) then
          local q, env = M.guard(t.id, text)
          return ds.execute(conn, t, q, reply(done), env)
        end
        local lines = { ("Run on %s (%s)?"):format(conn.name, t.label), "" }
        vim.list_extend(lines, vim.split(text, "\n"))
        mcp.approve((ctx.agent or "AI") .. " · database write", lines, function(ok)
          if not ok then
            return done("The user rejected this query. Do not retry it unless they ask.", true)
          end
          ds.execute(conn, t, text, reply(done))
        end, t.ft)
      end)
    end,
  },
  {
    name = "db_open_console",
    description = "Write a query into the connection's console tab in the editor without running it; the user "
      .. "reviews and runs it (Cmd+Enter). Use this when the user wants to run the query themselves.",
    schema = {
      type = "object",
      properties = { connection = conn_prop, query = { type = "string" } },
      required = { "connection", "query" },
    },
    run = function(args, done)
      with_conn(args.connection, done, function(conn, t)
        require("datasource").open_console_raw(conn, t)
        local buf = vim.api.nvim_get_current_buf()
        local last = vim.api.nvim_buf_line_count(buf)
        local lines = vim.split(args.query or "", "\n")
        local blank = vim.trim(vim.api.nvim_buf_get_lines(buf, last - 1, last, false)[1] or "") == ""
        vim.api.nvim_buf_set_lines(buf, last, last, false, vim.list_extend(blank and {} or { "" }, lines))
        vim.api.nvim_win_set_cursor(0, { vim.api.nvim_buf_line_count(buf) - #lines + 1, 0 })
        done(("Query written to the %s console; the user can run it with Cmd+Enter."):format(conn.name))
      end)
    end,
  },
}

return setmetatable(M.tools, { __index = M })
