--- neo-tree'yi workspace'in symlink klasörüyle çalıştırmak için gereken küçük uyarlamalar.
--- Ağaç düğümleri symlink yolları taşır (`<workspace>/<kök>/...`); git durumu ve "geçerli dosyayı göster" ise gerçek
--- yollarla çalışır. Dosya açmak için yama gerekmez: Neovim buffer adındaki symlink'leri kendisi çözer (`:edit`, `bufadd`).
--- Buradaki her yama yalnızca bir workspace açıkken devreye girer.
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

--- Yamaları bir kez kurar (neo-tree'nin `setup`'ından önce, eklenti spec'inin `opts` aşamasında çağrılır):
--- - `get_path_to_reveal`: geçerli dosyanın gerçek yolunu ağaçtaki symlink yoluna çevirir (Cmd+1 reveal, follow).
--- - `git.status_async`: workspace klasörü için çağrılınca her kökün kendi reposunda çalışır.
--- - `git.find_existing_status_code`: symlink yolunu gerçek yola çevirip git durumunu bulur.
function M.install()
  if installed then
    return
  end
  installed = true
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
