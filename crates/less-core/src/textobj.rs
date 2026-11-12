//! Metin nesnelerinin aralıkları (`iw`, `a"`, `i(`, `ip`, `it` ...). Sonuç: (başlangıç, dışlayıcı bitiş, satır bazlı mı).

use ropey::Rope;

use crate::command::TextObject;
use crate::motion::enclosing_pair;
use crate::text::{CharClass, class, is_blank, line_end, line_start, line_text};

/// Nesne aralığı; bulunamazsa `None`.
pub fn resolve(r: &Rope, p: usize, object: TextObject, around: bool, count: usize) -> Option<(usize, usize, bool)> {
    let count = count.max(1);
    match object {
        TextObject::Word { big } => word(r, p, big, around, count).map(|(a, b)| (a, b, false)),
        TextObject::Quote(q) => quote(r, p, q, around).map(|(a, b)| (a, b, false)),
        TextObject::Pair { open, close } => pair(r, p, open, close, around, count),
        TextObject::Paragraph => paragraph(r, p, around, count),
        TextObject::Tag => tag(r, p, around, count).map(|(a, b)| (a, b, false)),
    }
}

fn word(r: &Rope, p: usize, big: bool, around: bool, count: usize) -> Option<(usize, usize)> {
    let len = r.len_chars();
    if len == 0 {
        return None;
    }
    let line = r.char_to_line(p.min(len));
    let s = line_start(r, line);
    let e = line_end(r, line);
    if s == e {
        return None;
    }
    let p = p.clamp(s, e - 1);
    let cls = |i: usize| class(r.char(i), big);
    let run_end = |mut i: usize| {
        let c = cls(i);
        while i < e && cls(i) == c {
            i += 1;
        }
        i
    };
    let mut start = p;
    let c0 = cls(p);
    while start > s && cls(start - 1) == c0 {
        start -= 1;
    }
    let mut end = run_end(p);
    for _ in 1..count {
        if end >= e {
            break;
        }
        end = run_end(end);
    }
    if around {
        if c0 == CharClass::Blank {
            if end < e {
                end = run_end(end);
            }
        } else if end < e && cls(end) == CharClass::Blank {
            end = run_end(end);
        } else {
            while start > s && is_blank(r.char(start - 1)) {
                start -= 1;
            }
        }
    }
    Some((start, end))
}

fn quote(r: &Rope, p: usize, q: char, around: bool) -> Option<(usize, usize)> {
    let line = r.char_to_line(p.min(r.len_chars()));
    let s = line_start(r, line);
    let e = line_end(r, line);
    let mut quotes = Vec::new();
    let mut i = s;
    while i < e {
        let c = r.char(i);
        if c == '\\' {
            i += 2;
            continue;
        }
        if c == q {
            quotes.push(i);
        }
        i += 1;
    }
    let pairs: Vec<(usize, usize)> = quotes.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect();
    let (a, b) = pairs
        .iter()
        .copied()
        .find(|&(a, b)| a <= p && p <= b)
        .or_else(|| pairs.iter().copied().find(|&(a, _)| a > p))?;
    if !around {
        return Some((a + 1, b));
    }
    let mut end = b + 1;
    let mut start = a;
    if end < e && is_blank(r.char(end)) {
        while end < e && is_blank(r.char(end)) {
            end += 1;
        }
    } else {
        while start > s && is_blank(r.char(start - 1)) {
            start -= 1;
        }
    }
    Some((start, end))
}

fn pair(r: &Rope, p: usize, open: char, close: char, around: bool, count: usize) -> Option<(usize, usize, bool)> {
    let (a, b) = enclosing_pair(r, p, open, close, count)?;
    if around {
        return Some((a, b + 1, false));
    }
    let mut start = a + 1;
    let mut end = b;
    let close_line = r.char_to_line(b);
    let close_on_own_line = line_text(r, close_line).chars().take(b - line_start(r, close_line)).all(is_blank);
    if start < end && r.char(start) == '\n' && close_on_own_line && close_line > r.char_to_line(a) + 1 {
        start += 1;
        end = line_start(r, close_line);
        return Some((start, end, true));
    }
    if start < end && r.char(start) == '\n' {
        start += 1;
    }
    if start > end {
        start = end;
    }
    Some((start, end, false))
}

fn paragraph(r: &Rope, p: usize, around: bool, count: usize) -> Option<(usize, usize, bool)> {
    let n = r.len_lines();
    let line = r.char_to_line(p.min(r.len_chars()));
    let blank = |l: usize| line_text(r, l).trim().is_empty();
    let mut first = line;
    let b0 = blank(line);
    while first > 0 && blank(first - 1) == b0 {
        first -= 1;
    }
    let mut last = line;
    let mut blocks = if around { count * 2 } else { count };
    let mut kind = b0;
    loop {
        while last + 1 < n && blank(last + 1) == kind {
            last += 1;
        }
        blocks -= 1;
        if blocks == 0 || last + 1 >= n {
            break;
        }
        last += 1;
        kind = blank(last);
    }
    if around && blocks > 0 && !b0 {
        while first > 0 && blank(first - 1) {
            first -= 1;
        }
    }
    let start = line_start(r, first);
    let end = if last + 1 < n { line_start(r, last + 1) } else { r.len_chars() };
    Some((start, end, true))
}

fn tag(r: &Rope, p: usize, around: bool, count: usize) -> Option<(usize, usize)> {
    let text = r.to_string();
    let re = regex::Regex::new(r"<(/?)([A-Za-z][\w:.-]*)[^<>]*?(/?)>").ok()?;
    let mut stack: Vec<(String, usize, usize)> = Vec::new();
    let mut pairs: Vec<(usize, usize, usize, usize)> = Vec::new();
    for m in re.captures_iter(&text) {
        let whole = m.get(0)?;
        let (bs, be) = (r.byte_to_char(whole.start()), r.byte_to_char(whole.end()));
        let name = m.get(2)?.as_str().to_string();
        if !m[3].is_empty() {
            continue;
        }
        if m[1].is_empty() {
            stack.push((name, bs, be));
        } else if let Some(idx) = stack.iter().rposition(|(n, _, _)| *n == name) {
            let (_, os, oe) = stack[idx].clone();
            stack.truncate(idx);
            pairs.push((os, oe, bs, be));
        }
    }
    let mut enclosing: Vec<_> = pairs.into_iter().filter(|&(os, _, _, ce)| os <= p && p < ce).collect();
    enclosing.sort_by_key(|&(os, _, _, ce)| ce - os);
    let (os, oe, cs, ce) = *enclosing.get(count - 1)?;
    Some(if around { (os, ce) } else { (oe, cs) })
}
