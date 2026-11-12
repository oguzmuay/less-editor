//! Run ağacının veri modeli: sağlayıcıların ürettiği gruplar ve çalıştırılabilir öğeler.

use std::path::PathBuf;

/// Spring uygulamasının derleme aracı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTool {
    Maven,
    Gradle,
}

/// Node paket yöneticisi (lock dosyasından).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    /// Komut adı (`npm`, `pnpm`, `yarn`, `bun`).
    pub fn command(self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        }
    }
}

/// Öğenin türü; komut, hazır olma deseni ve debug davranışı buradan çıkar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    /// `@SpringBootApplication` sınıfı. `file` kaynak dosya, `module_dir` en yakın pom/gradle klasörü (= cwd).
    Spring { tool: BuildTool, file: PathBuf },
    /// package.json script'i.
    Node { pm: PackageManager, script: String },
}

/// Grup düğümünün türü (ikon / renk seçimi UI'da).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Spring,
    Node,
    Package,
}

/// Sağlayıcının bulduğu çalıştırılabilir öğe (durum bilgisi yok; durum `Runner`'da).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSpec {
    /// Kalıcı kimlik (LESS-nvim ile aynı: `spring:<dosya>`, `node:<klasör>:<script>`); run_configs.json anahtarı.
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub cwd: PathBuf,
    pub kind: ItemKind,
}

impl ItemSpec {
    /// Profil seçimi destekleniyor mu (Spring).
    pub fn supports_profiles(&self) -> bool {
        matches!(self.kind, ItemKind::Spring { .. })
    }

    /// VM seçeneklerinin verildiği ortam değişkeni (Node: `NODE_OPTIONS`); `None` ise komut satırında.
    pub fn vm_env(&self) -> Option<&'static str> {
        match self.kind {
            ItemKind::Node { .. } => Some("NODE_OPTIONS"),
            ItemKind::Spring { .. } => None,
        }
    }

    /// Debug hedefine otomatik DAP attach yapılabilir mi. Spring Kotlin ana sınıfında JDWP portu açılır ama
    /// attach yapılmaz (jdtls Kotlin bilmiyor).
    pub fn can_attach(&self) -> bool {
        match &self.kind {
            ItemKind::Spring { file, .. } => file.extension().is_some_and(|e| e == "java"),
            ItemKind::Node { .. } => true,
        }
    }
}

/// Ağaçtaki alt düğüm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Child {
    Group(Group),
    Item(ItemSpec),
}

/// Grup düğümü (Spring Boot, Node.js, paket).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub kind: GroupKind,
    pub children: Vec<Child>,
}

impl Group {
    /// Altındaki tüm öğeler (derinlik öncelikli, ağaç sırasıyla).
    pub fn items(&self) -> Vec<&ItemSpec> {
        let mut out = Vec::new();
        fn walk<'a>(g: &'a Group, out: &mut Vec<&'a ItemSpec>) {
            for c in &g.children {
                match c {
                    Child::Group(g) => walk(g, out),
                    Child::Item(i) => out.push(i),
                }
            }
        }
        walk(self, &mut out);
        out
    }
}
