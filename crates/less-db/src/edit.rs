//! Veri tab'ında bekleyen değişiklikler (#88): hücre düzenleme, NULL, satır kopyalama; SQL üretimi ve geri alma.
//! Satır/kolon indeksleri 0 tabanlı; `row >= base.len()` kopyalanan/yeni satırlardır.

use crate::types::{DbType, quote_ident, quote_table, value_literal};
use std::collections::BTreeMap;

/// Hücre değeri (`None` = NULL).
pub type Cell = Option<String>;

/// Kopyalanan ya da yeni eklenen satır.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingInsert {
    pub values: Vec<Cell>,
    /// Kopyalandığı andaki değerler; birincil anahtar kolonu bundan farklıysa INSERT'e girer.
    pub cloned: Option<Vec<Cell>>,
}

/// Geri alma kaydı.
#[derive(Debug, Clone, PartialEq)]
enum Undo {
    Cell { row: usize, col: usize, previous: Option<Cell> },
    InsertCell { insert: usize, col: usize, previous: Cell },
    InsertAdded,
}

/// Bir tablonun bekleyen düzenleme durumu.
#[derive(Debug, Clone)]
pub struct EditState {
    pub db_type: DbType,
    pub table: String,
    pub columns: Vec<String>,
    pub base: Vec<Vec<Cell>>,
    pub edits: BTreeMap<usize, BTreeMap<usize, Cell>>,
    pub inserts: Vec<PendingInsert>,
    undo: Vec<Undo>,
}

impl EditState {
    pub fn new(db_type: DbType, table: &str, columns: Vec<String>, base: Vec<Vec<Cell>>) -> Self {
        EditState {
            db_type,
            table: table.into(),
            columns,
            base,
            edits: BTreeMap::new(),
            inserts: Vec::new(),
            undo: Vec::new(),
        }
    }

    /// Toplam satır (veritabanı + eklenen).
    pub fn row_count(&self) -> usize {
        self.base.len() + self.inserts.len()
    }

    /// Bekleyen değişiklik sayısı (düzenlenen hücreler + eklenen satırlar).
    pub fn pending(&self) -> usize {
        self.inserts.len() + self.edits.values().map(BTreeMap::len).sum::<usize>()
    }

    /// Değişiklikler uygulanmış görünür hücre değeri.
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        if row < self.base.len() {
            self.edits.get(&row).and_then(|m| m.get(&col)).or_else(|| self.base[row].get(col))
        } else {
            self.inserts.get(row - self.base.len()).and_then(|i| i.values.get(col))
        }
    }

    /// Hücrenin bekleyen değişiklik içerip içermediği (düzenlenmiş) ya da eklenmiş satırda olup olmadığı.
    pub fn is_changed(&self, row: usize, col: usize) -> bool {
        row >= self.base.len() || self.edits.get(&row).is_some_and(|m| m.contains_key(&col))
    }

    /// Hücreye yeni değer koyar; orijinal değere dönülürse bekleyen değişiklik silinir. Geçersiz konumda `false`.
    pub fn set_cell(&mut self, row: usize, col: usize, value: Cell) -> bool {
        if col >= self.columns.len() || row >= self.row_count() {
            return false;
        }
        let nbase = self.base.len();
        if row >= nbase {
            let ins = &mut self.inserts[row - nbase];
            if ins.values.len() <= col {
                ins.values.resize(col + 1, None);
            }
            let previous = std::mem::replace(&mut ins.values[col], value);
            self.undo.push(Undo::InsertCell { insert: row - nbase, col, previous });
            return true;
        }
        let previous = self.edits.get(&row).and_then(|m| m.get(&col)).cloned();
        if self.base[row].get(col) == Some(&value) {
            self.drop_edit(row, col);
        } else {
            self.edits.entry(row).or_default().insert(col, value);
        }
        self.undo.push(Undo::Cell { row, col, previous });
        true
    }

    /// Hücreyi NULL yapar.
    pub fn set_null(&mut self, row: usize, col: usize) -> bool {
        self.set_cell(row, col, None)
    }

    fn drop_edit(&mut self, row: usize, col: usize) {
        if let Some(m) = self.edits.get_mut(&row) {
            m.remove(&col);
            if m.is_empty() {
                self.edits.remove(&row);
            }
        }
    }

    /// Satırın (bekleyen düzenlemeleriyle) kopyasını yeni satır olarak ekler; yeni satırın indeksini döndürür.
    pub fn copy_row(&mut self, row: usize) -> Option<usize> {
        let values: Vec<Cell> = (0..self.columns.len()).map(|c| self.cell(row, c).cloned().unwrap_or(None)).collect();
        if row >= self.row_count() {
            return None;
        }
        self.inserts.push(PendingInsert { cloned: Some(values.clone()), values });
        self.undo.push(Undo::InsertAdded);
        Some(self.row_count() - 1)
    }

    /// Boş (hepsi NULL) yeni satır ekler.
    pub fn add_row(&mut self) -> usize {
        self.inserts.push(PendingInsert { values: vec![None; self.columns.len()], cloned: None });
        self.undo.push(Undo::InsertAdded);
        self.row_count() - 1
    }

    /// Son değişikliği geri alır (`u`); geri alınacak bir şey yoksa `false`.
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            None => false,
            Some(Undo::Cell { row, col, previous }) => {
                match previous {
                    Some(v) => {
                        self.edits.entry(row).or_default().insert(col, v);
                    }
                    None => self.drop_edit(row, col),
                }
                true
            }
            Some(Undo::InsertCell { insert, col, previous }) => {
                if let Some(i) = self.inserts.get_mut(insert) {
                    i.values[col] = previous;
                }
                true
            }
            Some(Undo::InsertAdded) => {
                self.inserts.pop();
                true
            }
        }
    }

    /// İmleçteki hücrenin değişikliğini geri alır (`u` hücre): eklenmiş satırda satırı siler, düzenlenmiş hücrede
    /// hücreyi, değişmemiş hücrede satırın tüm düzenlemelerini geri alır.
    pub fn revert_at(&mut self, row: usize, col: usize) {
        let nbase = self.base.len();
        if row >= nbase {
            if row - nbase < self.inserts.len() {
                self.inserts.remove(row - nbase);
            }
        } else if self.edits.get(&row).is_some_and(|m| m.contains_key(&col)) {
            self.drop_edit(row, col);
        } else {
            self.edits.remove(&row);
        }
        self.undo.clear();
    }

    /// Tüm bekleyen değişiklikleri atar (`U`).
    pub fn revert_all(&mut self) {
        self.edits.clear();
        self.inserts.clear();
        self.undo.clear();
    }

    /// Bekleyen değişikliklerin SQL ifadeleri. `pk` birincil anahtar kolonları (commit anında okunur).
    pub fn statements(&self, pk: &[String]) -> Result<Vec<String>, String> {
        let ty = self.db_type;
        let table = quote_table(ty, &self.table);
        let index = |name: &str| self.columns.iter().position(|c| c == name);
        let mut out = Vec::new();
        if !self.edits.is_empty() {
            if pk.is_empty() {
                return Err(format!(
                    "{} has no primary key, so rows can't be updated safely (new rows can still be added)",
                    self.table
                ));
            }
            for k in pk {
                if index(k).is_none() {
                    return Err(format!("Primary key column {k} is not in the result"));
                }
            }
        }
        for (&row, cols) in &self.edits {
            let sets: Vec<String> = cols
                .iter()
                .map(|(&c, v)| format!("{} = {}", quote_ident(ty, &self.columns[c]), value_literal(ty, v.as_deref())))
                .collect();
            let wheres: Vec<String> = pk
                .iter()
                .filter_map(|k| index(k))
                .map(|c| {
                    let name = quote_ident(ty, &self.columns[c]);
                    match self.base[row].get(c).and_then(|v| v.as_deref()) {
                        None => format!("{name} IS NULL"),
                        Some(v) => format!("{name} = {}", value_literal(ty, Some(v))),
                    }
                })
                .collect();
            out.push(format!("UPDATE {table} SET {} WHERE {};", sets.join(", "), wheres.join(" AND ")));
        }
        for ins in &self.inserts {
            let mut names = Vec::new();
            let mut values = Vec::new();
            for (c, name) in self.columns.iter().enumerate() {
                let v = ins.values.get(c).cloned().unwrap_or(None);
                let is_pk = pk.iter().any(|k| k == name);
                let keep = !is_pk || ins.cloned.as_ref().is_some_and(|orig| orig.get(c) != Some(&v));
                if keep {
                    names.push(quote_ident(ty, name));
                    values.push(value_literal(ty, v.as_deref()));
                }
            }
            if names.is_empty() {
                out.push(format!("INSERT INTO {table} DEFAULT VALUES;"));
            } else {
                out.push(format!("INSERT INTO {table} ({}) VALUES ({});", names.join(", "), values.join(", ")));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(v: &str) -> Cell {
        Some(v.to_string())
    }

    fn state(ty: DbType) -> EditState {
        EditState::new(
            ty,
            "public.users",
            vec!["id".into(), "name".into(), "note".into()],
            vec![vec![c("1"), c("ann"), None], vec![c("2"), c("bob"), c("x")]],
        )
    }

    fn pk() -> Vec<String> {
        vec!["id".to_string()]
    }

    #[test]
    fn update_by_pk_88() {
        let mut s = state(DbType::Postgresql);
        s.set_cell(1, 1, c("o'neil"));
        s.set_null(1, 2);
        let sql = s.statements(&pk()).unwrap();
        assert_eq!(sql, vec![r#"UPDATE "public"."users" SET "name" = 'o''neil', "note" = NULL WHERE "id" = '2';"#]);
    }

    #[test]
    fn mysql_and_sqlserver_dialects_88() {
        let mut s = state(DbType::Mysql);
        s.set_cell(0, 1, c(r"a\b"));
        assert_eq!(s.statements(&pk()).unwrap()[0], r"UPDATE `public`.`users` SET `name` = 'a\\b' WHERE `id` = '1';");
        let mut s = state(DbType::Sqlserver);
        s.set_cell(0, 1, c("é"));
        assert_eq!(s.statements(&pk()).unwrap()[0], "UPDATE [public].[users] SET [name] = N'é' WHERE [id] = N'1';");
    }

    #[test]
    fn pk_null_uses_is_null_88() {
        let mut s = EditState::new(DbType::Sqlite, "t", vec!["id".into(), "v".into()], vec![vec![None, c("a")]]);
        s.set_cell(0, 1, c("b"));
        assert_eq!(s.statements(&pk()).unwrap()[0], r#"UPDATE "t" SET "v" = 'b' WHERE "id" IS NULL;"#);
    }

    #[test]
    fn edit_back_to_original_removes_pending_88() {
        let mut s = state(DbType::Sqlite);
        s.set_cell(0, 1, c("zed"));
        assert_eq!(s.pending(), 1);
        s.set_cell(0, 1, c("ann"));
        assert_eq!(s.pending(), 0);
        assert!(s.statements(&[]).unwrap().is_empty());
    }

    #[test]
    fn no_pk_blocks_update_but_not_insert_88() {
        let mut s = state(DbType::Sqlite);
        s.set_cell(0, 1, c("z"));
        assert!(s.statements(&[]).unwrap_err().contains("no primary key"));
        s.revert_all();
        s.copy_row(0);
        assert!(s.statements(&[]).is_ok());
        let mut s = state(DbType::Sqlite);
        s.set_cell(0, 1, c("z"));
        assert!(s.statements(&["missing".to_string()]).unwrap_err().contains("not in the result"));
    }

    #[test]
    fn copied_row_skips_untouched_pk_88() {
        let mut s = state(DbType::Postgresql);
        let r = s.copy_row(1).unwrap();
        assert_eq!(r, 2);
        s.set_cell(r, 1, c("carl"));
        assert_eq!(
            s.statements(&pk()).unwrap(),
            vec![r#"INSERT INTO "public"."users" ("name", "note") VALUES ('carl', 'x');"#]
        );
        s.set_cell(r, 0, c("9"));
        assert_eq!(
            s.statements(&pk()).unwrap()[0],
            r#"INSERT INTO "public"."users" ("id", "name", "note") VALUES ('9', 'carl', 'x');"#
        );
    }

    #[test]
    fn copy_includes_pending_edits_88() {
        let mut s = state(DbType::Sqlite);
        s.set_cell(0, 1, c("new"));
        let r = s.copy_row(0).unwrap();
        assert_eq!(s.cell(r, 1), Some(&c("new")));
    }

    #[test]
    fn plain_new_row_drops_pk_88() {
        let mut s = state(DbType::Postgresql);
        let r = s.add_row();
        s.set_cell(r, 1, c("n"));
        assert_eq!(
            s.statements(&pk()).unwrap()[0],
            r#"INSERT INTO "public"."users" ("name", "note") VALUES ('n', NULL);"#
        );
    }

    #[test]
    fn undo_stack_88() {
        let mut s = state(DbType::Sqlite);
        s.set_cell(0, 1, c("a1"));
        s.set_cell(0, 1, c("a2"));
        s.copy_row(1);
        assert_eq!(s.pending(), 2);
        assert!(s.undo());
        assert_eq!(s.pending(), 1);
        assert!(s.undo());
        assert_eq!(s.cell(0, 1), Some(&c("a1")));
        assert!(s.undo());
        assert_eq!(s.pending(), 0);
        assert!(!s.undo());
    }

    #[test]
    fn undo_insert_cell_edit_88() {
        let mut s = state(DbType::Sqlite);
        let r = s.add_row();
        s.set_cell(r, 1, c("x"));
        s.undo();
        assert_eq!(s.cell(r, 1), Some(&None));
    }

    #[test]
    fn revert_at_semantics_88() {
        let mut s = state(DbType::Sqlite);
        s.set_cell(0, 1, c("a"));
        s.set_cell(0, 2, c("b"));
        s.revert_at(0, 1);
        assert_eq!(s.pending(), 1);
        s.revert_at(0, 0);
        assert_eq!(s.pending(), 0);
        let r = s.copy_row(0).unwrap();
        s.revert_at(r, 0);
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn invalid_positions_rejected() {
        let mut s = state(DbType::Sqlite);
        assert!(!s.set_cell(9, 0, None));
        assert!(!s.set_cell(0, 9, None));
        assert!(s.copy_row(9).is_none());
    }

    #[test]
    fn is_changed_flags_88() {
        let mut s = state(DbType::Sqlite);
        s.set_cell(0, 1, c("a"));
        let r = s.copy_row(0).unwrap();
        assert!(s.is_changed(0, 1) && !s.is_changed(0, 0) && s.is_changed(r, 0));
    }
}
