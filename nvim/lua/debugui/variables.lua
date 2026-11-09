--- Debug panelinin Variables bölümü (dap-ui `scopes` elemanının yerine): o anki frame'in scope'ları ve değişkenleri
--- ağaç olarak. ⏎ / çift tık / → ← aç-kapa, `e` / F2 değeri değiştirir (mevcut değerle dolu giriş penceresi; değer bir
--- ifadedir, ör. `'abc'`, `42`, `null`), `y` değeri kopyalar. Son duraktan beri değişen değerler vurgulanır.
--- Değiştirme DAP `setVariable` ile (yoksa `setExpression`): js-debug'da `setExpression` programı değiştiriyor ama
--- scope listesi durma anındaki kopyadan geldiği için panel eski değeri gösteriyordu.
--- dap-ui'nin kendi düzenlemesi bu panelde bozuktu: tüm listeyi prompt buffer'ına çevirip değeri göndermiyordu.
local M = {}

local HEADER_LINES = 3 -- tablo üst çerçevesi + başlık + ayırıcı (`datasource.grid`)

---@class DebugVarNode
---@field kind "scope"|"var"
---@field name string
---@field value? string
---@field type? string
---@field ref integer variablesReference (0 = alt öğe yok)
---@field evaluateName? string
---@field parent_ref? integer
---@field parent? DebugVarNode
---@field path string
---@field depth integer
---@field expensive? boolean
---@field children? DebugVarNode[]

local state = {
  buf = nil, ---@type integer?
  expanded = {}, ---@type table<string, boolean>
  seen = {}, ---@type table<string, boolean> açılış durumu bir kez varsayılandan alınan scope'lar
  roots = {}, ---@type DebugVarNode[]
  rows = {}, ---@type DebugVarNode[] tablo satırları (sırayla)
  current = {}, ---@type table<string, string> yol → değer (son yükleme)
  baseline = {}, ---@type table<string, string> yol → değer (önceki durak)
  overrides = {}, ---@type table<string, string> bu duraklamada değiştirilen değerler (js-debug listeyi önbellekten verir)
  own = 0, -- kendi attığımız, `after.scopes` dinleyicisinde atlanacak scopes istekleri (yoksa sonsuz döngü)
  session = nil, ---@type table? son duran oturum (js-debug'da çocuk oturum; `dap.session()` ebeveyn olabilir)
  gen = 0,
  status = "idle", ---@type "idle"|"paused"|"running"
}

--- Tek satırlık değer görünümü.
---@param s string?
---@return string
local function flat(s)
  return ((s or ""):gsub("\r?\n", "↵"))
end

--- Liste elemanı mı: ad yalnız sayı (js-debug `0`) ya da `[0]` (java-debug); dizini döndürür.
---@param node DebugVarNode
---@return integer?
local function index_of(node)
  return tonumber(node.name:match("^%[?(%d+)%]?$"))
end

--- Tablo satırının ad hücresi: girinti + aç/kapa işareti + ad (liste elemanı `[i]`).
---@param node DebugVarNode
---@return string
local function name_cell(node)
  local icon = node.ref > 0 and (state.expanded[node.path] and "▾ " or "▸ ") or "  "
  local i = node.kind == "var" and index_of(node)
  return ("  "):rep(node.depth) .. icon .. (i and ("[" .. i .. "]") or node.name)
end

--- Değişkenleri `Name | Value | Type` tablosu olarak çizer (Database veri tab'ıyla aynı görünüm, `datasource.grid`).
--- Scope satırları başlık renginde, son duraktan beri değişen değerler sarı, program çalışırken tablo soluk.
local function render()
  local buf = state.buf
  if not (buf and vim.api.nvim_buf_is_valid(buf)) then
    return
  end
  local grid = require("datasource.grid")
  state.rows = {}
  if state.status == "idle" or #state.roots == 0 then
    local msg = state.status == "idle" and " No debug session · Cmd+F8 toggles a breakpoint, Debug starts from the Run panel"
      or (state.status == "running" and " Running…" or " Loading…")
    grid.render_text(buf, { msg }, "", "Comment")
    return
  end
  local rows = {}
  local function add(node)
    table.insert(state.rows, node)
    table.insert(rows, { name_cell(node), node.kind == "var" and flat(node.value) or "", node.type or "" })
    for _, child in ipairs(state.expanded[node.path] and node.children or {}) do
      add(child)
    end
  end
  for _, root in ipairs(state.roots) do
    add(root)
  end
  local dim = state.status ~= "paused"
  grid.render(buf, { columns = { "Name", "Value", "Type" }, rows = rows }, "Variables", function(r, c)
    local node = state.rows[r]
    if dim then
      return "Comment"
    elseif node.kind == "scope" then
      return c == 1 and "Title" or nil
    elseif c == 1 then
      return "Identifier"
    elseif c == 2 then
      local changed = state.baseline[node.path] ~= nil and state.baseline[node.path] ~= node.value
      return changed and "DebugVariableChanged" or "DebugVariableValue"
    end
    return "Comment"
  end)
end

--- Değişkenlerin (açık olanların alt öğeleri dahil) yüklenmesi; bitince `done`.
---@param session table
---@param node DebugVarNode
---@param gen integer
---@param done fun()
local function load(session, node, gen, done)
  if node.ref == 0 or not state.expanded[node.path] then
    return done()
  end
  session:request("variables", { variablesReference = node.ref }, function(err, resp)
    if gen ~= state.gen then
      return
    end
    node.children = {}
    local vars = (not err and resp and resp.variables) or {}
    local pending = #vars + 1
    local function one()
      pending = pending - 1
      if pending == 0 then
        done()
      end
    end
    for _, v in ipairs(vars) do
      local path = node.path .. "\0" .. v.name
      local child = {
        kind = "var",
        name = v.name,
        value = state.overrides[path] or v.value,
        type = v.type,
        ref = v.variablesReference or 0,
        evaluateName = v.evaluateName,
        parent_ref = node.ref,
        parent = node,
        path = path,
        depth = node.depth + 1,
      }
      state.current[child.path] = child.value
      table.insert(node.children, child)
      load(session, child, gen, one)
    end
    one()
  end)
end

--- Değişkenleri gösterilecek oturum: son duran oturum hâlâ açıksa o, değilse `dap.session()`.
---@return table?
local function current_session()
  local s = state.session
  if s and not s.closed and s.current_frame then
    return s
  end
  local cur = require("dap").session()
  if s and not s.closed and not (cur and cur.current_frame) then
    return s
  end
  return cur
end

--- Etkin oturumun o anki frame'inden scope'ları ve açık değişkenleri yeniden yükler.
function M.refresh()
  local session = current_session()
  state.gen = state.gen + 1
  local gen = state.gen
  if not session then
    state.status, state.roots = "idle", {}
    return render()
  end
  local frame = session.current_frame
  if not frame then
    state.status = "running"
    return render()
  end
  state.status = "paused"
  state.own = (state.own or 0) + 1
  session:request("scopes", { frameId = frame.id }, function(err, resp)
    if gen ~= state.gen then
      return
    end
    local roots = {}
    state.current = {}
    for _, s in ipairs((not err and resp and resp.scopes) or {}) do
      local node = {
        kind = "scope",
        name = s.name,
        ref = s.variablesReference or 0,
        path = s.name,
        depth = 0,
        expensive = s.expensive,
      }
      if not state.seen[node.path] then
        state.seen[node.path] = true
        state.expanded[node.path] = not s.expensive
      end
      table.insert(roots, node)
    end
    local pending = #roots + 1
    local function one()
      pending = pending - 1
      if pending == 0 and gen == state.gen then
        state.roots = roots
        vim.schedule(render)
      end
    end
    for _, node in ipairs(roots) do
      load(session, node, gen, one)
    end
    one()
  end)
end

---@return DebugVarNode?
local function node_at_cursor()
  local buf = state.buf
  local win = buf and vim.fn.bufwinid(buf)
  if not win or win == -1 then
    return nil
  end
  return state.rows and state.rows[vim.api.nvim_win_get_cursor(win)[1] - HEADER_LINES]
end

--- Duran oturum; durmuyorsa bilgi verip nil.
---@return table?
local function paused_session()
  local session = current_session()
  if not (session and session.current_frame) then
    vim.notify("Pause at a breakpoint to change variables", vim.log.levels.INFO)
    return nil
  end
  return session
end

--- Oturumun dili (ifade yazımı için): adaptör türünden, olmazsa frame kaynağının uzantısından.
---@param session table
---@return "js"|"java"|"python"|nil
local function language(session)
  local t = (session.config and session.config.type) or ""
  if t:match("node") or t:match("chrome") or t:match("msedge") then
    return "js"
  elseif t == "java" then
    return "java"
  elseif t == "python" or t == "debugpy" then
    return "python"
  end
  local path = session.current_frame.source and session.current_frame.source.path or ""
  local ext = path:match("%.(%w+)$")
  return ({ js = "js", mjs = "js", cjs = "js", ts = "js", tsx = "js", jsx = "js", java = "java", py = "python" })[ext]
end

--- Değişiklikten sonra listeyi yeniden yükler. `parent` verilirse onun altındaki bu duraklamaya ait değer
--- düzeltmeleri silinir (eleman silinince/eklenince dizinler kayar).
---@param session table
---@param parent? DebugVarNode
local function reload(session, parent)
  if parent then
    for path in pairs(state.overrides) do
      if vim.startswith(path, parent.path .. "\0") then
        state.overrides[path] = nil
      end
    end
  end
  if session.current_frame then
    pcall(session._request_scopes, session, session.current_frame)
  end
  M.refresh()
end

--- Değişkene değer verir (değer adaptörde ifade olarak değerlendirilir): DAP `setVariable`, yoksa `setExpression`.
---@param node DebugVarNode
---@param value string
local function set_value(node, value)
  local session = paused_session()
  if not session then
    return
  end
  local caps = session.capabilities or {}
  local cmd, args
  if caps.supportsSetVariable and node.parent_ref then
    cmd, args = "setVariable", { variablesReference = node.parent_ref, name = node.name, value = value }
  elseif caps.supportsSetExpression and node.evaluateName then
    cmd, args = "setExpression", { expression = node.evaluateName, value = value, frameId = session.current_frame.id }
  else
    vim.notify("This debugger can't change variables", vim.log.levels.WARN)
    return
  end
  session:request(cmd, args, function(err, resp)
    vim.schedule(function()
      if err then
        vim.notify(("Could not set %s: %s"):format(node.name, err.message or tostring(err)), vim.log.levels.ERROR)
        return
      end
      state.overrides[node.path] = resp and resp.value or value
      reload(session)
    end)
  end)
end

--- İfadeyi duran frame'de çalıştırır (`evaluate`, repl bağlamı), sonra listeyi yeniler.
---@param session table
---@param expr string
---@param parent DebugVarNode
local function run(session, expr, parent)
  session:request("evaluate", { expression = expr, frameId = session.current_frame.id, context = "repl" }, function(err)
    vim.schedule(function()
      if err then
        vim.notify(("%s failed: %s"):format(expr, err.message or tostring(err)), vim.log.levels.ERROR)
        return
      end
      reload(session, parent)
    end)
  end)
end

--- `e` / F2: değeri mevcut değerle dolu giriş penceresinde düzenletir.
---@param node DebugVarNode?
function M.edit(node)
  if not node or node.kind ~= "var" or not paused_session() then
    return
  end
  vim.ui.input({ prompt = ("Set %s (expression)"):format(name_cell(node):gsub("^[%s▸▾]+", "")), default = node.value }, function(value)
    if value and value ~= node.value then
      set_value(node, value)
    end
  end)
end

--- `n` / Del: değeri null yapar (Python'da None).
---@param node DebugVarNode?
function M.set_null(node)
  local session = node and node.kind == "var" and paused_session()
  if session then
    set_value(node, language(session) == "python" and "None" or "null")
  end
end

--- `d`: elemanı içinde bulunduğu listeden / nesneden siler. JS `splice` / `delete`, Java `List.remove` /
--- `Map.remove`, Python `pop`. Dizi (Java array) gibi boyutu sabit yapılarda hata mesajı adaptörden gelir.
---@param node DebugVarNode?
function M.delete(node)
  local session = node and node.kind == "var" and paused_session()
  if not session then
    return
  end
  local parent = node.parent
  local target = parent and parent.kind == "var" and parent.evaluateName
  if not target then
    vim.notify("Only elements of a list, map or object can be deleted", vim.log.levels.INFO)
    return
  end
  local i, lang = index_of(node), language(session)
  local key = vim.json.encode(node.name)
  local expr
  if lang == "js" then
    expr = i and ("%s.splice(%d, 1)"):format(target, i) or ("delete %s[%s]"):format(target, key)
  elseif lang == "java" then
    expr = i and ("%s.remove(%d)"):format(target, i) or ("%s.remove(%s)"):format(target, key)
  elseif lang == "python" then
    expr = ("%s.pop(%s)"):format(target, i or key)
  else
    vim.notify("Deleting elements is not supported for this debugger", vim.log.levels.WARN)
    return
  end
  run(session, expr, parent)
end

--- `a`: listeye eleman ekler (JS `push`, Java `add`, Python `append`); imleç listede ya da elemanındayken.
---@param node DebugVarNode?
function M.append(node)
  local session = node and node.kind == "var" and paused_session()
  if not session then
    return
  end
  local list = node.ref > 0 and node.evaluateName and not index_of(node) and node or node.parent
  if not (list and list.kind == "var" and list.evaluateName) then
    vim.notify("Put the cursor on a list to add an element", vim.log.levels.INFO)
    return
  end
  local method = ({ js = "push", java = "add", python = "append" })[language(session)]
  if not method then
    vim.notify("Adding elements is not supported for this debugger", vim.log.levels.WARN)
    return
  end
  vim.ui.input({ prompt = ("Add to %s (expression)"):format(list.name) }, function(value)
    if value and value ~= "" then
      state.expanded[list.path] = true
      run(session, ("%s.%s(%s)"):format(list.evaluateName, method, value), list)
    end
  end)
end

--- `K`: değerin tamamını yüzen pencerede gösterir.
---@param node DebugVarNode?
local function show(node)
  if node and node.value then
    vim.lsp.util.open_floating_preview(vim.split(node.value, "\n"), "text", { border = "rounded",
      title = " " .. node.name .. " ", max_width = math.floor(vim.o.columns * 0.6), focus_id = "debugui_value" })
  end
end

--- ⏎ / çift tık: alt öğesi olan satırı aç-kapa, yaprak değişkeni düzenle.
---@param node DebugVarNode?
local function activate(node)
  if not node then
    return
  elseif node.ref > 0 then
    state.expanded[node.path] = not state.expanded[node.path]
    M.refresh()
  else
    M.edit(node)
  end
end

---@param open boolean
local function set_open(open)
  local node = node_at_cursor()
  if node and node.ref > 0 and (state.expanded[node.path] or false) ~= open then
    activate(node)
  elseif node and not open and node.parent and node.depth > 0 then
    for r, n in ipairs(state.rows) do
      if n == node.parent then
        vim.api.nvim_win_set_cursor(0, { r + HEADER_LINES, 0 })
        return
      end
    end
  end
end

--- Panel buffer'ı (yoksa oluşturur).
---@return integer
function M.buffer()
  if state.buf and vim.api.nvim_buf_is_valid(state.buf) then
    return state.buf
  end
  local buf = vim.api.nvim_create_buf(false, true)
  state.buf = buf
  vim.bo[buf].bufhidden = "hide"
  vim.bo[buf].filetype = "debugvars"
  vim.api.nvim_buf_set_name(buf, "debug://variables")
  local function map(lhs, fn, desc)
    vim.keymap.set("n", lhs, fn, { buffer = buf, nowait = true, desc = desc })
  end
  map("<CR>", function() activate(node_at_cursor()) end, "Expand / edit variable")
  map("<2-LeftMouse>", function() activate(node_at_cursor()) end, "Expand / edit variable")
  map("e", function() M.edit(node_at_cursor()) end, "Set value")
  map("<F2>", function() M.edit(node_at_cursor()) end, "Set value")
  map("<Right>", function() set_open(true) end, "Expand")
  map("<Left>", function() set_open(false) end, "Collapse")
  map("y", function()
    local node = node_at_cursor()
    if node and node.value then
      vim.fn.setreg("+", node.value)
      vim.fn.setreg('"', node.value)
      vim.notify("Copied " .. node.name)
    end
  end, "Copy value")
  map("n", function() M.set_null(node_at_cursor()) end, "Set null")
  map("<Del>", function() M.set_null(node_at_cursor()) end, "Set null")
  map("d", function() M.delete(node_at_cursor()) end, "Delete element")
  map("a", function() M.append(node_at_cursor()) end, "Add element")
  map("K", function() show(node_at_cursor()) end, "Show full value")
  map("R", M.refresh, "Refresh")
  map("q", function() require("debugui").close_panel() end, "Hide variables")
  pcall(function()
    require("workbench").list_cursor(buf)
  end)
  render()
  return buf
end

--- dap dinleyicileri: durunca (ve frame değişince) yeniden yükler, devam edince soluklaşır, bitince temizlenir.
function M.setup()
  local dap = require("dap")
  dap.listeners.after.event_stopped["debugui_variables"] = function(session)
    state.session = session
    state.baseline = state.current
    state.overrides = {}
  end
  dap.listeners.after.scopes["debugui_variables"] = function(session)
    if (state.own or 0) > 0 then
      state.own = state.own - 1
      return
    end
    if session.current_frame then
      state.session = session
    end
    vim.schedule(M.refresh)
  end
  dap.listeners.after.event_continued["debugui_variables"] = function()
    vim.schedule(function()
      state.status = "running"
      state.overrides = {}
      render()
    end)
  end
  for _, event in ipairs({ "event_terminated", "event_exited", "disconnect" }) do
    dap.listeners.after[event]["debugui_variables"] = function()
      vim.schedule(M.refresh)
    end
  end
  local function hl()
    vim.api.nvim_set_hl(0, "DebugVariableValue", { link = "String", default = true })
    vim.api.nvim_set_hl(0, "DebugVariableChanged", { link = "DiagnosticWarn", default = true })
  end
  hl()
  vim.api.nvim_create_autocmd("ColorScheme", {
    group = vim.api.nvim_create_augroup("debugui_variables", { clear = true }),
    callback = hl,
  })
end

return M
