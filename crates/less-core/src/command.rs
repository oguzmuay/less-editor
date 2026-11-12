//! Düzenleme komutları. Vim ve Normal (modeless) giriş katmanları aynı `Command` kümesini üretir.

/// İmleç hareketi. Sayaç `Command::Move::count` ile verilir.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Motion {
    /// `h`: satır içinde.
    Left,
    /// `l`: satır içinde.
    Right,
    /// Sol ok: satır başında önceki satırın sonuna geçer (`whichwrap`, karar #11).
    LeftWrap,
    /// Sağ ok: satır sonunda sonraki satıra geçer.
    RightWrap,
    Up,
    Down,
    /// `w` / `W`
    WordForward {
        big: bool,
    },
    /// `b` / `B`
    WordBackward {
        big: bool,
    },
    /// `e` / `E`
    WordEnd {
        big: bool,
    },
    /// `ge` / `gE`
    WordEndBackward {
        big: bool,
    },
    /// Option+Sol: önceki kelime başı (satır atlar).
    WordLeft,
    /// Option+Sağ: sonraki kelime sonu.
    WordRight,
    /// `0`
    LineStart,
    /// `^`
    FirstNonBlank,
    /// `$` (sayaçla aşağı satırların sonu).
    LineEnd,
    /// `g_`
    LastNonBlank,
    /// Cmd+Sol: ilk dolu karakter, oradaysa sütun 1 (karar #12).
    SmartHome,
    /// `gg` (sayaç = satır no).
    FileStart,
    /// `G` (sayaç = satır no, yoksa son satır).
    FileEnd,
    /// Metnin ilk karakteri (modeless Cmd+Yukarı).
    DocumentStart,
    /// Metnin sonu (modeless Cmd+Aşağı).
    DocumentEnd,
    /// `:N` — 1 tabanlı satır.
    Line(usize),
    /// `f` `F` `t` `T`
    FindChar {
        ch: char,
        forward: bool,
        till: bool,
    },
    /// `;` (false) / `,` (true)
    RepeatFind {
        reverse: bool,
    },
    /// `%` (sayaçla dosyanın yüzdesi)
    MatchPair,
    /// `}`
    ParagraphForward,
    /// `{`
    ParagraphBackward,
    /// `H` `M` `L`
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    /// Ctrl-d / Ctrl-u (görünümü de kaydırır)
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    /// `/desen` (true) / `?desen` (false); deseni son arama yapar.
    Search {
        pattern: String,
        forward: bool,
    },
    /// `n` (false) / `N` (true)
    SearchNext {
        reverse: bool,
    },
    /// `` `a `` (false) / `'a` (true)
    Mark {
        name: char,
        linewise: bool,
    },
}

/// Metin nesneleri (`iw`, `a(` ...). `around` = `a`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextObject {
    Word {
        big: bool,
    },
    /// `"`, `'`, `` ` ``
    Quote(char),
    /// `(`/`)`/`b`, `{`/`}`/`B`, `[`/`]`, `<`/`>`
    Pair {
        open: char,
        close: char,
    },
    Paragraph,
    /// HTML/XML etiketi (`it` / `at`)
    Tag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
    Indent,
    Outdent,
    ToggleCase,
    Lowercase,
    Uppercase,
    /// `=`: şimdilik metni değiştirmez.
    Format,
    /// `gc` / Cmd+/
    Comment,
}

/// Ex adresinin tabanı.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineBase {
    /// `.`
    Current,
    /// `$`
    Last,
    /// 1 tabanlı satır
    Number(usize),
    /// `'a`, `'<`, `'>`
    Mark(char),
}

/// `.+3`, `'<`, `$` gibi satır adresi.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineAddr {
    pub base: LineBase,
    pub offset: isize,
}

impl LineAddr {
    pub fn new(base: LineBase) -> Self {
        Self { base, offset: 0 }
    }

    pub fn current() -> Self {
        Self::new(LineBase::Current)
    }
}

/// Ex satır aralığı (`%` = 1,$).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRange {
    pub start: LineAddr,
    pub end: LineAddr,
}

impl LineRange {
    pub fn all() -> Self {
        Self { start: LineAddr::new(LineBase::Number(1)), end: LineAddr::new(LineBase::Last) }
    }

    pub fn current() -> Self {
        Self { start: LineAddr::current(), end: LineAddr::current() }
    }

    pub fn visual() -> Self {
        Self { start: LineAddr::new(LineBase::Mark('<')), end: LineAddr::new(LineBase::Mark('>')) }
    }
}

/// Operatörün etki alanı.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Motion {
        motion: Motion,
        count: Option<usize>,
    },
    TextObject {
        object: TextObject,
        around: bool,
        count: usize,
    },
    /// `dd` `yy` `cc` `>>` `gcc`: imleç satırından itibaren N satır.
    Lines(usize),
    /// Görsel seçim ya da modeless seçim.
    Selection,
    /// Modeless Cmd+C / Cmd+X: seçim boşsa geçerli satır.
    SelectionOrLine,
    /// Ex aralığı (`:'<,'>d`, `:%y`)
    Range(LineRange),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorMode {
    /// Vim Normal / Visual: imleç bir karakterin üstünde, satır sonuna geçemez.
    Block,
    /// Insert ve modeless: imleç karakterler arasında.
    Bar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualKind {
    Char,
    Line,
    Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAnchor {
    Top,
    Center,
    Bottom,
}

/// `:set` değeri.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetValue {
    On,
    Off,
    Toggle,
    Value(String),
    /// `:set ts?`
    Query,
}

/// Editörün kendi başına yapamadığı, uygulamanın karar verdiği istekler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppCommand {
    /// `:w [yol]`
    Write { path: Option<String>, force: bool },
    /// `:q` / `:qa`. Uygulama son kod penceresinde reddedebilir (karar #41).
    Quit { force: bool, all: bool },
    /// `:wq` (only_if_modified = false) / `:x` (true)
    WriteQuit { force: bool, all: bool, only_if_modified: bool },
    /// `:e [yol]`
    Edit { path: Option<String>, force: bool },
    /// Editörün işlemediği `:set` seçenekleri (ör. relativenumber)
    Set { name: String, value: SetValue },
    /// Bilinmeyen ex komutu (`:Files`, `:Git`, `:Theme` ...), `:` olmadan, olduğu gibi.
    Ex(String),
    /// Vim modunda `Space` ile başlayan LESS kısayolu (`Space` sonrası tuşlar: `"j"`, `"D"`, `"du"`, `"ga"`).
    Leader(String),
    /// Motorun bilmediği Normal mod dizisi (`gd`, `K`, `]d` ...), tuş gösterimiyle; uygulama LSP vb. eylemlere bağlar.
    Unmapped(String),
    /// Ses / uyarı: geçersiz komut, bulunamayan arama vb. Metin İngilizce.
    Message(String),
}

/// Editör komutu. `Editor::apply` uygular.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `extend` = seçim genişler (görsel mod, Shift+ok).
    Move {
        motion: Motion,
        count: Option<usize>,
        extend: bool,
    },
    Operate {
        op: Operator,
        target: Target,
        register: Option<char>,
    },
    /// Her imlece metin yazar; boş olmayan seçim varsa (modeless) yerine yazar; `overwrite` iken üzerine yazar.
    InsertText(String),
    /// Otomatik girintili yeni satır.
    InsertNewline,
    /// Girinti birimi (modeless'ta seçim varsa satırları girintiler).
    InsertTab,
    /// Shift+Tab (modeless).
    Outdent,
    Backspace,
    DeleteForward,
    /// Ctrl-w / Option+Backspace
    DeleteWordBackward,
    /// Ctrl-u / Cmd+Backspace
    DeleteToLineStart,
    /// `o` / `O`
    OpenLine {
        above: bool,
    },
    /// `J` (spaces) / `gJ`; count = birleşecek satır sayısı (en az 2).
    Join {
        count: usize,
        spaces: bool,
    },
    /// `r`
    ReplaceChar {
        ch: char,
        count: usize,
    },
    /// `~` (normal mod)
    ToggleCaseChar {
        count: usize,
    },
    /// `p` (before = false) / `P`. Görsel seçim varsa yerine yapıştırır.
    Paste {
        register: Option<char>,
        before: bool,
        count: usize,
    },
    /// Insert modda Ctrl-r {reg}: metni imlece yazar.
    PasteInsert {
        register: char,
    },
    Undo {
        count: usize,
    },
    Redo {
        count: usize,
    },
    /// Insert oturumu gibi birden çok komutu tek geri alma adımı yapar.
    BeginUndoGroup,
    EndUndoGroup,
    SetCursorMode(CursorMode),
    SetVisual(Option<VisualKind>),
    /// Replace modu (`R`)
    SetOverwrite(bool),
    /// Görselde `o`
    SwapAnchor,
    SelectAll,
    SelectTextObject {
        object: TextObject,
        around: bool,
        count: usize,
    },
    /// Yalnız birincil imleç kalır, seçim kalkar.
    CollapseSelection,
    /// Alt / üst satıra imleç ekler.
    AddCursor {
        below: bool,
    },
    /// Görsel blokta `I` / `A`: her satıra imleç.
    BlockToCursors {
        append: bool,
    },
    /// Option+J / Option+K
    MoveLines {
        down: bool,
    },
    /// `m{a-z}`
    SetMark(char),
    /// `*` `#` (whole) / `g*` `g#`
    SearchWordUnderCursor {
        forward: bool,
        whole: bool,
    },
    /// `:noh`
    ClearSearchHighlight,
    /// `:s/desen/metin/bayraklar`
    Substitute {
        range: LineRange,
        pattern: String,
        replacement: String,
        global: bool,
        ignore_case: Option<bool>,
    },
    /// `zt` `zz` `zb`
    Scroll(ScrollAnchor),
    /// Ctrl-e / Ctrl-y / fare tekerleği
    ScrollLines(isize),
    /// `:.+3`, `:'a`
    GotoAddr(LineAddr),
    /// `gv`
    ReselectVisual,
    /// `:set` (editörün bildiği seçenekler: tabstop, shiftwidth, expandtab, hlsearch, ignorecase, smartcase, wrapscan)
    Set {
        name: String,
        value: SetValue,
    },
    App(AppCommand),
}
