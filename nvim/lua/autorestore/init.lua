local M = {}

---@class AutorestoreRecord
---@field version integer
---@field kind "project"|"workspace"
---@field dir string proje klasörü (workspace için symlink klasörü)
---@field name string? workspace adı
---@field time integer os.time()

---@class AutorestoreConfig
---@field file string son proje kaydı (JSON)
---@field recents string son açılan projeler listesi (JSON; LESS.app File > Open Recent menüsü okur)
---@field markers string[] bir klasörü "proje" yapan işaretçiler (`.git` her zaman önceliklidir)
---@field default_layout string[] proje açılınca açılacak paneller (`vim.g.default_layout` ezer)
---@field ignore string[] bu öneklerle başlayan klasörler "son proje" olarak kaydedilmez (geçici klasörler, testler)

---@type AutorestoreConfig
M.config = {
  file = vim.fn.stdpath("state") .. "/autorestore.json",
  recents = vim.fn.stdpath("state") .. "/recents.json",
  markers = {
    ".git",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "mvnw",
    "package.json",
    "go.mod",
    "Cargo.toml",
    "pyproject.toml",
  },
  default_layout = { "files", "claude" },
  ignore = { "/tmp/", "/private/tmp/", "/var/folders/", "/private/var/folders/" },
}

local state = {
  clean_launch = false,
  has_ui = false,
  restoring = false,
  layout_pending = false,
}

--- Yolu mutlak, sonda `/` olmayan ve symlink'leri çözülmüş hale getirir.
---@param path string
---@return string
local function realpath(path)
  path = vim.fs.normalize(vim.fn.fnamemodify(path, ":p"))
  return vim.uv.fs_realpath(path) or path
end

---@param path string
---@return boolean
local function is_dir(path)
  local stat = vim.uv.fs_stat(path)
  return stat ~= nil and stat.type == "directory"
end

--- Özellik açık mı: `vim.g.autorestore = false` ya da `NVIM_AUTORESTORE=0` kapatır.
---@return boolean
local function enabled()
  return vim.g.autorestore ~= false and vim.env.NVIM_AUTORESTORE ~= "0" and vim.env.NVIM_NO_RESTORE == nil
end

--- `dir` klasörünün kendisinde bir proje işaretçisi var mı?
---@param dir string
---@return boolean
local function has_marker(dir)
  for _, m in ipairs(M.config.markers) do
    if vim.uv.fs_stat(dir .. "/" .. m) then
      return true
    end
  end
  return false
end

--- `dir`'in içinde bulunduğu projenin kökü: yukarı doğru en yakın `.git` klasörü, yoksa en yakın işaretçili klasör.
--- Arama ev klasöründe ve `/`'de durur; ev klasörü ve `/` proje sayılmaz (ör. dotfiles reposu olan `~`).
---@param dir string
---@return string?
function M.project_root(dir)
  local home = realpath(vim.env.HOME or "~")
  local nearest
  for _, d in ipairs(vim.list_extend({ dir }, vim.iter(vim.fs.parents(dir)):totable())) do
    if d == home or d == "/" then
      break
    end
    if vim.uv.fs_stat(d .. "/.git") then
      return d
    end
    nearest = nearest or (has_marker(d) and d or nil)
  end
  return nearest
end

--- workspaces modülü yüklüyse açık (cwd'si workspace klasörü olan) workspace.
---@return Workspace?
local function current_workspace()
  local ok, ws = pcall(require, "workspaces")
  return ok and ws.current() or nil
end

--- Çalışma klasörü kayda değer bir proje mi: workspace klasörü ya da kendisinde işaretçi olan klasör
--- (ev klasörü hariç).
---@param cwd string
---@return AutorestoreRecord?
local function record_for(cwd)
  local ws = current_workspace()
  if ws then
    return { version = 1, kind = "workspace", name = ws.name, dir = cwd, time = os.time() }
  end
  if cwd ~= realpath(vim.env.HOME or "~") and cwd ~= "/" and has_marker(cwd) then
    return { version = 1, kind = "project", dir = cwd, time = os.time() }
  end
end

--- Son proje kaydını okur; dosya yoksa ya da bozuksa nil.
---@return AutorestoreRecord?
function M.read()
  local fd = io.open(M.config.file, "r")
  if not fd then
    return nil
  end
  local text = fd:read("*a")
  fd:close()
  local ok, data = pcall(vim.json.decode, text)
  if not ok or type(data) ~= "table" or type(data.dir) ~= "string" then
    return nil
  end
  if data.kind == "project" or data.kind == "workspace" then
    return data
  end
end

--- Çalışma klasörü bir projeyse onu "son proje" olarak yazar. Yalnızca argümansız (ya da tek klasör argümanlı)
--- açılışlarda, bu oturuma bir arayüz bağlanmışsa ve özellik açıkken (`vim.g.autorestore`, `NVIM_AUTORESTORE=0`,
--- `NVIM_NO_RESTORE`) yazılır: `nvim dosya.txt`, git commit editörü, headless betikler ve geri yüklemesi kapalı
--- açılışlar kaydı ezmez. `<leader>qd` ile oturum kaydı
--- durdurulduysa da yazılmaz. Arayüz şu an değil açılışta aranır: pencere kapatılınca TUI `VimLeavePre`'den önce ayrılır.
function M.remember()
  if not state.clean_launch or not enabled() or not state.has_ui then
    return
  end
  if package.loaded.persistence and not require("persistence").active() then
    return
  end
  local rec = record_for(realpath(vim.fn.getcwd()))
  if not rec then
    return
  end
  for _, prefix in ipairs(M.config.ignore) do
    if vim.startswith(rec.dir .. "/", prefix) then
      return
    end
  end
  vim.fn.mkdir(vim.fs.dirname(M.config.file), "p")
  pcall(vim.fn.writefile, { vim.json.encode(rec, { escape_slash = false }) }, M.config.file)
  M.add_recent(rec)
end

--- Kaydı son açılanlar listesinin başına koyar (aynı klasör tekrar etmez, en fazla 15).
--- Dosya `{ version = 1, items = AutorestoreRecord[] }`; LESS.app'in File > Open Recent menüsü bunu okur.
---@param rec AutorestoreRecord
function M.add_recent(rec)
  local items = {}
  local ok, data = pcall(function()
    return vim.json.decode(table.concat(vim.fn.readfile(M.config.recents), "\n"))
  end)
  for _, it in ipairs(ok and type(data) == "table" and data.items or {}) do
    if type(it) == "table" and it.dir ~= rec.dir and #items < 14 then
      items[#items + 1] = it
    end
  end
  table.insert(items, 1, rec)
  pcall(vim.fn.writefile, { vim.json.encode({ version = 1, items = items }, { escape_slash = false }) }, M.config.recents)
end

--- LESS penceresi kaydı: `stdpath("state")/instances/<pid>` (yalnız `LESS_APP=1`). Ölü kayıtları siler, kendini yazar,
--- çıkışta siler. Başka canlı LESS penceresi varsa true: o zaman yeni pencere son projeyi değil dashboard'u açar
--- (menüden / `lessed`'den klasörle açılınca `LESS_OPEN_DIR` bunu aşar).
---@return boolean
function M.register_instance()
  if vim.env.LESS_APP ~= "1" then
    return false
  end
  local dir = vim.fn.stdpath("state") .. "/instances"
  vim.fn.mkdir(dir, "p")
  local me, others = vim.fn.getpid(), false
  for name in vim.fs.dir(dir) do
    local pid = tonumber(name)
    if pid and pid ~= me and vim.uv.kill(pid, 0) == 0 then
      others = true
    elseif pid ~= me then
      os.remove(dir .. "/" .. name)
    end
  end
  local own = ("%s/%d"):format(dir, me)
  vim.fn.writefile({}, own)
  vim.api.nvim_create_autocmd("VimLeavePre", {
    callback = function()
      os.remove(own)
    end,
  })
  return others
end

--- `dir` için persistence oturum dosyası var mı (dallı ya da dalsız)?
---@param dir string
---@return boolean
local function has_session(dir)
  local ok, cfg = pcall(require, "persistence.config")
  if not ok or not (cfg.options and cfg.options.dir) then
    return false
  end
  local base = cfg.options.dir .. dir:gsub("[\\/:]+", "%%")
  if vim.fn.filereadable(base .. ".vim") == 1 then
    return true
  end
  return #vim.fn.glob(vim.fn.escape(base, "[]*?{}") .. "%%*.vim", true, true) > 0
end

--- Oturumu yükler. Yükleme sırasında başka bir Neovim'e ait olmayan swap dosyası çıkarsa soru sormadan dosyayı
--- salt okunur açar (açılış bir istemde takılmasın), kullanıcıyı uyarır.
local function load_session()
  local swapped = {}
  local id = vim.api.nvim_create_autocmd("SwapExists", {
    callback = function(ev)
      if vim.v.swapchoice == "" then
        vim.v.swapchoice = "o"
        table.insert(swapped, vim.fn.fnamemodify(ev.file, ":~:."))
      end
    end,
  })
  local ok, err = pcall(function()
    require("persistence").load()
  end)
  vim.api.nvim_del_autocmd(id)
  pcall(function()
    require("workbench").restore_pins()
  end)
  if not ok then
    vim.notify("Session restore failed: " .. tostring(err), vim.log.levels.WARN)
  end
  if #swapped > 0 then
    vim.notify(
      "Swap file found, opened read-only (use :recover to restore):\n" .. table.concat(swapped, "\n"),
      vim.log.levels.WARN
    )
  end
end

--- Workspace'i açar. cwd zaten workspace klasörüyse `workspaces.open` oturumu yüklemediği için önce oturum yüklenir.
---@param name string
---@return boolean
local function open_workspace(name)
  local ws = require("workspaces")
  local item = ws.get(name)
  if not item or not vim.iter(item.roots):any(is_dir) then
    return false
  end
  if realpath(vim.fn.getcwd()) == ws.link_dir(item) then
    load_session()
  end
  ws.open(name)
  return ws.current() ~= nil
end

--- Proje klasörüne geçip oturumunu yükler (proje seçicisi de kullanır: `workspaces.ui.projects`).
---@param dir string
---@return boolean
local function open_project(dir)
  if not is_dir(dir) then
    return false
  end
  if realpath(vim.fn.getcwd()) ~= dir then
    vim.cmd.cd(vim.fn.fnameescape(dir))
  end
  load_session()
  return true
end

M.open_project = open_project

--- Açılışta ne açılacağını belirler:
--- 1. cwd bir workspace klasörü → o workspace;
--- 2. cwd bir proje içinde (kendisinde ya da üst klasörlerinde işaretçi; ev klasörü hariç) → projenin kökü;
--- 3. cwd'nin kendi oturumu var → cwd;
--- 4. aksi halde (ev klasörü, ilgisiz bir yer) → son kayıtlı proje/workspace.
---@return { kind: "project"|"workspace", dir: string, name: string? }?
function M.target()
  local cwd = realpath(vim.fn.getcwd())
  local ws = current_workspace()
  if ws then
    return { kind = "workspace", name = ws.name, dir = cwd }
  end
  local root = M.project_root(cwd)
  if root then
    return { kind = "project", dir = root }
  end
  if cwd ~= realpath(vim.env.HOME or "~") and has_session(cwd) then
    return { kind = "project", dir = cwd }
  end
  return M.read()
end

--- Bu argümanlardan biriyle başlatılan Neovim otomatik açma yapmaz (komut, oturum, quickfix, diff, stdin, headless).
local skip_flags = { "-c", "-S", "-q", "-t", "-d", "-b", "-", "-es", "-Es", "--headless" }

--- `nvim` dashboard'un açılacağı biçimde mi başlatıldı: argüman, `+komut`/`-c`, stdin, headless yok; yüzen olmayan tek
--- pencerede boş ilk buffer var (açılışta lazy.nvim'in kurulum penceresi gibi yüzen pencereler sayılmaz).
---@return boolean
local function plain_start()
  if vim.fn.argc(-1) > 0 then
    return false
  end
  for i, a in ipairs(vim.v.argv) do
    if i > 1 and (a:sub(1, 1) == "+" or vim.tbl_contains(skip_flags, a)) then
      return false
    end
  end
  local uis = vim.api.nvim_list_uis()
  if #uis == 0 or (uis[1].stdout_tty and not uis[1].stdin_tty) then
    return false
  end
  local wins = vim.tbl_filter(function(w)
    return vim.api.nvim_win_get_config(w).relative == ""
  end, vim.api.nvim_list_wins())
  local buf = #wins == 1 and vim.api.nvim_win_get_buf(wins[1]) or -1
  return buf == 1
    and vim.api.nvim_buf_get_name(buf) == ""
    and not vim.bo[buf].modified
    and vim.api.nvim_buf_line_count(buf) == 1
    and vim.api.nvim_buf_get_lines(buf, 0, 1, false)[1] == ""
end

--- Snacks dashboard'un UIEnter'da açılmasını engeller (VimEnter, UIEnter'dan önce çalışır).
local function suppress_dashboard()
  if _G.Snacks and Snacks.config and Snacks.config.dashboard then
    Snacks.config.dashboard.enabled = false
  end
end

--- VimEnter: koşullar uygunsa son projeyi/workspace'i (ya da cwd'deki projeyi) açar; açamazsa sessizce
--- dashboard'a bırakır.
function M.on_vim_enter()
  local explicit = vim.env.LESS_OPEN_DIR ~= nil
  vim.env.LESS_OPEN_DIR = nil
  state.has_ui = state.has_ui or #vim.api.nvim_list_uis() > 0
  local others = M.register_instance()
  if not enabled() or vim.g.autorestore_skip or not plain_start() or (others and not explicit) then
    return
  end
  local target = M.target()
  if not target then
    return
  end
  state.restoring = true
  local ok, opened = pcall(function()
    if target.kind == "workspace" then
      return open_workspace(target.name)
    end
    return open_project(target.dir)
  end)
  state.restoring = false
  if not ok then
    vim.notify("Could not restore " .. target.dir .. ": " .. tostring(opened), vim.log.levels.WARN)
    return
  end
  if opened then
    M.remember()
    suppress_dashboard()
    if target.kind == "project" then
      vim.notify("Restored project: " .. vim.fn.fnamemodify(target.dir, ":~"))
    end
    M.schedule_layout()
  end
end

--- Varsayılan düzende açılacak paneller (`vim.g.default_layout` = false ise boş).
---@return string[]
local function layout_panels()
  local want = vim.g.default_layout
  if want == false or vim.env.NVIM_DEFAULT_LAYOUT == "0" then
    return {}
  end
  return type(want) == "table" and want or M.config.default_layout
end

--- Editör alanındaki dashboard'u boş bir buffer'la değiştirir (dashboard'dan proje/oturum/workspace seçilince).
local function replace_dashboard()
  for _, w in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.bo[vim.api.nvim_win_get_buf(w)].filetype == "snacks_dashboard" then
      vim.api.nvim_win_call(w, function()
        vim.cmd.enew()
      end)
    end
  end
end

--- Proje düzenini kurar: bu proje/workspace için kayıtlı düzen varsa tam olarak o (paneller + boyutlar; listede
--- olmayan açık paneller kapanır), yoksa varsayılan düzen (`layout_panels`, açık olanlara dokunulmaz). Sonunda odak
--- editöre (normal mod, en baştaki pinli tab) döner; o an bir picker gibi yüzen pencere odaktaysa odağa dokunmaz.
function M.apply_layout()
  local layout = require("autorestore.layout")
  local saved = layout.saved()
  local want = saved and saved.panels or layout_panels()
  if (#want == 0 and not saved) or #vim.api.nvim_list_uis() == 0 then
    layout.resume()
    return
  end
  replace_dashboard()
  local float = vim.api.nvim_win_get_config(0).relative ~= ""
  layout.open(want, saved ~= nil)
  vim.schedule(function()
    if saved then
      layout.apply_sizes(saved.sizes or {})
    end
    layout.resume()
    if not float and vim.api.nvim_win_get_config(0).relative == "" then
      require("workbench").focus_code()
      vim.cmd.stopinsert()
    end
  end)
end

--- Düzeni bir sonraki döngüde (oturum yüklemesi bittikten sonra) tek sefer kurar; art arda gelen tetikleri birleştirir.
function M.schedule_layout()
  require("autorestore.layout").pause()
  if state.layout_pending then
    return
  end
  state.layout_pending = true
  vim.schedule(function()
    state.layout_pending = false
    M.apply_layout()
  end)
end

--- Dashboard'u editör penceresinde açar. Açılıştan önce (`nvim +Dashboard`) çağrılırsa yalnızca otomatik açmayı atlar;
--- dashboard'u snacks kendisi açar.
function M.dashboard()
  if vim.v.vim_did_enter == 0 then
    vim.g.autorestore_skip = true
    return
  end
  if vim.api.nvim_win_get_config(0).relative == "" and vim.bo.buftype ~= "" then
    require("workbench").code()
  end
  Snacks.dashboard()
end

--- Komutları ve autocmd'leri kurar. Ortam değişkenleri: `NVIM_AUTORESTORE_FILE` (kayıt dosyası, testler için; verilirse
--- geçici klasörler de kaydedilir),
--- `NVIM_AUTORESTORE=0` / `NVIM_NO_RESTORE=1` (bu açılışta otomatik açma yok), `NVIM_DEFAULT_LAYOUT=0`.
---@param opts AutorestoreConfig?
function M.setup(opts)
  M.config = vim.tbl_extend("force", M.config, opts or {})
  if vim.env.NVIM_AUTORESTORE_FILE then
    M.config.file, M.config.ignore = vim.env.NVIM_AUTORESTORE_FILE, {}
  end
  local argc = vim.fn.argc(-1)
  state.clean_launch = argc == 0 or (argc == 1 and is_dir(vim.fn.argv(0) --[[@as string]]))

  vim.api.nvim_create_user_command("Dashboard", M.dashboard, { desc = "Open the dashboard" })

  local layout = require("autorestore.layout")
  layout.key = function()
    if not enabled() or not state.has_ui then
      return nil
    end
    local cwd = realpath(vim.fn.getcwd())
    local rec = record_for(cwd)
    if not rec or (not vim.env.NVIM_LAYOUTS_FILE and vim.iter(M.config.ignore):any(function(prefix)
      return vim.startswith(cwd .. "/", prefix)
    end)) then
      return nil
    end
    return cwd
  end
  layout.setup()

  local group = vim.api.nvim_create_augroup("autorestore", { clear = true })
  vim.api.nvim_create_autocmd("UIEnter", {
    group = group,
    once = true,
    callback = function()
      state.has_ui = true
    end,
  })
  vim.api.nvim_create_autocmd("VimEnter", {
    group = group,
    nested = true,
    once = true,
    callback = M.on_vim_enter,
  })
  vim.api.nvim_create_autocmd("VimLeavePre", { group = group, callback = M.remember })
  vim.api.nvim_create_autocmd("DirChanged", {
    group = group,
    pattern = { "global", "tabpage" },
    callback = function()
      if state.restoring or vim.v.vim_did_enter == 0 then
        return
      end
      if record_for(realpath(vim.fn.getcwd())) then
        M.remember()
        M.schedule_layout()
      end
    end,
  })
  vim.api.nvim_create_autocmd("User", {
    group = group,
    pattern = "PersistenceLoadPost",
    callback = function()
      if not state.restoring and vim.v.vim_did_enter == 1 then
        M.schedule_layout()
      end
    end,
  })
end

return M
