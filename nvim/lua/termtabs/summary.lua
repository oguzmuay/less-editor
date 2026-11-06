local M = {}

local sessions = require("termtabs.sessions")

local ns = vim.api.nvim_create_namespace("termtabs_summary")
local mascot_ns = vim.api.nvim_create_namespace("termtabs_summary_mascot")
local summary_buf = nil
---@type table<integer, integer> satır numarası -> oturum buffer'ı
local line_targets = {}

local status_text = {
  busy = { "working", "DiagnosticOk" },
  waiting = { "waiting for approval/input", "DiagnosticError" },
  idle = { "idle (your turn)", "DiagnosticInfo" },
}

---@type { row: integer, height: integer, color: integer, status: string?, kind: string? }[] oturum bloklarının maskot konumları
local mascots = {}
local tick, timer = 0, nil

--- Oturum başına maskot renkleri (ilki Claude turuncusu); `tab.n`'e göre sabit.
local COLORS = { "#D97757", "#7AA2F7", "#9ECE6A", "#BB9AF7", "#E0AF68", "#7DCFFF", "#F7768E", "#73DACA" }
local MASCOT_W = 14

--- AI türü başına 9 sütunluk, 3 satırlık maskot: `awake`, `sleep` (gözler kapalı) ve `grip` (2. satır, kazmayı tutan el;
--- 8 sütun). Claude: Claude Code maskotu; Codex: `>_` yüzlü robot; Gemini: dört köşeli yıldız; Ollama: lama.
local BODIES = {
  claude = {
    awake = { " ▐▛███▜▌ ", "▝▜█████▛▘", "  ▘▘ ▝▝  " },
    sleep = { " ▐█████▌ ", "▝▜█████▛▘", "  ▘▘ ▝▝  " },
    grip = "▝▜█████▛",
  },
  codex = {
    awake = { " ▐ >_  ▌ ", "▝▀█████▀▘", "   ▌ ▐   " },
    sleep = { " ▐ -_- ▌ ", "▝▀█████▀▘", "   ▌ ▐   " },
    grip = "▝▀█████▀",
  },
  gemini = {
    awake = { "    ▲    ", "  ◀ ● ▶  ", "    ▼    " },
    sleep = { "    ▲    ", "  ◀ ─ ▶  ", "    ▼    " },
    grip = "  ◀ ● ▶ ",
  },
  ollama = {
    awake = { "  ▗▘ ▝▖  ", "  █• •█  ", "  ▝█▄█▘  " },
    sleep = { "  ▗▘ ▝▖  ", "  █- -█  ", "  ▝█▄█▘  " },
    grip = "  █• •█ ",
  },
}

--- Maskotun 4 satırlık karesi (sağında token taşı): `busy` kazmayı kaldırır, savurur, taşa vurur ve token'lar (◆)
--- sıçrar; `idle` taşın yanında uyur (z Z); `waiting` başında yanıp sönen "!"; diğerleri sabit.
---@param status string?
---@param t integer kare sayacı
---@param kind? string AI türü (varsayılan claude)
---@return string[]
function M.frame(status, t, kind)
  local b = BODIES[kind or "claude"] or BODIES.claude
  local r1, r2, r3 = b.awake[1], b.awake[2], b.awake[3]
  local rock = "  ▟█▙"
  if status == "busy" then
    return ({
      { "        ┏━┻━┓ ", r1 .. "╱    ", r2 .. "     ", r3 .. rock },
      { "              ", r1 .. "━━━┓ ", r2 .. "   ┃ ", r3 .. rock },
      { "             ◆", r1 .. "  ◆  ", b.grip .. "━━━┓  ", r3 .. rock },
    })[t % 3 + 1]
  end
  local top = "              "
  if status == "idle" then
    top = ({ "      z       ", "     z Z      ", "    z Z z     " })[t % 3 + 1]
    r1, r2, r3 = b.sleep[1], b.sleep[2], b.sleep[3]
  elseif status == "waiting" and t % 2 == 0 then
    top = "    !         "
  end
  return { top, r1 .. "     ", r2 .. "     ", r3 .. rock }
end

--- Maskotları blokların soluna satır içi sanal metin olarak çizer (metin satırları değişmez, yalnız bu namespace).
local function draw_mascots()
  local buf = M.existing()
  if not buf then
    return
  end
  vim.api.nvim_buf_clear_namespace(buf, mascot_ns, 0, -1)
  for _, m in ipairs(mascots) do
    local rows = M.frame(m.status, tick, m.kind)
    for k = 0, m.height - 1 do
      vim.api.nvim_buf_set_extmark(buf, mascot_ns, m.row + k, 0, {
        virt_text = { { (rows[k + 1] or string.rep(" ", MASCOT_W)) .. "  ", "TermTabsMascot" .. m.color } },
        virt_text_pos = "inline",
      })
    end
  end
end

--- Özet görünürken 400 ms'de bir maskot karesini ilerletir; özet görünmez olunca durur.
local function animate()
  if timer then
    return
  end
  timer = vim.uv.new_timer()
  timer:start(400, 400, vim.schedule_wrap(function()
    local buf = M.existing()
    if not buf or #vim.fn.win_findbuf(buf) == 0 then
      timer:stop()
      timer:close()
      timer = nil
      return
    end
    tick = tick + 1
    draw_mascots()
  end))
end

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
  local multi = #require("termtabs.accounts").list() > 1
  local lines, marks, targets = {}, {}, {}
  mascots = {}
  for i, color in ipairs(COLORS) do
    vim.api.nvim_set_hl(0, "TermTabsMascot" .. i, { fg = color })
  end
  local total = 0
  for _, tab in ipairs(tabs) do
    total = total + (tab.meta and tab.meta.total and (tab.meta.total + tab.meta.subagents) or 0)
  end
  local function add(text, target, hl)
    table.insert(lines, text)
    targets[#lines] = target
    if hl then
      table.insert(marks, { #lines - 1, 0, #text, hl })
    end
  end
  add(
    ("AI sessions: %d open · %s Claude tokens · %s"):format(
      #tabs,
      sessions.fmt_tokens(total),
      os.date("%H:%M:%S")
    ),
    nil,
    "Title"
  )
  add("Enter / double-click: go to session   Cmd+Shift+Left/Right: tabs   Cmd+T: new AI session", nil, "Comment")
  local agents = require("termtabs.agents")
  for i, tab in ipairs(tabs) do
    local m = tab.meta
    add("", nil)
    local block = #lines
    local status = m and status_text[m.status] or { "starting", "Comment" }
    local account = multi and tab.account and ("  @" .. tab.account.name) or ""
    account = account .. (agents.has_mail(tab) and "  ✉ new mail" or "")
    local head = ("%d. %s %s%s"):format(i, tt.status_icon(tab), tt.label(tab, "claude"), account)
    local tail = "  [" .. status[1] .. (m and m.waiting_for and (": " .. m.waiting_for) or "") .. "]"
    add(head .. tail, tab.buf, "Bold")
    table.insert(marks, { #lines - 1, #head, #head + #tail, status[2] })
    if m and m.simple then
      add(("   %s · mailbox name %s"):format(agents.kinds[tab.agent].label, tab.ai_name or "-"), tab.buf)
      add(("   %s"):format(vim.fn.fnamemodify(tab.cwd, ":~")), tab.buf, "Comment")
    elseif m then
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
    while #lines - block < 4 do
      add("", tab.buf)
    end
    mascots[#mascots + 1] = {
      row = block,
      height = #lines - block,
      color = (tab.n - 1) % #COLORS + 1,
      status = m and m.status,
      kind = tab.agent,
    }
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
  draw_mascots()
  animate()
end

return M
