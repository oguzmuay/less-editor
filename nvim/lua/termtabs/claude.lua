local M = {}

local new_requested = false

---@return table
local function tt()
  return require("termtabs")
end

--- Komut, yapılandırılmış temel Claude komutundan fazlasını içeriyor mu (ör. `:ClaudeCode --resume`)?
--- Öyleyse mevcut oturuma geçmek yerine bu argümanlarla yeni bir oturum sekmesi açılır.
---@param cmd string
---@return boolean
local function has_args(cmd)
  local ok, term = pcall(require, "claudecode.terminal")
  local base = ok and term.defaults.terminal_cmd or "claude"
  return vim.trim(cmd) ~= vim.trim(base or "claude")
end

--- Yeni Claude Code oturumunu Claude panelinde yeni bir sekme olarak başlatır.
---@param cmd string claudecode'un oluşturduğu komut
---@param env table<string, string> IDE entegrasyonu ortam değişkenleri (CLAUDE_CODE_SSE_PORT vb.)
---@param config table claudecode terminal yapılandırması (cwd, split_width_percentage)
---@param focus? boolean
local function create(cmd, env, config, focus)
  local argv = require("claudecode.utils").parse_command(cmd)
  local buf = tt().spawn("claude", argv, {
    cwd = config.cwd,
    env = env,
    enter = focus ~= false,
    width = math.floor(vim.o.columns * (config.split_width_percentage or 0.38)),
    on_exit = function(code)
      if code ~= 0 and code ~= 129 and code ~= 143 then
        vim.notify(("Claude Code exited with code %d"):format(code), vim.log.levels.WARN)
      end
    end,
  })
  if buf then
    vim.keymap.set("t", "<S-CR>", function()
      vim.api.nvim_feedkeys("\\", "t", true)
      vim.defer_fn(function()
        vim.api.nvim_feedkeys("\r", "t", true)
      end, 10)
    end, { buffer = buf, desc = "New line" })
  end
end

--- Mevcut oturum varsa paneli gösterir, yoksa (ya da yeni oturum istendiyse) yeni oturum açar.
local function open(cmd, env, config, focus)
  focus = focus ~= false
  if new_requested or has_args(cmd) or #tt().tabs("claude") == 0 then
    new_requested = false
    create(cmd, env, config, focus)
    return
  end
  tt().show("claude", focus)
end

---@type table<string, integer> claudecode istemci id'si -> bağlanan Claude sürecinin pid'i
local client_pids = {}

--- WebSocket istemcisinin (Claude CLI) pid'ini bulur: TCP bağlantısının karşı ucundaki portu `lsof` ile sürece
--- eşler (Neovim'in kendisi hariç). Sonuç istemci id'sine göre önbelleklenir.
---@param client table claudecode.server.client
---@return integer?
function M.client_pid(client)
  if client_pids[client.id] then
    return client_pids[client.id]
  end
  local ok, peer = pcall(client.tcp_handle.getpeername, client.tcp_handle)
  if not ok or type(peer) ~= "table" or not peer.port then
    return
  end
  local host = peer.ip:find(":") and ("[" .. peer.ip .. "]") or peer.ip
  local res =
    vim.system({ "lsof", "-nP", ("-iTCP@%s:%d"):format(host, peer.port), "-sTCP:ESTABLISHED", "-Fp" }):wait(2000)
  for pid in (res.stdout or ""):gmatch("p(%d+)") do
    pid = tonumber(pid)
    if pid ~= vim.fn.getpid() then
      client_pids[client.id] = pid
      return pid
    end
  end
end

--- `pid` süreci ya da onun bir üst süreci `ancestor` mı?
---@param pid integer
---@param ancestor integer
---@return boolean
local function descends(pid, ancestor)
  for _ = 1, 5 do
    if pid == ancestor then
      return true
    end
    local proc = vim.api.nvim_get_proc(pid)
    if not (proc and proc.ppid) or proc.ppid <= 1 then
      return false
    end
    pid = proc.ppid
  end
  return false
end

--- claudecode IDE sunucusu `at_mentioned` (ClaudeCodeSend / ClaudeCodeAdd / sağ tık "Claude'a gönder") mesajını
--- bağlı tüm Claude oturumlarına yayınlar. Birden fazla oturum bağlıyken bunu yalnızca aktif sekmedeki oturuma
--- gönderecek şekilde sarar; eşleşme bulunamazsa eski davranışa (herkese yayın) döner.
local function route_mentions()
  local ok, server = pcall(require, "claudecode.server.init")
  if not ok or server._termtabs_routed then
    return
  end
  server._termtabs_routed = true
  local broadcast = server.broadcast
  server.broadcast = function(method, params)
    local tcp = server.state.server
    local active_buf = tt().active("claude")
    local active = vim.iter(tt().tabs("claude")):find(function(tab)
      return tab.buf == active_buf
    end)
    if method ~= "at_mentioned" or not (tcp and active and active.pid) or vim.tbl_count(tcp.clients or {}) < 2 then
      return broadcast(method, params)
    end
    for _, client in pairs(tcp.clients) do
      local pid = client.handshake_complete and M.client_pid(client)
      if pid and descends(pid, active.pid) then
        local msg = vim.json.encode({ jsonrpc = "2.0", method = method, params = params or vim.empty_dict() })
        require("claudecode.server.client").send_message(client, msg)
        return true
      end
    end
    return broadcast(method, params)
  end
end

--- claudecode.nvim için özel terminal sağlayıcısı. Snacks sağlayıcısının penceresi `fixbuf` ile başka buffer
--- göstermeye izin vermediği için sekmeler (aynı pencerede buffer değiştirme) özel sağlayıcıyla yapılır.
--- `get_active_bufnr` son seçilen oturumu döndürür: diff, gönderme ve odak işlemleri o oturuma gider.
---@type ClaudeCodeTerminalProvider
M.provider = {
  setup = route_mentions,
  is_available = function()
    return true
  end,
  get_active_bufnr = function()
    return tt().active("claude")
  end,
  open = open,
  close = function()
    tt().hide("claude")
  end,
  simple_toggle = function(cmd, env, config)
    if not has_args(cmd) and tt().win("claude") then
      tt().hide("claude")
    else
      open(cmd, env, config, true)
    end
  end,
  focus_toggle = function(cmd, env, config)
    local win = tt().win("claude")
    if has_args(cmd) or not win then
      open(cmd, env, config, true)
    elseif win == vim.api.nvim_get_current_win() then
      tt().hide("claude")
    else
      tt().show("claude", true)
    end
  end,
}

--- Yeni bir Claude Code oturumu sekmesi açar. Komut ve IDE ortam değişkenlerini claudecode üretsin diye
--- `claudecode.terminal.open()` üzerinden gider; sağlayıcı bu bayrağı görünce yeni oturum başlatır.
function M.new_session()
  local ok, term = pcall(require, "claudecode.terminal")
  if not ok then
    vim.notify("Failed to load claudecode.nvim", vim.log.levels.ERROR)
    return
  end
  new_requested = true
  term.open()
  new_requested = false
end

return M
