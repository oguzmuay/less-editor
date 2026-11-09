--- neo-tree'yi workspace'in symlink klasörüyle çalıştırmak için gereken küçük uyarlamalar.
--- Ağaç düğümleri symlink yolları taşır (`<workspace>/<kök>/...`); git durumu ve "geçerli dosyayı göster" ise gerçek
--- yollarla çalışır. Dosya açmak için yama gerekmez: Neovim buffer adındaki symlink'leri kendisi çözer (`:edit`, `bufadd`).
--- Buradaki her yama yalnızca bir workspace açıkken devreye girer (`guard_missing` hariç: her zaman).
local M = {}

local installed = false

---@return table
local function ws()
  return require("workspaces")
end

--- Açık workspace'in kök klasörleri için git durumunu (asenkron) yeniler.
function M.git_status()
  local roots = ws().roots()
  local git = require("neo-tree.git")
  local opts = require("neo-tree").config.git_status_async_options
  for _, root in ipairs(roots or {}) do
    git.status_async(root, nil, opts)
  end
end

--- `git_status_changed` olayı: kökler workspace klasörünün altında görünmediği için neo-tree'nin kendi kontrolü
--- ağacı yeniden çizmez; değişen repo workspace köklerinden biriyse burada yeniden çizilir.
---@param args { git_root: string }?
function M.on_git_status_changed(args)
  local roots = ws().roots()
  if not roots or not args or not args.git_root then
    return
  end
  for _, root in ipairs(roots) do
    local g = args.git_root
    if root == g or vim.startswith(root, g .. "/") or vim.startswith(g, root .. "/") then
      require("neo-tree.sources.manager").redraw("filesystem")
      return
    end
  end
end

--- Repo kökünün o anki dalı (`.git/HEAD`'den; worktree'de `.git` dosyasındaki `gitdir`), ayrık HEAD'de kısa sha.
---@param root string
---@return string?
local function head(root)
  local function first_line(path)
    local fd = io.open(path, "r")
    local line = fd and fd:read("*l")
    if fd then
      fd:close()
    end
    return line
  end
  local git = root .. "/.git"
  local stat = vim.uv.fs_stat(git)
  if not stat then
    return nil
  end
  if stat.type == "file" then
    local dir = (first_line(git) or ""):match("^gitdir:%s*(.-)%s*$")
    if not dir then
      return nil
    end
    git = dir:sub(1, 1) == "/" and dir or (root .. "/" .. dir)
  end
  local ref = first_line(git .. "/HEAD")
  return ref and (ref:match("^ref: refs/heads/(.+)$") or ref:sub(1, 7)) or nil
end

--- Ağaç düğümü açık workspace'in bir kök klasörüyse (symlink klasörünün doğrudan çocuğu) git dalı; değilse nil.
---@param node table neo-tree düğümü
---@return string?
function M.branch(node)
  if node.type ~= "directory" then
    return nil
  end
  local cur = ws().current()
  if not cur or vim.fs.dirname(node.path) ~= ws().link_dir(cur) then
    return nil
  end
  return head(ws().to_real(node.path))
end

--- neo-tree `name` bileşeni: workspace köklerinin adının yanına dalı (` main`) ekler.
---@return table
function M.name_component(config, node, state)
  local name = require("neo-tree.sources.common.components").name(config, node, state)
  local branch = M.branch(node)
  if not branch then
    return name
  end
  return { name, { text = "  " .. vim.fn.nr2char(0xe725) .. " " .. branch, highlight = "Comment" } }
end

---@param path string?
---@return boolean
local function is_dir(path)
  local st = path and vim.uv.fs_stat(path)
  return st ~= nil and st.type == "directory"
end

--- Diskten silinmiş klasörler için neo-tree'nin `ENOENT` hatasını önler. neo-tree "gösterilecek dosya"nın
--- (follow / reveal; ör. oturumdan kalan, artık olmayan bir klasördeki buffer) üst klasörlerini varlık kontrolü
--- yapmadan tarar: klasör yoksa hata basar ve async taramada iş hiç bitmez (ağaç yenilenmez). Yama:
--- - üst klasörü olmayan `path_to_reveal` atılır (ağaç yine yenilenir, silinen klasör listeden düşer);
--- - silinmiş bir düğümü açma isteği (`parent_id`) tüm ağacı yenilemeye çevrilir, düğüm ağaçtan çıkar;
--- - silinmiş düğümün ön yüklemesi (`get_dir_items_async`) atlanır.
function M.guard_missing()
  local fs_scan = require("neo-tree.sources.filesystem.lib.fs_scan")
  local get_items = fs_scan.get_items
  fs_scan.get_items = function(state, parent_id, path_to_reveal, callback, async, recursive)
    if path_to_reveal and not is_dir(vim.fs.dirname(path_to_reveal)) then
      path_to_reveal = nil
    end
    if parent_id and not is_dir(parent_id) then
      parent_id, recursive = nil, nil
    end
    return get_items(state, parent_id, path_to_reveal, callback, async, recursive)
  end
  local get_dir_items_async = fs_scan.get_dir_items_async
  fs_scan.get_dir_items_async = function(state, parent_id, ...)
    if parent_id and not is_dir(parent_id) then
      return
    end
    return get_dir_items_async(state, parent_id, ...)
  end
end

--- Yamaları bir kez kurar (neo-tree'nin `setup`'ından önce, eklenti spec'inin `opts` aşamasında çağrılır):
--- - `get_path_to_reveal`: geçerli dosyanın gerçek yolunu ağaçtaki symlink yoluna çevirir (Cmd+1 reveal, follow).
--- - `git.status_async`: workspace klasörü için çağrılınca her kökün kendi reposunda çalışır.
--- - `git.find_existing_status_code`: symlink yolunu gerçek yola çevirip git durumunu bulur.
--- - `guard_missing`: silinmiş klasörlerde neo-tree hatası.
function M.install()
  if installed then
    return
  end
  installed = true
  M.guard_missing()
  local manager = require("neo-tree.sources.manager")
  local git = require("neo-tree.git")

  local get_path_to_reveal = manager.get_path_to_reveal
  manager.get_path_to_reveal = function(...)
    local path = get_path_to_reveal(...)
    return path and ws().to_link(path) or path
  end

  local status_async = git.status_async
  git.status_async = function(path, base, opts, callback)
    local cur = ws().current()
    if cur and path == ws().link_dir(cur) then
      M.git_status()
      if callback then
        callback(nil)
      end
      return
    end
    return status_async(path and ws().to_real(path) or path, base, opts, callback)
  end

  local find_existing_status_code = git.find_existing_status_code
  git.find_existing_status_code = function(path, ...)
    return find_existing_status_code(path and ws().to_real(path) or path, ...)
  end
end

return M
