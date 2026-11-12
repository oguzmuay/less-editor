//! Dil sunucusu kaydı: hangi dil için hangi ikili, hangi argümanlar, proje kökü nasıl bulunur.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::java;
use crate::language::Language;

/// Sunucu ikilisi arama ve önbellek ayarları. Testlerde geçici klasörlerle doldurulur.
#[derive(Debug, Clone)]
pub struct LspConfig {
    /// PATH klasörleri (varsayılan `$PATH`).
    pub path_dirs: Vec<PathBuf>,
    /// PATH'ten sonra bakılan klasörler: LESS-rs'in kurduğu sunucular (`servers_dir()/bin`), LESS ve nvim Mason
    /// kurulumları (yalnız okunur).
    pub extra_bin_dirs: Vec<PathBuf>,
    /// `~/.cache/less-rs` (jdtls çalışma klasörleri burada).
    pub cache_dir: PathBuf,
    pub jdk_search: java::JdkSearch,
}

impl Default for LspConfig {
    fn default() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let mut path_dirs: Vec<PathBuf> =
            std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        // Finder/Dock'tan açılan uygulamanın PATH'i `/usr/bin:/bin:…`; `#!/usr/bin/env node` sunucular node'u bulamazdı.
        for dir in
            [home.join(".local/bin"), "/opt/homebrew/bin".into(), "/usr/local/bin".into(), home.join(".volta/bin")]
        {
            if !path_dirs.contains(&dir) {
                path_dirs.push(dir);
            }
        }
        LspConfig {
            path_dirs,
            extra_bin_dirs: vec![
                crate::servers::servers_dir().join("bin"),
                home.join(".local/share/less/nvim/mason/bin"),
                home.join(".local/share/nvim/mason/bin"),
            ],
            cache_dir: home.join(".cache/less-rs"),
            jdk_search: java::JdkSearch::default(),
        }
    }
}

impl LspConfig {
    /// İkiliyi önce PATH'te, sonra Mason klasörlerinde arar.
    pub fn find_binary(&self, name: &str) -> Option<PathBuf> {
        self.path_dirs.iter().chain(&self.extra_bin_dirs).map(|d| d.join(name)).find(|p| is_executable(p))
    }

    /// Sunucu sürecinin `PATH`'i: [`LspConfig::path_dirs`] + Mason klasörleri (betik sunucular `node`, `python3`,
    /// `java` gibi yorumlayıcıları ve kardeş ikilileri buradan bulur).
    pub fn child_path(&self) -> std::ffi::OsString {
        std::env::join_paths(self.path_dirs.iter().chain(&self.extra_bin_dirs)).unwrap_or_default()
    }

    /// Mason'ın `share` klasörü (jdtls bundle'ları, lombok): bulunan ilk `mason/bin`'in kardeşi.
    pub fn mason_share(&self) -> Option<PathBuf> {
        self.extra_bin_dirs.iter().filter_map(|b| b.parent()).map(|m| m.join("share")).find(|s| s.is_dir())
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Bir dil sunucusunun tanımı.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerSpec {
    /// Kararlı kimlik (`"jdtls"`, `"vtsls"`…); olaylarda ve diagnostik kaynağında kullanılır.
    pub id: &'static str,
    /// Aday ikililer, öncelik sırasıyla (ilk bulunan kullanılır) ve argümanları.
    pub candidates: &'static [(&'static str, &'static [&'static str])],
    /// Kök işaretleri (yukarı doğru ilk eşleşen klasör); hiçbiri yoksa `.git`, o da yoksa dosyanın klasörü.
    pub root_markers: &'static [&'static str],
    /// Kurulum ipucu ("server not found" olayında gösterilir).
    pub install_hint: &'static str,
}

const LOCKS: &[&str] = &["package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb", "bun.lock"];

/// Dil için sunucu tanımı; sunucusu olmayan diller (SQL, TOML…) için `None`.
pub fn server_for(lang: Language) -> Option<ServerSpec> {
    use Language as L;
    Some(match lang {
        L::Java => ServerSpec {
            id: "jdtls",
            candidates: &[("jdtls", &[])],
            root_markers: &[],
            install_hint: "Install jdtls (brew install jdtls) and Java 25+ (brew install openjdk)",
        },
        L::TypeScript | L::Tsx | L::JavaScript | L::Jsx => ServerSpec {
            id: "vtsls",
            candidates: &[("vtsls", &["--stdio"]), ("typescript-language-server", &["--stdio"])],
            root_markers: LOCKS,
            install_hint: "Install vtsls (npm i -g @vtsls/language-server)",
        },
        L::Json => ServerSpec {
            id: "jsonls",
            candidates: &[("vscode-json-language-server", &["--stdio"])],
            root_markers: &["package.json"],
            install_hint: "Install vscode-langservers-extracted (npm i -g vscode-langservers-extracted)",
        },
        L::Yaml => ServerSpec {
            id: "yamlls",
            candidates: &[("yaml-language-server", &["--stdio"])],
            root_markers: &[],
            install_hint: "Install yaml-language-server (npm i -g yaml-language-server)",
        },
        L::Lua => ServerSpec {
            id: "lua_ls",
            candidates: &[("lua-language-server", &[])],
            root_markers: &[".luarc.json", ".luarc.jsonc", "stylua.toml"],
            install_hint: "Install lua-language-server (brew install lua-language-server)",
        },
        L::Rust => ServerSpec {
            id: "rust_analyzer",
            candidates: &[("rust-analyzer", &[])],
            root_markers: &["Cargo.lock", "Cargo.toml"],
            install_hint: "Install rust-analyzer (rustup component add rust-analyzer)",
        },
        L::Markdown => ServerSpec {
            id: "marksman",
            candidates: &[("marksman", &["server"])],
            root_markers: &[".marksman.toml"],
            install_hint: "Install marksman (brew install marksman)",
        },
        L::Bash => ServerSpec {
            id: "bashls",
            candidates: &[("bash-language-server", &["start"])],
            root_markers: &[],
            install_hint: "Install bash-language-server (npm i -g bash-language-server)",
        },
        L::Css => ServerSpec {
            id: "cssls",
            candidates: &[("vscode-css-language-server", &["--stdio"])],
            root_markers: &["package.json"],
            install_hint: "Install vscode-langservers-extracted (npm i -g vscode-langservers-extracted)",
        },
        L::Html => ServerSpec {
            id: "html",
            candidates: &[("vscode-html-language-server", &["--stdio"])],
            root_markers: &["package.json"],
            install_hint: "Install vscode-langservers-extracted (npm i -g vscode-langservers-extracted)",
        },
        L::Toml => ServerSpec {
            id: "taplo",
            candidates: &[("taplo", &["lsp", "stdio"])],
            root_markers: &[],
            install_hint: "Install taplo (brew install taplo)",
        },
        L::Kotlin => ServerSpec {
            id: "kotlin_ls",
            candidates: &[("kotlin-language-server", &[]), ("kotlin-lsp", &["--stdio"])],
            root_markers: &["settings.gradle.kts", "settings.gradle", "pom.xml", "build.gradle.kts", "build.gradle"],
            install_hint: "Install it from :options (Language Servers → Kotlin)",
        },
        L::Go => ServerSpec {
            id: "gopls",
            candidates: &[("gopls", &[])],
            root_markers: &["go.work", "go.mod"],
            install_hint: "Install it from :options (Language Servers → gopls)",
        },
        L::Python => ServerSpec {
            id: "basedpyright",
            candidates: &[("basedpyright-langserver", &["--stdio"]), ("pyright-langserver", &["--stdio"])],
            root_markers: &[
                "pyproject.toml",
                "setup.py",
                "setup.cfg",
                "requirements.txt",
                "Pipfile",
                "pyrightconfig.json",
            ],
            install_hint: "Install it from :options (Language Servers → basedpyright)",
        },
        L::Sql => return None,
    })
}

const ESLINT_CONFIGS: &[&str] = &[
    "eslint.config.js",
    "eslint.config.mjs",
    "eslint.config.cjs",
    "eslint.config.ts",
    "eslint.config.mts",
    "eslint.config.cts",
    ".eslintrc",
    ".eslintrc.js",
    ".eslintrc.cjs",
    ".eslintrc.json",
    ".eslintrc.yaml",
    ".eslintrc.yml",
];

const TAILWIND_CONFIGS: &[&str] = &[
    "tailwind.config.js",
    "tailwind.config.cjs",
    "tailwind.config.mjs",
    "tailwind.config.ts",
    "tailwind.config.cts",
    "tailwind.config.mts",
];

fn eslint() -> ServerSpec {
    ServerSpec {
        id: "eslint",
        candidates: &[("vscode-eslint-language-server", &["--stdio"])],
        root_markers: ESLINT_CONFIGS,
        install_hint: "Install vscode-langservers-extracted (npm i -g vscode-langservers-extracted)",
    }
}

fn tailwind() -> ServerSpec {
    ServerSpec {
        id: "tailwindcss",
        candidates: &[("tailwindcss-language-server", &["--stdio"])],
        root_markers: TAILWIND_CONFIGS,
        install_hint: "Install it from :options (Language Servers → Tailwind CSS)",
    }
}

/// Ana sunucuya ek sunucular (ESLint diagnostik, Tailwind tamamlama + diagnostik). Kökleri **yalnız** işaretten
/// bulunur ([`extra_root`]; yapılandırması olmayan projede başlatılmaz). Belge eşitlemesi ve tamamlama alırlar,
/// diğer istekler (hover, tanım…) yalnız ana sunucuya gider.
pub fn linters_for(lang: Language) -> Vec<ServerSpec> {
    use Language as L;
    match lang {
        L::TypeScript | L::Tsx | L::JavaScript | L::Jsx => vec![eslint(), tailwind()],
        L::Css | L::Html => vec![tailwind()],
        _ => Vec::new(),
    }
}

/// Ek sunucunun kökü: işaret dosyası; Tailwind'de ayrıca `tailwindcss` bağımlılığı olan en yakın `package.json`
/// (Tailwind v4'te yapılandırma dosyası yok).
pub fn extra_root(spec: &ServerSpec, file: &Path) -> Option<PathBuf> {
    let marked = java::find_root_with(file, spec.root_markers);
    if marked.is_some() || spec.id != "tailwindcss" {
        return marked;
    }
    file.ancestors()
        .skip(1)
        .find(|dir| std::fs::read_to_string(dir.join("package.json")).is_ok_and(|s| s.contains("\"tailwindcss\"")))
        .map(Path::to_path_buf)
}

/// Dosyanın proje kökü: Java'da [`java::java_root`], diğerlerinde kök işaretleri → `.git` → dosyanın klasörü.
pub fn root_for(spec: &ServerSpec, file: &Path) -> PathBuf {
    let fallback = || {
        java::find_root_with(file, &[".git"])
            .or_else(|| file.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("/"))
    };
    if spec.id == "jdtls" {
        return java::java_root(file).unwrap_or_else(fallback);
    }
    if spec.id == "rust_analyzer" {
        let mut root = java::find_root_with(file, &["Cargo.toml"]);
        while let Some(dir) = root.clone() {
            match dir.parent().and_then(|p| java::find_root_with(p, &["Cargo.toml"])) {
                Some(up) if up.join("Cargo.lock").exists() || has_workspace(&up) => root = Some(up),
                _ => break,
            }
        }
        return root.unwrap_or_else(fallback);
    }
    java::find_root_with(file, spec.root_markers).unwrap_or_else(fallback)
}

fn has_workspace(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|s| s.contains("[workspace]"))
}

/// Başlatılacak somut komut ve sunucuya özel ayarlar.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// `workspace/configuration` ve `didChangeConfiguration` ile verilen ayarlar.
    pub settings: Value,
    pub init_options: Value,
    /// Kullanıcıya gösterilecek uyarı (ör. jdtls için Java 25+ yok).
    pub warning: Option<String>,
}

/// İkiliyi bulup komutu kurar; ikili yoksa `None` ("server not found").
pub fn launch_for(spec: &ServerSpec, root: &Path, config: &LspConfig) -> Option<Launch> {
    let (program, args) = spec.candidates.iter().find_map(|(bin, args)| config.find_binary(bin).map(|p| (p, args)))?;
    let mut launch = Launch {
        program,
        args: args.iter().map(|a| a.to_string()).collect(),
        settings: Value::Object(Default::default()),
        init_options: Value::Null,
        warning: None,
    };
    if spec.id == "jdtls" {
        let jdks = java::find_jdks(&config.jdk_search);
        let j = java::jdtls_launch(root, &config.cache_dir, &jdks, config.mason_share().as_deref());
        launch.args = j.args;
        launch.settings = j.settings;
        launch.init_options = j.init_options;
        if j.missing_java {
            launch.warning = Some(format!(
                "jdtls requires Java {}+ but none was found. Install: brew install openjdk",
                java::JDTLS_MIN_JAVA
            ));
        }
    } else if spec.id == "eslint" {
        launch.settings = eslint_settings(root);
    } else if spec.id == "tailwindcss" {
        launch.settings = json!({ "tailwindCSS": {
            "validate": true,
            "classAttributes": ["class", "className", "class:list", "classList", "ngClass"],
            "lint": {
                "cssConflict": "warning", "invalidApply": "error", "invalidConfigPath": "error",
                "invalidScreen": "error", "invalidTailwindDirective": "error", "invalidVariant": "error",
                "recommendedVariantOrder": "warning"
            }
        }});
    } else if spec.id == "vtsls" {
        launch.settings = json!({ "typescript": { "updateImportsOnFileMove": { "enabled": "always" } } });
    }
    Some(launch)
}

/// vscode-eslint sunucusunun `workspace/configuration` yanıtı (alanların hepsi beklenir; eksikte sunucu çöker).
/// Değerler nvim-lspconfig `eslint` ile aynı; flat config kökte `eslint.config.*` varsa.
fn eslint_settings(root: &Path) -> Value {
    let flat = ESLINT_CONFIGS.iter().filter(|f| f.starts_with("eslint.config")).any(|f| root.join(f).exists());
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    json!({
        "validate": "on",
        "packageManager": null,
        "useESLintClass": false,
        "useFlatConfig": flat,
        "experimental": { "useFlatConfig": flat },
        "codeActionOnSave": { "enable": false, "mode": "all" },
        "format": false,
        "quiet": false,
        "onIgnoredFiles": "off",
        "options": {},
        "rulesCustomizations": [],
        "run": "onType",
        "problems": { "shortenToSingleLine": false },
        "nodePath": "",
        "workingDirectory": { "mode": "location" },
        "workspaceFolder": { "uri": super::uri::path_to_uri(root), "name": name },
        "codeAction": {
            "disableRuleComment": { "enable": true, "location": "separateLine" },
            "showDocumentation": { "enable": true }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn exe(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn finds_binary_in_path_then_mason() {
        let tmp = tempfile::tempdir().unwrap();
        let path_dir = tmp.path().join("bin");
        let mason = tmp.path().join("mason/bin");
        exe(&mason.join("vtsls"));
        exe(&path_dir.join("typescript-language-server"));
        fs::create_dir_all(&path_dir).unwrap();
        fs::write(path_dir.join("marksman"), "not executable").unwrap();
        let config = LspConfig {
            path_dirs: vec![path_dir.clone()],
            extra_bin_dirs: vec![mason.clone()],
            cache_dir: tmp.path().join("cache"),
            jdk_search: java::JdkSearch { homebrew_opt: vec![], jvm_dirs: vec![] },
        };
        assert_eq!(config.find_binary("vtsls"), Some(mason.join("vtsls")));
        assert_eq!(config.find_binary("marksman"), None);
        let child: Vec<PathBuf> = std::env::split_paths(&config.child_path()).collect();
        assert_eq!(child, [path_dir.clone(), mason.clone()], "server PATH = PATH dirs + Mason (node scripts)");
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert!(LspConfig::default().path_dirs.contains(&home.join(".local/bin")), "GUI launch fallback dirs");
        let ts = server_for(Language::Tsx).unwrap();
        let launch = launch_for(&ts, tmp.path(), &config).unwrap();
        assert_eq!(launch.program, mason.join("vtsls"), "vtsls preferred over typescript-language-server");
        assert!(launch_for(&server_for(Language::Markdown).unwrap(), tmp.path(), &config).is_none());
        assert!(server_for(Language::Sql).is_none());

        exe(&mason.join("jdtls"));
        let j = launch_for(&server_for(Language::Java).unwrap(), Path::new("/w/api"), &config).unwrap();
        assert!(j.warning.as_deref().unwrap().contains("Java 25+"));
        assert!(j.args.iter().any(|a| a.contains("generatesMetadataFilesAtProjectRoot=false")));
    }

    #[test]
    fn roots_from_lock_files_and_cargo_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        fs::create_dir_all(t.join(".git")).unwrap();
        fs::create_dir_all(t.join("fe/src")).unwrap();
        fs::write(t.join("fe/pnpm-lock.yaml"), "").unwrap();
        fs::write(t.join("fe/src/package.json"), "").unwrap();
        let ts = server_for(Language::TypeScript).unwrap();
        assert_eq!(root_for(&ts, &t.join("fe/src/a.ts")), t.join("fe"));
        assert_eq!(root_for(&ts, &t.join("other/b.ts")), t.to_path_buf());

        fs::create_dir_all(t.join("rs/crates/x/src")).unwrap();
        fs::write(t.join("rs/Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(t.join("rs/crates/x/Cargo.toml"), "[package]\n").unwrap();
        let ra = server_for(Language::Rust).unwrap();
        assert_eq!(root_for(&ra, &t.join("rs/crates/x/src/lib.rs")), t.join("rs"));
    }
}
