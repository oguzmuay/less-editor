//! Vurgu türleri: grammar sorgularındaki capture adlarının (`@keyword.function`) anlamsal karşılığı.

/// Tema crate'inin renge eşlediği anlamsal vurgu türü.
///
/// Ayrıntılı türler (`Method`, `TypeBuiltin`…) temada tanımlı değilse [`HighlightKind::fallback`] ile kaba türe iner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HighlightKind {
    Keyword,
    Operator,
    Punctuation,
    PunctuationBracket,
    PunctuationSpecial,
    String,
    StringEscape,
    StringSpecial,
    Comment,
    Function,
    FunctionBuiltin,
    FunctionMacro,
    Method,
    Constructor,
    Type,
    TypeBuiltin,
    Variable,
    VariableBuiltin,
    Parameter,
    Property,
    Constant,
    ConstantBuiltin,
    Boolean,
    Number,
    Attribute,
    Tag,
    Label,
    Namespace,
    Embedded,
    /// Markdown başlığı.
    Heading,
    /// Markdown italik.
    Emphasis,
    /// Markdown kalın.
    Strong,
    /// Bağlantı / URL / referans.
    Link,
    /// Satır içi kod, kod bloğu içeriği (enjeksiyon yoksa).
    Literal,
    Error,
}

impl HighlightKind {
    /// Kararlı ad (tema dosyalarında anahtar olarak kullanılabilir): `"keyword"`, `"function.method"`…
    pub fn name(self) -> &'static str {
        match self {
            HighlightKind::Keyword => "keyword",
            HighlightKind::Operator => "operator",
            HighlightKind::Punctuation => "punctuation",
            HighlightKind::PunctuationBracket => "punctuation.bracket",
            HighlightKind::PunctuationSpecial => "punctuation.special",
            HighlightKind::String => "string",
            HighlightKind::StringEscape => "string.escape",
            HighlightKind::StringSpecial => "string.special",
            HighlightKind::Comment => "comment",
            HighlightKind::Function => "function",
            HighlightKind::FunctionBuiltin => "function.builtin",
            HighlightKind::FunctionMacro => "function.macro",
            HighlightKind::Method => "function.method",
            HighlightKind::Constructor => "constructor",
            HighlightKind::Type => "type",
            HighlightKind::TypeBuiltin => "type.builtin",
            HighlightKind::Variable => "variable",
            HighlightKind::VariableBuiltin => "variable.builtin",
            HighlightKind::Parameter => "variable.parameter",
            HighlightKind::Property => "property",
            HighlightKind::Constant => "constant",
            HighlightKind::ConstantBuiltin => "constant.builtin",
            HighlightKind::Boolean => "boolean",
            HighlightKind::Number => "number",
            HighlightKind::Attribute => "attribute",
            HighlightKind::Tag => "tag",
            HighlightKind::Label => "label",
            HighlightKind::Namespace => "namespace",
            HighlightKind::Embedded => "embedded",
            HighlightKind::Heading => "markup.heading",
            HighlightKind::Emphasis => "markup.italic",
            HighlightKind::Strong => "markup.bold",
            HighlightKind::Link => "markup.link",
            HighlightKind::Literal => "markup.raw",
            HighlightKind::Error => "error",
        }
    }

    /// Bir alt seviyedeki genel tür; zaten en genelse kendisi.
    pub fn fallback(self) -> HighlightKind {
        match self {
            HighlightKind::PunctuationBracket | HighlightKind::PunctuationSpecial => HighlightKind::Punctuation,
            HighlightKind::StringEscape | HighlightKind::StringSpecial => HighlightKind::String,
            HighlightKind::FunctionBuiltin | HighlightKind::FunctionMacro | HighlightKind::Method => {
                HighlightKind::Function
            }
            HighlightKind::Constructor | HighlightKind::TypeBuiltin => HighlightKind::Type,
            HighlightKind::VariableBuiltin | HighlightKind::Parameter => HighlightKind::Variable,
            HighlightKind::ConstantBuiltin | HighlightKind::Boolean => HighlightKind::Constant,
            HighlightKind::Label => HighlightKind::Keyword,
            other => other,
        }
    }

    /// Capture adını türe çevirir; vurgu olmayan capture'lar (`@spell`, `@none`, `@local.*`) için `None`.
    pub fn from_capture(name: &str) -> Option<HighlightKind> {
        use HighlightKind as K;
        let mut parts = name.split('.');
        let head = parts.next()?;
        let sub = parts.next().unwrap_or("");
        Some(match head {
            "keyword" | "conditional" | "repeat" | "include" | "import" | "storageclass" | "exception" | "preproc"
            | "define" | "charset" | "media" | "keyframes" | "supports" => K::Keyword,
            "operator" => K::Operator,
            "punctuation" => match sub {
                "bracket" => K::PunctuationBracket,
                "special" => K::PunctuationSpecial,
                _ => K::Punctuation,
            },
            "string" => match sub {
                "escape" => K::StringEscape,
                "special" | "regex" | "regexp" => match parts.next() {
                    Some("key") => K::Property,
                    _ => K::StringSpecial,
                },
                _ => K::String,
            },
            "escape" => K::StringEscape,
            "character" => K::String,
            "comment" => K::Comment,
            "function" => match sub {
                "builtin" => K::FunctionBuiltin,
                "macro" => K::FunctionMacro,
                "method" => K::Method,
                _ => K::Function,
            },
            "method" => K::Method,
            "constructor" => K::Constructor,
            "type" => match sub {
                "builtin" => K::TypeBuiltin,
                "qualifier" => K::Keyword,
                _ => K::Type,
            },
            "variable" => match sub {
                "builtin" => K::VariableBuiltin,
                "parameter" => K::Parameter,
                "member" => K::Property,
                _ => K::Variable,
            },
            "parameter" => K::Parameter,
            "property" | "field" => K::Property,
            "constant" => match sub {
                "builtin" => K::ConstantBuiltin,
                _ => K::Constant,
            },
            "boolean" => K::Boolean,
            "number" | "float" => K::Number,
            "attribute" => K::Attribute,
            "tag" => match sub {
                "error" => K::Error,
                "attribute" => K::Attribute,
                "delimiter" => K::PunctuationBracket,
                _ => K::Tag,
            },
            "label" => K::Label,
            "namespace" | "module" => K::Namespace,
            "embedded" => K::Embedded,
            "text" | "markup" => match sub {
                "title" | "heading" => K::Heading,
                "emphasis" | "italic" => K::Emphasis,
                "strong" | "bold" => K::Strong,
                "uri" | "reference" | "link" => K::Link,
                "literal" | "raw" => K::Literal,
                _ => return None,
            },
            "error" => K::Error,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::HighlightKind as K;

    #[test]
    fn maps_capture_names() {
        assert_eq!(K::from_capture("keyword.function"), Some(K::Keyword));
        assert_eq!(K::from_capture("function.method"), Some(K::Method));
        assert_eq!(K::from_capture("string.special.key"), Some(K::Property));
        assert_eq!(K::from_capture("text.title"), Some(K::Heading));
        assert_eq!(K::from_capture("spell"), None);
        assert_eq!(K::from_capture("none"), None);
        assert_eq!(K::Method.fallback(), K::Function);
        assert_eq!(K::Method.name(), "function.method");
    }
}
