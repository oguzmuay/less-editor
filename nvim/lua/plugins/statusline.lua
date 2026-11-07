--- Durum satırının en sağı (lualine_z, saatin solu): CPU / RAM / disk özeti ve "Leaked IP" (yerel IP şakası).
--- Dar pencerede (120 sütun altı) kaynaklar, 90 altı IP gizlenir.
return {
  {
    "nvim-lualine/lualine.nvim",
    opts = function(_, opts)
      local sysinfo = require("sysinfo")
      opts.sections = opts.sections or {}
      opts.sections.lualine_z = opts.sections.lualine_z or {}
      table.insert(opts.sections.lualine_z, 1, {
        sysinfo.resources,
        cond = function()
          return vim.o.columns >= 120
        end,
      })
      table.insert(opts.sections.lualine_z, 2, {
        sysinfo.leaked_ip,
        cond = function()
          return vim.o.columns >= 90
        end,
      })
    end,
  },
}
