local M = {}

local hooked = false

--- Run panelini yeniden çizdirir (runner modülü yüklüyse).
local function refresh()
  local runner = package.loaded["runner"]
  if runner then
    runner.refresh()
  end
end

--- Oturumun kök (en üst) oturumunu döndürür; js-debug hedefleri kök oturumun çocuğu olarak açılır.
---@param session table
---@return table
local function root_of(session)
  while session.parent do
    session = session.parent
  end
  return session
end

--- nvim-dap dinleyicilerini bir kez bağlar: oturum açılınca/kapanınca öğenin "bağlanıyor" durumu temizlenir ve
--- panel yeniden çizilir (bağlı/bağlı değil göstergesi `M.sessions()` ile çizim sırasında hesaplanır).
---@param dap table
local function hook(dap)
  if hooked then
    return
  end
  hooked = true
  local function on_event(session)
    local root = session and root_of(session)
    local id = root and root.config and root.config.runnerId
    local item = id and package.loaded["runner"] and package.loaded["runner"].item(id)
    if item and item.dap_state == "attaching" then
      item.dap_state = nil
    end
    vim.schedule(refresh)
  end
  for _, event in ipairs({ "event_initialized", "event_terminated", "event_exited", "disconnect" }) do
    dap.listeners.after[event]["runner"] = on_event
  end
  dap.listeners.on_session["runner"] = function()
    vim.schedule(refresh)
  end
end

--- Sistemde TCP dinleyen portları (lsof) küme olarak döndürür.
---@return table<integer, boolean>
function M.listening_ports()
  local res = vim.system({ "lsof", "-nP", "-iTCP", "-sTCP:LISTEN", "-Fn" }, { text = true }):wait()
  local ports = {}
  for port in ((res.stdout or "") .. "\n"):gmatch(":(%d+)\n") do
    ports[tonumber(port)] = true
  end
  return ports
end

--- `base`'den başlayarak boş bir TCP portu bulur: lsof'ta dinlenmeyen, `taken` içinde olmayan (panelin başka
--- öğelerine ayrılmış) ve 127.0.0.1'e bağlanabilen ilk port.
---@param base integer
---@param taken table<integer, boolean>
---@return integer?
function M.free_port(base, taken)
  local busy = M.listening_ports()
  for port = base, base + 500 do
    if not busy[port] and not taken[port] then
      local tcp = vim.uv.new_tcp()
      local ok = tcp and tcp:bind("127.0.0.1", port)
      if tcp then
        tcp:close()
      end
      if ok then
        return port
      end
    end
  end
end

--- Öğeye ait açık kök DAP oturumlarını döndürür. nvim-dap hiç yüklenmediyse boş liste (çizim dap'i yüklemez).
---@param item RunnerItem
---@return table[]
function M.sessions(item)
  local dap = package.loaded["dap"]
  local out = {}
  if dap then
    for _, session in pairs(dap.sessions()) do
      if not session.closed and session.config and session.config.runnerId == item.id then
        table.insert(out, session)
      end
    end
  end
  return out
end

--- Sağlayıcının hazırladığı attach yapılandırmasını öğeyle etiketleyip (`runnerId`) yeni bir DAP oturumu açar.
--- Bağlantı 20 sn içinde kurulamazsa "bağlanıyor" durumu bırakılır (nvim-dap hatayı kendisi bildirir).
---@param item RunnerItem
---@param config table
function M.run(item, config)
  local dap = require("dap")
  hook(dap)
  config.runnerId = item.id
  item.dap_state = "attaching"
  refresh()
  dap.run(config, { new = true })
  vim.defer_fn(function()
    if item.dap_state == "attaching" then
      item.dap_state = nil
      refresh()
    end
  end, 20000)
end

--- Öğenin hâlâ açık DAP oturumlarını debuggee'yi öldürmeden kapatır; disconnect isteği hata verirse oturum
--- doğrudan kapatılır.
---@param item RunnerItem
function M.detach(item)
  for _, session in ipairs(M.sessions(item)) do
    if not pcall(session.disconnect, session, { terminateDebuggee = false }) then
      pcall(session.close, session)
    end
  end
end

--- `cond` doğru olana kadar `interval` ms aralıkla bekler; `timeout` ms içinde olmazsa `on_timeout` çağrılır.
--- `cond` nil yerine `false` döndürürse bekleme sessizce iptal edilir.
---@param cond fun(): boolean?
---@param on_ok fun()
---@param on_timeout fun()
---@param timeout integer
---@param interval integer?
function M.wait(cond, on_ok, on_timeout, timeout, interval)
  local deadline = vim.uv.now() + timeout
  local function poll()
    local res = cond()
    if res then
      return on_ok()
    elseif res == false then
      return
    elseif vim.uv.now() > deadline then
      return on_timeout()
    end
    vim.defer_fn(poll, interval or 500)
  end
  poll()
end

return M
