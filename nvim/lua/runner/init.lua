local M = {}

---@class RunnerDebug
---@field port integer? boş port aramasının başlangıcı; verilirse başlatmadan önce `item.debug_port` atanır
---@field ready string debug hedefinin dinlemeye başladığını gösteren Lua pattern; 1. yakalama = port
---@field env (fun(item: RunnerItem): table<string, string>?)? debug modunda sürece eklenecek ortam değişkenleri
---@field attach (fun(item: RunnerItem, port: integer, run: fun(config: table)))? DAP attach; yoksa sadece port açılır

---@class RunnerItem
---@field id string
---@field label string
---@field detail string?
---@field icon string
---@field icon_hl string
---@field cwd string
---@field cmd fun(item: RunnerItem): string[] debug modunda `item.mode == "debug"` ve `item.debug_port` dolu gelir
---@field ready string? Lua pattern; eşleşene kadar durum "starting" kalır
---@field profiles boolean? profil seçimi destekleniyor mu (Spring)
---@field debug RunnerDebug? debug desteği (yoksa Debug eylemi kapalı)
---@field profile string?
---@field config RunnerConfig? kalıcı çalıştırma ayarları (`M.config`)
---@field vm_env string? VM seçeneklerinin verildiği ortam değişkeni (Node: NODE_OPTIONS); yoksa sağlayıcı cmd'de kullanır
---@field buf integer?
---@field job integer?
---@field status "stopped"|"starting"|"running"|"done"|"failed"
---@field mode "run"|"debug"? son başlatma modu (Rerun bunu kullanır)
---@field port string?
---@field debug_port integer?
---@field debug_targets integer[]? çıktıda yakalanan debug portları (node: süreç başına bir tane)
---@field dap_state "waiting"|"attaching"|nil
---@field stopping boolean?
---@field after_exit fun()?
---@field partial string?

---@class RunnerConfig
---@field profile string?
---@field env table<string, string>?
---@field vm_options string?
---@field args string?

---@class RunnerNode
---@field id string
---@field label string
---@field detail string?
---@field icon string
---@field icon_hl string
---@field children (RunnerNode|RunnerItem)[]

local rdap = require("runner.dap")
local load_configs

local ns = vim.api.nvim_create_namespace("runner")
local FT = "runner"
local PANEL_WIDTH = 56

local providers = { "runner.providers.spring", "runner.providers.node" }

local state = {
  buf = nil,
  root = nil,
  ---@type string[]?
  roots = nil,
  ---@type RunnerNode[]
  tree = {},
  ---@type table<string, RunnerItem>
  items = {},
  ---@type table<string, boolean>
  collapsed = {},
  line_map = {},
  log_win = nil,
  ---@type integer? buton alanının metin sütunu (0 tabanlı), panel görünür değilse nil
  button_col = nil,
}

local status_icons = {
  stopped = { "\u{F10C}", "Comment" },
  starting = { "\u{F110}", "DiagnosticWarn" },
  running = { "\u{F111}", "DiagnosticOk" },
  done = { "\u{F00C}", "DiagnosticOk" },
  failed = { "\u{F00D}", "DiagnosticError" },
}

local ICON = {
  run = "\u{F04B}",
  stop = "\u{F04D}",
  debug = "\u{F188}",
  rerun = "\u{F01E}",
  log = "\u{F15C}",
  profile = "\u{F013}",
  attached = "\u{F0C1}",
  detached = "\u{F127}",
  spinner = "\u{F110}",
}

local CHEVRON_OPEN, CHEVRON_CLOSED = "\u{F47C}", "\u{F460}"

--- Buton başına ekran sütunu: " X " (ikon ortada, iki yanı tıklanabilir boşluk).
local BUTTON_WIDTH = 3
local BUTTON_COUNT = 4

--- Panelde kullanılan renk gruplarını tanımlar (renk şeması değişince yeniden çağrılır).
local function setup_highlights()
  local function set(name, opts)
    opts.default = true
    vim.api.nvim_set_hl(0, name, opts)
  end
  set("RunnerSpring", { fg = "#6DB33F" })
  set("RunnerNode", { fg = "#8CC84B" })
  set("RunnerPackage", { link = "Directory" })
  set("RunnerScript", { link = "Function" })
  set("RunnerGroup", { link = "Title" })
  set("RunnerDetail", { link = "Comment" })
  set("RunnerPort", { link = "DiagnosticInfo" })
  set("RunnerProfile", { link = "Special" })
  set("RunnerDebug", { fg = "#FF9E64" })
  set("RunnerDebugAttached", { link = "DiagnosticOk" })
  set("RunnerButtonRun", { link = "DiagnosticOk" })
  set("RunnerButtonStop", { link = "DiagnosticError" })
  set("RunnerButtonDebug", { link = "RunnerDebug" })
  set("RunnerButtonRerun", { link = "DiagnosticInfo" })
  set("RunnerButtonLog", { link = "Comment" })
end

setup_highlights()
vim.api.nvim_create_autocmd("ColorScheme", { callback = setup_highlights })

---@param node RunnerNode|RunnerItem
---@return boolean
local function is_group(node)
  return node.children ~= nil
end

--- Öğe debug modunda çalışıyor mu.
---@param item RunnerItem
---@return boolean
local function debugging(item)
  return item.job ~= nil and item.mode == "debug"
end

--- Öğenin durum ikonu ve rengi; debug modunda çalışan öğe böcek ikonu ve turuncu renkle ayrışır.
---@param item RunnerItem
---@return string icon, string hl
local function status_icon(item)
  if debugging(item) then
    return item.status == "starting" and ICON.spinner or ICON.debug, "RunnerDebug"
  end
  return unpack(status_icons[item.status])
end

--- Debug modunda çalışan öğe için durum rozeti: JDWP portu ve debugger bağlı / bekleniyor / bağlı değil.
---@param item RunnerItem
---@return string text, string hl
local function debug_badge(item)
  if not debugging(item) then
    return "", "RunnerDetail"
  end
  local port = item.debug_port and ("jdwp :" .. item.debug_port .. " · ") or ""
  local sessions = #rdap.sessions(item)
  if sessions > 0 then
    local count = sessions > 1 and (" ×" .. sessions) or ""
    return ("  %s %sattached%s"):format(ICON.attached, port, count), "RunnerDebugAttached"
  elseif item.dap_state == "waiting" then
    return ("  %s %swaiting for jdtls…"):format(ICON.spinner, port), "DiagnosticWarn"
  elseif item.dap_state == "attaching" then
    return ("  %s %sattaching…"):format(ICON.spinner, port), "DiagnosticWarn"
  end
  return ("  %s %sdetached"):format(ICON.detached, port), "RunnerDetail"
end

--- Düğümün altındaki toplam, çalışan ve debug modunda çalışan öğe sayısını döndürür.
---@param node RunnerNode
---@return integer total, integer running, integer debug
local function count(node)
  local total, running, debug = 0, 0, 0
  for _, child in ipairs(node.children) do
    if is_group(child) then
      local t, r, d = count(child)
      total, running, debug = total + t, running + r, debug + d
    else
      total = total + 1
      if child.job then
        running = running + 1
      end
      if debugging(child) then
        debug = debug + 1
      end
    end
  end
  return total, running, debug
end

---@class RunnerButton
---@field icon string
---@field hl string
---@field name string
---@field fn fun(item: RunnerItem)

--- Öğe satırının sonundaki butonlar; yerleri sabittir, içerikleri duruma göre değişir
--- (çalışırken ▶ yerine ■). Debug butonu: başlat / çalışanı debug modunda yeniden başlat / debugger'ı yeniden bağla.
---@param item RunnerItem
---@return RunnerButton[]
local function buttons(item)
  return {
    item.job and { icon = ICON.stop, hl = "RunnerButtonStop", name = "stop", fn = M.stop }
      or { icon = ICON.run, hl = "RunnerButtonRun", name = "run", fn = M.start },
    { icon = ICON.debug, hl = item.debug and "RunnerButtonDebug" or "RunnerDetail", name = "debug", fn = M.debug },
    { icon = ICON.rerun, hl = "RunnerButtonRerun", name = "rerun", fn = M.rerun },
    { icon = ICON.log, hl = "RunnerButtonLog", name = "log", fn = M.logs },
  }
end

--- Paneli ağacın güncel haliyle yeniden çizer. Kapalı (collapsed) düğümlerin altı çizilmez.
--- Panel penceresi görünürse öğe satırlarının sağ kenarına buton alanı (virt_text, sabit pencere sütunu) çizilir.
local function render()
  local buf = state.buf
  if not (buf and vim.api.nvim_buf_is_valid(buf)) then
    return
  end
  local lines, marks, rows = {}, {}, {}
  state.line_map = {}

  local function add(parts, entry)
    local text, col, row = "", 0, #lines
    for _, p in ipairs(parts) do
      if p[1] ~= "" then
        if p[2] then
          table.insert(marks, { row, col, col + #p[1], p[2] })
        end
        text, col = text .. p[1], col + #p[1]
      end
    end
    table.insert(lines, text)
    if entry then
      state.line_map[#lines] = entry
    end
  end

  add({
    { " " .. ICON.run .. " Run", "Title" },
    {
      "   ⏎ run/stop · d debug · r rerun · s stop · l log · e config · p profile · m menu · ␣ fold · R rescan · q close",
      "RunnerDetail",
    },
  })

  local function walk(node, depth, parent)
    local indent = string.rep("  ", depth + 1)
    if is_group(node) then
      local open = not state.collapsed[node.id]
      local total, running, debug = count(node)
      add({
        { indent },
        { (open and CHEVRON_OPEN or CHEVRON_CLOSED) .. " ", "RunnerDetail" },
        { node.icon .. " ", node.icon_hl },
        { node.label, depth == 0 and "RunnerGroup" or "RunnerPackage" },
        { "  " .. total, "RunnerDetail" },
        { running > 0 and ("  " .. status_icons.running[1] .. " " .. running) or "", "DiagnosticOk" },
        { debug > 0 and ("  " .. ICON.debug .. " " .. debug) or "", "RunnerDebug" },
        { node.detail and ("  " .. node.detail) or "", "RunnerDetail" },
      }, { node = node, parent = parent })
      if open then
        for _, child in ipairs(node.children) do
          walk(child, depth + 1, node)
        end
      end
    else
      local icon, hl = status_icon(node)
      local badge, badge_hl = debug_badge(node)
      add({
        { indent .. "  " },
        { icon .. " ", hl },
        { node.icon .. " ", node.icon_hl },
        { node.label, "Normal" },
        { node.port and ("  :" .. node.port) or "", "RunnerPort" },
        { node.profile and ("  [" .. node.profile .. "]") or "", "RunnerProfile" },
        { M.has_config(node) and ("  " .. ICON.profile) or "", "RunnerProfile" },
        { badge, badge_hl },
        { node.detail and ("  " .. node.detail) or "", "RunnerDetail" },
      }, { node = node, parent = parent, buttons = buttons(node) })
      table.insert(rows, #lines)
    end
  end

  if #state.tree == 0 then
    add({ { "   Nothing runnable found (Spring Boot / package.json)", "RunnerDetail" } })
  end
  for _, group in ipairs(state.tree) do
    walk(group, 0, nil)
  end

  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  for _, m in ipairs(marks) do
    vim.api.nvim_buf_set_extmark(buf, ns, m[1], m[2], { end_row = m[1], end_col = m[3], hl_group = m[4] })
  end

  local win = M.find_panel()
  local info = win and vim.fn.getwininfo(win)[1]
  local col = info and (info.width - info.textoff - BUTTON_WIDTH * BUTTON_COUNT - 1)
  state.button_col = col and col >= 24 and col or nil
  if state.button_col then
    for _, lnum in ipairs(rows) do
      local chunks = {}
      for _, b in ipairs(state.line_map[lnum].buttons) do
        vim.list_extend(chunks, { { " " }, { b.icon, b.hl }, { " " } })
      end
      vim.api.nvim_buf_set_extmark(buf, ns, lnum - 1, 0, {
        virt_text = chunks,
        virt_text_win_col = state.button_col,
        hl_mode = "combine",
      })
    end
  end
end

--- Paneli bir sonraki döngüde yeniden çizer (DAP dinleyicileri ve job callback'leri için).
function M.refresh()
  vim.schedule(render)
end

--- Öğeyi id ile döndürür.
---@param id string
---@return RunnerItem?
function M.item(id)
  return state.items[id]
end

local function configs_file()
  return vim.env.NVIM_RUN_CONFIGS_FILE or (vim.fn.stdpath("data") .. "/run_configs.json")
end

---@type table<string, RunnerConfig>?
local configs

---@return table<string, RunnerConfig>
function load_configs()
  if not configs then
    local ok, data = pcall(function()
      return vim.json.decode(table.concat(vim.fn.readfile(configs_file()), "\n"))
    end)
    configs = ok and type(data) == "table" and data or {}
  end
  return configs
end

--- Öğenin kalıcı çalıştırma ayarları (kopya). Kayıt `stdpath("data")/run_configs.json`, öğe id'sine göre
--- (testte `NVIM_RUN_CONFIGS_FILE`).
---@param item RunnerItem
---@return RunnerConfig
function M.config(item)
  return vim.deepcopy(item.config or load_configs()[item.id] or {})
end

--- Ayarları kaydeder (boş değerler silinir) ve paneli yeniler; çalışan süreç kendiliğinden yeniden başlamaz.
---@param item RunnerItem
---@param cfg RunnerConfig
function M.set_config(item, cfg)
  local clean = {}
  for k, v in pairs(cfg) do
    if type(v) == "string" then
      v = vim.trim(v)
    end
    if v ~= "" and not (type(v) == "table" and next(v) == nil) then
      clean[k] = v
    end
  end
  if not item.profiles then
    clean.profile = nil
  end
  local all = load_configs()
  all[item.id] = next(clean) and clean or nil
  vim.fn.mkdir(vim.fs.dirname(configs_file()), "p")
  vim.fn.writefile({ vim.json.encode(all) }, configs_file())
  item.config, item.profile = clean, clean.profile
  render()
end

--- Profil dışında ayar (ortam, VM seçenekleri, argüman) var mı (panelde ⚙).
---@param item RunnerItem
---@return boolean
function M.has_config(item)
  local c = item.config or {}
  return c.env ~= nil or c.vm_options ~= nil or c.args ~= nil
end

--- Tüm öğeler (proje kökü değiştiyse ya da hiç taranmadıysa önce taranır, `open_panel` gibi).
---@return RunnerItem[]
function M.items()
  local roots = M.roots()
  if table.concat(roots, "\n") ~= state.root then
    M.scan(roots)
    render()
  end
  local list = vim.tbl_values(state.items)
  table.sort(list, function(a, b)
    return a.id < b.id
  end)
  return list
end

--- Taranacak kök klasörler: workspace açıksa tüm kökleri, değilse çalışma klasörü.
---@return string[]
function M.roots()
  local ok, ws = pcall(require, "workspaces")
  return ok and ws.roots() or { vim.fn.getcwd() }
end

--- Sağlayıcıyı her kökte çalıştırıp sonuçları tek grupta birleştirir.
--- Birden fazla kök varsa her alt düğümün açıklamasına kök klasörün adı eklenir.
---@param mod string
---@param roots string[]
---@return RunnerNode?
local function discover(mod, roots)
  local merged
  for _, root in ipairs(roots) do
    local ok, group = pcall(require(mod).discover, root)
    if not ok then
      vim.notify(mod .. ": " .. tostring(group), vim.log.levels.WARN)
    elseif group and #group.children > 0 then
      if #roots > 1 then
        for _, child in ipairs(group.children) do
          child.detail = vim.fs.basename(root) .. (child.detail and (" › " .. child.detail) or "")
        end
      end
      if merged then
        vim.list_extend(merged.children, group.children)
      else
        merged = group
      end
    end
  end
  return merged
end

--- Tüm sağlayıcıları çalıştırıp ağacı yeniden kurar. Önceden bilinen öğelerin çalışma durumu (job, log, profil,
--- mod, debug) korunur.
---@param roots string|string[] tek kök ya da (workspace için) kök listesi
function M.scan(roots)
  roots = type(roots) == "table" and roots or { roots }
  state.root = table.concat(roots, "\n")
  state.roots = roots
  state.tree = {}
  local seen = {}
  local function adopt(node)
    for i, child in ipairs(node.children) do
      if is_group(child) then
        adopt(child)
      else
        local prev = state.items[child.id]
        if prev then
          for k, v in pairs(child) do
            prev[k] = v
          end
          node.children[i] = prev
        else
          child.status = "stopped"
          child.config = vim.deepcopy(load_configs()[child.id] or {})
          child.profile = child.config.profile
          state.items[child.id] = child
        end
        seen[child.id] = true
      end
    end
  end
  for _, mod in ipairs(providers) do
    local group = discover(mod, roots)
    if group then
      adopt(group)
      table.insert(state.tree, group)
    end
  end
  for id, item in pairs(state.items) do
    if not seen[id] and not item.job then
      state.items[id] = nil
    end
  end
end

--- Öğenin log terminalini alt şeritte, panelin sağındaki tek log penceresinde gösterir.
---@param item RunnerItem
function M.logs(item)
  if not (item.buf and vim.api.nvim_buf_is_valid(item.buf)) then
    vim.notify(item.label .. " has not been run yet", vim.log.levels.WARN, { title = "Run" })
    return
  end
  if state.log_win and vim.api.nvim_win_is_valid(state.log_win) then
    vim.api.nvim_win_set_buf(state.log_win, item.buf)
  else
    local panel = M.find_panel()
    state.log_win = require("workbench").open_bottom(item.buf, { enter = false, after = panel })
    if panel then
      vim.api.nvim_win_set_width(panel, PANEL_WIDTH)
      render()
    end
  end
  vim.api.nvim_win_call(state.log_win, function()
    vim.cmd.normal({ "G", bang = true })
  end)
end

--- Debug portuna (hedefe) bağlanmayı sağlayıcıya bırakır; sağlayıcı attach desteklemiyorsa portu bildirir.
---@param item RunnerItem
---@param port integer
local function attach(item, port)
  if item.debug.attach then
    item.debug.attach(item, port, function(config)
      rdap.run(item, config)
    end)
  else
    vim.notify(
      ("%s: debug port %d is open; attach a debugger manually"):format(item.label, port),
      vim.log.levels.INFO,
      { title = "Run" }
    )
  end
end

--- Tam bir çıktı satırını işler: debug hedefi (port) → attach, hazır olma işareti, uygulama portu.
---@param item RunnerItem
---@param line string
---@return boolean changed
local function on_line(item, line)
  local changed = false
  local target = debugging(item) and item.debug.ready and line:match(item.debug.ready)
  if target then
    local port = tonumber(target)
    if not vim.tbl_contains(item.debug_targets, port) then
      table.insert(item.debug_targets, port)
      vim.schedule(function()
        if item.job then
          attach(item, port)
        end
      end)
    end
    return false
  end
  local port = not line:find("ws://", 1, true)
    and (
      line:match("started on port[s]?[^%d]*(%d+)")
      or line:match("localhost:(%d+)")
      or line:match("127%.0%.0%.1:(%d+)")
    )
  if port and item.port ~= port then
    item.port, changed = port, true
  end
  if item.status == "starting" and item.ready and line:match(item.ready) then
    item.status, changed = "running", true
  end
  return changed
end

--- Terminal çıktısını satırlara böler (parça sınırında bölünen satır bir sonraki parçayla birleştirilir) ve işler.
---@param item RunnerItem
---@param data string[]
local function on_output(item, data)
  data[1] = (item.partial or "") .. data[1]
  item.partial = table.remove(data)
  if #item.partial > 4096 then
    table.insert(data, item.partial)
    item.partial = ""
  end
  local changed = false
  for _, line in ipairs(data) do
    changed = on_line(item, (line:gsub("\r$", ""))) or changed
  end
  if changed then
    vim.schedule(render)
  end
end

--- Panelin başka öğelerine ayrılmış debug portları.
---@param except RunnerItem
---@return table<integer, boolean>
local function taken_ports(except)
  local taken = {}
  for _, other in pairs(state.items) do
    if other ~= except and other.job and other.debug_port then
      taken[other.debug_port] = true
    end
  end
  return taken
end

--- Öğeyi kendi terminal buffer'ında başlatır ve logunu gösterir. `mode` "debug" ise sağlayıcının debug ayarlarıyla
--- (boş JDWP portu / inspector ortamı) başlatılır; debug hedefi çıktıda görününce debugger otomatik bağlanır.
--- Log penceresi eski çalıştırmanın buffer'ını gösteriyorsa pencere kapanmasın diye önce yeni buffer'a geçirilir.
---@param item RunnerItem
---@param mode "run"|"debug"?
function M.start(item, mode)
  if item.job then
    return
  end
  mode = mode or "run"
  if mode == "debug" and not item.debug then
    vim.notify("Debugging is not supported for " .. item.label, vim.log.levels.WARN, { title = "Run" })
    return
  end
  local debug_port
  if mode == "debug" and item.debug.port then
    debug_port = rdap.free_port(item.debug.port, taken_ports(item))
    if not debug_port then
      vim.notify("No free debug port found for " .. item.label, vim.log.levels.ERROR, { title = "Run" })
      return
    end
  end
  local old = item.buf
  item.buf = vim.api.nvim_create_buf(false, true)
  vim.b[item.buf].workbench_bottom = true
  if old and vim.api.nvim_buf_is_valid(old) then
    if
      state.log_win
      and vim.api.nvim_win_is_valid(state.log_win)
      and vim.api.nvim_win_get_buf(state.log_win) == old
    then
      vim.api.nvim_win_set_buf(state.log_win, item.buf)
    end
    vim.api.nvim_buf_delete(old, { force = true })
  end
  item.status, item.port, item.stopping = item.ready and "starting" or "running", nil, false
  item.mode, item.debug_port, item.debug_targets, item.dap_state, item.partial = mode, debug_port, {}, nil, nil
  local cmd = item.cmd(item)
  local cfg = item.config or {}
  local env = vim.deepcopy(cfg.env or {})
  if item.vm_env and (cfg.vm_options or "") ~= "" then
    env[item.vm_env] = cfg.vm_options
  end
  if mode == "debug" and item.debug.env then
    env = vim.tbl_extend("force", env, item.debug.env(item))
  end
  env = next(env) and env or nil
  vim.api.nvim_buf_call(item.buf, function()
    item.job = vim.fn.jobstart(cmd, {
      term = true,
      cwd = item.cwd,
      env = env,
      on_stdout = function(_, data)
        on_output(item, data)
      end,
      on_exit = function(_, code)
        item.job = nil
        item.status = item.stopping and "stopped" or (code == 0 and "done" or "failed")
        item.dap_state = nil
        local after = item.after_exit
        item.after_exit = nil
        vim.schedule(function()
          rdap.detach(item)
          render()
          if after then
            after()
          end
        end)
      end,
    })
  end)
  if item.job <= 0 then
    item.job, item.status = nil, "failed"
    vim.notify(
      "Could not start " .. item.label .. ": " .. table.concat(cmd, " "),
      vim.log.levels.ERROR,
      { title = "Run" }
    )
  end
  pcall(vim.api.nvim_buf_set_name, item.buf, "run://" .. item.label .. "#" .. item.buf)
  render()
  M.logs(item)
end

--- Süreci durdurur (bekleyen yeniden başlatmaya dokunmaz).
---@param item RunnerItem
local function kill(item)
  if item.job then
    item.stopping = true
    vim.fn.jobstop(item.job)
  end
end

--- Öğenin sürecini durdurur; bekleyen bir yeniden başlatma varsa iptal eder.
---@param item RunnerItem
function M.stop(item)
  item.after_exit = nil
  kill(item)
end

--- Çalışıyorsa durdurur, duruyorsa son modla değil her zaman normal (run) modda başlatır.
---@param item RunnerItem
function M.toggle(item)
  if item.job then
    M.stop(item)
  else
    M.start(item, "run")
  end
end

--- Yeniden çalıştırır: çalışıyorsa durdurur ve süreç kapanınca aynı modla (ya da verilen `mode` ile) başlatır;
--- durmuş/bitmiş/hatalı öğeyi son modla başlatır (hiç çalışmadıysa run). Job'ın ana süreci (paket yöneticisi,
--- Maven) alt süreçlerinden önce kapanabildiği için yeni süreç, eski uygulamanın portu boşalınca (en çok 10 sn) başlar.
---@param item RunnerItem
---@param mode "run"|"debug"?
function M.rerun(item, mode)
  mode = mode or item.mode or "run"
  if not item.job then
    return M.start(item, mode)
  end
  local port = tonumber(item.port)
  item.after_exit = function()
    rdap.wait(function()
      return not (port and rdap.listening_ports()[port]) or nil
    end, function()
      M.start(item, mode)
    end, function()
      M.start(item, mode)
    end, 10000, 250)
  end
  kill(item)
end

--- Geriye uyumluluk: `restart` = `rerun`.
M.restart = M.rerun

--- Debug eylemi: duruyorsa debug modunda başlatır; normal modda çalışıyorsa debug modunda yeniden başlatır;
--- debug modunda çalışıp debugger bağlı değilse (kapatıldıysa / jdtls hazır değildi) hâlâ dinleyen hedeflere
--- yeniden bağlanır.
---@param item RunnerItem
function M.debug(item)
  if not item.debug then
    vim.notify("Debugging is not supported for " .. item.label, vim.log.levels.WARN, { title = "Run" })
  elseif not item.job then
    M.start(item, "debug")
  elseif item.mode ~= "debug" then
    M.rerun(item, "debug")
  elseif #rdap.sessions(item) > 0 then
    vim.notify("Debugger is already attached to " .. item.label, vim.log.levels.INFO, { title = "Run" })
  elseif item.dap_state then
    vim.notify("Debugger is attaching to " .. item.label, vim.log.levels.INFO, { title = "Run" })
  else
    local listening = rdap.listening_ports()
    item.debug_targets = vim.tbl_filter(function(port)
      return listening[port] == true
    end, item.debug_targets or {})
    if #item.debug_targets == 0 then
      vim.notify("No debug target is listening for " .. item.label, vim.log.levels.WARN, { title = "Run" })
    end
    for _, port in ipairs(item.debug_targets) do
      attach(item, port)
    end
  end
end

--- Profil adını sorup öğeye atar; çalışıyorsa yeni profille (aynı modda) yeniden başlatır.
---@param item RunnerItem
function M.set_profile(item)
  if not item.profiles then
    return
  end
  vim.ui.input({ prompt = item.label .. " profile (empty = none): ", default = item.profile or "" }, function(input)
    if input == nil then
      return
    end
    M.set_config(item, vim.tbl_extend("force", M.config(item), { profile = input ~= "" and input or nil }))
    if item.job then
      M.rerun(item)
    end
  end)
end

--- `KEY=value; KEY2=value` → tablo (formdaki ortam alanı).
---@param s string
---@return table<string, string>?, string? err
local function parse_env(s)
  local env = {}
  for part in vim.gsplit(s, ";", { plain = true, trimempty = true }) do
    part = vim.trim(part)
    if part ~= "" then
      local k, v = part:match("^([%a_][%w_]*)%s*=%s*(.*)$")
      if not k then
        return nil, "Environment must look like KEY=value; KEY2=value (bad: " .. part .. ")"
      end
      env[k] = v
    end
  end
  return env
end

---@param env table<string, string>?
---@return string
local function format_env(env)
  local keys = vim.tbl_keys(env or {})
  table.sort(keys)
  return table.concat(vim.tbl_map(function(k)
    return k .. "=" .. env[k]
  end, keys), "; ")
end

--- Çalıştırma ayarları formu (IntelliJ Run Configuration gibi): profil (Spring), ortam, VM seçenekleri, argümanlar.
--- Çalışan öğe kaydedilince yeni ayarlarla yeniden başlar.
---@param item RunnerItem
function M.edit_config(item)
  local cfg = M.config(item)
  local fields = {}
  if item.profiles then
    fields[#fields + 1] = { key = "profile", label = "Profile", value = cfg.profile, hint = "dev,local" }
  end
  vim.list_extend(fields, {
    { key = "env", label = "Environment", value = format_env(cfg.env), hint = "KEY=value; KEY2=value" },
    { key = "vm_options", label = item.vm_env and "Node options" or "VM options", value = cfg.vm_options,
      hint = item.vm_env and "--max-old-space-size=4096" or "-Xmx1g -Dfoo=bar" },
    { key = "args", label = "Program arguments", value = cfg.args, hint = "--server.port=8081" },
  })
  require("datasource.form").open({
    title = item.label .. " · Run configuration",
    fields = fields,
    submit = function(v)
      local env, err = parse_env(v.env)
      if not env then
        return err
      end
      M.set_config(item, { profile = v.profile, env = env, vm_options = v.vm_options, args = v.args })
      if item.job then
        M.rerun(item)
      end
    end,
  })
end

--- Öğenin o anki durumuna uyan eylemler (sağ tık menüsü ve `Space j` seçicisi için).
---@param item RunnerItem
---@return { label: string, fn: fun(item: RunnerItem) }[]
local function actions(item)
  local list = {}
  local function add(icon, label, fn)
    table.insert(list, { label = icon .. "  " .. label, fn = fn })
  end
  if item.job then
    add(ICON.stop, "Stop", M.stop)
    add(ICON.rerun, item.mode == "debug" and "Rerun (debug)" or "Rerun", M.rerun)
    if item.debug and item.mode ~= "debug" then
      add(ICON.debug, "Restart in debug mode", M.debug)
    elseif debugging(item) and #rdap.sessions(item) == 0 and not item.dap_state then
      add(ICON.debug, "Attach debugger", M.debug)
    end
  else
    add(ICON.run, "Run", M.toggle)
    if item.debug then
      add(ICON.debug, "Debug", M.debug)
    end
    if item.mode then
      add(ICON.rerun, "Rerun (" .. item.mode .. ")", M.rerun)
    end
  end
  if item.buf and vim.api.nvim_buf_is_valid(item.buf) then
    add(ICON.log, "Show log", M.logs)
  end
  add(ICON.profile, "Edit configuration…", M.edit_config)
  if item.profiles then
    add(ICON.profile, "Set profile…" .. (item.profile and (" [" .. item.profile .. "]") or ""), M.set_profile)
  end
  return list
end

--- Öğe için eylem menüsünü (vim.ui.select) açar.
---@param item RunnerItem
function M.menu(item)
  vim.ui.select(actions(item), {
    prompt = item.label,
    format_item = function(a)
      return a.label
    end,
  }, function(choice)
    if choice then
      choice.fn(item)
    end
  end)
end

--- İmlecin bulunduğu satırdaki ağaç girdisini ({ node, parent, buttons? }) döndürür.
---@return { node: RunnerNode|RunnerItem, parent: RunnerNode?, buttons: RunnerButton[]? }?
local function current()
  return state.line_map[vim.api.nvim_win_get_cursor(0)[1]]
end

--- Son mouse olayının konumundaki buton (panel penceresinde, öğe satırında, buton sütunlarında ise).
---@return { button: RunnerButton, item: RunnerItem }?
local function button_at_mouse()
  local pos = vim.fn.getmousepos()
  if not state.button_col or pos.winid == 0 or vim.api.nvim_win_get_buf(pos.winid) ~= state.buf then
    return nil
  end
  local entry = state.line_map[pos.line]
  if not (entry and entry.buttons) then
    return nil
  end
  local textcol = pos.wincol - 1 - vim.fn.getwininfo(pos.winid)[1].textoff
  local offset = textcol - state.button_col
  local index = offset >= 0 and math.floor(offset / BUTTON_WIDTH) + 1 or 0
  local button = entry.buttons[index]
  return button and { button = button, item = entry.node } or nil
end

--- İmleci verilen düğümün satırına taşır.
---@param node table
local function goto_node(node)
  for lnum, entry in pairs(state.line_map) do
    if entry.node == node then
      vim.api.nvim_win_set_cursor(0, { lnum, 0 })
      return
    end
  end
end

--- Grup düğümünü açar/kapatır; `open` verilirse o duruma getirir. İmleç aynı düğümde kalır.
---@param node RunnerNode
---@param open boolean?
local function set_open(node, open)
  if open == nil then
    open = state.collapsed[node.id] == true
  end
  state.collapsed[node.id] = not open or nil
  render()
  goto_node(node)
end

--- Sağ tık → öğe eylem menüsü; panellerin ortak global `<RightMouse>` eşlemesine (`workbench.on_right_click`)
--- kaydolur. Panel dışındaki tıklamalarda varsayılan `<RightMouse>` (PopUp menüsü) aynen çalışır.
local function map_right_click()
  require("workbench").on_right_click(FT, function(pos)
    if not (state.buf and pos.winid ~= 0 and vim.api.nvim_win_get_buf(pos.winid) == state.buf) then
      return false
    end
    local entry = state.line_map[pos.line]
    if entry and not is_group(entry.node) then
      vim.schedule(function()
        M.menu(entry.node)
      end)
    end
    return true
  end)
end

--- Panel buffer'ını oluşturur ve tuş/mouse eşlemelerini bağlar.
--- Butonlar `<LeftRelease>` ile tetiklenir: panel odakta değilken yapılan tıklamada `<LeftMouse>` önce paneli
--- odaklar, bırakma olayı ise artık panel buffer'ının eşlemesine düşer (tek tıkla çalışır).
---@return integer
local function create_buf()
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].filetype = FT
  vim.bo[buf].bufhidden = "hide"
  local function map(lhs, fn)
    vim.keymap.set("n", lhs, function()
      local entry = current()
      if entry then
        fn(entry.node, entry.parent)
      end
    end, { buffer = buf, nowait = true })
  end
  local function on_item(fn)
    return function(node)
      if not is_group(node) then
        fn(node)
      end
    end
  end
  local function activate(node)
    if is_group(node) then
      set_open(node)
    else
      M.toggle(node)
    end
  end
  map("<Space>", function(node)
    if is_group(node) then
      set_open(node)
    end
  end)
  map("<CR>", activate)
  map("<2-LeftMouse>", function(node)
    if not button_at_mouse() then
      activate(node)
    end
  end)
  vim.keymap.set("n", "<LeftRelease>", function()
    local hit = button_at_mouse()
    if hit then
      vim.schedule(function()
        hit.button.fn(hit.item)
      end)
    end
    return "<LeftRelease>"
  end, { buffer = buf, expr = true })
  map_right_click()
  map("<Right>", function(node)
    if is_group(node) then
      set_open(node, true)
    end
  end)
  map("<Left>", function(node, parent)
    if is_group(node) and not state.collapsed[node.id] then
      set_open(node, false)
    elseif parent then
      goto_node(parent)
    end
  end)
  map("d", on_item(M.debug))
  map("r", on_item(M.rerun))
  map("s", on_item(M.stop))
  map("l", on_item(M.logs))
  map("p", on_item(M.set_profile))
  map("e", on_item(M.edit_config))
  map("m", on_item(M.menu))
  vim.keymap.set("n", "R", function()
    M.scan(state.roots or M.roots())
    render()
  end, { buffer = buf, nowait = true })
  vim.keymap.set("n", "q", function()
    M.close_panel(vim.api.nvim_get_current_win())
  end, { buffer = buf, nowait = true })
  return buf
end

--- Panel penceresinin boyu değişince buton sütununu yeniden hesaplamak için yeniden çizer.
vim.api.nvim_create_autocmd("WinResized", {
  group = vim.api.nvim_create_augroup("runner_resize", { clear = true }),
  callback = function()
    local panel = M.find_panel()
    if panel and vim.tbl_contains(vim.v.event.windows or {}, panel) then
      render()
    end
  end,
})

--- Açık panel penceresini döndürür.
---@return integer?
function M.find_panel()
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == FT then
      return win
    end
  end
end

--- Paneli alt şeride açar. Proje kökü (workspace'te kök listesi) değiştiyse veya ağaç boşsa yeniden tarar.
function M.open_panel()
  local roots = M.roots()
  if table.concat(roots, "\n") ~= state.root or #state.tree == 0 then
    M.scan(roots)
  end
  if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
    state.buf = create_buf()
  end
  local win = require("workbench").open_bottom(state.buf, { width = PANEL_WIDTH })
  vim.wo[win].cursorline = true
  vim.wo[win].wrap = false
  render()
  if vim.api.nvim_buf_line_count(state.buf) > 1 then
    vim.api.nvim_win_set_cursor(win, { 2, 0 })
  end
end

--- Panel ve log penceresini kapatır; çalışan süreçler arka planda devam eder.
---@param win integer
function M.close_panel(win)
  if state.log_win and vim.api.nvim_win_is_valid(state.log_win) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
    vim.api.nvim_win_hide(state.log_win)
  end
  if vim.api.nvim_win_is_valid(win) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
    vim.api.nvim_win_hide(win)
  end
end

--- Panel açmadan önce öğe, sonra eylem seçtirir (run/stop, debug, rerun, log, profil).
function M.pick()
  M.scan(M.roots())
  local flat = {}
  local function collect(node, path)
    for _, child in ipairs(node.children) do
      if is_group(child) then
        collect(child, path .. child.label .. " › ")
      else
        table.insert(flat, { item = child, path = path })
      end
    end
  end
  for _, group in ipairs(state.tree) do
    collect(group, group.label .. " › ")
  end
  if #flat == 0 then
    vim.notify("Nothing runnable found (Spring Boot / package.json)", vim.log.levels.INFO, { title = "Run" })
    return
  end
  vim.ui.select(flat, {
    prompt = "Run",
    format_item = function(e)
      return status_icon(e.item) .. " " .. e.path .. e.item.label
    end,
  }, function(e)
    if e then
      M.menu(e.item)
    end
  end)
end

return M
