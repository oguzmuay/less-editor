local M = {}

local lockfiles = {
  ["pnpm-lock.yaml"] = "pnpm",
  ["yarn.lock"] = "yarn",
  ["bun.lock"] = "bun",
  ["bun.lockb"] = "bun",
  ["package-lock.json"] = "npm",
}

--- Paket klasöründen yukarı doğru en yakın lock dosyasına bakarak paket yöneticisini belirler (workspace'ler dahil).
---@param dir string
---@param root string
---@return string
local function package_manager(dir, root)
  local lock = vim.fs.find(vim.tbl_keys(lockfiles), { upward = true, path = dir, stop = vim.fs.dirname(root) })[1]
  return lock and lockfiles[vim.fs.basename(lock)] or "npm"
end

--- Script adlarını package.json'daki yazılış sırasına göre döndürür (JSON çözümlemesi sırayı korumadığı için).
---@param text string
---@param scripts table<string, string>
---@return string[]
local function ordered_names(text, scripts)
  local start = text:find('"scripts"%s*:') or 1
  local pos = {}
  local names = vim.tbl_keys(scripts)
  for _, name in ipairs(names) do
    pos[name] = text:find('"' .. vim.pesc(name) .. '"%s*:', start) or math.huge
  end
  table.sort(names, function(a, b)
    return pos[a] == pos[b] and a < b or pos[a] < pos[b]
  end)
  return names
end

--- Debug modunda eklenecek ortam: her node sürecinde inspector rastgele boş portta açılır (`--inspect=127.0.0.1:0`).
--- Sabit port verilmez çünkü NODE_OPTIONS paket yöneticisine (npm/pnpm/yarn de node'dur) ve script'in tüm alt
--- süreçlerine geçer; sabit portu ilk açılan süreç (paket yöneticisi) alır, asıl uygulama inspector'sız kalırdı.
---@return table<string, string>
local function debug_env(item)
  local base = item.config and item.config.vm_options or vim.env.NODE_OPTIONS or ""
  return { NODE_OPTIONS = vim.trim(base .. " --inspect=127.0.0.1:0") }
end

--- Inspector'ı açılan bir node sürecine nvim-dap `pwa-node` attach ile bağlanır. Portu dinleyen süreç, öğenin job'ının
--- kendisiyse (paket yöneticisi) ya da port artık dinlenmiyorsa (süreç kapanmış) atlanır; diğer her node süreci (script, alt süreçleri, watch modunda yeniden doğanlar)
--- ayrı bir oturum olur. Alt süreçler kendi inspector satırlarıyla yakalandığı için js-debug'ın otomatik alt süreç
--- bağlama özelliği kapatılır (aynı sürece iki kez bağlanmamak için).
---@param item RunnerItem
---@param port integer
---@param run fun(config: table)
local function attach(item, port, run)
  local job_pid = item.job and vim.fn.jobpid(item.job)
  vim.system({ "lsof", "-nP", "-iTCP:" .. port, "-sTCP:LISTEN", "-t" }, { text = true }, function(res)
    local pid = tonumber((res.stdout or ""):match("%d+"))
    vim.schedule(function()
      if not pid or pid == job_pid then
        return
      end
      run({
        type = "pwa-node",
        request = "attach",
        name = ("%s (pid %d)"):format(item.label, pid),
        address = "127.0.0.1",
        port = port,
        cwd = item.cwd,
        sourceMaps = true,
        autoAttachChildProcesses = false,
        skipFiles = { "<node_internals>/**" },
        resolveSourceMapLocations = { item.cwd .. "/**", "!**/node_modules/**" },
      })
    end)
  end)
end

--- Kökteki tüm package.json dosyalarını (node_modules hariç) okuyup "Node.js" grubunu oluşturur.
--- Her paket bir alt düğüm, her script o paketin altında bir öğedir.
---@param root string
---@return RunnerNode?
function M.discover(root)
  local res = vim
    .system({ "rg", "--files", "--glob=package.json", "--glob=!**/node_modules/**", root }, { text = true })
    :wait()
  local packages = {}
  for file in vim.gsplit(res.stdout or "", "\n", { trimempty = true }) do
    local fd = io.open(file, "r")
    local text = fd and fd:read("*a") or ""
    if fd then
      fd:close()
    end
    local ok, data = pcall(vim.json.decode, text)
    if ok and type(data) == "table" and type(data.scripts) == "table" and next(data.scripts) then
      local dir = vim.fs.dirname(file)
      local rel = vim.fs.relpath(root, dir) or dir
      local pm = package_manager(dir, root)
      local scripts = {}
      for _, name in ipairs(ordered_names(text, data.scripts)) do
        table.insert(scripts, {
          id = "node:" .. dir .. ":" .. name,
          label = name,
          detail = tostring(data.scripts[name]),
          icon = "\u{F489}",
          icon_hl = "RunnerScript",
          cwd = dir,
          vm_env = "NODE_OPTIONS",
          cmd = function(item)
            local args = item.config and item.config.args
            -- shortcut: argümanlar boşlukla bölünür (tırnak desteği yok); gerekirse shell tarzı ayrıştırıcı
            return vim.list_extend({ pm, "run", name }, args and vim.list_extend({ "--" }, vim.split(args, "%s+")) or {})
          end,
          debug = {
            ready = "Debugger listening on ws://[^/]-:(%d+)/",
            env = debug_env,
            attach = attach,
          },
        })
      end
      table.insert(packages, {
        id = "node:" .. dir,
        label = type(data.name) == "string" and data.name or (rel == "." and vim.fs.basename(root) or rel),
        detail = rel .. " · " .. pm,
        icon = "\u{F487}",
        icon_hl = "RunnerPackage",
        children = scripts,
        rel = rel,
      })
    end
  end
  table.sort(packages, function(a, b)
    return a.rel < b.rel
  end)
  return { id = "node", label = "Node.js", icon = "\u{E718}", icon_hl = "RunnerNode", children = packages }
end

return M
