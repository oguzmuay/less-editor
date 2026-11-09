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
          -- Enter / tek tık: önizleme tab'ı (odak Files'ta kalır); aynı dosyada tekrar Enter / çift tık: normal tab (lua/previewtab)
          ["<cr>"] = function(state) require("previewtab").neotree(state) end,
          ["<2-LeftMouse>"] = function(state) require("previewtab").neotree(state) end,
          ["<LeftRelease>"] = function(state) require("previewtab").neotree(state, true) end,
          ["<D-n>"] = "add", -- Cmd+N: imleçteki klasörde (dosyadaysa onun klasöründe) yeni dosya
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

  -- Database paneli kendi modülümüz (lua/datasource); dadbod-ui kullanılmıyor
  { "kristijanhusak/vim-dadbod-ui", enabled = false },

  {
    "akinsho/bufferline.nvim",
    opts = {
      options = {
        hover = { enabled = true, delay = 150, reveal = { "close" } },
        offsets = {
          { filetype = "neo-tree", text = "Files", highlight = "Directory", text_align = "left" },
          { filetype = "gitstatus", text = "Git", highlight = "Directory", text_align = "left" },
        },
      },
    },
  },

  -- which-key `v` ile görsel moda girince menüyü hemen açmasın (V ve Ctrl+V'deki gibi): menü ancak görsel modda bir
  -- önek tuşuna (ör. Space) basınca çıkar
  {
    "folke/which-key.nvim",
    opts = {
      defer = function(ctx)
        return ctx.mode == "v" or ctx.mode == "V" or ctx.mode == "<C-V>"
      end,
    },
  },
}
