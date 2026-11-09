-- Autocmds are automatically loaded on the VeryLazy event
-- Default autocmds that are always set: https://github.com/LazyVim/LazyVim/blob/main/lua/lazyvim/config/autocmds.lua
--
-- Add any additional autocmds here
-- with `vim.api.nvim_create_autocmd`
--
-- Or remove existing autocmds by their group name (which is prefixed with `lazyvim_` for the defaults)
-- e.g. vim.api.nvim_del_augroup_by_name("lazyvim_wrap_spell")

-- Markdown'da yazım denetimi kapalı (LazyVim `lazyvim_wrap_spell` açıyordu; İngilizce sözlük Türkçe metni hata gösteriyordu)
vim.api.nvim_create_autocmd("FileType", {
  group = vim.api.nvim_create_augroup("workbench_markdown_nospell", { clear = true }),
  pattern = { "markdown", "markdown.mdx" },
  callback = function()
    vim.opt_local.spell = false
  end,
})
