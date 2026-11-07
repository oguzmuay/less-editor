--- `:Theme` (editör teması seçici) — mantık `lua/theme/` altında, IntelliJ şemaları `colors/` + `theme/intellij.lua`.
--- Ek tema eklentileri `lazy = true`: lazy.nvim `:colorscheme` ile ilk seçildiğinde yükler.
return {
  {
    name = "theme",
    dir = vim.fn.stdpath("config") .. "/lua/theme",
    virtual = true,
    lazy = false,
    priority = 1000,
    config = function()
      require("theme").setup()
    end,
  },
  {
    "LazyVim/LazyVim",
    opts = {
      colorscheme = function()
        require("theme").load()
      end,
    },
  },
  { "Mofiqul/vscode.nvim", lazy = true },
  { "projekt0n/github-nvim-theme", name = "github-theme", lazy = true },
  { "navarasu/onedark.nvim", lazy = true },
  { "rebelot/kanagawa.nvim", lazy = true },
  { "rose-pine/neovim", name = "rose-pine", lazy = true },
  { "ellisonleao/gruvbox.nvim", lazy = true },
  { "EdenEast/nightfox.nvim", lazy = true },
}
