//! Cmd+F dosya içi arama çubuğu (VS Code gibi kod alanının sağ üstünde): yazdıkça arar, büyük/küçük harf, tam kelime
//! ve regex düğmeleri, `n / toplam` sayacı, Enter sonraki, Shift+Enter önceki, Esc kapatır. Vim ve modeless modda
//! aynı. Eşleşmeyi çubuk bulur (`next_match`); desen editörün arama durumuna da Vim sözdizimiyle yazılır
//! (`\v` + regex + `\C`/`\c`), böylece eşleşme vurgusu ve Vim'de `n` / `N` çubuk kapandıktan sonra da çalışır.

use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use less_theme::Palette;

use crate::fonts::icon;
use crate::widgets::{color, text_input, ui_font};

const WIDTH: f32 = 470.0;

/// Arama seçenekleri (proje aramasıyla ortak).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindOptions {
    pub case: bool,
    pub word: bool,
    pub regex: bool,
}

impl FindOptions {
    /// Sorgunun regex hali (`regex` kapalıysa kaçışlı düz metin, `word` ise `\b` sınırlı). Büyük/küçük harf ayrı.
    pub fn regex_source(&self, query: &str) -> String {
        let body = if self.regex { query.to_owned() } else { regex::escape(query) };
        if self.word { format!(r"\b(?:{body})\b") } else { body }
    }

    /// Sorgunun derlenmiş hali; regex geçersizse hata metni.
    pub fn compile(&self, query: &str) -> Result<regex::Regex, String> {
        regex::RegexBuilder::new(&self.regex_source(query))
            .case_insensitive(!self.case)
            .multi_line(true)
            .build()
            .map_err(|e| e.to_string().lines().last().unwrap_or("Invalid regex").trim().to_owned())
    }

    /// Editörün arama deseni (Vim sözdizimi): `\v` sonrası olduğu gibi regex, sonda `\C` / `\c`. Ters bölü `\x5c`
    /// yazılır ki `less-core`'un `\c` ayıklaması düz metindeki `\c`'yi bozmasın.
    pub fn vim_pattern(&self, query: &str) -> String {
        let mut body = self.regex_source(query);
        if !self.regex {
            body = body.replace(r"\\", r"\x5c");
        }
        format!(r"\v{body}{}", if self.case { r"\C" } else { r"\c" })
    }
}

/// Çubuğun bu karedeki isteği.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Keep,
    Close,
    /// Sorgu ya da seçenek değişti: başlangıç konumundan yeniden ara (boş sorgu = vurguyu kaldır).
    Changed,
    Next,
    Prev,
}

/// Dosya içi arama çubuğunun durumu; kapansa da sorgu ve seçenekler sonraki açılış için kalır.
#[derive(Default)]
pub struct FindBar {
    pub open: bool,
    pub query: String,
    pub opts: FindOptions,
    /// Yazarken aramanın başladığı karakter (çubuk açıldığında imleç; Enter ile ilerler).
    pub origin: usize,
    /// `(geçerli eşleşme 1 tabanlı, toplam)`; sorgu boşsa `None`.
    pub count: Option<(usize, usize)>,
    /// Geçersiz regex.
    pub error: Option<String>,
    focus: bool,
}

impl FindBar {
    /// Çubuğu açar (açıksa yalnız odaklar); `seed` seçili tek satırlık metin.
    pub fn open(&mut self, origin: usize, seed: Option<String>) {
        if let Some(s) = seed.filter(|s| !s.is_empty() && !s.contains('\n')) {
            self.query = if self.opts.regex { regex::escape(&s) } else { s };
        }
        self.open = true;
        self.origin = origin;
        self.focus = true;
    }

    /// Çubuğu `code` alanının sağ üstüne çizer.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, code: Rect) -> Outcome {
        let width = WIDTH.min(code.width() - 24.0).max(220.0);
        let at = pos2(code.right() - width - 22.0, code.top() + 6.0);
        let frame = egui::Frame::NONE
            .fill(color(p.surface))
            .stroke(Stroke::new(1.0, color(p.border)))
            .corner_radius(8.0)
            .inner_margin(egui::Margin::same(6))
            .shadow(egui::Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(60) });
        egui::Area::new(egui::Id::unique("find-in-file"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| frame.show(ui, |ui| self.contents(ui, p, width)).inner)
            .inner
    }

    fn contents(&mut self, ui: &mut Ui, p: &Palette, width: f32) -> Outcome {
        let mut out = Outcome::Keep;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let id = egui::Id::unique("find-in-file-query");
            let input_w = width - 3.0 * 28.0 - 3.0 * 26.0 - 70.0;
            let resp = text_input(ui, p, id, &mut self.query, "Find", None, input_w, std::mem::take(&mut self.focus));
            if self.error.is_some() {
                ui.painter().rect_stroke(
                    resp.rect.expand2(vec2(12.0, 4.0)),
                    7.0,
                    Stroke::new(1.0, color(p.error)),
                    egui::StrokeKind::Inside,
                );
            }
            if resp.changed() {
                out = Outcome::Changed;
            }
            let (enter, shift, esc) = ui.input(|i| {
                let enter = i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } => Some(modifiers.shift),
                    _ => None,
                });
                (enter.is_some(), enter.unwrap_or(false), i.key_pressed(egui::Key::Escape))
            });
            if enter && (resp.lost_focus() || resp.has_focus()) {
                out = if shift { Outcome::Prev } else { Outcome::Next };
                self.focus = true;
            } else if esc && (resp.lost_focus() || resp.has_focus()) {
                out = Outcome::Close;
            }
            for (label, tip, on) in [
                ("Aa", "Match case", &mut self.opts.case),
                ("ab", "Match whole word", &mut self.opts.word),
                (".*", "Use regular expression", &mut self.opts.regex),
            ] {
                if toggle(ui, p, label, *on).on_hover_text(tip).clicked() {
                    *on = !*on;
                    self.focus = true;
                    out = Outcome::Changed;
                }
            }
            let count = match (&self.error, self.count) {
                (Some(_), _) => "Invalid".to_owned(),
                (None, Some((_, 0))) => "No results".to_owned(),
                (None, Some((i, n))) if i > 0 => format!("{i} of {n}"),
                (None, Some((_, n))) => format!("? of {n}"),
                (None, None) => String::new(),
            };
            let (r, _) = ui.allocate_exact_size(vec2(70.0, 24.0), Sense::hover());
            let tint =
                if self.count == Some((0, 0)) || self.error.is_some() { color(p.error) } else { color(p.comment) };
            ui.painter().text(r.center(), Align2::CENTER_CENTER, count, ui_font(11.5), tint);
            if let Some(e) = &self.error {
                ui.interact(r, ui.scope_id().with("find-error"), Sense::hover()).on_hover_text(e);
            }
            for (glyph, tip, o) in [
                (icon::CARET_UP, "Previous match (Shift+Enter)", Outcome::Prev),
                (icon::CARET_DOWN, "Next match (Enter)", Outcome::Next),
                (icon::X, "Close (Esc)", Outcome::Close),
            ] {
                if icon_button(ui, p, glyph).on_hover_text(tip).clicked() {
                    out = o;
                }
            }
        });
        out
    }
}

/// Seçenek düğmesi (`Aa`, `ab`, `.*`); açıkken vurgulu. Proje aramasında da kullanılır.
pub fn toggle(ui: &mut Ui, p: &Palette, label: &str, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
    let fill = if on {
        color(p.accent).gamma_multiply(0.3)
    } else if resp.hovered() {
        color(p.bg_highlight)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 5.0, fill);
    if on {
        ui.painter().rect_stroke(rect, 5.0, Stroke::new(1.0, color(p.accent)), egui::StrokeKind::Inside);
    }
    let tint = if on { color(p.fg) } else { color(p.comment) };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, egui::FontId::monospace(12.0), tint);
    resp
}

fn icon_button(ui: &mut Ui, p: &Palette, glyph: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 5.0, color(p.bg_highlight));
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, ui_font(13.0), color(p.fg_dim));
    resp
}

/// `next_match` yönü.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// `from`da ya da sonrasında başlayan ilk eşleşme (yazarken).
    AtOrAfter,
    /// `from`dan sonra başlayan (Enter).
    After,
    /// `from`dan önce başlayan (Shift+Enter).
    Before,
}

/// Bayt `from`a göre eşleşme; yoksa dosyanın başına / sonuna sarar. Boş eşleşmeler atlanır.
pub fn next_match(re: &regex::Regex, text: &str, from: usize, dir: Dir) -> Option<(usize, usize)> {
    let all = re.find_iter(text).filter(|m| m.end() > m.start()).map(|m| (m.start(), m.end()));
    match dir {
        Dir::AtOrAfter => {
            let mut all = all.peekable();
            let first = all.peek().copied();
            all.find(|m| m.0 >= from).or(first)
        }
        Dir::After => {
            let mut all = all.peekable();
            let first = all.peek().copied();
            all.find(|m| m.0 > from).or(first)
        }
        Dir::Before => {
            let (mut before, mut last) = (None, None);
            for m in all {
                if m.0 < from {
                    before = Some(m);
                }
                last = Some(m);
            }
            before.or(last)
        }
    }
}

/// `text` içindeki eşleşme sayısı ve imlecin (bayt) üstünde başlayan eşleşmenin sırası (1 tabanlı, yoksa 0).
pub fn count_matches(re: &regex::Regex, text: &str, cursor_byte: usize) -> (usize, usize) {
    let mut current = 0;
    let mut total = 0;
    for m in re.find_iter(text).filter(|m| m.end() > m.start()) {
        total += 1;
        if m.start() == cursor_byte {
            current = total;
        }
    }
    (current, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_build_patterns() {
        let plain = FindOptions::default();
        assert_eq!(plain.regex_source("a.b"), r"a\.b");
        assert_eq!(plain.vim_pattern(r"a\c"), r"\va\x5cc\c");
        let word = FindOptions { word: true, case: true, regex: false };
        assert_eq!(word.vim_pattern("foo"), r"\v\b(?:foo)\b\C");
        let re = FindOptions { regex: true, ..FindOptions::default() };
        assert!(re.compile("fo+(").is_err());
        assert!(re.compile("FO+").unwrap().is_match("xfoo"), "case-insensitive by default");
        let re = less_core::search::compile(&plain.vim_pattern(r"a\c"), true, true).unwrap();
        assert!(re.is_match(r"xA\Cy"), "escaped backslash survives the \\c strip");
        let re = less_core::search::compile(&word.vim_pattern("foo"), true, true).unwrap();
        assert!(re.is_match("a foo b") && !re.is_match("food") && !re.is_match("FOO"));
    }

    #[test]
    fn next_match_wraps_in_both_directions() {
        let re = FindOptions::default().compile("ab").unwrap();
        let t = "ab xab ab";
        assert_eq!(next_match(&re, t, 0, Dir::AtOrAfter), Some((0, 2)));
        assert_eq!(next_match(&re, t, 0, Dir::After), Some((4, 6)));
        assert_eq!(next_match(&re, t, 7, Dir::After), Some((0, 2)), "wraps to the top");
        assert_eq!(next_match(&re, t, 0, Dir::Before), Some((7, 9)), "wraps to the bottom");
        assert_eq!(next_match(&re, "x", 0, Dir::After), None);
    }

    #[test]
    fn count_marks_match_under_cursor() {
        let re = FindOptions::default().compile("ab").unwrap();
        assert_eq!(count_matches(&re, "ab xab AB", 3), (0, 3));
        assert_eq!(count_matches(&re, "ab xab AB", 4), (2, 3));
    }
}
