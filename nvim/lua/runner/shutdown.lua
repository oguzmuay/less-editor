--- Çıkışta Run panelinin çalışan süreçlerini düzgün kapatır. `:qa`/`:wqa`/`:xa`, Cmd+Q ve `Space q q` buradan geçer:
--- çalışan süreç yoksa komut aynen çalışır. Varsa her birine Ctrl+C (SIGINT; Spring kapanış kancaları, npm/node
--- düzgün kapanır) gönderilir ve ortada süreç listesi açılır: ↑↓ gez, `k` seçileni zorla öldür (süreç grubuna
--- SIGKILL), `K` hepsini, `q`/Esc çıkışı iptal et (durdurma sinyali geri alınmaz). Hepsi kapanınca çıkılır.
local M = {}

local ns = vim.api.nvim_create_namespace("runner_shutdown")

---@class ShutdownState
---@field items RunnerItem[]
---@field started table<RunnerItem, number>
---@field killed table<RunnerItem, boolean>
---@field cmd string
---@field buf integer
---@field win integer
---@field timer uv.uv_timer_t

---@type ShutdownState?
local active

--- Süreci (ve aynı süreç grubundaki alt süreçlerini: mvn → JVM, npm → node) SIGKILL ile öldürür. Terminal job'ları
--- kendi oturumunun lideri olduğu için grup no = pid.
---@param item RunnerItem
function M.kill(item)
  if not item.job then
    return
  end
  local ok, pid = pcall(vim.fn.jobpid, item.job)
  if ok and pid and pid > 0 then
    pcall(vim.uv.kill, -pid, "sigkill")
    pcall(vim.uv.kill, pid, "sigkill")
  end
  item.stopping = true
  pcall(vim.fn.jobstop, item.job)
end

--- Kaydedilmemiş dosya varsa ve komut zaten yazmıyor/zorlamıyorsa önce sorar (süreçler durdurulduktan sonra
--- çıkış iptal olmasın diye). Dönen komut çalıştırılacak son çıkış komutudur; nil = iptal.
---@param cmd string
---@return string?
local function resolve_unsaved(cmd)
  local ok, parsed = pcall(vim.api.nvim_parse_cmd, cmd, {})
  if not ok or parsed.bang or parsed.cmd == "wqall" or parsed.cmd == "xall" then
    return cmd
  end
  local modified = vim.tbl_filter(function(b)
    return b.listed == 1 and vim.bo[b.bufnr].buftype == ""
  end, vim.fn.getbufinfo({ bufmodified = 1 }))
  if #modified == 0 then
    return cmd
  end
  local choice = vim.fn.confirm(
    ("%d file(s) have unsaved changes."):format(#modified),
    "&Save all and quit\n&Discard and quit\n&Cancel",
    3
  )
  if choice == 1 then
    return "wqall"
  elseif choice == 2 then
    return "qall!"
  end
end

--- Son çıkış komutunu çalıştırır; hata (ör. yazılamayan dosya) mesaj olarak gösterilir.
---@param cmd string
local function finish(cmd)
  local ok, err = pcall(vim.cmd, cmd)
  if not ok then
    vim.api.nvim_echo({ { (tostring(err):gsub("^.-Vim%(%a+%):", "")), "ErrorMsg" } }, true, {})
  end
end

--- Pencereyi ve zamanlayıcıyı kapatır.
local function close()
  if not active then
    return
  end
  local s = active
  active = nil
  s.timer:stop()
  s.timer:close()
  if vim.api.nvim_win_is_valid(s.win) then
    vim.api.nvim_win_close(s.win, true)
  end
  if vim.api.nvim_buf_is_valid(s.buf) then
    vim.api.nvim_buf_delete(s.buf, { force = true })
  end
end

--- Listeyi yeniden çizer; hepsi kapandıysa pencereyi kapatıp çıkar.
local function render()
  local s = active
  if not s then
    return
  end
  local pending = vim.tbl_filter(function(item)
    return item.job ~= nil
  end, s.items)
  if #pending == 0 then
    local cmd = s.cmd
    close()
    finish(cmd)
    return
  end
  local lines, marks = { "" }, {}
  local width = 0
  for _, item in ipairs(s.items) do
    width = math.max(width, vim.api.nvim_strwidth(item.label))
  end
  for i, item in ipairs(s.items) do
    local state, hl
    if not item.job then
      state, hl = "✓ stopped", "DiagnosticOk"
    elseif s.killed[item] then
      state, hl = "✗ killing…", "DiagnosticError"
    else
      state = ("◌ stopping… %ds"):format(math.floor(vim.uv.now() / 1000 - s.started[item]))
      hl = "DiagnosticWarn"
    end
    local label = item.label .. string.rep(" ", width - vim.api.nvim_strwidth(item.label))
    lines[#lines + 1] = "  " .. label .. "   " .. state
    marks[#marks + 1] = { i, #label + 5, hl }
  end
  lines[#lines + 1] = ""
  lines[#lines + 1] = "  ↑↓ move · k kill · K kill all · q cancel quit"
  vim.bo[s.buf].modifiable = true
  vim.api.nvim_buf_set_lines(s.buf, 0, -1, false, lines)
  vim.bo[s.buf].modifiable = false
  vim.api.nvim_buf_clear_namespace(s.buf, ns, 0, -1)
  for _, m in ipairs(marks) do
    vim.api.nvim_buf_set_extmark(s.buf, ns, m[1], m[2], { end_col = #lines[m[1] + 1], hl_group = m[3] })
  end
  vim.api.nvim_buf_set_extmark(s.buf, ns, #lines - 1, 0, { end_col = #lines[#lines], hl_group = "Comment" })
  local w = math.max(48, vim.fn.max(vim.tbl_map(vim.api.nvim_strwidth, lines)) + 2)
  vim.api.nvim_win_set_config(s.win, {
    relative = "editor",
    width = w,
    height = #lines,
    row = math.floor((vim.o.lines - #lines) / 2) - 1,
    col = math.floor((vim.o.columns - w) / 2),
  })
  local row = vim.api.nvim_win_get_cursor(s.win)[1]
  if row < 2 or row > #s.items + 1 then
    vim.api.nvim_win_set_cursor(s.win, { 2, 0 })
  end
end

--- İmlecin üstündeki öğe.
---@return RunnerItem?
local function current()
  local s = active
  return s and s.items[vim.api.nvim_win_get_cursor(s.win)[1] - 1]
end

--- Çıkış komutu yerine geçer. Run'da çalışan süreç yoksa komutu çalıştırır; varsa durdurup listeyi gösterir.
---@param cmd string ör. "qall", "confirm qall", "wqall"
function M.quit(cmd)
  local runner = package.loaded["runner"]
  local items = runner and runner.running() or {}
  if #items == 0 then
    return finish(cmd)
  end
  if active then
    vim.api.nvim_set_current_win(active.win)
    return
  end
  vim.cmd.stopinsert()
  local final = resolve_unsaved(cmd)
  if not final then
    return
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].bufhidden = "wipe"
  vim.bo[buf].filetype = "runner_shutdown"
  local win = vim.api.nvim_open_win(buf, true, {
    relative = "editor",
    width = 48,
    height = 3,
    row = 1,
    col = 1,
    border = "rounded",
    title = " Stopping running processes ",
    title_pos = "center",
    zindex = 200,
  })
  vim.wo[win].wrap = false
  require("workbench").list_cursor(buf)
  local started = {}
  for _, item in ipairs(items) do
    started[item] = vim.uv.now() / 1000
    item.stopping = true
    pcall(vim.api.nvim_chan_send, item.job, "\3")
  end
  active = { items = items, started = started, killed = {}, cmd = final, buf = buf, win = win, timer = vim.uv.new_timer() }
  local function map(lhs, fn)
    vim.keymap.set("n", lhs, fn, { buffer = buf, nowait = true })
  end
  map("k", function()
    local item = current()
    if item and item.job then
      active.killed[item] = true
      M.kill(item)
      render()
    end
  end)
  map("K", function()
    for _, item in ipairs(active.items) do
      if item.job then
        active.killed[item] = true
        M.kill(item)
      end
    end
    render()
  end)
  for _, lhs in ipairs({ "q", "<Esc>" }) do
    map(lhs, function()
      close()
      vim.notify("Quit cancelled. Processes that were asked to stop keep shutting down.", vim.log.levels.WARN)
    end)
  end
  vim.api.nvim_create_autocmd("WinLeave", {
    buffer = buf,
    callback = function()
      vim.schedule(function()
        if active and active.buf == buf and vim.api.nvim_win_is_valid(active.win) then
          vim.api.nvim_set_current_win(active.win)
        end
      end)
    end,
  })
  render()
  active.timer:start(250, 250, vim.schedule_wrap(render))
end

return M
