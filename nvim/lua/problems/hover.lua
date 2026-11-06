--- Hata/uyarı tooltip'i: satır sonundaki kısa sanal metin yerine o satırın diagnostiklerini tam metinle yüzen pencerede
--- gösterir. İki tetik: imleç kod penceresinde diagnostikli satırda durunca (`CursorHold`, satırın altında) ve fare
--- diagnostikli satırın üstünde bekleyince (`mousemoveevent`, fare konumunda). İmleç oynayınca / fare satırdan çıkınca /
--- tuşa basılınca kapanır. Başka bir yüzen pencere (LSP hover, tamamlama, picker) açıkken açılmaz.
--- Fare bir kelimenin (metot, sınıf, alan) üstünde bekleyince LSP hover'ı (Javadoc / JSDoc) da aynı pencerede,
--- diagnostiklerin altında gösterilir (IntelliJ'deki gibi); kelime değişince yenilenir.
--- Kapatmak için `vim.g.diagnostic_hover = false`.
local M = {}

local MAX_WIDTH = 80
local MOUSE_DELAY = 350
local ns = vim.api.nvim_create_namespace("problems_hover")

local state = {
  win = nil,
  ---@type { win: integer, line: integer, mouse: boolean, word?: integer }?
  at = nil,
  seq = 0,
  timer = nil,
}

local SEV = {
  [vim.diagnostic.severity.ERROR] = { "Error", "DiagnosticError" },
  [vim.diagnostic.severity.WARN] = { "Warn", "DiagnosticWarn" },
  [vim.diagnostic.severity.INFO] = { "Info", "DiagnosticInfo" },
  [vim.diagnostic.severity.HINT] = { "Hint", "DiagnosticHint" },
}

--- Tooltip açıksa kapatır.
function M.close()
  if state.win and vim.api.nvim_win_is_valid(state.win) then
    pcall(vim.api.nvim_win_close, state.win, true)
  end
  state.win, state.at = nil, nil
end

--- Bizimki dışında görünen yüzen pencere ya da tamamlama menüsü var mı?
---@return boolean
local function other_float()
  if vim.fn.pumvisible() == 1 then
    return true
  end
  return vim.iter(vim.api.nvim_tabpage_list_wins(0)):any(function(w)
    local cfg = vim.api.nvim_win_get_config(w)
    return w ~= state.win and cfg.relative ~= "" and cfg.focusable ~= false and not cfg.hide
  end)
end

--- Satırın diagnostikleri, önem sırasıyla (en ciddi önce).
---@param buf integer
---@param line integer 0 tabanlı
---@return vim.Diagnostic[]
local function line_diagnostics(buf, line)
  local list = vim.diagnostic.get(buf, { lnum = line })
  table.sort(list, function(a, b)
    return a.severity < b.severity or (a.severity == b.severity and a.col < b.col)
  end)
  return list
end

--- Satırın diagnostiklerini tooltip satırlarına ve vurgularına çevirir.
---@param buf integer
---@param line integer 0 tabanlı
---@return string[] lines, table[] hls
local function diag_lines(buf, line)
  local diags = line_diagnostics(buf, line)
  local icons = (LazyVim and LazyVim.config.icons.diagnostics) or {}
  local lines, hls = {}, {}
  for _, d in ipairs(diags) do
    local sev = SEV[d.severity] or SEV[vim.diagnostic.severity.HINT]
    local icon = icons[sev[1]] or "● "
    local src = d.source and (" " .. d.source .. (d.code and ("(" .. d.code .. ")") or "")) or ""
    for i, text in ipairs(vim.split(vim.trim(d.message), "\n")) do
      local prefix = i == 1 and icon or (" "):rep(vim.fn.strdisplaywidth(icon))
      table.insert(lines, prefix .. text)
      table.insert(hls, { #lines - 1, 0, #prefix, sev[2] })
    end
    if src ~= "" then
      lines[#lines] = lines[#lines] .. src
      table.insert(hls, { #lines - 1, #lines[#lines] - #src, #lines[#lines], "Comment" })
    end
  end
  return lines, hls
end

--- Satırları yüzen pencerede gösterir (`markdown` ise LSP dokümantasyonu içerir, markdown olarak renklenir).
---@param lines string[]
---@param hls table[]
---@param at { win: integer, line: integer, mouse: boolean, word?: integer }
---@param markdown? boolean
local function show(lines, hls, at, markdown)
  M.close()
  if #lines == 0 then
    return
  end
  local mouse = at.mouse
  local width = 1
  for _, l in ipairs(lines) do
    width = math.max(width, vim.fn.strdisplaywidth(l))
  end
  width = math.min(width, MAX_WIDTH, vim.o.columns - 4)
  local fbuf = vim.api.nvim_create_buf(false, true)
  vim.bo[fbuf].bufhidden = "wipe"
  vim.api.nvim_buf_set_lines(fbuf, 0, -1, false, lines)
  for _, h in ipairs(hls) do
    vim.api.nvim_buf_set_extmark(fbuf, ns, h[1], h[2], { end_col = h[3], hl_group = h[4] })
  end
  local fwin = vim.api.nvim_open_win(fbuf, false, {
    relative = mouse and "mouse" or "cursor",
    row = 1,
    col = 0,
    width = width,
    height = 1,
    style = "minimal",
    border = "rounded",
    focusable = false,
    zindex = 60,
  })
  vim.wo[fwin].wrap = true
  vim.wo[fwin].linebreak = true
  if markdown then
    vim.wo[fwin].conceallevel = 2
    vim.wo[fwin].concealcursor = "nvic"
    vim.bo[fbuf].filetype = "markdown" -- pencere açıldıktan sonra: render-markdown ancak böyle çizer
    pcall(vim.treesitter.start, fbuf, "markdown")
  end
  local height = vim.api.nvim_win_text_height(fwin, {}).all
  vim.api.nvim_win_set_height(fwin, math.max(1, math.min(height, math.floor(vim.o.lines * 0.4))))
  state.win, state.at = fwin, at
end

--- Diagnostik tooltip'i (imleç için). Aynı satırda zaten açıksa dokunmaz.
---@param win integer
---@param line integer 0 tabanlı
---@param mouse boolean
local function open(win, line, mouse)
  if state.at and state.at.win == win and state.at.line == line and state.win and vim.api.nvim_win_is_valid(state.win) then
    return
  end
  local lines, hls = diag_lines(vim.api.nvim_win_get_buf(win), line)
  if #lines == 0 then
    return M.close()
  end
  show(lines, hls, { win = win, line = line, mouse = mouse })
end

--- Satırdaki kelimenin (harf, rakam, `_`) başlangıç sütunu; `col` kelime üstünde değilse nil.
---@param buf integer
---@param line integer 0 tabanlı
---@param col integer 0 tabanlı bayt
---@return integer?
local function word_start(buf, line, col)
  local text = vim.api.nvim_buf_get_lines(buf, line, line + 1, false)[1] or ""
  local i = col + 1
  if not text:sub(i, i):match("[%w_]") then
    return nil
  end
  while i > 1 and text:sub(i - 1, i - 1):match("[%w_]") do
    i = i - 1
  end
  return i - 1
end

--- Fare konumu için diagnostik + LSP hover (dokümantasyon) tooltip'i. Hover yanıtı gelene kadar fare kelimeden
--- ayrıldıysa gösterilmez.
---@param win integer
---@param line integer 0 tabanlı
---@param col integer 0 tabanlı bayt
local function open_mouse(win, line, col)
  local buf = vim.api.nvim_win_get_buf(win)
  local lines, hls = diag_lines(buf, line)
  local word = word_start(buf, line, col)
  local at = { win = win, line = line, mouse = true, word = word }
  local clients = word and vim.lsp.get_clients({ bufnr = buf, method = "textDocument/hover" }) or {}
  if #clients == 0 then
    return show(lines, hls, at)
  end
  state.seq = state.seq + 1
  local seq = state.seq
  local text = vim.api.nvim_buf_get_lines(buf, line, line + 1, false)[1] or ""
  vim.lsp.buf_request_all(buf, "textDocument/hover", function(client)
    return {
      textDocument = { uri = vim.uri_from_bufnr(buf) },
      position = { line = line, character = vim.str_utfindex(text, client.offset_encoding, col, false) },
    }
  end, function(results)
    local now = vim.fn.getmousepos()
    if seq ~= state.seq or now.winid ~= win or now.line - 1 ~= line or other_float() then
      return
    end
    local doc, md = {}, false
    for _, r in pairs(results) do
      local contents = r.result and r.result.contents
      if contents then
        md = md or contents.kind ~= "plaintext"
        vim.list_extend(doc, vim.lsp.util.convert_input_to_markdown_lines(contents))
      end
    end
    while #doc > 0 and vim.trim(doc[#doc]) == "" do
      doc[#doc] = nil
    end
    if #doc > 0 and #lines > 0 then
      table.insert(lines, ("─"):rep(40))
      table.insert(hls, { #lines - 1, 0, #lines[#lines], "FloatBorder" })
    end
    show(vim.list_extend(lines, doc), hls, at, md and #doc > 0)
  end)
end

---@return boolean
local function enabled()
  return vim.g.diagnostic_hover ~= false
end

--- CursorHold: imleçteki satırda diagnostik varsa tooltip (normal mod, kod penceresi, başka float yokken).
local function on_hold()
  local win = vim.api.nvim_get_current_win()
  if not enabled() or vim.api.nvim_get_mode().mode ~= "n" or other_float() or (state.at and state.at.mouse)
    or not require("problems").is_code_window(win) then
    return
  end
  open(win, vim.api.nvim_win_get_cursor(win)[1] - 1, false)
end

--- Fare hareketi: tooltip başka satırdaysa kapatır, fare diagnostikli satırda `MOUSE_DELAY` ms durursa açar.
local function on_mouse_move()
  local pos = vim.fn.getmousepos()
  local word = pos.winid ~= 0 and pos.line > 0
    and word_start(vim.api.nvim_win_get_buf(pos.winid), pos.line - 1, pos.column - 1) or nil
  local same = state.at and state.at.mouse and state.at.win == pos.winid and state.at.line == pos.line - 1
    and state.at.word == word
  if state.at and state.at.mouse and not same then
    M.close()
  end
  state.timer = state.timer or vim.uv.new_timer()
  state.timer:stop()
  if same or pos.winid == 0 or pos.line == 0 then
    return
  end
  state.timer:start(MOUSE_DELAY, 0, vim.schedule_wrap(function()
    local now = vim.fn.getmousepos()
    if now.winid ~= pos.winid or now.line ~= pos.line or now.column ~= pos.column or not enabled() or other_float()
      or not require("problems").is_code_window(now.winid) then
      return
    end
    open_mouse(now.winid, now.line - 1, now.column - 1)
  end))
end

--- Autocmd'leri ve fare dinleyicisini kurar (`<MouseMove>` eşlenmez, bufferline'ınkine dokunulmaz).
function M.setup()
  local group = vim.api.nvim_create_augroup("problems_hover", { clear = true })
  vim.api.nvim_create_autocmd("CursorHold", { group = group, callback = on_hold })
  vim.api.nvim_create_autocmd({ "CursorMoved", "InsertEnter", "BufLeave", "WinLeave", "WinScrolled" }, {
    group = group,
    callback = function()
      if state.at and not state.at.mouse then
        M.close()
      end
    end,
  })
  vim.api.nvim_create_autocmd("DiagnosticChanged", {
    group = group,
    callback = function()
      if state.at and not state.at.mouse then
        M.close()
      end
    end,
  })
  local move = vim.keycode("<MouseMove>")
  vim.on_key(function(key, typed)
    if key == move or typed == move then
      vim.schedule(on_mouse_move)
    elseif state.at and state.at.mouse and (typed or "") ~= "" then -- eklentilerin `normal!` tuşları kapatmasın
      vim.schedule(M.close)
    end
  end, ns)
end

return M
