--- Claude Code hesapları: her hesap ayrı bir `CLAUDE_CONFIG_DIR` (varsayılan hesap `~/.claude`, env verilmez).
--- Yeni oturumlar seçili hesapla açılır; açık oturumlar kendi hesabında kalır (`tab.account`).
--- Kayıt `stdpath("data")/claude_accounts.json` → `{ current, accounts = [{ name, dir }] }` (testte
--- `NVIM_CLAUDE_ACCOUNTS_FILE`); kayıt yokken `~/.claude-*` klasörleri hesap sayılır (ör. `claude-work`).
--- claudecode.nvim IDE kilit dosyasını yalnız `~/.claude/ide`'ye yazar; diğer hesapların `ide/` klasörüne symlink'i konur.
local M = {}

local DEFAULT = { name = "claude", dir = vim.fs.normalize("~/.claude") }

local function file()
  return vim.env.NVIM_CLAUDE_ACCOUNTS_FILE or (vim.fn.stdpath("data") .. "/claude_accounts.json")
end

---@return { current: string, accounts: { name: string, dir: string }[] }
local function load()
  local ok, data = pcall(function()
    return vim.json.decode(table.concat(vim.fn.readfile(file()), "\n"))
  end)
  if ok and type(data) == "table" and type(data.accounts) == "table" then
    return data
  end
  data = { current = DEFAULT.name, accounts = { DEFAULT } }
  for _, dir in ipairs(vim.fn.glob(vim.fs.normalize("~/.claude-*"), true, true)) do
    if vim.fn.isdirectory(dir) == 1 then
      table.insert(data.accounts, { name = vim.fs.basename(dir):sub(2), dir = dir })
    end
  end
  return data
end

local function save(data)
  vim.fn.mkdir(vim.fs.dirname(file()), "p")
  vim.fn.writefile({ vim.json.encode(data, { escape_slash = false }) }, file())
end

---@return { name: string, dir: string }[]
function M.list()
  return load().accounts
end

--- Yeni oturumların açılacağı hesap.
---@return { name: string, dir: string }
function M.current()
  local data = load()
  for _, a in ipairs(data.accounts) do
    if a.name == data.current then
      return a
    end
  end
  return DEFAULT
end

---@param account { name: string, dir: string }
---@return boolean
function M.is_default(account)
  return account.dir == DEFAULT.dir
end

--- Hesabın Claude Code'a verilecek ortamı (varsayılan hesapta boş).
---@param account { name: string, dir: string }
---@return table<string, string>
function M.env(account)
  return M.is_default(account) and {} or { CLAUDE_CONFIG_DIR = account.dir }
end

--- claudecode'un `~/.claude/ide/<port>.lock` kilidini hesabın `ide/` klasörüne symlink'ler (IDE bağlantısı için).
---@param account { name: string, dir: string }
---@param port string|integer
function M.link_ide(account, port)
  local ok, lockfile = pcall(require, "claudecode.lockfile")
  if M.is_default(account) or not ok then
    return
  end
  local dir = account.dir .. "/ide"
  vim.fn.mkdir(dir, "p", tonumber("700", 8))
  local dst = ("%s/%s.lock"):format(dir, port)
  if not vim.uv.fs_lstat(dst) then
    vim.uv.fs_symlink(("%s/%s.lock"):format(lockfile.lock_dir, port), dst)
  end
end

---@param data table
---@param name string
local function select(data, name)
  data.current = name
  save(data)
  vim.notify(("New Claude sessions use account: %s"):format(name))
end

--- Hesap ekleme: ad, sonra yapılandırma klasörü (varsayılan `~/.<ad>`). Klasör yoksa Claude Code ilk açılışta giriş ister.
---@param data table
local function add(data)
  vim.ui.input({ prompt = "Account name: " }, function(name)
    name = name and vim.trim(name) or ""
    if name == "" then
      return
    end
    if vim.iter(data.accounts):any(function(a) return a.name == name end) then
      vim.notify(("Account already exists: %s"):format(name), vim.log.levels.WARN)
      return
    end
    vim.ui.input({ prompt = "Config folder: ", default = "~/." .. name, completion = "dir" }, function(dir)
      dir = dir and vim.trim(dir) or ""
      if dir == "" then
        return
      end
      table.insert(data.accounts, { name = name, dir = vim.fs.normalize(vim.fn.expand(dir)) })
      select(data, name)
    end)
  end)
end

--- `:Claude account`: hesap seç (yeni oturumlar için) ya da yeni hesap ekle.
function M.pick()
  local data = load()
  local choices = vim.list_extend(vim.deepcopy(data.accounts), { { add = true } })
  vim.ui.select(choices, {
    prompt = "Claude account for new sessions",
    format_item = function(a)
      if a.add then
        return "+ Add account…"
      end
      return ("%s %s  %s"):format(a.name == data.current and "●" or " ", a.name, vim.fn.fnamemodify(a.dir, ":~"))
    end,
  }, function(choice)
    if not choice then
      return
    end
    if choice.add then
      add(data)
    else
      select(data, choice.name)
    end
  end)
end

return M
