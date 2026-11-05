local M = {}

local ns = vim.api.nvim_create_namespace("spring_panel")
local PANEL_FT = "springboot"

---@class SpringApp
---@field name string
---@field file string
---@field module_dir string
---@field tool "maven"|"gradle"
---@field profile string?
---@field buf integer?
---@field job integer?
---@field status "stopped"|"starting"|"running"|"failed"
---@field port string?

---@type SpringApp[]
local apps = {}
local state = { buf = nil, root = nil, line_map = {}, log_win = nil }

local icons = {
  stopped = { "○", "Comment" },
  starting = { "◐", "DiagnosticWarn" },
  running = { "●", "DiagnosticOk" },
  failed = { "✗", "DiagnosticError" },
}

local help = {
  "<CR> / çift tık  başlat / durdur",
  "r  yeniden başlat   l  logları aç",
  "p  profil seç       R  yeniden tara",
  "q  paneli kapat",
}

--- Verilen proje kökünde @SpringBootApplication içeren Java/Kotlin dosyalarını bulup uygulama listesine çevirir.
--- Daha önce bulunan ve hâlâ var olan uygulamaların çalışma durumu korunur.
---@param root string
function M.scan(root)
  state.root = root
  local res = vim
    .system({
      "rg",
      "--files-with-matches",
      "--glob=*.java",
      "--glob=*.kt",
      "--glob=!**/target/**",
      "--glob=!**/build/**",
      "-e",
      "@SpringBootApplication",
      root,
    }, { text = true })
    :wait()
  local previous = {}
  for _, app in ipairs(apps) do
    previous[app.file] = app
  end
  apps = {}
  for file in vim.gsplit(res.stdout or "", "\n", { trimempty = true }) do
    local module_dir = vim.fs.root(file, { "pom.xml", "build.gradle", "build.gradle.kts" })
    if module_dir then
      table.insert(apps, previous[file] or {
        name = vim.fn.fnamemodify(file, ":t:r"),
        file = file,
        module_dir = module_dir,
        tool = vim.uv.fs_stat(module_dir .. "/pom.xml") and "maven" or "gradle",
        status = "stopped",
      })
    end
  end
  table.sort(apps, function(a, b)
    return a.name < b.name
  end)
end

--- Uygulamayı çalıştıracak komutu oluşturur. Proje içinde mvnw/gradlew varsa onu, yoksa global aracı kullanır.
---@param app SpringApp
---@return string[]
local function build_cmd(app)
  if app.tool == "maven" then
    local wrapper = vim.fs.find("mvnw", { upward = true, path = app.module_dir, type = "file" })[1]
    local cmd = { wrapper or "mvn", "spring-boot:run" }
    if app.profile then
      table.insert(cmd, "-Dspring-boot.run.profiles=" .. app.profile)
    end
    return cmd
  end
  local wrapper = vim.fs.find("gradlew", { upward = true, path = app.module_dir, type = "file" })[1]
  local cmd = { wrapper or "gradle", "bootRun" }
  if app.profile then
    table.insert(cmd, "--args=--spring.profiles.active=" .. app.profile)
  end
  return cmd
end

--- Panel buffer'ını uygulamaların güncel durumuyla yeniden çizer.
local function render()
  local buf = state.buf
  if not (buf and vim.api.nvim_buf_is_valid(buf)) then
    return
  end
  local lines = { " Spring Boot", "" }
  local marks = { { 0, 0, -1, "Title" } }
  state.line_map = {}
  if #apps == 0 then
    table.insert(lines, " @SpringBootApplication bulunamadı")
    table.insert(marks, { 2, 0, -1, "Comment" })
  end
  for _, app in ipairs(apps) do
    local icon, hl = unpack(icons[app.status])
    local meta = {}
    if app.port then
      table.insert(meta, ":" .. app.port)
    end
    if app.profile then
      table.insert(meta, "[" .. app.profile .. "]")
    end
    table.insert(meta, vim.fn.fnamemodify(app.module_dir, ":t"))
    local head = " " .. icon .. " " .. app.name
    table.insert(lines, head .. "  " .. table.concat(meta, " "))
    local row = #lines - 1
    state.line_map[row + 1] = app
    table.insert(marks, { row, 1, 1 + #icon, hl })
    table.insert(marks, { row, #head, -1, "Comment" })
  end
  table.insert(lines, "")
  for _, h in ipairs(help) do
    table.insert(lines, " " .. h)
    table.insert(marks, { #lines - 1, 0, -1, "Comment" })
  end
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  for _, m in ipairs(marks) do
    vim.api.nvim_buf_set_extmark(buf, ns, m[1], m[2], {
      end_row = m[1],
      end_col = m[3] == -1 and #lines[m[1] + 1] or m[3],
      hl_group = m[4],
    })
  end
end

--- Log satırlarından uygulamanın ayağa kalktığını ve hangi portu dinlediğini yakalar.
---@param app SpringApp
---@param data string[]
local function on_output(app, data)
  local changed = false
  for _, line in ipairs(data) do
    local port = line:match("started on port[s]?[^%d]*(%d+)")
    if port and app.port ~= port then
      app.port, changed = port, true
    end
    if app.status == "starting" and line:match("Started %S+ in [%d%.]+ seconds") then
      app.status, changed = "running", true
    end
  end
  if changed then
    vim.schedule(render)
  end
end

--- Uygulamayı kendi terminal buffer'ında başlatır. Çıktı "l" ile açılan log penceresinde izlenir.
---@param app SpringApp
function M.start(app)
  if app.job then
    return
  end
  if app.buf and vim.api.nvim_buf_is_valid(app.buf) then
    vim.api.nvim_buf_delete(app.buf, { force = true })
  end
  app.buf = vim.api.nvim_create_buf(false, true)
  app.status, app.port = "starting", nil
  local cmd = build_cmd(app)
  vim.api.nvim_buf_call(app.buf, function()
    app.job = vim.fn.jobstart(cmd, {
      term = true,
      cwd = app.module_dir,
      on_stdout = function(_, data)
        on_output(app, data)
      end,
      on_exit = function(_, code)
        app.job = nil
        app.status = (code == 0 or code == 143 or code == 130) and "stopped" or "failed"
        app.port = nil
        vim.schedule(render)
      end,
    })
  end)
  pcall(vim.api.nvim_buf_set_name, app.buf, "spring://" .. app.name)
  render()
  vim.notify(table.concat(cmd, " "), vim.log.levels.INFO, { title = "▶ " .. app.name })
end

--- Uygulamanın sürecini durdurur.
---@param app SpringApp
function M.stop(app)
  if app.job then
    vim.fn.jobstop(app.job)
  end
end

--- Çalışıyorsa durdurur, duruyorsa başlatır.
---@param app SpringApp
function M.toggle(app)
  if app.job then
    M.stop(app)
  else
    M.start(app)
  end
end

--- Uygulamayı durdurup süreç tamamen kapandıktan sonra yeniden başlatır.
---@param app SpringApp
function M.restart(app)
  if not app.job then
    return M.start(app)
  end
  local job = app.job
  M.stop(app)
  vim.defer_fn(function()
    vim.fn.jobwait({ job }, 10000)
    M.start(app)
  end, 100)
end

--- Uygulamanın log terminalini ekranın altındaki tek bir log penceresinde gösterir.
---@param app SpringApp
function M.logs(app)
  if not (app.buf and vim.api.nvim_buf_is_valid(app.buf)) then
    vim.notify(app.name .. " henüz çalıştırılmadı", vim.log.levels.WARN)
    return
  end
  if state.log_win and vim.api.nvim_win_is_valid(state.log_win) then
    vim.api.nvim_win_set_buf(state.log_win, app.buf)
  else
    local panel = M.find_panel()
    state.log_win = require("workbench").open_bottom(app.buf, { enter = false, after = panel })
    if panel then
      vim.api.nvim_win_set_width(panel, 44)
    end
  end
  vim.api.nvim_win_call(state.log_win, function()
    vim.cmd.normal({ "G", bang = true })
  end)
end

--- Profil adını sorup uygulamaya atar; uygulama çalışıyorsa yeni profille yeniden başlatır.
---@param app SpringApp
function M.set_profile(app)
  vim.ui.input({ prompt = app.name .. " profili (boş = yok): ", default = app.profile or "" }, function(input)
    if input == nil then
      return
    end
    app.profile = input ~= "" and input or nil
    render()
    if app.job then
      M.restart(app)
    end
  end)
end

--- İmlecin bulunduğu satırdaki uygulamayı döndürür.
---@return SpringApp?
local function current_app()
  return state.line_map[vim.api.nvim_win_get_cursor(0)[1]]
end

--- Panel buffer'ını oluşturur ve tuş/mouse eşlemelerini bağlar.
---@return integer
local function create_buf()
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].filetype = PANEL_FT
  vim.bo[buf].bufhidden = "hide"
  local function map(lhs, fn)
    vim.keymap.set("n", lhs, function()
      local app = current_app()
      if app then
        fn(app)
      end
    end, { buffer = buf, nowait = true })
  end
  map("<CR>", M.toggle)
  map("<2-LeftMouse>", M.toggle)
  map("r", M.restart)
  map("s", M.stop)
  map("l", M.logs)
  map("p", M.set_profile)
  vim.keymap.set("n", "R", function()
    M.scan(state.root or vim.fn.getcwd())
    render()
  end, { buffer = buf, nowait = true })
  vim.keymap.set("n", "q", function()
    M.close_panel(vim.api.nvim_get_current_win())
  end, { buffer = buf, nowait = true })
  return buf
end

--- Açık panel penceresini döndürür.
---@return integer?
function M.find_panel()
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == PANEL_FT then
      return win
    end
  end
end

--- Paneli alt şeride açar. Proje kökü değiştiyse uygulamaları yeniden tarar.
function M.open_panel()
  local root = vim.fn.getcwd()
  if root ~= state.root or #apps == 0 then
    M.scan(root)
  end
  if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
    state.buf = create_buf()
  end
  local win = require("workbench").open_bottom(state.buf, { width = 44 })
  vim.wo[win].cursorline = true
  vim.wo[win].wrap = false
  render()
  local rows = vim.tbl_keys(state.line_map)
  if #rows > 0 then
    vim.api.nvim_win_set_cursor(win, { math.min(unpack(rows)), 1 })
  end
end

--- Panel ve log penceresini kapatır; çalışan uygulamalar arka planda çalışmaya devam eder.
---@param win integer
function M.close_panel(win)
  if state.log_win and vim.api.nvim_win_is_valid(state.log_win) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
    vim.api.nvim_win_hide(state.log_win)
  end
  if vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_win_hide(win)
  end
end

--- Tüm uygulamaları gösteren bir seçici açar (panel açmadan hızlı başlat/durdur için).
function M.pick()
  M.scan(vim.fn.getcwd())
  vim.ui.select(apps, {
    prompt = "Spring Boot",
    format_item = function(app)
      return icons[app.status][1] .. " " .. app.name
    end,
  }, function(app)
    if app then
      M.toggle(app)
    end
  end)
end

return M
