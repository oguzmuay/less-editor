//! Terminal emülasyonu: `portable-pty` çocuk süreci + `alacritty_terminal` ayrıştırıcısı.
//!
//! İş parçacığı modeli: her pty oturumunun bir okuma iş parçacığı vardır; pty'den gelen baytları kilit altında
//! `Term`'e işler, `generation`'ı artırır, uygulamanın istediği yanıtları (DSR, renk sorgusu) pty'ye yazar ve
//! `on_change` kancasını çağırır (UI `request_repaint`). Süreç bitince aynı iş parçacığı çıkış kodunu bekler.
//! UI tarafı yalnız [`Terminal::snapshot`] (kısa kilit) ve yazma/yeniden boyutlandırma çağırır.
//! [`Terminal::feed_only`] pty'siz kiptir: Run logları aynı ızgarayla çizilsin diye baytlar elle verilir.

pub mod grid;
pub mod keys;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{self, Color, NamedColor, Processor, Rgb};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize};

pub use grid::{CellStyle, CursorShape, CursorView, GridCell, GridSnapshot, ModeFlags, TermColor};
pub use keys::{
    Key, KeyOptions, Modifiers, MouseAction, MouseButton, MouseEvent, encode_focus, encode_key, encode_mouse,
    encode_paste,
};

/// Varsayılan geçmiş (scrollback) satır sayısı.
pub const SCROLLBACK: usize = 10_000;

/// Terminal boyutu (hücre).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermSize {
    pub cols: u16,
    pub rows: u16,
}

impl TermSize {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self { cols: cols.max(2), rows: rows.max(1) }
    }
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

/// Başlatma isteği. `env` mevcut ortamın üstüne eklenir; `TERM=xterm-256color` ve `COLORTERM=truecolor` her zaman.
#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    pub argv: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
    pub env: HashMap<String, String>,
    pub size: Option<TermSize>,
}

/// Seçim türü: karakter, kelime (çift tık) ya da satır (üçlü tık).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Simple,
    Word,
    Line,
    Block,
}

type Hook = Arc<dyn Fn() + Send + Sync>;

/// `Term`'in olaylarını toplayan dinleyici; ayrıştırma sırasında çağrıldığı için kendi küçük kilidi vardır.
#[derive(Clone, Default)]
struct Listener {
    events: Arc<Mutex<Vec<Event>>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        lock(&self.events).push(event);
    }
}

struct Inner {
    term: Term<Listener>,
    processor: Processor,
    events: Arc<Mutex<Vec<Event>>>,
    generation: u64,
    title: Option<String>,
    bell: bool,
    exit: Option<ExitInfo>,
    size: TermSize,
}

/// Sürecin çıkışı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitInfo {
    pub code: u32,
    pub signal: Option<String>,
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

/// Bir terminal oturumu (pty'li ya da yalnız besleme). Ucuz klonlanır; tüm klonlar aynı oturumu paylaşır.
#[derive(Clone)]
pub struct Terminal {
    inner: Arc<Mutex<Inner>>,
    writer: Option<Writer>,
    master: Option<Arc<Mutex<Box<dyn MasterPty + Send>>>>,
    killer: Option<Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>>,
    pid: Option<u32>,
    hook: Arc<Mutex<Option<Hook>>>,
    alive: Arc<AtomicBool>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Terminal {
    fn new_inner(size: TermSize) -> Inner {
        let listener = Listener::default();
        let events = listener.events.clone();
        let config = Config { scrolling_history: SCROLLBACK, ..Config::default() };
        Inner {
            term: Term::new(config, &size, listener),
            processor: Processor::new(),
            events,
            generation: 1,
            title: None,
            bell: false,
            exit: None,
            size,
        }
    }

    /// pty'siz terminal: baytlar [`Terminal::feed`] ile verilir (Run logları).
    pub fn feed_only(size: TermSize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Self::new_inner(size))),
            writer: None,
            master: None,
            killer: None,
            pid: None,
            hook: Arc::new(Mutex::new(None)),
            alive: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Komutu yeni bir pty'de başlatır ve okuma iş parçacığını kurar.
    pub fn spawn(opts: SpawnOptions) -> std::io::Result<Self> {
        let size = opts.size.unwrap_or(TermSize::new(80, 24));
        let Some((program, args)) = opts.argv.split_first() else {
            return Err(std::io::Error::other("empty command"));
        };
        let pty = portable_pty::native_pty_system();
        let pair = pty
            .openpty(PtySize { rows: size.rows, cols: size.cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        if let Some(cwd) = &opts.cwd {
            cmd.cwd(cwd);
        }
        for k in inherited_claude_vars(std::env::vars_os().map(|(k, _)| k)) {
            cmd.env_remove(k);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| std::io::Error::other(e.to_string()))?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| std::io::Error::other(e.to_string()))?;
        let writer: Writer =
            Arc::new(Mutex::new(pair.master.take_writer().map_err(|e| std::io::Error::other(e.to_string()))?));
        let pid = child.process_id();
        let killer = child.clone_killer();
        let terminal = Self {
            inner: Arc::new(Mutex::new(Self::new_inner(size))),
            writer: Some(writer),
            master: Some(Arc::new(Mutex::new(pair.master))),
            killer: Some(Arc::new(Mutex::new(killer))),
            pid,
            hook: Arc::new(Mutex::new(None)),
            alive: Arc::new(AtomicBool::new(true)),
        };
        let reader_side = terminal.clone();
        std::thread::Builder::new().name("less-ai-pty".into()).spawn(move || {
            reader_side.read_loop(reader);
            let status = child.wait().ok();
            let exit = status.map(|s| ExitInfo { code: s.exit_code(), signal: s.signal().map(str::to_string) });
            reader_side.alive.store(false, Ordering::SeqCst);
            {
                let mut inner = lock(&reader_side.inner);
                inner.exit = exit.or(Some(ExitInfo { code: 1, signal: None }));
                inner.generation += 1;
            }
            reader_side.notify();
        })?;
        Ok(terminal)
    }

    fn read_loop(&self, mut reader: Box<dyn Read + Send>) {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.process(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    }

    fn process(&self, bytes: &[u8]) {
        let replies = {
            let mut guard = lock(&self.inner);
            let inner = &mut *guard;
            inner.processor.advance(&mut inner.term, bytes);
            inner.generation += 1;
            let events: Vec<Event> = std::mem::take(&mut *lock(&inner.events));
            let mut replies = Vec::new();
            for ev in events {
                match ev {
                    Event::Title(t) => inner.title = Some(t),
                    Event::ResetTitle => inner.title = None,
                    Event::Bell => inner.bell = true,
                    Event::PtyWrite(s) => replies.push(s),
                    Event::ColorRequest(idx, fmt) => {
                        let rgb = inner.term.colors()[idx].unwrap_or(default_rgb(idx));
                        replies.push(fmt(rgb));
                    }
                    Event::TextAreaSizeRequest(fmt) => {
                        let size = inner.size;
                        replies.push(fmt(alacritty_terminal::event::WindowSize {
                            num_lines: size.rows,
                            num_cols: size.cols,
                            cell_width: 8,
                            cell_height: 16,
                        }));
                    }
                    _ => {}
                }
            }
            replies
        };
        for r in replies {
            let _ = self.write(r.as_bytes());
        }
        self.notify();
    }

    fn notify(&self) {
        let hook = lock(&self.hook).clone();
        if let Some(h) = hook {
            h();
        }
    }

    /// İçerik değişince (okuma iş parçacığından) çağrılacak kanca; UI burada yeniden çizim ister.
    pub fn set_on_change(&self, hook: impl Fn() + Send + Sync + 'static) {
        *lock(&self.hook) = Some(Arc::new(hook));
    }

    /// Baytları doğrudan ayrıştırıcıya verir (pty'siz kip; pty'li oturumda da ekrana yazmak için kullanılabilir).
    pub fn feed(&self, bytes: &[u8]) {
        self.process(bytes);
    }

    /// Log metni besler: tek başına `\n` → `\r\n` (borudan gelen çıktı satır başına dönmez).
    pub fn feed_log(&self, bytes: &[u8]) {
        let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 16);
        let mut prev = 0u8;
        for &b in bytes {
            if b == b'\n' && prev != b'\r' {
                out.push(b'\r');
            }
            out.push(b);
            prev = b;
        }
        self.process(&out);
    }

    /// Sürece bayt yazar (pty'siz kipte etkisiz).
    pub fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        let Some(w) = &self.writer else { return Ok(()) };
        let mut w = lock(w);
        w.write_all(bytes)?;
        w.flush()
    }

    /// Tuşu kodlayıp gönderir; Shift+Enter yeni satırı gibi çok parçalı girdide parçalar arası 10 ms beklenir
    /// (bekleme ayrı iş parçacığında, çağıran bloklanmaz). Canlı görünüme dönmek için kaydırma sıfırlanır.
    pub fn send_key(&self, key: Key, mods: Modifiers, shift_enter_newline: bool) {
        let opts = KeyOptions::from_mode(&self.mode(), shift_enter_newline);
        let chunks = encode_key(key, mods, opts);
        if chunks.is_empty() {
            return;
        }
        self.scroll_to_bottom();
        self.write_chunks(chunks, Duration::from_millis(10));
    }

    /// Parçaları sırayla, aralarında `gap` bekleyerek yazar.
    pub fn write_chunks(&self, chunks: Vec<Vec<u8>>, gap: Duration) {
        if chunks.len() <= 1 {
            for c in chunks {
                let _ = self.write(&c);
            }
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || {
            for (i, c) in chunks.iter().enumerate() {
                if i > 0 {
                    std::thread::sleep(gap);
                }
                let _ = me.write(c);
            }
        });
    }

    /// Metni yapıştırır (bracketed paste kipine göre).
    pub fn paste(&self, text: &str) {
        let bracketed = self.mode().bracketed_paste;
        self.scroll_to_bottom();
        let _ = self.write(&encode_paste(text, bracketed));
    }

    /// Fare olayını uygulama istiyorsa gönderir; gönderildiyse `true` (değilse UI seçim/kaydırma yapar).
    pub fn send_mouse(&self, ev: MouseEvent) -> bool {
        match encode_mouse(ev, &self.mode()) {
            Some(bytes) => self.write(&bytes).is_ok(),
            None => false,
        }
    }

    /// Odak değişimini, uygulama 1004 kipini açtıysa bildirir.
    pub fn send_focus(&self, focused: bool) {
        if self.mode().focus_events {
            let _ = self.write(&encode_focus(focused));
        }
    }

    /// Ekranı ve pty'yi yeniden boyutlandırır.
    pub fn resize(&self, size: TermSize) {
        {
            let mut inner = lock(&self.inner);
            if inner.size == size {
                return;
            }
            inner.size = size;
            inner.term.resize(size);
            inner.generation += 1;
        }
        if let Some(m) = &self.master {
            let _ = lock(m).resize(PtySize { rows: size.rows, cols: size.cols, pixel_width: 0, pixel_height: 0 });
        }
    }

    pub fn size(&self) -> TermSize {
        lock(&self.inner).size
    }

    /// Görünümü `delta` satır kaydırır (pozitif = geçmişe, yukarı).
    pub fn scroll(&self, delta: i32) {
        let mut inner = lock(&self.inner);
        let before = inner.term.grid().display_offset();
        inner.term.scroll_display(Scroll::Delta(delta));
        if inner.term.grid().display_offset() != before {
            inner.generation += 1;
        }
    }

    pub fn scroll_page(&self, up: bool) {
        let mut inner = lock(&self.inner);
        inner.term.scroll_display(if up { Scroll::PageUp } else { Scroll::PageDown });
        inner.generation += 1;
    }

    /// Canlı görünüme (en alta) döner.
    pub fn scroll_to_bottom(&self) {
        let mut inner = lock(&self.inner);
        if inner.term.grid().display_offset() != 0 {
            inner.term.scroll_display(Scroll::Bottom);
            inner.generation += 1;
        }
    }

    fn point(inner: &Inner, col: usize, row: usize) -> Point {
        let offset = inner.term.grid().display_offset() as i32;
        let cols = inner.size.cols as usize;
        Point::new(Line(row as i32 - offset), Column(col.min(cols.saturating_sub(1))))
    }

    /// Görünüm koordinatında seçim başlatır.
    pub fn select_start(&self, col: usize, row: usize, kind: SelectionKind) {
        let mut inner = lock(&self.inner);
        let p = Self::point(&inner, col, row);
        let ty = match kind {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
            SelectionKind::Block => SelectionType::Block,
        };
        inner.term.selection = Some(Selection::new(ty, p, Side::Left));
        inner.generation += 1;
    }

    /// Seçimin ucunu taşır (sürükleme). Ucun bulunduğu hücre seçime dahildir.
    pub fn select_update(&self, col: usize, row: usize) {
        let mut inner = lock(&self.inner);
        let p = Self::point(&inner, col, row);
        if let Some(sel) = inner.term.selection.as_mut() {
            sel.update(p, Side::Right);
            inner.generation += 1;
        }
    }

    pub fn select_clear(&self) {
        let mut inner = lock(&self.inner);
        if inner.term.selection.take().is_some() {
            inner.generation += 1;
        }
    }

    /// Seçili metin (kopyalama).
    pub fn selection_text(&self) -> Option<String> {
        lock(&self.inner).term.selection_to_string().filter(|s| !s.is_empty())
    }

    /// Kipler (tuş/fare/yapıştırma kodlaması için).
    pub fn mode(&self) -> ModeFlags {
        mode_flags(*lock(&self.inner).term.mode())
    }

    /// İçerik sayacı: her değişimde artar; UI son çizdiği değerle karşılaştırır.
    pub fn generation(&self) -> u64 {
        lock(&self.inner).generation
    }

    /// OSC 0/2 başlığı.
    pub fn title(&self) -> Option<String> {
        lock(&self.inner).title.clone()
    }

    /// Zil çaldı mı? Okununca sıfırlanır.
    pub fn take_bell(&self) -> bool {
        std::mem::take(&mut lock(&self.inner).bell)
    }

    /// Çocuk sürecin pid'i.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Süreç çalışıyor mu (pty'siz kipte her zaman `false`).
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Süreç bittiyse çıkış bilgisi.
    pub fn exit_status(&self) -> Option<ExitInfo> {
        lock(&self.inner).exit.clone()
    }

    /// Süreci sonlandırır (SIGHUP; `nvim jobstop` gibi).
    pub fn kill(&self) {
        if let Some(k) = &self.killer {
            let _ = lock(k).kill();
        }
    }

    /// Ön plandaki süreç grubunun lideri (kabuk kendisi boştayken kabuğun pid'i).
    pub fn foreground_pid(&self) -> Option<u32> {
        let m = self.master.as_ref()?;
        lock(m).process_group_leader().and_then(|p| u32::try_from(p).ok())
    }

    /// Ön planda kabuk dışında bir süreç çalışıyorsa adı (`ps -o comm=`), yoksa `None`.
    pub fn foreground_process_name(&self) -> Option<String> {
        let fg = self.foreground_pid()?;
        if Some(fg) == self.pid {
            return None;
        }
        process_name(fg)
    }

    /// Render'a hazır anlık görüntü (kısa kilit, yalnız görünür satırlar kopyalanır).
    pub fn snapshot(&self) -> GridSnapshot {
        let inner = lock(&self.inner);
        let term = &inner.term;
        let grid = term.grid();
        let rows = grid.screen_lines();
        let cols = grid.columns();
        let offset = grid.display_offset();
        let colors = term.colors();
        let selection = term.selection.as_ref().and_then(|s| s.to_range(term));
        let mut cells = Vec::with_capacity(rows);
        for r in 0..rows {
            let line = Line(r as i32 - offset as i32);
            let row = &grid[line];
            let mut out = Vec::with_capacity(cols);
            for c in 0..cols {
                let cell = &row[Column(c)];
                let flags = cell.flags;
                let point = Point::new(line, Column(c));
                out.push(GridCell {
                    ch: if flags.contains(Flags::WIDE_CHAR_SPACER) { ' ' } else { cell.c },
                    zerowidth: cell.zerowidth().map(|z| z.iter().collect()),
                    fg: map_color(cell.fg, colors),
                    bg: map_color(cell.bg, colors),
                    style: CellStyle {
                        bold: flags.contains(Flags::BOLD),
                        italic: flags.contains(Flags::ITALIC),
                        underline: flags.intersects(Flags::ALL_UNDERLINES),
                        inverse: flags.contains(Flags::INVERSE),
                        dim: flags.contains(Flags::DIM),
                        strikeout: flags.contains(Flags::STRIKEOUT),
                        hidden: flags.contains(Flags::HIDDEN),
                    },
                    wide: flags.contains(Flags::WIDE_CHAR),
                    wide_spacer: flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
                    selected: selection.as_ref().is_some_and(|s| s.contains(point)),
                });
            }
            cells.push(out);
        }
        let mode = *term.mode();
        let cursor_point = grid.cursor.point;
        let style = term.cursor_style();
        let view_row = cursor_point.line.0 + offset as i32;
        let visible = mode.contains(TermMode::SHOW_CURSOR) && view_row >= 0 && (view_row as usize) < rows;
        let shape = match style.shape {
            ansi::CursorShape::Underline => CursorShape::Underline,
            ansi::CursorShape::Beam => CursorShape::Beam,
            ansi::CursorShape::HollowBlock => CursorShape::HollowBlock,
            _ => CursorShape::Block,
        };
        GridSnapshot {
            cols,
            rows,
            cells,
            cursor: CursorView {
                row: view_row.max(0) as usize,
                col: cursor_point.column.0,
                shape,
                visible: visible && style.shape != ansi::CursorShape::Hidden,
            },
            display_offset: offset,
            history_size: grid.history_size(),
            mode: mode_flags(mode),
            title: inner.title.clone(),
            generation: inner.generation,
        }
    }
}

fn mode_flags(mode: TermMode) -> ModeFlags {
    ModeFlags {
        app_cursor: mode.contains(TermMode::APP_CURSOR),
        app_keypad: mode.contains(TermMode::APP_KEYPAD),
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        alt_screen: mode.contains(TermMode::ALT_SCREEN),
        mouse_click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
        mouse_drag: mode.contains(TermMode::MOUSE_DRAG),
        mouse_motion: mode.contains(TermMode::MOUSE_MOTION),
        sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
        focus_events: mode.contains(TermMode::FOCUS_IN_OUT),
        alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
    }
}

/// alacritty rengini UI rengine çevirir; uygulamanın OSC 4 ile değiştirdiği indeks varsa onun RGB'si.
fn map_color(color: Color, colors: &alacritty_terminal::term::color::Colors) -> TermColor {
    let indexed = |i: usize| match colors[i] {
        Some(Rgb { r, g, b }) => TermColor::Rgb(r, g, b),
        None => TermColor::Indexed(i as u8),
    };
    match color {
        Color::Spec(Rgb { r, g, b }) => TermColor::Rgb(r, g, b),
        Color::Indexed(i) => indexed(i as usize),
        Color::Named(named) => {
            let i = named as usize;
            if i < 16 {
                return indexed(i);
            }
            match named {
                NamedColor::Background => TermColor::DefaultBg,
                NamedColor::DimBlack => indexed(0),
                NamedColor::DimRed => indexed(1),
                NamedColor::DimGreen => indexed(2),
                NamedColor::DimYellow => indexed(3),
                NamedColor::DimBlue => indexed(4),
                NamedColor::DimMagenta => indexed(5),
                NamedColor::DimCyan => indexed(6),
                NamedColor::DimWhite => indexed(7),
                _ => TermColor::DefaultFg,
            }
        }
    }
}

/// Renk sorgusu (OSC 10/11/4 `?`) yanıtı için tema bilinmediğinde kullanılan koyu tema varsayılanı.
fn default_rgb(idx: usize) -> Rgb {
    match idx {
        256 => Rgb { r: 0xdc, g: 0xdc, b: 0xdc },
        257 => Rgb { r: 0x1e, g: 0x1f, b: 0x22 },
        _ => Rgb { r: 0x80, g: 0x80, b: 0x80 },
    }
}

/// Sürecin kısa adı (`ps -o comm=`, yolun son parçası).
pub fn process_name(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps").args(["-o", "comm=", "-p", &pid.to_string()]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return None;
    }
    let base = Path::new(&s).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or(s);
    Some(base.trim_start_matches('-').to_string())
}

/// LESS bir Claude Code oturumunun içinden başlatıldıysa devraldığı oturum işaretleri. Yeni oturumlara geçerse Claude
/// kendini alt oturum sayar ("Transcript saving is off — inherited CLAUDE_CODE_CHILD_SESSION marker").
fn inherited_claude_vars(keys: impl Iterator<Item = std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    const EXACT: [&str; 4] = ["CLAUDECODE", "CLAUDE_PID", "AI_AGENT", "CLAUDE_EFFORT"];
    const PREFIX: [&str; 2] = ["CLAUDE_CODE_SESSION", "CLAUDE_CODE_MESSAGING"];
    const MARKERS: [&str; 3] = ["CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_EXECPATH"];
    keys.filter(|k| {
        let k = k.to_string_lossy();
        EXACT.contains(&&*k) || MARKERS.contains(&&*k) || PREFIX.iter().any(|p| k.starts_with(p))
    })
    .collect()
}

#[cfg(test)]
mod inherited_env_tests {
    #[test]
    fn strips_only_session_markers() {
        let keys = [
            "CLAUDECODE",
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CODE_MESSAGING_TOKEN",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_USE_BEDROCK",
            "PATH",
        ];
        let got = super::inherited_claude_vars(keys.iter().map(Into::into));
        let got: Vec<_> = got.iter().map(|k| k.to_string_lossy().into_owned()).collect();
        assert_eq!(
            got,
            ["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_MESSAGING_TOKEN"]
        );
    }
}
