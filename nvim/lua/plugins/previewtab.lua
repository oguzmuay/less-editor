--- Önizleme tab'ı (VS Code tarzı; mantık `lua/previewtab/`). neo-tree eşlemeleri `plugins/workbench.lua`'da.
return {
  {
    name = "previewtab",
    dir = vim.fn.stdpath("config") .. "/lua/previewtab",
    virtual = true,
    lazy = false,
    config = function()
      require("previewtab").setup()
    end,
  },

  {
    "akinsho/bufferline.nvim",
    opts = function()
      require("previewtab").patch_bufferline()
    end,
  },
}
