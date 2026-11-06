--- Workspace seçici ve yönetim menüleri. Hepsi `vim.ui.select` / `vim.ui.input` (Snacks) üzerinden çalışır;
--- listelerde çift tık ile seçim yapılabildiği için mouse ile de kullanılabilir.
local M = {}

---@return table
local function ws()
  return require("workspaces")
end

--- Workspace'i listede gösterilecek metne çevirir: açık olan ● ile işaretlenir, kök klasör adları yanında yazar.
---@param w Workspace
---@return string
local function label(w)
  local cur = ws().current()
  local names = vim.tbl_map(function(r)
    return vim.fs.basename(r)
  end, w.roots)
  local mark = (cur and cur.name == w.name) and "● " or "  "
  return mark .. w.name .. "  ·  " .. (#names > 0 and table.concat(names, ", ") or "(no folders)")
end

--- Ad verilmemişse workspace seçtirir; açık workspace varsa ve `prefer_current` ise doğrudan onu kullanır.
---@param name string?
---@param prompt string
---@param fn fun(name: string)
---@param prefer_current boolean?
local function with_workspace(name, prompt, fn, prefer_current)
  if name then
    return fn(name)
  end
  local cur = prefer_current and ws().current()
  if cur then
    return fn(cur.name)
  end
  local list = ws().list()
  if #list == 0 then
    vim.notify("No saved workspaces. Create one with :Workspace new", vim.log.levels.INFO)
    return
  end
  vim.ui.select(list, { prompt = prompt, format_item = label }, function(w)
    if w then
      fn(w.name)
    end
  end)
end

--- Ana seçici (dashboard `w`, `:Workspace`, `Space f w`): seçilen workspace açılır.
--- Listenin sonunda "Yeni workspace" ve "Yönet" girişleri vardır.
function M.pick()
  local items = vim.tbl_map(function(w)
    return { ws = w }
  end, ws().list())
  table.insert(items, { action = "new", text = "  ＋ Create new workspace…" })
  if #items > 1 then
    table.insert(items, { action = "manage", text = "  ⚙ Manage workspaces (add/remove folders, rename, delete)…" })
  end
  vim.ui.select(items, {
    prompt = "Workspaces",
    format_item = function(item)
      return item.ws and label(item.ws) or item.text
    end,
  }, function(item)
    if not item then
      return
    elseif item.ws then
      ws().open(item.ws.name)
    elseif item.action == "new" then
      M.new()
    else
      M.manage()
    end
  end)
end

--- Daha önce açılmış projeler: persistence oturum dosyalarından (`/` → `%`, dal soneki `%%<dal>`), diskte duran,
--- workspace klasörü olmayan klasörler; son kullanılana göre.
---@return { dir: string, time: integer }[]
local function recent_projects()
  local ok, persistence = pcall(require, "persistence")
  local cfg = ok and require("persistence.config").options
  if not (cfg and cfg.dir) then
    return {}
  end
  local out, seen = {}, {}
  for _, file in ipairs(persistence.list()) do
    local name = file:sub(#cfg.dir + 1, -5)
    local dir = (vim.split(name, "%%", { plain = true })[1]):gsub("%%", "/")
    if not seen[dir] and vim.fn.isdirectory(dir) == 1 and not ws().is_link_dir(dir) and dir ~= vim.env.HOME then
      seen[dir] = true
      local stat = vim.uv.fs_stat(file)
      table.insert(out, { dir = dir, time = stat and stat.mtime.sec or 0 })
    end
  end
  return out
end

--- Klasörü yeni (ya da kayıtlı) proje olarak açar: klasöre geçer, oturumu varsa yükler; varsayılan panel düzeni
--- `DirChanged` ile kurulur.
---@param dir string
local function open_project(dir)
  dir = ws().normalize(dir)
  if vim.fn.isdirectory(dir) == 0 then
    vim.notify("Folder not found: " .. dir, vim.log.levels.WARN)
    return
  end
  require("autorestore").open_project(dir)
end

--- "Open Folder…" (dashboard `o`): yolu sorup klasörü proje olarak açar; daha önce açılmamış klasörler için.
function M.open_folder()
  vim.ui.input({ prompt = "Open folder as project: ", default = "~/", completion = "dir" }, function(path)
    if path and vim.trim(path) ~= "" then
      open_project(vim.trim(path))
    end
  end)
end

--- Projeler ve workspace'ler tek listede (dashboard `p`, `Space f p`), son kullanılana göre; her satırda türünü
--- gösteren etiket. Sonda "Open folder…" ve "New workspace…" girişleri; Ctrl+O da klasör açar.
function M.projects()
  local g = vim.fn.nr2char
  local items = {}
  local cur = ws().current()
  for _, w in ipairs(ws().list()) do
    local names = vim.tbl_map(vim.fs.basename, w.roots)
    table.insert(items, {
      text = w.name .. " " .. table.concat(names, " "),
      kind = "workspace",
      name = w.name,
      detail = #names > 0 and table.concat(names, ", ") or "(no folders)",
      time = w.opened or 0,
      current = cur and cur.name == w.name,
    })
  end
  local cwd = vim.fn.getcwd()
  for _, p in ipairs(recent_projects()) do
    table.insert(items, {
      text = vim.fs.basename(p.dir) .. " " .. p.dir,
      kind = "project",
      dir = p.dir,
      name = vim.fs.basename(p.dir),
      detail = vim.fn.fnamemodify(p.dir, ":~"),
      time = p.time,
      current = not cur and p.dir == cwd,
    })
  end
  table.sort(items, function(a, b)
    return a.time > b.time
  end)
  table.insert(items, { text = "open folder new project", kind = "action", name = "Open folder…", run = M.open_folder,
    detail = "open a folder as a new project" })
  table.insert(items, { text = "new workspace", kind = "action", name = "New workspace…", run = function() M.new() end,
    detail = "group folders from different places" })
  local badge = {
    workspace = { g(0xf1b3) .. " workspace", "Special" },
    project = { g(0xf07b) .. " project  ", "Directory" },
    action = { g(0xf067) .. "          ", "Special" },
  }
  Snacks.picker.pick({
    title = "Projects & Workspaces",
    items = items,
    layout = { preset = "select" },
    format = function(item)
      local b = badge[item.kind]
      return {
        { item.current and "● " or "  ", "DiagnosticOk" },
        { b[1], b[2] },
        { "  " },
        { item.name, "SnacksPickerFile" },
        { "  " .. item.detail, "Comment" },
      }
    end,
    confirm = function(picker, item)
      picker:close()
      if not item then
        return
      end
      vim.schedule(function()
        if item.run then
          item.run()
        elseif item.kind == "workspace" then
          ws().open(item.name)
        else
          open_project(item.dir)
        end
      end)
    end,
    actions = {
      open_folder = function(picker)
        picker:close()
        vim.schedule(M.open_folder)
      end,
    },
    win = {
      input = {
        keys = {
          ["<C-o>"] = { "open_folder", mode = { "i", "n" } },
        },
      },
    },
  })
end

--- Yeni workspace: ad sorulur, ardından ilk klasör seçtirilir ve yönetim menüsü açılır.
---@param name string?
function M.new(name)
  local function create(n)
    if not n or vim.trim(n) == "" then
      return
    end
    n = vim.trim(n)
    if ws().create(n) then
      M.add_folder(n, nil, function()
        M.manage(n)
      end)
    end
  end
  if name then
    return create(name)
  end
  vim.ui.input({ prompt = "New workspace name: " }, create)
end

--- Klasör kaynağı seçtirir: neo-tree'deki klasör, çalışma klasörü, son projeler (Snacks projects) ya da elle yol.
---@param cb fun(path: string)
local function choose_folder(cb)
  local items = {}
  if vim.bo.filetype == "neo-tree" then
    local ok, state = pcall(function()
      return require("neo-tree.sources.manager").get_state("filesystem")
    end)
    local node = ok and state.tree and state.tree:get_node()
    if node then
      local path = node.type == "directory" and node.path or vim.fs.dirname(node.path)
      table.insert(items, { path = ws().to_real(path), text = "Folder selected in tree: " })
    end
  end
  if not ws().current() then
    table.insert(items, { path = vim.fn.getcwd(), text = "Working directory: " })
  end
  table.insert(items, { action = "projects", text = "Choose from recent projects…" })
  table.insert(items, { action = "input", text = "Enter a path…" })
  vim.ui.select(items, {
    prompt = "Folder to add",
    format_item = function(item)
      return item.text .. (item.path and vim.fn.fnamemodify(item.path, ":~") or "")
    end,
  }, function(item)
    if not item then
      return
    elseif item.path then
      cb(item.path)
    elseif item.action == "projects" then
      Snacks.picker.projects({
        title = "Project to add to workspace",
        confirm = function(picker, it)
          picker:close()
          if it and it.file then
            vim.schedule(function()
              cb(it.file)
            end)
          end
        end,
      })
    else
      vim.ui.input({ prompt = "Folder path: ", default = "~/", completion = "dir" }, function(path)
        if path and vim.trim(path) ~= "" then
          cb(vim.trim(path))
        end
      end)
    end
  end)
end

--- Workspace'e klasör ekler. Ad verilmezse açık workspace (yoksa seçilen) kullanılır; yol verilmezse seçtirilir.
---@param name string?
---@param path string?
---@param done fun()?
function M.add_folder(name, path, done)
  with_workspace(name, "Workspace to add a folder to", function(n)
    local function add(p)
      if ws().add_root(n, p) then
        vim.notify(vim.fn.fnamemodify(ws().normalize(p), ":~") .. " → " .. n)
      end
      if done then
        done()
      end
    end
    if path then
      add(path)
    else
      choose_folder(add)
    end
  end, true)
end

--- Workspace'ten klasör çıkarır (diskteki klasör silinmez).
---@param name string?
---@param done fun()?
function M.remove_folder(name, done)
  with_workspace(name, "Workspace to remove a folder from", function(n)
    local w = ws().get(n)
    if not w or #w.roots == 0 then
      vim.notify("Workspace has no folders", vim.log.levels.INFO)
      return
    end
    vim.ui.select(w.roots, {
      prompt = "Folder to remove (" .. n .. ")",
      format_item = function(r)
        return vim.fn.fnamemodify(r, ":~")
      end,
    }, function(root)
      if root then
        ws().remove_root(n, root)
      end
      if done then
        done()
      end
    end)
  end, true)
end

--- Workspace'i yeniden adlandırır.
---@param name string?
---@param new_name string?
---@param done fun(new: string)?
function M.rename(name, new_name, done)
  with_workspace(name, "Workspace to rename", function(n)
    local function apply(new)
      if new and vim.trim(new) ~= "" and ws().rename(n, new) and done then
        done(vim.trim(new))
      end
    end
    if new_name then
      return apply(new_name)
    end
    vim.ui.input({ prompt = "New name: ", default = n }, apply)
  end, true)
end

--- Workspace kaydını onay alarak siler (klasörlere dokunulmaz).
---@param name string?
function M.delete(name)
  with_workspace(name, "Workspace to delete", function(n)
    vim.ui.select({ "Yes, delete", "Cancel" }, {
      prompt = "Delete “" .. n .. "”? (folders stay on disk)",
    }, function(choice)
      if choice == "Yes, delete" and ws().delete(n) then
        vim.notify("Workspace deleted: " .. n)
      end
    end)
  end)
end

--- Yönetim menüsü: önce workspace (ya da yeni), sonra eylem seçilir. Klasör ekle/çıkar ve adlandırma sonrası
--- menü yeniden açılır; böylece arka arkaya birden fazla klasör eklenebilir.
---@param name string?
function M.manage(name)
  if not name then
    local items = vim.tbl_map(function(w)
      return { ws = w }
    end, ws().list())
    table.insert(items, { text = "  ＋ Create new workspace…" })
    vim.ui.select(items, {
      prompt = "Manage workspaces",
      format_item = function(item)
        return item.ws and label(item.ws) or item.text
      end,
    }, function(item)
      if item and item.ws then
        M.manage(item.ws.name)
      elseif item then
        M.new()
      end
    end)
    return
  end
  local w = ws().get(name)
  if not w then
    return
  end
  local again = function()
    M.manage(name)
  end
  local actions = {
    { text = "Open", run = function() ws().open(name) end },
    { text = "Add folder…", run = function() M.add_folder(name, nil, again) end },
    { text = "Remove folder…", run = function() M.remove_folder(name, again) end },
    { text = "Rename…", run = function() M.rename(name, nil, M.manage) end },
    { text = "Delete…", run = function() M.delete(name) end },
  }
  if #w.roots == 0 then
    actions = vim.tbl_filter(function(a)
      return a.text ~= "Open" and a.text ~= "Remove folder…"
    end, actions)
  end
  local roots = #w.roots > 0 and table.concat(vim.tbl_map(function(r)
    return vim.fn.fnamemodify(r, ":~")
  end, w.roots), ", ") or "no folders"
  vim.ui.select(actions, {
    prompt = name .. " — " .. roots,
    format_item = function(a)
      return a.text
    end,
  }, function(a)
    if a then
      a.run()
    end
  end)
end

return M
