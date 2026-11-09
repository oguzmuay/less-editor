-- Options are automatically loaded before lazy.nvim startup
-- Default options that are always set: https://github.com/LazyVim/LazyVim/blob/main/lua/lazyvim/config/options.lua
-- Add any additional options here

vim.g.lazyvim_picker = "snacks"
vim.g.autoformat = true

vim.opt.mouse = "a"
vim.opt.mousemoveevent = true
vim.opt.mousescroll = "ver:3,hor:6"
vim.opt.clipboard = "unnamedplus"
vim.opt.splitright = true
vim.opt.splitbelow = true
vim.opt.relativenumber = true -- imleç satırı mutlak, diğerleri uzaklık (10j)
vim.opt.scrolloff = 6
vim.opt.whichwrap:append("<,>,[,]")

vim.env.PATH = "/opt/homebrew/opt/mysql-client/bin:/opt/homebrew/opt/libpq/bin:" .. vim.env.PATH

-- LESS.app: pencere başlığı = proje adı
if vim.env.LESS_APP == "1" then
  vim.opt.title = true
  vim.opt.titlestring = "%{fnamemodify(getcwd(), ':t')}"
end

-- Neovim'in ftplugin/sql.vim'i insert modda <Left>/<Right>'ı sqlcomplete'e bağlıyor (ok tuşları çalışmıyordu)
vim.g.omni_sql_no_default_maps = 1
