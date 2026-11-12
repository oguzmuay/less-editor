//! Ex komut satırı (`:`) ayrıştırıcısı. Editörün bildiği komutlar `Command`'a, bilinmeyenler (`:Files`, `:Theme` ...)
//! olduğu gibi `AppCommand::Ex`'e çevrilir.

use less_core::command::{AppCommand, Command, LineAddr, LineBase, LineRange, Operator, SetValue, Target};

/// `:` sonrası metni komutlara çevirir. Hata metni İngilizce ve kullanıcıya gösterilir.
pub fn parse(line: &str) -> Result<Vec<Command>, String> {
    let line = line.trim_start_matches([':', ' ', '\t']).trim_end();
    if line.is_empty() {
        return Ok(Vec::new());
    }
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let range = parse_range(&chars, &mut i)?;
    while i < chars.len() && chars[i] == ' ' {
        i += 1;
    }
    let rest: String = chars[i..].iter().collect();
    if rest.is_empty() {
        let Some(r) = range else { return Ok(Vec::new()) };
        return Ok(vec![Command::GotoAddr(r.end)]);
    }
    let name_len = rest.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let (name, after) = if name_len == 0 {
        let first = rest.chars().next().unwrap_or(' ');
        let n = if first == '<' || first == '>' { rest.chars().take_while(|&c| c == first).count() } else { 1 };
        rest.split_at(rest.char_indices().nth(n).map_or(rest.len(), |(b, _)| b))
    } else {
        rest.split_at(name_len)
    };
    let bang = after.starts_with('!');
    let arg = if bang { &after[1..] } else { after }.trim();
    let arg_opt = (!arg.is_empty()).then(|| arg.to_string());
    let lines = || range.clone().unwrap_or_else(LineRange::current);
    let ab = |full: &str, min: usize| name.len() >= min && full.starts_with(name);
    let app = |c: AppCommand| Ok(vec![Command::App(c)]);

    if name == "s" || (ab("substitute", 2) && !ab("set", 2)) || name == "&" {
        return substitute(lines(), after);
    }
    match name {
        _ if ab("write", 1) => app(AppCommand::Write { path: arg_opt, force: bang }),
        "wq" => app(AppCommand::WriteQuit { force: bang, all: false, only_if_modified: false }),
        "wqa" | "wqall" | "xa" | "xall" => {
            app(AppCommand::WriteQuit { force: bang, all: true, only_if_modified: name.starts_with('x') })
        }
        _ if ab("xit", 1) || name == "exit" => {
            app(AppCommand::WriteQuit { force: bang, all: false, only_if_modified: true })
        }
        _ if ab("quit", 1) => app(AppCommand::Quit { force: bang, all: false }),
        "qa" | "qall" | "quitall" | "quita" => app(AppCommand::Quit { force: bang, all: true }),
        _ if ab("edit", 1) => app(AppCommand::Edit { path: arg_opt, force: bang }),
        _ if ab("nohlsearch", 3) => Ok(vec![Command::ClearSearchHighlight]),
        _ if ab("delete", 1) => Ok(vec![op(Operator::Delete, lines(), arg)]),
        _ if ab("yank", 1) => Ok(vec![op(Operator::Yank, lines(), arg)]),
        _ if ab("undo", 1) => Ok(vec![Command::Undo { count: 1 }]),
        _ if ab("redo", 3) => Ok(vec![Command::Redo { count: 1 }]),
        _ if ab("join", 1) => {
            let r = lines();
            let span = match (&r.start.base, &r.end.base) {
                (LineBase::Number(a), LineBase::Number(b)) if r.start.offset == 0 && r.end.offset == 0 => b - a + 1,
                _ => 2,
            };
            let count = arg.parse::<usize>().map(|n| n + 1).unwrap_or(span).max(2);
            Ok(vec![Command::GotoAddr(r.start), Command::Join { count, spaces: !bang }])
        }
        _ if name.starts_with('>') || name.starts_with('<') => {
            let o = if name.starts_with('>') { Operator::Indent } else { Operator::Outdent };
            Ok((0..name.len()).map(|_| op(o, lines(), "")).collect())
        }
        _ if ab("set", 2) => parse_set(arg),
        _ => app(AppCommand::Ex(line.to_string())),
    }
}

fn op(o: Operator, range: LineRange, arg: &str) -> Command {
    let register = arg.chars().next().filter(|c| !c.is_ascii_digit());
    Command::Operate { op: o, target: Target::Range(range), register }
}

fn parse_range(chars: &[char], i: &mut usize) -> Result<Option<LineRange>, String> {
    if chars.get(*i) == Some(&'%') {
        *i += 1;
        return Ok(Some(LineRange::all()));
    }
    let Some(start) = parse_addr(chars, i)? else { return Ok(None) };
    if matches!(chars.get(*i), Some(',') | Some(';')) {
        *i += 1;
        let end = parse_addr(chars, i)?.unwrap_or_else(LineAddr::current);
        return Ok(Some(LineRange { start, end }));
    }
    Ok(Some(LineRange { start: start.clone(), end: start }))
}

fn parse_addr(chars: &[char], i: &mut usize) -> Result<Option<LineAddr>, String> {
    let base = match chars.get(*i) {
        Some('.') => {
            *i += 1;
            LineBase::Current
        }
        Some('$') => {
            *i += 1;
            LineBase::Last
        }
        Some('\'') => {
            let m = *chars.get(*i + 1).ok_or("E20: Mark not set")?;
            *i += 2;
            LineBase::Mark(m)
        }
        Some(c) if c.is_ascii_digit() => LineBase::Number(number(chars, i).unwrap_or(0)),
        Some('+') | Some('-') => LineBase::Current,
        _ => return Ok(None),
    };
    let mut offset = 0isize;
    while let Some(&c @ ('+' | '-')) = chars.get(*i) {
        *i += 1;
        let n = number(chars, i).unwrap_or(1) as isize;
        offset += if c == '+' { n } else { -n };
    }
    Ok(Some(LineAddr { base, offset }))
}

fn number(chars: &[char], i: &mut usize) -> Option<usize> {
    let start = *i;
    while chars.get(*i).is_some_and(|c| c.is_ascii_digit()) {
        *i += 1;
    }
    chars[start..*i].iter().collect::<String>().parse().ok()
}

/// `s/desen/metin/bayraklar`: ayraç `/` dışında herhangi bir işaret olabilir; `\` ile kaçırılmış ayraç metne girer.
fn substitute(range: LineRange, after: &str) -> Result<Vec<Command>, String> {
    let mut cs = after.chars();
    let Some(delim) = cs.next().filter(|c| !c.is_alphanumeric() && *c != ' ' && *c != '"' && *c != '\\') else {
        return Err("E35: No previous regular expression".into());
    };
    let mut parts = vec![String::new()];
    let mut escaped = false;
    for c in cs {
        if escaped {
            if c != delim {
                parts.last_mut().unwrap_or(&mut String::new()).push('\\');
            }
            if let Some(p) = parts.last_mut() {
                p.push(c);
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == delim && parts.len() < 3 {
            parts.push(String::new());
        } else if let Some(p) = parts.last_mut() {
            p.push(c);
        }
    }
    if escaped && let Some(p) = parts.last_mut() {
        p.push('\\');
    }
    let pattern = parts[0].clone();
    let replacement = parts.get(1).cloned().unwrap_or_default();
    let flags = parts.get(2).cloned().unwrap_or_default();
    let ignore_case = if flags.contains('I') {
        Some(false)
    } else if flags.contains('i') {
        Some(true)
    } else {
        None
    };
    Ok(vec![Command::Substitute { range, pattern, replacement, global: flags.contains('g'), ignore_case }])
}

fn parse_set(arg: &str) -> Result<Vec<Command>, String> {
    if arg.is_empty() {
        return Ok(vec![Command::App(AppCommand::Ex("set".into()))]);
    }
    Ok(arg
        .split_whitespace()
        .map(|item| {
            let (name, value) = if let Some((n, v)) = item.split_once(['=', ':']) {
                (n.to_string(), SetValue::Value(v.to_string()))
            } else if let Some(n) = item.strip_suffix('?') {
                (n.to_string(), SetValue::Query)
            } else if let Some(n) = item.strip_suffix('!') {
                (n.to_string(), SetValue::Toggle)
            } else if let Some(n) = item.strip_prefix("inv") {
                (n.to_string(), SetValue::Toggle)
            } else if let Some(n) = item.strip_prefix("no").filter(|n| !n.is_empty() && *n != "w") {
                (n.to_string(), SetValue::Off)
            } else {
                (item.to_string(), SetValue::On)
            };
            Command::Set { name, value }
        })
        .collect())
}
