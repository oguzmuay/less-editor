//! Notlar deposu (#98): proje başına `.less/notes/*.md`, klasörde `.gitignore` = `*`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub path: PathBuf,
    pub name: String,
}

/// Proje not klasörü; yoksa oluşturur ve `.gitignore` yazar.
pub fn dir(project: &Path) -> std::io::Result<PathBuf> {
    let dir = project.join(".less").join("notes");
    std::fs::create_dir_all(&dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(ignore, "*\n")?;
    }
    Ok(dir)
}

/// Notlar oluşturulma sırasıyla (eşitlikte ada göre).
pub fn list(project: &Path) -> std::io::Result<Vec<Note>> {
    let dir = dir(project)?;
    let mut items: Vec<(SystemTime, Note)> = Vec::new();
    for e in std::fs::read_dir(&dir)?.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let meta = e.metadata()?;
        let born = meta.created().or_else(|_| meta.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        items.push((born, Note { path, name }));
    }
    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    Ok(items.into_iter().map(|(_, n)| n).collect())
}

/// Ad → dosya adı: kırpılır, `/ \ :` ve denetim karakterleri `-` olur.
pub fn safe_name(name: &str) -> String {
    name.trim().chars().map(|c| if matches!(c, '/' | '\\' | ':') || c.is_control() { '-' } else { c }).collect()
}

/// Notun dosya yolu.
pub fn path_for(project: &Path, name: &str) -> std::io::Result<PathBuf> {
    Ok(dir(project)?.join(format!("{}.md", safe_name(name))))
}

/// Yeni not oluşturur. Ad boşsa "Note N" (ilk boş sayı) verilir; aynı ad varsa mevcut yol döner.
pub fn create(project: &Path, name: Option<&str>) -> std::io::Result<Note> {
    let dir = dir(project)?;
    let name = match name.map(safe_name).filter(|n| !n.is_empty()) {
        Some(n) => n,
        None => {
            (1..).map(|i| format!("Note {i}")).find(|n| !dir.join(format!("{n}.md")).exists()).expect("sınırsız aralık")
        }
    };
    let path = dir.join(format!("{name}.md"));
    if !path.exists() {
        std::fs::write(&path, "")?;
    }
    Ok(Note { path, name })
}

/// Notu siler.
pub fn delete(note: &Note) -> std::io::Result<()> {
    std::fs::remove_file(&note.path)
}

/// Notu yeniden adlandırır; hedef varsa hata.
pub fn rename(note: &Note, new_name: &str) -> std::io::Result<Note> {
    let name = safe_name(new_name);
    if name.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Enter a note name."));
    }
    let path = note.path.with_file_name(format!("{name}.md"));
    if path.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "A note with that name already exists."));
    }
    std::fs::rename(&note.path, &path)?;
    Ok(Note { path, name })
}

/// Proje başına son açık not (yalnız bellekte).
#[derive(Default)]
pub struct LastNotes(HashMap<PathBuf, PathBuf>);

impl LastNotes {
    pub fn set(&mut self, project: &Path, note: &Path) {
        self.0.insert(project.to_path_buf(), note.to_path_buf());
    }

    /// Hatırlanan not hâlâ varsa o, yoksa listedeki ilk not.
    pub fn current(&self, project: &Path) -> Option<Note> {
        let notes = list(project).ok()?;
        let last = self.0.get(project);
        notes.iter().find(|n| Some(&n.path) == last).or(notes.first()).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("less-rs-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dir_has_gitignore_98() {
        let p = tmp("gi");
        let d = dir(&p).unwrap();
        assert_eq!(std::fs::read_to_string(d.join(".gitignore")).unwrap(), "*\n");
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn create_numbers_and_sanitizes_98() {
        let p = tmp("cr");
        assert_eq!(create(&p, None).unwrap().name, "Note 1");
        assert_eq!(create(&p, None).unwrap().name, "Note 2");
        assert_eq!(create(&p, Some(" a/b:c ")).unwrap().name, "a-b-c");
        assert_eq!(list(&p).unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn rename_and_delete_98() {
        let p = tmp("rn");
        let a = create(&p, Some("a")).unwrap();
        create(&p, Some("b")).unwrap();
        assert!(rename(&a, "b").is_err());
        let c = rename(&a, "c").unwrap();
        assert!(c.path.exists() && !a.path.exists());
        delete(&c).unwrap();
        assert_eq!(list(&p).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn last_note_per_project_98() {
        let p = tmp("last");
        let a = create(&p, Some("a")).unwrap();
        let b = create(&p, Some("b")).unwrap();
        let mut last = LastNotes::default();
        assert!(last.current(&p).is_some());
        last.set(&p, &b.path);
        assert_eq!(last.current(&p).unwrap().name, "b");
        delete(&b).unwrap();
        assert_eq!(last.current(&p).unwrap(), a);
        let _ = std::fs::remove_dir_all(&p);
    }
}
