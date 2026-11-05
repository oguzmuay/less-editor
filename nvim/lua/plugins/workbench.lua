return {
  {
    "coder/claudecode.nvim",
    opts = {
      terminal = {
        split_side = "right",
        split_width_percentage = 0.38,
        -- Özel sağlayıcı: oturumlar Claude panelinde sekme olarak açılır (termtabs).
        provider = require("termtabs.claude").provider,
      },
      diff_opts = {
        layout = "vertical",
        open_in_new_tab = false,
      },
    },
  },

  {
    "nvim-neo-tree/neo-tree.nvim",
    opts = {
      close_if_last_window = true,
      window = {
        width = 34,
        mappings = {
          ["<2-LeftMouse>"] = "open",
          ["<LeftRelease>"] = "noop",
          ["<Right>"] = "open",
          ["<Left>"] = "close_node",
        },
      },
      filesystem = {
        follow_current_file = { enabled = true },
        use_libuv_file_watcher = true,
        filtered_items = {
          hide_dotfiles = false,
          hide_gitignored = false,
          hide_by_name = { ".git", ".DS_Store", "node_modules" },
        },
      },
    },
  },

  {
    "kristijanhusak/vim-dadbod-ui",
    cmd = { "DBUIClose" },
  },

  {
    "akinsho/bufferline.nvim",
    opts = {
      options = {
        hover = { enabled = true, delay = 150, reveal = { "close" } },
        offsets = {
          { filetype = "neo-tree", text = "Files", highlight = "Directory", text_align = "left" },
          { filetype = "dbui", text = "Database", highlight = "Directory", text_align = "left" },
          { filetype = "gitstatus", text = "Git", highlight = "Directory", text_align = "left" },
        },
      },
    },
  },
}
