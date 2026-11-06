--- LESS MCP sunucusu (stdio). AI CLI'ı bunu `nvim --headless --clean -l server.lua` ile başlatır; satır satır JSON-RPC
--- okur, araç listesini ve çağrılarını `$NVIM` soketindeki editöre (`require("lessmcp")`) iletir. Araç çağrısı editörde
--- asenkron çalışır (insan onayı bekleyebilir); sonuç gelene kadar `poll` edilir. Editör yanıtı JSON metni olarak döner.
--- Editör yoksa (Gemini'nin genel ayarından LESS dışında açıldıysa) araç listesi boş döner, sunucu hata vermez.
local ch = vim.env.NVIM and vim.env.NVIM ~= "" and select(2, pcall(vim.fn.sockconnect, "pipe", vim.env.NVIM, { rpc = true }))
local NO_EDITOR = "Not running inside the LESS editor"

local function editor(fn, ...)
  if type(ch) ~= "number" or ch <= 0 then
    error(NO_EDITOR)
  end
  return vim.rpcrequest(ch, "nvim_exec_lua", ("return require('lessmcp').%s(...)"):format(fn), { ... })
end

local function send(id, result_json, err)
  local head = '{"jsonrpc":"2.0","id":' .. vim.json.encode(id)
  if err then
    io.write(head, ',"error":', vim.json.encode({ code = err[1], message = err[2] }), "}\n")
  else
    io.write(head, ',"result":', result_json, "}\n")
  end
  io.flush()
end

local handlers = {
  initialize = function(params)
    return vim.json.encode({
      protocolVersion = params.protocolVersion or "2025-06-18",
      capabilities = { tools = vim.empty_dict() },
      serverInfo = { name = "less", version = "1.0.0" },
      instructions = "Tools of the LESS editor the user is working in: open files and result lists in the editor, "
        .. "the Git, Run and Database panels. Writes to a database and discarding git changes ask the user first.",
    })
  end,
  ping = function()
    return "{}"
  end,
  ["tools/list"] = function()
    local ok, res = pcall(editor, "list")
    return ok and res or '{"tools":[]}'
  end,
  ["tools/call"] = function(params)
    local id = editor("call", params.name, vim.json.encode(params.arguments or vim.empty_dict()), vim.env.LESS_AI_NAME)
    while true do
      local res = editor("poll", id)
      if type(res) == "string" then
        return res
      end
      vim.wait(100)
    end
  end,
}

for line in io.lines() do
  local ok, msg = pcall(vim.json.decode, line)
  if ok and type(msg) == "table" and msg.method and msg.id ~= nil then
    local fn = handlers[msg.method]
    if not fn then
      send(msg.id, nil, { -32601, "Method not found: " .. msg.method })
    else
      local good, res = pcall(fn, type(msg.params) == "table" and msg.params or {})
      if good then
        send(msg.id, res)
      else
        send(msg.id, nil, { -32603, tostring(res) })
      end
    end
  end
end
