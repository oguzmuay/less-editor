//! Yan yana diff modeli (#85): HEAD içeriği ↔ buffer içeriği, değişiklik blokları, bloğu geri alma.

use similar::{Algorithm, DiffTag, capture_diff_slices};

/// Blok türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Added,
    Deleted,
    Changed,
}

/// Değişiklik bloğu; aralıklar 0 tabanlı satır indeksleri (`start..start+len`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub base_start: usize,
    pub base_len: usize,
    pub cur_start: usize,
    pub cur_len: usize,
}

fn lines(s: &str) -> Vec<&str> {
    s.lines().collect()
}

/// HEAD (`base`) ile güncel metin arasındaki blokları döndürür. Bitişik silme + değişiklik tek blok olur.
pub fn blocks(base: &str, current: &str) -> Vec<Block> {
    let (a, b) = (lines(base), lines(current));
    let mut out: Vec<Block> = Vec::new();
    let mut open: Option<Block> = None;
    for op in capture_diff_slices(Algorithm::Patience, &a, &b) {
        let (tag, o, n) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            out.extend(open.take());
            continue;
        }
        match open.as_mut() {
            Some(blk) => {
                blk.base_len = o.end - blk.base_start;
                blk.cur_len = n.end - blk.cur_start;
            }
            None => {
                open = Some(Block {
                    kind: BlockKind::Changed,
                    base_start: o.start,
                    base_len: o.len(),
                    cur_start: n.start,
                    cur_len: n.len(),
                })
            }
        }
    }
    out.extend(open);
    for blk in &mut out {
        blk.kind = match (blk.base_len, blk.cur_len) {
            (0, _) => BlockKind::Added,
            (_, 0) => BlockKind::Deleted,
            _ => BlockKind::Changed,
        };
    }
    out
}

/// Bloğu HEAD'deki haline döndürür ve yeni buffer metnini verir (sonda yeni satır korunur).
pub fn rollback(base: &str, current: &str, block: &Block) -> String {
    let (a, b) = (lines(base), lines(current));
    let mut out: Vec<&str> = b[..block.cur_start].to_vec();
    out.extend_from_slice(&a[block.base_start..block.base_start + block.base_len]);
    out.extend_from_slice(&b[block.cur_start + block.cur_len..]);
    join(&out, current.ends_with('\n') || current.is_empty() && base.ends_with('\n'))
}

/// Tüm değişiklikleri geri alır (HEAD metni).
pub fn rollback_all(base: &str) -> String {
    base.to_owned()
}

fn join(lines: &[&str], trailing_nl: bool) -> String {
    let mut s = lines.join("\n");
    if trailing_nl && !lines.is_empty() {
        s.push('\n');
    }
    s
}

/// Satırın (1 tabanlı, sağ buffer `cur`, sol HEAD) hangi bloğa düştüğü; boş aralıklı blokta sınır satırları da sayılır.
pub fn block_at(blocks: &[Block], line: usize, left: bool) -> Option<&Block> {
    blocks.iter().find(|b| {
        let (start, count) = if left { (b.base_start + 1, b.base_len) } else { (b.cur_start + 1, b.cur_len) };
        if count == 0 {
            line == start || line == start + 1 || line + 1 == start
        } else {
            line >= start && line < start + count
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_and_rollback_85() {
        let base = "a\nb\nc\nd\ne\nf\n";
        let cur = "a\nB\nc\nd\nNEW\ne\n";
        let bl = blocks(base, cur);
        assert_eq!(bl.len(), 3);
        assert_eq!(bl[0].kind, BlockKind::Changed);
        assert_eq!(bl[1].kind, BlockKind::Added);
        assert_eq!(bl[2].kind, BlockKind::Deleted);
        let r = rollback(base, cur, &bl[1]);
        assert_eq!(r, "a\nB\nc\nd\ne\n");
        let r = rollback(base, cur, &bl[0]);
        assert_eq!(r, "a\nb\nc\nd\nNEW\ne\n");
        let r = rollback(base, cur, &bl[2]);
        assert_eq!(r, "a\nB\nc\nd\nNEW\ne\nf\n");
        assert_eq!(rollback_all(base), base);
    }

    #[test]
    fn adjacent_delete_and_change_merge_85() {
        let base = "a\nx\ny\nz\nb\n";
        let cur = "a\nQ\nb\n";
        let bl = blocks(base, cur);
        assert_eq!(bl.len(), 1);
        assert_eq!((bl[0].base_start, bl[0].base_len, bl[0].cur_start, bl[0].cur_len), (1, 3, 1, 1));
        assert_eq!(rollback(base, cur, &bl[0]), base);
        assert!(blocks(base, base).is_empty());
    }

    #[test]
    fn block_lookup_85() {
        let bl = blocks("a\nb\n", "a\nb\nc\n");
        assert!(block_at(&bl, 3, false).is_some());
        assert!(block_at(&bl, 1, false).is_none());
    }
}
