--- LESS ilk açılış ekranı. Veri klasöründe LazyVim yokken (`config/lazy.lua` karar verir) çağrılır; kurulu
--- sistemde hiç yüklenmez. Akış:
---   1. `start()`: ortada "Setting up LESS" penceresi; lazy.nvim indirilirken `status()` ile durum yazılır.
---   2. lazy'nin kurulum ekranı (`lazy.view.show("install")`) kurulum süresince bastırılır, yerine eklenti sayacı
---      (n/N) çizilir.
---   3. `finish()`: pencere kapanır; hata varsa lazy ekranı + "restart to retry" bildirimi, yoksa VeryLazy'de
---      `tools()` Mason'ı (LSP sunucuları dahil) yükleyip kurulumları tek, yerinde güncellenen bildirimle izler.
--- UI yoksa (headless) pencere çizilmez.
local M = {}

local WIDTH = 46
local BAR = 30
local state = {}

--- Bildirim: snacks notifier varsa `id` ile yerinde günceller, yoksa vim.notify.
---@param msg string
---@param level? integer
---@param opts? table
local function notify(msg, level, opts)
  opts = vim.tbl_extend("force", { id = "less_firstrun", title = "LESS" }, opts or {})
  if _G.Snacks and Snacks.notifier then
    Snacks.notifier.notify(msg, level or vim.log.levels.INFO, opts)
  else
    vim.notify(msg, level or vim.log.levels.INFO, { title = "LESS" })
  end
end

--- Ortadaki kurulum penceresini `text` (+ verilirse 0..1 ilerleme çubuğu) ile çizer; yoksa açar.
---@param text string
---@param ratio? number
function M.status(text, ratio)
  if #vim.api.nvim_list_uis() == 0 then
    return
  end
  local lines = { "", "Setting up LESS", "", text }
  if ratio then
    local done = math.floor(ratio * BAR + 0.5)
    lines[#lines + 1] = string.rep("━", done) .. string.rep("─", BAR - done)
  end
  vim.list_extend(lines, { "", "This happens only once.", "" })
  for i, l in ipairs(lines) do
    lines[i] = string.rep(" ", math.floor((WIDTH - vim.fn.strdisplaywidth(l)) / 2)) .. l
  end
  if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
    state.buf = vim.api.nvim_create_buf(false, true)
  end
  vim.api.nvim_buf_set_lines(state.buf, 0, -1, false, lines)
  local cfg = {
    relative = "editor",
    width = WIDTH,
    height = #lines,
    row = math.max(0, math.floor((vim.o.lines - #lines) / 2) - 1),
    col = math.max(0, math.floor((vim.o.columns - WIDTH) / 2)),
    style = "minimal",
    border = "rounded",
    focusable = false,
    zindex = 250,
  }
  if state.win and vim.api.nvim_win_is_valid(state.win) then
    vim.api.nvim_win_set_config(state.win, cfg)
  else
    state.win = vim.api.nvim_open_win(state.buf, false, cfg)
  end
  vim.cmd.redraw()
end

--- Kurulum penceresini kapatır.
local function close()
  if state.win and vim.api.nvim_win_is_valid(state.win) then
    vim.api.nvim_win_close(state.win, true)
  end
  state.win = nil
end

--- lazy'nin kurulum görevlerinden eklenti sayacını hesaplayıp pencereye yazar (lazy'nin kendi sayımıyla aynı mantık).
local function progress()
  if not state.timer then
    return -- finish() sonrası kuyrukta kalmış çağrı pencereyi yeniden açmasın
  end
  local total, done = 0, 0
  for _, p in pairs(require("lazy.core.config").plugins) do
    if p._.tasks and #p._.tasks > 0 then
      total = total + 1
      local running = vim.iter(p._.tasks):any(function(t)
        return t:running()
      end)
      done = done + (running and 0 or 1)
    end
  end
  if total > 0 then
    M.status(("Installing plugins  %d/%d"):format(done, total), done / total)
  end
end

--- nvim-treesitter'ın parser kurulum ilerlemesini ("[nvim-treesitter/install/bash]: Downloading…") ilk açılış süresince
--- susturur: `info` mesajları ekrana basılmadan yalnız `:TSLog` kaydına (`debug`) yazılır, uyarı/hatalar görünür kalır.
--- Yoksa noice bu satırları uzun mesaj olarak bölmede açıyordu. Modül yüklenirken (`package.preload`) yamalanır ve
--- bu oturum boyunca öyle kalır (parser indirmeleri dil araçları bildirimi bittikten sonra da sürebilir).
local function quiet_treesitter()
  local name = "nvim-treesitter.log"
  package.preload[name] = function()
    package.preload[name] = nil
    local log
    for i = 2, #package.loaders do
      local loader = package.loaders[i](name)
      if type(loader) == "function" then
        log = loader(name)
        break
      end
    end
    if type(log) == "table" and log.Logger then
      log.Logger.info = log.Logger.debug
    end
    return log
  end
end

--- İlk açılışı başlatır: pencereyi açar, lazy kurulumu sırasında lazy ekranı yerine sayacı gösterir.
function M.start()
  M.status("Getting ready…")
  quiet_treesitter()
  vim.api.nvim_create_autocmd("User", {
    pattern = "LazyInstallPre",
    callback = function()
      local view = require("lazy.view")
      if not state.show then
        state.show = view.show
        view.show = function(mode)
          if mode ~= "install" then
            return state.show(mode)
          end
        end
      end
      if not state.timer then
        state.timer = assert(vim.uv.new_timer())
        state.timer:start(0, 100, vim.schedule_wrap(progress))
      end
    end,
  })
end

--- lazy.nvim indirilemedi: pencereyi kapatıp tekrar deneme ipucuyla hatayı gösterir (çağıran çıkar).
---@param out string git çıktısı
function M.bootstrap_failed(out)
  close()
  vim.api.nvim_echo({
    { "LESS could not download its plugin manager (lazy.nvim).\n", "ErrorMsg" },
    { out, "WarningMsg" },
    { "\nCheck your network connection and start LESS again to retry. Press any key to exit..." },
  }, true, {})
end

--- `lazy.setup()` bittikten sonra: sayaç durur, lazy ekranı geri gelir. Build adımlarının açtığı terminal pencereleri
--- (ör. markdown-preview `install.sh`) gizlenir, iş arka planda sürer; yoksa dashboard ("tek pencere" koşulu) açılmaz.
--- Hata varsa ayrıntı + tekrar deneme ipucu, yoksa VeryLazy'de dil araçları kurulumu.
function M.finish()
  if state.timer then
    state.timer:stop()
    state.timer:close()
    state.timer = nil
  end
  if state.show then
    require("lazy.view").show = state.show
  end
  close()
  for _, w in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    local b = vim.api.nvim_win_get_buf(w)
    if vim.api.nvim_win_get_config(w).relative == "" and vim.bo[b].buftype == "terminal" then
      vim.api.nvim_win_hide(w)
    end
  end
  local Plugin = require("lazy.core.plugin")
  local failed = vim.tbl_filter(Plugin.has_errors, vim.tbl_values(require("lazy.core.config").plugins))
  if #failed > 0 then
    require("lazy.view").show("install")
    vim.schedule(function()
      notify(
        ("Could not install %d plugin(s). Check your network connection and restart LESS to retry."):format(#failed),
        vim.log.levels.ERROR,
        { timeout = false }
      )
    end)
    return
  end
  vim.api.nvim_create_autocmd("User", { pattern = "VeryLazy", once = true, callback = M.tools })
end

--- Mason'ı, registry hazır olunca da nvim-lspconfig'i yükler (LSP sunucularının `ensure_installed`'ı onun config'inde;
--- registry yokken yüklenirse sunucu listesi boş kalır). Mason kurulumlarını sayıp "Installing language tools (n/N)…"
--- bildirimini günceller, bitince "LESS is ready" (ya da başarısızları) yazar. 3 sn bekleyen kurulum yoksa biter.
function M.tools()
  if #vim.api.nvim_list_uis() == 0 then
    return
  end
  local ok = pcall(require("lazy").load, { plugins = { "mason.nvim" } })
  local has_mr, mr = pcall(require, "mason-registry")
  if not (ok and has_mr) then
    return notify("LESS is ready.")
  end
  local pending, done, failed = {}, 0, {}
  mr:on("package:install:handle", function(handle)
    pending[handle.package.name] = true
  end)
  mr:on("package:install:success", function(pkg)
    pending[pkg.name], done = nil, done + 1
  end)
  mr:on("package:install:failed", function(pkg)
    pending[pkg.name] = nil
    failed[#failed + 1] = pkg.name
  end)
  notify("Installing language tools…", nil, { timeout = false })
  mr.refresh(vim.schedule_wrap(function()
    -- ilk kurulumda sunucu başına "installing X" / "X was successfully installed" yerine tek ilerleme bildirimi
    package.loaded["mason-lspconfig.notify"] = function(msg, level)
      if level and level > vim.log.levels.INFO then
        vim.notify(msg, level, { title = "mason-lspconfig.nvim" })
      end
    end
    pcall(require("lazy").load, { plugins = { "nvim-lspconfig" } })
    for _, pkg in ipairs(mr.get_all_packages()) do
      if pkg:is_installing() then
        pending[pkg.name] = true
      end
    end
    local idle = 0
    local timer = assert(vim.uv.new_timer())
    timer:start(1000, 1000, vim.schedule_wrap(function()
      local open = vim.tbl_count(pending)
      local total = done + #failed + open
      if open > 0 then
        idle = 0
        return notify(("Installing language tools (%d/%d)…"):format(done + #failed, total), nil, { timeout = false })
      end
      idle = idle + 1
      if idle < 3 then
        return
      end
      timer:stop()
      timer:close()
      if #failed > 0 then
        notify(
          ("%d language tool(s) failed: %s. Run :Mason to retry."):format(#failed, table.concat(failed, ", ")),
          vim.log.levels.WARN,
          { timeout = false }
        )
      else
        notify(total > 0 and ("Language tools installed (%d). LESS is ready."):format(total) or "LESS is ready.")
      end
    end))
  end))
end

return M
