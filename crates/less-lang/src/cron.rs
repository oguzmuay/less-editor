//! Cron ifadesini (Spring 6 alan, Unix 5 alan, `@daily` gibi makrolar) İngilizce cümleye çevirir; kod içindeki
//! cron string'inin üstünde durunca tooltip'te ne zaman tetikleneceği yazılır.

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Base {
    All,
    One(u32),
    Span(u32, u32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Item {
    base: Base,
    step: Option<u32>,
}

type Field = Vec<Item>;

/// `line` içinde `byte` konumundaki cron ifadesinin açıklaması: imlecin bulunduğu tırnaklı string, yoksa
/// `key = value` / `key: value` satırının değeri (properties / yaml).
pub fn describe_at(line: &str, byte: usize) -> Option<String> {
    let quotes: Vec<usize> = line.match_indices('"').map(|(i, _)| i).collect();
    for pair in quotes.as_chunks::<2>().0 {
        if (pair[0]..=pair[1]).contains(&byte) {
            return describe(&line[pair[0] + 1..pair[1]]);
        }
    }
    let sep = line.find(['=', ':'])?;
    (byte > sep).then(|| describe(line[sep + 1..].trim().trim_matches(['"', '\'']))).flatten()
}

/// Cron ifadesinin cümlesi (`"0 0 9-17 * * MON-FRI"` → `"At minute 0, between 09:00 and 17:59, on Monday
/// through Friday."`); geçerli bir cron değilse `None`.
pub fn describe(expr: &str) -> Option<String> {
    let expr = match expr.trim() {
        "@yearly" | "@annually" => "0 0 0 1 1 *",
        "@monthly" => "0 0 0 1 * *",
        "@weekly" => "0 0 0 * * 0",
        "@daily" | "@midnight" => "0 0 0 * * *",
        "@hourly" => "0 0 * * * *",
        e => e,
    };
    let f: Vec<&str> = expr.split_whitespace().collect();
    let (sec, min, hour, dom, mon, dow) = match f.as_slice() {
        [m, h, d, mo, w] => ("0", *m, *h, *d, *mo, *w),
        [s, m, h, d, mo, w] => (*s, *m, *h, *d, *mo, *w),
        _ => return None,
    };
    let sec = parse(sec, 0, 59, &[])?;
    let min = parse(min, 0, 59, &[])?;
    let hour = parse(hour, 0, 23, &[])?;
    let mon = parse(mon, 1, 12, &MONTHS)?;

    let mut parts = time_parts(&sec, &min, &hour);
    let day = day_of_month(dom)?;
    let week = day_of_week(dow)?;
    let month = phrase(&mon, "month", |v| MONTHS[v as usize - 1].to_owned()).map(|p| match p {
        Phrase::Every(s) => s,
        Phrase::List(s) => format!("in {s}"),
    });
    let dated = day.is_some() || week.is_some() || month.is_some();
    parts.extend([day, week, month].into_iter().flatten());
    if !dated {
        parts.push("every day".to_owned());
    }
    let mut out = parts.join(", ");
    out[..1].make_ascii_uppercase();
    out.push('.');
    Some(out)
}

fn time_parts(sec: &Field, min: &Field, hour: &Field) -> Vec<String> {
    let single = |f: &Field| match f.as_slice() {
        [Item { base: Base::One(v), step: None }] => Some(*v),
        _ => None,
    };
    let hours: Option<Vec<u32>> = hour
        .iter()
        .map(|i| match i {
            Item { base: Base::One(v), step: None } => Some(*v),
            _ => None,
        })
        .collect();
    if let (Some(s), Some(m), Some(hs)) = (single(sec), single(min), hours) {
        let at = hs.iter().map(|h| if s == 0 { format!("{h:02}:{m:02}") } else { format!("{h:02}:{m:02}:{s:02}") });
        return vec![format!("at {}", join(at.collect()))];
    }
    let mut parts = Vec::new();
    match phrase(sec, "second", |v| v.to_string()) {
        None => parts.push("every second".to_owned()),
        Some(Phrase::Every(s)) => parts.push(s),
        Some(Phrase::List(_)) if single(sec) == Some(0) => {}
        Some(Phrase::List(s)) => parts.push(format!("at second {s}")),
    }
    let every_hour = matches!(hour.as_slice(), [Item { base: Base::All, step: None }]);
    match phrase(min, "minute", |v| v.to_string()) {
        None if parts.is_empty() => parts.push("every minute".to_owned()),
        None => {}
        Some(Phrase::Every(s)) => parts.push(s),
        Some(Phrase::List(s)) if every_hour => parts.push(format!("at minute {s} of every hour")),
        Some(Phrase::List(s)) => parts.push(format!("at minute {s}")),
    }
    let hour_text = |v: u32| format!("{v:02}:00");
    match hour.as_slice() {
        _ if every_hour => {}
        [Item { base: Base::Span(a, b), step: None }] => parts.push(format!("between {a:02}:00 and {b:02}:59")),
        _ => match phrase(hour, "hour", hour_text) {
            Some(Phrase::Every(s)) => parts.push(s),
            Some(Phrase::List(s)) => parts.push(format!("during hour {s}")),
            None => {}
        },
    }
    parts
}

fn day_of_month(dom: &str) -> Option<Option<String>> {
    Some(Some(match dom {
        "*" | "?" => return Some(None),
        "L" => "on the last day of the month".to_owned(),
        "LW" => "on the last weekday of the month".to_owned(),
        d if d.ends_with('W') => {
            let n: u32 = d[..d.len() - 1].parse().ok().filter(|n| (1..=31).contains(n))?;
            format!("on the weekday nearest day {n} of the month")
        }
        d => match phrase(&parse(d, 1, 31, &[])?, "day", |v| v.to_string())? {
            Phrase::Every(s) => s,
            Phrase::List(s) => format!("on day {s} of the month"),
        },
    }))
}

fn day_of_week(dow: &str) -> Option<Option<String>> {
    let day = |s: &str| value(s, 0, 7, &DAYS).map(|v| DAYS[v as usize % 7]);
    Some(Some(match dow {
        "*" | "?" => return Some(None),
        d if d.len() > 1 && d.ends_with('L') => format!("on the last {} of the month", day(&d[..d.len() - 1])?),
        d if d.contains('#') => {
            let (w, n) = d.split_once('#')?;
            let nth = ["first", "second", "third", "fourth", "fifth"].get(n.parse::<usize>().ok()?.checked_sub(1)?)?;
            format!("on the {nth} {} of the month", day(w)?)
        }
        d => match phrase(&parse(d, 0, 7, &DAYS)?, "day", |v| DAYS[v as usize % 7].to_owned())? {
            Phrase::Every(s) => s,
            Phrase::List(s) => format!("on {s}"),
        },
    }))
}

enum Phrase {
    /// `"every 15 minutes"` gibi kendi başına cümle.
    Every(String),
    /// Değer listesi (`"0 and 30"`); başına bağlam eklenir.
    List(String),
}

/// Alanın ifadesi; `*` (her değer) için `None`.
fn phrase(field: &Field, unit: &str, name: impl Fn(u32) -> String) -> Option<Phrase> {
    let every = |n: u32| if n == 1 { format!("every {unit}") } else { format!("every {n} {unit}s") };
    match field.as_slice() {
        [Item { base: Base::All, step: None }] => None,
        [Item { base: Base::All, step: Some(n) }] => Some(Phrase::Every(every(*n))),
        [Item { base: Base::One(a), step: Some(n) }] if *a <= 1 => Some(Phrase::Every(every(*n))),
        [Item { base: Base::One(a), step: Some(n) }] => {
            Some(Phrase::Every(format!("{}, starting at {}", every(*n), name(*a))))
        }
        [Item { base: Base::Span(a, b), step: Some(n) }] => {
            Some(Phrase::Every(format!("{}, from {} through {}", every(*n), name(*a), name(*b))))
        }
        items => {
            let texts = items.iter().map(|i| match i.base {
                Base::One(v) => name(v),
                Base::Span(a, b) => format!("{} through {}", name(a), name(b)),
                Base::All => format!("every {unit}"),
            });
            Some(Phrase::List(join(texts.collect())))
        }
    }
}

/// `"a"`, `"a and b"`, `"a, b and c"`.
fn join(mut items: Vec<String>) -> String {
    match items.len() {
        0 | 1 => items.pop().unwrap_or_default(),
        _ => {
            let last = items.pop().unwrap_or_default();
            format!("{} and {last}", items.join(", "))
        }
    }
}

/// Virgüllü alanı ayrıştırır (`*`, `?`, `5`, `1-5`, `*/15`, `0/30`, `MON-FRI`); aralık dışıysa `None`.
fn parse(field: &str, lo: u32, hi: u32, names: &[&str]) -> Option<Field> {
    field
        .split(',')
        .map(|part| {
            let (base, step) = match part.split_once('/') {
                Some((b, s)) => (b, Some(s.parse::<u32>().ok().filter(|s| *s > 0)?)),
                None => (part, None),
            };
            let base = match base {
                "*" | "?" => Base::All,
                b => match b.split_once('-') {
                    Some((a, b)) => {
                        let (a, b) = (value(a, lo, hi, names)?, value(b, lo, hi, names)?);
                        (a <= b).then_some(Base::Span(a, b))?
                    }
                    None => Base::One(value(b, lo, hi, names)?),
                },
            };
            Some(Item { base, step })
        })
        .collect()
}

/// Sayı ya da adın ilk üç harfi (`JAN` = 1, `SUN` = 0), `lo..=hi` içinde.
fn value(s: &str, lo: u32, hi: u32, names: &[&str]) -> Option<u32> {
    let named = || {
        let i = names.iter().position(|n| s.len() == 3 && n[..3].eq_ignore_ascii_case(s))?;
        Some(i as u32 + if names.len() == 12 { 1 } else { 0 })
    };
    s.parse::<u32>().ok().or_else(named).filter(|v| (lo..=hi).contains(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_spring_javadoc_examples() {
        let d = |e| describe(e).unwrap();
        assert_eq!(d("0 0 * * * *"), "At minute 0 of every hour, every day.");
        assert_eq!(d("*/10 * * * * *"), "Every 10 seconds, every day.");
        assert_eq!(d("0 0 8-10 * * *"), "At minute 0, between 08:00 and 10:59, every day.");
        assert_eq!(d("0 0 6,19 * * *"), "At 06:00 and 19:00, every day.");
        assert_eq!(d("0 0/30 8-10 * * *"), "Every 30 minutes, between 08:00 and 10:59, every day.");
        assert_eq!(d("0 0 9-17 * * MON-FRI"), "At minute 0, between 09:00 and 17:59, on Monday through Friday.");
        assert_eq!(d("0 0 0 25 DEC ?"), "At 00:00, on day 25 of the month, in December.");
        assert_eq!(d("0 0 0 L * *"), "At 00:00, on the last day of the month.");
        assert_eq!(d("0 0 0 ? * 5L"), "At 00:00, on the last Friday of the month.");
        assert_eq!(d("0 0 0 ? * MON#2"), "At 00:00, on the second Monday of the month.");
        assert_eq!(d("30 2 * * 1"), "At 02:30, on Monday.");
        assert_eq!(d("@daily"), "At 00:00, every day.");
        assert_eq!(d("0 15 10 * * *"), "At 10:15, every day.");
    }

    #[test]
    fn rejects_non_cron_and_finds_string_under_cursor() {
        assert_eq!(describe("hello world"), None);
        assert_eq!(describe("0 0 25 * * *"), None, "hour out of range");
        assert_eq!(describe("a b c d e"), None);
        let line = r#"    @Scheduled(cron = "0 0 2 * * *", zone = "UTC")"#;
        assert_eq!(describe_at(line, 25).as_deref(), Some("At 02:00, every day."));
        assert_eq!(describe_at(line, 42), None, "inside \"UTC\"");
        assert_eq!(describe_at("job.cron=0 */5 * * * *", 12).as_deref(), Some("Every 5 minutes, every day."));
        assert_eq!(describe_at("  cron: \"@hourly\"", 10).as_deref(), Some("At minute 0 of every hour, every day."));
    }
}
