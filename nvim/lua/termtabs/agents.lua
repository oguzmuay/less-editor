--- AI paneli (Cmd+3) sağlayıcıları: Claude Code (claudecode.nvim üzerinden), Codex, Gemini, Ollama.
--- Her sekme `tab.agent` (tür) ve `tab.ai_name` ("codex-1") taşır. Son seçilen tür varsayılan olur (Cmd+3 boş panelde
--- onu açar); kayıt `stdpath("data")/ai.json` (testte `NVIM_AI_FILE`).
---
--- Ortak posta kutusu: projede `.less/ai/` (içinde `.gitignore` = `*`): `README.md` (protokol + açık AI'lar),
--- `inbox/<ad>.md`, `shared/`. Her AI `LESS_AI_DIR` / `LESS_AI_NAME` ortamıyla ve kısa bir talimatla başlar.
--- Bir AI'ın inbox dosyası değişince o AI boştaysa terminaline "yeni mesaj" bildirimi yazılır (Ollama'ya mesajın kendisi);
--- meşgulse boşa düşünce. İki AI'ın birbirini sonsuza kadar tetiklememesi için AI başına 10 dakikada en fazla 6 otomatik
--- bildirim.
local M = {}

---@class termtabs.Agent
---@field label string
---@field cmd? fun(name: string, dir: string, text: string, model?: string): string[] Claude hariç (Claude claudecode ile)

---@type table<string, termtabs.Agent>
M.kinds = {
  claude = { label = "Claude" },
  codex = {
    label = "Codex",
    cmd = function(_, _, text)
      return { "codex", "-c", "developer_instructions=" .. vim.json.encode(text) }
    end,
  },
  gemini = {
    label = "Gemini",
    cmd = function(_, _, text)
      return { "gemini", "-i", text .. "\n\nReply only with: ready." }
    end,
  },
  ollama = {
    label = "Ollama",
    cmd = function(_, _, _, model)
      return { "ollama", "run", model }
    end,
  },
}
M.order = { "claude", "codex", "gemini", "ollama" }

local NUDGE_LIMIT, NUDGE_WINDOW = 6, 600

local function state_file()
  return vim.env.NVIM_AI_FILE or (vim.fn.stdpath("data") .. "/ai.json")
end

---@return { default: string, ollama_model: string? }
local function load()
  local ok, data = pcall(function()
    return vim.json.decode(table.concat(vim.fn.readfile(state_file()), "\n"))
  end)
  return ok and type(data) == "table" and data or { default = "claude" }
end

local function save(data)
  vim.fn.mkdir(vim.fs.dirname(state_file()), "p")
  vim.fn.writefile({ vim.json.encode(data) }, state_file())
end

--- Cmd+3 boş panelde açılacak tür (son seçilen).
---@return string
function M.default()
  local d = load().default
  return M.kinds[d] and d or "claude"
end

local function tt()
  return require("termtabs")
end

--- Projenin posta kutusu klasörü.
---@param cwd string
---@return string
function M.dir(cwd)
  return cwd .. "/.less/ai"
end

--- Tür için sıradaki ad: "codex-1", "codex-2" … (açık sekmelerde kullanılmayan en küçük numara).
---@param kind string
---@return string
local function next_name(kind)
  local used = {}
  for _, tab in ipairs(tt().tabs("claude")) do
    used[tab.ai_name or ""] = true
  end
  local n = 1
  while used[("%s-%d"):format(kind, n)] do
    n = n + 1
  end
  return ("%s-%d"):format(kind, n)
end

--- AI'a verilen talimat: kim olduğu, posta kutusu ve protokol.
---@param name string
---@param dir string
---@return string
function M.instructions(name, dir)
  return table.concat({
    ("You are %s, one of several AI agents working side by side in the LESS editor."):format(name),
    ("Shared mailbox: %s (README.md there lists the agents and this protocol)."):format(dir),
    ("To message another agent, append to %s/inbox/<agent>.md a section starting with '## from %s · <time>'."):format(dir, name),
    ("Your inbox is %s/inbox/%s.md; read it when the user asks or when you are told you have new mail."):format(dir, name),
    ("Put files you want to share in %s/shared/. Only message other agents when the task needs it."):format(dir),
    require("lessmcp").hint,
  }, "\n")
end

--- Posta kutusunu hazırlar ve README'yi açık AI'larla yeniden yazar.
---@param dir string
function M.prepare(dir)
  vim.fn.mkdir(dir .. "/inbox", "p")
  vim.fn.mkdir(dir .. "/shared", "p")
  if vim.fn.filereadable(dir .. "/.gitignore") == 0 then
    vim.fn.writefile({ "*" }, dir .. "/.gitignore")
  end
  local lines = {
    "# LESS AI mailbox",
    "",
    "Agents in this editor talk through this folder.",
    "",
    "- Send: append to `inbox/<agent>.md` a section `## from <your name> · <time>` followed by the message.",
    "- Receive: read `inbox/<your name>.md` (your name is in `$LESS_AI_NAME`). New messages are at the end.",
    "- Share files in `shared/`. Ollama agents cannot read files; they only receive messages.",
    "",
    "## Agents",
    "",
  }
  for _, tab in ipairs(tt().tabs("claude")) do
    if tab.ai_name and M.dir(tab.cwd) == dir then
      lines[#lines + 1] = ("- `%s` (%s)"):format(tab.ai_name, M.kinds[tab.agent].label)
    end
  end
  vim.fn.writefile(lines, dir .. "/README.md")
  M.watch(dir)
end

--- Yeni sekmenin adını, ortamını ve (Claude dışı) komutunu hazırlar.
---@param kind string
---@param cwd string
---@param model? string
--- LESS MCP sunucusu (`lessmcp.connect`) Claude dışı komutlara eklenir; Claude'da `mcp_argv` ayrıca döner.
---@return { name: string, dir: string, env: table<string, string>, text: string, argv: string[]?, mcp_argv: string[] }
function M.launch(kind, cwd, model)
  local name, dir = next_name(kind), M.dir(cwd)
  local text = M.instructions(name, dir)
  local spec = M.kinds[kind]
  local mcp_argv, mcp_env = require("lessmcp").connect(kind, name)
  local argv = spec.cmd and vim.list_extend(spec.cmd(name, dir, text, model), mcp_argv)
  return {
    name = name,
    dir = dir,
    env = vim.tbl_extend("force", { LESS_AI_DIR = dir, LESS_AI_NAME = name }, mcp_env),
    text = text,
    argv = argv,
    mcp_argv = mcp_argv,
  }
end

--- Claude dışı bir AI'ı yeni sekmede başlatır.
---@param kind string
---@param model? string Ollama modeli
function M.spawn(kind, model)
  local cwd = vim.fn.getcwd()
  local l = M.launch(kind, cwd, model)
  local buf = tt().spawn("claude", l.argv, {
    cwd = cwd,
    env = l.env,
    agent = kind,
    ai_name = l.name,
    width = math.floor(vim.o.columns * 0.38),
  })
  if buf then
    M.prepare(l.dir)
  end
end

--- Türü varsayılan yapar ve yeni sekmede açar (Ollama'da model sorulur, son model hatırlanır).
---@param kind string
function M.open(kind)
  local data = load()
  data.default = kind
  save(data)
  if kind == "claude" then
    return require("termtabs.claude").new_session()
  end
  if kind ~= "ollama" then
    return M.spawn(kind)
  end
  vim.ui.input({ prompt = "Ollama model: ", default = data.ollama_model or "llama3.2" }, function(model)
    model = model and vim.trim(model) or ""
    if model ~= "" then
      data.ollama_model = model
      save(data)
      M.spawn("ollama", model)
    end
  end)
end

--- Cmd+T (AI panelinde) ve `:AI new`: tür seçici.
function M.pick()
  local default = M.default()
  vim.ui.select(M.order, {
    prompt = "New AI session",
    format_item = function(kind)
      return (kind == default and "● " or "  ") .. M.kinds[kind].label
    end,
  }, function(kind)
    if kind then
      M.open(kind)
    end
  end)
end

local watchers = {} ---@type table<string, uv.uv_fs_event_t>
local pending = {} ---@type table<string, boolean> ai_name -> okunmamış posta
local offsets = {} ---@type table<string, integer> Ollama: iletilen bayt
local nudges = {} ---@type table<string, integer[]>

--- Posta kutusunun inbox klasörünü izler (değişen `<ad>.md` → o AI'a bekleyen posta).
---@param dir string
function M.watch(dir)
  if watchers[dir] then
    return
  end
  local w = vim.uv.new_fs_event()
  if not w then
    return
  end
  watchers[dir] = w
  w:start(dir .. "/inbox", {}, function(_, file)
    local name = file and file:match("^(.+)%.md$")
    if name then
      pending[name] = true
      vim.schedule(M.deliver)
    end
  end)
end

---@param name string
---@return boolean
local function allow_nudge(name)
  local now, recent = os.time(), {}
  for _, t in ipairs(nudges[name] or {}) do
    if now - t < NUDGE_WINDOW then
      recent[#recent + 1] = t
    end
  end
  nudges[name] = recent
  if #recent >= NUDGE_LIMIT then
    return false
  end
  table.insert(recent, now)
  return true
end

--- Bekleyen postası olan ve boşta duran AI'lara bildirir (termtabs zamanlayıcısı da çağırır).
function M.deliver()
  for _, tab in ipairs(tt().tabs("claude")) do
    local name = tab.ai_name
    local idle = tab.meta and tab.meta.status == "idle"
    if name and pending[name] and tab.job and idle and allow_nudge(name) then
      pending[name] = nil
      local inbox = ("%s/inbox/%s.md"):format(M.dir(tab.cwd), name)
      local text
      if tab.agent == "ollama" then
        local fd = io.open(inbox, "r")
        local all = fd and fd:read("*a") or ""
        if fd then
          fd:close()
        end
        text = vim.trim(all:sub((offsets[name] or 0) + 1)):gsub("%s*\n%s*", " ")
        offsets[name] = #all
      else
        text = ("You have new mail in %s — read the new messages at the end and act on them."):format(inbox)
      end
      if text ~= "" then
        vim.fn.chansend(tab.job, text)
        vim.defer_fn(function()
          pcall(vim.fn.chansend, tab.job, "\r")
        end, 80)
      end
    end
  end
end

--- Zamanlayıcı adımı: bekleyen posta varsa (panel gizli olsa da) AI durumlarını tazeleyip iletir.
function M.tick()
  if not next(pending) then
    return
  end
  for _, tab in ipairs(tt().tabs("claude")) do
    if pending[tab.ai_name or ""] then
      tt().refresh_tab(tab, "claude")
    end
  end
  M.deliver()
end

--- Sekmenin bekleyen postası var mı (özet ekranı için)?
---@param tab termtabs.Tab
---@return boolean
function M.has_mail(tab)
  return tab.ai_name ~= nil and pending[tab.ai_name] == true
end

return M
