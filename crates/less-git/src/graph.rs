//! Commit grafiği (#58): şerit hesabı parent'lardan; yuvarlak köşeler, şerit başına renk, ● commit / ○ merge.

use crate::log::Commit;

/// Renk numarası 1..=COLORS.
pub const COLORS: u8 = 8;

/// Grafik hücresi: karakter + renk no (boşlukta `None`). Her şerit 2 hücre: sembol + bağlantı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub color: Option<u8>,
}

#[derive(Clone)]
struct Lane {
    hash: String,
    color: u8,
}

fn put(v: &mut Vec<Option<(char, u8)>>, i: usize, val: (char, u8)) {
    if v.len() <= i {
        v.resize(i + 1, None);
    }
    v[i] = Some(val);
}

fn find(lanes: &[Option<Lane>], hash: &str) -> Option<usize> {
    lanes.iter().position(|l| l.as_ref().is_some_and(|l| l.hash == hash))
}

fn free_slot(lanes: &[Option<Lane>], from: usize) -> usize {
    (from..lanes.len()).find(|&i| lanes[i].is_none()).unwrap_or(lanes.len())
}

fn set_lane(lanes: &mut Vec<Option<Lane>>, i: usize, lane: Lane) {
    if i >= lanes.len() {
        lanes.resize(i + 1, None);
    }
    lanes[i] = Some(lane);
}

/// Commit'leri (topo sırada, çocuk önce) satır satır grafik hücrelerine çevirir. Listede olmayan parent'lar şeridi açık bırakır.
pub fn layout(commits: &[Commit]) -> Vec<Vec<Cell>> {
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut last_color = 0u8;
    let mut rows = Vec::with_capacity(commits.len());
    let new_lane = |hash: &str, last: &mut u8| {
        *last = *last % COLORS + 1;
        Lane { hash: hash.to_owned(), color: *last }
    };
    for c in commits {
        let col = match find(&lanes, &c.hash) {
            Some(i) => i,
            None => {
                let i = free_slot(&lanes, 0);
                set_lane(&mut lanes, i, new_lane(&c.hash, &mut last_color));
                i
            }
        };
        let mut sym: Vec<Option<(char, u8)>> = Vec::new();
        let mut conn: Vec<Option<(char, u8)>> = Vec::new();
        for (i, l) in lanes.iter().enumerate() {
            if let Some(l) = l {
                put(&mut sym, i, ('│', l.color));
            }
        }
        let col_color = lanes[col].as_ref().map_or(1, |l| l.color);
        put(&mut sym, col, (if c.parents.len() > 1 { '○' } else { '●' }, col_color));

        let link =
            |sym: &mut Vec<Option<(char, u8)>>, conn: &mut Vec<Option<(char, u8)>>, j: usize, ch: char, color: u8| {
                let (a, b) = (col.min(j), col.max(j));
                for k in a..b {
                    put(conn, k, ('─', color));
                    if k > a
                        && let Some(Some(('│', cc))) = sym.get(k).copied()
                    {
                        put(sym, k, ('┼', cc));
                    }
                }
                put(sym, j, (ch, color));
            };

        let merging: Vec<usize> =
            (0..lanes.len()).filter(|&i| i != col && lanes[i].as_ref().is_some_and(|l| l.hash == c.hash)).collect();
        for i in merging {
            let color = lanes[i].as_ref().map_or(1, |l| l.color);
            link(&mut sym, &mut conn, i, '╯', color);
            lanes[i] = None;
        }
        match c.parents.first() {
            Some(p) => {
                if let Some(l) = lanes[col].as_mut() {
                    l.hash = p.clone();
                }
            }
            None => lanes[col] = None,
        }
        for p in c.parents.iter().skip(1) {
            match find(&lanes, p) {
                Some(j) if j != col => {
                    let color = lanes[j].as_ref().map_or(1, |l| l.color);
                    link(&mut sym, &mut conn, j, if j > col { '┤' } else { '├' }, color);
                }
                Some(_) => {}
                None => {
                    let j = free_slot(&lanes, col + 1);
                    let lane = new_lane(p, &mut last_color);
                    let color = lane.color;
                    set_lane(&mut lanes, j, lane);
                    link(&mut sym, &mut conn, j, '╮', color);
                }
            }
        }
        while matches!(lanes.last(), Some(None)) {
            lanes.pop();
        }
        let width = sym.len().max(conn.len());
        let cell = |v: &[Option<(char, u8)>], i: usize| match v.get(i).copied().flatten() {
            Some((ch, color)) => Cell { ch, color: Some(color) },
            None => Cell { ch: ' ', color: None },
        };
        let mut cells = Vec::with_capacity(width * 2);
        for i in 0..width {
            cells.push(cell(&sym, i));
            cells.push(cell(&conn, i));
        }
        rows.push(cells);
    }
    rows
}

/// Satırı düz metne çevirir (test / önizleme).
pub fn row_text(row: &[Cell]) -> String {
    row.iter().map(|c| c.ch).collect::<String>().trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(h: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: h.into(),
            short: h.into(),
            parents: parents.iter().map(|s| (*s).into()).collect(),
            author: String::new(),
            date: String::new(),
            subject: String::new(),
            refs: vec![],
        }
    }

    #[test]
    fn linear_history_58() {
        let rows = layout(&[c("c", &["b"]), c("b", &["a"]), c("a", &[])]);
        assert_eq!(rows.iter().map(|r| row_text(r)).collect::<Vec<_>>(), vec!["●", "●", "●"]);
        assert_eq!(rows[0][0].color, Some(1));
    }

    #[test]
    fn branch_and_merge_58() {
        let rows = layout(&[c("m", &["a2", "b2"]), c("b2", &["a1"]), c("a2", &["a1"]), c("a1", &[])]);
        let t: Vec<String> = rows.iter().map(|r| row_text(r)).collect();
        assert_eq!(t, vec!["○─╮", "│ ●", "● │", "●─╯"]);
        assert_eq!(rows[0][0].color, Some(1));
        assert_eq!(rows[0][2].color, Some(2));
    }

    #[test]
    fn open_parent_keeps_lane_58() {
        let rows = layout(&[c("x", &["missing"]), c("y", &["missing"])]);
        assert_eq!(row_text(&rows[0]), "●");
        assert_eq!(row_text(&rows[1]), "│ ●");
    }
}
