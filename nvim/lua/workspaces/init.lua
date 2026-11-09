local M = {}

---@class Workspace
---@field name string
---@field roots string[] kök klasörlerin gerçek (symlink'i çözülmüş) mutlak yolları
---@field opened integer? son açılış zamanı (os.time)

---@class WorkspacesConfig
---@field file string kayıt dosyası (JSON)
---@field links string workspace başına symlink klasörlerinin tutulduğu üst klasör
---@field restore_session boolean workspace açılınca kayıtlı oturumunu yükle

---@type WorkspacesConfig
M.config = {
  file = vim.fn.stdpath("data") .. "/workspaces.json",
  links = vim.fn.stdpath("data") .. "/workspaces",
  restore_session = true,
}

local cache = { stamp = nil, list = nil, links = nil }

--- Yolu mutlak, `~`'suz, sonda `/` olmayan ve (varsa) symlink'leri çözülmüş hale getirir.
---@param path string
---@return string
local function normalize(path)
  path = vim.fs.normalize(vim.fn.fnamemodify(vim.fn.expand(path), ":p"))
  return vim.uv.fs_realpath(path) or path
end
M.normalize = normalize

--- Symlink klasörlerinin üst klasörünü normalize edilmiş haliyle döndürür (klasör ilk workspace açılınca oluşur).
---@return string
local function links_root()
  if not cache.links then
    local links = vim.fs.normalize(vim.fn.fnamemodify(vim.fn.expand(M.config.links), ":p"))
    cache.links = normalize(vim.fs.dirname(links)) .. "/" .. vim.fs.basename(links)
  end
  return cache.links
end

--- Workspace adından klasör adı üretir (harf, rakam, `-`, `_`, `.` dışındakiler `-` olur).
---@param name string
---@return string
local function slug(name)
  local s = name:gsub("[^%w%-_%.]", "-")
  return (s == "" or s == "." or s == "..") and "workspace" or s
end

--- Kayıt dosyasını okur; dosya değişmediyse bellekteki kopyayı döndürür.
---@return Workspace[]
local function load()
  local stat = vim.uv.fs_stat(M.config.file)
  local stamp = stat and (stat.mtime.sec .. "." .. stat.mtime.nsec .. ":" .. stat.size) or "missing"
  if cache.list and cache.stamp == stamp then
    return cache.list
  end
  local list = {}
  if stat then
    local fd = io.open(M.config.file, "r")
    local text = fd and fd:read("*a") or ""
    if fd then
      fd:close()
    end
    local ok, data = pcall(vim.json.decode, text)
    if ok and type(data) == "table" and type(data.workspaces) == "table" then
      for _, ws in ipairs(data.workspaces) do
        if type(ws) == "table" and type(ws.name) == "string" and type(ws.roots) == "table" then
          table.insert(list, { name = ws.name, roots = ws.roots, opened = tonumber(ws.opened) })
        end
      end
    else
      vim.notify("Failed to read workspace registry: " .. M.config.file, vim.log.levels.ERROR)
    end
  end
  cache.list, cache.stamp = list, stamp
  return list
end

--- Listeyi kayıt dosyasına yazar. Elle düzenlenebilsin diye her workspace ayrı satırdadır.
---@param list Workspace[]
local function save(list)
  local lines = { "{", '  "version": 1,', '  "workspaces": [' }
  for i, ws in ipairs(list) do
    local entry = vim.json.encode({ name = ws.name, roots = ws.roots, opened = ws.opened }, { escape_slash = false })
    table.insert(lines, "    " .. entry .. (i < #list and "," or ""))
  end
  vim.list_extend(lines, { "  ]", "}" })
  vim.fn.mkdir(vim.fs.dirname(M.config.file), "p")
  vim.fn.writefile(lines, M.config.file)
  cache.list, cache.stamp = nil, nil
end

--- Kayıtlı workspace'leri son açılana göre sıralı döndürür.
---@return Workspace[]
function M.list()
  local list = vim.deepcopy(load())
  table.sort(list, function(a, b)
    if (a.opened or 0) ~= (b.opened or 0) then
      return (a.opened or 0) > (b.opened or 0)
    end
    return a.name:lower() < b.name:lower()
  end)
  return list
end

--- Adı verilen workspace'i (kayıt listesindeki indisiyle) döndürür.
---@param name string
---@return Workspace?, integer?
local function find(name)
  for i, ws in ipairs(load()) do
    if ws.name == name then
      return ws, i
    end
  end
end

--- Adı verilen adla çakışan (aynı klasör adı, büyük/küçük harf duyarsız) kayıtlı workspace.
---@param name string
---@return Workspace?
function M.existing(name)
  local key = slug(vim.trim(name)):lower()
  for _, ws in ipairs(load()) do
    if slug(ws.name):lower() == key then
      return vim.deepcopy(ws)
    end
  end
end

---@param name string
---@return Workspace?
function M.get(name)
  return vim.deepcopy((find(name)))
end

--- Kayıt listesini değiştirip kaydeder. `fn` hata mesajı döndürürse hiçbir şey yazılmaz.
---@param fn fun(list: Workspace[]): string?
---@return boolean
local function update(fn)
  local list = vim.deepcopy(load())
  local err = fn(list)
  if err then
    vim.notify(err, vim.log.levels.WARN)
    return false
  end
  save(list)
  return true
end

--- Ad boş olmamalı ve klasör adı (slug) başka bir workspace'inkiyle çakışmamalı (macOS büyük/küçük harf duyarsız).
---@param list Workspace[]
---@param name string
---@param except string?
---@return string? err
local function validate_name(list, name, except)
  if vim.trim(name) == "" then
    return "Workspace name cannot be empty"
  end
  for _, ws in ipairs(list) do
    if ws.name ~= except and slug(ws.name):lower() == slug(name):lower() then
      return "A workspace with this name already exists: " .. ws.name
    end
  end
end

--- Workspace'in symlink klasörünün yolu (Neovim'in workspace'teki çalışma klasörü).
---@param ws Workspace|string
---@return string
function M.link_dir(ws)
  return links_root() .. "/" .. slug(type(ws) == "table" and ws.name or ws)
end

--- Köklere symlink klasöründe kullanılacak adları verir: klasör adı; çakışırsa `ad-2`, `ad-3`...
---@param roots string[]
---@return { name: string, root: string }[]
local function entries(roots)
  local used, out = {}, {}
  for _, root in ipairs(roots) do
    local base = vim.fs.basename(root)
    local name, i = base, 1
    while used[name:lower()] do
      i = i + 1
      name = base .. "-" .. i
    end
    used[name:lower()] = true
    table.insert(out, { name = name, root = root })
  end
  return out
end

--- Symlink klasörünü kayıtla eşitler: fazla/yanlış symlink'leri siler, eksikleri oluşturur.
--- Symlink olmayan dosyalara dokunmaz.
---@param ws Workspace
---@return string dir
local function sync(ws)
  local dir = M.link_dir(ws)
  vim.fn.mkdir(dir, "p")
  local want = {}
  for _, e in ipairs(entries(ws.roots)) do
    want[e.name] = e.root
  end
  local existing = {}
  for name, kind in vim.fs.dir(dir) do
    existing[name] = kind
  end
  for name, kind in pairs(existing) do
    if kind == "link" and vim.uv.fs_readlink(dir .. "/" .. name) ~= want[name] then
      vim.uv.fs_unlink(dir .. "/" .. name)
      existing[name] = nil
    end
  end
  for name, root in pairs(want) do
    if not existing[name] then
      vim.uv.fs_symlink(root, dir .. "/" .. name)
    end
  end
  return dir
end

--- Çalışma klasörü bir workspace'in symlink klasörüyse o workspace'i döndürür.
--- neo-tree'nin git geri çağrılarından (fast event) da çağrıldığı için `vim.fn` kullanmaz.
---@return Workspace?
function M.current()
  local cwd = vim.in_fast_event() and vim.uv.cwd() or vim.fn.getcwd()
  cwd = vim.uv.fs_realpath(cwd) or cwd
  if vim.fs.dirname(cwd) ~= links_root() then
    return nil
  end
  local name = vim.fs.basename(cwd)
  for _, ws in ipairs(load()) do
    if slug(ws.name) == name then
      return vim.deepcopy(ws)
    end
  end
end

--- Açık workspace'in diskte var olan kökleri; workspace açık değilse nil.
---@return string[]?
function M.roots()
  local ws = M.current()
  if not ws then
    return nil
  end
  return vim.tbl_filter(function(root)
    local stat = vim.uv.fs_stat(root)
    return stat ~= nil and stat.type == "directory"
  end, ws.roots)
end

--- Klasör bir workspace'in symlink klasörü mü (persistence oturum listesinde workspace'leri ayırmak için).
---@param dir string
---@return boolean
function M.is_link_dir(dir)
  return vim.fs.dirname(dir) == links_root()
end

--- Arama/tarama yapılacak klasörler: workspace açıksa kökleri, değilse çalışma klasörü.
---@return string[]
function M.dirs()
  return M.roots() or { vim.fn.getcwd() }
end

--- Symlink klasörü üzerinden verilen bir yolu gerçek yoluna çevirir; değilse aynen döndürür.
---@param path string
---@return string
function M.to_real(path)
  local base = links_root()
  local ws_slug, name, rest = path:match("^" .. vim.pesc(base) .. "/([^/]+)/([^/]+)(.*)$")
  if not ws_slug then
    return path
  end
  for _, ws in ipairs(load()) do
    if slug(ws.name) == ws_slug then
      for _, e in ipairs(entries(ws.roots)) do
        if e.name == name then
          return e.root .. rest
        end
      end
    end
  end
  local real = vim.uv.fs_realpath(base .. "/" .. ws_slug .. "/" .. name)
  return real and (real .. rest) or path
end

--- Açık workspace'in köklerinden birinin altındaki gerçek yolu symlink klasöründeki karşılığına çevirir
--- (neo-tree ağacındaki düğüm kimliği). Eşleşme yoksa aynen döndürür.
---@param path string
---@return string
function M.to_link(path)
  local ws = M.current()
  if not ws then
    return path
  end
  local best
  for _, e in ipairs(entries(ws.roots)) do
    if (path == e.root or vim.startswith(path, e.root .. "/")) and (not best or #e.root > #best.root) then
      best = e
    end
  end
  return best and (M.link_dir(ws) .. "/" .. best.name .. path:sub(#best.root + 1)) or path
end

--- Snacks picker kaynak `config` kancası: workspace klasöründe (ya da `cwd` olarak workspace klasörü verilmişse)
--- dosya/metin araması symlink'leri izlemek yerine tüm kökleri gerçek yollarıyla arar.
---@param opts snacks.picker.Config
---@return snacks.picker.Config?
function M.picker_config(opts)
  if (opts.dirs and #opts.dirs > 0) or opts.buffers or opts.rtp then
    return
  end
  local ws = M.current()
  if not ws or normalize(opts.cwd or vim.fn.getcwd()) ~= M.link_dir(ws) then
    return
  end
  opts.dirs = M.roots()
  return opts
end

--- persistence.nvim'in verilen klasör için kullandığı oturum dosyasının yolu.
---@param dir string
---@return string?
local function session_file(dir)
  local ok, cfg = pcall(require, "persistence.config")
  if not ok or not cfg.options or not cfg.options.dir then
    return nil
  end
  return cfg.options.dir .. dir:gsub("[\\/:]+", "%%") .. ".vim"
end

--- neo-tree yüklüyse dosya ağacını verilen köke taşır; `show` true ise kapalı paneli de açar (odaklamadan).
---@param dir string
---@param show boolean?
local function tree_to(dir, show)
  if not package.loaded["neo-tree"] and not show then
    return
  end
  local state = require("neo-tree.sources.manager").get_state("filesystem")
  if show or require("neo-tree.ui.renderer").window_exists(state) then
    require("neo-tree.command").execute({ action = "show", source = "filesystem", dir = dir })
  elseif state.path ~= dir then
    state.path, state.dirty = nil, true
  end
end

--- Açık neo-tree ağacını yeniler (kök eklenip çıkarılınca).
local function refresh_tree()
  if package.loaded["neo-tree"] then
    require("neo-tree.sources.manager").refresh("filesystem")
  end
end

--- Kaydedilmiş dosya buffer'larını döndürür (terminal, panel vb. hariç).
---@return integer[]
local function file_buffers()
  return vim.tbl_filter(function(b)
    return vim.bo[b].buflisted and vim.bo[b].buftype == "" and vim.api.nvim_buf_get_name(b) ~= ""
  end, vim.api.nvim_list_bufs())
end

--- Mevcut projeden çıkmadan önce: kaydedilmemiş dosya varsa vazgeçer; oturumu kaydeder ve dosya buffer'larını kapatır.
--- Terminaller (Claude, shell, Run logları) açık kalır.
---@return boolean
local function leave_current()
  local bufs = file_buffers()
  for _, b in ipairs(bufs) do
    if vim.bo[b].modified then
      vim.notify("Unsaved file: " .. vim.fn.fnamemodify(vim.api.nvim_buf_get_name(b), ":~"), vim.log.levels.WARN)
      return false
    end
  end
  if package.loaded.persistence and require("persistence").active() and #bufs > 0 then
    pcall(vim.cmd, "Neotree close")
    local ok, err = pcall(require("persistence").save)
    if not ok then
      vim.notify("Failed to save session: " .. tostring(err), vim.log.levels.WARN)
    end
  end
  for _, b in ipairs(bufs) do
    pcall(vim.api.nvim_buf_delete, b, {})
  end
  return true
end

--- Workspace'i açar (klasörü yoksa önce klasör ekleme seçicisi açılır, eklenince workspace açılır): symlink klasörünü eşitler, çalışma klasörünü oraya taşır, varsa oturumunu yükler
--- ve dosya panelinde tüm kökleri gösterir.
---@param name string
function M.open(name)
  local ws = find(name)
  if not ws then
    vim.notify("Workspace not found: " .. name, vim.log.levels.WARN)
    return
  end
  local missing = vim.tbl_filter(function(r)
    return vim.fn.isdirectory(r) == 0
  end, ws.roots)
  if #missing == #ws.roots then
    vim.notify("Workspace “" .. name .. "” has no folders yet — choose a folder to add", vim.log.levels.INFO)
    require("workspaces.ui").add_folder(name, nil, function()
      local w = find(name)
      if w and vim.iter(w.roots):any(function(r) return vim.fn.isdirectory(r) == 1 end) then
        M.open(name)
      end
    end)
    return
  end
  if #missing > 0 then
    vim.notify("Skipped missing folders:\n" .. table.concat(missing, "\n"), vim.log.levels.WARN)
  end
  local dir = sync(ws)
  if normalize(vim.fn.getcwd()) ~= dir then
    if not leave_current() then
      return
    end
    vim.cmd.cd(vim.fn.fnameescape(dir))
    local session = session_file(dir)
    if M.config.restore_session and session and vim.fn.filereadable(session) == 1 then
      require("persistence").load()
    end
  end
  update(function(list)
    for _, w in ipairs(list) do
      if w.name == name then
        w.opened = os.time()
      end
    end
  end)
  tree_to(dir, true)
  vim.notify("Workspace opened: " .. name)
end

--- Açık workspace'ten çıkar: oturumu kaydeder ve çalışma klasörünü ilk köke taşır.
function M.close()
  local ws = M.current()
  if not ws then
    vim.notify("No workspace is open", vim.log.levels.INFO)
    return
  end
  if not leave_current() then
    return
  end
  local target = M.roots()[1] or vim.env.HOME
  vim.cmd.cd(vim.fn.fnameescape(target))
  tree_to(target)
end

--- Yeni workspace oluşturur.
---@param name string
---@param roots string[]?
---@return boolean
function M.create(name, roots)
  name = vim.trim(name)
  return update(function(list)
    local err = validate_name(list, name)
    if err then
      return err
    end
    table.insert(list, { name = name, roots = {} })
  end) and vim.iter(roots or {}):all(function(r)
    return M.add_root(name, r)
  end)
end

--- Workspace'e kök klasör ekler; workspace açıksa ağaç hemen güncellenir.
---@param name string
---@param path string
---@return boolean
function M.add_root(name, path)
  local root = normalize(path)
  if vim.fn.isdirectory(root) == 0 then
    vim.notify("Not a folder: " .. path, vim.log.levels.WARN)
    return false
  end
  local ok = update(function(list)
    for _, ws in ipairs(list) do
      if ws.name == name then
        if vim.tbl_contains(ws.roots, root) then
          return "Folder already added: " .. vim.fn.fnamemodify(root, ":~")
        end
        if vim.startswith(root, links_root() .. "/") then
          return "A workspace folder cannot be added as a root"
        end
        table.insert(ws.roots, root)
        return
      end
    end
    return "Workspace not found: " .. name
  end)
  if ok then
    M.after_change(name)
  end
  return ok
end

--- Workspace'ten kök klasörü çıkarır (diskteki klasöre dokunmaz).
---@param name string
---@param root string
---@return boolean
function M.remove_root(name, root)
  local ok = update(function(list)
    for _, ws in ipairs(list) do
      if ws.name == name then
        local n = #ws.roots
        ws.roots = vim.tbl_filter(function(r)
          return r ~= root
        end, ws.roots)
        return n == #ws.roots and ("No such folder in workspace: " .. root) or nil
      end
    end
    return "Workspace not found: " .. name
  end)
  if ok then
    M.after_change(name)
  end
  return ok
end

--- Kök listesi değişince: workspace açıksa symlink klasörünü eşitler ve ağacı yeniler.
---@param name string
function M.after_change(name)
  local ws = find(name)
  if not ws or vim.fn.isdirectory(M.link_dir(ws)) == 0 then
    return
  end
  sync(ws)
  local cur = M.current()
  if cur and cur.name == name then
    refresh_tree()
  end
end

--- Symlink klasörünü (yalnızca içindeki symlink'ler ve boşsa klasörün kendisi) siler.
---@param dir string
local function remove_link_dir(dir)
  if vim.fn.isdirectory(dir) == 0 then
    return
  end
  for name, kind in vim.fs.dir(dir) do
    if kind == "link" then
      vim.uv.fs_unlink(dir .. "/" .. name)
    end
  end
  vim.uv.fs_rmdir(dir)
end

--- Workspace'i yeniden adlandırır; symlink klasörü ve oturum dosyası da taşınır, açıksa yeni klasöre geçilir.
---@param old string
---@param new string
---@return boolean
function M.rename(old, new)
  new = vim.trim(new)
  if old == new then
    return true
  end
  local cur = M.current()
  local old_dir = M.link_dir(old)
  local ok = update(function(list)
    local err = validate_name(list, new, old)
    if err then
      return err
    end
    for _, ws in ipairs(list) do
      if ws.name == old then
        ws.name = new
        return
      end
    end
    return "Workspace not found: " .. old
  end)
  if not ok then
    return false
  end
  local new_dir = M.link_dir(new)
  if old_dir ~= new_dir then
    remove_link_dir(old_dir)
    local s_old, s_new = session_file(old_dir), session_file(new_dir)
    if s_old and vim.uv.fs_stat(s_old) then
      vim.uv.fs_rename(s_old, s_new)
    end
    if cur and cur.name == old then
      vim.cmd.cd(vim.fn.fnameescape(sync(find(new) --[[@as Workspace]])))
      tree_to(new_dir)
    end
  end
  return true
end

--- Workspace kaydını siler (klasörlere dokunmaz); symlink klasörü ve workspace oturumu da silinir.
---@param name string
---@return boolean
function M.delete(name)
  local cur = M.current()
  if cur and cur.name == name then
    M.close()
    if M.current() then
      return false
    end
  end
  local dir = M.link_dir(name)
  local ok = update(function(list)
    for i, ws in ipairs(list) do
      if ws.name == name then
        table.remove(list, i)
        return
      end
    end
    return "Workspace not found: " .. name
  end)
  if ok then
    remove_link_dir(dir)
    local session = session_file(dir)
    if session then
      vim.uv.fs_unlink(session)
    end
  end
  return ok
end

--- Çalışma klasörü bir workspace klasörüne geçince (açılış, oturum yükleme, `:cd`) symlink'leri eşitler,
--- neo-tree yamalarını kurar ve ağacı workspace'e taşır.
local function on_enter()
  local ws = M.current()
  if not ws then
    return
  end
  sync(find(ws.name) --[[@as Workspace]])
  if package.loaded["neo-tree"] then
    tree_to(M.link_dir(ws))
  end
end

--- `:Workspace` alt komutları.
local subcommands = {
  open = function(arg)
    if arg == "" then
      require("workspaces.ui").pick()
    else
      M.open(arg)
    end
  end,
  new = function(arg)
    require("workspaces.ui").new(arg ~= "" and arg or nil)
  end,
  add = function(arg)
    require("workspaces.ui").add_folder(nil, arg ~= "" and arg or nil)
  end,
  remove = function()
    require("workspaces.ui").remove_folder()
  end,
  rename = function(arg)
    require("workspaces.ui").rename(nil, arg ~= "" and arg or nil)
  end,
  delete = function(arg)
    require("workspaces.ui").delete(arg ~= "" and arg or nil)
  end,
  manage = function(arg)
    require("workspaces.ui").manage(arg ~= "" and arg or nil)
  end,
  close = function()
    M.close()
  end,
}

--- `:Workspace` için tamamlama: alt komutlar, workspace adları ve (add için) klasörler.
---@param lead string
---@param line string
---@return string[]
local function complete(lead, line)
  local sub = line:match("^%S+%s+(%S+)%s")
  if not sub then
    return vim.tbl_filter(function(s)
      return vim.startswith(s, lead)
    end, vim.tbl_keys(subcommands))
  end
  if sub == "add" then
    return vim.fn.getcompletion(lead, "dir")
  end
  if sub == "open" or sub == "delete" or sub == "manage" then
    return vim.tbl_filter(function(n)
      return vim.startswith(n, lead)
    end, vim.tbl_map(function(ws)
      return ws.name
    end, M.list()))
  end
  return {}
end

--- Workspace komutunu ve otomatik komutları kurar.
--- Ortam değişkenleri (testler için): `NVIM_WORKSPACES_FILE` kayıt dosyası, `NVIM_WORKSPACES_DIR` symlink klasörleri.
---@param opts WorkspacesConfig?
function M.setup(opts)
  M.config = vim.tbl_extend("force", M.config, opts or {})
  M.config.file = vim.env.NVIM_WORKSPACES_FILE or M.config.file
  M.config.links = vim.env.NVIM_WORKSPACES_DIR or M.config.links
  cache.list, cache.stamp, cache.links = nil, nil, nil
  links_root()

  vim.api.nvim_create_user_command("Workspace", function(cmd)
    local sub, arg = cmd.args:match("^(%S*)%s*(.-)%s*$")
    if sub == "" then
      return require("workspaces.ui").pick()
    end
    local fn = subcommands[sub]
    if not fn then
      vim.notify("Unknown subcommand: " .. sub, vim.log.levels.WARN)
      return
    end
    fn(arg)
  end, { nargs = "*", complete = complete, desc = "Workspaces (multi-root)" })

  local group = vim.api.nvim_create_augroup("workspaces", { clear = true })
  vim.api.nvim_create_autocmd("DirChanged", {
    group = group,
    pattern = { "global", "tabpage", "window" },
    callback = vim.schedule_wrap(on_enter),
  })
  vim.api.nvim_create_autocmd("User", {
    group = group,
    pattern = "PersistenceSavePre",
    callback = function()
      if M.current() then
        pcall(vim.cmd, "Neotree close")
      end
    end,
  })
  if M.current() then
    vim.schedule(on_enter)
  end
end

return M
