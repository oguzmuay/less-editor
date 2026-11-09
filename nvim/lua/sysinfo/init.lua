local M = {}

local uv = vim.uv
local INTERVAL = 5000

local state = {
  timer = nil,
  cpu = nil, ---@type { busy: number, total: number }?
  text = "",
  ip = nil, ---@type string?
  mem_used = nil, ---@type number?
}

local ICON = {
  cpu = vim.fn.nr2char(0xF4BC),
  mem = vim.fn.nr2char(0xF2DB),
  disk = vim.fn.nr2char(0xF02CA),
  ip = vim.fn.nr2char(0xF0A60),
}

--- Bayt değerini kısa GB metnine çevirir (`9.8G`, 100 GB üstünde ondalıksız).
---@param bytes number
---@return string
local function gb(bytes)
  local g = bytes / 1024 ^ 3
  return (g >= 100 and string.format("%d", g) or string.format("%.1f", g)) .. "G"
end

--- Tüm çekirdeklerin toplam CPU kullanımı (%): iki örnek arasındaki meşgul / toplam süre farkı. İlk çağrıda nil.
---@return integer?
local function cpu_percent()
  local busy, total = 0, 0
  for _, c in ipairs(uv.cpu_info() or {}) do
    local t = c.times
    local sum = t.user + t.nice + t.sys + t.idle + t.irq
    total, busy = total + sum, busy + sum - t.idle
  end
  local prev = state.cpu
  state.cpu = { busy = busy, total = total }
  if not prev or total <= prev.total then
    return nil
  end
  return math.floor((busy - prev.busy) / (total - prev.total) * 100 + 0.5)
end

--- Kullanılan RAM'i arka planda günceller. macOS'ta `vm_stat` ile Activity Monitor'deki "Memory Used"a yakın değer
--- (uygulama + wired + sıkıştırılmış; libuv'nin "free" değeri önbelleği kullanılmış sayar), Linux'ta MemAvailable.
local function update_mem()
  local total = uv.get_total_memory()
  if vim.fn.has("mac") == 0 then
    local ok, lines = pcall(vim.fn.readfile, "/proc/meminfo")
    local avail = ok and table.concat(lines, "\n"):match("MemAvailable:%s+(%d+)")
    state.mem_used = avail and total - tonumber(avail) * 1024 or total - uv.get_free_memory()
    return
  end
  vim.system({ "vm_stat" }, { text = true }, function(res)
    local out = res.stdout or ""
    local page = tonumber(out:match("page size of (%d+)")) or 4096
    local function pages(name)
      return tonumber(out:match(name .. ":%s+(%d+)")) or 0
    end
    local anon = pages("Anonymous pages")
    local app = anon > 0 and (anon - pages("Pages purgeable")) or pages("Pages active")
    local used = (app + pages("Pages wired down") + pages("Pages occupied by compressor")) * page
    state.mem_used = math.min(used, total)
  end)
end

--- Yerel IPv4 adresi: önce en0/eth0/wlan0, yoksa döngü dışı ilk IPv4.
---@return string?
local function local_ip()
  local ifaces = uv.interface_addresses() or {}
  for _, name in ipairs({ "en0", "eth0", "wlan0", "en1" }) do
    for _, a in ipairs(ifaces[name] or {}) do
      if a.family == "inet" and not a.internal then
        return a.ip
      end
    end
  end
  for _, addrs in pairs(ifaces) do
    for _, a in ipairs(addrs) do
      if a.family == "inet" and not a.internal then
        return a.ip
      end
    end
  end
end

--- Örnekleri alıp durum satırı metnini yeniler: `CPU %`, `kullanılan/toplam RAM`, ev klasörünün diskinde boş alan.
local function sample()
  update_mem()
  local parts = {}
  local cpu = cpu_percent()
  parts[#parts + 1] = ICON.cpu .. " " .. (cpu and (cpu .. "%%") or "…") -- durum satırında `%` kaçışlı yazılır (E539)
  if state.mem_used then
    parts[#parts + 1] = ICON.mem .. " " .. gb(state.mem_used) .. "/" .. gb(uv.get_total_memory())
  end
  local fs = uv.fs_statfs(vim.env.HOME or "/")
  if fs then
    parts[#parts + 1] = ICON.disk .. " " .. gb(fs.bavail * fs.bsize) .. " free"
  end
  state.text = table.concat(parts, "  ")
  state.ip = local_ip()
end

--- Örnekleme zamanlayıcısını (5 sn) bir kez başlatır; Neovim'in kapanmasını bekletmez.
local function ensure_timer()
  if state.timer then
    return
  end
  update_mem()
  cpu_percent()
  state.timer = uv.new_timer()
  state.timer:start(1000, INTERVAL, vim.schedule_wrap(sample))
  state.timer:unref()
end

--- Durum satırı bileşeni: CPU, RAM ve disk özeti.
---@return string
function M.resources()
  ensure_timer()
  return state.text
end

--- Durum satırı bileşeni: yerel IP'yi "Leaked IP" şakasıyla gösterir (ağ yoksa boş).
---@return string
function M.leaked_ip()
  ensure_timer()
  return state.ip and (ICON.ip .. " Leaked IP: " .. state.ip) or ""
end

return M
