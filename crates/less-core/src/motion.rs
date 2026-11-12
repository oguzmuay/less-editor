//! Rope üzerinde saf hareket hesapları (kelime, paragraf, eşleşen parantez, karakter bulma).

use ropey::Rope;

use crate::text::{CharClass, class, is_empty_line_at, line_end, line_start, line_text};

fn ch(r: &Rope, i: usize) -> char {
    r.char(i)
}

fn blank_or_nl(c: char) -> bool {
    matches!(class(c, false), CharClass::Blank | CharClass::Eol)
}

/// `w`: sonraki kelime başı; boş satır da durak. Metin sonunda `len`.
pub fn word_forward(r: &Rope, p: usize, big: bool) -> usize {
    let len = r.len_chars();
    if p >= len {
        return len;
    }
    let mut i = p;
    let c0 = class(ch(r, i), big);
    if c0 == CharClass::Word || c0 == CharClass::Punct {
        while i < len && class(ch(r, i), big) == c0 {
            i += 1;
        }
    }
    while i < len {
        let c = ch(r, i);
        if c == '\n' {
            i += 1;
            if i < len && ch(r, i) == '\n' {
                return i;
            }
            continue;
        }
        if blank_or_nl(c) {
            i += 1;
            continue;
        }
        break;
    }
    i
}

/// `b`: önceki kelime başı; boş satır da durak.
pub fn word_backward(r: &Rope, p: usize, big: bool) -> usize {
    if p == 0 {
        return 0;
    }
    let mut i = p - 1;
    while i > 0 && blank_or_nl(ch(r, i)) {
        if is_empty_line_at(r, i) {
            return i;
        }
        i -= 1;
    }
    if is_empty_line_at(r, i) || blank_or_nl(ch(r, i)) {
        return i;
    }
    let c0 = class(ch(r, i), big);
    while i > 0 && class(ch(r, i - 1), big) == c0 {
        i -= 1;
    }
    i
}

/// `e`: sonraki kelime sonu (kapsayıcı).
pub fn word_end(r: &Rope, p: usize, big: bool) -> usize {
    let len = r.len_chars();
    if len == 0 {
        return 0;
    }
    let mut i = p + 1;
    while i < len && blank_or_nl(ch(r, i)) {
        i += 1;
    }
    if i >= len {
        return len - 1;
    }
    let c0 = class(ch(r, i), big);
    while i + 1 < len && class(ch(r, i + 1), big) == c0 {
        i += 1;
    }
    i
}

/// `ge`: önceki kelime sonu.
pub fn word_end_backward(r: &Rope, p: usize, big: bool) -> usize {
    if p == 0 || r.len_chars() == 0 {
        return 0;
    }
    let mut i = p.min(r.len_chars() - 1);
    let c0 = class(ch(r, i), big);
    if c0 == CharClass::Word || c0 == CharClass::Punct {
        while i > 0 && class(ch(r, i - 1), big) == c0 {
            i -= 1;
        }
    }
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && blank_or_nl(ch(r, i)) {
        if is_empty_line_at(r, i) {
            return i;
        }
        i -= 1;
    }
    i
}

/// Option+Sağ: boşlukları atla, kelimenin sonuna (dışlayıcı) git.
pub fn word_right(r: &Rope, p: usize) -> usize {
    let len = r.len_chars();
    let mut i = p;
    while i < len && blank_or_nl(ch(r, i)) {
        i += 1;
    }
    if i >= len {
        return len;
    }
    let c0 = class(ch(r, i), false);
    while i < len && class(ch(r, i), false) == c0 {
        i += 1;
    }
    i
}

/// Option+Sol: boşlukları atla, kelimenin başına git.
pub fn word_left(r: &Rope, p: usize) -> usize {
    let mut i = p;
    while i > 0 && blank_or_nl(ch(r, i - 1)) {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let c0 = class(ch(r, i - 1), false);
    while i > 0 && class(ch(r, i - 1), false) == c0 {
        i -= 1;
    }
    i
}

/// `}`: paragraftan sonraki boş satırın başı, yoksa metin sonu.
pub fn paragraph_forward(r: &Rope, line: usize) -> usize {
    let n = r.len_lines();
    let mut l = line;
    while l + 1 < n && line_text(r, l).is_empty() {
        l += 1;
    }
    while l + 1 < n && !line_text(r, l).is_empty() {
        l += 1;
    }
    if line_text(r, l).is_empty() && l != line { line_start(r, l) } else { line_end(r, l) }
}

/// `{`: önceki boş satırın başı, yoksa 0.
pub fn paragraph_backward(r: &Rope, line: usize) -> usize {
    let mut l = line;
    while l > 0 && line_text(r, l).is_empty() {
        l -= 1;
    }
    while l > 0 && !line_text(r, l).is_empty() {
        l -= 1;
    }
    line_start(r, l)
}

/// `f` / `t` / `F` / `T`: satır içinde `count`. eşleşme. `skip_adjacent` = `;` ile tekrarlanan `t` (bitişik eşleşmede kalmaz).
pub fn find_char(
    r: &Rope,
    p: usize,
    c: char,
    forward: bool,
    till: bool,
    count: usize,
    skip_adjacent: bool,
) -> Option<usize> {
    let line = r.char_to_line(p);
    let s = line_start(r, line);
    let e = line_end(r, line);
    let mut i = p;
    let mut found = 0;
    if forward {
        let mut j = if till && skip_adjacent { p + 1 } else { p };
        while j + 1 < e {
            j += 1;
            if ch(r, j) == c {
                found += 1;
                if found == count {
                    i = j;
                    break;
                }
            }
        }
        if found < count {
            return None;
        }
        Some(if till { i - 1 } else { i })
    } else {
        let mut j = if till && skip_adjacent && p > s { p - 1 } else { p };
        while j > s {
            j -= 1;
            if ch(r, j) == c {
                found += 1;
                if found == count {
                    i = j;
                    break;
                }
            }
        }
        if found < count {
            return None;
        }
        Some(if till { i + 1 } else { i })
    }
}

fn pair_of(c: char) -> Option<(char, char, bool)> {
    match c {
        '(' => Some(('(', ')', true)),
        '[' => Some(('[', ']', true)),
        '{' => Some(('{', '}', true)),
        ')' => Some(('(', ')', false)),
        ']' => Some(('[', ']', false)),
        '}' => Some(('{', '}', false)),
        _ => None,
    }
}

/// `%`: imleçteki ya da satırda imleçten sonraki ilk parantezin eşi.
pub fn match_pair(r: &Rope, p: usize) -> Option<usize> {
    let line = r.char_to_line(p);
    let e = line_end(r, line);
    let start = (p..e).find(|&i| pair_of(ch(r, i)).is_some())?;
    let (open, close, forward) = pair_of(ch(r, start))?;
    let mut depth = 0usize;
    if forward {
        for i in start..r.len_chars() {
            let c = ch(r, i);
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    } else {
        let mut i = start + 1;
        while i > 0 {
            i -= 1;
            let c = ch(r, i);
            if c == close {
                depth += 1;
            } else if c == open {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// `p` çevresindeki `open`/`close` çiftinin konumları (iç içe yapıyı sayarak). `count` dış katman sayısı.
pub fn enclosing_pair(r: &Rope, p: usize, open: char, close: char, count: usize) -> Option<(usize, usize)> {
    let len = r.len_chars();
    if len == 0 {
        return None;
    }
    let p = p.min(len - 1);
    let mut left = None;
    if ch(r, p) == open {
        left = Some(p);
    } else {
        let mut depth = 0usize;
        let mut i = if ch(r, p) == close { p } else { p + 1 };
        while i > 0 {
            i -= 1;
            let c = ch(r, i);
            if c == close && i != p {
                depth += 1;
            } else if c == open {
                if depth == 0 {
                    left = Some(i);
                    break;
                }
                depth -= 1;
            }
        }
    }
    let mut left = left?;
    for _ in 1..count {
        let mut depth = 0usize;
        let mut i = left;
        let mut found = None;
        while i > 0 {
            i -= 1;
            let c = ch(r, i);
            if c == close {
                depth += 1;
            } else if c == open {
                if depth == 0 {
                    found = Some(i);
                    break;
                }
                depth -= 1;
            }
        }
        left = found?;
    }
    let mut depth = 0usize;
    for i in left..len {
        let c = ch(r, i);
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some((left, i));
            }
        }
    }
    None
}
