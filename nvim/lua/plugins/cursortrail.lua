--- Cursor trail (smear-cursor.nvim): imleç bir yerden başka yere giderken arkasında kısa bir iz bırakır.
--- `:CursorTrail` ya da `Space u t` açar/kapatır; seçim kalıcıdır (`stdpath("state")/cursor_trail`, "on"/"off").
--- Varsayılan açık. Testte `NVIM_CURSOR_TRAIL_FILE` ile dosya yönlendirilebilir.
local function state_file()
  return vim.env.NVIM_CURSOR_TRAIL_FILE or (vim.fn.stdpath("state") .. "/cursor_trail")
end

--- Kayıtlı seçim; dosya yoksa açık.
---@return boolean
local function saved()
  local fd = io.open(state_file(), "r")
  if not fd then
    return true
  end
  local value = fd:read("*l")
  fd:close()
  return value ~= "off"
end

---@param on boolean
local function save(on)
  pcall(vim.fn.writefile, { on and "on" or "off" }, state_file())
end

return {
  {
    "sphamba/smear-cursor.nvim",
    event = "VeryLazy",
    cond = vim.g.neovide == nil,
    opts = function()
      return {
        enabled = saved(),
        hide_target_hack = true,
        cursor_color = "none",
      }
    end,
    config = function(_, opts)
      local smear = require("smear_cursor")
      smear.setup(opts)
      local toggle = Snacks.toggle({
        name = "Cursor Trail",
        get = function()
          return smear.enabled
        end,
        set = function(on)
          smear.enabled = on
          save(on)
        end,
      })
      toggle:map("<leader>ut")
      vim.api.nvim_create_user_command("CursorTrail", function(args)
        local want = ({ on = true, off = false })[args.args]
        if want == nil then
          want = not smear.enabled
        end
        toggle:set(want)
        vim.notify("Cursor trail " .. (want and "on" or "off"))
      end, {
        nargs = "?",
        complete = function()
          return { "on", "off" }
        end,
        desc = "Toggle the cursor trail (on | off)",
      })
    end,
  },
}
