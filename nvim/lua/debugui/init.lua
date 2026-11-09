--- Debug paneli: nvim-dap-ui elemanlarını kendi split düzeni yerine workbench alt şeridinde gösterir.
--- Soldan sağa Variables (scopes, %40) | Frames (stacks, %25) | REPL; REPL'in winbar'ında dap-ui kontrol çubuğu.
--- Run / Git History gibi şeritte tek araçtır. dap-ui'nin kendi `open()`'ı kullanılmaz: sol/alt split'leri
--- `arrange_layout` ile sürekli çekişip sağ sütunu titretiyordu.
local M = {}

local PANES = {
  { element = "scopes", title = "%#Title# Variables", share = 0.4 },
  { element = "stacks", title = "%#Title# Frames", share = 0.25 },
  { element = "repl", title = "%!v:lua.require'debugui'.controls()" },
}

--- Paneli oluşturan elemanın penceresini döndürür (alt şeritte, dap-ui buffer'ını gösteren).
---@param element string
---@return integer?
local function win_of(element)
  local ok, dapui = pcall(require, "dapui")
  if not ok then
    return nil
  end
  local buf = dapui.elements[element].buffer()
  for _, win in ipairs(vim.fn.win_findbuf(buf)) do
    if vim.api.nvim_win_get_config(win).relative == "" and vim.w[win].workbench_bottom then
      return win
    end
  end
end

--- Panel açıksa odaktaki ya da ilk panel penceresini döndürür.
---@return integer?
function M.find_panel()
  local cur = vim.api.nvim_get_current_win()
  local first
  for _, pane in ipairs(PANES) do
    local win = win_of(pane.element)
    if win == cur then
      return cur
    end
    first = first or win
  end
  return first
end

--- Paneli alt şeride açar (şeritteki diğer araçları `workbench.debug` gizler). Elemanlar hemen çizilir; sonrası
--- dap-ui'nin kendi render döngüsüyle (oturum olayları) güncellenir.
function M.open_panel()
  local dapui = require("dapui")
  local workbench = require("workbench")
  local prev, first
  for _, pane in ipairs(PANES) do
    local elem = dapui.elements[pane.element]
    local buf = elem.buffer()
    local win = win_of(pane.element)
    if not win then
      win = workbench.open_bottom(buf, { enter = false, after = prev })
    end
    vim.wo[win].winbar = pane.title
    vim.wo[win].wrap = false
    pcall(elem.render)
    prev, first = win, first or win
  end
  local total = 0
  for _, pane in ipairs(PANES) do
    total = total + vim.api.nvim_win_get_width(win_of(pane.element)) + 1
  end
  for _, pane in ipairs(PANES) do
    if pane.share then
      vim.api.nvim_win_set_width(win_of(pane.element), math.floor(total * pane.share))
    end
  end
  workbench.arrange_layout()
  vim.api.nvim_set_current_win(first)
end

--- REPL penceresinin winbar'ı: dap-ui kontrol çubuğu (devam/duraklat, adımlar, yeniden başlat, durdur).
--- dap-ui'nin `enable_controls`'ı pencereyi yalnız ilk açılışta bulduğu için ifade olarak çizilir.
---@return string
function M.controls()
  return require("dapui.controls").controls(vim.g.statusline_winid == vim.api.nvim_get_current_win())
end

--- Paneli kapatır (tüm pencereleri). `win` verilmezse açık olanlar bulunur.
---@param _? integer
function M.close_panel(_)
  for _, pane in ipairs(PANES) do
    local win = win_of(pane.element)
    if win and #vim.api.nvim_tabpage_list_wins(0) > 1 then
      vim.api.nvim_win_hide(win)
    end
  end
end

--- dap dinleyicileri: kesme noktasında / istisnada durunca panel (kapalıysa) açılır, odak kodda kalır; adım
--- atmak (`step`) açmaz. Son oturum bitince panel kapanır.
function M.setup()
  local dap = require("dap")
  dap.listeners.after.event_stopped["debugui"] = function(_, body)
    local reason = body and body.reason
    if (reason == "breakpoint" or reason == "exception") and not M.find_panel() then
      vim.schedule(function()
        local cur = vim.api.nvim_get_current_win()
        require("workbench").debug()
        if vim.api.nvim_win_is_valid(cur) then
          vim.api.nvim_set_current_win(cur)
        end
      end)
    end
  end
  for _, event in ipairs({ "event_stopped", "event_continued", "event_terminated", "event_exited" }) do
    dap.listeners.after[event]["debugui_controls"] = function()
      vim.schedule(function()
        vim.cmd("redrawstatus!")
      end)
    end
  end
  for _, event in ipairs({ "event_terminated", "event_exited" }) do
    dap.listeners.before[event]["debugui"] = function()
      vim.schedule(function()
        if vim.tbl_isempty(dap.sessions()) then
          M.close_panel()
        end
      end)
    end
  end
end

return M
