local M = {}

local root = vim.fs.normalize("~/.claude")
local max_chunk = 8 * 1024 * 1024
local json_opts = { luanil = { object = true, array = true } }

---@class termtabs.Transcript
---@field offset integer okunan bayt sayısı
---@field rest string son okumada yarım kalan satır
---@field ids table<string, table> mesaj id'si -> o mesajın usage değerleri (aynı mesaj birden çok satıra yazılır)
---@field input integer
---@field output integer
---@field cache_read integer
---@field cache_create integer
---@field context integer son ana-akış isteğinin bağlam boyutu
---@field ai_title string?
---@field custom_title string?
---@field last_tool string?
---@field last_text string?
---@field last_prompt string?
---@field mtime integer?

---@type table<string, termtabs.Transcript>
local cache = {}

--- Dosyanın tamamını okur; yoksa nil döner.
---@param path string
---@return string?
local function read_file(path)
  local fd = vim.uv.fs_open(path, "r", 438)
  if not fd then
    return
  end
  local stat = vim.uv.fs_fstat(fd)
  local data = stat and vim.uv.fs_read(fd, stat.size, 0)
  vim.uv.fs_close(fd)
  return data
end

--- Metni tek satıra indirir (ilk dolu satır, boşluklar sadeleşir) ve `max` karakterde keser.
---@param s any
---@param max? integer
---@return string?
function M.one_line(s, max)
  if type(s) ~= "string" then
    return
  end
  local line = s:match("[^\r\n]*%S[^\r\n]*")
  if not line then
    return
  end
  line = vim.trim(line:gsub("%s+", " "))
  max = max or 120
  if vim.fn.strchars(line) > max then
    line = vim.fn.strcharpart(line, 0, max - 1) .. "…"
  end
  return line
end

--- Araç çağrısını "Araç: ayrıntı" biçiminde özetler (Bash açıklaması, dosya adı, arama deseni vb.).
---@param name string
---@param input any
---@return string
local function tool_summary(name, input)
  input = type(input) == "table" and input or {}
  local path = input.file_path or input.notebook_path or input.path
  local detail = input.description
    or (type(path) == "string" and vim.fs.basename(path))
    or input.pattern
    or input.command
    or input.url
    or input.query
    or input.prompt
  if type(detail) ~= "string" then
    for _, v in pairs(input) do
      if type(v) == "string" then
        detail = v
        break
      end
    end
  end
  detail = M.one_line(detail, 100)
  return detail and (name .. ": " .. detail) or name
end

---@return termtabs.Transcript
local function new_state()
  return { offset = 0, rest = "", ids = {}, input = 0, output = 0, cache_read = 0, cache_create = 0, context = 0 }
end

--- Mesajın usage değerini toplamlara ekler; aynı mesaj id'si tekrar gelirse eskisini düşüp yenisini ekler.
---@param st termtabs.Transcript
---@param id string
---@param usage table
---@param sidechain boolean
local function add_usage(st, id, usage, sidechain)
  local new = {
    input = usage.input_tokens or 0,
    output = usage.output_tokens or 0,
    cache_read = usage.cache_read_input_tokens or 0,
    cache_create = usage.cache_creation_input_tokens or 0,
  }
  local old = st.ids[id] or {}
  for k, v in pairs(new) do
    st[k] = st[k] + v - (old[k] or 0)
  end
  st.ids[id] = new
  if not sidechain then
    st.context = new.input + new.cache_read + new.cache_create
  end
end

--- Transcript'in tek bir JSONL satırını işler. Pahalı JSON çözümlemesinden önce düz metin ön-filtresi uygulanır.
---@param st termtabs.Transcript
---@param line string
local function handle_line(st, line)
  local kind = line:match('^{"type":"([%w_-]+)"') or line:match('"type":"(assistant)"') or line:match('"type":"(user)"')
  if kind == "user" and (line:find('"tool_use_id"', 1, true) or line:find('"isMeta":true', 1, true)) then
    return
  end
  if not (kind == "assistant" or kind == "user" or kind == "ai-title" or kind == "custom-title") then
    return
  end
  local ok, rec = pcall(vim.json.decode, line, json_opts)
  if not ok or type(rec) ~= "table" then
    return
  end
  if rec.type == "ai-title" then
    st.ai_title = rec.aiTitle or st.ai_title
  elseif rec.type == "custom-title" then
    st.custom_title = rec.customTitle or st.custom_title
  elseif rec.type == "assistant" and type(rec.message) == "table" then
    local msg = rec.message
    if type(msg.usage) == "table" then
      add_usage(st, msg.id or rec.uuid or tostring(st.offset), msg.usage, rec.isSidechain == true)
    end
    if rec.isSidechain then
      return
    end
    for _, c in ipairs(type(msg.content) == "table" and msg.content or {}) do
      if c.type == "tool_use" then
        st.last_tool = tool_summary(c.name or "?", c.input)
      elseif c.type == "text" then
        st.last_text = M.one_line(c.text) or st.last_text
      end
    end
  elseif rec.type == "user" and not rec.isSidechain and type(rec.message) == "table" then
    local content = rec.message.content
    if type(content) == "table" then
      content = vim.iter(content):find(function(c)
        return c.type == "text"
      end)
      content = content and content.text
    end
    if type(content) == "string" and not content:match("^%s*<") then
      st.last_prompt = M.one_line(content) or st.last_prompt
    end
  end
end

--- Transcript'in son okumadan bu yana eklenen kısmını okur ve durumu günceller. Tek çağrıda en fazla 8 MB okunur,
--- böylece büyük (--resume ile açılmış) oturumlar arayüzü uzun süre dondurmaz; kalanı sonraki yenilemede okunur.
---@param path string
---@return termtabs.Transcript
function M.read(path)
  local st = cache[path] or new_state()
  cache[path] = st
  local fd = vim.uv.fs_open(path, "r", 438)
  if not fd then
    return st
  end
  local stat = vim.uv.fs_fstat(fd)
  if stat and stat.size < st.offset then
    st = new_state()
    cache[path] = st
  end
  if stat and stat.size > st.offset then
    local data = vim.uv.fs_read(fd, math.min(stat.size - st.offset, max_chunk), st.offset) or ""
    st.offset = st.offset + #data
    data = st.rest .. data
    local start = 1
    while true do
      local nl = data:find("\n", start, true)
      if not nl then
        break
      end
      handle_line(st, data:sub(start, nl - 1))
      start = nl + 1
    end
    st.rest = data:sub(start)
  end
  st.mtime = stat and stat.mtime.sec or st.mtime
  vim.uv.fs_close(fd)
  return st
end

--- Claude Code sürecinin `~/.claude/sessions/<pid>.json` kaydını okur: sessionId, cwd, status (busy/idle/waiting),
--- waitingFor, name, nameSource. Claude Code bu dosyayı her durum değişiminde günceller; /clear sonrası sessionId değişir.
---@param pid integer
---@return table?
function M.process_info(pid)
  local data = read_file(("%s/sessions/%d.json"):format(root, pid))
  local ok, info = pcall(vim.json.decode, data or "", json_opts)
  return ok and type(info) == "table" and info or nil
end

--- Oturumun transcript yolunu bulur: `~/.claude/projects/<cwd'deki harf/rakam dışı karakterler '-'>/<sessionId>.jsonl`.
--- Kaçış kuralı tutmazsa tüm proje klasörlerinde aranır. Oturumda henüz mesaj yoksa dosya da yoktur.
---@param info table
---@return string?
function M.transcript_path(info)
  if type(info.sessionId) ~= "string" then
    return
  end
  if type(info.cwd) == "string" then
    local path = ("%s/projects/%s/%s.jsonl"):format(root, info.cwd:gsub("[^%w]", "-"), info.sessionId)
    if vim.uv.fs_stat(path) then
      return path
    end
  end
  return vim.fn.glob(("%s/projects/*/%s.jsonl"):format(root, info.sessionId), true, true)[1]
end

--- Oturumun alt ajan (subagents/*.jsonl) transcript'lerindeki token toplamı.
---@param path string ana transcript yolu
---@return integer tokens, integer count
local function subagent_tokens(path)
  local dir = path:gsub("%.jsonl$", "") .. "/subagents"
  local total, count = 0, 0
  if not vim.uv.fs_stat(dir) then
    return 0, 0
  end
  for name, kind in vim.fs.dir(dir) do
    if kind == "file" and name:match("%.jsonl$") then
      local st = M.read(dir .. "/" .. name)
      total = total + st.input + st.output + st.cache_read + st.cache_create
      count = count + 1
    end
  end
  return total, count
end

---@class termtabs.Snapshot
---@field status string? busy | idle | waiting
---@field waiting_for string?
---@field session_id string?
---@field cwd string?
---@field title string? /rename adı > kullanıcı adı > Claude'un ürettiği başlık
---@field input integer
---@field output integer
---@field cache_read integer
---@field cache_create integer
---@field total integer
---@field context integer
---@field subagents integer alt ajanların harcadığı token
---@field subagent_count integer
---@field last_tool string?
---@field last_text string?
---@field last_prompt string?
---@field mtime integer?

--- Claude Code sürecinin (pid) anlık durumu: oturum kaydı + transcript'ten başlık, token ve son etkinlik.
---@param pid integer
---@return termtabs.Snapshot?
function M.snapshot(pid)
  local info = M.process_info(pid)
  if not info then
    return
  end
  local path = M.transcript_path(info)
  local st = path and M.read(path) or new_state()
  local sub, sub_count = 0, 0
  if path then
    sub, sub_count = subagent_tokens(path)
  end
  local user_name = info.nameSource == "user" and info.name or nil
  return {
    status = info.status,
    waiting_for = info.waitingFor,
    session_id = info.sessionId,
    cwd = info.cwd,
    title = st.custom_title or user_name or st.ai_title,
    input = st.input,
    output = st.output,
    cache_read = st.cache_read,
    cache_create = st.cache_create,
    total = st.input + st.output + st.cache_read + st.cache_create,
    context = st.context,
    subagents = sub,
    subagent_count = sub_count,
    last_tool = st.last_tool,
    last_text = st.last_text,
    last_prompt = st.last_prompt,
    mtime = st.mtime,
  }
end

--- Token sayısını kısaltır: 950 -> "950", 12345 -> "12.3K", 2345678 -> "2.35M".
---@param n integer?
---@return string
function M.fmt_tokens(n)
  n = n or 0
  if n >= 1e6 then
    return ("%.2fM"):format(n / 1e6)
  elseif n >= 1e3 then
    return ("%.1fK"):format(n / 1e3)
  end
  return tostring(n)
end

return M
