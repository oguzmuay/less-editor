//! Database özel tab'larının UI dışı durumu: konsol, sorgu sonucu, veri ve yapı tablosu.

use less_db::edit::EditState;
use less_db::protocol::{ColumnInfo, QueryResult};
use less_db::types::Kind;
use less_db::{Connection, DbType};

use super::console::Console;

#[derive(Clone, Debug, Default)]
pub struct Grid {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub cursor: (usize, usize),
    pub notice: Option<String>,
    pub affected: Option<u64>,
}

impl Grid {
    pub fn from_result(result: QueryResult) -> Self {
        Self {
            columns: result.columns,
            rows: result.rows,
            cursor: (0, 0),
            notice: result.notice,
            affected: result.affected,
        }
    }

    pub fn move_cursor(&mut self, dr: isize, dc: isize, rows: usize) {
        if self.columns.is_empty() || rows == 0 {
            self.cursor = (0, 0);
            return;
        }
        let row = (self.cursor.0 as isize + dr).clamp(0, rows as isize - 1);
        let col = (self.cursor.1 as isize + dc).clamp(0, self.columns.len() as isize - 1);
        self.cursor = (row as usize, col as usize);
    }
}

pub struct DataTab {
    pub table: String,
    pub grid: Grid,
    /// Yalnız SQL tablolarında bekleyen düzenleme durumu.
    pub edit: Option<EditState>,
}

impl DataTab {
    pub fn new(connection: &Connection, table: String) -> Self {
        let edit =
            (connection.ty.kind() == Kind::Sql).then(|| EditState::new(connection.ty, &table, Vec::new(), Vec::new()));
        Self { table, grid: Grid::default(), edit }
    }

    pub fn apply(&mut self, result: QueryResult) {
        let editable = self.edit.is_some();
        let cursor = self.grid.cursor;
        self.grid = Grid::from_result(result);
        self.grid.cursor = (
            cursor.0.min(self.grid.rows.len().saturating_sub(1)),
            cursor.1.min(self.grid.columns.len().saturating_sub(1)),
        );
        if editable {
            self.edit = Some(EditState::new(
                self.grid_edit_type(),
                &self.table,
                self.grid.columns.clone(),
                self.grid.rows.clone(),
            ));
        }
    }

    fn grid_edit_type(&self) -> DbType {
        self.edit.as_ref().map(|e| e.db_type).unwrap_or(DbType::Sqlite)
    }

    pub fn rows(&self) -> Vec<Vec<Option<String>>> {
        let Some(edit) = &self.edit else { return self.grid.rows.clone() };
        (0..edit.row_count())
            .map(|r| (0..edit.columns.len()).map(|c| edit.cell(r, c).cloned().unwrap_or(None)).collect())
            .collect()
    }

    pub fn pending(&self) -> usize {
        self.edit.as_ref().map_or(0, EditState::pending)
    }

    pub fn value(&self, row: usize, col: usize) -> Option<String> {
        match &self.edit {
            Some(edit) => edit.cell(row, col).cloned().unwrap_or(None),
            None => self.grid.rows.get(row).and_then(|r| r.get(col)).cloned().unwrap_or(None),
        }
    }
}

pub enum TabKind {
    Console(Console),
    Result(Grid),
    Data(DataTab),
    Structure(Grid),
}

pub struct DbTab {
    pub connection: String,
    pub kind: TabKind,
    pub loading: bool,
    pub error: Option<String>,
}

impl DbTab {
    pub fn console(connection: &str, console: Console) -> Self {
        Self { connection: connection.into(), kind: TabKind::Console(console), loading: false, error: None }
    }

    pub fn result(connection: &str) -> Self {
        Self { connection: connection.into(), kind: TabKind::Result(Grid::default()), loading: false, error: None }
    }

    pub fn data(connection: &Connection, table: String) -> Self {
        Self {
            connection: connection.name.clone(),
            kind: TabKind::Data(DataTab::new(connection, table)),
            loading: true,
            error: None,
        }
    }

    pub fn structure(connection: &str, columns: Vec<ColumnInfo>) -> Self {
        let rows = columns
            .into_iter()
            .map(|c| {
                vec![
                    Some(c.name),
                    Some(c.data_type),
                    Some(if c.nullable { "YES" } else { "NO" }.into()),
                    c.default,
                    Some(if c.primary_key { "PRIMARY KEY" } else { "" }.into()),
                ]
            })
            .collect();
        let grid = Grid {
            columns: ["Name", "Type", "Nullable", "Default", "Key"].into_iter().map(str::to_string).collect(),
            rows,
            ..Grid::default()
        };
        Self { connection: connection.into(), kind: TabKind::Structure(grid), loading: false, error: None }
    }

    pub fn grid(&self) -> Option<&Grid> {
        match &self.kind {
            TabKind::Result(g) | TabKind::Structure(g) => Some(g),
            TabKind::Data(d) => Some(&d.grid),
            TabKind::Console(_) => None,
        }
    }

    pub fn grid_mut(&mut self) -> Option<&mut Grid> {
        match &mut self.kind {
            TabKind::Result(g) | TabKind::Structure(g) => Some(g),
            TabKind::Data(d) => Some(&mut d.grid),
            TabKind::Console(_) => None,
        }
    }

    pub fn rows(&self) -> Vec<Vec<Option<String>>> {
        match &self.kind {
            TabKind::Result(g) | TabKind::Structure(g) => g.rows.clone(),
            TabKind::Data(d) => d.rows(),
            TabKind::Console(_) => Vec::new(),
        }
    }

    pub fn value(&self, row: usize, col: usize) -> Option<String> {
        match &self.kind {
            TabKind::Data(d) => d.value(row, col),
            TabKind::Result(g) | TabKind::Structure(g) => {
                g.rows.get(row).and_then(|r| r.get(col)).cloned().unwrap_or(None)
            }
            TabKind::Console(_) => None,
        }
    }

    pub fn pending(&self) -> usize {
        match &self.kind {
            TabKind::Data(d) => d.pending(),
            _ => 0,
        }
    }
}
