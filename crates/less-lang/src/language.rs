//! Dil tanımları: yoldan dil tespiti, yorum işaretleri, girinti ipuçları, LSP `languageId`.

use std::path::Path;

/// Gömülü (derlemeye katılmış) tree-sitter grammar'ı olan diller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    Java,
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Json,
    Yaml,
    Markdown,
    Sql,
    Lua,
    Rust,
    Toml,
    Bash,
    Css,
    Html,
    Kotlin,
    Go,
    Python,
}

/// Bir dilin yorum işaretleri (Cmd+/ ve "yorum içinde mi" kararları için).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentTokens {
    /// Satır yorumu öneki (`//`, `#`, `--`); yoksa `None`.
    pub line: Option<&'static str>,
    /// Blok yorum açılış/kapanış çifti.
    pub block: Option<(&'static str, &'static str)>,
}

/// Girinti ipuçları: birim, ve bir satırın sonunda/başında girintiyi değiştiren işaretler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndentHints {
    /// Bir girinti seviyesinin genişliği (boşluk sayısı; `use_tabs` ise sekme başına görüntü genişliği).
    pub width: usize,
    pub use_tabs: bool,
    /// Önceki satır (yorum/boşluk atılmış) bunlardan biriyle bitiyorsa yeni satır bir seviye içeri girer.
    pub increase_after: &'static [&'static str],
    /// Satır (baştaki boşluk atılmış) bunlardan biriyle başlıyorsa bir seviye dışarı çıkar.
    pub decrease_on: &'static [&'static str],
}

const BRACES_IN: &[&str] = &["{", "(", "["];
const BRACES_OUT: &[&str] = &["}", ")", "]"];

impl Language {
    /// Tüm diller (testler ve seçiciler için).
    pub const ALL: [Language; 18] = [
        Language::Java,
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Jsx,
        Language::Json,
        Language::Yaml,
        Language::Markdown,
        Language::Sql,
        Language::Lua,
        Language::Rust,
        Language::Toml,
        Language::Bash,
        Language::Css,
        Language::Html,
        Language::Kotlin,
        Language::Go,
        Language::Python,
    ];

    /// Dosya yolundan dili bulur: önce tam dosya adı (`Dockerfile` yok; `.bashrc`, `Cargo.lock`), sonra uzantı.
    pub fn from_path(path: &Path) -> Option<Language> {
        let name = path.file_name()?.to_str()?;
        match name {
            ".bashrc" | ".bash_profile" | ".zshrc" | ".zprofile" | ".profile" | "PKGBUILD" => {
                return Some(Language::Bash);
            }
            "Cargo.lock" | "uv.lock" | "poetry.lock" => return Some(Language::Toml),
            ".luarc.json" | ".eslintrc" | ".prettierrc" | ".babelrc" => return Some(Language::Json),
            _ => {}
        }
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Self::from_extension(&ext)
    }

    /// Uzantıdan dil (noktasız, küçük harf beklenir; büyük harf de kabul edilir).
    pub fn from_extension(ext: &str) -> Option<Language> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "java" => Language::Java,
            "ts" | "mts" | "cts" => Language::TypeScript,
            "tsx" => Language::Tsx,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "jsx" => Language::Jsx,
            "json" | "jsonc" | "json5" | "code-workspace" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "md" | "markdown" | "mdx" => Language::Markdown,
            "sql" | "psql" | "mysql" | "pgsql" => Language::Sql,
            "lua" => Language::Lua,
            "rs" => Language::Rust,
            "toml" => Language::Toml,
            "sh" | "bash" | "zsh" | "ksh" => Language::Bash,
            "css" | "scss" | "less" => Language::Css,
            "html" | "htm" | "xhtml" | "vue" | "svelte" => Language::Html,
            "kt" | "kts" => Language::Kotlin,
            "go" => Language::Go,
            "py" | "pyi" | "pyw" => Language::Python,
            _ => return None,
        })
    }

    /// Markdown kod bloğu bilgi dizesinden (```` ```ts ````) dil.
    pub fn from_name(name: &str) -> Option<Language> {
        let lower = name.trim().to_ascii_lowercase();
        Some(match lower.as_str() {
            "java" => Language::Java,
            "typescript" | "ts" => Language::TypeScript,
            "tsx" | "typescriptreact" => Language::Tsx,
            "javascript" | "js" | "node" => Language::JavaScript,
            "jsx" | "javascriptreact" => Language::Jsx,
            "json" | "jsonc" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "markdown" | "md" => Language::Markdown,
            "sql" => Language::Sql,
            "lua" => Language::Lua,
            "rust" | "rs" => Language::Rust,
            "toml" => Language::Toml,
            "bash" | "sh" | "shell" | "zsh" | "console" => Language::Bash,
            "css" | "scss" => Language::Css,
            "html" | "xml" => Language::Html,
            "kotlin" | "kt" => Language::Kotlin,
            "go" | "golang" => Language::Go,
            "python" | "py" | "python3" => Language::Python,
            _ => return Self::from_extension(&lower),
        })
    }

    /// Kullanıcıya görünen ad (durum satırı).
    pub fn display_name(self) -> &'static str {
        match self {
            Language::Java => "Java",
            Language::TypeScript => "TypeScript",
            Language::Tsx => "TypeScript JSX",
            Language::JavaScript => "JavaScript",
            Language::Jsx => "JavaScript JSX",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Markdown => "Markdown",
            Language::Sql => "SQL",
            Language::Lua => "Lua",
            Language::Rust => "Rust",
            Language::Toml => "TOML",
            Language::Bash => "Shell",
            Language::Css => "CSS",
            Language::Html => "HTML",
            Language::Kotlin => "Kotlin",
            Language::Go => "Go",
            Language::Python => "Python",
        }
    }

    /// LSP `textDocument.languageId`.
    pub fn lsp_id(self) -> &'static str {
        match self {
            Language::Java => "java",
            Language::TypeScript => "typescript",
            Language::Tsx => "typescriptreact",
            Language::JavaScript => "javascript",
            Language::Jsx => "javascriptreact",
            Language::Json => "json",
            Language::Yaml => "yaml",
            Language::Markdown => "markdown",
            Language::Sql => "sql",
            Language::Lua => "lua",
            Language::Rust => "rust",
            Language::Toml => "toml",
            Language::Bash => "shellscript",
            Language::Css => "css",
            Language::Html => "html",
            Language::Kotlin => "kotlin",
            Language::Go => "go",
            Language::Python => "python",
        }
    }

    /// Yorum işaretleri.
    pub fn comment_tokens(self) -> CommentTokens {
        let c_like = CommentTokens { line: Some("//"), block: Some(("/*", "*/")) };
        match self {
            Language::Java
            | Language::TypeScript
            | Language::Tsx
            | Language::JavaScript
            | Language::Jsx
            | Language::Rust
            | Language::Kotlin
            | Language::Go => c_like,
            Language::Json => CommentTokens { line: Some("//"), block: Some(("/*", "*/")) },
            Language::Yaml | Language::Toml | Language::Bash | Language::Python => {
                CommentTokens { line: Some("#"), block: None }
            }
            Language::Markdown | Language::Html => CommentTokens { line: None, block: Some(("<!--", "-->")) },
            Language::Sql => CommentTokens { line: Some("--"), block: Some(("/*", "*/")) },
            Language::Lua => CommentTokens { line: Some("--"), block: Some(("--[[", "]]")) },
            Language::Css => CommentTokens { line: None, block: Some(("/*", "*/")) },
        }
    }

    /// Girinti ipuçları (biçimlendirici yoksa yeni satır girintisi için).
    pub fn indent_hints(self) -> IndentHints {
        let spaces =
            |width, increase_after, decrease_on| IndentHints { width, use_tabs: false, increase_after, decrease_on };
        match self {
            Language::Java | Language::Rust | Language::Kotlin => spaces(4, BRACES_IN, BRACES_OUT),
            Language::Go => IndentHints { use_tabs: true, ..spaces(4, BRACES_IN, BRACES_OUT) },
            Language::Python => spaces(4, &[":", "(", "[", "{"], &[")", "]", "}"]),
            Language::TypeScript
            | Language::Tsx
            | Language::JavaScript
            | Language::Jsx
            | Language::Json
            | Language::Css
            | Language::Toml => spaces(2, BRACES_IN, BRACES_OUT),
            Language::Yaml => spaces(2, &[":", "|", ">", "- "], &[]),
            Language::Markdown => spaces(2, &[], &[]),
            Language::Sql => spaces(2, &["("], &[")"]),
            Language::Lua => spaces(
                2,
                &["then", "do", "else", "function()", "{", "(", "repeat"],
                &["end", "else", "elseif", "until", "}", ")"],
            ),
            Language::Bash => {
                spaces(2, &["then", "do", "{", "(", "else", "in"], &["fi", "done", "}", ")", "else", "elif", "esac"])
            }
            Language::Html => spaces(2, &[">"], &["</"]),
        }
    }
}

impl IndentHints {
    /// Bir seviyelik girinti metni.
    pub fn unit(&self) -> String {
        if self.use_tabs { "\t".to_string() } else { " ".repeat(self.width) }
    }

    /// `prev_line`'dan sonra açılan yeni satırın girintisi; `current` yeni satırın (varsa) baştaki boşluksuz metni.
    /// HTML'de `>` ile biten kapanış etiketi (`</div>`) ve kendini kapatan etiket (`/>`) içeri girmez.
    pub fn next_line_indent(&self, prev_line: &str, current: &str) -> String {
        let base: String = prev_line.chars().take_while(|c| c.is_whitespace()).collect();
        let trimmed = prev_line.trim_end();
        let content = trimmed.trim_start();
        let html_closing = content.starts_with("</") || trimmed.ends_with("/>");
        let increase = !content.is_empty()
            && !html_closing
            && self.increase_after.iter().any(|t| trimmed.ends_with(t.trim_end()) && ends_on_token(trimmed, t));
        let decrease = self.decrease_on.iter().any(|t| current.trim_start().starts_with(t));
        match (increase, decrease) {
            (true, false) => base + &self.unit(),
            (false, true) => dedent(&base, self),
            _ => base,
        }
    }
}

/// Kelime işaretlerinde (`then`, `do`) önceki karakter kelime parçası olmamalı (`undo` ≠ `do`).
fn ends_on_token(line: &str, token: &str) -> bool {
    let token = token.trim_end();
    if !token.chars().all(|c| c.is_alphanumeric()) {
        return true;
    }
    let before = &line[..line.len() - token.len()];
    !before.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_')
}

fn dedent(base: &str, hints: &IndentHints) -> String {
    if let Some(stripped) = base.strip_suffix('\t') {
        return stripped.to_string();
    }
    let keep = base.len().saturating_sub(hints.width);
    base[..keep].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_language_from_path() {
        let cases = [
            ("src/Main.java", Some(Language::Java)),
            ("a/b.tsx", Some(Language::Tsx)),
            ("x.mjs", Some(Language::JavaScript)),
            ("README.MD", Some(Language::Markdown)),
            ("Cargo.lock", Some(Language::Toml)),
            ("/home/u/.zshrc", Some(Language::Bash)),
            ("schema.sql", Some(Language::Sql)),
            ("build.gradle.kts", Some(Language::Kotlin)),
            ("main.go", Some(Language::Go)),
            ("app.py", Some(Language::Python)),
            ("noext", None),
            ("photo.png", None),
        ];
        for (path, want) in cases {
            assert_eq!(Language::from_path(&PathBuf::from(path)), want, "{path}");
        }
        assert_eq!(Language::from_name("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_name("Java"), Some(Language::Java));
    }

    #[test]
    fn indent_hints_follow_braces_and_keywords() {
        let java = Language::Java.indent_hints();
        assert_eq!(java.next_line_indent("    void f() {", ""), "        ");
        assert_eq!(java.next_line_indent("        x();", "}"), "    ");
        let lua = Language::Lua.indent_hints();
        assert_eq!(lua.next_line_indent("if x then", ""), "  ");
        assert_eq!(lua.next_line_indent("undo", ""), "");
        assert_eq!(Language::Sql.comment_tokens().line, Some("--"));
    }
}
