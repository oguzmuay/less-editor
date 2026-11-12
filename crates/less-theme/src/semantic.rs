//! Anlamsal vurgu adları ve sözdizimi dışı renkler.

use crate::{Palette, Rgb};

/// Vurgulama / arayüz için anlamsal renk adları.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HighlightKind {
    Keyword,
    String,
    Comment,
    Function,
    Type,
    Variable,
    Constant,
    Number,
    Operator,
    Punctuation,
    Attribute,
    Tag,
    Property,
    Namespace,
    DiffAdd,
    DiffDelete,
    DiffChange,
    DiagError,
    DiagWarn,
    DiagInfo,
    DiagHint,
}

impl HighlightKind {
    /// Tüm türler (test ve tablo üretimi için).
    pub const ALL: [HighlightKind; 21] = [
        Self::Keyword,
        Self::String,
        Self::Comment,
        Self::Function,
        Self::Type,
        Self::Variable,
        Self::Constant,
        Self::Number,
        Self::Operator,
        Self::Punctuation,
        Self::Attribute,
        Self::Tag,
        Self::Property,
        Self::Namespace,
        Self::DiffAdd,
        Self::DiffDelete,
        Self::DiffChange,
        Self::DiagError,
        Self::DiagWarn,
        Self::DiagInfo,
        Self::DiagHint,
    ];

    /// tree-sitter yakalama adından (`keyword.control`, `string.escape`...) tür; bilinmeyende `None`.
    pub fn from_capture(name: &str) -> Option<Self> {
        let head = name.split('.').next().unwrap_or(name);
        Some(match head {
            "keyword" | "conditional" | "repeat" | "include" | "exception" => Self::Keyword,
            "string" | "character" => Self::String,
            "comment" => Self::Comment,
            "function" | "method" | "constructor" => Self::Function,
            "type" | "class" | "interface" | "enum" | "struct" => Self::Type,
            "variable" | "parameter" => Self::Variable,
            "constant" | "boolean" => Self::Constant,
            "number" | "float" => Self::Number,
            "operator" => Self::Operator,
            "punctuation" | "delimiter" => Self::Punctuation,
            "attribute" | "annotation" | "decorator" => Self::Attribute,
            "tag" => Self::Tag,
            "property" | "field" => Self::Property,
            "namespace" | "module" => Self::Namespace,
            _ => return None,
        })
    }
}

/// Sözdizimi dışı renkler.
#[derive(Clone, Copy, Debug)]
pub struct Extra {
    pub variable: Rgb,
    pub constant: Rgb,
    pub operator: Rgb,
    pub punctuation: Rgb,
    pub tag: Rgb,
    pub property: Rgb,
    pub namespace: Rgb,
    pub diff_add: Rgb,
    pub diff_delete: Rgb,
    pub diff_change: Rgb,
    pub warn: Rgb,
    pub info: Rgb,
    pub hint: Rgb,
}

impl Extra {
    /// Sıra: variable, constant, operator, punctuation, tag, property, namespace, diff add/delete/change,
    /// warn, info, hint.
    pub const fn new(v: [u32; 13]) -> Self {
        Self {
            variable: Rgb::hex(v[0]),
            constant: Rgb::hex(v[1]),
            operator: Rgb::hex(v[2]),
            punctuation: Rgb::hex(v[3]),
            tag: Rgb::hex(v[4]),
            property: Rgb::hex(v[5]),
            namespace: Rgb::hex(v[6]),
            diff_add: Rgb::hex(v[7]),
            diff_delete: Rgb::hex(v[8]),
            diff_change: Rgb::hex(v[9]),
            warn: Rgb::hex(v[10]),
            info: Rgb::hex(v[11]),
            hint: Rgb::hex(v[12]),
        }
    }
}

/// Git geçmişi grafiğindeki şerit renkleri sayısı (`GitHistoryLane1..8`).
pub const LANES: usize = 8;

/// Claude maskotu için oturum başına sabit renkler (ilki Claude turuncusu).
pub const MASCOT: [Rgb; 8] = [
    Rgb::hex(0xd97757),
    Rgb::hex(0x7aa2f7),
    Rgb::hex(0x9ece6a),
    Rgb::hex(0xbb9af7),
    Rgb::hex(0xe0af68),
    Rgb::hex(0x7dcfff),
    Rgb::hex(0xf7768e),
    Rgb::hex(0x1abc9c),
];

impl Palette {
    /// Anlamsal ada karşılık gelen renk.
    pub fn color(&self, kind: HighlightKind) -> Rgb {
        use HighlightKind::*;
        match kind {
            Keyword => self.syntax.keyword,
            String => self.syntax.string,
            Comment => self.syntax.comment,
            Function => self.syntax.function,
            Type => self.syntax.ty,
            Number => self.syntax.number,
            Attribute => self.syntax.annotation,
            Variable => self.extra.variable,
            Constant => self.extra.constant,
            Operator => self.extra.operator,
            Punctuation => self.extra.punctuation,
            Tag => self.extra.tag,
            Property => self.extra.property,
            Namespace => self.extra.namespace,
            DiffAdd => self.extra.diff_add,
            DiffDelete => self.extra.diff_delete,
            DiffChange => self.extra.diff_change,
            DiagError => self.error,
            DiagWarn => self.extra.warn,
            DiagInfo => self.extra.info,
            DiagHint => self.extra.hint,
        }
    }

    /// Git grafiği şerit rengi (`lane` 0 tabanlı, 8'den sonra başa sarar).
    pub fn git_lane(&self, lane: usize) -> Rgb {
        self.avatars[lane % LANES]
    }

    /// Claude maskotu rengi (oturum numarasına göre sabit).
    pub fn mascot(&self, n: usize) -> Rgb {
        MASCOT[n % MASCOT.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_color_in_every_theme() {
        for p in crate::ALL {
            for k in HighlightKind::ALL {
                let _ = p.color(k);
            }
        }
    }

    #[test]
    fn capture_names_map() {
        assert_eq!(HighlightKind::from_capture("keyword.control"), Some(HighlightKind::Keyword));
        assert_eq!(HighlightKind::from_capture("nope"), None);
    }

    #[test]
    fn git_lane_wraps() {
        let p = crate::TOKYONIGHT_NIGHT;
        assert_eq!(p.git_lane(8), p.git_lane(0));
    }
}
