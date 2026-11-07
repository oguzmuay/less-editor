local M = {}

local SEP = "\31"

---@class GitBranch
---@field name string kısa ad (`main`, `origin/main`)
---@field ref string tam ref (`refs/heads/main`)
---@field remote? string remote dal ise remote adı (`origin`)
---@field current? boolean HEAD bu dalda mı
---@field track? string upstream'e göre `↑1 ↓2` / `gone`

--- `ahead 1, behind 2` → `↑1 ↓2`.
---@param track string
---@return string?
local function short_track(track)
  if track == "" then
    return nil
  end
  if track == "gone" then
    return "gone"
  end
  local ahead, behind = track:match("ahead (%d+)"), track:match("behind (%d+)")
  return vim.trim((ahead and ("↑" .. ahead .. " ") or "") .. (behind and ("↓" .. behind) or ""))
end

--- Repodaki local ve remote dalları listeler (ada göre sıralı). `refs/remotes/<remote>/HEAD` sembolik ref'i atlanır.
--- HEAD bir dalda değilse `detached` true döner.
---@param root string
---@return { locals: GitBranch[], remotes: GitBranch[], detached: boolean }
function M.list(root)
  local fmt = table.concat({ "%(refname)", "%(refname:short)", "%(HEAD)", "%(upstream:track,nobracket)" }, "%1f")
  local res = vim
    .system({ "git", "for-each-ref", "--format=" .. fmt, "refs/heads", "refs/remotes" }, { cwd = root, text = true })
    :wait()
  local remotes = vim.split(
    vim.system({ "git", "remote" }, { cwd = root, text = true }):wait().stdout or "",
    "\n",
    { trimempty = true }
  )
  table.sort(remotes, function(a, b)
    return #a > #b
  end)
  local out = { locals = {}, remotes = {}, detached = true }
  for line in vim.gsplit(res.stdout or "", "\n", { trimempty = true }) do
    local f = vim.split(line, SEP, { plain = true })
    local ref, name = f[1], f[2]
    if ref and vim.startswith(ref, "refs/heads/") then
      local current = f[3] == "*"
      out.detached = out.detached and not current
      table.insert(out.locals, { name = name, ref = ref, current = current, track = short_track(f[4] or "") })
    elseif ref and not ref:match("/HEAD$") then
      local rest = ref:sub(#"refs/remotes/" + 1)
      local remote = vim.iter(remotes):find(function(r)
        return vim.startswith(rest, r .. "/")
      end)
      table.insert(out.remotes, { name = rest, ref = ref, remote = remote })
    end
  end
  return out
end

--- Dala geçer. Remote dalda aynı adlı local dal varsa ona, yoksa upstream'i ayarlı yeni local dal açılır.
--- Sonuç `cb(ok, message)` ile ana döngüde bildirilir.
---@param root string
---@param branch GitBranch
---@param locals GitBranch[]
---@param cb fun(ok: boolean, msg: string)
function M.checkout(root, branch, locals, cb)
  local cmd = { "git", "switch", branch.name }
  if branch.remote then
    local name = branch.name:sub(#branch.remote + 2)
    local exists = vim.iter(locals):any(function(b)
      return b.name == name
    end)
    cmd = exists and { "git", "switch", name } or { "git", "switch", "-c", name, "--track", branch.name }
  end
  vim.system(cmd, { cwd = root, text = true }, function(res)
    vim.schedule(function()
      local msg = (res.stderr or "") ~= "" and res.stderr or res.stdout or ""
      cb(res.code == 0, vim.trim(msg))
    end)
  end)
end

return M
