//! Panel sözleşmesi ve kayıt listesi. Sözleşmenin anlatımı: `less-rs/docs/handoff/B0.md`.
//!
//! Her panel kendi klasöründe (`git/`, `run/`, `ai/`, `db/`) `pub fn panels() -> Vec<Box<dyn Panel>>` verir;
//! host (`project.rs`) bunları [`all`] ile toplar. Panel host'a doğrudan dokunmaz: [`Cx`] üzerinden okur ve istek
//! (`Request`) bırakır, host istekleri çağrı bitince uygular.

pub mod ai;
pub mod db;
pub mod git;
pub mod options;
pub mod run;

use std::path::{Path, PathBuf};

use eframe::egui;
use less_input::{EditMode, KeyEvent};
use less_lang::Language;
use less_theme::Palette;
pub use less_workbench::layout::{Effect, Panel as Area, StripPart};

use crate::editor::EditorTab;

/// Kayıtlı tüm paneller. Yeni panel = kendi klasöründeki `panels()` listesine bir satır.
pub fn all() -> Vec<Box<dyn Panel>> {
    let mut v = Vec::new();
    v.extend(git::panels());
    v.extend(run::panels());
    v.extend(ai::panels());
    v.extend(db::panels());
    v.extend(options::panels());
    v
}

/// Ekranda bir panelin çizebileceği bölge.
///
/// - `Area(Git)`: sol sütun (Files host'undur). `Area(Ai)`, `Area(Terminal)`, `Area(Database)`: sağ sütun.
/// - `Strip(..)`: alt şeridin parçaları (Run: `RunTree`/`RunLog`/`Variables`, Git History: `Branches`/`History`).
///   `Messages` host'undur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Area(Area),
    Strip(StripPart),
}

impl Region {
    /// Bölgenin ait olduğu yerleşim paneli (odak ve Cmd+N tuşu bu düzeyde).
    pub fn area(self) -> Area {
        match self {
            Region::Area(a) => a,
            Region::Strip(StripPart::RunTree | StripPart::RunLog | StripPart::Variables) => Area::Run,
            Region::Strip(StripPart::Branches | StripPart::History) => Area::GitHistory,
            Region::Strip(StripPart::Messages) => Area::Messages,
        }
    }
}

/// Host'tan panele giden olay. `event` / `tab_event` `true` dönerse olay tüketilmiştir.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// Odaktaki panele (ya da etkin özel tab'a) basılan tuş. Panel kullanmazsa uygulama kısayoluna bakılır.
    Key(KeyEvent),
    /// Hiç kimsenin kullanmadığı tuş (F7–F9, Cmd+F8, Cmd+Enter, Cmd+D…): paneller sırayla denenir.
    Shortcut(KeyEvent),
    /// Host'un bilmediği ex komutu, `:` olmadan (`"GitBlame"`, `"R my name"`).
    Ex(&'a str),
    /// Vim `Space` kısayolu, `Space` sonrası tuşlar (`"j"`, `"du"`, `"D"`, `"ga"`).
    Leader(&'a str),
    /// Vim motorunun bilmediği dizi (`"gd"`, `"K"`); önce `editor::lang` görür.
    Unmapped(&'a str),
    /// Yerleşim değişti (Cmd+N, `:Run`, Database'in AI'ı gizlemesi…); tüm panellere gider.
    Layout(Effect),
    /// Kod alanında bir dosya kaydedildi; tüm panellere gider.
    Saved(&'a Path),
    /// Paneller arası mesaj (`Cx::send`); tüm panellere gider. Konu adı `"<panel id>.<ad>"` biçiminde.
    Message { topic: &'a str, data: &'a str },
}

/// Panel kontratı. Yalnız `id`, `regions` ve `ui` zorunlu.
pub trait Panel {
    /// Kalıcı, benzersiz kimlik (`"git"`, `"run"`, `"ai"`, `"db"`); özel tab sahipliği bununla.
    fn id(&self) -> &'static str;

    /// Çizdiği bölgeler. Aynı bölgeyi iki panel istemez; ilk kayıtlı kazanır.
    fn regions(&self) -> &[Region];

    /// Her karede, çizimden önce (panel görünmese de): süreç / kanal yoklama. Arka plan işi bitince
    /// `cx.ctx.request_repaint()` ile kare istenir.
    fn tick(&mut self, _cx: &mut Cx) {}

    /// Bölgeyi çizer; `ui.max_rect()` bölgenin tamamı (kırpılmış). `cx.focused` bölgenin paneli odakta mı.
    fn ui(&mut self, region: Region, ui: &mut egui::Ui, cx: &mut Cx);

    /// Olay; `true` = tüketildi.
    fn event(&mut self, _event: &Event, _cx: &mut Cx) -> bool {
        false
    }

    /// Bu panelin açtığı özel kod tab'ını çizer (`Cx::open_tab`).
    fn tab_ui(&mut self, _key: &str, _ui: &mut egui::Ui, _cx: &mut Cx) {}

    /// Özel tab etkin ve kod alanı odaktayken tuş (`Event::Key`) vb.
    fn tab_event(&mut self, _key: &str, _event: &Event, _cx: &mut Cx) -> bool {
        false
    }

    /// Özel tab kapandı (Cmd+W, ✕).
    fn tab_closed(&mut self, _key: &str, _cx: &mut Cx) {}

    /// Pencere kapanmadan önce (`:qa`, Cmd+Q). `false` dönen panel kapanışı erteler; işi bitince
    /// `cx.quit()` ile yeniden ister (#80 Run süreçlerinin kapatılması).
    fn can_quit(&mut self, _cx: &mut Cx) -> bool {
        true
    }
}

/// Panelin host'tan istediği işler; çağrı bitince sırayla uygulanır.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    OpenFile { path: PathBuf, line: Option<usize> },
    OpenText { title: String, text: String, language: Option<Language> },
    OpenTab { owner: &'static str, key: String, title: String },
    CloseTab { owner: &'static str, key: String },
    Layout(LayoutOp),
    Notify { text: String, error: bool },
    Send { topic: String, data: String },
    Quit,
}

/// Yerleşim isteği (`less-workbench` `Layout` çağrıları).
#[allow(dead_code, reason = "B1–B5 panelleri kullanır")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutOp {
    Open(Area),
    Close(Area),
    Toggle(Area),
    /// Cmd+N davranışı (#5).
    Press(Area),
    /// Açık paneli odaklar (`Area::Code` = koda dön).
    Focus(Area),
    /// Debug durdu: Variables'ı göster (#93).
    ShowVariables,
    HideVariables,
}

/// Panel çağrısının bağlamı: okunacak durum + istek kuyruğu.
#[allow(dead_code, reason = "B1–B5 panelleri okur")]
pub struct Cx<'a> {
    pub ctx: &'a egui::Context,
    pub palette: &'a Palette,
    /// Proje kökleri (workspace'te birden çok).
    pub roots: &'a [PathBuf],
    pub edit_mode: EditMode,
    /// Çağrılan bölgenin / panelin odakta olup olmadığı.
    pub focused: bool,
    /// Kod alanındaki etkin dosya tab'ı (yol, metin, imleç, seçim).
    pub active: Option<&'a EditorTab>,
    /// Çağrılan panelin kimliği (özel tab sahipliği için host doldurur).
    pub owner: &'static str,
    pub requests: &'a mut Vec<Request>,
}

#[allow(dead_code, reason = "B1–B5 panelleri kullanır")]
impl Cx<'_> {
    /// Dosyayı kod alanında açar (varsa o tab'a geçer), `line` 0 tabanlı; kod alanı odak alır.
    pub fn open_file(&mut self, path: impl Into<PathBuf>, line: Option<usize>) {
        self.requests.push(Request::OpenFile { path: path.into(), line });
    }

    /// Salt okunur metin tab'ı (vurgulamalı); aynı başlıkla yeniden açılınca içerik yenilenir.
    pub fn open_text(&mut self, title: impl Into<String>, text: impl Into<String>, language: Option<Language>) {
        self.requests.push(Request::OpenText { title: title.into(), text: text.into(), language });
    }

    /// Bu panelin çizdiği özel tab'ı açar / öne getirir (`Panel::tab_ui(key)`); aynı anahtarla başlık güncellenir.
    pub fn open_tab(&mut self, key: impl Into<String>, title: impl Into<String>) {
        self.requests.push(Request::OpenTab { owner: self.owner, key: key.into(), title: title.into() });
    }

    pub fn close_tab(&mut self, key: impl Into<String>) {
        self.requests.push(Request::CloseTab { owner: self.owner, key: key.into() });
    }

    pub fn layout(&mut self, op: LayoutOp) {
        self.requests.push(Request::Layout(op));
    }

    /// Paneli odaklar (kapalıysa açar).
    pub fn focus(&mut self, area: Area) {
        self.layout(LayoutOp::Open(area));
    }

    /// Kısa bildirim (durum satırı + balon).
    pub fn info(&mut self, text: impl Into<String>) {
        self.requests.push(Request::Notify { text: text.into(), error: false });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.requests.push(Request::Notify { text: text.into(), error: true });
    }

    /// Paneller arası mesaj: tüm panellere `Event::Message` olarak gider (gönderen dahil).
    pub fn send(&mut self, topic: impl Into<String>, data: impl Into<String>) {
        self.requests.push(Request::Send { topic: topic.into(), data: data.into() });
    }

    /// Ertelenen kapanışı yeniden ister (`Panel::can_quit`).
    pub fn quit(&mut self) {
        self.requests.push(Request::Quit);
    }
}

/// Panel testleri için bağlam üretici (GUI açmadan `Panel::event` / `tick` çağırmak için).
#[cfg(test)]
pub mod testing {
    use super::*;

    pub struct Harness {
        pub ctx: egui::Context,
        pub palette: Palette,
        pub roots: Vec<PathBuf>,
        pub requests: Vec<Request>,
    }

    impl Harness {
        pub fn new(roots: Vec<PathBuf>) -> Self {
            Self { ctx: egui::Context::default(), palette: less_theme::TOKYONIGHT_NIGHT, roots, requests: Vec::new() }
        }

        /// `owner` kimliğiyle, odakta bir bağlam.
        pub fn cx(&mut self, owner: &'static str) -> Cx<'_> {
            Cx {
                ctx: &self.ctx,
                palette: &self.palette,
                roots: &self.roots,
                edit_mode: EditMode::Vim,
                focused: true,
                active: None,
                owner,
                requests: &mut self.requests,
            }
        }
    }

    #[test]
    fn cx_records_requests_with_owner() {
        let mut h = Harness::new(vec![PathBuf::from("/tmp")]);
        let mut cx = h.cx("git");
        cx.open_tab("diff:a.rs", "a.rs (diff)");
        cx.focus(Area::Git);
        assert_eq!(
            h.requests,
            vec![
                Request::OpenTab { owner: "git", key: "diff:a.rs".into(), title: "a.rs (diff)".into() },
                Request::Layout(LayoutOp::Open(Area::Git)),
            ]
        );
    }

    #[test]
    fn strip_parts_belong_to_their_panel_93() {
        assert_eq!(Region::Strip(StripPart::Variables).area(), Area::Run);
        assert_eq!(Region::Strip(StripPart::Branches).area(), Area::GitHistory);
        assert_eq!(Region::Area(Area::Database).area(), Area::Database);
    }
}
