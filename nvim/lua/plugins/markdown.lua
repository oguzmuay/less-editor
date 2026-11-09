-- Markdown önizlemesi (render-markdown, buffer içinde): md dosyasında Cmd+P aç/kapa (config/keymaps.lua).
-- LazyVim başlık simgelerini ve checkbox'ları kapatıyordu; önizleme açıkken düz metinden belirgin farklı görünsün diye
-- açıldı (tooltip'ler de aynı simgeleri kullanır). Yazım denetimi ve markdownlint stil uyarıları md'de kapalı
-- (config/autocmds.lua, aşağıdaki nvim-lint): Türkçe kelimeler ve satır uzunluğu "hata" gibi işaretleniyordu;
-- marksman'ın gerçek hataları (kırık link vb.) kalır.
return {
  {
    "MeanderingProgrammer/render-markdown.nvim",
    opts = {
      heading = { icons = { "󰲡 ", "󰲣 ", "󰲥 ", "󰲧 ", "󰲩 ", "󰲫 " } },
      checkbox = { enabled = true },
      overrides = {
        buftype = {
          nofile = {
            heading = { width = "block", right_pad = 1 },
          },
        },
      },
    },
  },
  {
    "mfussenegger/nvim-lint",
    opts = function(_, opts)
      opts.linters_by_ft = opts.linters_by_ft or {}
      opts.linters_by_ft.markdown = {}
      opts.linters_by_ft["markdown.mdx"] = {}
    end,
  },
}
