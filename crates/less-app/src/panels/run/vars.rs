//! Debug Variables bölümü (#91 #93 #97): `less_run::variables::VariablesModel` tablosu, imleç ve düzenleme işleri.
//!
//! DAP istekleri bloklayıcı olduğu için her iş (yenileme, değer atama, eleman silme / ekleme) modelin bir kopyasıyla
//! arka plan iş parçacığında yapılır; sonuç kanaldan döner ve yalnız son işin sonucu uygulanır.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use less_run::dap::{DapClient, DebugManager, Lang, SessionId};
use less_run::variables::{VarRow, VariablesModel, ViewStatus};

/// Oturum bağlamı (iş parçacığına kopyalanır).
#[derive(Clone)]
struct Target {
    client: DapClient,
    frame: Option<i64>,
    caps: less_run::dap::types::Capabilities,
    lang: Option<Lang>,
}

/// Arka plan işinin türü.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Refresh,
    Set { path: String, value: String },
    Delete { path: String },
    Append { path: String, value: String },
}

struct Done {
    generation: u64,
    model: VariablesModel,
    error: Option<String>,
}

/// Variables görünümü.
pub struct VarsView {
    pub model: VariablesModel,
    /// Satır imleci (yol; satırlar değişince yerinde kalsın).
    pub cursor: Option<String>,
    pub session: Option<SessionId>,
    generation: u64,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    waker: Option<eframe::egui::Context>,
}

impl Default for VarsView {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { model: VariablesModel::new(), cursor: None, session: None, generation: 0, tx, rx, waker: None }
    }
}

impl VarsView {
    pub fn set_waker(&mut self, ctx: eframe::egui::Context) {
        self.waker = Some(ctx);
    }

    /// Görünen satırlar.
    pub fn rows(&self) -> Vec<VarRow> {
        self.model.rows()
    }

    /// İmlecin satır dizini (yoksa 0).
    pub fn cursor_index(&self, rows: &[VarRow]) -> usize {
        self.cursor.as_ref().and_then(|c| rows.iter().position(|r| &r.path == c)).unwrap_or(0)
    }

    /// İmleci `delta` satır taşır.
    pub fn move_cursor(&mut self, delta: isize) {
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let i = (self.cursor_index(&rows) as isize + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.cursor = Some(rows[i].path.clone());
    }

    /// İmleçteki satır.
    pub fn current(&self) -> Option<VarRow> {
        let rows = self.rows();
        let i = self.cursor_index(&rows);
        rows.into_iter().nth(i)
    }

    /// Oturum durdu: karşılaştırma tabanı kaydırılır, tablo yenilenir.
    pub fn on_stopped(&mut self, mgr: &DebugManager, session: SessionId) {
        self.session = Some(session);
        self.model.on_stopped();
        self.run(mgr, Op::Refresh);
    }

    pub fn on_continued(&mut self) {
        self.generation += 1;
        self.model.on_continued();
    }

    pub fn on_ended(&mut self) {
        self.generation += 1;
        self.session = None;
        self.model.on_ended();
    }

    /// Aç / kapa ve yenile.
    pub fn toggle(&mut self, mgr: &DebugManager, path: &str) {
        self.model.toggle(path);
        self.run(mgr, Op::Refresh);
    }

    /// Oturumdan iş bağlamı.
    fn target(&self, mgr: &DebugManager) -> Option<Target> {
        let s = self.session.and_then(|id| mgr.session(id)).or_else(|| mgr.stopped_session())?;
        Some(Target { client: s.client.clone(), frame: s.frame_id(), caps: s.capabilities.clone(), lang: s.lang() })
    }

    /// Duran oturumun dili (null değeri için).
    pub fn lang(&self, mgr: &DebugManager) -> Option<Lang> {
        self.target(mgr).and_then(|t| t.lang)
    }

    /// İşi arka planda başlatır; `false` = oturum yok.
    pub fn run(&mut self, mgr: &DebugManager, op: Op) -> bool {
        let Some(t) = self.target(mgr) else { return false };
        self.generation += 1;
        let (generation, tx, mut model, waker) =
            (self.generation, self.tx.clone(), self.model.clone(), self.waker.clone());
        let _ = thread::Builder::new().name("run-vars".into()).spawn(move || {
            let error = apply(&mut model, &t, &op).err();
            let _ = model.refresh(&t.client, t.frame);
            let _ = tx.send(Done { generation, model, error });
            if let Some(w) = waker {
                w.request_repaint();
            }
        });
        true
    }

    /// Biten işleri uygular; hata metinlerini döndürür.
    pub fn poll(&mut self) -> Vec<String> {
        let mut errors = Vec::new();
        while let Ok(d) = self.rx.try_recv() {
            if let Some(e) = d.error {
                errors.push(e);
            }
            if d.generation == self.generation {
                self.model = d.model;
            }
        }
        errors
    }

    /// Program devam ediyor (tablo soluk) mu.
    pub fn is_running(&self) -> bool {
        self.model.status == ViewStatus::Running
    }
}

/// İşin DAP kısmı (yenilemeden önce).
fn apply(model: &mut VariablesModel, t: &Target, op: &Op) -> Result<(), String> {
    match op {
        Op::Refresh => Ok(()),
        Op::Set { path, value } => {
            let frame = t.frame.ok_or("The program is running")?;
            let req = model.set_request(path, value, &t.caps, frame)?;
            model.apply_set(&t.client, path, &req).map(|_| ()).map_err(|e| e.to_string())
        }
        Op::Delete { path } => {
            let (expr, parent) = model.delete_expr(path, t.lang)?;
            t.client.evaluate(&expr, t.frame, "repl").map_err(|e| e.to_string())?;
            model.clear_overrides_under(&parent);
            Ok(())
        }
        Op::Append { path, value } => {
            let (list, list_expr) = model.append_target(path)?;
            let expr = VariablesModel::append_expr(&list_expr, value, t.lang)?;
            t.client.evaluate(&expr, t.frame, "repl").map_err(|e| e.to_string())?;
            model.clear_overrides_under(&list);
            model.expand(&list);
            Ok(())
        }
    }
}
