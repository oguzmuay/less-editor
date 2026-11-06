--- Tek pencerede çok alanlı form (bağlantı ekle/düzenle). Her satır bir alanın değeridir; etiketler satır başında
--- satır içi sanal metin olarak durur, yani yalnız değerler düzenlenir. Gizli alanlar (parola) conceal ile `•` görünür.
local M = {}

local ns = vim.api.nvim_create_namespace("datasource_form")

---@class DatasourceField
---@field key string
---@field label string
---@field value? string
---@field secret? boolean
---@field hint? string boşken satır sonunda gösterilen örnek

--- Formu ortada açar. Enter kaydeder (`submit` hata metni döndürürse form açık kalır), Esc/q iptal eder,
--- Tab / Shift+Tab alanlar arasında gezer. Satır eklenip silinirse kaydetmez.
---@param opts { title: string, fields: DatasourceField[], submit: fun(values: table<string, string>): string? }
function M.open(opts)
  local fields = opts.fields
  local label_w = 0
  for _, f in ipairs(fields) do
    label_w = math.max(label_w, vim.fn.strdisplaywidth(f.label))
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].bufhidden = "wipe"
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.tbl_map(function(f)
    return f.value or ""
  end, fields))
  local width = math.min(vim.o.columns - 4, math.max(60, label_w + 40))
  local win = vim.api.nvim_open_win(buf, true, {
    relative = "editor",
    width = width,
    height = #fields,
    row = math.floor((vim.o.lines - #fields) / 2) - 2,
    col = math.floor((vim.o.columns - width) / 2),
    border = "rounded",
    style = "minimal",
    title = " " .. opts.title .. " ",
    title_pos = "center",
    footer = " ⏎ Save · Tab Next field · Esc Cancel ",
    footer_pos = "center",
  })
  vim.wo[win].conceallevel = 2
  vim.wo[win].concealcursor = "nvic"
  vim.wo[win].wrap = false
  for i, f in ipairs(fields) do
    local pad = (" "):rep(label_w - vim.fn.strdisplaywidth(f.label))
    vim.api.nvim_buf_set_extmark(buf, ns, i - 1, 0, {
      virt_text = { { " " .. f.label .. pad .. "  ", "Comment" } },
      virt_text_pos = "inline",
      right_gravity = false,
    })
    if f.hint then
      vim.api.nvim_buf_set_extmark(buf, ns, i - 1, 0, { virt_text = { { "  " .. f.hint, "NonText" } } })
    end
    if f.secret then
      vim.fn.matchadd("Conceal", ("\\%%%dl."):format(i), 10, -1, { conceal = "•", window = win })
    end
  end

  local function close()
    if vim.api.nvim_win_is_valid(win) then
      vim.api.nvim_win_close(win, true)
      vim.cmd.stopinsert()
    end
  end
  local function submit()
    local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
    if #lines ~= #fields then
      vim.notify("Form lines changed; press u to undo", vim.log.levels.WARN)
      return
    end
    local values = {}
    for i, f in ipairs(fields) do
      values[f.key] = vim.trim(lines[i])
    end
    local err = opts.submit(values)
    if err then
      vim.notify(err, vim.log.levels.WARN)
      return
    end
    close()
  end
  local function move(delta)
    local row = vim.api.nvim_win_get_cursor(win)[1] + delta
    row = (row - 1) % #fields + 1
    vim.api.nvim_win_set_cursor(win, { row, #vim.api.nvim_buf_get_lines(buf, row - 1, row, false)[1] })
  end
  local function map(modes, lhs, fn)
    vim.keymap.set(modes, lhs, fn, { buffer = buf, nowait = true })
  end
  map({ "n", "i" }, "<CR>", submit)
  map({ "n", "i" }, "<Tab>", function() move(1) end)
  map({ "n", "i" }, "<S-Tab>", function() move(-1) end)
  map({ "n", "i" }, "<Down>", function() move(1) end)
  map({ "n", "i" }, "<Up>", function() move(-1) end)
  map("n", "<Esc>", close)
  map("n", "q", close)
  vim.api.nvim_create_autocmd("WinLeave", { buffer = buf, once = true, callback = vim.schedule_wrap(close) })

  vim.api.nvim_win_set_cursor(win, { 1, 0 })
  vim.cmd.startinsert({ bang = true })
  return buf, win
end

return M
