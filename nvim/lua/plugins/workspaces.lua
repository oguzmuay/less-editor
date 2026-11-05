--- Workspaces (çoklu kök): diskte farklı yerlerdeki klasörleri tek proje gibi açar. Mantık `lua/workspaces/` altında;
--- bu dosya yalnızca eklenti bağlantılarını kurar (neo-tree yamaları, snacks picker).
return {
  {
    name = "workspaces",
    dir = vim.fn.stdpath("config") .. "/lua/workspaces",
    virtual = true,
    lazy = false,
    config = function()
      require("workspaces").setup()
    end,
  },

  {
    "nvim-neo-tree/neo-tree.nvim",
    opts = function(_, opts)
      require("workspaces.neotree").install()
      opts.event_handlers = opts.event_handlers or {}
      table.insert(opts.event_handlers, {
        event = "git_status_changed",
        handler = function(args)
          require("workspaces.neotree").on_git_status_changed(args)
        end,
      })
    end,
  },

  {
    "folke/snacks.nvim",
    keys = {
      { "<leader>fw", function() require("workspaces.ui").pick() end, desc = "Workspaces" },
      { "<leader>fW", function() require("workspaces.ui").manage() end, desc = "Manage workspaces" },
    },
    opts = function(_, opts)
      opts.picker = opts.picker or {}
      opts.picker.sources = opts.picker.sources or {}
      for _, source in ipairs({ "files", "grep", "grep_word" }) do
        local src = opts.picker.sources[source] or {}
        local prev = src.config
        src.config = function(o)
          o = prev and prev(o) or o
          return require("workspaces").picker_config(o) or o
        end
        opts.picker.sources[source] = src
      end
    end,
  },
}
