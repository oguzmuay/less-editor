--- Notlar (Cmd+N, `:Notes`): ekranın ortasında tab'lı yüzen pencere — mantık `lua/notes/` altında.
return {
  {
    name = "notes",
    dir = vim.fn.stdpath("config") .. "/lua/notes",
    virtual = true,
    lazy = false,
    config = function()
      require("notes").setup()
    end,
  },
}
