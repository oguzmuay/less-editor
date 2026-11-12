//! Node.js sağlayıcısı: kökteki tüm package.json dosyaları (node_modules hariç), her paket bir alt grup, her script
//! bir öğe.

use std::fs;
use std::path::Path;

use super::Provider;
use super::walk::find_files;
use crate::model::{Child, Group, GroupKind, ItemKind, ItemSpec, PackageManager};

/// Aynı klasörde birden çok lock dosyası varsa öncelik sırası.
const LOCKFILES: &[(&str, PackageManager)] = &[
    ("pnpm-lock.yaml", PackageManager::Pnpm),
    ("yarn.lock", PackageManager::Yarn),
    ("bun.lock", PackageManager::Bun),
    ("bun.lockb", PackageManager::Bun),
    ("package-lock.json", PackageManager::Npm),
];

/// Node.js sağlayıcısı.
pub struct NodeProvider;

/// Paket klasöründen köke kadar (kök dahil) yukarı doğru en yakın lock dosyasının paket yöneticisi; yoksa npm.
pub fn package_manager(dir: &Path, root: &Path) -> PackageManager {
    for d in dir.ancestors() {
        if let Some((_, pm)) = LOCKFILES.iter().find(|(name, _)| d.join(name).is_file()) {
            return *pm;
        }
        if d == root {
            break;
        }
    }
    PackageManager::Npm
}

/// Script adlarını package.json'daki yazılış sırasıyla döndürür (JSON nesnesi sırayı korumadığı için metindeki
/// `"ad":` konumuna bakılır, `"scripts"` anahtarından sonra).
pub fn ordered_scripts(text: &str, names: &[String]) -> Vec<String> {
    let start = text.find("\"scripts\"").unwrap_or(0);
    let pos = |name: &str| -> usize {
        let needle = format!("\"{name}\"");
        let mut from = start;
        while let Some(i) = text[from..].find(&needle) {
            let at = from + i;
            let after = text[at + needle.len()..].trim_start();
            if after.starts_with(':') {
                return at;
            }
            from = at + needle.len();
        }
        usize::MAX
    };
    let mut list: Vec<(usize, String)> = names.iter().map(|n| (pos(n), n.clone())).collect();
    list.sort();
    list.into_iter().map(|(_, n)| n).collect()
}

impl Provider for NodeProvider {
    fn discover(&self, root: &Path) -> Option<Group> {
        let files = find_files(root, &["node_modules"], &|p| p.file_name().is_some_and(|n| n == "package.json"));
        let mut packages: Vec<(String, Group)> = Vec::new();
        for file in files {
            let Ok(text) = fs::read_to_string(&file) else { continue };
            let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let Some(scripts) = data.get("scripts").and_then(|s| s.as_object()).filter(|s| !s.is_empty()) else {
                continue;
            };
            let Some(dir) = file.parent() else { continue };
            let rel = match dir.strip_prefix(root) {
                Ok(r) if r.as_os_str().is_empty() => ".".to_string(),
                Ok(r) => r.display().to_string(),
                Err(_) => dir.display().to_string(),
            };
            let pm = package_manager(dir, root);
            let names: Vec<String> = scripts.keys().cloned().collect();
            let children = ordered_scripts(&text, &names)
                .into_iter()
                .map(|name| {
                    let detail = match &scripts[&name] {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    Child::Item(ItemSpec {
                        id: format!("node:{}:{}", dir.display(), name),
                        label: name.clone(),
                        detail: Some(detail),
                        cwd: dir.to_path_buf(),
                        kind: ItemKind::Node { pm, script: name },
                    })
                })
                .collect();
            let label = match data.get("name").and_then(|n| n.as_str()) {
                Some(n) => n.to_string(),
                None if rel == "." => root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                None => rel.clone(),
            };
            packages.push((
                rel.clone(),
                Group {
                    id: format!("node:{}", dir.display()),
                    label,
                    detail: Some(format!("{rel} · {}", pm.command())),
                    kind: GroupKind::Package,
                    children,
                },
            ));
        }
        packages.sort_by(|a, b| a.0.cmp(&b.0));
        Some(Group {
            id: "node".into(),
            label: "Node.js".into(),
            detail: None,
            kind: GroupKind::Node,
            children: packages.into_iter().map(|(_, g)| Child::Group(g)).collect(),
        })
    }
}
