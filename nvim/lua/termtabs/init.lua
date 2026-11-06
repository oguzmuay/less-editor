local M = {}

local sessions = require("termtabs.sessions")

---@alias termtabs.GroupName "claude"|"shell"

---@class termtabs.Tab
---@field buf integer
---@field job integer?
---@field pid integer?
---@field n integer grup içindeki sıra numarası (yedek ad için)
---@field cwd string
---@field name string? kullanıcının verdiği ad (:R / :TerminalRename)
---@field auto string? otomatik ad (shell: süreç/klasör, Claude: oturum başlığı); zamanlayıcı günceller
---@field meta termtabs.Snapshot? AI sekmeleri (Claude dışında yalnız `status`, `simple = true`)
---@field agent string? AI türü (termtabs.agents.kinds)
---@field ai_name string? AI adı ("codex-1"); posta kutusu adı
---@field account table? Claude hesabı
---@field tick integer? son bakılan changedtick (Claude dışı durum tahmini)

---@class termtabs.Group
---@field tabs termtabs.Tab[]
---@field current integer? pencerede gösterilecek buffer (Claude için özet buffer'ı olabilir)
---@field active integer? son seçilen gerçek sekmenin buffer'ı (claudecode'un "aktif terminali")
---@field seq integer

---@type table<termtabs.GroupName, termtabs.Group>
local groups = {
  claude = { tabs = {}, seq = 0 },
  shell = { tabs = {}, seq = 0 },
}

local fallback_names = { claude = "Claude %d", shell = "Terminal %d" }
local winbar_expr = "%!v:lua.require'termtabs'.winbar()"
local timer = nil

--- Buffer'ın ait olduğu sekme grubunu ("claude" | "shell") döndürür.
---@param buf integer
---@return termtabs.GroupName?
function M.group_of(buf)
  return vim.api.nvim_buf_is_valid(buf) and vim.b[buf].termtabs or nil
end

--- Buffer'a ait sekmeyi ve grubunu bulur (özet buffer'ı sekme değildir).
---@param buf integer?
---@return termtabs.Tab?, termtabs.GroupName?
local function find_tab(buf)
  for name, g in pairs(groups) do
    for _, tab in ipairs(g.tabs) do
      if tab.buf == buf then
        return tab, name
      end
    end
  end
end

--- Mevcut sekmede (tabpage) grubun panel penceresini döndürür: içinde o gruba ait bir buffer gösteren pencere.
---@param group termtabs.GroupName
---@return integer?
function M.win(group)
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.api.nvim_win_get_config(win).relative == "" and M.group_of(vim.api.nvim_win_get_buf(win)) == group then
      return win
    end
  end
end

--- Gruptaki gerçek sekmeler (özet hariç).
---@param group termtabs.GroupName
---@return termtabs.Tab[]
function M.tabs(group)
  return groups[group].tabs
end

--- Tab çubuğundaki öğeler: Claude grubunda birden fazla oturum varken en solda özet, sonra sekmeler.
---@param group termtabs.GroupName
---@return { buf: integer, summary?: boolean, tab?: termtabs.Tab }[]
function M.items(group)
  local items = {}
  if group == "claude" and #groups.claude.tabs > 1 then
    table.insert(items, { buf = require("termtabs.summary").buf(), summary = true })
  end
  for _, tab in ipairs(groups[group].tabs) do
    table.insert(items, { buf = tab.buf, tab = tab })
  end
  return items
end

--- claudecode'un "aktif terminal"i: Claude grubunda son seçilen gerçek oturumun buffer'ı.
---@param group termtabs.GroupName
---@return integer?
function M.active(group)
  local g = groups[group]
  if g.active and find_tab(g.active) then
    return g.active
  end
  g.active = g.tabs[#g.tabs] and g.tabs[#g.tabs].buf or nil
  return g.active
end

--- Panel penceresinde gösterilecek buffer: seçili öğe hâlâ geçerliyse o, değilse aktif sekme.
---@param group termtabs.GroupName
---@return integer?
local function current(group)
  local g = groups[group]
  for _, item in ipairs(M.items(group)) do
    if item.buf == g.current then
      return g.current
    end
  end
  g.current = M.active(group)
  return g.current
end

--- Pencereye odaklanır; terminalse insert (terminal) moduna, değilse normal moda geçer.
---@param win integer
local function focus(win)
  vim.api.nvim_set_current_win(win)
  if vim.bo[vim.api.nvim_win_get_buf(win)].buftype == "terminal" then
    vim.cmd.startinsert()
  else
    vim.cmd.stopinsert()
  end
end

--- Panel penceresinin görünümünü ayarlar: tab çubuğu (winbar), sabit genişlik, satır numarası yok.
---@param win integer
local function decorate(win)
  for opt, val in pairs({
    winbar = winbar_expr,
    winfixwidth = true,
    number = false,
    relativenumber = false,
    signcolumn = "no",
  }) do
    vim.wo[win][opt] = val
  end
end

--- Buffer'ı pencereye koyar. Pencere mevcut pencere değilse autocmd'ler tetiklenmez; aksi halde workbench'in
--- terminal BufWinEnter autocmd'i `startinsert`'i o anki (başka) pencere için kurar.
---@param win integer
---@param buf integer
local function set_buf(win, buf)
  if vim.api.nvim_win_get_buf(win) == buf then
    return
  end
  if win == vim.api.nvim_get_current_win() then
    vim.api.nvim_win_set_buf(win, buf)
  else
    vim.cmd(("noautocmd call nvim_win_set_buf(%d, %d)"):format(win, buf))
  end
  decorate(win)
end

--- Grubun panel penceresini sağda açar (yerleşimi workbench düzeltir: üstte Claude, altta terminal).
---@param buf integer
---@param width? integer
---@return integer
local function open_window(buf, width)
  local win = vim.api.nvim_open_win(buf, false, {
    split = "right",
    win = -1,
    width = width or math.floor(vim.o.columns * 0.38),
  })
  decorate(win)
  require("workbench").arrange_right_column()
  return win
end

--- Grubun panelini gösterir (gerekirse pencereyi açar); `enter` ise odaklar. Hiç sekme yoksa nil döner.
---@param group termtabs.GroupName
---@param enter? boolean
---@return integer?
function M.show(group, enter)
  local win = M.win(group)
  if not win then
    local buf = current(group)
    if not buf then
      return
    end
    win = open_window(buf)
  end
  if enter then
    focus(win)
  end
  return win
end

--- Grubun panel penceresini gizler (süreçler arka planda çalışmaya devam eder).
---@param group termtabs.GroupName
function M.hide(group)
  local win = M.win(group)
  if win then
    pcall(vim.api.nvim_win_hide, win)
  end
end

--- Öğeyi (sekme ya da özet) seçer: panel penceresinde gösterir, gerekirse pencereyi açar.
---@param buf integer
---@param enter? boolean
function M.select(buf, enter)
  local group = M.group_of(buf)
  if not group then
    return
  end
  local g = groups[group]
  g.current = buf
  if find_tab(buf) then
    g.active = buf
  end
  local win = M.win(group)
  if win then
    set_buf(win, buf)
  else
    win = open_window(buf)
  end
  if vim.b[buf].termtabs_summary then
    require("termtabs.summary").render()
  end
  if enter then
    focus(win)
  end
  vim.cmd.redrawstatus({ bang = true })
end

--- Gruba yeni bir terminal sekmesi ekler ve komutu panel penceresinde başlatır.
---@param group termtabs.GroupName
---@param argv string|string[]
---@param opts? { cwd?: string, env?: table<string, string>, enter?: boolean, width?: integer, on_exit?: fun(code: integer) }
---@return integer? buf
function M.spawn(group, argv, opts)
  opts = opts or {}
  local g = groups[group]
  local buf = vim.api.nvim_create_buf(false, false)
  vim.b[buf].termtabs = group
  g.seq = g.seq + 1
  local tab = {
    buf = buf,
    n = g.seq,
    cwd = opts.cwd or vim.fn.getcwd(),
    account = opts.account,
    agent = opts.agent or (group == "claude" and "claude" or nil),
    ai_name = opts.ai_name,
  }
  table.insert(g.tabs, tab)
  g.current, g.active = buf, buf
  local win = M.win(group)
  if win then
    set_buf(win, buf)
  else
    win = open_window(buf, opts.width)
  end
  local ok, job = pcall(vim.api.nvim_win_call, win, function()
    return vim.fn.jobstart(argv, {
      term = true,
      cwd = tab.cwd,
      env = opts.env and next(opts.env) and opts.env or nil,
      on_exit = function(_, code)
        vim.schedule(function()
          if opts.on_exit then
            opts.on_exit(code)
          end
          M.remove(buf)
        end)
      end,
    })
  end)
  if not ok or type(job) ~= "number" or job <= 0 then
    vim.notify(("Failed to start terminal: %s"):format(ok and tostring(job) or job), vim.log.levels.ERROR)
    M.remove(buf)
    return
  end
  tab.job, tab.pid = job, vim.fn.jobpid(job)
  vim.bo[buf].bufhidden = "hide"
  M.refresh_tab(tab, group)
  M.start_timer()
  if opts.enter ~= false then
    focus(win)
  end
  vim.cmd.redrawstatus({ bang = true })
  return buf
end

--- Sekmeyi listeden çıkarır (süreç bittiğinde çağrılır): panel penceresi komşu sekmeye geçer,
--- hiç sekme kalmazsa pencere kapanır; buffer silinir.
---@param buf integer
function M.remove(buf)
  local tab, group = find_tab(buf)
  if not tab then
    return
  end
  local g = groups[group]
  local idx = vim.iter(ipairs(g.tabs)):find(function(_, t)
    return t == tab
  end)
  table.remove(g.tabs, idx)
  local neighbor = g.tabs[idx] or g.tabs[idx - 1]
  if g.active == buf then
    g.active = neighbor and neighbor.buf
  end
  if g.current == buf then
    g.current = g.active
  end
  local next_buf = current(group)
  local summary = require("termtabs.summary")
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    local shown = vim.api.nvim_win_get_buf(win)
    if shown == buf or (group == "claude" and shown == summary.existing() and next_buf ~= shown) then
      if next_buf then
        set_buf(win, next_buf)
      elseif not pcall(vim.api.nvim_win_close, win, true) then
        vim.api.nvim_win_set_buf(win, vim.api.nvim_create_buf(true, false))
        vim.wo[win].winbar = ""
      end
    end
  end
  if vim.api.nvim_buf_is_valid(buf) then
    pcall(vim.api.nvim_buf_delete, buf, { force = true })
  end
  if group == "claude" then
    vim.cmd.checktime()
  end
  vim.cmd.redrawstatus({ bang = true })
end

--- Sekmeyi kapatır: süreci sonlandırır (Claude oturumu için onay ister); sekme süreç bitince `remove` ile kalkar.
---@param buf? integer varsayılan: mevcut buffer
function M.close(buf)
  local tab, group = find_tab(buf or vim.api.nvim_get_current_buf())
  if not tab then
    vim.notify("No terminal/Claude tab to close", vim.log.levels.WARN)
    return
  end
  if
    group == "claude"
    and vim.fn.confirm(("Close session '%s'?"):format(M.label(tab, group)), "&Yes\n&No", 2) ~= 1
  then
    return
  end
  if tab.job then
    vim.fn.jobstop(tab.job)
  else
    M.remove(tab.buf)
  end
end

--- Shell terminalinin otomatik adı: shell'in pencere başlığı (oh-my-zsh: boştayken klasör, komut çalışırken komut),
--- başlık yoksa ön plandaki alt süreç, o da yoksa başlangıç klasörünün adı.
---@param tab termtabs.Tab
---@return string
local function shell_title(tab)
  local title = vim.b[tab.buf].term_title
  if type(title) == "string" and title ~= "" and not title:find("^term://") and title ~= vim.o.shell then
    local path = title:match("^[^@%s]+@[^:]+:%s*(.+)$")
    if path then
      return path == "~" and "~" or vim.fs.basename(path)
    end
    return title
  end
  local ok, children = pcall(vim.api.nvim_get_proc_children, tab.pid or 0)
  local child = ok and children[#children]
  local proc = child and vim.api.nvim_get_proc(child)
  if proc and proc.name then
    return vim.fs.basename(proc.name)
  end
  return vim.fs.basename(tab.cwd)
end

--- Sekmenin otomatik adını (ve Claude için oturum bilgisini) günceller.
---@param tab termtabs.Tab
---@param group termtabs.GroupName
function M.refresh_tab(tab, group)
  if group == "shell" then
    tab.auto = shell_title(tab)
    return
  end
  if tab.agent ~= "claude" then
    local tick = vim.api.nvim_buf_is_valid(tab.buf) and vim.api.nvim_buf_get_changedtick(tab.buf) or 0
    tab.meta = { status = tab.tick and tick ~= tab.tick and "busy" or "idle", simple = true }
    tab.tick = tick
    return
  end
  tab.meta = tab.pid and sessions.snapshot(tab.pid, tab.account and tab.account.dir) or nil
  tab.auto = tab.meta and tab.meta.title or nil
end

--- Sekme adı: kullanıcının verdiği ad > otomatik ad > "Claude N" / "Terminal N".
---@param tab termtabs.Tab
---@param group termtabs.GroupName
---@return string
function M.label(tab, group)
  local kind = tab.agent and require("termtabs.agents").kinds[tab.agent]
  local n = tab.ai_name and tab.ai_name:match("%-(%d+)$")
  return tab.name or tab.auto or (kind and n and (kind.label .. " " .. n)) or fallback_names[group]:format(tab.n)
end

--- Claude oturumunun durum simgesi: ● çalışıyor, ◐ onay/girdi bekliyor, ○ boşta.
---@param tab termtabs.Tab
---@return string
function M.status_icon(tab)
  local status = tab.meta and tab.meta.status
  return ({ busy = "●", waiting = "◐", idle = "○" })[status] or "◌"
end

--- Metni `max` karakterde keser.
---@param s string
---@param max integer
---@return string
local function truncate(s, max)
  if vim.fn.strchars(s) <= max then
    return s
  end
  return vim.fn.strcharpart(s, 0, max - 1) .. "…"
end

--- Panel penceresinin tab çubuğu (winbar ifadesi). Her öğe `%@` ile tıklanabilir:
--- sol tık seç, orta tık kapat, sağ tık yeniden adlandır; ✕ kapatır, + yeni sekme açar.
---@return string
function M.winbar()
  local win = vim.g.statusline_winid or vim.api.nvim_get_current_win()
  local shown = vim.api.nvim_win_get_buf(win)
  local group = M.group_of(shown)
  if not group then
    return ""
  end
  local items = M.items(group)
  local max = math.max(6, math.floor((vim.api.nvim_win_get_width(win) - 4) / math.max(#items, 1)) - 6)
  local parts = {}
  for _, item in ipairs(items) do
    local selected = item.buf == shown
    local label
    if item.summary then
      label = "≡ Summary"
    elseif group == "claude" then
      label = M.status_icon(item.tab) .. " " .. truncate(M.label(item.tab, group), max)
    else
      label = " " .. truncate(M.label(item.tab, group), max)
    end
    local part = ("%%#%s#%%%d@v:lua.TermTabsClick@ %s %%X"):format(
      selected and "TermTabsSel" or "TermTabs",
      item.buf,
      label:gsub("%%", "%%%%")
    )
    if selected and not item.summary then
      part = part .. ("%%%d@v:lua.TermTabsClose@✕ %%X"):format(item.buf)
    end
    table.insert(parts, part)
  end
  local new_id = group == "claude" and 1 or 2
  return table.concat(parts, "%#TermTabsFill# ") .. ("%%#TermTabsFill# %%%d@v:lua.TermTabsNew@ + %%X"):format(new_id)
end

--- Odak bir panel sekmesindeyse (ya da yeniden adlandırma için) hedef sekmeyi bulur:
--- önce mevcut buffer, sonra görünür terminal panelinin, sonra Claude panelinin seçili sekmesi.
---@return termtabs.Tab?, termtabs.GroupName?
local function target_tab()
  local tab, group = find_tab(vim.api.nvim_get_current_buf())
  if tab then
    return tab, group
  end
  for _, name in ipairs({ "shell", "claude" }) do
    local win = M.win(name)
    tab = win and find_tab(vim.api.nvim_win_get_buf(win))
    if tab then
      return tab, name
    end
  end
end

--- Sekmeyi yeniden adlandırır. Ad boşsa otomatik ada döner; ad verilmezse sorar.
---@param name? string
---@param buf? integer
function M.rename(name, buf)
  local tab, group
  if buf then
    tab, group = find_tab(buf)
  else
    tab, group = target_tab()
  end
  if not tab then
    vim.notify("No terminal tab found to rename", vim.log.levels.WARN)
    return
  end
  if name == nil then
    vim.ui.input({ prompt = "Tab name (empty = automatic): ", default = tab.name or "" }, function(input)
      if input then
        M.rename(input, tab.buf)
      end
    end)
    return
  end
  name = vim.trim(name)
  tab.name = name ~= "" and name or nil
  vim.cmd.redrawstatus({ bang = true })
  vim.notify(("Tab name: %s"):format(M.label(tab, group)))
end

--- Yeni shell terminali sekmesi açar (mevcut çalışma klasöründe).
---@param enter? boolean
function M.new_shell(enter)
  M.spawn("shell", vim.o.shell, { enter = enter ~= false })
end

--- Cmd+T: Claude panelinde yeni Claude Code oturumu, terminal panelinde yeni shell sekmesi;
--- başka yerde kod penceresinde yeni boş buffer.
function M.new_tab()
  local group = M.group_of(vim.api.nvim_get_current_buf())
  if group == "claude" then
    require("termtabs.agents").pick()
  elseif group == "shell" then
    M.new_shell(true)
  else
    local wb = require("workbench")
    local had_code = wb.code_win() ~= nil
    vim.cmd.stopinsert()
    wb.code()
    if had_code then
      vim.cmd.enew()
    end
    vim.b.workbench_scratch = true
    vim.bo.modifiable = true
  end
end

--- Odak bir panel penceresindeyse o panelin sekmeleri arasında `delta` kadar döner.
---@param delta integer
---@return boolean handled odak panelde değilse false (çağıran editör tablarına geçer)
function M.cycle(delta)
  local buf = vim.api.nvim_get_current_buf()
  local group = M.group_of(buf)
  if not group then
    return false
  end
  local items = M.items(group)
  local idx = vim.iter(ipairs(items)):find(function(_, item)
    return item.buf == buf
  end) or 1
  local item = items[(idx - 1 + delta) % #items + 1]
  if item and item.buf ~= buf then
    M.select(item.buf, true)
  end
  return true
end

--- Görünür paneller varsa otomatik adları, Claude oturum bilgilerini ve özeti yeniler.
local function refresh()
  local visible = { claude = M.win("claude") ~= nil, shell = M.win("shell") ~= nil }
  if not (visible.claude or visible.shell) then
    return
  end
  for name, g in pairs(groups) do
    if visible[name] then
      for _, tab in ipairs(g.tabs) do
        M.refresh_tab(tab, name)
      end
    end
  end
  require("termtabs.summary").render()
  vim.cmd.redrawstatus({ bang = true })
end

--- Sekmeler varken 2 saniyede bir `refresh` çalıştıran zamanlayıcıyı başlatır; hiç sekme kalmayınca durdurur.
function M.start_timer()
  if timer then
    return
  end
  timer = assert(vim.uv.new_timer())
  timer:start(
    2000,
    2000,
    vim.schedule_wrap(function()
      if #groups.claude.tabs + #groups.shell.tabs == 0 then
        timer:stop()
        timer:close()
        timer = nil
        return
      end
      refresh()
      require("termtabs.agents").tick()
    end)
  )
end

--- Tab çubuğu renkleri (renk şeması değişince yeniden bağlanır).
local function highlights()
  vim.api.nvim_set_hl(0, "TermTabs", { link = "TabLine", default = true })
  vim.api.nvim_set_hl(0, "TermTabsSel", { link = "TabLineSel", default = true })
  vim.api.nvim_set_hl(0, "TermTabsFill", { link = "TabLineFill", default = true })
end

--- `%@` tıklama işleyicileri: minwid = sekmenin buffer'ı (TermTabsNew için 1 = Claude, 2 = terminal).
function _G.TermTabsClick(minwid, _, button)
  if not vim.api.nvim_buf_is_valid(minwid) then
    return
  end
  if button == "m" then
    M.close(minwid)
  elseif button == "r" then
    if find_tab(minwid) then
      M.rename(nil, minwid)
    end
  else
    M.select(minwid, true)
  end
end

function _G.TermTabsClose(minwid)
  M.close(minwid)
end

function _G.TermTabsNew(minwid)
  local win = M.win(minwid == 1 and "claude" or "shell")
  if win then
    vim.api.nvim_set_current_win(win)
  end
  if minwid == 1 then
    require("termtabs.agents").pick()
  else
    M.new_shell(true)
  end
end

--- Komutları, renkleri ve autocmd'leri kurar: `:R`, `:TerminalRename`, `:TerminalNew`, `:ClaudeNew`, `:TermTabClose`.
function M.setup()
  highlights()
  local group = vim.api.nvim_create_augroup("termtabs", { clear = true })
  vim.api.nvim_create_autocmd("ColorScheme", { group = group, callback = highlights })
  vim.api.nvim_create_autocmd("BufWinEnter", {
    group = group,
    callback = function(ev)
      local win = vim.api.nvim_get_current_win()
      if M.group_of(ev.buf) then
        decorate(win)
      elseif vim.wo[win].winbar == winbar_expr then
        vim.wo[win].winbar = ""
      end
    end,
  })
  local rename = function(opts)
    M.rename(opts.args ~= "" and opts.args or nil)
  end
  vim.api.nvim_create_user_command(
    "TerminalRename",
    rename,
    { nargs = "?", desc = "Rename Terminal/Claude tab" }
  )
  vim.api.nvim_create_user_command("R", rename, { nargs = "?", desc = "Rename Terminal/Claude tab" })
  vim.api.nvim_create_user_command("TerminalNew", function()
    M.new_shell(true)
  end, { desc = "New terminal tab" })
  vim.api.nvim_create_user_command("ClaudeNew", function()
    require("termtabs.claude").new_session()
  end, { desc = "New Claude Code session tab" })
  vim.api.nvim_create_user_command("TermTabClose", function()
    M.close()
  end, { desc = "Close Terminal/Claude tab" })
end

return M
