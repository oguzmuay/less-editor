--- IntelliJ renk şemaları: JetBrains'in "Dark" (New UI), "IntelliJ Light" ve klasik "Darcula" editör paletleri.
--- Renkler IDE'nin varsayılan şemalarından (Settings > Editor > Color Scheme) alındı; Java/TS için IntelliJ'deki
--- ayrımlar korunur: anahtar kelime + `;`/`,` aynı renkte, sınıf adları düz, metot tanımı renkli, alanlar mor,
--- statik alan/sabit italik, anotasyon sarı. Treesitter, LSP semantic token (jdtls/vtsls) ve LESS panelleri kapsanır.
local M = {}

---@class IntellijPalette
---@field background "dark"|"light"
---@field bg string editör arka planı
---@field panel string araç pencereleri (Files, Git, yüzen pencereler)
---@field line string imleç satırı
---@field sel string seçim
---@field search string arama eşleşmesi
---@field border string ayırıcı / çerçeve
---@field fg string
---@field dim string ikincil metin
---@field linenr string
---@field linenr_cur string
---@field keyword string
---@field string string
---@field number string
---@field comment string
---@field doc string
---@field func string metot/fonksiyon tanımı
---@field call string metot çağrısı
---@field field string
---@field annotation string
---@field typeparam string
---@field tag string
---@field attr string
---@field escape string
---@field error string
---@field warn string
---@field info string
---@field hint string
---@field add string
---@field change string
---@field delete string
---@field diff_add string
---@field diff_change string
---@field diff_delete string
---@field diff_text string
---@field match string eşleşen parantez arka planı
---@field ref string LSP referans vurgusu
---@field accent string seçili sekme / başlık vurgusu
---@field terminal string[] 16 terminal rengi

---@type table<string, IntellijPalette>
M.palettes = {
  dark = {
    background = "dark",
    bg = "#1E1F22",
    panel = "#2B2D30",
    line = "#26282E",
    sel = "#214283",
    search = "#2D543F",
    border = "#393B40",
    fg = "#BCBEC4",
    dim = "#6F737A",
    linenr = "#4B5059",
    linenr_cur = "#A1A3AB",
    keyword = "#CF8E6D",
    string = "#6AAB73",
    number = "#2AACB8",
    comment = "#7A7E85",
    doc = "#5F826B",
    func = "#56A8F5",
    call = "#56A8F5",
    field = "#C77DBB",
    annotation = "#B3AE60",
    typeparam = "#16BAAC",
    tag = "#D5B778",
    attr = "#BABABA",
    escape = "#CF8E6D",
    error = "#F75464",
    warn = "#F2C55C",
    info = "#3592C4",
    hint = "#5FB865",
    add = "#549159",
    change = "#375FAD",
    delete = "#868A91",
    diff_add = "#294436",
    diff_change = "#25324D",
    diff_delete = "#484A4A",
    diff_text = "#385570",
    match = "#43454A",
    ref = "#373B39",
    accent = "#3574F0",
    terminal = {
      "#1E1F22", "#F75464", "#6AAB73", "#D5B778", "#56A8F5", "#C77DBB", "#2AACB8", "#BCBEC4",
      "#6F737A", "#FA6675", "#89CA8D", "#F2C55C", "#7EBCF7", "#D79BCC", "#5FCBD4", "#DFE1E5",
    },
  },
  light = {
    background = "light",
    bg = "#FFFFFF",
    panel = "#F7F8FA",
    line = "#F5F8FE",
    sel = "#A6D2FF",
    search = "#F6D97A",
    border = "#EBECF0",
    fg = "#080808",
    dim = "#8C8C8C",
    linenr = "#AEB3C2",
    linenr_cur = "#767A8A",
    keyword = "#0033B3",
    string = "#067D17",
    number = "#1750EB",
    comment = "#8C8C8C",
    doc = "#8C8C8C",
    func = "#00627A",
    call = "#080808",
    field = "#871094",
    annotation = "#9E880D",
    typeparam = "#20999D",
    tag = "#0033B3",
    attr = "#174AD4",
    escape = "#0037A6",
    error = "#E55757",
    warn = "#C7A450",
    info = "#3574F0",
    hint = "#5FB865",
    add = "#7AC98F",
    change = "#88ADF7",
    delete = "#C8CCD6",
    diff_add = "#E6F4E8",
    diff_change = "#EAF0FD",
    diff_delete = "#F0F0F0",
    diff_text = "#C9D9F8",
    match = "#93D9D9",
    ref = "#EDEBFC",
    accent = "#3574F0",
    terminal = {
      "#000000", "#E55757", "#067D17", "#9E880D", "#0033B3", "#871094", "#00627A", "#6C707E",
      "#8C8C8C", "#F25555", "#3B9C4B", "#C7A450", "#3574F0", "#A548B0", "#20999D", "#080808",
    },
  },
  darcula = {
    background = "dark",
    bg = "#2B2B2B",
    panel = "#3C3F41",
    line = "#323232",
    sel = "#214283",
    search = "#32593D",
    border = "#515151",
    fg = "#A9B7C6",
    dim = "#787878",
    linenr = "#606366",
    linenr_cur = "#A4A3A3",
    keyword = "#CC7832",
    string = "#6A8759",
    number = "#6897BB",
    comment = "#808080",
    doc = "#629755",
    func = "#FFC66D",
    call = "#A9B7C6",
    field = "#9876AA",
    annotation = "#BBB529",
    typeparam = "#507874",
    tag = "#E8BF6A",
    attr = "#BABABA",
    escape = "#CC7832",
    error = "#BC3F3C",
    warn = "#BE9117",
    info = "#3592C4",
    hint = "#629755",
    add = "#384C38",
    change = "#374752",
    delete = "#656E76",
    diff_add = "#294436",
    diff_change = "#385570",
    diff_delete = "#484A4A",
    diff_text = "#43698D",
    match = "#3B514D",
    ref = "#344134",
    accent = "#4A88C7",
    terminal = {
      "#2B2B2B", "#BC3F3C", "#6A8759", "#BBB529", "#6897BB", "#9876AA", "#299999", "#A9B7C6",
      "#787878", "#E06C75", "#A5C25C", "#FFC66D", "#8DB4E2", "#B788D3", "#5FBFBF", "#FFFFFF",
    },
  },
}

--- Paletten tüm vurgu gruplarını üretir.
---@param c IntellijPalette
---@return table<string, vim.api.keyset.highlight>
local function groups(c)
  local static = { fg = c.field, italic = true }
  return {
    Normal = { fg = c.fg, bg = c.bg },
    NormalNC = { fg = c.fg, bg = c.bg },
    NormalFloat = { fg = c.fg, bg = c.panel },
    FloatBorder = { fg = c.border, bg = c.panel },
    FloatTitle = { fg = c.fg, bg = c.panel, bold = true },
    NormalSB = { fg = c.fg, bg = c.panel },
    Cursor = { fg = c.bg, bg = c.fg },
    lCursor = { link = "Cursor" },
    TermCursor = { link = "Cursor" },
    CursorLine = { bg = c.line },
    CursorColumn = { bg = c.line },
    ColorColumn = { bg = c.line },
    CursorLineNr = { fg = c.linenr_cur },
    LineNr = { fg = c.linenr },
    LineNrAbove = { fg = c.linenr },
    LineNrBelow = { fg = c.linenr },
    SignColumn = { bg = c.bg },
    FoldColumn = { fg = c.linenr, bg = c.bg },
    Folded = { fg = c.dim, bg = c.line },
    WinSeparator = { fg = c.border, bg = c.bg },
    VertSplit = { link = "WinSeparator" },
    StatusLine = { fg = c.fg, bg = c.panel },
    StatusLineNC = { fg = c.dim, bg = c.panel },
    TabLine = { fg = c.dim, bg = c.panel },
    TabLineFill = { bg = c.panel },
    TabLineSel = { fg = c.fg, bg = c.bg, bold = true },
    WinBar = { fg = c.fg, bg = c.bg },
    WinBarNC = { fg = c.dim, bg = c.bg },
    Pmenu = { fg = c.fg, bg = c.panel },
    PmenuSel = { bg = c.sel },
    PmenuSbar = { bg = c.panel },
    PmenuThumb = { bg = c.border },
    PmenuKind = { fg = c.keyword, bg = c.panel },
    PmenuExtra = { fg = c.dim, bg = c.panel },
    PmenuMatch = { fg = c.accent, bold = true },
    Visual = { bg = c.sel },
    VisualNOS = { bg = c.sel },
    Search = { bg = c.search },
    IncSearch = { fg = c.bg, bg = c.warn },
    CurSearch = { link = "IncSearch" },
    Substitute = { fg = c.bg, bg = c.error },
    MatchParen = { bg = c.match, bold = true },
    NonText = { fg = c.linenr },
    Whitespace = { fg = c.border },
    EndOfBuffer = { fg = c.bg },
    SpecialKey = { fg = c.linenr },
    Directory = { fg = c.func },
    Title = { fg = c.func, bold = true },
    ErrorMsg = { fg = c.error },
    WarningMsg = { fg = c.warn },
    MoreMsg = { fg = c.hint },
    ModeMsg = { fg = c.fg, bold = true },
    Question = { fg = c.info },
    QuickFixLine = { bg = c.sel },
    Conceal = { fg = c.dim },
    WildMenu = { bg = c.sel },
    SpellBad = { sp = c.error, undercurl = true },
    SpellCap = { sp = c.warn, undercurl = true },
    SpellRare = { sp = c.info, undercurl = true },
    SpellLocal = { sp = c.hint, undercurl = true },

    DiffAdd = { bg = c.diff_add },
    DiffChange = { bg = c.diff_change },
    DiffDelete = { fg = c.delete, bg = c.diff_delete },
    DiffText = { bg = c.diff_text },
    Added = { fg = c.string },
    Changed = { fg = c.info },
    Removed = { fg = c.error },
    diffAdded = { link = "Added" },
    diffChanged = { link = "Changed" },
    diffRemoved = { link = "Removed" },
    diffFile = { fg = c.func, bold = true },
    diffLine = { fg = c.typeparam },

    Comment = { fg = c.comment, italic = c.background == "light" },
    Constant = static,
    String = { fg = c.string },
    Character = { fg = c.string },
    Number = { fg = c.number },
    Float = { fg = c.number },
    Boolean = { fg = c.keyword },
    Identifier = { fg = c.fg },
    Function = { fg = c.func },
    Statement = { fg = c.keyword },
    Conditional = { fg = c.keyword },
    Repeat = { fg = c.keyword },
    Label = { fg = c.keyword },
    Operator = { fg = c.fg },
    Keyword = { fg = c.keyword },
    Exception = { fg = c.keyword },
    PreProc = { fg = c.annotation },
    Include = { fg = c.keyword },
    Define = { fg = c.keyword },
    Macro = { fg = c.annotation },
    PreCondit = { fg = c.annotation },
    Type = { fg = c.fg },
    StorageClass = { fg = c.keyword },
    Structure = { fg = c.keyword },
    Typedef = { fg = c.keyword },
    Special = { fg = c.escape },
    SpecialChar = { fg = c.escape },
    Tag = { fg = c.tag },
    Delimiter = { fg = c.keyword },
    SpecialComment = { fg = c.doc, italic = true },
    Debug = { fg = c.warn },
    Underlined = { underline = true },
    Error = { fg = c.error },
    Todo = { fg = c.annotation, bold = true, italic = true },

    ["@variable"] = { fg = c.fg },
    ["@variable.builtin"] = { fg = c.keyword },
    ["@variable.parameter"] = { fg = c.fg },
    ["@variable.member"] = { fg = c.field },
    ["@constant"] = static,
    ["@constant.builtin"] = { fg = c.keyword },
    ["@constant.macro"] = static,
    ["@module"] = { fg = c.fg },
    ["@label"] = { fg = c.fg },
    ["@string"] = { link = "String" },
    ["@string.escape"] = { fg = c.escape },
    ["@string.regexp"] = { fg = c.string },
    ["@string.special"] = { fg = c.escape },
    ["@string.special.url"] = { fg = c.info, underline = true },
    ["@character"] = { link = "Character" },
    ["@number"] = { link = "Number" },
    ["@number.float"] = { link = "Number" },
    ["@boolean"] = { fg = c.keyword },
    ["@type"] = { fg = c.fg },
    ["@type.builtin"] = { fg = c.keyword },
    ["@type.definition"] = { fg = c.fg },
    ["@attribute"] = { fg = c.annotation },
    ["@attribute.builtin"] = { fg = c.annotation },
    ["@property"] = { fg = c.field },
    ["@function"] = { fg = c.func },
    ["@function.builtin"] = { fg = c.call },
    ["@function.call"] = { fg = c.call },
    ["@function.macro"] = { fg = c.annotation },
    ["@function.method"] = { fg = c.func },
    ["@function.method.call"] = { fg = c.call },
    ["@constructor"] = { fg = c.fg },
    ["@operator"] = { fg = c.fg },
    ["@keyword"] = { fg = c.keyword },
    ["@keyword.operator"] = { fg = c.keyword },
    ["@keyword.directive"] = { fg = c.annotation },
    ["@punctuation.delimiter"] = { fg = c.keyword },
    ["@punctuation.bracket"] = { fg = c.fg },
    ["@punctuation.special"] = { fg = c.escape },
    ["@comment"] = { link = "Comment" },
    ["@comment.documentation"] = { fg = c.doc, italic = true },
    ["@comment.error"] = { fg = c.error, bold = true },
    ["@comment.warning"] = { fg = c.warn, bold = true },
    ["@comment.todo"] = { link = "Todo" },
    ["@comment.note"] = { fg = c.info, bold = true },
    ["@tag"] = { fg = c.tag },
    ["@tag.builtin"] = { fg = c.tag },
    ["@tag.attribute"] = { fg = c.attr },
    ["@tag.delimiter"] = { fg = c.tag },
    ["@markup.heading"] = { fg = c.keyword, bold = true },
    ["@markup.strong"] = { bold = true },
    ["@markup.italic"] = { italic = true },
    ["@markup.strikethrough"] = { strikethrough = true },
    ["@markup.underline"] = { underline = true },
    ["@markup.raw"] = { fg = c.string },
    ["@markup.link"] = { fg = c.info },
    ["@markup.link.url"] = { fg = c.info, underline = true },
    ["@markup.link.label"] = { fg = c.func },
    ["@markup.list"] = { fg = c.keyword },
    ["@markup.quote"] = { fg = c.comment, italic = true },
    ["@diff.plus"] = { link = "Added" },
    ["@diff.minus"] = { link = "Removed" },
    ["@diff.delta"] = { link = "Changed" },

    ["@lsp.type.class"] = { fg = c.fg },
    ["@lsp.type.interface"] = { fg = c.fg },
    ["@lsp.type.enum"] = { fg = c.fg },
    ["@lsp.type.type"] = { fg = c.fg },
    ["@lsp.type.namespace"] = { fg = c.fg },
    ["@lsp.type.typeParameter"] = { fg = c.typeparam },
    ["@lsp.type.annotation"] = { fg = c.annotation },
    ["@lsp.type.decorator"] = { fg = c.annotation },
    ["@lsp.type.property"] = { fg = c.field },
    ["@lsp.type.enumMember"] = static,
    ["@lsp.type.variable"] = {},
    ["@lsp.type.parameter"] = { fg = c.fg },
    ["@lsp.type.method"] = { fg = c.call },
    ["@lsp.type.function"] = { fg = c.call },
    ["@lsp.type.keyword"] = { fg = c.keyword },
    ["@lsp.type.modifier"] = { fg = c.keyword },
    ["@lsp.typemod.method.declaration"] = { fg = c.func },
    ["@lsp.typemod.function.declaration"] = { fg = c.func },
    ["@lsp.typemod.property.static"] = static,
    ["@lsp.typemod.variable.static"] = static,
    ["@lsp.typemod.method.static"] = { italic = true },
    ["@lsp.mod.deprecated"] = { strikethrough = true },

    DiagnosticError = { fg = c.error },
    DiagnosticWarn = { fg = c.warn },
    DiagnosticInfo = { fg = c.info },
    DiagnosticHint = { fg = c.hint },
    DiagnosticOk = { fg = c.hint },
    DiagnosticUnderlineError = { sp = c.error, undercurl = true },
    DiagnosticUnderlineWarn = { sp = c.warn, undercurl = true },
    DiagnosticUnderlineInfo = { sp = c.info, underline = true },
    DiagnosticUnderlineHint = { sp = c.hint, underline = true },
    DiagnosticUnnecessary = { fg = c.dim },
    DiagnosticDeprecated = { strikethrough = true },
    LspReferenceText = { bg = c.ref },
    LspReferenceRead = { bg = c.ref },
    LspReferenceWrite = { bg = c.ref, underline = true },
    LspInlayHint = { fg = c.dim, bg = c.line },
    LspCodeLens = { fg = c.dim },
    LspSignatureActiveParameter = { bold = true, underline = true },

    GitSignsAdd = { fg = c.add },
    GitSignsChange = { fg = c.change },
    GitSignsDelete = { fg = c.delete },
    NeoTreeNormal = { fg = c.fg, bg = c.panel },
    NeoTreeNormalNC = { fg = c.fg, bg = c.panel },
    NeoTreeEndOfBuffer = { fg = c.panel, bg = c.panel },
    NeoTreeWinSeparator = { fg = c.bg, bg = c.bg },
    NeoTreeCursorLine = { bg = c.sel },
    NeoTreeDirectoryName = { fg = c.fg },
    NeoTreeDirectoryIcon = { fg = c.dim },
    NeoTreeRootName = { fg = c.fg, bold = true },
    NeoTreeGitModified = { fg = c.info },
    NeoTreeGitAdded = { fg = c.string },
    NeoTreeGitUntracked = { fg = c.error },
    NeoTreeGitDeleted = { fg = c.delete },
    NeoTreeGitIgnored = { fg = c.dim },
    NeoTreeGitConflict = { fg = c.error, bold = true },
    SnacksIndent = { fg = c.border },
    SnacksIndentScope = { fg = c.linenr },
    SnacksPickerMatch = { fg = c.accent, bold = true },
    SnacksDashboardHeader = { fg = c.func },
    SnacksDashboardKey = { fg = c.keyword },
    SnacksDashboardIcon = { fg = c.func },
    SnacksDashboardDesc = { fg = c.fg },
    SnacksDashboardFooter = { fg = c.dim },
    FlashLabel = { fg = c.bg, bg = c.error, bold = true },
    FlashMatch = { bg = c.search },
    TreesitterContext = { bg = c.panel },
    TreesitterContextLineNumber = { fg = c.linenr, bg = c.panel },
    WhichKeyNormal = { link = "NormalFloat" },
    BlinkCmpMenuBorder = { link = "FloatBorder" },
    BlinkCmpDocBorder = { link = "FloatBorder" },
    BlinkCmpKind = { fg = c.keyword },
  }
end

--- Şemayı uygular (`colors/intellij-dark.lua`, `intellij-light.lua`, `darcula.lua` çağırır).
---@param variant "dark"|"light"|"darcula"
function M.load(variant)
  local c = M.palettes[variant]
  vim.cmd("hi clear")
  if vim.g.syntax_on then
    vim.cmd("syntax reset")
  end
  vim.o.background = c.background
  vim.o.termguicolors = true
  vim.g.colors_name = variant == "darcula" and "darcula" or "intellij-" .. variant
  for name, hl in pairs(groups(c)) do
    vim.api.nvim_set_hl(0, name, hl)
  end
  for i, color in ipairs(c.terminal) do
    vim.g["terminal_color_" .. (i - 1)] = color
  end
end

return M
