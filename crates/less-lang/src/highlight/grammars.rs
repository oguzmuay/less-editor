//! Gömülü grammar'lar ve vurgu sorguları (her dil için bir kez derlenir, süreç boyu önbellekte).

use std::sync::OnceLock;

use tree_sitter::Query;

use crate::language::Language;

/// Bir dilin derlenmiş vurgu yapılandırması.
pub(crate) struct Grammar {
    pub language: tree_sitter::Language,
    /// Aynı düğümü yakalayan desenlerden sonraki kazanır (tree-sitter-highlight ve nvim ile aynı kural).
    pub query: Query,
}

/// Markdown satır içi grammar'ı (blok ağacındaki `inline` düğümleri üstünde çalışır).
pub(crate) fn markdown_inline() -> Option<&'static Grammar> {
    static CELL: OnceLock<Result<Grammar, String>> = OnceLock::new();
    CELL.get_or_init(|| build(tree_sitter_md::INLINE_LANGUAGE.into(), &[tree_sitter_md::HIGHLIGHT_QUERY_INLINE]))
        .as_ref()
        .ok()
}

/// Dilin grammar'ı; sorgu derlenemezse `None` (dil düz metin gösterilir).
pub(crate) fn grammar(lang: Language) -> Option<&'static Grammar> {
    loaded(lang).as_ref().ok()
}

/// Sorgu derleme hatası (testler ve tanılama için).
pub fn grammar_error(lang: Language) -> Option<String> {
    loaded(lang).as_ref().err().cloned()
}

fn loaded(lang: Language) -> &'static Result<Grammar, String> {
    static CELLS: [OnceLock<Result<Grammar, String>>; Language::ALL.len()] =
        [const { OnceLock::new() }; Language::ALL.len()];
    let index = Language::ALL.iter().position(|l| *l == lang).unwrap_or(0);
    CELLS[index].get_or_init(|| load(lang))
}

fn load(lang: Language) -> Result<Grammar, String> {
    use Language as L;
    let js = tree_sitter_javascript::HIGHLIGHT_QUERY;
    let jsx = tree_sitter_javascript::JSX_HIGHLIGHT_QUERY;
    let ts = tree_sitter_typescript::HIGHLIGHTS_QUERY;
    match lang {
        L::Java => build(tree_sitter_java::LANGUAGE.into(), &[tree_sitter_java::HIGHLIGHTS_QUERY]),
        L::TypeScript => build(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), &[js, ts]),
        L::Tsx => build(tree_sitter_typescript::LANGUAGE_TSX.into(), &[js, jsx, ts]),
        L::JavaScript | L::Jsx => build(tree_sitter_javascript::LANGUAGE.into(), &[js, jsx]),
        L::Json => build(tree_sitter_json::LANGUAGE.into(), &[tree_sitter_json::HIGHLIGHTS_QUERY]),
        L::Yaml => build(tree_sitter_yaml::LANGUAGE.into(), &[tree_sitter_yaml::HIGHLIGHTS_QUERY]),
        L::Markdown => build(tree_sitter_md::LANGUAGE.into(), &[tree_sitter_md::HIGHLIGHT_QUERY_BLOCK]),
        L::Sql => build(tree_sitter_sequel::LANGUAGE.into(), &[tree_sitter_sequel::HIGHLIGHTS_QUERY]),
        L::Lua => build(tree_sitter_lua::LANGUAGE.into(), &[tree_sitter_lua::HIGHLIGHTS_QUERY]),
        L::Rust => build(tree_sitter_rust::LANGUAGE.into(), &[tree_sitter_rust::HIGHLIGHTS_QUERY]),
        L::Toml => build(tree_sitter_toml_ng::LANGUAGE.into(), &[tree_sitter_toml_ng::HIGHLIGHTS_QUERY]),
        L::Bash => build(tree_sitter_bash::LANGUAGE.into(), &[tree_sitter_bash::HIGHLIGHT_QUERY]),
        L::Css => build(tree_sitter_css::LANGUAGE.into(), &[tree_sitter_css::HIGHLIGHTS_QUERY]),
        L::Html => build(tree_sitter_html::LANGUAGE.into(), &[tree_sitter_html::HIGHLIGHTS_QUERY]),
        L::Kotlin => build(tree_sitter_kotlin_sg::LANGUAGE.into(), &[tree_sitter_kotlin_sg::HIGHLIGHTS_QUERY]),
        L::Go => build(tree_sitter_go::LANGUAGE.into(), &[tree_sitter_go::HIGHLIGHTS_QUERY]),
        L::Python => build(tree_sitter_python::LANGUAGE.into(), &[tree_sitter_python::HIGHLIGHTS_QUERY]),
    }
}

fn build(language: tree_sitter::Language, sources: &[&str]) -> Result<Grammar, String> {
    let source = sources.join("\n");
    let query = Query::new(&language, &source).map_err(|err| err.to_string())?;
    Ok(Grammar { language, query })
}
