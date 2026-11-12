//! `:options` — Options tab'ı. Şimdilik bir bölüm: **Language Servers** (katalog, durum, Install / Update /
//! Uninstall, projede hangi dil için gerektiği). Yeni bölüm = [`OptionsPanel::sections`]'a bir satır + çizim
//! fonksiyonu.
//!
//! Kurulum yalnız düğmeye basınca başlar (`less_lang::servers::Installer`, arka plan iş parçacığı). Bitince
//! `lang.servers_changed` yayınlanır: `editor::lang` sunucusu bulunamamış açık belgeleri yeniden dener.

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Color32, RichText};
use less_lang::Language;
use less_lang::lsp::registry;
use less_lang::servers::{self, CatalogEntry, InstallEvent, InstallStatus, Installer, Job};

use super::{Cx, Event, Panel, Region};
use crate::widgets::color;

/// Sunucu kurulumu / kaldırılması bitti (veri: katalog kimliği); `editor::lang` dinler.
pub const TOPIC_SERVERS_CHANGED: &str = "lang.servers_changed";

const TAB_KEY: &str = "options";

type SectionFn = fn(&mut OptionsPanel, &mut egui::Ui, &mut Cx);

pub fn panels() -> Vec<Box<dyn Panel>> {
    vec![Box::new(OptionsPanel::new(None))]
}

/// Bir girdinin arayüzdeki hali.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    Status(InstallStatus),
    Working { job: Job, percent: Option<u8>, message: String },
    Error(String),
}

/// Options tab'ının sahibi.
pub struct OptionsPanel {
    /// İlk açılışta kurulur (testler kendi kurucusunu verir).
    installer: Option<Installer>,
    rows: HashMap<&'static str, Row>,
    /// Projede bulunan diller (arka planda taranır) ve Tailwind kullanımı.
    project: Option<(BTreeSet<Language>, bool)>,
    scan: Option<Receiver<(BTreeSet<Language>, bool)>>,
}

impl OptionsPanel {
    fn new(installer: Option<Installer>) -> Self {
        Self { installer, rows: HashMap::new(), project: None, scan: None }
    }

    /// Bölümler (başlık, çizim). Editor yalnız yer tutucu.
    fn sections() -> [(&'static str, SectionFn); 2] {
        [("Language Servers", Self::servers_ui), ("Editor", Self::editor_ui)]
    }

    fn installer(&mut self) -> &mut Installer {
        self.installer.get_or_insert_with(Installer::from_env)
    }

    /// Tab açıldı / yenile: kurulum durumları ve proje taraması.
    fn refresh(&mut self, cx: &Cx) {
        let config = self.installer().config();
        let dir = self.installer().dir().to_path_buf();
        for e in servers::catalog() {
            if !matches!(self.rows.get(e.id), Some(Row::Working { .. })) {
                self.rows.insert(e.id, Row::Status(servers::status(e, &dir, &config)));
            }
        }
        if self.scan.is_none() {
            let (tx, rx) = channel();
            let roots = cx.roots.to_vec();
            let ctx = cx.ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(scan_project(&roots));
                ctx.request_repaint();
            });
            self.scan = Some(rx);
        }
    }

    fn start(&mut self, entry: &'static CatalogEntry, job: Job, cx: &mut Cx) {
        match self.installer().start(entry, job) {
            Ok(()) => {
                let message = if job == Job::Uninstall { "Removing…" } else { "Starting…" };
                self.rows.insert(entry.id, Row::Working { job, percent: None, message: message.into() });
            }
            Err(e) => {
                cx.error(e.clone());
                self.rows.insert(entry.id, Row::Error(e));
            }
        }
    }

    /// Projede bu girdiyi gerektiren diller.
    fn needed_by(&self, entry: &CatalogEntry) -> Option<Vec<Language>> {
        let (langs, tailwind) = self.project.as_ref()?;
        if entry.id == "tailwindcss" && !tailwind {
            return Some(Vec::new());
        }
        Some(entry.languages.iter().copied().filter(|l| langs.contains(l)).collect())
    }

    fn servers_ui(&mut self, ui: &mut egui::Ui, cx: &mut Cx) {
        let p = *cx.palette;
        ui.label(
            RichText::new(format!("Installed into {}. Nothing is downloaded until you press Install.", self.dir()))
                .color(color(p.fg_dim))
                .size(12.0),
        );
        ui.add_space(8.0);
        let mut clicked: Option<(&'static CatalogEntry, Job)> = None;
        egui::Grid::new("options-servers").num_columns(4).spacing([18.0, 10.0]).striped(false).show(ui, |ui| {
            for h in ["Server", "Needed for", "Status", ""] {
                ui.label(RichText::new(h).color(color(p.fg_dim)).size(12.0).strong());
            }
            ui.end_row();
            for e in servers::catalog() {
                ui.label(RichText::new(e.name).color(color(p.fg)).size(13.0));
                let needed = match self.needed_by(e) {
                    None => "…".to_owned(),
                    Some(v) if v.is_empty() => "—".to_owned(),
                    Some(v) => v.iter().map(|l| l.display_name()).collect::<Vec<_>>().join(", "),
                };
                ui.label(RichText::new(needed).color(color(p.fg_dim)).size(12.5));
                let row = self.rows.get(e.id).cloned().unwrap_or(Row::Status(InstallStatus::NotInstalled));
                let (text, c) = status_text(&row, &p);
                let resp = ui.label(RichText::new(text).color(c).size(12.5));
                if let Row::Status(InstallStatus::Installed { path, .. }) = &row {
                    resp.on_hover_text(path.display().to_string());
                }
                ui.horizontal(|ui| match &row {
                    Row::Working { .. } => {
                        ui.add(egui::Spinner::new().size(14.0));
                    }
                    Row::Status(InstallStatus::Installed { managed: true, .. }) => {
                        if ui.button("Update").clicked() {
                            clicked = Some((e, Job::Update));
                        }
                        if ui.button("Uninstall").clicked() {
                            clicked = Some((e, Job::Uninstall));
                        }
                    }
                    Row::Status(InstallStatus::Installed { managed: false, .. }) => {
                        ui.label(RichText::new("Managed outside LESS").color(color(p.comment)).size(12.0));
                    }
                    Row::Status(InstallStatus::NotInstalled) | Row::Error(_) => {
                        if ui.button("Install").clicked() {
                            clicked = Some((e, Job::Install));
                        }
                    }
                });
                ui.end_row();
            }
        });
        if let Some((e, job)) = clicked {
            self.start(e, job, cx);
        }
        if ui.button("Refresh").clicked() {
            self.refresh(cx);
        }
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui, cx: &mut Cx) {
        ui.label(RichText::new("Editing mode and theme settings will appear here.").color(color(cx.palette.fg_dim)));
    }

    fn dir(&self) -> String {
        let dir = self.installer.as_ref().map_or_else(servers::servers_dir, |i| i.dir().to_path_buf());
        let home = std::env::var_os("HOME").map(PathBuf::from);
        match home.as_deref().and_then(|h| dir.strip_prefix(h).ok()) {
            Some(rel) => format!("~/{}", rel.display()),
            None => dir.display().to_string(),
        }
    }
}

fn status_text(row: &Row, p: &less_theme::Palette) -> (String, Color32) {
    match row {
        Row::Status(InstallStatus::Installed { managed: true, .. }) => ("Installed".into(), color(p.extra.diff_add)),
        Row::Status(InstallStatus::Installed { managed: false, .. }) => {
            ("Installed (system)".into(), color(p.extra.diff_add))
        }
        Row::Status(InstallStatus::NotInstalled) => ("Not installed".into(), color(p.fg_dim)),
        Row::Working { job: Job::Uninstall, .. } => ("Removing…".into(), color(p.extra.info)),
        Row::Working { percent: Some(n), .. } => (format!("Installing {n}%"), color(p.extra.info)),
        Row::Working { message, .. } => (message.clone(), color(p.extra.info)),
        Row::Error(e) => (format!("Error: {e}"), color(p.error)),
    }
}

impl Panel for OptionsPanel {
    fn id(&self) -> &'static str {
        "options"
    }

    fn regions(&self) -> &[Region] {
        &[]
    }

    fn ui(&mut self, _region: Region, _ui: &mut egui::Ui, _cx: &mut Cx) {}

    fn tick(&mut self, cx: &mut Cx) {
        if let Some(rx) = &self.scan
            && let Ok(found) = rx.try_recv()
        {
            self.project = Some(found);
        }
        let Some(inst) = &mut self.installer else { return };
        let events = inst.poll();
        if events.is_empty() {
            return;
        }
        for ev in events {
            match ev {
                InstallEvent::Progress { id, percent, message } => {
                    if let Some(Row::Working { percent: p, message: m, .. }) = self.rows.get_mut(id) {
                        (*p, *m) = (percent.or(*p), message);
                    }
                }
                InstallEvent::Finished { id, job, result } => {
                    let name = servers::catalog().iter().find(|e| e.id == id).map_or(id, |e| e.name);
                    match result {
                        Ok(()) => {
                            let done = if job == Job::Uninstall { "removed" } else { "installed" };
                            cx.info(format!("{name} {done}"));
                            self.rows.remove(id);
                            cx.send(TOPIC_SERVERS_CHANGED, id);
                        }
                        Err(e) => {
                            cx.error(format!("{name}: {e}"));
                            self.rows.insert(id, Row::Error(e));
                        }
                    }
                }
            }
        }
        self.refresh(cx);
        cx.ctx.request_repaint();
    }

    fn event(&mut self, event: &Event, cx: &mut Cx) -> bool {
        match event {
            Event::Ex("options" | "Options") => {
                self.refresh(cx);
                cx.open_tab(TAB_KEY, "Options");
                true
            }
            _ => false,
        }
    }

    fn tab_ui(&mut self, _key: &str, ui: &mut egui::Ui, cx: &mut Cx) {
        if self.rows.is_empty() {
            self.refresh(cx);
        }
        if self.rows.values().any(|r| matches!(r, Row::Working { .. })) {
            cx.ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let p = *cx.palette;
        ui.painter().rect_filled(ui.max_rect(), 0.0, color(p.bg));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 20)).show(ui, |ui| {
                ui.label(RichText::new("Options").size(22.0).color(color(p.fg)).strong());
                for (title, draw) in Self::sections() {
                    ui.add_space(18.0);
                    ui.label(RichText::new(title).size(16.0).color(color(p.accent)).strong());
                    ui.separator();
                    draw(self, ui, cx);
                }
            });
        });
    }
}

/// Projedeki dosya dillerini tarar (gizli klasörler ve derleme çıktıları hariç, en çok 20 000 girdi) ve
/// Tailwind kullanılıyor mu (yapılandırma ya da `tailwindcss` bağımlılığı).
fn scan_project(roots: &[PathBuf]) -> (BTreeSet<Language>, bool) {
    const SKIP: &[&str] = &["node_modules", "target", "build", "dist", "out", "vendor", "__pycache__", "venv"];
    let mut langs = BTreeSet::new();
    let mut budget = 20_000usize;
    let mut stack: Vec<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            budget = match budget.checked_sub(1) {
                Some(b) => b,
                None => break,
            };
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => {
                    if depth < 8 && !name.starts_with('.') && !SKIP.contains(&name.as_ref()) {
                        stack.push((path, depth + 1));
                    }
                }
                Ok(_) => {
                    if let Some(l) = Language::from_path(&path) {
                        langs.insert(l);
                    }
                }
                Err(_) => {}
            }
        }
    }
    let tailwind = registry::linters_for(Language::Css).into_iter().find(|s| s.id == "tailwindcss");
    let tailwind = tailwind.is_some_and(|spec| roots.iter().any(|r| uses_tailwind(&spec, r)));
    (langs, tailwind)
}

fn uses_tailwind(spec: &registry::ServerSpec, root: &Path) -> bool {
    registry::extra_root(spec, &root.join("index.css")).is_some()
}
