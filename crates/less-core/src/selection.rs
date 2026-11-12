//! Seçimler: her imleç bir `Range` (anchor + head, karakter indeksi). Birden çok aralık = çoklu imleç.

/// Tek imleç / seçim. `head` imlecin bulunduğu yer, `anchor` seçimin sabit ucu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
}

impl Range {
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    pub fn point(pos: usize) -> Self {
        Self { anchor: pos, head: pos }
    }

    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn map(self, f: impl Fn(usize) -> usize) -> Self {
        Self { anchor: f(self.anchor), head: f(self.head) }
    }
}

/// Bir ya da daha çok aralık; `primary` durum satırı ve kaydırma için esas alınan imleçtir.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    ranges: Vec<Range>,
    primary: usize,
}

impl Default for Selection {
    fn default() -> Self {
        Self::point(0)
    }
}

impl Selection {
    pub fn point(pos: usize) -> Self {
        Self { ranges: vec![Range::point(pos)], primary: 0 }
    }

    pub fn single(range: Range) -> Self {
        Self { ranges: vec![range], primary: 0 }
    }

    /// Boş liste verilirse 0 konumunda tek imleç olur.
    pub fn new(ranges: Vec<Range>, primary: usize) -> Self {
        if ranges.is_empty() {
            return Self::point(0);
        }
        let primary = primary.min(ranges.len() - 1);
        Self { ranges, primary }
    }

    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    pub fn ranges_mut(&mut self) -> &mut [Range] {
        &mut self.ranges
    }

    pub fn primary(&self) -> Range {
        self.ranges[self.primary]
    }

    pub fn primary_index(&self) -> usize {
        self.primary
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn push(&mut self, range: Range) {
        self.ranges.push(range);
    }

    pub fn set_primary(&mut self, index: usize) {
        self.primary = index.min(self.ranges.len() - 1);
    }

    /// Yalnız birincil aralık kalır, boş imlece iner.
    pub fn collapse_to_primary(&mut self) {
        let head = self.primary().head;
        self.ranges = vec![Range::point(head)];
        self.primary = 0;
    }

    /// Aralıkları sıralar, aynı ya da çakışan imleçleri birleştirir; birincil korunur.
    pub fn normalize(&mut self) {
        let primary = self.primary();
        let mut ranges = std::mem::take(&mut self.ranges);
        ranges.sort_by_key(|r| (r.from(), r.to()));
        let mut out: Vec<Range> = Vec::with_capacity(ranges.len());
        for r in ranges {
            if let Some(last) = out.last_mut() {
                let overlap = r.from() < last.to() || (r.from() == last.from() && r.to() == last.to());
                if overlap {
                    let from = last.from().min(r.from());
                    let to = last.to().max(r.to());
                    *last = if last.head >= last.anchor { Range::new(from, to) } else { Range::new(to, from) };
                    continue;
                }
            }
            out.push(r);
        }
        self.primary =
            out.iter().position(|r| r.from() <= primary.head && primary.head <= r.to().max(r.from())).unwrap_or(0);
        self.ranges = out;
    }
}
