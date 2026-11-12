//! Run sağlayıcıları: her biri bir kökte çalıştırılabilir öğeleri bulup tek bir grup döndürür.

mod node;
mod spring;
mod walk;

use std::path::Path;

pub use node::{NodeProvider, ordered_scripts, package_manager};
pub use spring::{SpringProvider, main_class};
pub(crate) use walk::find_upward as walk_find_upward;
pub use walk::{find_files, find_upward};

use crate::model::{Child, Group};

/// Kaynak türü (Spring, Node, …). Yeni sağlayıcı = bu trait'i uygulayan tip + `default_providers`'a ekleme.
pub trait Provider: Send + Sync {
    /// Kökte bulduğu öğelerin grubu; hiç öğe yoksa `None` ya da boş grup.
    fn discover(&self, root: &Path) -> Option<Group>;
}

/// Spring Boot + Node.js.
pub fn default_providers() -> Vec<Box<dyn Provider>> {
    vec![Box::new(SpringProvider), Box::new(NodeProvider)]
}

/// Sağlayıcıyı her kökte çalıştırıp sonuçları tek grupta birleştirir. Birden fazla kök varsa her üst düzey alt
/// düğümün açıklamasına kök klasörün adı eklenir (`backend-api › …`).
pub fn discover_provider(provider: &dyn Provider, roots: &[impl AsRef<Path>]) -> Option<Group> {
    let mut merged: Option<Group> = None;
    for root in roots {
        let root = root.as_ref();
        let Some(mut group) = provider.discover(root) else { continue };
        if group.children.is_empty() {
            continue;
        }
        if roots.len() > 1 {
            let base = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            for child in &mut group.children {
                let detail = match child {
                    Child::Group(g) => &mut g.detail,
                    Child::Item(i) => &mut i.detail,
                };
                *detail = Some(match detail.take() {
                    Some(d) => format!("{base} › {d}"),
                    None => base.clone(),
                });
            }
        }
        match &mut merged {
            Some(m) => m.children.extend(group.children),
            None => merged = Some(group),
        }
    }
    merged
}

/// Tüm sağlayıcıları kökler üzerinde çalıştırır; boş gruplar atlanır.
pub fn discover(providers: &[Box<dyn Provider>], roots: &[impl AsRef<Path>]) -> Vec<Group> {
    providers.iter().filter_map(|p| discover_provider(p.as_ref(), roots)).collect()
}
