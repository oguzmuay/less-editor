--- Git History için VS Code tarzı commit grafiği: şeritler (lane) commit parent'larından hesaplanır, yuvarlak
--- köşelerle (╭ ╮ ╰ ╯) çizilir, her şerit kendi rengindedir. Merge commit ○, diğerleri ●.
local M = {}

M.COLORS = 8

--- Commit'leri (topo sırada, çocuk önce) satır satır grafik hücrelerine çevirir. Her şerit 2 sütundur: sembol +
--- sağındaki bağlantı (─ ya da boşluk). Listede olmayan parent'lar (ör. `-n` sınırı) şeridi aşağıya kadar açık bırakır.
---@param commits { hash: string, parents: string[] }[]
---@return { [1]: string, [2]: integer? }[][] her satır için hücreler: { karakter, renk no (1..COLORS) }
function M.layout(commits)
  local lanes = {} ---@type ({ hash: string, color: integer }|false)[]
  local last_color = 0
  local function new_lane(hash)
    last_color = last_color % M.COLORS + 1
    return { hash = hash, color = last_color }
  end
  local function find(hash)
    for i, l in ipairs(lanes) do
      if l and l.hash == hash then
        return i
      end
    end
  end
  local function free_slot(after)
    for i = after + 1, #lanes do
      if not lanes[i] then
        return i
      end
    end
    return #lanes + 1
  end

  local rows = {}
  for _, c in ipairs(commits) do
    local col = find(c.hash)
    if not col then
      col = free_slot(0)
      lanes[col] = new_lane(c.hash)
    end
    local sym, conn = {}, {}
    for i, l in ipairs(lanes) do
      if l then
        sym[i] = { "│", l.color }
      end
    end
    sym[col] = { #c.parents > 1 and "○" or "●", lanes[col].color }

    local function link(j, ch, color)
      local a, b = math.min(col, j), math.max(col, j)
      for k = a, b - 1 do
        conn[k] = { "─", color }
        if k > a and sym[k] and sym[k][1] == "│" then
          sym[k] = { "┼", sym[k][2] }
        end
      end
      sym[j] = { ch, color }
    end

    for i, l in ipairs(lanes) do
      if l and i ~= col and l.hash == c.hash then
        link(i, "╯", l.color)
        lanes[i] = false
      end
    end
    if c.parents[1] then
      lanes[col].hash = c.parents[1]
    else
      lanes[col] = false
    end
    for k = 2, #c.parents do
      local j = find(c.parents[k])
      if j and j ~= col then
        link(j, j > col and "┤" or "├", lanes[j].color)
      elseif not j then
        j = free_slot(col)
        lanes[j] = new_lane(c.parents[k])
        link(j, "╮", lanes[j].color)
      end
    end
    while #lanes > 0 and not lanes[#lanes] do
      lanes[#lanes] = nil
    end

    local cells, width = {}, math.max(table.maxn(sym), table.maxn(conn))
    for i = 1, width do
      cells[#cells + 1] = sym[i] or { " " }
      cells[#cells + 1] = conn[i] or { " " }
    end
    rows[#rows + 1] = cells
  end
  return rows
end

return M
