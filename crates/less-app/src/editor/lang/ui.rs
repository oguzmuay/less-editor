//! Kod görünümünün üstüne çizim: diagnostik alt çizgileri, sağ kenar işaretleri ve sağ üstte sayaç (#31),
//! oluktaki breakpoint'ler (#92), tooltip (#52 #70), menü ve rename istemi (#13).

use eframe::egui::{self, Color32, FontId, Order, Rect, Stroke, pos2, text::LayoutJob, vec2};
use less_git::blame::BlameLine;
use less_lang::{Severity, TooltipStyle};
use less_theme::Palette;

use super::text::char_of_pos;
use super::{Anchor, Hit, Lang};
use crate::editor::EditorTab;
use crate::editor::view::CodeGeometry;
use crate::panels::Cx;
use crate::widgets::color;

const TIP_SIZE: f32 = 12.5;
const MARK_W: f32 = 5.0;

/// Önem derecesinin tema rengi.
pub fn severity_color(p: &Palette, s: Severity) -> Color32 {
    color(match s {
        Severity::Error => p.error,
        Severity::Warning => p.extra.warn,
        Severity::Info => p.extra.info,
        Severity::Hint => p.extra.hint,
    })
}

/// Sayaç parçaları: her önem derecesi için `✖ 2`, sonra `↑` `↓`; problem yoksa `✓` (Lua `winbar()`).
pub fn counter_parts(counts: &less_lang::SeverityCounts) -> Vec<(String, Option<Severity>, Hit)> {
    let mut parts: Vec<(String, Option<Severity>, Hit)> = Severity::ALL
        .into_iter()
        .filter(|s| counts.get(*s) > 0)
        .map(|s| (format!("{} {}", s.icon(), counts.get(s)), Some(s), Hit::Jump { forward: true, severity: Some(s) }))
        .collect();
    if parts.is_empty() {
        return vec![("\u{2713}".to_owned(), None, Hit::Line(u32::MAX))];
    }
    parts.push(("\u{2191}".to_owned(), None, Hit::Jump { forward: false, severity: None }));
    parts.push(("\u{2193}".to_owned(), None, Hit::Jump { forward: true, severity: None }));
    parts
}

/// Alt çizgiler, kenar işaretleri, sayaç ve breakpoint noktaları.
pub fn marks(lang: &mut Lang, tab: &EditorTab, geo: &CodeGeometry, ui: &egui::Ui, cx: &Cx) {
    let Some(path) = tab.path.as_deref() else { return };
    let p = cx.palette;
    let painter = ui.painter_at(geo.rect);
    let rope = tab.editor.buffer.rope();
    let lines = rope.len_lines().max(1);

    blame_strip(lang.blame.get(path), geo, ui, p);

    for &(ref bp_path, line) in &lang.breakpoints {
        if bp_path == path
            && let Some(y) = geo.line_y(line)
        {
            painter.circle_filled(pos2(geo.gutter.left() + 8.0, y + geo.line_h / 2.0), 4.5, color(p.error));
        }
    }

    let diags = lang.lsp.diagnostics().get(path);
    for d in diags.iter().rev() {
        let c = severity_color(p, d.severity);
        let y = geo.rect.top() + (d.start.line as f32 + 0.5) / lines as f32 * geo.rect.height();
        let mark = Rect::from_min_size(pos2(geo.rect.right() - MARK_W, y - 1.5), vec2(MARK_W, 3.0));
        painter.rect_filled(mark, 1.0, c);
        lang.hits.push((mark.expand2(vec2(0.0, 2.0)), Hit::Line(d.start.line)));
        if let Some(mm) = geo.minimap
            && let Some(row) = (d.start.line as usize).checked_sub(mm.first)
        {
            let y = mm.rect.top() + row as f32 * 2.0;
            if y < mm.rect.bottom() {
                painter.rect_filled(Rect::from_min_size(pos2(mm.rect.left(), y), vec2(mm.rect.width(), 2.0)), 0.0, c);
            }
        }

        let (a, b) = (char_of_pos(rope, d.start), char_of_pos(rope, d.end));
        let b = if b > a { b } else { (a + 1).min(rope.len_chars()) };
        let (la, lb) = (rope.char_to_line(a), rope.char_to_line(b.saturating_sub(1).max(a)));
        for line in la.max(geo.first_line)..=lb.min(geo.last_line.saturating_sub(1)) {
            let ls = rope.line_to_char(line);
            let le = ls + super::line_text(rope, line).chars().count();
            let (from, to) = (a.max(ls), b.min(le).max(a.max(ls) + 1));
            let (Some(x0), Some(x1)) = (geo.pos_of(rope, from), geo.pos_of(rope, to)) else { continue };
            let x1 = if to > le { x1.x + geo.char_w } else { x1.x };
            squiggle(&painter, x0.x.max(geo.text.left()), x1.min(geo.text.right()), x0.y + geo.line_h - 2.5, c);
        }
    }

    let parts = counter_parts(&lang.lsp.diagnostics().counts(path));
    if tab.language.is_none() && diags.is_empty() {
        return;
    }
    let font = FontId::proportional(12.0);
    let galleys: Vec<_> = parts
        .into_iter()
        .map(|(t, s, hit)| {
            let c = s.map_or(color(p.fg_dim), |s| severity_color(p, s));
            (ui.painter().layout_no_wrap(t, font.clone(), c), hit)
        })
        .collect();
    let gap = 8.0;
    let width: f32 = galleys.iter().map(|(g, _)| g.size().x).sum::<f32>() + gap * (galleys.len() as f32 + 1.0);
    let right = geo.minimap.map_or(geo.rect.right() - MARK_W, |m| m.rect.left());
    let bg = Rect::from_min_size(pos2(right - 4.0 - width, geo.rect.top() + 3.0), vec2(width, 18.0));
    painter.rect_filled(bg, 4.0, color(p.surface).gamma_multiply(0.92));
    let mut x = bg.left() + gap;
    for (g, hit) in galleys {
        let r = Rect::from_min_size(pos2(x, bg.center().y - g.size().y / 2.0), g.size());
        if hit != Hit::Line(u32::MAX) {
            lang.hits.push((r.expand(3.0), hit));
        }
        x += g.size().x + gap;
        painter.galley(r.min, g, color(p.fg));
    }
}

/// Oluğun sağ kenarında satır başına commit yaşı: yeni değişiklik parlak, eski soluk; üstünde durunca yazar,
/// tarih ve commit özeti. Commit edilmemiş satırlar en parlak.
fn blame_strip(lines: Option<&Vec<Option<BlameLine>>>, geo: &CodeGeometry, ui: &egui::Ui, p: &Palette) {
    let Some(lines) = lines.filter(|l| l.iter().any(Option::is_some)) else { return };
    let times = lines.iter().flatten().map(|b| b.time);
    let (min, max) = (times.clone().min().unwrap_or(0), times.max().unwrap_or(0));
    let painter = ui.painter_at(geo.gutter);
    for (line, blame) in lines.iter().enumerate().take(geo.last_line).skip(geo.first_line) {
        let Some(y) = geo.line_y(line) else { continue };
        let age =
            blame.as_ref().map_or(1.0, |b| if max > min { (b.time - min) as f32 / (max - min) as f32 } else { 1.0 });
        let bar = Rect::from_min_max(
            pos2(geo.gutter.right() - 6.0, y + 1.0),
            pos2(geo.gutter.right() - 3.0, y + geo.line_h - 1.0),
        );
        painter.rect_filled(bar, 1.0, color(p.accent).gamma_multiply(0.12 + 0.88 * age));
        let row = Rect::from_min_max(pos2(geo.gutter.left() + 14.0, y), pos2(geo.gutter.right(), y + geo.line_h));
        let text = match blame {
            Some(b) => format!("{} · {}\n{}", b.author, b.date, b.summary),
            None => "Not committed yet".to_owned(),
        };
        ui.interact(row, ui.scope_id().with(("blame", line)), egui::Sense::hover()).on_hover_text(text);
    }
}

fn squiggle(painter: &egui::Painter, x0: f32, x1: f32, y: f32, c: Color32) {
    let mut pts = Vec::new();
    let mut x = x0;
    let mut up = false;
    while x < x1 {
        pts.push(pos2(x, if up { y - 1.2 } else { y + 1.2 }));
        x += 2.5;
        up = !up;
    }
    pts.push(pos2(x1, y));
    painter.add(egui::Shape::line(pts, Stroke::new(1.0, c)));
}

/// Tooltip, menü ve rename istemi (öne çıkan egui alanları).
pub fn popups(lang: &mut Lang, tab: &mut EditorTab, geo: &CodeGeometry, ui: &mut egui::Ui, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let rope = tab.editor.buffer.rope().clone();
    let below_cursor =
        geo.pos_of(&rope, tab.editor.cursor()).map(|p| p + vec2(0.0, geo.line_h)).unwrap_or(geo.text.left_top());
    let p = *cx.palette;

    if let Some(tip) = &lang.tip
        && tab.path.as_ref() == Some(&tip.path)
        && !tip.tooltip.is_empty()
    {
        let at = match tip.anchor {
            Anchor::Mouse(m) => Some(m + vec2(4.0, geo.line_h)),
            Anchor::Cursor => geo.pos_of(&rope, tab.editor.cursor()).map(|p| p + vec2(0.0, geo.line_h)),
        };
        if let Some(at) = at {
            let cols = (geo.rect.width() / geo.char_w) as usize;
            let char_w = ctx.fonts_mut(|f| f.glyph_width(&FontId::monospace(TIP_SIZE), 'M'));
            let width = tip.tooltip.width(cols) as f32 * char_w;
            egui::Area::new(ui.scope_id().with("lang-tip"))
                .order(Order::Tooltip)
                .fixed_pos(at)
                .interactable(false)
                .constrain(true)
                .show(&ctx, |ui| tooltip_ui(ui, &tip.tooltip, width, &p));
        }
    }

    if let Some((path, text, since)) = &lang.notice {
        let left = super::NOTICE.saturating_sub(since.elapsed());
        if left.is_zero() {
            lang.notice = None;
        } else if path.is_none() || tab.path.as_ref() == path.as_ref() {
            ctx.request_repaint_after(left);
            let at = geo.pos_of(&rope, tab.editor.cursor()).unwrap_or(geo.text.left_top());
            egui::Area::new(ui.scope_id().with("lang-notice"))
                .order(Order::Tooltip)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(at - vec2(0.0, 2.0))
                .interactable(false)
                .constrain(true)
                .show(&ctx, |ui| {
                    popup_frame(&p).stroke(Stroke::new(1.0, color(p.extra.warn))).show(ui, |ui| {
                        ui.label(egui::RichText::new(text).color(color(p.extra.warn)).size(13.0));
                    });
                });
        }
    }

    if let Some(menu) = &lang.menu {
        let mut chosen = None;
        let at = menu.pos.unwrap_or(below_cursor);
        let shown = egui::Area::new(ui.scope_id().with("lang-menu"))
            .order(Order::Foreground)
            .fixed_pos(at)
            .constrain(true)
            .show(&ctx, |ui| {
                popup_frame(&p).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        ui.set_min_width(200.0);
                        for (i, (label, _)) in menu.items.iter().enumerate() {
                            let text = egui::RichText::new(label).color(color(p.fg)).size(13.0);
                            if ui.add(egui::Button::selectable(i == menu.selected, text)).clicked() {
                                chosen = Some(i);
                            }
                        }
                    });
                });
            });
        let outside = ui.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|pos| !shown.response.rect.contains(pos))
        });
        if let Some(i) = chosen {
            let action = lang.menu.take().and_then(|m| m.items.into_iter().nth(i)).map(|(_, a)| a);
            if let Some(a) = action {
                lang.run_action(a, tab, cx);
            }
        } else if outside {
            lang.menu = None;
        }
    }

    completion_ui(lang, tab, geo, ui, cx);

    if let Some(prompt) = &mut lang.rename {
        let at = geo.pos_of(&rope, prompt.at).map(|p| p + vec2(0.0, geo.line_h)).unwrap_or(below_cursor);
        let mut done = None;
        egui::Area::new(ui.scope_id().with("lang-rename")).order(Order::Foreground).fixed_pos(at).constrain(true).show(
            &ctx,
            |ui| {
                popup_frame(&p).show(ui, |ui| {
                    ui.label(egui::RichText::new("Rename to").color(color(p.fg_dim)).size(12.0));
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut prompt.text).font(FontId::monospace(13.0)).desired_width(240.0),
                    );
                    if std::mem::take(&mut prompt.focus) {
                        resp.request_focus();
                    } else if resp.lost_focus() {
                        done = Some(ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    }
                });
            },
        );
        if let Some(submit) = done {
            let prompt = lang.rename.take().expect("prompt is open");
            if submit {
                lang.submit_rename(prompt, tab, cx);
            }
        }
    }
}

fn popup_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(color(p.surface))
        .stroke(Stroke::new(1.0, color(p.border)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
}

/// Tooltip içeriği: renkli ikonlu diagnostik satırları, ayraç, dokümantasyon (kod çitleri gizli).
fn tooltip_ui(ui: &mut egui::Ui, tip: &less_lang::Tooltip, width: f32, p: &Palette) {
    popup_frame(p).show(ui, |ui| {
        ui.set_max_width(width);
        let font = FontId::monospace(TIP_SIZE);
        for line in &tip.lines {
            let mut job = LayoutJob::default();
            job.wrap.max_width = width;
            let fmt = |c: Color32| egui::TextFormat::simple(font.clone(), c);
            match line.style {
                TooltipStyle::Separator => {
                    ui.painter().hline(
                        ui.max_rect().x_range(),
                        ui.cursor().top() + 6.0,
                        Stroke::new(1.0, color(p.border)),
                    );
                    ui.add_space(12.0);
                    continue;
                }
                TooltipStyle::Diagnostic { severity, prefix_len } => {
                    job.append(&line.text[..prefix_len], 0.0, fmt(severity_color(p, severity)));
                    job.append(&line.text[prefix_len..], 0.0, fmt(color(p.fg)));
                }
                TooltipStyle::DiagnosticWithSource { severity, prefix_len, source_from } => {
                    job.append(&line.text[..prefix_len], 0.0, fmt(severity_color(p, severity)));
                    job.append(&line.text[prefix_len..source_from], 0.0, fmt(color(p.fg)));
                    job.append(&line.text[source_from..], 0.0, fmt(color(p.comment)));
                }
            }
            ui.label(job);
        }
        if let Some(doc) = &tip.doc {
            doc_ui(ui, doc, tip.doc_is_markdown, width, p);
        }
    });
}

/// Dokümantasyon: markdown ise biçimli (bağlantılar yalnız adlarıyla, adres gösterilmez), değilse düz metin.
fn doc_ui(ui: &mut egui::Ui, doc: &str, markdown: bool, width: f32, p: &Palette) {
    ui.set_max_width(width);
    if markdown {
        crate::editor::markdown::draw(ui, p, doc);
        return;
    }
    let mut job = LayoutJob::single_section(
        doc.to_owned(),
        egui::TextFormat::simple(FontId::monospace(TIP_SIZE), color(p.fg_dim)),
    );
    job.wrap.max_width = width;
    ui.label(job);
}

/// Tamamlama listesi (imlecin altında, kelime başına hizalı), seçili öğenin dokümantasyonu (sağında) ve imza
/// yardımı (imlecin üstünde).
fn completion_ui(lang: &mut Lang, tab: &EditorTab, geo: &CodeGeometry, ui: &mut egui::Ui, cx: &Cx) {
    let ctx = ui.ctx().clone();
    let p = *cx.palette;
    let rope = tab.editor.buffer.rope();
    let cursor_pt = geo.pos_of(rope, tab.editor.cursor());

    if let (Some(sig), Some(at)) = (&lang.signature, cursor_pt)
        && let Some(view) = &sig.help
    {
        let font = FontId::monospace(13.0);
        egui::Area::new(ui.scope_id().with("lang-signature"))
            .order(Order::Tooltip)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(at - vec2(0.0, 2.0))
            .interactable(false)
            .constrain(true)
            .show(&ctx, |ui| {
                popup_frame(&p).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = 560.0;
                    let fmt = |c: Color32| egui::TextFormat::simple(font.clone(), c);
                    match view.active {
                        Some((a, b)) => {
                            job.append(&view.label[..a], 0.0, fmt(color(p.fg)));
                            let mut strong = fmt(color(p.accent));
                            strong.underline = Stroke::new(1.0, color(p.accent));
                            job.append(&view.label[a..b], 0.0, strong);
                            job.append(&view.label[b..], 0.0, fmt(color(p.fg)));
                        }
                        None => job.append(&view.label, 0.0, fmt(color(p.fg))),
                    }
                    ui.label(job);
                    for d in [&view.param_doc, &view.doc].into_iter().flatten() {
                        let short: String = d.lines().take(6).collect::<Vec<_>>().join("\n");
                        doc_ui(ui, &short, true, 560.0, &p);
                    }
                });
            });
    }

    let Some(comp) = &mut lang.completion else { return };
    if comp.shown.is_empty() || tab.path.as_ref() != Some(&comp.path) {
        return;
    }
    let Some(anchor) = geo.pos_of(rope, comp.anchor).or(cursor_pt) else { return };
    let at = pos2(anchor.x - 22.0, anchor.y + geo.line_h + 2.0);
    let font = FontId::monospace(13.0);
    let row_h = 20.0;
    let list_w = 420.0;
    let scroll = std::mem::take(&mut comp.scroll);
    let mut clicked = None;
    let list = egui::Area::new(ui.scope_id().with("lang-completion"))
        .order(Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(&ctx, |ui| {
            egui::Frame::new()
                .fill(color(p.surface))
                .stroke(Stroke::new(1.0, color(p.border)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.set_width(list_w);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    egui::ScrollArea::vertical().max_height(row_h * 10.0).auto_shrink([false, true]).show(ui, |ui| {
                        for (row, &i) in comp.shown.iter().enumerate() {
                            let item = &comp.items[i].item;
                            let (rect, resp) = ui.allocate_exact_size(vec2(list_w, row_h), egui::Sense::click());
                            let selected = row == comp.selected;
                            if selected {
                                ui.painter().rect_filled(rect, 3.0, color(p.bg_highlight));
                                if scroll {
                                    ui.scroll_to_rect(rect, None);
                                }
                            }
                            let (letter, kind) = super::complete::kind_badge(item.kind);
                            let badge = Rect::from_min_size(rect.min + vec2(3.0, 3.0), vec2(14.0, 14.0));
                            let kc = color(crate::editor::syntax::kind_color(&p, kind));
                            ui.painter().rect_filled(badge, 3.0, kc.gamma_multiply(0.25));
                            ui.painter().text(
                                badge.center(),
                                egui::Align2::CENTER_CENTER,
                                letter,
                                FontId::monospace(10.5),
                                kc,
                            );
                            let deprecated = item.deprecated == Some(true)
                                || item
                                    .tags
                                    .as_ref()
                                    .is_some_and(|t| t.contains(&lsp_types::CompletionItemTag::DEPRECATED));
                            let mut label = egui::TextFormat::simple(font.clone(), color(p.fg));
                            if deprecated {
                                label.strikethrough = Stroke::new(1.0, color(p.fg_dim));
                            }
                            let mut job = LayoutJob::default();
                            job.append(&item.label, 0.0, label);
                            if let Some(d) = item.label_details.as_ref().and_then(|d| d.detail.as_deref()) {
                                job.append(d, 0.0, egui::TextFormat::simple(font.clone(), color(p.fg_dim)));
                            }
                            let galley = ui.painter().layout_job(job);
                            let text_at = pos2(badge.right() + 6.0, rect.center().y - galley.size().y / 2.0);
                            let label_right = text_at.x + galley.size().x;
                            ui.painter().with_clip_rect(rect).galley(text_at, galley, color(p.fg));
                            let detail = item
                                .label_details
                                .as_ref()
                                .and_then(|d| d.description.as_deref())
                                .or(item.detail.as_deref())
                                .map(|d| d.lines().next().unwrap_or(""));
                            if let Some(d) = detail
                                && rect.right() - label_right > 60.0
                            {
                                let small = FontId::monospace(11.5);
                                let max = ((rect.right() - label_right - 16.0) / 7.0) as usize;
                                let d: String = if d.chars().count() > max {
                                    d.chars().take(max.saturating_sub(1)).chain(['…']).collect()
                                } else {
                                    d.to_owned()
                                };
                                ui.painter().text(
                                    pos2(rect.right() - 6.0, rect.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    d,
                                    small,
                                    color(p.comment),
                                );
                            }
                            if resp.clicked() {
                                clicked = Some(i);
                            }
                        }
                    });
                });
        });

    let sel = comp.shown.get(comp.selected).map(|&i| &comp.items[i].item);
    let doc = sel.and_then(super::complete::documentation);
    let detail = sel.and_then(|i| i.detail.clone()).filter(|d| !d.trim().is_empty());
    if doc.is_some() || detail.is_some() {
        let r = list.response.rect;
        egui::Area::new(ui.scope_id().with("lang-completion-doc"))
            .order(Order::Foreground)
            .fixed_pos(r.right_top() + vec2(4.0, 0.0))
            .interactable(false)
            .constrain(true)
            .show(&ctx, |ui| {
                popup_frame(&p).show(ui, |ui| {
                    ui.set_max_width(380.0);
                    if let Some(d) = &detail {
                        let mut job = LayoutJob::single_section(
                            d.clone(),
                            egui::TextFormat::simple(FontId::monospace(TIP_SIZE), color(p.fg)),
                        );
                        job.wrap.max_width = 380.0;
                        ui.label(job);
                    }
                    if let Some(doc) = &doc {
                        if detail.is_some() {
                            ui.add_space(4.0);
                        }
                        let short: String = doc.lines().take(30).collect::<Vec<_>>().join("\n");
                        doc_ui(ui, &short, true, 380.0, &p);
                    }
                });
            });
    }
    if clicked.is_some() {
        lang.clicked_item = clicked;
    }
}
