local M = {}

local state = {
  code_win = nil,
}

local schedule_arrange

--- Pencere yüzen (floating) değil ve içinde normal bir dosya buffer'ı varsa "kod penceresi" sayılır.
---@param win integer
---@return boolean
local function is_code_win(win)
  if not vim.api.nvim_win_is_valid(win) then
    return false
  end
  if vim.api.nvim_win_get_config(win).relative ~= "" then
    return false
  end
  local buf = vim.api.nvim_win_get_buf(win)
  return vim.bo[buf].buftype == "" and vim.bo[buf].filetype ~= "neo-tree" and not vim.w[win].workbench_bottom
end

--- Mevcut sekmede, verilen koşulu sağlayan ilk pencereyi döndürür.
---@param pred fun(win: integer, buf: integer): boolean
---@return integer?
local function find_win(pred)
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.api.nvim_win_get_config(win).relative == "" and pred(win, vim.api.nvim_win_get_buf(win)) then
      return win
    end
  end
end

--- Verilen filetype'a sahip ilk pencereyi bulur.
---@param ft string|string[]
---@return integer?
local function find_ft(ft)
  local fts = type(ft) == "table" and ft or { ft }
  return find_win(function(_, buf)
    return vim.tbl_contains(fts, vim.bo[buf].filetype)
  end)
end

--- Pencereye odaklanır; terminal ise doğrudan yazmaya hazır olsun diye insert moduna geçer.
---@param win integer
local function focus(win)
  vim.api.nvim_set_current_win(win)
  if vim.bo[vim.api.nvim_win_get_buf(win)].buftype == "terminal" then
    vim.cmd.startinsert()
  else
    vim.cmd.stopinsert()
  end
end

--- IntelliJ tarzı panel davranışı: panel kapalıysa açar, açık ama odakta değilse odaklar,
--- zaten odaktaysa kapatıp kod penceresine döner.
---@param find fun(): integer?
---@param open fun()
---@param close fun(win: integer)
local function panel(find, open, close)
  local win = find()
  if not win then
    open()
    return
  end
  if win == vim.api.nvim_get_current_win() then
    close(win)
    M.code({ pinned = false })
  else
    focus(win)
  end
end

--- Son kullanılan kod penceresini takip eden autocmd'leri ve terminal davranışlarını kurar.
function M.setup()
  if vim.g.workbench_keep_code == nil then
    vim.g.workbench_keep_code = vim.fn.argc(-1) == 0
  end
  require("termtabs").setup()
  local group = vim.api.nvim_create_augroup("workbench", { clear = true })
  vim.api.nvim_create_autocmd("WinEnter", {
    group = group,
    callback = function()
      local win = vim.api.nvim_get_current_win()
      state.prev_focus, state.focus = state.focus, win
      if is_code_win(win) then
        state.code_win = win
      end
    end,
  })
  vim.api.nvim_create_autocmd({ "BufWinEnter", "TermOpen", "WinNew", "WinResized" }, {
    group = group,
    callback = function()
      schedule_arrange()
    end,
  })
  vim.api.nvim_create_autocmd("BufWinEnter", {
    group = group,
    callback = function(ev)
      M.guard_strip(ev.buf)
    end,
  })
  vim.api.nvim_create_autocmd("CmdlineLeave", {
    group = group,
    pattern = ":",
    callback = M.guard_cmdline,
  })
  vim.api.nvim_create_autocmd({ "WinEnter", "BufWinEnter" }, {
    group = group,
    pattern = "term://*",
    callback = function()
      vim.cmd.startinsert()
    end,
  })
  vim.api.nvim_create_autocmd({ "BufEnter", "BufWinEnter" }, {
    group = group,
    callback = function(ev)
      M.lock_empty(ev.buf)
    end,
  })
  vim.api.nvim_create_autocmd("FileType", {
    group = group,
    pattern = "neo-tree",
    callback = function(ev)
      M.list_cursor(ev.buf)
    end,
  })
  vim.api.nvim_create_autocmd("SessionLoadPost", {
    group = group,
    callback = function()
      vim.schedule(M.restore_pins)
    end,
  })
  vim.schedule(M.restore_pins)
end

--- Son tab kapanınca kalan boş, isimsiz buffer'ı salt okunur yapar (yanlışlıkla yazılıp "kaydedilmedi" uyarısı
--- çıkmasın). Cmd+T'nin açtığı boş buffer (`b:workbench_scratch`) hariç. Bu buffer'a dosya açılınca Vim onu yeniden
--- kullanır ve `modifiable` sıfırlanır.
---@param buf integer
function M.lock_empty(buf)
  if vim.b[buf].workbench_scratch or not vim.api.nvim_buf_is_valid(buf) then
    return
  end
  local bo = vim.bo[buf]
  if bo.buftype == "" and bo.modifiable and not bo.modified and vim.api.nvim_buf_get_name(buf) == ""
    and vim.api.nvim_buf_line_count(buf) == 1 and vim.api.nvim_buf_get_lines(buf, 0, 1, false)[1] == "" then
    bo.modifiable = false
  end
end

--- Cmd+1: Dosya gezgini (neo-tree). Git paneliyle aynı yuvayı paylaşır: açılırken Git panelini kapatır.
function M.files()
  panel(function()
    return find_ft("neo-tree")
  end, function()
    local git = require("gitstatus")
    local gw = git.find_panel()
    if gw then
      git.close_panel(gw)
    end
    vim.cmd("Neotree focus reveal")
    M.arrange_layout()
  end, function()
    vim.cmd("Neotree close")
  end)
end

--- bufferline'ın grup modülünü döndürür (gerekirse eklentiyi yükler); yoksa nil.
---@return table?
local function bufferline_groups()
  local ok, groups = pcall(require, "bufferline.groups")
  return ok and groups or nil
end

--- Tab çubuğunda (bufferline sırasıyla) en baştaki pinli buffer'ı döndürür; pinli tab yoksa nil.
--- Sıra için bufferline durumu önce tazelenir (tabline henüz çizilmemiş olabilir).
---@return integer?
local function first_pinned_buf()
  local groups = bufferline_groups()
  if not groups or type(_G.nvim_bufferline) ~= "function" then
    return nil
  end
  pcall(_G.nvim_bufferline)
  for _, item in ipairs(require("bufferline.state").components or {}) do
    local el = item.as_element and item:as_element()
    if el and vim.api.nvim_buf_is_valid(el.id) and groups._is_pinned(el) then
      return el.id
    end
  end
end

--- Kod penceresinde en baştaki pinli tab'ı gösterir. Pinli buffer başka bir kod penceresinde zaten açıksa o pencereye
--- geçer. Pinli tab yoksa hiçbir şey yapmaz.
---@param win integer odaktaki kod penceresi
local function show_first_pinned(win)
  local buf = first_pinned_buf()
  if not buf or vim.api.nvim_win_get_buf(win) == buf then
    return
  end
  local shown = find_win(function(w, b)
    return b == buf and is_code_win(w)
  end)
  if shown then
    focus(shown)
  else
    vim.api.nvim_win_set_buf(win, buf)
  end
end

--- Cmd+2 / `:Code`: Son kullanılan kod penceresi. Yoksa ilk uygun pencere, o da yoksa yeni bir pencere.
--- Odak bir panelden (kod penceresi dışından) geliyorsa ve pinli tab varsa en baştaki pinli tab gösterilir
--- (`opts.pinned = false` bunu kapatır, `opts.force = true` odak zaten koddayken de uygular).
--- Tab gezme (Cmd+Shift+Sol/Sağ) ve panel kapatma gibi iç kullanımlar `pinned = false` ile çağırır.
---@param opts? { pinned?: boolean, force?: boolean }
function M.code(opts)
  opts = opts or {}
  local from_code = is_code_win(vim.api.nvim_get_current_win())
  local win = M.code_win()
  if win then
    focus(win)
  else
    vim.cmd("topleft vnew")
    win = vim.api.nvim_get_current_win()
  end
  if opts.pinned ~= false and (opts.force or not from_code) then
    show_first_pinned(win)
  end
end

--- Proje/workspace açılışı gibi "koda geçiş" anları için: kod penceresine odaklanır ve pinli tab varsa en baştakini
--- gösterir (odak zaten kod penceresinde olsa bile).
function M.focus_code()
  M.code({ force = true })
end

--- Cmd+P: Kod penceresindeki tab'ı (bufferline buffer'ı) pinler / pin'i kaldırır. Odak bir paneldeyse son kullanılan
--- kod penceresindeki tab'a uygulanır ve bildirim gösterilir. Pinli tablar bufferline'da başa alınır ve 📌 taşır.
function M.toggle_pin()
  local cur = vim.api.nvim_get_current_win()
  local win = is_code_win(cur) and cur or M.code_win()
  local groups = bufferline_groups()
  if not win or not groups then
    vim.notify("No editor tab to pin", vim.log.levels.WARN)
    return
  end
  local buf = vim.api.nvim_win_get_buf(win)
  local el = { id = buf }
  local was_pinned = groups._is_pinned(el)
  if was_pinned then
    groups.remove_element("pinned", el)
  else
    groups.add_element("pinned", el)
  end
  require("bufferline.ui").refresh()
  if win ~= cur then
    local name = vim.fn.fnamemodify(vim.api.nvim_buf_get_name(buf), ":t")
    vim.notify(("%s tab: %s"):format(was_pinned and "Unpinned" or "Pinned", name ~= "" and name or "[No Name]"))
  end
end

--- Oturumla gelen pinleri (`vim.g.BufferlinePinnedBuffers`, `sessionoptions` içindeki `globals` ile kaydedilir)
--- bufferline'a geri yükler. bufferline bunu kendisi yalnızca ilk `SessionLoadPost`'ta ve kendi yüklenmesinden sonra
--- yapar; workspace değiştirme gibi sonraki oturum yüklemeleri ve eklentiden önce yüklenen oturumlar için burada da
--- yapılır. İdempotenttir.
function M.restore_pins()
  local list = vim.g.BufferlinePinnedBuffers
  local groups = type(list) == "string" and list ~= "" and bufferline_groups()
  if not groups then
    return
  end
  local by_name = {}
  for _, buf in ipairs(vim.api.nvim_list_bufs()) do
    by_name[vim.api.nvim_buf_get_name(buf)] = buf
  end
  for _, path in ipairs(vim.split(list, ",", { plain = true })) do
    local buf = by_name[path]
    if buf and vim.api.nvim_buf_is_valid(buf) then
      pcall(groups.add_element, "pinned", { id = buf })
    end
  end
  require("bufferline.ui").refresh()
end

--- Claude Code panelinin penceresini döndürür (hangi oturum sekmesi ya da özet gösterilirse gösterilsin).
---@return integer?
local function claude_win()
  return require("termtabs").win("claude")
end

--- Claude panelini gizler; oturumlar arka planda çalışmaya devam eder.
local function hide_claude()
  require("termtabs").hide("claude")
end

--- Database paneli açıksa penceresini döndürür (modül hiç yüklenmediyse yüklemez).
---@return integer?
local function db_win()
  return package.loaded.datasource and package.loaded.datasource.find_panel()
end

--- Claude ya da terminal açılırken sağ sütundaki Database panelini kapatır; kapanınca eski paneller geri gelmez.
local function drop_db()
  local win = db_win()
  if win then
    state.db_hidden = nil
    require("datasource").close_panel(win)
  end
end

--- Cmd+3: Sağdaki Claude Code paneli (oturum sekmeleriyle).
--- claudecode tembel yüklendiği için önce modülü yükleyip (setup + IDE sunucusu) sonra komutu çağırır.
function M.claude()
  panel(claude_win, function()
    drop_db()
    pcall(require, "claudecode")
    vim.cmd("ClaudeCodeFocus")
  end, hide_claude)
end

--- Sağ taraftaki terminal panelinin penceresini döndürür.
---@return integer?
local function shell_win()
  return require("termtabs").win("shell")
end

--- Terminal panelini sağa açar (seçili sekmeyle); hiç terminal yoksa yeni bir shell sekmesi başlatır.
--- Yerleşimi `arrange_right_column` düzeltir (Claude varsa onun altına).
local function open_shell()
  drop_db()
  local tt = require("termtabs")
  if not tt.show("shell", true) then
    tt.new_shell(true)
  end
end

--- Cmd+4: Sağdaki terminal paneli (shell sekmeleriyle).
function M.shell()
  panel(shell_win, open_shell, function(win)
    vim.api.nvim_win_hide(win)
  end)
end

--- Pencereleri ekrandaki konumuna göre (önce satır, sonra sütun) sıralar.
---@param wins integer[]
---@return integer[]
local function sort_by_position(wins)
  table.sort(wins, function(a, b)
    local pa, pb = vim.api.nvim_win_get_position(a), vim.api.nvim_win_get_position(b)
    return pa[1] == pb[1] and pa[2] < pb[2] or pa[1] < pb[1]
  end)
  return wins
end

--- Alt şeritteki (Spring, Git geçmişi, loglar) pencereleri soldan sağa döndürür.
---@return integer[]
function M.bottom_wins()
  local wins = {}
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.api.nvim_win_get_config(win).relative == "" and vim.b[vim.api.nvim_win_get_buf(win)].workbench_bottom then
      table.insert(wins, win)
    end
  end
  return sort_by_position(wins)
end

--- `winlayout()` düğümü verilen pencerelerden (sırasıyla) oluşuyor mu: tek pencere için yaprak,
--- birden fazlası için `kind` ("row" | "col") türünde, yalnızca bu yaprakları içeren bir kap.
---@param node table winlayout düğümü
---@param kind "row"|"col"
---@param wins integer[]
---@return boolean
local function node_is(node, kind, wins)
  if #wins == 1 then
    return node[1] == "leaf" and node[2] == wins[1]
  end
  return node[1] == kind and #node[2] == #wins and vim.iter(ipairs(wins)):all(function(i, w)
    return node[2][i][1] == "leaf" and node[2][i][2] == w
  end)
end

--- Düzen zaten hedefteyse true. Sağ sütun varsa kök `row[..., col[Claude, terminal]]`; alt şerit de varsa kök tam
--- olarak `row[col[ana alan, şerit], sağ sütun]`, yoksa `col[ana alan, şerit]`. Sol paneller ve kod ana alandadır.
---@param right integer[] sağ sütun pencereleri (Claude, terminal)
---@param strip integer[] alt şerit pencereleri (soldan sağa)
---@return boolean
local function layout_ok(right, strip)
  local root = vim.fn.winlayout()
  local rest = root
  if #right > 0 then
    if root[1] ~= "row" or not node_is(root[2][#root[2]], "col", right) then
      return false
    end
    if #strip == 0 then
      return true
    end
    if #root[2] ~= 2 then
      return false
    end
    rest = root[2][1]
  end
  if #strip == 0 or node_is(rest, "row", strip) then
    return true
  end
  return rest[1] == "col" and node_is(rest[2][#rest[2]], "row", strip)
end

--- Pencereyi ekranın bir kenarına (`J` alt, `L` sağ) tam genişlik/boy taşır; autocmd tetiklemez.
---@param win integer
---@param dir "J"|"L"
local function move_to_edge(win, dir)
  vim.cmd(("noautocmd call win_gotoid(%d) | wincmd %s"):format(win, dir))
end

--- `win`'i `target`'ın sağına (`vertical`) ya da altına taşır; autocmd tetiklemez.
---@param win integer
---@param target integer
---@param vertical boolean
local function move_after(win, target, vertical)
  vim.cmd(
    ("noautocmd call win_splitmove(%d, %d, #{vertical: v:%s, rightbelow: v:true})"):format(win, target, vertical)
  )
end

--- Düzen doğruyken sağ sütun ve alt şerit pencerelerinin boyutlarını hatırlar. Düzen bozulduğunda (ör. sol panel
--- tam boy açılıp şeridin solunu kapladığında) anlık boyutlar zaten küçülmüş olduğu için geri yükleme bunları kullanır.
---@param right integer[]
---@param strip integer[]
local function remember_sizes(right, strip)
  state.sizes = {}
  for _, w in ipairs(vim.list_extend(vim.list_extend({}, right), strip)) do
    state.sizes[w] = { width = vim.api.nvim_win_get_width(w), height = vim.api.nvim_win_get_height(w) }
  end
end

--- Yeniden düzenlemeden önce korunacak boyutları toplar: sağ sütun genişliği ve (son hariç) yükseklikleri,
--- sabit genişlikli ana alan pencereleri (neo-tree, DBUI gibi sol paneller `winfixwidth` taşır),
--- alt şerit yüksekliği ve (son hariç) şerit pencerelerinin genişlikleri. Şerit ve sağ sütun için önce son doğru
--- düzende hatırlanan boyut (`remember_sizes`) kullanılır; sağ sütun yükseklikleri ancak tüm pencereleri
--- hatırlanıyorsa geri yüklenir (yeni açılan Claude/terminal varsayılan bölünmeyi alır).
---@param right integer[]
---@param strip integer[]
---@return fun() boyutları geri yükleyen fonksiyon
local function save_sizes(right, strip)
  local api = vim.api
  local known = state.sizes or {}
  local function size(w, key)
    return known[w] and known[w][key] or (key == "width" and api.nvim_win_get_width(w) or api.nvim_win_get_height(w))
  end
  local skip = {}
  for _, w in ipairs(vim.list_extend(vim.list_extend({}, right), strip)) do
    skip[w] = true
  end
  local fixed = {}
  for _, w in ipairs(api.nvim_tabpage_list_wins(0)) do
    if not skip[w] and api.nvim_win_get_config(w).relative == "" and vim.wo[w].winfixwidth then
      fixed[w] = api.nvim_win_get_width(w)
    end
  end
  local right_width = #right > 0 and math.max(40, unpack(vim.tbl_map(function(w)
    return size(w, "width")
  end, right)))
  local right_heights = {}
  if vim.iter(right):all(function(w)
    return known[w] ~= nil
  end) then
    for i = 1, #right - 1 do
      right_heights[i] = known[right[i]].height
    end
  end
  local strip_height = #strip > 0 and size(strip[1], "height")
  local strip_widths = vim.tbl_map(function(w)
    return size(w, "width")
  end, vim.list_slice(strip, 1, #strip - 1))
  return function()
    local unfixed = vim.tbl_filter(function(w)
      return not vim.tbl_contains(right, w) and api.nvim_win_get_config(w).relative == "" and vim.wo[w].winfixwidth
    end, api.nvim_tabpage_list_wins(0))
    -- winfixwidth (ör. noice/nui split'leri) sağ sütunun genişliğini geri almasını engellerdi
    for _, w in ipairs(unfixed) do
      vim.wo[w].winfixwidth = false
    end
    if right_width then
      api.nvim_win_set_width(right[1], right_width)
    end
    for w, width in pairs(fixed) do
      if api.nvim_win_is_valid(w) then
        api.nvim_win_set_width(w, width)
      end
    end
    if strip_height then
      api.nvim_win_set_height(strip[1], strip_height)
    end
    for i, width in ipairs(strip_widths) do
      api.nvim_win_set_width(strip[i], width)
    end
    for i, height in ipairs(right_heights) do
      api.nvim_win_set_height(right[i], height)
    end
    for _, w in ipairs(unfixed) do
      if api.nvim_win_is_valid(w) then
        vim.wo[w].winfixwidth = true
      end
    end
  end
end

local arranging = false

--- Pencere düzenini zorlar: sağ sütun (üstte Claude, altta terminal) her zaman tam boy en sağda; alt şerit
--- (Run / Git geçmişi + loglar) sağ sütunun solunda ekranın altında, sol panellerin (Files, Database ve sonradan
--- açılan her sol panel) altına uzanır. Hedef: `row[col[row[sol panel, kod...], şerit], col[Claude, terminal]]`.
--- Sol paneller kendi başına tam boy açıldığı için açıldıktan sonra (BufWinEnter/WinNew/WinResized) tekrar çağrılır.
--- Düzen zaten doğruysa (`winlayout` kontrolü) hiçbir şeye dokunmaz. Yöntem: şeridi `wincmd J` ile en alta, sonra
--- sağ sütunu `wincmd L` ile en sağa taşımak; sol panel, şerit ve sağ sütun boyutları geri yüklenir.
function M.arrange_layout()
  if arranging then
    return
  end
  local right = vim.tbl_filter(function(w)
    return w ~= nil
  end, { claude_win(), shell_win(), db_win() })
  local strip = M.bottom_wins()
  if layout_ok(right, strip) then
    remember_sizes(right, strip)
    return
  end
  arranging = true
  local cur = vim.api.nvim_get_current_win()
  local restore = save_sizes(right, strip)
  local ok, err = pcall(function()
    if #strip > 0 then
      move_to_edge(strip[1], "J")
      for i = 2, #strip do
        move_after(strip[i], strip[i - 1], true)
      end
    end
    if #right > 0 then
      move_to_edge(right[1], "L")
      for i = 2, #right do
        move_after(right[i], right[i - 1], false)
      end
    end
    restore()
    remember_sizes(right, strip)
  end)
  if vim.api.nvim_win_is_valid(cur) then
    vim.cmd(("noautocmd call win_gotoid(%d)"):format(cur))
  end
  arranging = false
  if not ok then
    vim.notify("workbench: could not arrange windows: " .. tostring(err), vim.log.levels.WARN)
  end
end

--- Eski ad; `arrange_layout` ile aynı (termtabs gibi çağıranlar için korunur).
M.arrange_right_column = M.arrange_layout

local arrange_pending = false

--- `arrange_layout`'u bir sonraki döngüye erteler; aynı döngüdeki çok sayıda olayı tek çağrıda birleştirir.
function schedule_arrange()
  if arrange_pending then
    return
  end
  arrange_pending = true
  vim.schedule(function()
    arrange_pending = false
    M.arrange_layout()
  end)
end

--- Buffer'ı alt şeritte açar. Şerit yoksa ekranın altına (sol paneller dahil, sağ sütun hariç) yeni şerit kurar
--- (`arrange_layout` ile); varsa şeritteki son pencerenin (ya da `after` penceresinin) sağına ekler.
---@param buf integer
---@param opts? { enter?: boolean, width?: integer, after?: integer }
---@return integer
function M.open_bottom(buf, opts)
  opts = opts or {}
  vim.b[buf].workbench_bottom = true
  local strip = M.bottom_wins()
  local enter = opts.enter ~= false
  local win
  if #strip > 0 then
    win = vim.api.nvim_open_win(buf, enter, { split = "right", win = opts.after or strip[#strip], width = opts.width })
  else
    win = vim.api.nvim_open_win(buf, enter, {
      split = "below",
      win = -1,
      height = math.max(10, math.floor(vim.o.lines * 0.3)),
    })
    M.arrange_layout()
  end
  M.mark_bottom(win, opts.width)
  return win
end

--- Pencereyi alt şerit penceresi olarak işaretler: buffer'a `workbench_bottom`, pencereye `w:workbench_bottom`
--- (şeride sonradan normal dosya açılırsa `guard_strip` onu koda taşır) ve `:UIReset` için varsayılan genişlik.
---@param win integer
---@param width? integer
function M.mark_bottom(win, width)
  vim.b[vim.api.nvim_win_get_buf(win)].workbench_bottom = true
  vim.w[win].workbench_bottom = true
  vim.w[win].workbench_width = width
  for opt, val in pairs({ winfixheight = true, number = false, relativenumber = false, signcolumn = "no" }) do
    vim.wo[win][opt] = val
  end
end

--- Şeritteki (`except` dışındaki) tüm pencereleri gizler; tek pencere kaldıysa dokunmaz.
---@param except? integer
local function hide_strip(except)
  for _, win in ipairs(M.bottom_wins()) do
    if win ~= except and #vim.api.nvim_tabpage_list_wins(0) > 1 then
      vim.api.nvim_win_hide(win)
    end
  end
end

--- `BufWinEnter`'da çağrılır. (1) noice'un split görünümleri (uzun mesaj, `:Noice` geçmişi; nui `botright split`
--- açar) alt şeride alınır: Run / Git History gibi şeritteki araçlar gizlenir, pencere şerit olarak işaretlenir.
--- (2) Şerit penceresine işaretsiz bir buffer (ör. neo-tree'nin "son pencere"ye açtığı dosya) gelirse buffer kod
--- penceresine taşınır ve şerit penceresi kapanır; yoksa kod alanı ikiye bölünmüş gibi ikinci bir kod penceresi
--- sol panellerin altına uzanırdı. Kod penceresi yoksa pencere sıradan ana alan penceresine döner.
---@param buf integer
function M.guard_strip(buf)
  -- dadbod sorgu sonucu (`pedit`, b:db_input'u BufReadPost'ta alır) de şeride gider
  local strip_tool = vim.bo[buf].filetype == "noice" or vim.b[buf].db_input ~= nil
  for _, win in ipairs(vim.fn.win_findbuf(buf)) do
    if vim.api.nvim_win_get_config(win).relative == "" then
      if strip_tool and vim.w[win].workbench_bottom then
        vim.b[buf].workbench_bottom = true
      elseif strip_tool then
        M.mark_bottom(win)
        vim.wo[win].winfixwidth = false
        local focused = vim.tbl_contains(M.bottom_wins(), state.focus == win and state.prev_focus or state.focus)
        hide_strip(win)
        vim.schedule(function()
          if focused and M.code_win() then
            vim.api.nvim_set_current_win(M.code_win())
          end
          M.arrange_layout()
        end)
      elseif vim.w[win].workbench_bottom and not vim.b[buf].workbench_bottom then
        vim.schedule(function()
          if not (vim.api.nvim_win_is_valid(win) and vim.api.nvim_win_get_buf(win) == buf) then
            return
          end
          local code = state.code_win ~= win and is_code_win(state.code_win or -1) and state.code_win
            or find_win(function(w)
              return w ~= win and is_code_win(w) and not vim.w[w].workbench_bottom
            end)
          if not code then
            vim.w[win].workbench_bottom = nil
            return
          end
          vim.api.nvim_win_set_buf(code, buf)
          vim.api.nvim_win_close(win, true)
          focus(code)
        end)
      end
    end
  end
end

--- Database panelini sağ sütunda açar: görünen Claude/terminal gizlenir (genişlikleri devralınır) ve panel hangi
--- yolla kapanırsa kapansın (`WinClosed`) geri gösterilir. Panel açıkken Claude/terminal açılırsa geri yükleme iptal.
local function open_db()
  local tt = require("termtabs")
  local hidden, width = {}, nil
  for _, group in ipairs({ "claude", "shell" }) do
    local w = tt.win(group)
    if w then
      width = width or vim.api.nvim_win_get_width(w)
      table.insert(hidden, group)
      tt.hide(group)
    end
  end
  state.db_hidden = hidden
  local win = require("datasource").open_panel(width or math.floor(vim.o.columns * 0.38))
  M.arrange_layout()
  vim.api.nvim_create_autocmd("WinClosed", {
    pattern = tostring(win),
    once = true,
    callback = function()
      local restore = state.db_hidden or {}
      state.db_hidden = nil
      vim.schedule(function()
        for _, group in ipairs(restore) do
          tt.show(group)
        end
      end)
    end,
  })
end

--- Cmd+5: Database paneli (lua/datasource), sağ sütunda Claude/terminal'in yerine.
function M.db()
  panel(db_win, open_db, function(win)
    require("datasource").close_panel(win)
  end)
end

--- Alt şerit tek seferde tek araç gösterir: yeni araç açılmadan önce şeritteki tüm pencereler (loglar dahil) gizlenir.
---@param open fun()
---@return fun()
local function exclusive_bottom(open)
  return function()
    hide_strip()
    open()
  end
end

--- Cmd+6: Run paneli (Spring Boot uygulamaları, Node script'leri).
function M.runner()
  local runner = require("runner")
  panel(runner.find_panel, exclusive_bottom(runner.open_panel), runner.close_panel)
end

--- Cmd+7: Git History paneli (alt şerit).
function M.git_history()
  local git = require("githistory")
  panel(git.find_panel, exclusive_bottom(git.open_panel), git.close_panel)
end

--- `Space d u`: Debug paneli (dap-ui Variables / Frames / REPL, alt şerit).
function M.debug()
  local ui = require("debugui")
  panel(ui.find_panel, exclusive_bottom(ui.open_panel), ui.close_panel)
end

--- Cmd+0: Git paneli (commit'lenmemiş değişiklikler, stage/unstage). Files ve DB gibi solda tam boy açılır;
--- diğer sol panellerle yan yana durabilir.
function M.git()
  local git = require("gitstatus")
  panel(git.find_panel, function()
    vim.cmd("Neotree close") -- Files ile aynı yuva
    git.open_panel()
    M.arrange_layout()
  end, git.close_panel)
end

local right_click = {}

--- Panellerin sağ tık menüsü için ortak global `<RightMouse>` eşlemesi. Buffer-yerel olamaz: panel odakta
--- değilken buffer-yerel eşleme devreye girmez ve PopUp açılırdı. `handler(getmousepos())` tıklamayı kendi
--- panelinde yakaladıysa true döner (eşleme `<LeftMouse>` ile imleci oraya taşır); hiçbiri yakalamazsa varsayılan
--- `<RightMouse>` (PopUp). Aynı `name` tekrar kaydedilirse eskisinin yerine geçer. Başka bir eklenti global
--- `<RightMouse>` kurmuşsa ona dokunulmaz.
---@param name string
---@param handler fun(pos: table): boolean
function M.on_right_click(name, handler)
  local first = next(right_click) == nil
  right_click[name] = handler
  if not first or vim.fn.maparg("<RightMouse>", "n") ~= "" then
    return
  end
  vim.keymap.set("n", "<RightMouse>", function()
    local pos = vim.fn.getmousepos()
    for _, h in pairs(right_click) do
      if h(pos) then
        return "<LeftMouse>"
      end
    end
    return "<RightMouse>"
  end, { expr = true, desc = "Panel action menu" })
end

--- Son kullanılan kod penceresini döndürür; geçersizse mevcut sekmedeki ilk kod penceresini.
---@return integer?
function M.code_win()
  local win = state.code_win
  if win and is_code_win(win) and vim.api.nvim_win_get_tabpage(win) == vim.api.nvim_get_current_tabpage() then
    return win
  end
  return find_win(function(w)
    return is_code_win(w)
  end)
end

--- Git geçmişinde "sadece bu dosya" modu için son kullanılan kod penceresindeki dosyanın yolunu döndürür.
---@return string?
function M.code_file()
  local win = M.code_win()
  local name = win and vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(win)) or ""
  return name ~= "" and name or nil
end

--- Saf toggle: panel açıksa (odakta olmasa bile) kapatır, kapalıysa açar.
--- `name`: "files" | "claude" | "shell" | "db" | "runner" | "git_history" | "git".
---@param name string
function M.toggle(name)
  local panels = {
    files = { find = function() return find_ft("neo-tree") end, close = function() vim.cmd("Neotree close") end },
    claude = { find = claude_win, close = hide_claude },
    shell = { find = shell_win, close = vim.api.nvim_win_hide },
    db = { find = db_win, close = function(w) require("datasource").close_panel(w) end },
    runner = { find = function() return require("runner").find_panel() end, close = function(w) require("runner").close_panel(w) end },
    git_history = { find = function() return require("githistory").find_panel() end, close = function(w) require("githistory").close_panel(w) end },
    git = { find = function() return require("gitstatus").find_panel() end, close = function(w) require("gitstatus").close_panel(w) end },
    debug = { find = function() return require("debugui").find_panel() end, close = function(w) require("debugui").close_panel(w) end },
  }
  local p = assert(panels[name], "unknown panel: " .. name)
  local win = p.find()
  if not win then
    M[name]()
    return
  end
  local was_current = win == vim.api.nvim_get_current_win()
  p.close(win)
  if was_current or not vim.api.nvim_win_is_valid(vim.api.nvim_get_current_win()) then
    M.code({ pinned = false })
  end
end


--- `:UIReset`: panel boyutlarını varsayılana döndürür. Önce `wincmd =` (kod alanı eşitlenir), sonra sağ sütun
--- `columns * 0.38` (claudecode `split_width_percentage` / termtabs ile aynı), Claude/terminal yarı yarıya, sol paneller
--- neo-tree 34 / DBUI `g:db_ui_winwidth` (40) / Git 40, şerit `max(10, lines * 0.3)` yüksekliğinde ve şerit
--- pencereleri `open_bottom`'a verilen genişlikte (son pencere kalanı alır). Hatırlanan boyutlar (`state.sizes`)
--- silinir ki `arrange_layout` eski (büyümüş) boyutları geri yüklemesin.
function M.reset_ui()
  local api = vim.api
  state.sizes = nil
  local right = vim.tbl_filter(function(w)
    return w ~= nil
  end, { claude_win(), shell_win(), db_win() })
  local strip = M.bottom_wins()
  local panels = { ["neo-tree"] = 34, gitstatus = 40 }
  vim.cmd("noautocmd wincmd =")
  if #right > 0 then
    api.nvim_win_set_width(right[1], math.floor(vim.o.columns * 0.38))
  end
  for _, w in ipairs(api.nvim_tabpage_list_wins(0)) do
    local width = api.nvim_win_get_config(w).relative == "" and panels[vim.bo[api.nvim_win_get_buf(w)].filetype]
    if width then
      api.nvim_win_set_width(w, width)
    end
  end
  if #strip > 0 then
    api.nvim_win_set_height(strip[1], math.max(10, math.floor(vim.o.lines * 0.3)))
    for i = 1, #strip - 1 do
      local width = vim.w[strip[i]].workbench_width
      if width then
        api.nvim_win_set_width(strip[i], width)
      end
    end
  end
  if #right > 1 then
    local total = api.nvim_win_get_height(right[1]) + api.nvim_win_get_height(right[2])
    api.nvim_win_set_height(right[1], math.floor(total / 2))
  end
  M.arrange_layout()
end

--- Korunan kod penceresi mi: koruma açık (`g:workbench_keep_code`, varsayılan argümansız açılış) ve `win` sekmedeki
--- son kod penceresi (panel, şerit, sağ sütun, float ve diff pencereleri sayılmaz).
---@param win integer
---@return boolean
local function keep_code(win)
  local function code(w)
    return is_code_win(w) and not vim.wo[w].diff and not vim.w[w].workbench_bottom
  end
  if not (vim.g.workbench_keep_code and code(win)) then
    return false
  end
  return not vim.iter(vim.api.nvim_tabpage_list_wins(0)):any(function(w)
    return w ~= win and code(w)
  end)
end

--- Cmd+W: tab'ı (buffer) kapatır. Boş, isimsiz bir kod penceresinde başka kod penceresi de varsa tab yerine
--- pencereyi kapatır (fazladan açılmış boş editör bölmesi Cmd+W ile kapanmıyordu).
function M.close_tab()
  local win = vim.api.nvim_get_current_win()
  local buf = vim.api.nvim_win_get_buf(win)
  local function code(w)
    return is_code_win(w) and not vim.wo[w].diff and not vim.w[w].workbench_bottom
  end
  local extra = code(win)
    and vim.api.nvim_buf_get_name(buf) == ""
    and not vim.bo[buf].modified
    and vim.iter(vim.api.nvim_tabpage_list_wins(0)):any(function(w)
      return w ~= win and code(w)
    end)
  if extra then
    return vim.api.nvim_win_close(win, false)
  end
  Snacks.bufdelete()
end

--- Ex komutunu çalıştırır; hatayı Lua yığın izi olmadan, Vim'in kendi mesajıyla gösterir.
---@param cmd string
local function exec(cmd)
  local ok, err = pcall(vim.cmd, cmd)
  if not ok then
    vim.api.nvim_echo({ { (tostring(err):gsub("^.-Vim%(%a+%):", "")), "ErrorMsg" } }, true, {})
  end
end

local closers = { quit = "quit", close = "close", wq = "wq", xit = "xit", exit = "xit" }
local quitters = { qall = true, quitall = true, wqall = true, xall = true }

--- `CmdlineLeave`: Enter'la çalıştırılacak `:q`, `:quit`, `:close`, `:wq`, `:x`, `:exit` (her kısaltma ve `!` ile)
--- korunan kod penceresini kapatacaksa komut iptal edilir (`v:event.abort`), yerine `close_window` çalışır.
--- Aralık/sayı verilmiş ya da `|` ile zincirlenmiş komutlara dokunulmaz. `:qa`/`:wqa`/`:xa` Run panelinde çalışan
--- süreç varsa iptal edilip `runner.shutdown.quit`'e verilir (önce süreçler düzgün kapatılır).
function M.guard_cmdline()
  if vim.v.event.abort or vim.v.char ~= "\r" then
    return
  end
  local line = vim.fn.getcmdline()
  local ok, cmd = pcall(vim.api.nvim_parse_cmd, line, {})
  if ok and quitters[cmd.cmd] and cmd.nextcmd == "" and package.loaded["runner"] then
    if #package.loaded["runner"].running() > 0 then
      vim.cmd("let v:event.abort = v:true")
      vim.schedule(function()
        require("runner.shutdown").quit(line)
      end)
    end
    return
  end
  local kind = ok and closers[cmd.cmd]
  if not kind or cmd.range or cmd.count or cmd.nextcmd ~= "" or not keep_code(vim.api.nvim_get_current_win()) then
    return
  end
  vim.cmd("let v:event.abort = v:true") -- vim.v.event bir kopya döndürür, atama işe yaramaz
  vim.schedule(function()
    M.close_window(kind, cmd.bang, table.concat(cmd.args, " "))
  end)
end

--- `:q`, `:close`, `:wq`, `:x` (ve Ctrl+W c/q, ZZ, ZQ, `Space w d`) yerine geçer: odaktaki pencere korunan kod
--- penceresiyse kapatmaz (`:wq`/`:x` yine yazar) ve uyarır; değilse komutu aynen çalıştırır. `:qa` vb. dokunulmaz.
---@param cmd "quit"|"close"|"wq"|"xit"
---@param bang? boolean
---@param args? string
function M.close_window(cmd, bang, args)
  local tail = (bang and "!" or "") .. ((args or "") ~= "" and " " .. args or "")
  if not keep_code(vim.api.nvim_get_current_win()) then
    return exec(cmd .. tail)
  end
  if cmd == "wq" or cmd == "xit" then
    exec((cmd == "wq" and "write" or "update") .. tail)
  end
  vim.notify("The code editor stays open. Use Cmd+W to close a tab, or :qa to quit LESS.", vim.log.levels.WARN)
end

local list_cursor = { saved = nil }

--- Liste paneli buffer'ında (Run, Files) imleç gizlenir, satır `cursorline` ile vurgulanır ve sütun hep 0'da kalır:
--- panelde gezinme yalnız satırlar (öğeler) arasında olur. Gizleme `guicursor`'a normal mod için `blend = 100`
--- vurgusu eklenerek yapılır (Nvim TUI imleci gizler); pencereden çıkınca eski `guicursor` geri gelir. Komut satırı,
--- insert vb. modlarda imleç görünür kalır.
---@param buf integer
function M.list_cursor(buf)
  if vim.b[buf].workbench_list_cursor then
    return
  end
  vim.b[buf].workbench_list_cursor = true
  vim.api.nvim_set_hl(0, "WorkbenchHiddenCursor", { blend = 100, nocombine = true, default = true })
  local function hide()
    vim.wo.cursorline = true
    if not list_cursor.saved then
      list_cursor.saved = vim.o.guicursor
      vim.o.guicursor = vim.o.guicursor .. ",n:WorkbenchHiddenCursor"
    end
  end
  local function show()
    if list_cursor.saved then
      vim.o.guicursor = list_cursor.saved
      list_cursor.saved = nil
    end
  end
  local group = vim.api.nvim_create_augroup("workbench.list_cursor." .. buf, { clear = true })
  vim.api.nvim_create_autocmd({ "BufEnter", "WinEnter" }, { group = group, buffer = buf, callback = hide })
  vim.api.nvim_create_autocmd({ "BufLeave", "WinLeave" }, { group = group, buffer = buf, callback = show })
  vim.api.nvim_create_autocmd("CursorMoved", {
    group = group,
    buffer = buf,
    callback = function()
      local pos = vim.api.nvim_win_get_cursor(0)
      if pos[2] ~= 0 then
        vim.api.nvim_win_set_cursor(0, { pos[1], 0 })
      end
    end,
  })
  if vim.api.nvim_get_current_buf() == buf then
    hide()
  end
end

return M
