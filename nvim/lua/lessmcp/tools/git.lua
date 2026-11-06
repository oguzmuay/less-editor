--- MCP araçları: Git paneli (Cmd+0) ve diff'ler. Stage / unstage geri alınabildiği için onaysız; discard onay ister.
--- Commit, push, reset gibi işlemler bilerek yok (panelde de yok); salt okunur sorgular için AI kabukta git kullanabilir.
local mcp = require("lessmcp")

local SECTIONS = { "conflicts", "staged", "changes", "untracked" }

---@return string[]
local function repos()
  return require("githistory").repos()
end

--- Yolu (mutlak ya da proje köküne göre) içeren repo ve repo içindeki göreli yol.
---@param path string
---@return string? repo, string? rel
local function locate(path)
  local abs = mcp.abspath(path)
  for _, repo in ipairs(repos()) do
    local rel = vim.fs.relpath(repo, abs)
    if rel then
      return repo, rel
    end
  end
end

--- Yolları repolara göre gruplar; repo dışındakiler `missing`'e.
---@param paths string[]
---@return table<string, string[]> by_repo, string[] missing
local function group(paths)
  local by, missing = {}, {}
  for _, p in ipairs(paths) do
    local repo, rel = locate(p)
    if repo then
      by[repo] = by[repo] or {}
      table.insert(by[repo], rel)
    else
      missing[#missing + 1] = p
    end
  end
  return by, missing
end

---@param args string[]
---@param paths string[]
---@param done fun(text: string, is_error?: boolean)
local function each_repo(args, paths, done)
  local gs = require("gitstatus")
  local by, missing = group(paths)
  local n = 0
  for repo, rels in pairs(by) do
    if gs.git(repo, vim.list_extend(vim.deepcopy(args), rels)) then
      n = n + #rels
    end
  end
  gs.refresh()
  done(("Done for %d path(s).%s"):format(n, #missing > 0 and (" Not in a repository: " .. table.concat(missing, ", ")) or ""),
    n == 0)
end

local paths_schema = {
  type = "object",
  properties = { paths = { type = "array", items = { type = "string" } } },
  required = { "paths" },
}

return {
  {
    name = "git_status",
    description = "Uncommitted changes shown in the Git panel, per repository: branch, conflicts, staged, changes, untracked.",
    schema = { type = "object", properties = vim.empty_dict() },
    run = function(_, done)
      local gs, out = require("gitstatus"), {}
      for _, repo in ipairs(repos()) do
        local st = gs.status(repo)
        out[#out + 1] = ("# %s (%s)"):format(repo, st.branch)
        for _, sec in ipairs(SECTIONS) do
          for _, e in ipairs(st[sec]) do
            out[#out + 1] = ("%-9s %s %s%s"):format(sec, e.code, e.path, e.orig and (" ← " .. e.orig) or "")
          end
        end
      end
      done(#out > 0 and table.concat(out, "\n") or "No git repository in this project.")
    end,
  },
  {
    name = "git_stage",
    description = "Stage files (git add -A) in the Git panel.",
    schema = paths_schema,
    run = function(args, done)
      each_repo({ "add", "-A", "--" }, args.paths or {}, done)
    end,
  },
  {
    name = "git_unstage",
    description = "Unstage files (git reset); the working tree is not changed.",
    schema = paths_schema,
    run = function(args, done)
      each_repo({ "reset", "-q", "--" }, args.paths or {}, done)
    end,
  },
  {
    name = "git_discard",
    description = "Discard working tree changes of files (untracked files are deleted). The user must approve in the "
      .. "editor; staged files must be unstaged first.",
    schema = paths_schema,
    run = function(args, done, ctx)
      local gs, entries, skipped = require("gitstatus"), {}, {}
      for repo, rels in pairs((group(args.paths or {}))) do
        local st = gs.status(repo)
        for _, rel in ipairs(rels) do
          local e = vim.iter(vim.list_extend(vim.deepcopy(st.changes), st.untracked)):find(function(x)
            return x.path == rel
          end)
          if e then
            entries[#entries + 1] = e
          else
            skipped[#skipped + 1] = rel
          end
        end
      end
      if #entries == 0 then
        return done("Nothing to discard (only unstaged changes and untracked files can be discarded).", true)
      end
      local lines = { "Discard these changes? This cannot be undone.", "" }
      for _, e in ipairs(entries) do
        lines[#lines + 1] = ("  %s %s"):format(e.section == "untracked" and "delete " or "discard", e.path)
      end
      mcp.approve((ctx.agent or "AI") .. " · git discard", lines, function(ok)
        if not ok then
          return done("The user rejected the discard.", true)
        end
        for _, e in ipairs(entries) do
          gs.discard_entry(e)
        end
        done(("Discarded %d file(s).%s"):format(#entries,
          #skipped > 0 and (" Skipped (staged or unchanged): " .. table.concat(skipped, ", ")) or ""))
      end)
    end,
  },
  {
    name = "git_show_diff",
    description = "Open the diff of a changed file as a tab in the editor (staged = index vs HEAD, otherwise working tree vs index).",
    schema = {
      type = "object",
      properties = { path = { type = "string" }, staged = { type = "boolean" } },
      required = { "path" },
    },
    run = function(args, done)
      local repo = locate(args.path)
      if not repo then
        return done("Not in a repository: " .. args.path, true)
      end
      require("gitdiff").open({ path = mcp.abspath(args.path), staged = args.staged == true, cwd = repo })
      done("Diff opened.")
    end,
  },
  {
    name = "git_show_commit",
    description = "Open a commit's changes as a diff tab in the editor.",
    schema = {
      type = "object",
      properties = {
        rev = { type = "string", description = "Commit hash, branch or tag" },
        repo = { type = "string", description = "Repository path when the project has several" },
      },
      required = { "rev" },
    },
    run = function(args, done)
      local repo = args.repo and (locate(args.repo)) or repos()[1]
      if not repo then
        return done("No git repository in this project.", true)
      end
      if not require("gitdiff").open({ rev = args.rev, cwd = repo }) then
        return done("Could not open " .. args.rev, true)
      end
      done("Commit opened.")
    end,
  },
}
