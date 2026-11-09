-- Keymaps are automatically loaded on the VeryLazy event
-- Default keymaps that are always set: https://github.com/LazyVim/LazyVim/blob/main/lua/lazyvim/config/keymaps.lua
-- Add any additional keymaps here

local wb = require("workbench")
wb.setup()
require("workbench.commands").setup()
require("gitblame").setup()

local map = vim.keymap.set
local nt = { "n", "t", "i", "v" }

-- Paneller (Ghostty, Cmd+0..7'yi <D-0>..<D-7> olarak iletir)
map(nt, "<D-1>", wb.files, { desc = "Panel: Files" })
map(nt, "<D-2>", wb.code, { desc = "Panel: Code" })
map(nt, "<D-3>", wb.claude, { desc = "Panel: AI" })
map(nt, "<D-4>", wb.shell, { desc = "Panel: Terminal" })
map(nt, "<D-5>", wb.db, { desc = "Panel: Database" })
map(nt, "<D-6>", wb.runner, { desc = "Panel: Run" })
map(nt, "<D-7>", wb.git_history, { desc = "Panel: Git History" })
map(nt, "<D-0>", wb.git, { desc = "Panel: Git" })
map(nt, "<S-D-0>", wb.reset_ui, { desc = "Reset panel sizes" })

-- Leader karşılıkları (Cmd kullanılamayan yerler için)
map("n", "<leader>1", wb.files, { desc = "Panel: Files" })
map("n", "<leader>2", wb.code, { desc = "Panel: Code" })
map("n", "<leader>3", wb.claude, { desc = "Panel: AI" })
map("n", "<leader>4", wb.shell, { desc = "Panel: Terminal" })
map("n", "<leader>5", wb.db, { desc = "Panel: Database" })
map("n", "<leader>6", wb.runner, { desc = "Panel: Run" })
map("n", "<leader>7", wb.git_history, { desc = "Panel: Git History" })
map("n", "<leader>0", wb.git, { desc = "Panel: Git" })
map("n", "<leader>ga", "<cmd>GitBlame<cr>", { desc = "Toggle Git Blame (annotate)" })
map("n", "<leader>j", function() require("runner").pick() end, { desc = "Run: pick item → action" })

-- Editör
map({ "n", "i", "v" }, "<D-s>", "<cmd>w<cr><esc>", { desc = "Save" })
map({ "n", "t", "v" }, "<D-f>", function() Snacks.picker.files() end, { desc = "Find file" })
-- Yazarken Cmd+F dosya içinde arar: normal moda geçip `/` (sonraki/önceki eşleşme n / N)
map("i", "<D-f>", "<Esc>/", { desc = "Search in file" })
map(nt, "<D-e>", function() Snacks.picker.recent() end, { desc = "Recent files" })
map(nt, "<D-k>", function() require("shortcuts").open() end, { desc = "Search shortcuts" })
map(nt, "<D-F>", function() Snacks.picker.grep() end, { desc = "Grep in project" })
map(nt, "<D-E>", wb.files, { desc = "Panel: Files" })
map("n", "<D-w>", wb.close_tab, { desc = "Close buffer" })
-- Cmd+Q (yalnız LESS'te Neovim'e gelir): bu pencereyi kapatır, diğer LESS pencereleri açık kalır
-- Run panelinde çalışan süreç varsa önce düzgün kapatılır (lua/runner/shutdown.lua)
map({ "n", "i", "v", "t" }, "<D-q>", function() require("runner.shutdown").quit("confirm qall") end, { desc = "Close this window (quit Neovim)" })
map("n", "<leader>qq", function() require("runner.shutdown").quit("qall") end, { desc = "Quit All" })
map("n", "<D-/>", "gcc", { remap = true, desc = "Toggle comment" })
map("v", "<D-/>", "gc", { remap = true, desc = "Toggle comment" })
map("t", "<C-q>", "<C-\\><C-n>", { desc = "Terminal: normal mode" })
-- Son kod penceresi kapanmasın (:q/:close/:wq/:x workbench.guard_cmdline'da)
local closers = { ["<C-w>c"] = "close", ["<C-w>q"] = "quit", ["<C-w><C-q>"] = "quit", ["<leader>wd"] = "close", ZQ = "quit" }
for lhs, cmd in pairs(closers) do
  map("n", lhs, function() wb.close_window(cmd, lhs == "ZQ") end, { desc = "Close window (keeps the code editor)" })
end
map("n", "ZZ", function() wb.close_window("xit") end, { desc = "Save and close window (keeps the code editor)" })

-- Mouse: Ctrl/Option + tık ile tanıma git, sağ tık menüsüne LSP ve Claude eylemleri
map("n", "<C-LeftMouse>", "<LeftMouse><cmd>lua vim.lsp.buf.definition()<cr>", { desc = "Go to definition" })
map("n", "<M-LeftMouse>", "<LeftMouse><cmd>lua vim.lsp.buf.definition()<cr>", { desc = "Go to definition" })
vim.cmd([[
  anoremenu PopUp.-lsp- <Nop>
  anoremenu PopUp.Find\ References <cmd>lua Snacks.picker.lsp_references()<cr>
  anoremenu PopUp.Rename\ Symbol <cmd>lua vim.lsp.buf.rename()<cr>
  anoremenu PopUp.Code\ Action <cmd>lua vim.lsp.buf.code_action()<cr>
  anoremenu PopUp.Annotate\ with\ Git\ Blame <cmd>GitBlame<cr>
  anoremenu PopUp.-claude- <Nop>
  nnoremenu PopUp.Add\ File\ to\ Claude <cmd>ClaudeCodeAdd %<cr>
  vnoremenu PopUp.Send\ to\ Claude :ClaudeCodeSend<cr>
]])

--- İmleç zaten satırın ilk dolu karakterindeyse 0. sütuna, değilse ilk dolu karaktere gider (VS Code'daki Home gibi).
---@return boolean
local function at_first_nonblank()
  return vim.fn.col(".") == #vim.api.nvim_get_current_line():match("^%s*") + 1
end

-- Satır / dosya başı-sonu: Cmd+Sol/Sağ/Yukarı/Aşağı
map({ "n", "v" }, "<D-Left>", function() return at_first_nonblank() and "0" or "^" end, { expr = true, desc = "Line start" })
map("i", "<D-Left>", function() return at_first_nonblank() and "<Home>" or "<C-o>^" end, { expr = true, desc = "Line start" })
map("n", "<D-Right>", "$", { desc = "Line end" })
map("v", "<D-Right>", "g_", { desc = "Line end" })
map("i", "<D-Right>", "<End>", { desc = "Line end" })
map({ "n", "v" }, "<D-Up>", "gg", { desc = "File start" })
map({ "n", "v" }, "<D-Down>", "G", { desc = "File end" })
map("i", "<D-Up>", "<C-Home>", { desc = "File start" })
map("i", "<D-Down>", "<C-End>", { desc = "File end" })
map("t", "<D-Left>", "<C-a>", { desc = "Line start" })
map("t", "<D-Right>", "<C-e>", { desc = "Line end" })

-- Tablar: Cmd+Shift+Sol/Sağ veya Cmd+Option+Sol/Sağ. Claude/Terminal panelindeyken o panelin sekmeleri,
-- başka yerde editör tabları (diğer panellerden basılırsa önce koda geçer)
local termtabs = require("termtabs")
for _, lhs in ipairs({ "<S-D-Left>", "<M-D-Left>" }) do
  map(nt, lhs, function()
    if not termtabs.cycle(-1) then wb.code({ pinned = false }); vim.cmd("BufferLineCyclePrev") end
  end, { desc = "Previous tab" })
end
for _, lhs in ipairs({ "<S-D-Right>", "<M-D-Right>" }) do
  map(nt, lhs, function()
    if not termtabs.cycle(1) then wb.code({ pinned = false }); vim.cmd("BufferLineCycleNext") end
  end, { desc = "Next tab" })
end
-- Cmd+P: editör tab'ını pinle / pin'i kaldır (panelden basılırsa son kod penceresindeki tab)
map(nt, "<D-p>", wb.toggle_pin, { desc = "Pin / unpin editor tab" })
-- Cmd+N: Files panelinde yeni dosya (neo-tree eşlemesi, plugins/workbench.lua); başka yerde yeni LESS penceresi
-- (Ghostty'nin Cmd+N'i Neovim'e verildiği için new_window AppleScript ile istenir)
map(nt, "<D-n>", function()
  local app = vim.env.LESS_APP == "1" and "LESS" or "Ghostty"
  local script = ('tell application "%s" to perform action "new_window" on focused terminal of selected tab of front window')
    :format(app)
  vim.system({ "osascript", "-e", script }, { text = true }, function(res)
    if res.code ~= 0 then
      vim.schedule(function()
        vim.notify("Could not open a new window: " .. vim.trim(res.stderr or ""), vim.log.levels.WARN)
      end)
    end
  end)
end, { desc = "New window" })
-- Cmd+T: Claude panelinde yeni Claude oturumu, Terminal panelinde yeni shell sekmesi, başka yerde yeni boş buffer
map(nt, "<D-t>", termtabs.new_tab, { desc = "New tab (Claude / terminal / buffer)" })

-- Kelime atlama: Option+Sol/Sağ (Ghostty ESC b / ESC f gönderir; terminalde shell'e aynen gider)
map({ "n", "v" }, "<M-b>", "b", { desc = "Word back" })
map({ "n", "v" }, "<M-f>", "w", { desc = "Word forward" })
map("i", "<M-b>", "<C-Left>", { desc = "Word back" })
map("i", "<M-f>", "<C-Right>", { desc = "Word forward" })

-- LESS markası: LazyVim'in "Lazy" / "LazyVim Changelog" girdileri which-key'de görünmesin
map("n", "<leader>l", "<cmd>Lazy<cr>", { desc = "Plugins" })
pcall(vim.keymap.del, "n", "<leader>L")
