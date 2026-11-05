local M = {}

local sessions = require("termtabs.sessions")

local ns = vim.api.nvim_create_namespace("termtabs_summary")
local summary_buf = nil
---@type table<integer, integer> satır numarası -> oturum buffer'ı
local line_targets = {}

local status_text = {
  busy = { "working", "DiagnosticOk" },
  waiting = { "waiting for approval/input", "DiagnosticError" },
  idle = { "idle (your turn)", "DiagnosticInfo" },
}

--- Varsa özet buffer'ını döndürür (oluşturmaz).
---@return integer?
function M.existing()
  return summary_buf and vim.api.nvim_buf_is_valid(summary_buf) and summary_buf or nil
end

--- İmlecin bulunduğu satırdaki oturumun sekmesine geçer.
local function jump()
  local target = line_targets[vim.api.nvim_win_get_cursor(0)[1]]
  if target and vim.api.nvim_buf_is_valid(target) then
    require("termtabs").select(target, true)
  end
end

--- Özet buffer'ını döndürür; yoksa oluşturur (Claude grubuna ait, Enter / çift tık ile oturuma geçer).
---@return integer
function M.buf()
  if M.existing() then
    return summary_buf
  end
  summary_buf = vim.api.nvim_create_buf(false, true)
  local buf = summary_buf
  vim.b[buf].termtabs = "claude"
  vim.b[buf].termtabs_summary = true
  vim.bo[buf].bufhidden = "hide"
  vim.bo[buf].filetype = "termtabs-summary"
  vim.bo[buf].modifiable = false
  vim.keymap.set("n", "<CR>", jump, { buffer = buf, desc = "Go to session" })
  vim.keymap.set("n", "<2-LeftMouse>", jump, { buffer = buf, desc = "Go to session" })
  return buf
end

--- Unix zamanını "12 sn önce" / "3 dk önce" biçiminde yazar.
---@param t integer?
---@return string?
local function ago(t)
  if not t then
    return
  end
  local d = os.time() - t
  if d < 60 then
    return ("%ds ago"):format(math.max(d, 0))
  elseif d < 3600 then
    return ("%dm ago"):format(d / 60)
  end
  return ("%dh ago"):format(d / 3600)
end

--- Özet buffer'ı bir pencerede görünüyorsa içeriğini oturumların güncel durumuyla yeniden yazar.
--- Her oturum için: durum, token (girdi / önbellek / çıktı / bağlam / alt ajanlar), son araç, son istek ve yanıt.
function M.render()
  local buf = M.existing()
  if not buf or #vim.fn.win_findbuf(buf) == 0 then
    return
  end
  local tt = require("termtabs")
  local tabs = tt.tabs("claude")
  local lines, marks, targets = {}, {}, {}
  local total = 0
  for _, tab in ipairs(tabs) do
    total = total + (tab.meta and (tab.meta.total + tab.meta.subagents) or 0)
  end
  local function add(text, target, hl)
    table.insert(lines, text)
    targets[#lines] = target
    if hl then
      table.insert(marks, { #lines - 1, 0, #text, hl })
    end
  end
  add(
    ("Claude sessions: %d open · %s tokens total · %s"):format(
      #tabs,
      sessions.fmt_tokens(total),
      os.date("%H:%M:%S")
    ),
    nil,
    "Title"
  )
  add("Enter / double-click: go to session   Cmd+Shift+Left/Right: tabs   Cmd+T: new session", nil, "Comment")
  for i, tab in ipairs(tabs) do
    local m = tab.meta
    add("", nil)
    local status = m and status_text[m.status] or { "starting", "Comment" }
    local head = ("%d. %s %s"):format(i, tt.status_icon(tab), tt.label(tab, "claude"))
    local tail = "  [" .. status[1] .. (m and m.waiting_for and (": " .. m.waiting_for) or "") .. "]"
    add(head .. tail, tab.buf, "Bold")
    table.insert(marks, { #lines - 1, #head, #head + #tail, status[2] })
    if m then
      add(
        ("   Tokens: %s  (input %s · cache %s · output %s) · context %s"):format(
          sessions.fmt_tokens(m.total),
          sessions.fmt_tokens(m.input + m.cache_create),
          sessions.fmt_tokens(m.cache_read),
          sessions.fmt_tokens(m.output),
          sessions.fmt_tokens(m.context)
        ),
        tab.buf
      )
      if m.subagent_count > 0 then
        add(("   Subagents: %d · %s tokens"):format(m.subagent_count, sessions.fmt_tokens(m.subagents)), tab.buf)
      end
      add(("   %s %s"):format(m.status == "busy" and "Now:" or "Last tool:", m.last_tool or "-"), tab.buf)
      add("   Prompt: " .. (m.last_prompt or "-"), tab.buf)
      add("   Reply: " .. (m.last_text or "-"), tab.buf)
      local where = m.cwd and vim.fn.fnamemodify(m.cwd, ":~") or tab.cwd
      add(("   Last activity: %s · %s"):format(ago(m.mtime) or "-", where), tab.buf, "Comment")
    else
      add("   No session info yet (Claude Code is starting)", tab.buf, "Comment")
    end
  end
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.bo[buf].modified = false
  vim.api.nvim_buf_clear_namespace(buf, ns, 0, -1)
  for _, mk in ipairs(marks) do
    vim.api.nvim_buf_set_extmark(buf, ns, mk[1], mk[2], { end_col = mk[3], hl_group = mk[4] })
  end
  line_targets = targets
end

return M
