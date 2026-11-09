--- Debug paneli: Run panelinin içinde, logların sağında Variables bölümü (`debugui.variables`). Winbar'da dap-ui
--- kontrol çubuğu (devam/duraklat, adımlar, durdur). Ayrı bir şerit aracı değildir: açılırken Run paneli kapalıysa
--- önce o açılır (Git History gibi diğer şerit araçları gizlenir), kapanınca Run paneli yerinde kalır.
--- dap-ui'nin kendi `open()`'ı kullanılmaz: sol/alt split'leri `arrange_layout` ile çekişip sağ sütunu titretiyordu.
local M = {}

local MIN_WIDTH = 40

--- Variables penceresi (alt şeritte, bölümün buffer'ını gösteren).
---@return integer?
function M.find_panel()
  local buf = require("debugui.variables").buffer()
  for _, win in ipairs(vim.fn.win_findbuf(buf)) do
    if vim.api.nvim_win_get_config(win).relative == "" and vim.w[win].workbench_bottom then
      return win
    end
  end
end

--- Variables bölümünü Run panelinde logların sağına açar (Run paneli kapalıysa önce onu açar). Odak değişmez.
function M.open_panel()
  if M.find_panel() then
    return
  end
  local cur = vim.api.nvim_get_current_win()
  local workbench = require("workbench")
  local runner = require("runner")
  if not runner.find_panel() then
    workbench.open_runner({ debug = false })
  end
  local strip = workbench.bottom_wins()
  local total = 0
  for _, w in ipairs(strip) do
    total = total + vim.api.nvim_win_get_width(w) + 1
  end
  local width = math.max(MIN_WIDTH, math.floor(total * 0.3))
  local win = workbench.open_bottom(require("debugui.variables").buffer(), { enter = false, width = width })
  vim.wo[win].winbar = "%!v:lua.require'debugui'.winbar()"
  vim.wo[win].wrap = false
  vim.wo[win].winfixwidth = true
  require("debugui.variables").refresh()
  workbench.arrange_layout()
  if vim.api.nvim_win_is_valid(cur) then
    vim.api.nvim_set_current_win(cur)
  end
end

--- Variables winbar'ı: dap-ui kontrol çubuğu + başlık. dap-ui'nin `enable_controls`'ı pencereyi yalnız ilk açılışta
--- bulduğu için ifade olarak çizilir.
---@return string
function M.winbar()
  local ok, controls = pcall(function()
    return require("dapui.controls").controls(vim.g.statusline_winid == vim.api.nvim_get_current_win())
  end)
  return (ok and controls or "") .. "  %#Title#Variables %#Comment#· e set · ⏎ expand · q hide"
end

--- Yalnız Variables bölümünü gizler; Run paneli ve loglar kalır.
---@param _? integer
function M.close_panel(_)
  local win = M.find_panel()
  if win and #vim.api.nvim_tabpage_list_wins(0) > 1 then
    vim.api.nvim_win_hide(win)
    local panel = package.loaded["runner"] and package.loaded["runner"].find_panel()
    if panel and #require("workbench").bottom_wins() > 1 then
      vim.api.nvim_win_set_width(panel, package.loaded["runner"].PANEL_WIDTH)
    end
  end
end

--- dap dinleyicileri: kesme noktasında / istisnada durunca Variables (kapalıysa) Run panelinde açılır, odak kodda
--- kalır; adım atmak (`step`) açmaz. Son oturum bitince Variables gizlenir.
function M.setup()
  local dap = require("dap")
  require("debugui.variables").setup()
  dap.listeners.after.event_stopped["debugui"] = function(_, body)
    local reason = body and body.reason
    if reason == "breakpoint" or reason == "exception" then
      vim.schedule(M.open_panel)
    end
  end
  for _, event in ipairs({ "event_stopped", "event_continued", "event_terminated", "event_exited" }) do
    dap.listeners.after[event]["debugui_controls"] = function()
      vim.schedule(function()
        vim.cmd("redrawstatus!")
      end)
    end
  end
  dap.listeners.on_session["debugui"] = function(_, new)
    if new == nil then
      vim.schedule(function()
        if vim.tbl_isempty(dap.sessions()) then
          require("debugui.variables").refresh()
          M.close_panel()
        end
      end)
    end
  end
end

return M
