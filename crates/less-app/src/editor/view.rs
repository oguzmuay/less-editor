//! Kod görünümünün çizimi: oluk (göreli satır numaraları #54), seçim, arama eşleşmeleri, vurgulu metin, imleçler,
//! sağda mini harita (VS Code gibi; `:Minimap`). Yalnız görünen satırlar çizilir; dikey kaydırma `editor.view.top`
//! (motor yönetir), yatay `EditorTab::left_col`. Mini haritanın sağında `RULER_W` genişliğinde boş şerit kalır:
//! `editor::lang`'in problem işaretleri (`geo.rect.right()`'a yaslı) oraya çizilir.

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, pos2, text::LayoutJob, vec2,
};
use less_core::command::CursorMode;
use less_core::editor::Session;
use less_theme::Palette;
use ropey::Rope;

use super::EditorTab;
use super::syntax::{char_at_col, display_cols, kind_color};
use crate::widgets::color;

pub const CODE_SIZE: f32 = 13.5;
/// Mini haritanın genişliği.
pub const MINIMAP_W: f32 = 96.0;
/// Mini haritanın sağında problem işaretlerine (lang) kalan şerit.
pub const RULER_W: f32 = 10.0;
/// Mini haritada satır yüksekliği ve karakter genişliği (piksel).
const MM_LINE: f32 = 2.0;
const MM_CHAR: f32 = 1.0;
/// Görünüm bundan darsa mini harita çizilmez.
const MINIMAP_MIN_VIEW: f32 = 480.0;

/// Bir karede kod görünümünün ölçüleri; `editor::lang` (B5) üstüne çizmek ve fareyi çevirmek için kullanır.
#[allow(dead_code, reason = "B5 editor::lang okur")]
#[derive(Clone, Copy, Debug)]
pub struct CodeGeometry {
    /// Görünümün tamamı.
    pub rect: Rect,
    /// Satır numarası oluğu (breakpoint / işaret sütunu dahil).
    pub gutter: Rect,
    /// Metin alanı.
    pub text: Rect,
    pub line_h: f32,
    pub char_w: f32,
    /// Görünen satırlar `first_line..last_line`.
    pub first_line: usize,
    pub last_line: usize,
    pub left_col: usize,
    pub tab_width: usize,
    /// Mini harita (kapalıysa ya da görünüm darsa `None`). `text.right()` mini haritanın soludur.
    pub minimap: Option<Minimap>,
}

/// Mini haritanın yeri ve ilk satırı.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Minimap {
    pub rect: Rect,
    /// Mini haritanın en üstündeki satır.
    pub first: usize,
}

impl Minimap {
    /// `y` noktasındaki satır.
    pub fn line_at(&self, y: f32) -> usize {
        self.first + ((y - self.rect.top()) / MM_LINE).max(0.0) as usize
    }
}

/// Mini haritanın ilk satırı: belge sığıyorsa 0, sığmıyorsa görünümle orantılı kayar (VS Code gibi).
pub fn minimap_first(total: usize, view_top: usize, view_rows: usize, capacity: usize) -> usize {
    if total <= capacity {
        return 0;
    }
    let ratio = (view_top as f32 / total.saturating_sub(view_rows).max(1) as f32).min(1.0);
    (ratio * (total - capacity) as f32).round() as usize
}

impl CodeGeometry {
    /// Satırın üst kenarı (görünmüyorsa `None`).
    pub fn line_y(&self, line: usize) -> Option<f32> {
        (self.first_line..self.last_line)
            .contains(&line)
            .then(|| self.text.top() + (line - self.first_line) as f32 * self.line_h)
    }

    /// Karakterin hücresinin sol üst köşesi.
    pub fn pos_of(&self, rope: &Rope, char_idx: usize) -> Option<Pos2> {
        let char_idx = char_idx.min(rope.len_chars());
        let line = rope.char_to_line(char_idx);
        let y = self.line_y(line)?;
        let text = line_text(rope, line);
        let cols = display_cols(&text, self.tab_width);
        let col = cols[(char_idx - rope.line_to_char(line)).min(cols.len() - 1)];
        Some(pos2(self.text.left() + (col as f32 - self.left_col as f32) * self.char_w, y))
    }

    /// Noktaya en yakın karakter sınırı (fare tıklaması). Görünümün dışında `None`.
    pub fn char_at(&self, rope: &Rope, pos: Pos2) -> Option<usize> {
        if !self.rect.contains(pos) {
            return None;
        }
        let row = ((pos.y - self.text.top()) / self.line_h).floor().max(0.0) as usize;
        let line = (self.first_line + row).min(rope.len_lines().saturating_sub(1));
        let x = ((pos.x - self.text.left()) / self.char_w + 0.5).floor().max(0.0) as usize + self.left_col;
        let cols = display_cols(&line_text(rope, line), self.tab_width);
        Some(rope.line_to_char(line) + char_at_col(&cols, x))
    }
}

/// Satır metni, satır sonu olmadan.
pub fn line_text(rope: &Rope, line: usize) -> String {
    let mut s = rope.line(line).to_string();
    while s.ends_with('\n') || s.ends_with('\r') {
        s.pop();
    }
    s
}

/// Hücre genişliği ve satır yüksekliği.
pub fn cell(ui: &Ui) -> (f32, f32) {
    let font = FontId::monospace(CODE_SIZE);
    (ui.fonts_mut(|f| f.glyph_width(&font, 'M')), (CODE_SIZE * 1.55).round())
}

/// `height` piksele sığan satır sayısı (`editor.view.height`).
pub fn rows_for(height: f32, line_h: f32) -> usize {
    ((height / line_h).floor() as usize).max(1)
}

/// Yatay kaydırmayı imleç görünür kalacak şekilde düzeltir.
pub fn keep_cursor_visible(left_col: usize, cursor_col: usize, text_cols: usize) -> usize {
    let margin = 4.min(text_cols / 4);
    if cursor_col < left_col + margin {
        cursor_col.saturating_sub(margin)
    } else if cursor_col + margin + 1 > left_col + text_cols {
        cursor_col + margin + 1 - text_cols
    } else {
        left_col
    }
}

/// Çizim seçenekleri.
#[derive(Clone, Copy, Debug)]
pub struct ViewOptions {
    pub numbers: bool,
    pub relative_numbers: bool,
    pub focused: bool,
}

/// Kod görünümünü mini haritasız çizer (`show_with(.., false)`).
#[allow(dead_code, reason = "editor::lang testleri kullanır")]
pub fn show(
    ui: &mut Ui,
    p: &Palette,
    tab: &mut EditorTab,
    session: &Session,
    opts: ViewOptions,
) -> (Response, CodeGeometry) {
    show_with(ui, p, tab, session, opts, false)
}

/// Kod görünümünü `ui.max_rect()` içine çizer; fare olayları dönen yanıtta. `minimap` sağda mini harita ister.
pub fn show_with(
    ui: &mut Ui,
    p: &Palette,
    tab: &mut EditorTab,
    session: &Session,
    opts: ViewOptions,
    minimap: bool,
) -> (Response, CodeGeometry) {
    let rect = ui.max_rect();
    let resp = ui.interact(rect, ui.scope_id().with("code-view"), Sense::click_and_drag());
    let (char_w, line_h) = cell(ui);
    let font = FontId::monospace(CODE_SIZE);
    let rope = tab.editor.buffer.rope().clone();
    let tab_width = tab.editor.buffer.tab_width;
    let line_count = rope.len_lines().max(1);
    let digits = line_count.to_string().len().max(3);
    let gutter_w = digits as f32 * char_w + 30.0;
    let gutter = Rect::from_min_max(rect.min, pos2(rect.left() + gutter_w, rect.bottom()));
    let first = tab.editor.view.top.min(line_count - 1);
    let view_rows = rows_for(rect.height(), line_h);
    let last = (first + view_rows + 1).min(line_count);
    let minimap = (minimap && rect.width() >= MINIMAP_MIN_VIEW).then(|| {
        let r = Rect::from_min_max(
            pos2(rect.right() - RULER_W - MINIMAP_W, rect.top()),
            pos2(rect.right() - RULER_W, rect.bottom()),
        );
        let capacity = (r.height() / MM_LINE) as usize;
        Minimap { rect: r, first: minimap_first(line_count, first, view_rows, capacity) }
    });
    let text_right = minimap.map_or(rect.right(), |m| m.rect.left());
    let text = Rect::from_min_max(pos2(gutter.right() + 6.0, rect.top()), pos2(text_right, rect.bottom()));

    let (cur_line, cur_col) = tab.editor.cursor_line_col();
    let cur_cols = display_cols(&line_text(&rope, cur_line), tab_width);
    let cursor_col = cur_cols[cur_col.min(cur_cols.len() - 1)];
    let text_cols = ((text.width() / char_w).floor() as usize).max(1);
    if tab.cursor_seen != Some(tab.editor.cursor()) {
        tab.cursor_seen = Some(tab.editor.cursor());
        tab.left_col = keep_cursor_visible(tab.left_col, cursor_col, text_cols);
    }
    let geo = CodeGeometry {
        rect,
        gutter,
        text,
        line_h,
        char_w,
        first_line: first,
        last_line: last,
        left_col: tab.left_col,
        tab_width,
        minimap,
    };

    let painter = ui.painter_at(Rect::from_min_max(rect.min, pos2(text_right, rect.bottom())));
    let spans = tab.editor.selection_spans();
    let matches = tab.editor.search_matches(session, first, last.saturating_sub(1));
    let highlights = tab.highlights(first..last).to_vec();
    let x_of = |col: usize| text.left() + (col as f32 - geo.left_col as f32) * char_w;

    for (row, line) in (first..last).enumerate() {
        let y = rect.top() + row as f32 * line_h;
        let line_rect = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), line_h));
        let s = line_text(&rope, line);
        let cols = display_cols(&s, tab_width);
        let start = rope.line_to_char(line);
        let n = cols.len() - 1;
        if line == cur_line && spans.is_empty() {
            painter.rect_filled(line_rect, 0.0, color(p.bg_highlight).gamma_multiply(0.45));
        }
        if let Some((mark, ranges)) = &tab.mark
            && *mark == line
        {
            painter.rect_filled(line_rect, 0.0, color(p.bg_highlight));
            let byte_char = |b: usize| s.char_indices().position(|(i, _)| i >= b).unwrap_or(n);
            for &(a, b) in ranges {
                let r = Rect::from_min_max(
                    pos2(x_of(cols[byte_char(a)]), y + 2.0),
                    pos2(x_of(cols[byte_char(b)]), y + line_h - 2.0),
                );
                painter.rect_filled(r, 3.0, color(p.accent).gamma_multiply(0.35));
            }
        }
        let paint_range = |a: usize, b: usize, fill: Color32| {
            let (lo, hi) = (a.max(start), b.min(start + n + 1));
            if lo >= hi && !(a <= start && b > start) {
                return;
            }
            let c0 = cols[(lo - start).min(n)];
            let c1 = if hi > start + n { cols[n] + 1 } else { cols[(hi - start).min(n)] };
            let r = Rect::from_min_max(pos2(x_of(c0), y), pos2(x_of(c1.max(c0)), y + line_h));
            painter.rect_filled(r, 0.0, fill);
        };
        for &(a, b) in &matches {
            paint_range(a, b, color(p.extra.warn).gamma_multiply(0.3));
        }
        for &(a, b) in &spans {
            paint_range(a, b, color(p.accent).gamma_multiply(0.3));
        }
        let job = line_job(&s, highlights.get(row).map(Vec::as_slice).unwrap_or_default(), tab_width, &font, p);
        let galley = painter.layout_job(job);
        painter.galley(pos2(x_of(0), y + (line_h - galley.size().y) / 2.0), galley, color(p.fg));

        if !opts.numbers && !opts.relative_numbers {
            continue;
        }
        let num = if opts.relative_numbers && line != cur_line { line.abs_diff(cur_line) } else { line + 1 };
        let num_color = if line == cur_line { color(p.fg) } else { color(p.comment) };
        painter.text(
            pos2(gutter.right() - 14.0, y + line_h / 2.0),
            Align2::RIGHT_CENTER,
            num.to_string(),
            font.clone(),
            num_color,
        );
    }
    painter.vline(gutter.right(), rect.y_range(), Stroke::new(1.0, color(p.border).gamma_multiply(0.6)));

    let block = tab.editor.cursor_mode() == CursorMode::Block;
    let tint = if opts.focused { color(p.accent) } else { color(p.comment) };
    for range in tab.editor.selection().ranges() {
        let Some(at) = geo.pos_of(&rope, range.head) else { continue };
        if at.x < text.left() - 0.5 || at.x > text.right() {
            continue;
        }
        if block {
            let r = Rect::from_min_size(at, vec2(char_w, line_h));
            if opts.focused {
                painter.rect_filled(r, 1.0, tint.gamma_multiply(0.75));
            } else {
                painter.rect_stroke(r, 1.0, Stroke::new(1.0, tint), egui::StrokeKind::Inside);
            }
        } else {
            painter.vline(at.x, at.y + 1.0..=at.y + line_h - 1.0, Stroke::new(2.0, tint));
        }
    }
    if let Some(mm) = minimap {
        paint_minimap(ui, p, tab, &rope, mm, first..last.max(first + view_rows).min(line_count));
    }
    (resp, geo)
}

/// Mini harita: satır başına `MM_LINE` piksel, karakter başına `MM_CHAR`; boşluk dışı her renk koşusu bir çubuk.
/// Görünen bölge hafif dolgulu kutu.
fn paint_minimap(ui: &Ui, p: &Palette, tab: &mut EditorTab, rope: &Rope, mm: Minimap, visible: std::ops::Range<usize>) {
    let painter = ui.painter_at(mm.rect);
    painter.rect_filled(mm.rect, 0.0, color(p.bg));
    let rows = ((mm.rect.height() / MM_LINE) as usize).min(rope.len_lines().saturating_sub(mm.first));
    let range = mm.first..mm.first + rows;
    let highlights = tab.minimap_highlights(range.clone()).to_vec();
    let tab_width = tab.editor.buffer.tab_width.max(1);
    let max_cols = ((mm.rect.width() - 6.0) / MM_CHAR) as usize;
    let x0 = mm.rect.left() + 4.0;
    for (row, line) in range.enumerate() {
        let y = mm.rect.top() + row as f32 * MM_LINE;
        let text = line_text(rope, line);
        let spans = highlights.get(row).map(Vec::as_slice).unwrap_or_default();
        let mut col = 0;
        let mut run: Option<(usize, usize, Color32)> = None;
        let flush = |run: &mut Option<(usize, usize, Color32)>| {
            if let Some((a, b, c)) = run.take() {
                let r = Rect::from_min_max(
                    pos2(x0 + a as f32 * MM_CHAR, y),
                    pos2(x0 + b as f32 * MM_CHAR, y + MM_LINE - 0.5),
                );
                painter.rect_filled(r, 0.0, c.gamma_multiply(0.7));
            }
        };
        for (byte, c) in text.char_indices() {
            if col >= max_cols {
                break;
            }
            let w = if c == '\t' { tab_width - col % tab_width } else { 1 };
            if c.is_whitespace() {
                flush(&mut run);
            } else {
                let tint = spans
                    .iter()
                    .rev()
                    .find(|s| s.start <= byte && byte < s.end)
                    .map_or(color(p.fg_dim), |s| color(kind_color(p, s.kind)));
                match &mut run {
                    Some((_, b, t)) if *t == tint && *b == col => *b = col + w,
                    _ => {
                        flush(&mut run);
                        run = Some((col, col + w, tint));
                    }
                }
            }
            col += w;
        }
        flush(&mut run);
    }
    if visible.start >= mm.first {
        let top = mm.rect.top() + (visible.start - mm.first) as f32 * MM_LINE;
        let h = visible.len() as f32 * MM_LINE;
        let boxed = Rect::from_min_size(pos2(mm.rect.left(), top), vec2(mm.rect.width(), h.max(MM_LINE * 2.0)));
        let hovered = ui.rect_contains_pointer(mm.rect);
        painter.rect_filled(boxed, 0.0, color(p.fg).gamma_multiply(if hovered { 0.12 } else { 0.07 }));
    }
    painter.vline(mm.rect.left(), mm.rect.y_range(), Stroke::new(1.0, color(p.border).gamma_multiply(0.4)));
}

/// Satırı renkli parçalara böler; sekmeler boşluğa genişler. `spans` satır içi bayt aralıklarıdır.
pub(crate) fn line_job(
    line: &str,
    spans: &[less_lang::HighlightSpan],
    tab_width: usize,
    font: &FontId,
    p: &Palette,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut run = String::new();
    let mut run_color = color(p.fg);
    let mut col = 0;
    let tw = tab_width.max(1);
    for (byte, c) in line.char_indices() {
        let c_color = spans
            .iter()
            .rev()
            .find(|s| s.start <= byte && byte < s.end)
            .map_or(color(p.fg), |s| color(kind_color(p, s.kind)));
        if c_color != run_color && !run.is_empty() {
            job.append(&std::mem::take(&mut run), 0.0, egui::TextFormat::simple(font.clone(), run_color));
        }
        run_color = c_color;
        if c == '\t' {
            let w = tw - col % tw;
            run.extend(std::iter::repeat_n(' ', w));
            col += w;
        } else {
            run.push(c);
            col += 1;
        }
    }
    if !run.is_empty() {
        job.append(&run, 0.0, egui::TextFormat::simple(font.clone(), run_color));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_scroll_follows_cursor() {
        assert_eq!(keep_cursor_visible(0, 10, 80), 0);
        assert_eq!(keep_cursor_visible(0, 100, 80), 25);
        assert_eq!(keep_cursor_visible(50, 10, 80), 6);
    }

    #[test]
    fn geometry_maps_points_and_chars() {
        let rope = Rope::from_str("a\tb\nxyz");
        let geo = CodeGeometry {
            rect: Rect::from_min_size(Pos2::ZERO, vec2(400.0, 100.0)),
            gutter: Rect::from_min_size(Pos2::ZERO, vec2(40.0, 100.0)),
            text: Rect::from_min_max(pos2(40.0, 0.0), pos2(400.0, 100.0)),
            line_h: 20.0,
            char_w: 10.0,
            first_line: 0,
            last_line: 2,
            left_col: 0,
            tab_width: 4,
            minimap: None,
        };
        assert_eq!(geo.pos_of(&rope, 2), Some(pos2(80.0, 0.0)));
        assert_eq!(geo.char_at(&rope, pos2(81.0, 5.0)), Some(2));
        assert_eq!(geo.char_at(&rope, pos2(54.0, 25.0)), Some(5));
        assert_eq!(geo.char_at(&rope, pos2(399.0, 25.0)), Some(7));
        assert_eq!(geo.line_y(2), None);
    }

    #[test]
    fn minimap_tracks_view_proportionally() {
        assert_eq!(minimap_first(100, 50, 40, 300), 0, "short file fits");
        assert_eq!(minimap_first(1000, 0, 40, 300), 0);
        assert_eq!(minimap_first(1000, 960, 40, 300), 700, "bottom of file = bottom of minimap");
        assert_eq!(minimap_first(1000, 480, 40, 300), 350);
        let mm = Minimap { rect: Rect::from_min_size(pos2(0.0, 10.0), vec2(96.0, 600.0)), first: 350 };
        assert_eq!(mm.line_at(10.0), 350);
        assert_eq!(mm.line_at(30.0), 360);
    }

    #[test]
    fn minimap_narrows_text_and_leaves_ruler_for_marks() {
        let ctx = egui::Context::default();
        let mut tab = EditorTab::from_text(&"fn main() {}\n".repeat(500), Some(less_lang::Language::Rust), false);
        let session = Session::new(Box::new(less_core::registers::MemoryClipboard::default()));
        let opts = ViewOptions { numbers: true, relative_numbers: false, focused: true };
        let mut geos = Vec::new();
        for mm in [true, false] {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                geos.push(show_with(ui, &less_theme::TOKYONIGHT_NIGHT, &mut tab, &session, opts, mm).1)
            });
            out.textures_delta.clear();
        }
        let m = geos[0].minimap.expect("minimap on");
        assert_eq!(geos[0].text.right(), m.rect.left());
        assert_eq!(m.rect.right(), geos[0].rect.right() - RULER_W);
        assert!(geos[1].minimap.is_none());
        assert_eq!(geos[1].text.right(), geos[1].rect.right());
    }

    #[test]
    fn line_job_colors_runs_and_expands_tabs() {
        let p = less_theme::TOKYONIGHT_NIGHT;
        let spans = [less_lang::HighlightSpan { start: 0, end: 2, kind: less_lang::HighlightKind::Keyword }];
        let job = line_job("if\tx", &spans, 4, &FontId::monospace(12.0), &p);
        assert_eq!(job.text, "if  x");
        assert_eq!(job.sections.len(), 2);
        assert_eq!(job.sections[0].format.color, color(p.syntax.keyword));
    }
}
