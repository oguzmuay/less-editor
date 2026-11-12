//! Spring Boot sağlayıcısı: `@SpringBootApplication` içeren Java/Kotlin sınıfları.

use std::fs;
use std::path::Path;

use super::Provider;
use super::walk::{find_files, root_with};
use crate::model::{BuildTool, Child, Group, GroupKind, ItemKind, ItemSpec};

/// Spring Boot sağlayıcısı. `target/`, `build/`, `node_modules/` ve gizli klasörler taranmaz.
pub struct SpringProvider;

impl Provider for SpringProvider {
    fn discover(&self, root: &Path) -> Option<Group> {
        let files = find_files(root, &["target", "build", "node_modules"], &|p| {
            p.extension().is_some_and(|e| e == "java" || e == "kt")
                && fs::read_to_string(p).is_ok_and(|t| t.contains("@SpringBootApplication"))
        });
        let mut items: Vec<ItemSpec> = files
            .into_iter()
            .filter_map(|file| {
                let module = root_with(file.parent()?, &["pom.xml", "build.gradle", "build.gradle.kts"])?;
                let tool = if module.join("pom.xml").exists() { BuildTool::Maven } else { BuildTool::Gradle };
                let label = file.file_stem()?.to_string_lossy().into_owned();
                let module_name = module.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let tool_name = match tool {
                    BuildTool::Maven => "maven",
                    BuildTool::Gradle => "gradle",
                };
                Some(ItemSpec {
                    id: format!("spring:{}", file.display()),
                    label,
                    detail: Some(format!("{tool_name} · {module_name}")),
                    cwd: module,
                    kind: ItemKind::Spring { tool, file },
                })
            })
            .collect();
        items.sort_by(|a, b| a.label.cmp(&b.label));
        Some(Group {
            id: "spring".into(),
            label: "Spring Boot".into(),
            detail: None,
            kind: GroupKind::Spring,
            children: items.into_iter().map(Child::Item).collect(),
        })
    }
}

/// Kaynak dosyadaki `package` satırından (ilk 200 satır) sınıfın tam adını çıkarır.
pub fn main_class(file: &Path) -> String {
    let name = file.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let text = fs::read_to_string(file).unwrap_or_default();
    for line in text.lines().take(200) {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("package")
            && rest.starts_with(char::is_whitespace)
        {
            let pkg = rest.trim().trim_end_matches(';').trim();
            if !pkg.is_empty() && pkg.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                return format!("{pkg}.{name}");
            }
        }
    }
    name
}
