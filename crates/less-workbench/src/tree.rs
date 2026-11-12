//! Files paneli modeli: bir ya da birden çok kökten tembel (açıldıkça okunan) dosya ağacı. UI bilmez;
//! `rows()` çizilecek satırları sırayla verir.

use std::path::{Path, PathBuf};

/// Ağaçta hiç gösterilmeyen adlar.
const HIDDEN: [&str; 2] = [".git", ".DS_Store"];

#[derive(Debug)]
struct Node {
    path: PathBuf,
    name: String,
    is_dir: bool,
    expanded: bool,
    children: Option<Vec<Node>>,
}

impl Node {
    fn new(path: PathBuf, is_dir: bool) -> Self {
        let name =
            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        Self { path, name, is_dir, expanded: false, children: None }
    }

    fn load(&mut self) {
        if self.children.is_none() {
            self.children = Some(read_dir(&self.path));
        }
    }

    fn find_mut(&mut self, path: &Path) -> Option<&mut Node> {
        if self.path == path {
            return Some(self);
        }
        if !path.starts_with(&self.path) {
            return None;
        }
        self.children.as_mut()?.iter_mut().find_map(|c| c.find_mut(path))
    }
}

/// Klasörü okur: önce klasörler, sonra dosyalar; ikisi de büyük/küçük harf duyarsız alfabetik.
fn read_dir(dir: &Path) -> Vec<Node> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut nodes: Vec<Node> = entries
        .flatten()
        .filter(|e| !HIDDEN.contains(&e.file_name().to_string_lossy().as_ref()))
        .map(|e| {
            let path = e.path();
            let is_dir = path.is_dir();
            Node::new(path, is_dir)
        })
        .collect();
    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    nodes
}

/// Çizilecek tek satır.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
}

pub struct FileTree {
    roots: Vec<Node>,
}

impl FileTree {
    /// Kökler açık başlar (tek kök de çok kök de).
    pub fn new(roots: &[PathBuf]) -> Self {
        let roots = roots
            .iter()
            .map(|r| {
                let mut n = Node::new(r.clone(), true);
                n.expanded = true;
                n.load();
                n
            })
            .collect();
        Self { roots }
    }

    /// Klasörü açar / kapatır; dosyaysa bir şey yapmaz.
    pub fn toggle(&mut self, path: &Path) {
        if let Some(node) = self.roots.iter_mut().find_map(|r| r.find_mut(path))
            && node.is_dir
        {
            node.expanded = !node.expanded;
            if node.expanded {
                node.load();
            }
        }
    }

    /// Klasörü açar (`true`) ya da kapatır; zaten o durumdaysa ya da dosyaysa bir şey yapmaz.
    pub fn set_expanded(&mut self, path: &Path, expanded: bool) {
        if let Some(node) = self.roots.iter_mut().find_map(|r| r.find_mut(path))
            && node.is_dir
            && node.expanded != expanded
        {
            node.expanded = expanded;
            if expanded {
                node.load();
            }
        }
    }

    /// Yolun üst klasörlerini kökten aşağı açar (dosya ağaçta görünür olsun diye). Yol hiçbir kökün altında değilse
    /// `false`.
    pub fn reveal(&mut self, path: &Path) -> bool {
        let Some(root) = self.roots.iter().map(|r| r.path.clone()).find(|r| path.starts_with(r)) else { return false };
        let mut chain: Vec<&Path> = path.ancestors().skip(1).take_while(|a| a.starts_with(&root)).collect();
        chain.reverse();
        for dir in chain {
            self.set_expanded(dir, true);
        }
        true
    }

    /// Görünen satırlar (açık klasörlerin içi dahil), yukarıdan aşağı.
    pub fn rows(&self) -> Vec<Row> {
        fn walk(n: &Node, depth: usize, out: &mut Vec<Row>) {
            out.push(Row { path: n.path.clone(), name: n.name.clone(), depth, is_dir: n.is_dir, expanded: n.expanded });
            if n.expanded
                && let Some(children) = &n.children
            {
                children.iter().for_each(|c| walk(c, depth + 1, out));
            }
        }
        let mut out = Vec::new();
        self.roots.iter().for_each(|r| walk(r, 0, &mut out));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_first_hidden_skipped_and_toggle() {
        let root = std::env::temp_dir().join(format!("less-rs-tree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/inner")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("b.txt"), "").unwrap();
        std::fs::write(root.join("A.md"), "").unwrap();
        std::fs::write(root.join("src/main.rs"), "").unwrap();

        let mut tree = FileTree::new(std::slice::from_ref(&root));
        let names: Vec<String> = tree.rows().into_iter().map(|r| r.name).collect();
        assert_eq!(&names[1..], ["src", "A.md", "b.txt"]);

        tree.toggle(&root.join("src"));
        let rows = tree.rows();
        assert_eq!(rows[2].name, "inner");
        assert_eq!(rows[2].depth, 2);
        assert_eq!(rows[3].name, "main.rs");

        tree.toggle(&root.join("src"));
        assert_eq!(tree.rows().len(), 4);
        assert!(tree.reveal(&root.join("src/inner/x")));
        assert!(tree.rows().iter().any(|r| r.name == "inner" && r.expanded));
        assert!(!tree.reveal(Path::new("/elsewhere/file")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
