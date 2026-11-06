--- Aranabilir kısayol listesi (Cmd+K, `:Shortcuts`): `KISAYOLLAR.md`'deki tablo satırları Snacks picker'da.
--- Tek kaynak doküman; yeni kısayol oraya yazılınca burada da görünür. Arama Türkçe harfleri ASCII'ye de katlar
--- ("satir secme" → "satır seç").
local M = {}

local FOLD = { ["ç"] = "c", ["ğ"] = "g", ["ı"] = "i", ["ö"] = "o", ["ş"] = "s", ["ü"] = "u", ["Ç"] = "c", ["Ğ"] = "g",
  ["İ"] = "i", ["Ö"] = "o", ["Ş"] = "s", ["Ü"] = "u" }

---@param s string
local function fold(s)
  return (s:gsub("[\195\196\197][\128-\191]", FOLD)):lower()
end

---@param s string
local function plain(s)
  return vim.trim((s:gsub("%*%*", ""):gsub("`", "")))
end

--- `KISAYOLLAR.md` tablolarını satır satır okur: tuş sütun(lar)ı + son sütun açıklama, üstteki `##` başlığı bölüm.
---@param file string
---@return table[]
function M.items(file)
  local lines = vim.fn.readfile(file)
  local items, section = {}, ""
  for i, line in ipairs(lines) do
    local heading = line:match("^##+%s+(.+)")
    if heading then
      section = heading
    elseif line:match("^|") and not line:match("^|%s*%-") and not (lines[i + 1] or ""):match("^|%s*%-") then
      local cells = vim.split((line:gsub("\\|", "\1"):gsub("^|", ""):gsub("|%s*$", "")), "|", { plain = true })
      cells = vim.tbl_map(function(c)
        return plain(c:gsub("\1", "|"))
      end, cells)
      if #cells >= 2 then
        local desc = table.remove(cells)
        local key = table.concat(vim.tbl_filter(function(c) return c ~= "" and c ~= "—" end, cells), " · ")
        local text = key .. " " .. desc .. " " .. section
        items[#items + 1] = { text = text .. " " .. fold(text), key = key, desc = desc, section = section, file = file,
          pos = { i, 0 } }
      end
    end
  end
  return items
end

--- Kısayol arama penceresini açar; seçim yalnız kapatır (önizleme dokümanın ilgili satırını gösterir).
function M.open()
  local file = vim.fn.stdpath("config") .. "/KISAYOLLAR.md"
  if vim.fn.filereadable(file) == 0 then
    vim.notify("Shortcut list not found: " .. file, vim.log.levels.WARN)
    return
  end
  Snacks.picker({
    title = "Shortcuts",
    items = M.items(file),
    preview = "file",
    format = function(item)
      return {
        { item.key, "Special" },
        { "  " },
        { item.desc },
        { "  " .. item.section, "Comment" },
      }
    end,
    confirm = function(picker)
      picker:close()
    end,
  })
end

return M
