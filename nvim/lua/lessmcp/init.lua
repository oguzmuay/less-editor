--- LESS MCP: panellerin AI araçları (editor, git, run, db). Sunucu süreci `lessmcp/server.lua` (her AI oturumu kendi
--- kopyasını başlatır) çağrıları `$NVIM` soketinden buraya iletir; araçlar editörün içinde, panellerin kendi kodunu
--- kullanarak çalışır. Geri dönüşü olmayan işlemler (`approve`) kullanıcı onayı olmadan yapılmaz.
local M = {}

---@class LessTool
---@field name string
---@field description string
---@field schema table JSON Schema (inputSchema)
---@field run fun(args: table, done: fun(text: string, is_error?: boolean), ctx: { agent?: string })

local MODULES = { "lessmcp.tools.editor", "lessmcp.tools.git", "lessmcp.tools.run", "lessmcp.tools.db" }
local SERVER = vim.fn.fnamemodify(vim.fs.joinpath(vim.fs.dirname(debug.getinfo(1, "S").source:sub(2)), "server.lua"), ":p")

---@type table<string, LessTool>?
local registry
local results, seq = {}, 0

---@return table<string, LessTool>, LessTool[]
local function tools()
  if not registry then
    registry = {}
    for _, mod in ipairs(MODULES) do
      for _, tool in ipairs(require(mod)) do
        registry[tool.name] = tool
      end
    end
  end
  local list = vim.tbl_values(registry)
  table.sort(list, function(a, b)
    return a.name < b.name
  end)
  return registry, list
end

--- `tools/list` sonucu (JSON).
---@return string
function M.list()
  local _, list = tools()
  return vim.json.encode({
    tools = vim.tbl_map(function(t)
      return { name = t.name, description = t.description, inputSchema = t.schema }
    end, list),
  })
end

--- Aracı başlatır; sonuç `poll(id)` ile alınır.
---@param name string
---@param args_json string
---@param agent? string çağıran AI'ın adı (`LESS_AI_NAME`)
---@return integer
function M.call(name, args_json, agent)
  seq = seq + 1
  local id = seq
  local function done(text, is_error)
    if results[id] == false then
      results[id] = vim.json.encode({ content = { { type = "text", text = text } }, isError = is_error == true })
    end
  end
  results[id] = false
  vim.schedule(function()
    local tool = tools()[name]
    if not tool then
      return done("Unknown tool: " .. name, true)
    end
    local ok, err = pcall(function()
      tool.run(vim.json.decode(args_json, { luanil = { object = true } }) or {}, done, { agent = agent })
    end)
    if not ok then
      done(tostring(err), true)
    end
  end)
  return id
end

--- Bitmiş çağrının sonucu (JSON) ya da nil.
---@param id integer
---@return string?
function M.poll(id)
  local res = results[id]
  if res then
    results[id] = nil
    return res
  end
end

local queue, showing = {}, false

local function next_approval()
  local req = table.remove(queue, 1)
  if not req then
    showing = false
    return
  end
  showing = true
  local lines = vim.list_extend({ "" }, req.lines)
  table.insert(lines, "")
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].bufhidden = "wipe"
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.bo[buf].filetype = req.ft or ""
  local width = math.min(vim.o.columns - 4, math.max(60, vim.fn.strdisplaywidth(req.title) + 6))
  for _, l in ipairs(lines) do
    width = math.min(vim.o.columns - 4, math.max(width, vim.fn.strdisplaywidth(l) + 2))
  end
  local height = math.min(#lines, vim.o.lines - 6)
  vim.cmd.stopinsert()
  local win = vim.api.nvim_open_win(buf, true, {
    relative = "editor",
    width = width,
    height = height,
    row = math.floor((vim.o.lines - height) / 2) - 1,
    col = math.floor((vim.o.columns - width) / 2),
    border = "rounded",
    style = "minimal",
    title = " " .. req.title .. " ",
    title_pos = "center",
    footer = " y Approve · n Reject ",
    footer_pos = "center",
  })
  vim.wo[win].wrap = true
  local answered = false
  local function answer(ok)
    if answered then
      return
    end
    answered = true
    if vim.api.nvim_win_is_valid(win) then
      vim.api.nvim_win_close(win, true)
    end
    req.cb(ok)
    vim.schedule(next_approval)
  end
  for _, k in ipairs({ "y", "Y" }) do
    vim.keymap.set("n", k, function() answer(true) end, { buffer = buf, nowait = true })
  end
  for _, k in ipairs({ "n", "N", "q", "<Esc>" }) do
    vim.keymap.set("n", k, function() answer(false) end, { buffer = buf, nowait = true })
  end
  vim.api.nvim_create_autocmd("BufWipeout", { buffer = buf, once = true, callback = function() answer(false) end })
end

--- AI'ın istediği işlemi kullanıcıya gösterip onay ister (ortada pencere, y / n). Aynı anda gelen istekler sıraya girer;
--- pencere başka yolla kapanırsa red sayılır.
---@param title string
---@param lines string[]
---@param cb fun(ok: boolean)
---@param ft? string içeriğin filetype'ı (renklendirme)
function M.approve(title, lines, cb, ft)
  table.insert(queue, { title = title, lines = lines, cb = cb, ft = ft })
  if not showing then
    next_approval()
  end
end

--- Yolu çalışma klasörüne göre mutlak yapar.
---@param path string
---@return string
function M.abspath(path)
  return vim.fs.normalize(vim.fn.fnamemodify(vim.fn.expand(path), ":p"))
end

--- AI talimatına eklenen satır.
M.hint = "Your MCP server `less` has the editor's tools: open files and result lists in the editor, the Git panel "
  .. "(stage, unstage, discard, diffs), Run configurations (start, stop, logs, profile, env, VM options, args) and the "
  .. "Database panel (query, tables, consoles). Use them when the user asks to show or do something in the editor. "
  .. "Database writes and git discards wait for the user's approval in the editor."

---@param agent string
---@return table
local function server(agent)
  return {
    command = vim.fn.exepath("nvim") ~= "" and vim.fn.exepath("nvim") or vim.v.progpath,
    args = { "--headless", "--clean", "-l", SERVER },
    env = { NVIM = vim.v.servername, LESS_AI_NAME = agent },
  }
end

--- Gemini'nin oturum başına MCP bayrağı yok ve sistem ayar dosyası root'a ait olmalı; bu yüzden sunucu kullanıcının
--- `~/.gemini/settings.json`'una bir kez eklenir (diğer ayarlar korunur). Soket ve ad oturumun ortamından gelir
--- (`$NVIM`, `$LESS_AI_NAME`); LESS dışında sunucu boş araç listesi verir.
---@param s table
function M.gemini_settings(s)
  local file = vim.env.NVIM_GEMINI_SETTINGS or vim.fs.normalize("~/.gemini/settings.json")
  local data = {}
  if vim.uv.fs_stat(file) then
    local ok, decoded = pcall(vim.json.decode, table.concat(vim.fn.readfile(file), "\n"))
    if not ok or type(decoded) ~= "table" then
      vim.notify("Could not read " .. file .. "; LESS tools are not connected to Gemini", vim.log.levels.WARN)
      return
    end
    data = decoded
  end
  local want = { command = s.command, args = s.args, env = { NVIM = "$NVIM", LESS_AI_NAME = "$LESS_AI_NAME" },
    timeout = 900000 }
  data.mcpServers = type(data.mcpServers) == "table" and data.mcpServers or {}
  if vim.deep_equal(data.mcpServers.less, want) then
    return
  end
  data.mcpServers.less = want
  vim.fn.mkdir(vim.fs.dirname(file), "p")
  vim.fn.writefile(vim.split(vim.json.encode(data, { indent = "  " }), "\n"), file)
end

--- AI türüne göre LESS MCP sunucusunu bağlayan ek argümanlar ve ortam.
---@param kind string
---@param agent string
---@return string[] argv, table<string, string> env
function M.connect(kind, agent)
  local s = server(agent)
  if kind == "claude" then
    return { "--mcp-config", vim.json.encode({ mcpServers = { less = vim.tbl_extend("force", { type = "stdio" }, s) } }) }, {}
  elseif kind == "codex" then
    local env = {}
    for k, v in pairs(s.env) do
      env[#env + 1] = ("%s = %s"):format(k, vim.json.encode(v))
    end
    return {
      "-c", "mcp_servers.less.command=" .. vim.json.encode(s.command),
      "-c", "mcp_servers.less.args=" .. vim.json.encode(s.args),
      "-c", "mcp_servers.less.env={ " .. table.concat(env, ", ") .. " }",
      "-c", "mcp_servers.less.tool_timeout_sec=900",
    }, {}
  elseif kind == "gemini" then
    M.gemini_settings(s)
  end
  return {}, {}
end

return M
