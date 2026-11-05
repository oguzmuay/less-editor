--- `:Font` komutu (Ghostty yazı tipi seçici) — mantık `lua/font/` altında. Testte `NVIM_FONT_FILE` ile yönlendir.
return {
  {
    name = "font",
    dir = vim.fn.stdpath("config") .. "/lua/font",
    virtual = true,
    lazy = false,
    config = function()
      require("font").setup()
    end,
  },
}
