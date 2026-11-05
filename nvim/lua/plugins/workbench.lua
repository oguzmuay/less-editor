local build_files = { "pom.xml", "build.gradle", "build.gradle.kts" }

--- Monorepo içinde bir Java dosyasının ait olduğu projenin kökünü bulur.
--- Sıra: Gradle settings dosyası → iç içe pom.xml/build.gradle zincirinin en üstü → wrapper/.git.
--- Böylece backend'de mvnw olmasa bile kök, monorepo kökü (FE dahil) yerine backend klasörü olur.
---@param path string
---@return string?
local function java_root(path)
  local settings = vim.fs.root(path, { "settings.gradle", "settings.gradle.kts" })
  if settings then
    return settings
  end
  local function has_build_file(dir)
    return vim.iter(build_files):any(function(f)
      return vim.uv.fs_stat(dir .. "/" .. f) ~= nil
    end)
  end
  local dir = vim.fs.root(path, build_files)
  while dir do
    local parent = vim.fs.dirname(dir)
    if parent == dir or not has_build_file(parent) then
      return dir
    end
    dir = parent
  end
  return vim.fs.root(path, { "mvnw", "gradlew", ".git" })
end

return {
  {
    "mfussenegger/nvim-jdtls",
    opts = { root_dir = java_root },
  },

  {
    "coder/claudecode.nvim",
    opts = {
      terminal = {
        split_side = "right",
        split_width_percentage = 0.38,
        provider = "snacks",
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
        },
      },
    },
  },

  {
    "sindrets/diffview.nvim",
    cmd = { "DiffviewOpen", "DiffviewClose", "DiffviewFileHistory" },
    opts = function()
      local close = { "n", "q", "<cmd>DiffviewClose<cr>", { desc = "Close diffview" } }
      return {
        keymaps = { view = { close }, file_panel = { close }, file_history_panel = { close } },
      }
    end,
  },
}
