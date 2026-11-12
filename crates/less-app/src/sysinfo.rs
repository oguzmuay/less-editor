//! Durum satırı sistem bilgisi (#73): beş saniyede bir arka planda örneklenir. Kaynaklar 120, "Leaked IP" 90
//! sütundan dar ekranda gizlenir; GUI'de sütun genişliği kod fontunun hücre ölçüsünden hesaplanır.

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;
use less_workspace::sysinfo::{self, Snapshot};

const INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct SystemInfo {
    snapshot: Snapshot,
    rx: Option<Receiver<Snapshot>>,
    sampled: Option<Instant>,
}

impl SystemInfo {
    /// Zamanı gelen örneği arka planda başlatır ve tamamlananı alır. Testlerde dış komut çalıştırılmaz.
    pub fn tick(&mut self, ctx: &egui::Context, now: Instant) {
        if cfg!(test) {
            return;
        }
        if let Some(snapshot) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.snapshot = snapshot;
            self.rx = None;
            self.sampled = Some(now);
        }
        let due = self.sampled.is_none_or(|at| now.duration_since(at) >= INTERVAL);
        if due && self.rx.is_none() {
            let (tx, rx) = mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(sysinfo::sample());
                ctx.request_repaint();
            });
            self.rx = Some(rx);
            self.sampled = Some(now);
        }
        ctx.request_repaint_after(INTERVAL);
    }

    /// Durum satırının sağına eklenecek bileşenler, `(atılma önceliği, metin)`; bkz. [`status_parts`].
    pub fn parts(&self, width: f32, cell_width: f32) -> Vec<(u8, String)> {
        status_parts(&self.snapshot, width / cell_width.max(1.0))
    }

    #[cfg(test)]
    pub fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = snapshot;
    }
}

/// Lua'daki eşikler: kaynaklar yalnız 120+, IP yalnız 90+ sütunda. Parçalar ayrı ki dar pencerede tek tek düşsün:
/// öncelik büyük olan önce atılır (disk 7, RAM 5, CPU 4, IP 3; bkz. `status::fit`).
pub fn status_parts(snapshot: &Snapshot, columns: f32) -> Vec<(u8, String)> {
    let mut parts = Vec::new();
    if columns >= 120.0 {
        if let Some(c) = snapshot.cpu_percent {
            parts.push((4, format!("CPU {c}%")));
        }
        if let (Some(u), Some(t)) = (snapshot.mem_used, snapshot.mem_total) {
            parts.push((5, format!("RAM {}/{}", sysinfo::gb(u), sysinfo::gb(t))));
        }
        if let Some(d) = snapshot.disk_free {
            parts.push((7, format!("Disk {} free", sysinfo::gb(d))));
        }
    }
    if columns >= 90.0
        && let Some(ip) = &snapshot.ip
    {
        parts.push((3, format!("Leaked IP {ip}")));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            cpu_percent: Some(12),
            mem_used: Some(8 * 1024u64.pow(3)),
            mem_total: Some(16 * 1024u64.pow(3)),
            disk_free: Some(100 * 1024u64.pow(3)),
            ip: Some("192.168.1.2".into()),
        }
    }

    #[test]
    fn narrow_status_hides_resources_then_ip_73() {
        let wide: Vec<String> = status_parts(&snapshot(), 120.0).into_iter().map(|(_, t)| t).collect();
        assert_eq!(wide, ["CPU 12%", "RAM 8.0G/16.0G", "Disk 100G free", "Leaked IP 192.168.1.2"]);
        let medium = status_parts(&snapshot(), 100.0);
        assert_eq!(medium, [(3, "Leaked IP 192.168.1.2".to_owned())]);
        assert!(status_parts(&snapshot(), 89.0).is_empty());
    }

    #[test]
    fn default_window_width_shows_every_part_73() {
        let mut info = SystemInfo::default();
        info.set_snapshot(snapshot());
        let cell = crate::editor::view::CODE_SIZE * 0.6;
        assert_eq!(info.parts(1280.0, cell).len(), 4, "1280 px window (default size) is wide enough");
        assert_eq!(info.parts(800.0, cell).len(), 1, "only Leaked IP below 120 columns");
    }
}
