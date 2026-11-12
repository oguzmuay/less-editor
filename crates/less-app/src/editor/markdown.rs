//! Markdown önizlemesi (#83): `pulldown-cmark` olaylarından küçük bir blok ağacı kurulur, egui ile çizilir.
//! Başlık, paragraf, vurgu, satır içi kod, kod bloğu, alıntı, liste (görev kutuları dahil), tablo, çizgi, bağlantı.

use eframe::egui::{self, Align, Color32, FontId, Label, Layout, RichText, Sense, Stroke, Ui};
use less_theme::Palette;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::widgets::color;

/// Satır içi metin parçası ve stili.
#[derive(Clone, Debug, Default, PartialEq)]
struct Inline {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    link: Option<String>,
}

/// SVG kaynağını sığacak boyutta çizer; kaynak değişince adres de değişir, böylece önbellek yenilenir.
pub fn show_svg(ui: &mut Ui, text: &str) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    let uri = format!("bytes://preview-{:x}.svg", hasher.finish());
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        ui.add(
            egui::Image::from_bytes(uri, text.as_bytes().to_vec())
                .fit_to_original_size(1.0)
                .max_size(ui.available_size()),
        );
    });
}

/// Önizleme bloğu.
#[derive(Clone, Debug, PartialEq)]
enum Block {
    Para(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    Code(String),
    Quote(Vec<Block>),
    List(Option<u64>, Vec<Vec<Block>>),
    Table(Vec<Vec<Vec<Inline>>>),
    Rule,
}

/// Açık kapsayıcı: kök, alıntı, liste ya da liste öğesi.
enum Frame {
    Root(Vec<Block>),
    Quote(Vec<Block>),
    List(Option<u64>, Vec<Vec<Block>>),
    Item(Vec<Block>),
}

impl Frame {
    fn blocks(&mut self) -> &mut Vec<Block> {
        match self {
            Frame::Root(b) | Frame::Quote(b) | Frame::Item(b) => b,
            // Liste doğrudan blok almaz; olmayan öğeye düşen içerik için yeni öğe açılır.
            Frame::List(_, items) => {
                if items.is_empty() {
                    items.push(Vec::new());
                }
                items.last_mut().expect("item")
            }
        }
    }
}

/// Markdown metnini blok ağacına çevirir.
fn parse(text: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut stack = vec![Frame::Root(Vec::new())];
    let mut inl: Option<Vec<Inline>> = None;
    let mut heading: Option<u8> = None;
    let mut code: Option<String> = None;
    let mut table: Option<Vec<Vec<Vec<Inline>>>> = None;
    let (mut bold, mut italic, mut strike) = (0u32, 0u32, 0u32);
    let mut link: Option<String> = None;

    fn flush(stack: &mut [Frame], inl: &mut Option<Vec<Inline>>) {
        if let Some(runs) = inl.take().filter(|r| !r.is_empty()) {
            stack.last_mut().expect("frame").blocks().push(Block::Para(runs));
        }
    }

    for ev in Parser::new_ext(text, opts) {
        let push_text = |t: String, is_code: bool, inl: &mut Option<Vec<Inline>>| {
            inl.get_or_insert_with(Vec::new).push(Inline {
                text: t,
                bold: bold > 0 || heading.is_some(),
                italic: italic > 0,
                code: is_code,
                strike: strike > 0,
                link: link.clone(),
            });
        };
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    if table.is_none() {
                        flush(&mut stack, &mut inl);
                    }
                }
                Tag::Heading { level, .. } => {
                    flush(&mut stack, &mut inl);
                    heading = Some(level as u8);
                }
                Tag::CodeBlock(_) => {
                    flush(&mut stack, &mut inl);
                    code = Some(String::new());
                }
                Tag::BlockQuote(_) => {
                    flush(&mut stack, &mut inl);
                    stack.push(Frame::Quote(Vec::new()));
                }
                Tag::List(start) => {
                    flush(&mut stack, &mut inl);
                    stack.push(Frame::List(start, Vec::new()));
                }
                Tag::Item => {
                    flush(&mut stack, &mut inl);
                    stack.push(Frame::Item(Vec::new()));
                }
                Tag::Table(_) => {
                    flush(&mut stack, &mut inl);
                    table = Some(Vec::new());
                }
                Tag::TableHead | Tag::TableRow => {
                    if let Some(t) = table.as_mut() {
                        t.push(Vec::new());
                    }
                }
                Tag::TableCell => inl = Some(Vec::new()),
                Tag::Emphasis => italic += 1,
                Tag::Strong => bold += 1,
                Tag::Strikethrough => strike += 1,
                Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => link = Some(dest_url.to_string()),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if table.is_none() {
                        flush(&mut stack, &mut inl);
                    }
                }
                TagEnd::Heading(_) => {
                    let level = heading.take().unwrap_or(1);
                    let runs = inl.take().unwrap_or_default();
                    stack.last_mut().expect("frame").blocks().push(Block::Heading(level, runs));
                }
                TagEnd::CodeBlock => {
                    let body = code.take().unwrap_or_default();
                    let body = body.strip_suffix('\n').unwrap_or(&body).to_owned();
                    stack.last_mut().expect("frame").blocks().push(Block::Code(body));
                }
                TagEnd::BlockQuote(_) => {
                    flush(&mut stack, &mut inl);
                    if let Some(Frame::Quote(b)) = stack.pop() {
                        stack.last_mut().expect("frame").blocks().push(Block::Quote(b));
                    }
                }
                TagEnd::Item => {
                    flush(&mut stack, &mut inl);
                    if let Some(Frame::Item(b)) = stack.pop()
                        && let Some(Frame::List(_, items)) = stack.last_mut()
                    {
                        items.push(b);
                    }
                }
                TagEnd::List(_) => {
                    flush(&mut stack, &mut inl);
                    if let Some(Frame::List(start, items)) = stack.pop() {
                        stack.last_mut().expect("frame").blocks().push(Block::List(start, items));
                    }
                }
                TagEnd::TableCell => {
                    let cell = inl.take().unwrap_or_default();
                    if let Some(row) = table.as_mut().and_then(|t| t.last_mut()) {
                        row.push(cell);
                    }
                }
                TagEnd::Table => {
                    if let Some(t) = table.take() {
                        stack.last_mut().expect("frame").blocks().push(Block::Table(t));
                    }
                }
                TagEnd::Emphasis => italic = italic.saturating_sub(1),
                TagEnd::Strong => bold = bold.saturating_sub(1),
                TagEnd::Strikethrough => strike = strike.saturating_sub(1),
                TagEnd::Link | TagEnd::Image => link = None,
                _ => {}
            },
            Event::Text(t) => match code.as_mut() {
                Some(body) => body.push_str(&t),
                None => push_text(t.to_string(), false, &mut inl),
            },
            Event::Code(t) => push_text(t.to_string(), true, &mut inl),
            Event::Html(t) | Event::InlineHtml(t) => match code.as_mut() {
                Some(body) => body.push_str(&t),
                None => push_text(t.trim_end_matches('\n').to_string(), false, &mut inl),
            },
            Event::SoftBreak => push_text(" ".into(), false, &mut inl),
            Event::HardBreak => push_text("\n".into(), false, &mut inl),
            Event::TaskListMarker(done) => push_text(if done { "☑ " } else { "☐ " }.into(), false, &mut inl),
            Event::Rule => {
                flush(&mut stack, &mut inl);
                stack.last_mut().expect("frame").blocks().push(Block::Rule);
            }
            _ => {}
        }
    }
    flush(&mut stack, &mut inl);
    while stack.len() > 1 {
        let frame = stack.pop().expect("frame");
        let block = match frame {
            Frame::Quote(b) => Block::Quote(b),
            Frame::List(s, items) => Block::List(s, items),
            Frame::Item(b) => Block::List(None, vec![b]),
            Frame::Root(b) => Block::Quote(b),
        };
        stack.last_mut().expect("frame").blocks().push(block);
    }
    match stack.pop() {
        Some(Frame::Root(b)) => b,
        _ => Vec::new(),
    }
}

const BODY: f32 = 14.0;

/// Önizlemeyi kaydırılabilir alanda çizer; tıklanan bağlantının hedefini döndürür (web adresleri tarayıcıda açılır).
pub fn show(ui: &mut Ui, p: &Palette, text: &str, id: impl egui::AsIdSalt) -> Option<String> {
    let blocks = parse(text);
    let mut clicked = None;
    ui.painter().rect_filled(ui.max_rect(), 0.0, color(p.bg));
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(32, 20)).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(860.0));
            draw_blocks(ui, p, &blocks, &mut clicked);
        });
    });
    clicked
}

fn draw_blocks(ui: &mut Ui, p: &Palette, blocks: &[Block], clicked: &mut Option<String>) {
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            ui.add_space(8.0);
        }
        match b {
            Block::Para(runs) => inline(ui, p, runs, BODY, clicked),
            Block::Heading(level, runs) => {
                let size = [26.0, 22.0, 18.0, 16.0, 15.0, 14.0][usize::from(level.saturating_sub(1)).min(5)];
                ui.add_space(4.0);
                inline(ui, p, runs, size, clicked);
                if *level <= 2 {
                    ui.add_space(2.0);
                    let r = ui.available_rect_before_wrap();
                    ui.painter().hline(r.left()..=r.right(), r.top(), Stroke::new(1.0, color(p.border)));
                    ui.add_space(2.0);
                }
            }
            Block::Code(body) => {
                egui::Frame::new()
                    .fill(color(p.bg_dark))
                    .stroke(Stroke::new(1.0, color(p.border)))
                    .corner_radius(4)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        egui::ScrollArea::horizontal().id_salt(ui.next_auto_id()).show(ui, |ui| {
                            let rt = RichText::new(body).font(FontId::monospace(13.0)).color(color(p.fg));
                            ui.add(Label::new(rt).extend());
                        });
                    });
            }
            Block::Quote(inner) => {
                let resp = ui.indent(ui.next_auto_id(), |ui| draw_blocks(ui, p, inner, clicked));
                let r = resp.response.rect;
                ui.painter().vline(r.left() + 4.0, r.y_range(), Stroke::new(3.0, color(p.border)));
            }
            Block::List(start, items) => {
                for (n, item) in items.iter().enumerate() {
                    let marker = match start {
                        Some(s) => format!("{}.", s + n as u64),
                        None => "•".to_owned(),
                    };
                    ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                        let rt = RichText::new(marker).size(BODY).color(color(p.comment));
                        ui.add_sized([22.0, BODY * 1.3], Label::new(rt));
                        ui.vertical(|ui| draw_blocks(ui, p, item, clicked));
                    });
                }
            }
            Block::Table(rows) => {
                egui::Grid::new(ui.next_auto_id()).striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                    for (r, row) in rows.iter().enumerate() {
                        for cell in row {
                            let runs: Vec<Inline> =
                                cell.iter().map(|c| Inline { bold: c.bold || r == 0, ..c.clone() }).collect();
                            inline(ui, p, &runs, BODY, clicked);
                        }
                        ui.end_row();
                    }
                });
            }
            Block::Rule => {
                ui.separator();
            }
        }
    }
}

/// Satır içi parçaları sarmalı akışla çizer; bağlantılar tıklanabilir.
fn inline(ui: &mut Ui, p: &Palette, runs: &[Inline], size: f32, clicked: &mut Option<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for run in runs {
            let mut rt = if run.code {
                RichText::new(&run.text).font(FontId::monospace(size - 1.0)).background_color(color(p.bg_dark))
            } else {
                RichText::new(&run.text).size(size)
            };
            let fg: Color32 = if run.link.is_some() { color(p.accent) } else { color(p.fg) };
            rt = rt.color(fg);
            if run.bold {
                rt = rt.strong().color(fg);
            }
            if run.italic {
                rt = rt.italics();
            }
            if run.strike {
                rt = rt.strikethrough();
            }
            match &run.link {
                Some(url) => {
                    let resp = ui.add(Label::new(rt.underline()).sense(Sense::click()));
                    if resp.on_hover_text(url).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *clicked = Some(url.clone());
                    }
                }
                None => {
                    ui.add(Label::new(rt));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(runs: &[Inline]) -> String {
        runs.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn decision_83_markdown_blocks() {
        let md = "# Title\n\nSome **bold** and `code` [link](https://x.dev).\n\n- a\n- [x] b\n\n> quote\n\n```rs\nfn x() {}\n```\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n---\n";
        let b = parse(md);
        assert!(matches!(&b[0], Block::Heading(1, r) if texts(r) == "Title"));
        let Block::Para(runs) = &b[1] else { panic!("{:?}", b[1]) };
        assert!(runs.iter().any(|r| r.bold && r.text == "bold"));
        assert!(runs.iter().any(|r| r.code && r.text == "code"));
        assert!(runs.iter().any(|r| r.link.as_deref() == Some("https://x.dev") && r.text == "link"));
        let Block::List(None, items) = &b[2] else { panic!("{:?}", b[2]) };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[1][0], Block::Para(r) if texts(r) == "☑ b"));
        assert!(matches!(&b[3], Block::Quote(q) if matches!(&q[0], Block::Para(r) if texts(r) == "quote")));
        assert_eq!(b[4], Block::Code("fn x() {}".into()));
        let Block::Table(rows) = &b[5] else { panic!("{:?}", b[5]) };
        assert_eq!(rows.len(), 2);
        assert_eq!(texts(&rows[1][1]), "2");
        assert_eq!(b[6], Block::Rule);
    }

    #[test]
    fn decision_83_nested_ordered_list() {
        let b = parse("3. one\n   - inner\n4. two\n");
        let Block::List(Some(3), items) = &b[0] else { panic!("{:?}", b[0]) };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0][1], Block::List(None, inner) if inner.len() == 1));
    }
}
