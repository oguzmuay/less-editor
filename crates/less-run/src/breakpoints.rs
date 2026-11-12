//! Breakpoint deposu: proje köküne göre `{ file, line, condition? }`, `breakpoints.json`'da
//! (`{ "version": 1, "projects": { "<kök>": [ … ] } }`).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dap::types::SourceBreakpoint;

/// Tek breakpoint (satır 1 tabanlı).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BreakpointEntry {
    pub file: PathBuf,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileData {
    version: u32,
    projects: BTreeMap<String, Vec<BreakpointEntry>>,
}

/// Bir projenin breakpoint'leri.
#[derive(Debug, Clone)]
pub struct BreakpointStore {
    path: PathBuf,
    project: String,
    list: Vec<BreakpointEntry>,
}

fn read(path: &Path) -> FileData {
    fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

impl BreakpointStore {
    /// Varsayılan dosyada `project` kökünün kaydı.
    pub fn open_default(project: &Path) -> BreakpointStore {
        BreakpointStore::open(crate::paths::breakpoints_file(), project)
    }

    pub fn open(path: PathBuf, project: &Path) -> BreakpointStore {
        let project = project.display().to_string();
        let list = read(&path).projects.remove(&project).unwrap_or_default();
        BreakpointStore { path, project, list }
    }

    /// Tümü (dosya, satır sırasıyla).
    pub fn all(&self) -> &[BreakpointEntry] {
        &self.list
    }

    /// Dosyanın satırları.
    pub fn for_file(&self, file: &Path) -> Vec<SourceBreakpoint> {
        self.list
            .iter()
            .filter(|b| b.file == file)
            .map(|b| SourceBreakpoint { line: b.line, condition: b.condition.clone() })
            .collect()
    }

    /// Dosyaya göre gruplu (DAP `setBreakpoints` için).
    pub fn by_file(&self) -> BTreeMap<PathBuf, Vec<SourceBreakpoint>> {
        let mut out: BTreeMap<PathBuf, Vec<SourceBreakpoint>> = BTreeMap::new();
        for b in &self.list {
            out.entry(b.file.clone())
                .or_default()
                .push(SourceBreakpoint { line: b.line, condition: b.condition.clone() });
        }
        out
    }

    /// Satırda varsa kaldırır, yoksa ekler; yeni durumu (var mı) döndürür ve kaydeder.
    pub fn toggle(&mut self, file: &Path, line: u32) -> io::Result<bool> {
        let before = self.list.len();
        self.list.retain(|b| !(b.file == file && b.line == line));
        let added = self.list.len() == before;
        if added {
            self.list.push(BreakpointEntry { file: file.to_path_buf(), line, condition: None });
            self.list.sort();
        }
        self.save()?;
        Ok(added)
    }

    /// Koşul atar (boş = koşulsuz); breakpoint yoksa ekler.
    pub fn set_condition(&mut self, file: &Path, line: u32, condition: Option<&str>) -> io::Result<()> {
        let condition = condition.map(str::trim).filter(|c| !c.is_empty()).map(String::from);
        match self.list.iter_mut().find(|b| b.file == file && b.line == line) {
            Some(b) => b.condition = condition,
            None => {
                self.list.push(BreakpointEntry { file: file.to_path_buf(), line, condition });
                self.list.sort();
            }
        }
        self.save()
    }

    /// Dosyanın tüm breakpoint'lerini siler.
    pub fn clear_file(&mut self, file: &Path) -> io::Result<()> {
        self.list.retain(|b| b.file != file);
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        let mut data = read(&self.path);
        data.version = 1;
        if self.list.is_empty() {
            data.projects.remove(&self.project);
        } else {
            data.projects.insert(self.project.clone(), self.list.clone());
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&data).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, &self.path)
    }
}
