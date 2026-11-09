--- nvim-dap-ui kendi split düzeniyle açılmaz (LazyVim varsayılanı debug başlayınca açıyordu): sol/alt split'leri
--- workbench yerleşimine uymuyor, `arrange_layout` onları her olayda taşıyıp sağ sütunu (Claude) titretiyordu.
--- Kesme noktası dinleyicisi nvim-dap yüklenince kurulur (dap-ui o ana kadar yüklenmemiş olabilir).
--- Variables `debugui` modülüyle Run panelinde logların sağında: `Space d u` aç/odakla/kapat, kesme noktasında kendiliğinden.
return {
  {
    "rcarriga/nvim-dap-ui",
    init = function()
      vim.api.nvim_create_autocmd("User", {
        pattern = "LazyLoad",
        callback = function(ev)
          if ev.data == "nvim-dap" then
            require("debugui").setup()
            return true
          end
        end,
      })
    end,
    keys = {
      {
        "<leader>du",
        function()
          require("workbench").debug()
        end,
        desc = "Debug Variables",
      },
    },
    config = function(_, opts)
      local dap = require("dap")
      require("dapui").setup(opts)
      dap.listeners.after.event_initialized["dapui_config"] = nil
      dap.listeners.before.event_terminated["dapui_config"] = nil
      dap.listeners.before.event_exited["dapui_config"] = nil
      require("debugui").setup()
    end,
  },
}
