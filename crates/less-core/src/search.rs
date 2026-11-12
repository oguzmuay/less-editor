//! Arama: Vim "magic" desenini `regex` sözdizimine çevirir, büyük/küçük harf kuralları, sonraki eşleşme.

use regex::Regex;

/// Oturum boyu arama durumu (Vim'de global).
#[derive(Clone, Debug)]
pub struct SearchState {
    pub pattern: Option<String>,
    /// Son `/` (true) ya da `?` (false).
    pub forward: bool,
    /// `hlsearch` vurgusu görünür mü (`:noh` kapatır, yeni arama açar).
    pub highlight: bool,
    pub ignore_case: bool,
    pub smart_case: bool,
    pub wrap: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self { pattern: None, forward: true, highlight: false, ignore_case: true, smart_case: true, wrap: true }
    }
}

impl SearchState {
    /// Geçerli desenin derlenmiş hali.
    pub fn regex(&self) -> Option<Regex> {
        compile(self.pattern.as_deref()?, self.ignore_case, self.smart_case)
    }
}

/// Vim magic desen → `regex`. `\(` `\)` `\|` `\+` `\?` `\=` `\{` grup / nicelik olur, `\<` `\>` kelime sınırı;
/// kaçışsız `(` `)` `|` `+` `?` `{` `}` düz karakterdir. `\v` (very magic) sonrası desen olduğu gibi kalır.
pub fn vim_to_regex(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('v') => {
                    out.extend(chars.by_ref());
                    break;
                }
                Some('(') => out.push('('),
                Some(')') => out.push(')'),
                Some('|') => out.push('|'),
                Some('+') => out.push('+'),
                Some('?') | Some('=') => out.push('?'),
                Some('{') => {
                    let mut body = String::new();
                    for n in chars.by_ref() {
                        if n == '}' {
                            break;
                        }
                        if n != '\\' {
                            body.push(n);
                        }
                    }
                    let lazy = body.starts_with('-');
                    let rest = body.trim_start_matches('-');
                    if rest.is_empty() {
                        out.push('*');
                    } else {
                        out.push('{');
                        out.push_str(rest);
                        out.push('}');
                    }
                    if lazy {
                        out.push('?');
                    }
                }
                Some('<') | Some('>') => out.push_str("\\b"),
                Some(n @ ('s' | 'S' | 'd' | 'D' | 'w' | 'W' | 'n' | 't' | '.' | '*' | '[' | ']' | '^' | '$' | '/')) => {
                    out.push('\\');
                    out.push(n);
                }
                Some('\\') => out.push_str("\\\\"),
                Some(n) => out.push_str(&regex::escape(&n.to_string())),
                None => out.push_str("\\\\"),
            },
            '(' | ')' | '|' | '+' | '?' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '~' => out.push('~'),
            _ => out.push(c),
        }
    }
    out
}

/// Deseni derler. `ignore_case` + `smart_case`: desende büyük harf varsa duyarlı. `\c` / `\C` desen içinde zorlar.
/// Geçersiz regex düz metin olarak aranır.
pub fn compile(pattern: &str, ignore_case: bool, smart_case: bool) -> Option<Regex> {
    if pattern.is_empty() {
        return None;
    }
    let mut ic = ignore_case && !(smart_case && pattern.chars().any(|c| c.is_uppercase()));
    let mut p = pattern.to_string();
    if p.contains("\\c") {
        ic = true;
        p = p.replace("\\c", "");
    }
    if p.contains("\\C") {
        ic = false;
        p = p.replace("\\C", "");
    }
    let flag = if ic { "(?i)" } else { "" };
    let multi = "(?m)";
    Regex::new(&format!("{multi}{flag}{}", vim_to_regex(&p)))
        .or_else(|_| Regex::new(&format!("{multi}{flag}{}", regex::escape(&p))))
        .ok()
}

/// `from` baytından sonraki (ileri) ya da önceki (geri) eşleşmenin bayt aralığı. İleri aramada `from`daki eşleşme atlanır.
pub fn find(re: &Regex, text: &str, from: usize, forward: bool, wrap: bool) -> Option<(usize, usize)> {
    if forward {
        let hit = re.find_iter(text).find(|m| m.start() > from);
        hit.map(|m| (m.start(), m.end())).or_else(
            || {
                if wrap { re.find(text).map(|m| (m.start(), m.end())) } else { None }
            },
        )
    } else {
        let mut last = None;
        let mut any = None;
        for m in re.find_iter(text) {
            if m.start() < from {
                last = Some((m.start(), m.end()));
            }
            any = Some((m.start(), m.end()));
        }
        last.or(if wrap { any } else { None })
    }
}

/// Vim `:s` değiştirme metni → `regex` değiştirme sözdizimi: `&` ve `\0` tüm eşleşme, `\1`–`\9` gruplar, `\n` / `\r` yeni satır.
pub fn vim_replacement(rep: &str) -> String {
    let mut out = String::new();
    let mut chars = rep.chars();
    while let Some(c) = chars.next() {
        match c {
            '&' => out.push_str("${0}"),
            '$' => out.push_str("$$"),
            '\\' => match chars.next() {
                Some(d @ '0'..='9') => {
                    out.push_str("${");
                    out.push(d);
                    out.push('}');
                }
                Some('n') | Some('r') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('&') => out.push('&'),
                Some('$') => out.push_str("$$"),
                Some(o) => out.push(o),
                None => out.push('\\'),
            },
            _ => out.push(c),
        }
    }
    out
}
