--- Açılışta son projeyi/workspace'i otomatik açma ve proje açılınca varsayılan panel düzeni. Mantık
--- `lua/autorestore/` altında; bu dosya modülü kurar, dashboard kısayolunu ekler ve (testler için) persistence'ın
--- oturum klasörünü `NVIM_SESSIONS_DIR` ile yönlendirmeye izin verir.
return {
  {
    name = "autorestore",
    dir = vim.fn.stdpath("config") .. "/lua/autorestore",
    virtual = true,
    lazy = false,
    config = function()
      require("autorestore").setup()
    end,
    keys = {
      { "<leader>qh", function() require("autorestore").dashboard() end, desc = "Dashboard" },
    },
  },

  {
    "folke/persistence.nvim",
    opts = function(_, opts)
      if vim.env.NVIM_SESSIONS_DIR then
        opts.dir = vim.env.NVIM_SESSIONS_DIR:gsub("/?$", "/")
      end
    end,
  },
}
