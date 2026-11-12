//! Çıkışta Run süreçlerinin düzgün kapatılması (#80): her birine SIGINT (Ctrl+C), ortada liste; `k` seçileni / `K`
//! hepsini SIGKILL ile öldürür, hepsi kapanınca çıkılır. UI bu süreçte `Runner::poll` çağırmaya devam etmelidir.

use std::time::Instant;

use crate::runner::Runner;

/// Listedeki satırın durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownState {
    /// Kapandı.
    Stopped,
    /// SIGINT gönderildi, bekleniyor (`secs` saniyedir).
    Stopping { secs: u64 },
    /// SIGKILL gönderildi.
    Killing,
}

/// Liste satırı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownRow {
    pub id: String,
    pub label: String,
    pub state: ShutdownState,
}

/// Süren kapanış.
#[derive(Debug, Clone)]
pub struct Shutdown {
    items: Vec<(String, String, Instant, bool)>,
}

impl Shutdown {
    /// Çalışan süreç yoksa `None` (UI doğrudan çıkar); varsa hepsine SIGINT gönderir.
    pub fn begin(runner: &mut Runner) -> Option<Shutdown> {
        let ids: Vec<(String, String)> =
            runner.running().iter().map(|s| (s.spec.id.clone(), s.spec.label.clone())).collect();
        if ids.is_empty() {
            return None;
        }
        let now = Instant::now();
        for (id, _) in &ids {
            runner.interrupt(id);
        }
        Some(Shutdown { items: ids.into_iter().map(|(id, label)| (id, label, now, false)).collect() })
    }

    /// Liste satırları (başlangıç sırasıyla).
    pub fn rows(&self, runner: &Runner) -> Vec<ShutdownRow> {
        self.items
            .iter()
            .map(|(id, label, started, killed)| {
                let running = runner.item(id).is_some_and(|s| s.is_running());
                let state = if !running {
                    ShutdownState::Stopped
                } else if *killed {
                    ShutdownState::Killing
                } else {
                    ShutdownState::Stopping { secs: started.elapsed().as_secs() }
                };
                ShutdownRow { id: id.clone(), label: label.clone(), state }
            })
            .collect()
    }

    /// Öğeyi zorla öldürür (`k`).
    pub fn kill(&mut self, runner: &mut Runner, id: &str) {
        if let Some(entry) = self.items.iter_mut().find(|e| e.0 == id)
            && runner.item(id).is_some_and(|s| s.is_running())
        {
            entry.3 = true;
            runner.force_kill(id);
        }
    }

    /// Hepsini zorla öldürür (`K`).
    pub fn kill_all(&mut self, runner: &mut Runner) {
        let ids: Vec<String> = self.items.iter().map(|e| e.0.clone()).collect();
        for id in ids {
            self.kill(runner, &id);
        }
    }

    /// Hepsi kapandı mı (çıkılabilir).
    pub fn finished(&self, runner: &Runner) -> bool {
        self.items.iter().all(|(id, ..)| !runner.item(id).is_some_and(|s| s.is_running()))
    }
}
