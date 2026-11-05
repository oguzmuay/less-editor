--- IntelliJ tarzı sağ kenar problem göstergesi:
--- - nvim-scrollview: kod penceresinin sağ kenarında scrollbar + diagnostik / git / arama işaretleri
---   (sol tık işarete gider, çubuk sürüklenir, sağ tık işaret bilgisini gösterir).
--- - problems modülü: kod penceresinin winbar'ının sağında `✖ 1  ⚠ 3  ↑ ↓` sayacı (temizse `✓`).
return {
  {
    "dstein64/nvim-scrollview",
    event = { "BufReadPost", "BufNewFile" },
    dependencies = { "lewis6991/gitsigns.nvim" },
    keys = {
      { "<leader>uM", "<cmd>ScrollViewToggle<cr>", desc = "Toggle Scrollbar Markers" },
    },
    opts = function()
      local icons = { error = "▬", warn = "▬", info = "▬", hint = "▬" }
      return {
        excluded_filetypes = require("problems").excluded_filetypes,
        current_only = false,
        base = "right",
        column = 1,
        visibility = "info",
        winblend_gui = 40,
        hover = true,
        mouse_primary = "left",
        mouse_secondary = "right",
        signs_on_startup = { "diagnostics", "search", "conflicts" },
        signs_scrollbar_overlap = "over",
        signs_max_per_row = 2,
        diagnostics_error_symbol = icons.error,
        diagnostics_warn_symbol = icons.warn,
        diagnostics_info_symbol = icons.info,
        diagnostics_hint_symbol = icons.hint,
        search_symbol = "━",
      }
    end,
    config = function(_, opts)
      require("scrollview").setup(opts)
      local ok, gs = pcall(require, "scrollview.contrib.gitsigns")
      if ok then
        gs.setup({ add_symbol = "▕", change_symbol = "▕", delete_symbol = "▁" })
      end
    end,
  },

  {
    name = "problems",
    dir = vim.fn.stdpath("config") .. "/lua/problems",
    virtual = true,
    lazy = false,
    config = function()
      require("problems").setup()
    end,
  },
}
