//! Pencere durumu (konum, boyut, tam ekran / büyütülmüş, zoom) proje başına hatırlanır.
//!
//! Kayıt `state_dir/windows.json`: her projenin son durumu ve hangi projeden olursa olsun en son pencere (`last`).
//! Uygulama açılırken pencere `last` ile kurulur; proje açılınca o projenin kaydı uygulanır. Konum ve boyut zoom'dan
//! bağımsız (zoom 1'deki nokta) tutulur, böylece zoom değişse de pencere aynı fiziksel boyutta açılır. Büyütülmüş ya da
//! tam ekranken normal pencerenin konum/boyutu korunur; çıkınca pencere eski yerine döner.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Pos2, Vec2, ViewportBuilder, ViewportCommand};
use serde::{Deserialize, Serialize};

/// Değişiklikten sonra yazmadan önce beklenen süre (sürükleme / boyutlandırma bitsin).
const SAVE_DELAY: Duration = Duration::from_millis(400);
/// Uygulanan durum pencereye yerleşene kadar (macOS tam ekran animasyonu dahil) yakalama yapılmaz.
const SETTLE: Duration = Duration::from_millis(1500);
const MIN_SIZE: [f32; 2] = [640.0, 420.0];

/// Kaydedilen pencere durumu; `pos` dış çerçevenin sol üstü, `size` iç alan, ikisi de zoom 1'deki nokta.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub pos: Option<[f32; 2]>,
    pub size: [f32; 2],
    #[serde(default)]
    pub maximized: bool,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default = "one")]
    pub zoom: f32,
}

fn one() -> f32 {
    1.0
}

impl WindowState {
    /// Bozuk ya da çok küçük kayıtları en küçük boyuta, zoom'u egui'nin sınırlarına çeker.
    fn sanitized(mut self) -> Self {
        self.size = [self.size[0].max(MIN_SIZE[0]), self.size[1].max(MIN_SIZE[1])];
        if !self.zoom.is_finite() || self.zoom <= 0.0 {
            self.zoom = 1.0;
        }
        self.zoom = self.zoom.clamp(0.2, 5.0);
        self.pos = self.pos.filter(|p| p.iter().all(|v| v.is_finite()));
        self
    }

    /// Açılış penceresini bu duruma göre kurar (zoom ilk karede uygulanır).
    pub fn builder(self, viewport: ViewportBuilder) -> ViewportBuilder {
        let mut viewport = viewport.with_inner_size(self.size).with_maximized(self.maximized);
        if let Some(pos) = self.pos {
            viewport = viewport.with_position(pos);
        }
        viewport
    }

    /// Penceredeki şu anki durum. `prev`: büyütülmüş / tam ekranken korunacak normal konum ve boyut.
    /// Örnek: zoom 2'de 500×300 noktalık iç alan → `size` `[1000, 600]`.
    fn capture(info: &egui::ViewportInfo, zoom: f32, prev: Option<WindowState>) -> Option<Self> {
        if info.minimized == Some(true) {
            return None;
        }
        let maximized = info.maximized.unwrap_or(false);
        let fullscreen = info.fullscreen.unwrap_or(false);
        let normal = |r: Option<egui::Rect>| r.filter(|_| !maximized && !fullscreen);
        let size = match normal(info.inner_rect) {
            Some(r) => (r.size() * zoom).into(),
            None => prev.map(|p| p.size).or_else(|| info.inner_rect.map(|r| (r.size() * zoom).into()))?,
        };
        let pos = match normal(info.outer_rect) {
            Some(r) => Some((r.min.to_vec2() * zoom).into()),
            None => prev.and_then(|p| p.pos),
        };
        Some(Self { pos, size, maximized, fullscreen, zoom })
    }

    /// Bu durumu çalışan pencereye uygulayan komutlar; konum/boyut şu anki zoom'a göre noktaya çevrilir.
    fn commands(self, info: &egui::ViewportInfo, zoom: f32) -> Vec<ViewportCommand> {
        let mut out = Vec::new();
        if info.fullscreen.unwrap_or(false) != self.fullscreen {
            out.push(ViewportCommand::Fullscreen(self.fullscreen));
        }
        if info.maximized.unwrap_or(false) != self.maximized {
            out.push(ViewportCommand::Maximized(self.maximized));
        }
        if !self.fullscreen && !self.maximized {
            out.push(ViewportCommand::InnerSize(Vec2::from(self.size) / zoom));
            if let Some(pos) = self.pos {
                out.push(ViewportCommand::OuterPosition(Pos2::from(pos) / zoom));
            }
        }
        out
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    last: Option<WindowState>,
    #[serde(default)]
    projects: BTreeMap<String, WindowState>,
}

/// Kaydı okur, açık projenin penceresini izler ve değişince yazar.
pub struct Windows {
    path: PathBuf,
    store: Store,
    key: Option<String>,
    /// Bir sonraki karede uygulanacak durum; zoom ondan bir kare sonra (konum/boyut eski zoom'la çevrilsin).
    apply: Option<WindowState>,
    zoom_next: Option<f32>,
    settle_until: Option<Instant>,
    current: Option<WindowState>,
    dirty_since: Option<Instant>,
}

impl Windows {
    pub fn load() -> Self {
        Self::open(less_workspace::paths::state_dir().join("windows.json"))
    }

    pub fn open(path: PathBuf) -> Self {
        let store = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self {
            path,
            store,
            key: None,
            apply: None,
            zoom_next: None,
            settle_until: None,
            current: None,
            dirty_since: None,
        }
    }

    /// Uygulamanın en son penceresi (açılış penceresi bununla kurulur).
    pub fn last(&self) -> Option<WindowState> {
        self.store.last.map(WindowState::sanitized)
    }

    /// Proje açıldı: kaydı varsa bir sonraki karede uygulanır, sonraki değişiklikler bu projeye yazılır.
    pub fn select(&mut self, key: &Path) {
        let key = key.to_string_lossy().into_owned();
        self.apply = self.store.projects.get(&key).copied().map(WindowState::sanitized);
        self.key = Some(key);
        self.current = None;
    }

    /// Proje kapandı: karşılama ekranındaki değişiklikler yalnız `last`'a yazılır.
    pub fn deselect(&mut self) {
        self.key = None;
    }

    /// Her kare: bekleyen durumu uygular, pencereyi yakalar, değiştiyse kısa bir beklemeden sonra (kapanışta hemen) yazar.
    pub fn frame(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let (info, closing) = ctx.input(|i| (i.viewport().clone(), i.viewport().close_requested()));
        if let Some(zoom) = self.zoom_next.take() {
            ctx.set_zoom_factor(zoom);
        }
        if let Some(state) = self.apply.take() {
            for cmd in state.commands(&info, ctx.zoom_factor()) {
                ctx.send_viewport_cmd(cmd);
            }
            self.zoom_next = Some(state.zoom);
            self.current = Some(state);
            self.settle_until = Some(now + SETTLE);
            ctx.request_repaint();
            return;
        }
        if self.settle_until.is_some_and(|t| now < t) {
            ctx.request_repaint_after(SETTLE);
            return;
        }
        let Some(state) = WindowState::capture(&info, ctx.zoom_factor(), self.current) else { return };
        if self.current != Some(state) {
            self.current = Some(state);
            self.dirty_since = Some(now);
        }
        match self.dirty_since {
            Some(t) if closing || now.duration_since(t) >= SAVE_DELAY => self.save(),
            Some(_) => ctx.request_repaint_after(SAVE_DELAY),
            None => {}
        }
    }

    fn save(&mut self) {
        self.dirty_since = None;
        let Some(state) = self.current else { return };
        self.store.last = Some(state);
        if let Some(key) = &self.key {
            self.store.projects.insert(key.clone(), state);
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.store) {
            let tmp = self.path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &self.path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2, vec2};

    fn info(inner: Rect, maximized: bool, fullscreen: bool) -> egui::ViewportInfo {
        egui::ViewportInfo {
            inner_rect: Some(inner),
            outer_rect: Some(inner),
            maximized: Some(maximized),
            fullscreen: Some(fullscreen),
            minimized: Some(false),
            ..Default::default()
        }
    }

    #[test]
    fn capture_stores_zoom_independent_size_and_keeps_normal_rect_when_maximized() {
        let normal = info(Rect::from_min_size(pos2(10.0, 20.0), vec2(500.0, 300.0)), false, false);
        let s = WindowState::capture(&normal, 2.0, None).unwrap();
        assert_eq!((s.pos, s.size, s.zoom), (Some([20.0, 40.0]), [1000.0, 600.0], 2.0));

        let big = info(Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 900.0)), true, false);
        let m = WindowState::capture(&big, 2.0, Some(s)).unwrap();
        assert!(m.maximized);
        assert_eq!((m.pos, m.size), (s.pos, s.size), "normal rect survives maximize");

        let cmds = m.commands(&normal, 1.0);
        assert_eq!(cmds, vec![ViewportCommand::Maximized(true)]);
        let cmds = s.commands(&big, 2.0);
        assert_eq!(cmds[0], ViewportCommand::Maximized(false));
        assert_eq!(cmds[1], ViewportCommand::InnerSize(vec2(500.0, 300.0)));
    }

    #[test]
    fn store_roundtrips_per_project_and_last() {
        let dir = std::env::temp_dir().join(format!("less-windows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("windows.json");
        let mut w = Windows::open(path.clone());
        w.select(Path::new("/p/a"));
        w.current =
            Some(WindowState { pos: None, size: [800.0, 600.0], maximized: false, fullscreen: true, zoom: 1.5 });
        w.save();

        let mut w = Windows::open(path);
        assert!(w.last().unwrap().fullscreen);
        w.select(Path::new("/p/a"));
        assert_eq!(w.apply.unwrap().zoom, 1.5);
        w.select(Path::new("/p/b"));
        assert!(w.apply.is_none(), "unknown project keeps the current window");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
