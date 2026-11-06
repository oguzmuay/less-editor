--- MCP araçları: Run paneli (Cmd+6). Öğeler id ya da etiketle seçilir; ayarlar paneldeki `e` formuyla aynı kayıttır.
local function runner()
  return require("runner")
end

--- id ya da (tekil) etiketle öğe.
---@param key string?
---@return RunnerItem?, string? err
local function find(key)
  if not key then
    return nil, "Missing id"
  end
  local items = runner().items()
  local by_id = vim.iter(items):find(function(i) return i.id == key end)
  if by_id then
    return by_id
  end
  local by_label = vim.tbl_filter(function(i) return i.label == key end, items)
  if #by_label == 1 then
    return by_label[1]
  end
  return nil, #by_label > 1 and ("Several items are named %s; use the id (run_list)"):format(key)
    or ("No run item: %s (see run_list)"):format(key)
end

local id_schema = {
  type = "object",
  properties = { id = { type = "string", description = "Item id or label from run_list" } },
  required = { "id" },
}

---@param fn fun(item: RunnerItem, args: table, done: fun(text: string, is_error?: boolean))
local function with_item(fn)
  return function(args, done)
    local item, err = find(args.id)
    if not item then
      return done(err, true)
    end
    fn(item, args, done)
  end
end

return {
  {
    name = "run_list",
    description = "Runnable apps and scripts of the Run panel (Spring Boot apps, package.json scripts) with status, "
      .. "port and run configuration.",
    schema = { type = "object", properties = vim.empty_dict() },
    run = function(_, done)
      local out = {}
      for _, item in ipairs(runner().items()) do
        out[#out + 1] = vim.json.encode({
          id = item.id,
          label = item.label,
          status = item.status,
          mode = item.job and item.mode or nil,
          port = item.port,
          cwd = item.cwd,
          supports_profile = item.profiles == true,
          config = runner().config(item),
        })
      end
      done(#out > 0 and table.concat(out, "\n") or "Nothing runnable found.")
    end,
  },
  {
    name = "run_start",
    description = "Start a run item (mode run or debug); restarts it if it is already running.",
    schema = {
      type = "object",
      properties = {
        id = { type = "string" },
        mode = { type = "string", enum = { "run", "debug" } },
      },
      required = { "id" },
    },
    run = with_item(function(item, args, done)
      runner().rerun(item, args.mode or "run")
      done(("Starting %s; read the output with run_logs."):format(item.label))
    end),
  },
  {
    name = "run_stop",
    description = "Stop a running item.",
    schema = id_schema,
    run = with_item(function(item, _, done)
      runner().stop(item)
      done("Stopping " .. item.label)
    end),
  },
  {
    name = "run_logs",
    description = "Last lines of a run item's log (console output).",
    schema = {
      type = "object",
      properties = { id = { type = "string" }, lines = { type = "integer", description = "Default 100, max 1000" } },
      required = { "id" },
    },
    run = with_item(function(item, args, done)
      if not (item.buf and vim.api.nvim_buf_is_valid(item.buf)) then
        return done(item.label .. " has not been run yet.", true)
      end
      local lines = vim.api.nvim_buf_get_lines(item.buf, 0, -1, false)
      while #lines > 0 and vim.trim(lines[#lines]) == "" do
        lines[#lines] = nil
      end
      local n = math.min(args.lines or 100, 1000)
      done(("[%s · %s]\n"):format(item.label, item.status)
        .. table.concat(vim.list_slice(lines, math.max(1, #lines - n + 1)), "\n"))
    end),
  },
  {
    name = "run_set_config",
    description = "Change a run item's run configuration (like IntelliJ Run Configurations): Spring profile, "
      .. "environment variables, VM options (Node: NODE_OPTIONS), program arguments. Only the given fields change; "
      .. "an empty string (or empty env object) clears a field. Takes effect on the next start.",
    schema = {
      type = "object",
      properties = {
        id = { type = "string" },
        profile = { type = "string" },
        env = { type = "object", additionalProperties = { type = "string" } },
        vm_options = { type = "string" },
        args = { type = "string" },
      },
      required = { "id" },
    },
    run = with_item(function(item, args, done)
      local cfg = runner().config(item)
      for _, k in ipairs({ "profile", "env", "vm_options", "args" }) do
        if args[k] ~= nil then
          cfg[k] = args[k]
        end
      end
      runner().set_config(item, cfg)
      done(("Saved for %s: %s"):format(item.label, vim.json.encode(runner().config(item))))
    end),
  },
}
