-- Tooltip'lerdeki (LSP dokümantasyonu, nofile buffer) markdown başlıkları: `#` yerine seviye simgesi. LazyVim
-- normal .md dosyalarında simgeleri kapatıyor (`icons = {}`); o davranış korunur.
return {
  "MeanderingProgrammer/render-markdown.nvim",
  opts = {
    overrides = {
      buftype = {
        nofile = {
          heading = { icons = { "󰲡 ", "󰲣 ", "󰲥 ", "󰲧 ", "󰲩 ", "󰲫 " }, width = "block", right_pad = 1 },
        },
      },
    },
  },
}
