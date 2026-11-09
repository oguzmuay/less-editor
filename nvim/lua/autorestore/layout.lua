--- Proje / workspace başına panel düzeni: hangi paneller açık (Files ya da Git, AI, terminal, Database, Run ya da
--- Git History) ve boyutları (sol panel genişliği, sağ sütun genişliği, AI/terminal bölünmesi, alt şerit yüksekliği).
--- Kayıt `stdpath("state")/layouts.json` (testte `NVIM_LAYOUTS_FILE`), anahtar proje klasörü (workspace'te symlink
--- klasörü). Pencere olaylarından sonra (`vim.schedule`) yakalanır; düzen kurulurken (`busy`) ve olay ile yakalama
--- arasında cwd değiştiyse (workspace/proje geçişi) yakalanmaz, yoksa eski projenin düzeni yenisinin üstüne yazılırdı.
local M = {}

local state = {
  busy = true, -- ilk düzen kurulana kadar yakalama yok
  pending = false,
  data = nil, ---@type table?
}

---@return string
local function file()
  return vim.env.NVIM_LAYOUTS_FILE or (vim.fn.stdpath("state") .. "/layouts.json")
end

---@return { version: integer, layouts: table<string, table> }
local function read()
  if state.data then
    return state.data
  end
  local ok, data = pcall(function()
    return vim.json.decode(table.concat(vim.fn.readfile(file()), "\n"))
  end)
  state.data = (ok and type(data) == "table" and type(data.layouts) == "table") and data
    or { version = 1, layouts = {} }
  return state.data
end

local function write()
  vim.fn.mkdir(vim.fs.dirname(file()), "p")
  pcall(vim.fn.writefile, { vim.json.encode(read(), { escape_slash = false }) }, file())
end

--- Yüklü değilse modülü yüklemeden nil döner (açılmamış panel için modül yüklenmesin).
---@param name string
local function loaded(name)
  return package.loaded[name]
end

local function neo_tree_win()
  for _, w in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.bo[vim.api.nvim_win_get_buf(w)].filetype == "neo-tree" then
      return w
    end
  end
end

--- Panel adı → görünür pencereyi bulan, açan ve kapatan fonksiyonlar. Sıra açılış sırasıdır.
local PANELS = {
  {
    name = "files",
    find = neo_tree_win,
    open = function()
      require("neo-tree.command").execute({ action = "show", source = "filesystem", dir = vim.fn.getcwd() })
    end,
    close = function()
      vim.cmd("Neotree close")
    end,
  },
  {
    name = "git",
    find = function() return loaded("gitstatus") and loaded("gitstatus").find_panel() end,
    open = function() require("workbench").git() end,
    close = function(w) require("gitstatus").close_panel(w) end,
  },
  {
    name = "claude",
    find = function() return loaded("termtabs") and loaded("termtabs").win("claude") end,
    open = function() require("workbench").claude() end,
    close = function() require("termtabs").hide("claude") end,
  },
  {
    name = "terminal",
    find = function() return loaded("termtabs") and loaded("termtabs").win("shell") end,
    open = function() require("workbench").shell() end,
    close = function() require("termtabs").hide("shell") end,
  },
  {
    name = "db",
    find = function() return loaded("datasource") and loaded("datasource").find_panel() end,
    open = function() require("workbench").db() end,
    close = function(w) require("datasource").close_panel(w) end,
  },
  {
    name = "runner",
    find = function() return loaded("runner") and loaded("runner").find_panel() end,
    open = function() require("workbench").open_runner() end,
    close = function(w) require("runner").close_panel(w) end,
  },
  {
    name = "git_history",
    find = function() return loaded("githistory") and loaded("githistory").find_panel() end,
    open = function() require("workbench").git_history() end,
    close = function(w) require("githistory").close_panel(w) end,
  },
}

--- O anki düzen: açık panel adları ve boyutlar.
---@return { panels: string[], sizes: table }
local function snapshot()
  local panels, wins = {}, {}
  for _, p in ipairs(PANELS) do
    local w = p.find()
    if w then
      table.insert(panels, p.name)
      wins[p.name] = w
    end
  end
  local api = vim.api
  local sizes = {}
  local left = wins.files or wins.git
  if left then
    sizes.left = api.nvim_win_get_width(left)
  end
  local right = wins.claude or wins.terminal or wins.db
  if right then
    sizes.right = api.nvim_win_get_width(right)
  end
  if wins.claude and wins.terminal then
    local a, b = api.nvim_win_get_height(wins.claude), api.nvim_win_get_height(wins.terminal)
    sizes.right_split = a / (a + b)
  end
  local strip = require("workbench").bottom_wins()
  if #strip > 0 then
    sizes.bottom = api.nvim_win_get_height(strip[1])
  end
  return { panels = panels, sizes = sizes }
end

--- Düzeni yakalar ve kaydeder; `dir` yakalama planlandığındaki cwd (değiştiyse atlanır).
---@param dir string
local function capture(dir)
  state.pending = false
  local exiting = vim.v.exiting
  if state.busy or (exiting ~= nil and exiting ~= vim.NIL) or vim.fn.getcwd() ~= dir then
    return
  end
  local key = M.key()
  if not key then
    return
  end
  local snap = snapshot()
  local old = read().layouts[key]
  if old and vim.deep_equal(old.panels, snap.panels) and vim.deep_equal(old.sizes, snap.sizes) then
    return
  end
  snap.time = os.time()
  read().layouts[key] = snap
  write()
end

--- Pencere olayından sonra yakalamayı bir sonraki döngüye planlar.
function M.schedule_capture()
  if state.busy or state.pending then
    return
  end
  state.pending = true
  local dir = vim.fn.getcwd()
  vim.schedule(function()
    capture(dir)
  end)
end

--- Kayıt anahtarı: düzen kaydedilebilir bir proje/workspace ise cwd, değilse nil. `autorestore` doldurur.
---@type fun(): string?
M.key = function()
  return nil
end

--- Düzen kurulurken yakalamayı durdurur (proje geçişi başladı).
function M.pause()
  state.busy = true
end

--- Kayıtlı düzen (yoksa nil).
---@return { panels: string[], sizes: table }?
function M.saved()
  local key = M.key()
  return key and read().layouts[key] or nil
end

--- Panelleri açar; `exact` ise listede olmayan açık panelleri kapatır.
---@param want string[]
---@param exact boolean
function M.open(want, exact)
  local set = {}
  for _, name in ipairs(want) do
    set[name] = true
  end
  if exact then
    for _, p in ipairs(PANELS) do
      local w = not set[p.name] and p.find()
      if w then
        pcall(p.close, w)
      end
    end
  end
  for _, p in ipairs(PANELS) do
    if set[p.name] and not p.find() then
      local ok, err = pcall(p.open)
      if not ok then
        vim.notify(("Could not open %s panel: %s"):format(p.name, err), vim.log.levels.WARN)
      end
    end
  end
end

--- Kayıtlı boyutları uygular (sınırları `workbench.arrange_layout` yine uygular).
---@param sizes table
function M.apply_sizes(sizes)
  local api = vim.api
  local wins = {}
  for _, p in ipairs(PANELS) do
    wins[p.name] = p.find() or nil
  end
  local left = wins.files or wins.git
  if left and sizes.left then
    pcall(api.nvim_win_set_width, left, sizes.left)
  end
  local right = wins.claude or wins.terminal or wins.db
  if right and sizes.right then
    pcall(api.nvim_win_set_width, right, sizes.right)
  end
  if wins.claude and wins.terminal and sizes.right_split then
    local total = api.nvim_win_get_height(wins.claude) + api.nvim_win_get_height(wins.terminal)
    pcall(api.nvim_win_set_height, wins.claude, math.floor(total * sizes.right_split + 0.5))
  end
  local strip = require("workbench").bottom_wins()
  if #strip > 0 and sizes.bottom then
    pcall(api.nvim_win_set_height, strip[1], sizes.bottom)
  end
  require("workbench").arrange_layout()
end

--- Düzen kuruldu: yakalama yeniden açılır (kurulumun kendi olayları bittikten sonra).
function M.resume()
  vim.schedule(function()
    state.busy = false
    M.schedule_capture()
  end)
end

function M.setup()
  vim.api.nvim_create_autocmd({ "WinNew", "WinClosed", "WinResized", "BufWinEnter" }, {
    group = vim.api.nvim_create_augroup("autorestore_layout", { clear = true }),
    callback = M.schedule_capture,
  })
end

return M
