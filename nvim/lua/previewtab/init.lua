--- VS Code tarzı önizleme tab'ı. Files (neo-tree) ya da Git panelinden açılan, henüz açık olmayan dosya "önizleme"
--- olarak açılır (bufferline'da italik); sonraki önizleme aynı bufferline yuvasında onun yerini alır (eski buffer
--- `Snacks.bufdelete` ile kapanır — önizleme tanım gereği değiştirilmemiştir). Aynı anda tek önizleme olur.
--- Normal tab'a çevirenler: değişiklik (`BufModifiedSet`), `:w`, aynı dosyayı panelden tekrar açma (Enter / çift tık),
--- pin (Cmd+P; bufferline `groups.add_element` sarılır), Snacks picker'dan açma (Cmd+F, Cmd+E, grep; `lsp_*`
--- kaynaklarında yalnız başka dosyadan gelince — önizlemenin içinde gd çevirmez).
--- Durum yalnız bellekte: oturumdan gelen dosyalar normal tab'dır.
--- bufferline (`highlights.for_element`, `groups.add_element`, `state.custom_sort`) ve Snacks (`picker.actions.jump`)
--- iç yapısına dayanır — **güncellenince kontrol et.**
local M = {}

local preview ---@type integer?
local italic = {} ---@type table<string, string>

---@return integer?
function M.get()
  if preview and vim.api.nvim_buf_is_valid(preview) and vim.bo[preview].buflisted then
    return preview
  end
  preview = nil
end

--- `buf` önizlemeyse normal tab'a çevirir.
---@param buf integer?
function M.promote(buf)
  if buf and buf == preview then
    preview = nil
    vim.cmd.redrawtabline()
  end
end

---@param p string
local function real(p)
  return vim.uv.fs_realpath(p) or p
end

---@param path string
---@return integer?
local function find(path)
  path = real(path)
  for _, b in ipairs(vim.api.nvim_list_bufs()) do
    local name = vim.api.nvim_buf_get_name(b)
    if name ~= "" and real(name) == path then
      return b
    end
  end
end

--- bufferline sırasında `old`'un yuvasına `new`'i koyar (yoksa yeni buffer sona eklenirdi).
---@param old integer
---@param new integer
local function take_slot(old, new)
  local state = package.loaded["bufferline.state"]
  if not state or not state.components then
    return
  end
  local ids, slot = {}, nil
  for _, c in ipairs(state.components) do
    if c.id == old then
      slot = #ids + 1
    end
    if c.id ~= new then
      ids[#ids + 1] = c.id
    end
  end
  if not slot then
    return
  end
  ids[slot] = new
  state.custom_sort = ids
  if require("bufferline.config").options.persist_buffer_sort then
    require("bufferline.utils").save_positions(ids)
  end
end

--- Panelden dosya açma. Dosya zaten normal tab'sa `open()`; önizlemeyse (ve `keep` değilse) önce normal tab'a çevirir;
--- açık değilse önizleme olarak açar ve varsa eski önizlemenin yerine koyar.
---@param path string
---@param open fun() normal açma (dosyayı kod penceresinde gösterir)
---@param keep? boolean true: dosya zaten önizlemeyse çevirme (tek tık)
function M.open(path, open, keep)
  local buf = find(path)
  if buf and vim.bo[buf].buflisted then
    if not keep then
      M.promote(buf)
    end
    return open()
  end
  local old = M.get()
  open()
  local new = find(path)
  if not new or new == old or not vim.bo[new].buflisted or vim.bo[new].buftype ~= "" then
    return
  end
  preview = new
  if old and not vim.bo[old].modified then
    take_slot(old, new)
    pcall(Snacks.bufdelete, old)
  end
  vim.cmd.redrawtabline()
end

--- neo-tree eşlemesi (Enter, çift tık, tek tık). Dosyada `M.open`, klasörde neo-tree'nin `open`'ı.
--- Dosya önizleme olarak yeni açılıyorsa odak ağaçta kalır; tekrar Enter önizlemeyi normal tab'a çevirip odağı ona taşır.
--- `click` (tek tık): odak her zaman ağaçta kalır, klasörde ve tıklama ağaç penceresinde değilse bir şey yapmaz.
---@param state table neo-tree state
---@param click? boolean
function M.neotree(state, click)
  local win = vim.api.nvim_get_current_win()
  if click and vim.fn.getmousepos().winid ~= win then
    return
  end
  local node = state.tree:get_node()
  if not node or node.type ~= "file" then
    if not click then
      state.commands.open(state)
    end
    return
  end
  local path = node.path or node:get_id()
  local buf = find(path)
  local stay = click or not (buf and vim.bo[buf].buflisted)
  M.open(path, function()
    state.commands.open(state)
  end, click)
  if stay and vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_set_current_win(win)
  end
end

---@param name string
local function italic_hl(name)
  if not italic[name] then
    local spec = vim.api.nvim_get_hl(0, { name = name, link = false })
    spec.italic = true
    vim.api.nvim_set_hl(0, name .. "Preview", spec)
    italic[name] = name .. "Preview"
  end
  return italic[name]
end

--- bufferline yüklenirken (spec `opts`) çağrılır: önizleme adı italik, pinlemek normal tab'a çevirir.
function M.patch_bufferline()
  local hl = require("bufferline.highlights")
  local for_element = hl.for_element
  hl.for_element = function(element)
    local h = for_element(element)
    if element.id == preview and h.buffer then
      h.buffer = italic_hl(h.buffer)
    end
    return h
  end
  local groups = require("bufferline.groups")
  local add_element = groups.add_element
  groups.add_element = function(name, element)
    if name == "pinned" and element then
      M.promote(element.id)
    end
    return add_element(name, element)
  end
end

--- Snacks picker'ın `jump`/`confirm` eylemini sarar: picker'dan önizleme dosyasına gidilirse normal tab'a çevirir.
local function patch_picker()
  local ok, actions = pcall(require, "snacks.picker.actions")
  if not ok then
    return
  end
  local jump = actions.jump
  local function wrapped(picker, item, action)
    local main = picker.main
    local from = main and vim.api.nvim_win_is_valid(main) and vim.api.nvim_win_get_buf(main)
    local ret = jump(picker, item, action)
    local cur = vim.api.nvim_get_current_buf()
    if cur ~= from or not (picker.opts.source or ""):find("^lsp_") then
      M.promote(cur)
    end
    return ret
  end
  actions.jump, actions.confirm = wrapped, wrapped
end

function M.setup()
  local group = vim.api.nvim_create_augroup("previewtab", { clear = true })
  local function on(event, fn)
    vim.api.nvim_create_autocmd(event, { group = group, callback = fn })
  end
  on("BufModifiedSet", function(a)
    if vim.bo[a.buf].modified then
      M.promote(a.buf)
    end
  end)
  on("BufWritePost", function(a)
    M.promote(a.buf)
  end)
  on("BufDelete", function(a)
    if a.buf == preview then
      preview = nil
    end
  end)
  on("ColorScheme", function()
    italic = {}
  end)
  on("BufAdd", function(a)
    -- custom_sort'ta olmayan buffer'lar arasındaki sıra kararsız (table.sort); yenileri sona ekle
    local state = package.loaded["bufferline.state"]
    if state and state.custom_sort and not vim.tbl_contains(state.custom_sort, a.buf) then
      table.insert(state.custom_sort, a.buf)
    end
  end)
  patch_picker()
end

return M
