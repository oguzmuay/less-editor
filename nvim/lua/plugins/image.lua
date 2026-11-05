--- Görüntü görüntüleyici (snacks.image, kitty grafik protokolü; Ghostty/LESS.app destekler).
--- - png/jpg/gif/webp/bmp/tiff/heic/avif/icns/pdf (ilk sayfa) dosyaları buffer'da görüntü olarak açılır.
---   svg bilerek listede yok: kod deposunda düzenlenen bir metin dosyası (markdown içindeki svg yine çizilir).
--- - Markdown'da görüntüler satır içinde, diğer dillerde (html/css/jsx/vue…) imleç `src`/`url()` üstündeyken
---   yüzen pencerede gösterilir (kodun arasında büyük görseller olmasın).
--- - Terminal tespiti XTVERSION sorgusuyla; yanıt gelmezse `TERM=xterm-ghostty` Ghostty sayılır
---   (nvim :terminal'i TERM'i değiştirdiği için iç içe nvim yanlış tespit etmez).
--- Görseller unicode placeholder ile çizilir: pencereye kırpılır, buffer görünmeyince kaybolur.
local function float_only_outside_markdown()
  local langs ---@type string[]?
  vim.api.nvim_create_autocmd("FileType", {
    group = vim.api.nvim_create_augroup("workbench.image", { clear = true }),
    callback = function(ev)
      langs = langs or Snacks.image.langs()
      local lang = vim.treesitter.language.get_lang(vim.bo[ev.buf].filetype)
      if lang == "markdown" or not vim.tbl_contains(langs, lang) or vim.b[ev.buf].snacks_image_attached then
        return
      end
      vim.b[ev.buf].snacks_image_attached = true
      vim.api.nvim_create_autocmd("CursorMoved", {
        group = vim.api.nvim_create_augroup("workbench.image." .. ev.buf, { clear = true }),
        buffer = ev.buf,
        callback = vim.schedule_wrap(function()
          Snacks.image.hover()
        end),
      })
    end,
  })
end

return {
  {
    "folke/snacks.nvim",
    opts = function(_, opts)
      opts.image = { enabled = true }
      for _, e in ipairs(require("snacks.image.terminal").envs()) do
        if e.name == "ghostty" then
          e.env = { TERM = "ghostty" }
        end
      end
      float_only_outside_markdown()
      -- snacks yükleme sırasında boş buffer'a set_lines yapıyor; previewtab'ın BufModifiedSet'inden önce çalışır
      vim.api.nvim_create_autocmd("BufModifiedSet", {
        group = vim.api.nvim_create_augroup("workbench.image.modified", { clear = true }),
        callback = function(ev)
          if vim.bo[ev.buf].filetype == "image" and vim.bo[ev.buf].modified then
            vim.bo[ev.buf].modified = false
          end
        end,
      })
    end,
  },
}
