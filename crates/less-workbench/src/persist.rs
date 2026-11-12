//! Proje / workspace başına panel düzeni (`layouts.json`, #99) ve pinli tablar (#30).
//!
//! Dosyalar `less_workspace::paths::state_dir()` altındadır (`LESS_RS_STATE_DIR` ile yönlendirilir). Biçim
//! LESS-nvim'inkiyle aynı yapıdadır: `{version:1, layouts:{<anahtar>:{panels, sizes, time}}}`; boyutlar burada
//! birim (px) cinsindendir ve LESS-nvim dosyasından içe aktarılmaz (orada sütun sayısı).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use less_workspace::paths;
use serde::{Deserialize, Serialize};

use crate::layout::{Layout, Panel};

/// Kayıtlı boyutlar (birim: px).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedSizes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<f32>,
    /// AI'ın sağ sütundaki payı.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_split: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<f32>,
}

/// Tek projenin düzeni.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub panels: Vec<String>,
    #[serde(default)]
    pub sizes: SavedSizes,
    #[serde(default)]
    pub time: i64,
}

/// Kayıtlı düzen yokken açılış düzeni (#81): Files + Code + AI.
pub fn default_snapshot() -> Snapshot {
    Snapshot { panels: vec!["files".into(), "claude".into()], ..Snapshot::default() }
}

impl Snapshot {
    /// O anki düzenin görüntüsü. Sol / sağ genişlik, açık olana göre kaydedilir.
    pub fn capture(layout: &Layout, now: i64) -> Self {
        let left = match layout.left() {
            Some(Panel::Git) => layout.sizes.git_w,
            Some(_) => layout.sizes.files_w,
            None => None,
        };
        Self {
            panels: layout.open_panels().into_iter().map(|p| p.key().to_owned()).collect(),
            sizes: SavedSizes {
                left,
                right: layout.sizes.right_w,
                right_split: layout.sizes.ai_ratio,
                bottom: layout.sizes.strip_h,
            },
            time: now,
        }
    }

    /// Düzeni uygular: kayıtlı panel listesi tam uygulanır (listede olmayanlar kapanır), boyutlar yüklenir.
    pub fn apply(&self, layout: &mut Layout) {
        let want: Vec<Panel> = self.panels.iter().filter_map(|k| Panel::from_key(k)).collect();
        for p in layout.open_panels() {
            if !want.contains(&p) {
                layout.close(p);
            }
        }
        let side = |p: &Panel| matches!(p, Panel::Database | Panel::Maven);
        for p in want.iter().filter(|p| !side(p)) {
            layout.open(*p);
        }
        if let Some(p) = want.iter().find(|p| side(p)) {
            layout.open(*p);
        }
        layout.press(Panel::Code);
        if let Some(w) = self.sizes.right {
            layout.sizes.right_w = Some(w);
        }
        match layout.left() {
            Some(Panel::Git) => layout.sizes.git_w = self.sizes.left.or(layout.sizes.git_w),
            Some(_) => layout.sizes.files_w = self.sizes.left.or(layout.sizes.files_w),
            None => {}
        }
        layout.sizes.ai_ratio = self.sizes.right_split.or(layout.sizes.ai_ratio);
        layout.sizes.strip_h = self.sizes.bottom.or(layout.sizes.strip_h);
    }
}

#[derive(Default, Serialize, Deserialize)]
struct LayoutsFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    layouts: BTreeMap<String, Snapshot>,
}

/// `layouts.json` deposu.
pub struct LayoutStore {
    path: PathBuf,
    file: LayoutsFile,
}

impl LayoutStore {
    pub fn load() -> Self {
        Self::open(&paths::state_dir().join("layouts.json"))
    }

    pub fn open(path: &Path) -> Self {
        let file = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path: path.to_path_buf(), file }
    }

    /// Anahtarın kayıtlı düzeni, yoksa varsayılan (#81).
    pub fn get_or_default(&self, key: &str) -> Snapshot {
        self.file.layouts.get(key).cloned().unwrap_or_else(default_snapshot)
    }

    pub fn get(&self, key: &str) -> Option<&Snapshot> {
        self.file.layouts.get(key)
    }

    /// Düzeni kaydeder; panel ve boyutlar değişmediyse dosyaya dokunmaz.
    pub fn save(&mut self, key: &str, snap: Snapshot) -> std::io::Result<bool> {
        if let Some(old) = self.file.layouts.get(key)
            && old.panels == snap.panels
            && old.sizes == snap.sizes
        {
            return Ok(false);
        }
        self.file.layouts.insert(key.to_owned(), snap);
        self.file.version = 1;
        let json = serde_json::to_vec_pretty(&self.file).map_err(std::io::Error::other)?;
        paths::write_atomic(&self.path, &json)?;
        Ok(true)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct PinsFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    pins: BTreeMap<String, Vec<PathBuf>>,
}

/// Proje başına pinli tab yolları (`pins.json`); sıra pin sırasıdır, en baştaki koda geçişte gösterilir (#30).
pub struct PinStore {
    path: PathBuf,
    file: PinsFile,
}

impl PinStore {
    pub fn load() -> Self {
        Self::open(&paths::state_dir().join("pins.json"))
    }

    pub fn open(path: &Path) -> Self {
        let file = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path: path.to_path_buf(), file }
    }

    pub fn pins(&self, key: &str) -> &[PathBuf] {
        self.file.pins.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn is_pinned(&self, key: &str, path: &Path) -> bool {
        self.pins(key).iter().any(|p| p == path)
    }

    /// Cmd+P: pinliyse kaldırır, değilse sona ekler. Yeni durumu (pinli mi) döndürür.
    pub fn toggle(&mut self, key: &str, path: &Path) -> std::io::Result<bool> {
        let list = self.file.pins.entry(key.to_owned()).or_default();
        let pinned = if let Some(i) = list.iter().position(|p| p == path) {
            list.remove(i);
            false
        } else {
            list.push(path.to_path_buf());
            true
        };
        if list.is_empty() {
            self.file.pins.remove(key);
        }
        self.save()?;
        Ok(pinned)
    }

    /// Koda geçişte gösterilecek ilk pinli tab.
    pub fn first(&self, key: &str) -> Option<&Path> {
        self.pins(key).first().map(PathBuf::as_path)
    }

    fn save(&mut self) -> std::io::Result<()> {
        self.file.version = 1;
        let json = serde_json::to_vec_pretty(&self.file).map_err(std::io::Error::other)?;
        paths::write_atomic(&self.path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("less-rs-wb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn default_layout_is_files_and_ai_81() {
        let store = LayoutStore::open(&tmp("def").join("layouts.json"));
        assert_eq!(store.get_or_default("/p").panels, ["files", "claude"]);
    }

    #[test]
    fn snapshot_roundtrip_99() {
        let dir = tmp("rt");
        let mut l = Layout::default();
        l.reset(1600.0, 900.0);
        l.open(Panel::Git);
        l.open(Panel::Terminal);
        l.open(Panel::Run);
        l.press(Panel::Code);
        let mut store = LayoutStore::open(&dir.join("layouts.json"));
        assert!(store.save("/p", Snapshot::capture(&l, 5)).unwrap());
        assert!(!store.save("/p", Snapshot::capture(&l, 6)).unwrap());

        let again = LayoutStore::open(&dir.join("layouts.json"));
        let mut l2 = Layout::default();
        l2.open(Panel::Files);
        l2.open(Panel::Ai);
        again.get("/p").unwrap().apply(&mut l2);
        assert!(l2.is_open(Panel::Git) && l2.is_open(Panel::Terminal) && l2.is_open(Panel::Run));
        assert!(!l2.is_open(Panel::Files) && !l2.is_open(Panel::Ai));
        assert_eq!(l2.sizes.right_w, l.sizes.right_w);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn database_restores_ai_after_apply_45_99() {
        let mut l = Layout::default();
        l.open(Panel::Ai);
        l.open(Panel::Database);
        let snap = Snapshot::capture(&l, 1);
        let mut l2 = Layout::default();
        snap.apply(&mut l2);
        assert!(l2.is_open(Panel::Database));
        l2.close(Panel::Database);
        assert!(l2.is_open(Panel::Ai));
    }

    #[test]
    fn pins_toggle_and_order_30() {
        let dir = tmp("pins");
        let mut s = PinStore::open(&dir.join("pins.json"));
        assert!(s.toggle("/p", Path::new("/p/a.rs")).unwrap());
        assert!(s.toggle("/p", Path::new("/p/b.rs")).unwrap());
        assert_eq!(s.first("/p"), Some(Path::new("/p/a.rs")));
        assert!(!s.toggle("/p", Path::new("/p/a.rs")).unwrap());
        let s2 = PinStore::open(&dir.join("pins.json"));
        assert_eq!(s2.pins("/p"), [PathBuf::from("/p/b.rs")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
