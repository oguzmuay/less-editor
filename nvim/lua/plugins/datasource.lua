--- Database paneli (lua/datasource) setup'ı: konsol dosyalarını bağlantılarına bağlayan autocmd'ler, ikon renkleri.
--- Sorgular vim-dadbod ile çalıştığı için o da açılışta yüklenir (adaptör fonksiyonları autoload'dan gelir).
return {
  {
    name = "datasource",
    dir = vim.fn.stdpath("config") .. "/lua/datasource",
    virtual = true,
    lazy = false,
    dependencies = { "tpope/vim-dadbod" },
    config = function()
      require("datasource").setup()
    end,
  },
}
