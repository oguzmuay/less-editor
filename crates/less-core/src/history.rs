//! Geri al / yinele. Değişiklikler işlem (transaction) olarak gruplanır; iç içe `begin`/`end` tek adımdır.

use ropey::Rope;

use crate::selection::Selection;

/// Tek değişiklik; işlemdeki değişiklikler sırayla uygulanır (her `pos` önceki değişikliklerden sonraki metne göredir).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub pos: usize,
    pub removed: String,
    pub inserted: String,
}

/// Uygulanan ham değişiklik: (konum, silinen karakter sayısı, eklenen karakter sayısı). İşaret kaydırma için.
pub type RawEdit = (usize, usize, usize);

#[derive(Clone, Debug)]
struct Transaction {
    id: u64,
    edits: Vec<Edit>,
    before: Selection,
    after: Selection,
    typing: bool,
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    open: Option<Transaction>,
    depth: usize,
    next_id: u64,
}

impl History {
    /// Bir işlem açar; açık işlem varsa yalnız derinlik artar.
    pub fn begin(&mut self, sel: &Selection) {
        self.depth += 1;
        if self.open.is_none() {
            self.open =
                Some(Transaction { id: 0, edits: Vec::new(), before: sel.clone(), after: sel.clone(), typing: true });
        }
    }

    /// Derinlik sıfırlanınca işlem kapanır. `typing` iken önceki yazma işlemiyle birleşebilir.
    pub fn end(&mut self, sel: &Selection, merge_typing: bool) {
        if self.depth == 0 {
            return;
        }
        self.depth -= 1;
        if self.depth == 0 {
            self.close(sel, merge_typing);
        }
    }

    /// Açık grup kalmışsa (ör. Esc gelmeden geri al) zorla kapatır.
    pub fn force_close(&mut self, sel: &Selection) {
        if self.depth > 0 {
            self.depth = 0;
            self.close(sel, false);
        }
    }

    fn close(&mut self, sel: &Selection, merge_typing: bool) {
        let Some(mut tx) = self.open.take() else { return };
        if tx.edits.is_empty() {
            return;
        }
        self.next_id += 1;
        tx.id = self.next_id;
        tx.after = sel.clone();
        self.redo.clear();
        if merge_typing
            && tx.typing
            && let Some(top) = self.undo.last_mut().filter(|t| t.typing)
        {
            top.edits.append(&mut tx.edits);
            top.after = tx.after;
            top.id = tx.id;
            return;
        }
        self.undo.push(tx);
    }

    /// Açık işlem yazma birleşmesine uygun değil (yazma dışı değişiklik içeriyor).
    pub fn mark_not_typing(&mut self) {
        if let Some(tx) = self.open.as_mut() {
            tx.typing = false;
        }
    }

    pub fn is_open(&self) -> bool {
        self.depth > 0
    }

    /// Açık işlemde henüz kapanmamış değişiklik var mı (insert oturumu sürerken buffer kirli sayılır).
    pub fn has_open_edits(&self) -> bool {
        self.open.as_ref().is_some_and(|t| !t.edits.is_empty())
    }

    /// Açık işlemi (değişiklik varsa) kapatıp aynı derinlikte yenisini açar; insert modda kaydetme için.
    pub fn checkpoint(&mut self) {
        let Some(tx) = self.open.as_ref().filter(|t| !t.edits.is_empty()) else { return };
        let sel = tx.before.clone();
        let depth = self.depth;
        self.close(&sel, false);
        self.depth = 0;
        self.begin(&sel);
        self.depth = depth;
    }

    pub fn record(&mut self, edit: Edit, sel: &Selection) {
        if self.open.is_none() {
            self.begin(sel);
            self.depth -= 1;
            if let Some(tx) = self.open.as_mut() {
                tx.edits.push(edit);
            }
            self.close(sel, false);
            return;
        }
        if let Some(tx) = self.open.as_mut() {
            tx.edits.push(edit);
        }
    }

    /// Kayıtlı (kapanmış) son işlemin kimliği; değiştirilmemiş durum 0.
    pub fn current_rev(&self) -> u64 {
        self.undo.last().map(|t| t.id).unwrap_or(0)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self, rope: &mut Rope) -> Option<(Selection, Vec<RawEdit>)> {
        let tx = self.undo.pop()?;
        let mut raw = Vec::new();
        for e in tx.edits.iter().rev() {
            let n = e.inserted.chars().count();
            rope.remove(e.pos..e.pos + n);
            rope.insert(e.pos, &e.removed);
            raw.push((e.pos, n, e.removed.chars().count()));
        }
        let sel = tx.before.clone();
        self.redo.push(tx);
        Some((sel, raw))
    }

    pub fn redo(&mut self, rope: &mut Rope) -> Option<(Selection, Vec<RawEdit>)> {
        let tx = self.redo.pop()?;
        let mut raw = Vec::new();
        for e in &tx.edits {
            let n = e.removed.chars().count();
            rope.remove(e.pos..e.pos + n);
            rope.insert(e.pos, &e.inserted);
            raw.push((e.pos, n, e.inserted.chars().count()));
        }
        let sel = tx.after.clone();
        self.undo.push(tx);
        Some((sel, raw))
    }
}
