//! Maven paneli (Cmd+M, `:Maven`): sağ sütunda Database'in yerinde. Köklerdeki her `pom.xml` modülünün lifecycle
//! fazlarını, plugin goal'larını ve araç goal'larını listeler; seçilen goal pty'de çalışır, çıktısı panelin altında
//! görünür. `n` / `:MavenNew grup:artifact` yeni Maven projesi oluşturur.

pub mod pom;

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Align2, FontId, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use less_input::{Key, KeyEvent};
use less_run::output::{LineSplitter, strip_ansi};
use less_run::process::{ExitInfo, ProcMsg, PtyProcess};
use less_theme::Palette;

use super::run::icons;
use super::{Area, Cx, Effect, Event, LayoutOp, Panel, Region};
use crate::fonts::icon;
use crate::widgets::{color, ui_font};

const REGIONS: [Region; 1] = [Region::Area(Area::Maven)];
const ROW_H: f32 = 24.0;
const HEAD_H: f32 = 30.0;
const LOG_ROW_H: f32 = 16.0;
const MAX_LOG: usize = 20_000;

/// Bu klasörün panelleri; host `panels::all()` ile toplar.
pub fn panels() -> Vec<Box<dyn Panel>> {
    vec![Box::new(MavenPanel::default())]
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RowKind {
    Module,
    Group,
    /// Çalıştırılacak goal (`clean`, `spring-boot:run`).
    Goal(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    key: String,
    label: String,
    depth: usize,
    module: usize,
    kind: RowKind,
    open: bool,
    parent: Option<usize>,
}

/// Son çalıştırılan goal: süreç, çıktı satırları ve çıkış bilgisi.
struct Job {
    title: String,
    proc: PtyProcess,
    rx: Receiver<ProcMsg>,
    splitter: LineSplitter,
    lines: Vec<String>,
    exit: Option<ExitInfo>,
}

/// Maven sağ sütun paneli.
#[derive(Default)]
pub struct MavenPanel {
    modules: Vec<pom::Module>,
    scanned: bool,
    /// Varsayılan açık/kapalı durumu tersine çevrilmiş düğümler.
    toggled: HashSet<String>,
    cursor: usize,
    job: Option<Job>,
    /// `n` ile açılan yeni proje girişi (`grup:artifact`).
    prompt: Option<String>,
}

impl MavenPanel {
    fn scan(&mut self, cx: &Cx) {
        self.modules = pom::discover(cx.roots);
        self.scanned = true;
    }

    fn is_open(&self, key: &str, default: bool) -> bool {
        default != self.toggled.contains(key)
    }

    /// Ağacı düzleştirir: modül → Lifecycle / Plugins (plugin → goal) / Tools.
    fn rows(&self) -> Vec<Row> {
        let mut out = Vec::new();
        let single = self.modules.len() == 1;
        for (m, module) in self.modules.iter().enumerate() {
            let base = module.dir.display().to_string();
            let open = self.is_open(&base, single);
            let me = out.len();
            out.push(Row {
                key: base.clone(),
                label: module.name.clone(),
                depth: 0,
                module: m,
                kind: RowKind::Module,
                open,
                parent: None,
            });
            if !open {
                continue;
            }
            let group = |out: &mut Vec<Row>, key: String, label: &str, depth, parent, default| {
                let open = self.is_open(&key, default);
                out.push(Row {
                    key,
                    label: label.to_string(),
                    depth,
                    module: m,
                    kind: RowKind::Group,
                    open,
                    parent: Some(parent),
                });
                open
            };
            let goal = |out: &mut Vec<Row>, label: String, goal: String, depth, parent| {
                out.push(Row {
                    key: format!("{base}|goal|{goal}"),
                    label,
                    depth,
                    module: m,
                    kind: RowKind::Goal(goal),
                    open: false,
                    parent: Some(parent),
                });
            };
            let at = out.len();
            if group(&mut out, format!("{base}|lifecycle"), "Lifecycle", 1, me, true) {
                for phase in pom::LIFECYCLE {
                    goal(&mut out, phase.to_string(), phase.to_string(), 2, at);
                }
            }
            if !module.plugins.is_empty() {
                let plugins = out.len();
                if group(&mut out, format!("{base}|plugins"), "Plugins", 1, me, true) {
                    for plugin in &module.plugins {
                        let at = out.len();
                        if group(
                            &mut out,
                            format!("{base}|plugin|{}", plugin.prefix),
                            &plugin.prefix,
                            2,
                            plugins,
                            false,
                        ) {
                            for g in &plugin.goals {
                                goal(
                                    &mut out,
                                    format!("{}:{g}", plugin.prefix),
                                    format!("{}:{g}", plugin.prefix),
                                    3,
                                    at,
                                );
                            }
                        }
                    }
                }
            }
            let at = out.len();
            if group(&mut out, format!("{base}|tools"), "Tools", 1, me, false) {
                for tool in pom::TOOLS {
                    goal(&mut out, tool.to_string(), tool.to_string(), 2, at);
                }
            }
        }
        out
    }

    fn run(&mut self, module: usize, goal: &str, cx: &mut Cx) {
        if self.job.as_ref().is_some_and(|j| j.exit.is_none()) {
            cx.error("A Maven goal is already running (x to stop)");
            return;
        }
        let Some(module) = self.modules.get(module) else { return };
        let spec = pom::command(&module.dir, goal);
        let (tx, rx) = channel();
        let ctx = cx.ctx.clone();
        let title = format!("{} · mvn {goal}", module.name);
        match less_run::process::spawn(&spec, 40, 160, move |msg| {
            let _ = tx.send(msg);
            ctx.request_repaint();
        }) {
            Ok(proc) => {
                let lines = vec![format!("$ {}", spec.argv.join(" "))];
                self.job = Some(Job { title, proc, rx, splitter: LineSplitter::default(), lines, exit: None });
            }
            Err(e) => cx.error(format!("Could not start {}: {e}", spec.argv[0])),
        }
    }

    fn stop(&mut self) {
        if let Some(job) = self.job.as_ref().filter(|j| j.exit.is_none()) {
            job.proc.stop();
        }
    }

    /// Satırı etkinleştirir: goal ise çalıştırır, düğümse açar / kapatır.
    fn activate(&mut self, row: &Row, cx: &mut Cx) {
        match &row.kind {
            RowKind::Goal(goal) => self.run(row.module, &goal.clone(), cx),
            _ => self.toggle(&row.key),
        }
    }

    fn toggle(&mut self, key: &str) {
        if !self.toggled.remove(key) {
            self.toggled.insert(key.to_string());
        }
    }

    fn new_project(&mut self, spec: &str, cx: &mut Cx) {
        let Some(root) = cx.roots.first().cloned() else {
            cx.error("Open a project folder first");
            return;
        };
        let created =
            pom::coordinates(spec).and_then(|(g, a)| pom::create_project(&root, &g, &a).map_err(|e| e.to_string()));
        match created {
            Ok(pom) => {
                cx.info(format!("Created {}", pom.display()));
                cx.refresh_files();
                cx.open_file(pom, None);
                self.scan(cx);
            }
            Err(e) => cx.error(e),
        }
    }

    fn prompt_key(&mut self, key: KeyEvent, cx: &mut Cx) -> bool {
        let Some(text) = self.prompt.as_mut() else { return false };
        match key.key {
            Key::Esc => self.prompt = None,
            Key::Enter => {
                let spec = self.prompt.take().unwrap_or_default();
                self.new_project(&spec, cx);
            }
            Key::Backspace => {
                text.pop();
            }
            _ => {
                if let Some(c) = key.plain_char() {
                    text.push(c);
                }
            }
        }
        true
    }

    fn panel_key(&mut self, key: KeyEvent, cx: &mut Cx) -> bool {
        if self.prompt_key(key, cx) {
            return true;
        }
        let rows = self.rows();
        let index = self.cursor.min(rows.len().saturating_sub(1));
        let row = rows.get(index).cloned();
        let last = rows.len().saturating_sub(1);
        match (key.key, key.plain_char()) {
            (Key::Up, _) | (_, Some('k')) => self.cursor = index.saturating_sub(1),
            (Key::Down, _) | (_, Some('j')) => self.cursor = (index + 1).min(last),
            (Key::Home | Key::PageUp, _) | (_, Some('g')) => self.cursor = 0,
            (Key::End | Key::PageDown, _) | (_, Some('G')) => self.cursor = last,
            (Key::Enter, _) | (_, Some(' ')) => {
                if let Some(row) = row {
                    self.activate(&row, cx);
                }
            }
            (Key::Right, _) | (_, Some('l')) => match row {
                Some(r) if r.kind != RowKind::Module && r.kind != RowKind::Group => self.activate(&r, cx),
                Some(r) if !r.open => self.toggle(&r.key),
                _ => {}
            },
            (Key::Left, _) | (_, Some('h')) => match row {
                Some(r) if r.open => self.toggle(&r.key),
                Some(Row { parent: Some(p), .. }) => self.cursor = p,
                _ => {}
            },
            (_, Some('x')) => self.stop(),
            (_, Some('r')) => self.scan(cx),
            (_, Some('n')) => self.prompt = Some(String::new()),
            (_, Some('c')) if self.job.as_ref().is_some_and(|j| j.exit.is_some()) => self.job = None,
            (_, Some('q')) => cx.layout(LayoutOp::Close(Area::Maven)),
            (Key::Esc, _) => cx.layout(LayoutOp::Focus(Area::Code)),
            _ => return false,
        }
        true
    }

    fn poll(&mut self, cx: &mut Cx) {
        let Some(job) = self.job.as_mut() else { return };
        while let Ok(msg) = job.rx.try_recv() {
            match msg {
                ProcMsg::Output(bytes) => job.lines.extend(job.splitter.push(&bytes).iter().map(|l| strip_ansi(l))),
                ProcMsg::Exit(info) => {
                    if info.success() {
                        cx.info(format!("{} finished", job.title));
                    } else {
                        cx.error(format!("{} failed (exit {})", job.title, info.code));
                    }
                    job.exit = Some(info);
                }
            }
        }
        if job.lines.len() > MAX_LOG {
            job.lines.drain(..job.lines.len() - MAX_LOG);
        }
    }

    fn tree_ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.palette;
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, color(p.surface));
        let (head, _) = ui.allocate_exact_size(vec2(rect.width(), HEAD_H), Sense::hover());
        let tint = if cx.focused { color(p.accent) } else { color(p.comment) };
        ui.painter().text(head.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, "MAVEN", ui_font(11.0), tint);
        let button = |ui: &mut Ui, at: f32, glyph: &str, id: &str| {
            let r = Rect::from_center_size(head.right_center() - vec2(at, 0.0), vec2(24.0, 24.0));
            ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, ui_font(15.0), color(p.accent));
            ui.interact(r, ui.scope_id().with(id), Sense::click()).clicked()
        };
        if button(ui, 18.0, icon::PLUS, "mvn-new") {
            self.prompt = Some(String::new());
        }
        if button(ui, 46.0, icons::RERUN, "mvn-rescan") {
            self.scan(cx);
        }
        if let Some(text) = &self.prompt {
            let (r, _) = ui.allocate_exact_size(vec2(rect.width(), ROW_H + 6.0), Sense::hover());
            ui.painter().rect_filled(r.shrink2(vec2(6.0, 2.0)), 4.0, color(p.bg_highlight));
            let shown = if text.is_empty() { "groupId:artifactId  (Enter create · Esc cancel)" } else { text };
            let fg = if text.is_empty() { p.comment } else { p.fg };
            ui.painter().text(
                r.left_center() + vec2(14.0, 0.0),
                Align2::LEFT_CENTER,
                format!("New project › {shown}"),
                ui_font(12.5),
                color(fg),
            );
        }
        let rows = self.rows();
        if rows.is_empty() {
            ui.painter().text(
                ui.cursor().min + vec2(14.0, 22.0),
                Align2::LEFT_TOP,
                "No pom.xml found\n\nn / +  New Maven project\nr      Rescan",
                ui_font(12.5),
                color(p.comment),
            );
            return;
        }
        let current = self.cursor.min(rows.len() - 1);
        let mut clicked = None;
        egui::ScrollArea::vertical().id_salt("mvn-tree").auto_shrink([false, false]).show_rows(
            ui,
            ROW_H,
            rows.len(),
            |ui, range| {
                for (index, row) in rows.iter().enumerate().take(range.end).skip(range.start) {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    if index == current {
                        let fill =
                            if cx.focused { color(p.accent).gamma_multiply(0.25) } else { color(p.bg_highlight) };
                        ui.painter().rect_filled(r.shrink2(vec2(3.0, 1.0)), 4.0, fill);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, 0.0, color(p.bg_highlight).gamma_multiply(0.5));
                    }
                    paint_row(ui, p, row, r);
                    if resp.clicked() {
                        clicked = Some((index, false));
                    }
                    if resp.double_clicked() {
                        clicked = Some((index, true));
                    }
                }
            },
        );
        if let Some((index, double)) = clicked {
            self.cursor = index;
            let row = &rows[index];
            if double || row.kind == RowKind::Module || row.kind == RowKind::Group {
                self.activate(&row.clone(), cx);
            }
        }
    }

    fn log_ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.palette;
        let Some(job) = &self.job else { return };
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, color(p.bg));
        ui.painter().hline(rect.x_range(), rect.top(), egui::Stroke::new(1.0, color(p.border)));
        let (head, _) = ui.allocate_exact_size(vec2(rect.width(), HEAD_H - 4.0), Sense::hover());
        let (glyph, tone) = match &job.exit {
            None => (icons::SPINNER, p.extra.warn),
            Some(e) if e.success() => (icons::CHECK_CIRCLE, p.extra.diff_add),
            Some(_) => (icons::X_CIRCLE, p.error),
        };
        ui.painter().text(head.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, glyph, ui_font(13.0), color(tone));
        ui.painter().text(
            head.left_center() + vec2(32.0, 0.0),
            Align2::LEFT_CENTER,
            &job.title,
            ui_font(12.0),
            color(p.fg),
        );
        let hint = if job.exit.is_none() { "x stop" } else { "c clear" };
        ui.painter().text(
            head.right_center() - vec2(10.0, 0.0),
            Align2::RIGHT_CENTER,
            hint,
            ui_font(11.0),
            color(p.comment),
        );
        let font = FontId::monospace(11.5);
        egui::ScrollArea::both().id_salt("mvn-log").auto_shrink([false, false]).stick_to_bottom(true).show_rows(
            ui,
            LOG_ROW_H,
            job.lines.len(),
            |ui, range| {
                for line in &job.lines[range] {
                    let fg = if line.contains("[ERROR]") || line.contains("BUILD FAILURE") {
                        p.error
                    } else if line.contains("[WARNING]") {
                        p.extra.warn
                    } else if line.contains("BUILD SUCCESS") {
                        p.extra.diff_add
                    } else {
                        p.fg_dim
                    };
                    ui.add(egui::Label::new(egui::RichText::new(line).font(font.clone()).color(color(fg))).extend());
                }
            },
        );
    }
}

fn paint_row(ui: &Ui, p: &Palette, row: &Row, r: Rect) {
    let x = r.left() + 9.0 + row.depth as f32 * 16.0;
    let y = r.center().y;
    let (caret, glyph, tint) = match row.kind {
        RowKind::Module => (if row.open { icon::CARET_DOWN } else { icon::CARET_RIGHT }, icon::FOLDER, p.accent),
        RowKind::Group => (if row.open { icon::CARET_DOWN } else { icon::CARET_RIGHT }, icon::SQUARES_FOUR, p.comment),
        RowKind::Goal(_) => ("", icons::PLAY, p.extra.diff_add),
    };
    ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, caret, ui_font(10.0), color(p.comment));
    ui.painter().text(pos2(x + 16.0, y), Align2::LEFT_CENTER, glyph, ui_font(12.5), color(tint));
    let fg = if row.kind == RowKind::Group { p.fg_dim } else { p.fg };
    ui.painter().text(pos2(x + 36.0, y), Align2::LEFT_CENTER, &row.label, ui_font(13.0), color(fg));
}

impl Panel for MavenPanel {
    fn id(&self) -> &'static str {
        "maven"
    }

    fn regions(&self) -> &[Region] {
        &REGIONS
    }

    fn tick(&mut self, cx: &mut Cx) {
        self.poll(cx);
    }

    fn ui(&mut self, _region: Region, ui: &mut Ui, cx: &mut Cx) {
        if !self.scanned {
            self.scan(cx);
        }
        let rect = ui.max_rect();
        let log_h = if self.job.is_some() { (rect.height() * 0.45).round() } else { 0.0 };
        let split = rect.bottom() - log_h;
        let mut top = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(rect.min, pos2(rect.right(), split))));
        top.set_clip_rect(top.max_rect());
        self.tree_ui(&mut top, cx);
        if log_h > 0.0 {
            let mut bottom =
                ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(pos2(rect.left(), split), rect.max)));
            bottom.set_clip_rect(bottom.max_rect());
            self.log_ui(&mut bottom, cx);
        }
    }

    fn event(&mut self, event: &Event, cx: &mut Cx) -> bool {
        match *event {
            Event::Key(key) => self.panel_key(key, cx),
            Event::Ex(cmd) => match cmd.strip_prefix("MavenNew") {
                Some(rest) if rest.is_empty() || rest.starts_with(' ') => {
                    if rest.trim().is_empty() {
                        self.prompt = Some(String::new());
                        cx.focus(Area::Maven);
                    } else {
                        self.new_project(rest, cx);
                    }
                    true
                }
                _ => false,
            },
            Event::Saved(path) => {
                if self.scanned && path.file_name().is_some_and(|n| n == "pom.xml") {
                    self.scan(cx);
                }
                false
            }
            Event::Layout(Effect::Opened(Area::Maven)) => {
                self.scan(cx);
                false
            }
            _ => false,
        }
    }

    fn can_quit(&mut self, _cx: &mut Cx) -> bool {
        self.stop();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::Request;
    use crate::panels::testing::Harness;

    fn panel_with_project() -> (tempfile::TempDir, Harness, MavenPanel) {
        let t = tempfile::tempdir().unwrap();
        pom::create_project(t.path(), "com.acme", "demo").unwrap();
        let mut h = Harness::new(vec![t.path().to_path_buf()]);
        let mut panel = MavenPanel::default();
        panel.scan(&h.cx("maven"));
        (t, h, panel)
    }

    fn press(panel: &mut MavenPanel, h: &mut Harness, c: char) -> bool {
        panel.event(&Event::Key(KeyEvent::char(c)), &mut h.cx("maven"))
    }

    #[test]
    fn single_module_lists_lifecycle_plugins_and_navigates() {
        let (_t, mut h, mut panel) = panel_with_project();
        let labels: Vec<_> = panel.rows().iter().map(|r| r.label.clone()).collect();
        assert_eq!(&labels[..3], ["demo", "Lifecycle", "clean"]);
        assert!(labels.contains(&"exec".to_string()) && labels.contains(&"Tools".to_string()));
        let exec = labels.iter().position(|l| l == "exec").unwrap();
        panel.cursor = exec;
        press(&mut panel, &mut h, 'l');
        assert!(panel.rows().iter().any(|r| r.kind == RowKind::Goal("exec:java".into())));
        press(&mut panel, &mut h, 'j');
        press(&mut panel, &mut h, 'h');
        assert_eq!(panel.cursor, exec);
        press(&mut panel, &mut h, 'h');
        assert!(!panel.rows().iter().any(|r| r.label == "exec:java"));
    }

    #[test]
    fn prompt_creates_project_and_ex_command_validates() {
        let (t, mut h, mut panel) = panel_with_project();
        press(&mut panel, &mut h, 'n');
        for c in "com.acme:lib".chars() {
            press(&mut panel, &mut h, c);
        }
        panel.event(&Event::Key(KeyEvent::plain(Key::Enter)), &mut h.cx("maven"));
        assert!(t.path().join("lib/pom.xml").is_file());
        assert_eq!(panel.modules.len(), 2);
        assert!(h.requests.contains(&Request::RefreshFiles));
        h.requests.clear();
        assert!(panel.event(&Event::Ex("MavenNew 9bad:x"), &mut h.cx("maven")));
        assert!(matches!(h.requests.last(), Some(Request::Notify { error: true, .. })));
        assert!(!panel.event(&Event::Ex("MavenNewer"), &mut h.cx("maven")));
    }
}
