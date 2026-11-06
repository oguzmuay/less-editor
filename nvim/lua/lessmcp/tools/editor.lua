--- MCP araçları: kod editörü (dosya açma, aranabilir sonuç listesi).
local mcp = require("lessmcp")

--- Dosyaları kod penceresinde tab olarak açar; odak çağrıdan önceki pencerede kalır (AI terminali yazmaya devam eder).
---@param paths string[]
---@param line? integer
---@return string[] opened, string[] missing
local function open(paths, line)
  local wb = require("workbench")
  local prev, mode = vim.api.nvim_get_current_win(), vim.api.nvim_get_mode().mode
  local win = wb.code_win()
  if not win then
    wb.code({ pinned = false })
    win = vim.api.nvim_get_current_win()
  end
  local opened, missing = {}, {}
  vim.api.nvim_win_call(win, function()
    for _, p in ipairs(paths) do
      local path = mcp.abspath(p)
      if vim.uv.fs_stat(path) then
        vim.cmd.edit(vim.fn.fnameescape(path))
        opened[#opened + 1] = path
      else
        missing[#missing + 1] = p
      end
    end
    if line and #opened > 0 then
      pcall(vim.api.nvim_win_set_cursor, 0, { line, 0 })
      vim.cmd.normal({ "zz", bang = true })
    end
  end)
  if vim.api.nvim_win_is_valid(prev) then
    vim.api.nvim_set_current_win(prev)
    if mode == "t" and vim.bo[vim.api.nvim_win_get_buf(prev)].buftype == "terminal" then
      vim.cmd.startinsert()
    end
  end
  return opened, missing
end

return {
  {
    name = "editor_open_files",
    description = "Open files as tabs in the user's code editor (LESS). Paths are absolute or relative to the project "
      .. "root. For many results (more than ~8 files) prefer editor_show_list.",
    schema = {
      type = "object",
      properties = {
        paths = { type = "array", items = { type = "string" }, description = "Files to open" },
        line = { type = "integer", description = "Line to jump to in the last opened file" },
      },
      required = { "paths" },
    },
    run = function(args, done)
      local opened, missing = open(args.paths or {}, args.line)
      local msg = ("Opened %d file(s)."):format(#opened)
      if #missing > 0 then
        msg = msg .. " Not found: " .. table.concat(missing, ", ")
      end
      done(msg, #opened == 0)
    end,
  },
  {
    name = "editor_show_list",
    description = "Show a searchable list of locations (search results, entities, TODOs…) in the editor. The user "
      .. "picks entries to open them; the list also goes to the quickfix list.",
    schema = {
      type = "object",
      properties = {
        title = { type = "string" },
        items = {
          type = "array",
          items = {
            type = "object",
            properties = {
              path = { type = "string" },
              line = { type = "integer" },
              col = { type = "integer" },
              text = { type = "string", description = "Short description shown next to the file" },
            },
            required = { "path" },
          },
        },
      },
      required = { "title", "items" },
    },
    run = function(args, done)
      local items = vim.tbl_map(function(it)
        return { filename = mcp.abspath(it.path), lnum = it.line or 1, col = it.col or 1, text = it.text or "" }
      end, args.items or {})
      vim.fn.setqflist({}, " ", { title = args.title, items = items })
      Snacks.picker.qflist({ title = args.title })
      done(("Showing %d item(s) in the editor."):format(#items))
    end,
  },
}
