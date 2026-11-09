--- Snacks input (vim.ui.input) penceresi odağı kaybedince iptal edilir.
--- Prompt açıkken Cmd+1 gibi bir panel kısayolu odağı başka pencereye taşıyınca yüzen pencere ekranda asılı
--- kalıyordu; sonraki prompt onun altında açıldığı için eskisi kapatılamıyordu (ör. Run paneli `p` profil).
local function cancel_on_leave(win)
  win:on("WinLeave", function(self)
    vim.schedule(function()
      if self:valid() and vim.api.nvim_get_current_win() ~= self.win then
        self:execute("cancel")
      end
    end)
  end, { buf = true })
end

return {
  {
    "folke/snacks.nvim",
    opts = function(_, opts)
      opts.styles = opts.styles or {}
      opts.styles.input = vim.tbl_extend("force", opts.styles.input or {}, { on_win = cancel_on_leave })
    end,
  },
}
