local M = {}

local ns_lens = vim.api.nvim_create_namespace("gitblame_lens")
local ns_annotate = vim.api.nvim_create_namespace("gitblame_annotate")
local UNCOMMITTED = string.rep("0", 40)
local MAX_AUTHOR = 18

local lens_kinds = {
  [5] = true, -- Class
  [6] = true, -- Method
  [9] = true, -- Constructor
  [10] = true, -- Enum
  [11] = true, -- Interface
  [12] = true, -- Function
  [23] = true, -- Struct
}

local leaf_kinds = { [6] = true, [9] = true, [12] = true }

local author_hls = { "String", "Function", "Constant", "Type", "Identifier", "Special", "DiagnosticInfo" }

local state = {
  enabled = false,
  src_win = nil,
  win = nil,
  buf = nil,
  shas = {},
  timers = {},
}

---@class BlameCommit
---@field sha string
---@field author string
---@field time integer
---@field summary string

--- Buffer'ın güncel içeriğiyle (kaydedilmemiş değişiklikler dahil) git blame çalıştırır.
--- Sonuç: her satır için commit bilgisi; commit edilmemiş satırlar `nil`.
---@param buf integer
---@param cb fun(lines: (BlameCommit|false)[])
local function blame(buf, cb)
  local file = vim.api.nvim_buf_get_name(buf)
  if file == "" or vim.bo[buf].buftype ~= "" or not vim.uv.fs_stat(file) then
    return
  end
  local content = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), "\n") .. "\n"
  vim.system(
    { "git", "blame", "--porcelain", "--contents", "-", "--", vim.fs.basename(file) },
    { cwd = vim.fs.dirname(file), stdin = content, text = true },
    vim.schedule_wrap(function(res)
      if res.code ~= 0 or not vim.api.nvim_buf_is_valid(buf) then
        return
      end
      local commits, lines, current = {}, {}, nil
      for line in vim.gsplit(res.stdout, "\n", { plain = true }) do
        local sha, final = line:match("^(%x+) %d+ (%d+)")
        if sha and #sha == 40 then
          current = commits[sha] or { sha = sha, author = "", time = 0, summary = "" }
          commits[sha] = current
          lines[tonumber(final)] = sha ~= UNCOMMITTED and current or false
        elseif current then
          local key, val = line:match("^(%S+) (.*)$")
          if key == "author" then
            current.author = val
          elseif key == "author-time" then
            current.time = tonumber(val) or 0
          elseif key == "summary" then
            current.summary = val
          end
        end
      end
      cb(lines)
    end)
  )
end

--- Metni verilen genişliğe kırpar veya boşlukla doldurur.
---@param s string
---@param n integer
---@return string
local function fit(s, n)
  local w = vim.fn.strdisplaywidth(s)
  if w > n then
    return vim.fn.strcharpart(s, 0, n - 1) .. "…"
  end
  return s .. string.rep(" ", n - w)
end

--- Yazar adını her seferinde aynı renge eşler; böylece farklı kişiler blame sütununda ayırt edilir.
---@param author string
---@return string
local function author_hl(author)
  local sum = 0
  for i = 1, #author do
    sum = sum + author:byte(i)
  end
  return author_hls[sum % #author_hls + 1]
end

--- LSP'den dosyadaki sembolleri (DocumentSymbol ağacı ya da düz SymbolInformation listesi) alıp
--- lens gösterilecek tanımların { satır, başlangıç, bitiş } listesini döndürür (0 tabanlı).
--- Metot/fonksiyon gövdelerine inilmez; böylece callback'ler gibi iç fonksiyonlar lens almaz.
---@param buf integer
---@param cb fun(decls: { line: integer, s: integer, e: integer }[])
local function declarations(buf, cb)
  local client = vim.lsp.get_clients({ bufnr = buf, method = "textDocument/documentSymbol" })[1]
  if not client then
    return
  end
  local params = { textDocument = vim.lsp.util.make_text_document_params(buf) }
  client:request("textDocument/documentSymbol", params, function(err, result)
    if err or type(result) ~= "table" then
      return
    end
    local decls = {}
    local function walk(symbols)
      for _, sym in ipairs(symbols) do
        local range = sym.range or (sym.location and sym.location.range)
        if range and lens_kinds[sym.kind] then
          local head = (sym.selectionRange or range).start.line
          table.insert(decls, { line = head, s = range.start.line, e = range["end"].line })
        end
        if sym.children and not leaf_kinds[sym.kind] then
          walk(sym.children)
        end
      end
    end
    walk(result)
    vim.schedule(function()
      cb(decls)
    end)
  end, buf)
end

--- Tanım satırlarının sonuna, o bloğu en çok değiştiren yazarı (ve kaç kişinin daha dokunduğunu) yazar.
--- Blame sütunu açıkken lens gösterilmez.
---@param buf integer
function M.refresh_lens(buf)
  if not vim.api.nvim_buf_is_valid(buf) then
    return
  end
  if state.enabled then
    vim.api.nvim_buf_clear_namespace(buf, ns_lens, 0, -1)
    return
  end
  declarations(buf, function(decls)
    blame(buf, function(lines)
      vim.api.nvim_buf_clear_namespace(buf, ns_lens, 0, -1)
      if state.enabled then
        return
      end
      for _, d in ipairs(decls) do
        local counts, order = {}, {}
        for l = d.s + 1, d.e + 1 do
          local c = lines[l]
          if c then
            if not counts[c.author] then
              table.insert(order, c.author)
            end
            counts[c.author] = (counts[c.author] or 0) + 1
          end
        end
        table.sort(order, function(a, b)
          return counts[a] > counts[b]
        end)
        if #order > 0 then
          local text = "\u{F007} " .. order[1] .. (#order > 1 and (" +" .. (#order - 1)) or "")
          pcall(vim.api.nvim_buf_set_extmark, buf, ns_lens, d.line, 0, {
            virt_text = { { "  " .. text, "GitBlameLens" } },
            virt_text_pos = "eol",
            hl_mode = "combine",
          })
        end
      end
    end)
  end)
end

--- Blame sütununun içeriğini kaynak buffer'ın blame bilgisiyle yeniden yazar.
---@param src_buf integer
local function fill_annotate(src_buf)
  blame(src_buf, function(lines)
    if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
      return
    end
    local count = vim.api.nvim_buf_line_count(src_buf)
    local text, marks, width = {}, {}, 10
    state.shas = {}
    for l = 1, count do
      local c = lines[l]
      if c then
        local date = os.date("%Y-%m-%d", c.time)
        local author = fit(c.author, MAX_AUTHOR)
        text[l] = " " .. date .. "  " .. author
        state.shas[l] = c.sha
        table.insert(marks, { l - 1, 1, 1 + #date, "GitBlameDate" })
        table.insert(marks, { l - 1, 3 + #date, #text[l], author_hl(c.author) })
        width = math.max(width, vim.fn.strdisplaywidth(text[l]) + 1)
      else
        text[l] = ""
      end
    end
    vim.bo[state.buf].modifiable = true
    vim.api.nvim_buf_set_lines(state.buf, 0, -1, false, text)
    vim.bo[state.buf].modifiable = false
    vim.api.nvim_buf_clear_namespace(state.buf, ns_annotate, 0, -1)
    for _, m in ipairs(marks) do
      vim.api.nvim_buf_set_extmark(state.buf, ns_annotate, m[1], m[2], { end_col = m[3], hl_group = m[4] })
    end
    if state.win and vim.api.nvim_win_is_valid(state.win) then
      vim.api.nvim_win_set_width(state.win, width)
      if state.src_win and vim.api.nvim_win_is_valid(state.src_win) then
        vim.api.nvim_win_call(state.src_win, function()
          vim.cmd("syncbind")
        end)
      end
    end
  end)
end

--- Blame sütununda imlecin bulunduğu satırın commit'ini kod editöründe diff tab'ı olarak açar (tüm commit).
local function open_commit()
  local sha = state.shas[vim.api.nvim_win_get_cursor(0)[1]]
  local src = state.src_win and vim.api.nvim_win_is_valid(state.src_win) and vim.api.nvim_win_get_buf(state.src_win)
  if sha and src then
    require("gitdiff").open({ rev = sha, cwd = vim.fs.dirname(vim.api.nvim_buf_get_name(src)) })
  end
end

--- Kaynak ve blame pencerelerinin kaydırma/imleç bağını kurar ya da kaldırır.
---@param on boolean
local function bind(on)
  for _, win in ipairs({ state.src_win, state.win }) do
    if win and vim.api.nvim_win_is_valid(win) then
      vim.wo[win].scrollbind = on
      vim.wo[win].cursorbind = on
    end
  end
end

--- Blame sütununu kapatır ve lens'leri geri getirir.
function M.close()
  bind(false)
  if state.win and vim.api.nvim_win_is_valid(state.win) and #vim.api.nvim_tabpage_list_wins(0) > 1 then
    vim.api.nvim_win_close(state.win, true)
  end
  state.enabled, state.win = false, nil
  if state.src_win and vim.api.nvim_win_is_valid(state.src_win) then
    M.refresh_lens(vim.api.nvim_win_get_buf(state.src_win))
  end
end

--- Kod penceresinin soluna, kaydırması kodla senkron blame sütununu açar.
function M.open()
  local src = require("workbench").code_win()
  if not src then
    return
  end
  state.src_win, state.enabled = src, true
  if not (state.buf and vim.api.nvim_buf_is_valid(state.buf)) then
    state.buf = vim.api.nvim_create_buf(false, true)
    vim.bo[state.buf].filetype = "gitblame"
    vim.bo[state.buf].bufhidden = "hide"
    for _, lhs in ipairs({ "<CR>", "<2-LeftMouse>" }) do
      vim.keymap.set("n", lhs, open_commit, { buffer = state.buf, nowait = true })
    end
    vim.keymap.set("n", "q", M.close, { buffer = state.buf, nowait = true })
  end
  state.win = vim.api.nvim_open_win(state.buf, false, { split = "left", win = src, width = 30 })
  for opt, val in pairs({
    number = false,
    relativenumber = false,
    signcolumn = "no",
    foldcolumn = "0",
    statuscolumn = "",
    foldenable = false,
    wrap = false,
    winfixwidth = true,
    cursorline = true,
    list = false,
  }) do
    vim.wo[state.win][opt] = val
  end
  vim.wo[src].foldenable = false
  bind(true)
  local src_buf = vim.api.nvim_win_get_buf(src)
  vim.api.nvim_buf_clear_namespace(src_buf, ns_lens, 0, -1)
  fill_annotate(src_buf)
end

--- :GitBlame — blame sütununu aç/kapat.
function M.toggle()
  if state.enabled and state.win and vim.api.nvim_win_is_valid(state.win) then
    M.close()
  else
    M.open()
  end
end

--- Değişiklikleri biriktirip kısa bir gecikmeyle lens/blame'i yeniler (her tuşta git çalıştırmamak için).
---@param buf integer
local function schedule_refresh(buf)
  local timer = state.timers[buf] or vim.uv.new_timer()
  state.timers[buf] = timer
  timer:stop()
  timer:start(
    400,
    0,
    vim.schedule_wrap(function()
      if not vim.api.nvim_buf_is_valid(buf) then
        return
      end
      if state.enabled and state.src_win and vim.api.nvim_win_is_valid(state.src_win) and vim.api.nvim_win_get_buf(state.src_win) == buf then
        fill_annotate(buf)
      end
      M.refresh_lens(buf)
    end)
  )
end

--- Renkleri, :GitBlame komutunu ve yenileme autocmd'lerini kurar.
function M.setup()
  local function highlights()
    vim.api.nvim_set_hl(0, "GitBlameLens", { link = "Comment", default = true })
    vim.api.nvim_set_hl(0, "GitBlameDate", { link = "Comment", default = true })
  end
  highlights()
  vim.api.nvim_create_user_command("GitBlame", M.toggle, { desc = "Toggle Git Blame column" })
  local group = vim.api.nvim_create_augroup("gitblame", { clear = true })
  vim.api.nvim_create_autocmd("ColorScheme", { group = group, callback = highlights })
  vim.api.nvim_create_autocmd({ "LspAttach", "BufWritePost", "InsertLeave", "TextChanged" }, {
    group = group,
    callback = function(ev)
      schedule_refresh(ev.buf)
    end,
  })
  vim.api.nvim_create_autocmd("BufWinEnter", {
    group = group,
    callback = function(ev)
      if state.enabled and vim.api.nvim_get_current_win() == state.src_win and vim.bo[ev.buf].buftype == "" then
        vim.wo[state.src_win].scrollbind = true
        vim.wo[state.src_win].cursorbind = true
        fill_annotate(ev.buf)
      end
      schedule_refresh(ev.buf)
    end,
  })
  vim.api.nvim_create_autocmd("WinClosed", {
    group = group,
    callback = function(ev)
      if tonumber(ev.match) == state.win then
        vim.schedule(M.close)
      end
    end,
  })
end

return M
