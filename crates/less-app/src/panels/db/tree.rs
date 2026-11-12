//! Database bağlantı ağacının UI dışı modeli ve satır eylemleri (#45 #46 #47).

use less_db::Connection;
use less_db::protocol::TableInfo;

#[derive(Clone, Debug, Default)]
pub struct ConnectionState {
    pub expanded: bool,
    pub loading: bool,
    pub loaded: bool,
    pub tables: Vec<TableInfo>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKind {
    Connection(String),
    Table { connection: String, table: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub label: String,
    pub detail: String,
    pub depth: usize,
    pub parent: Option<usize>,
    pub error: bool,
}

/// Görünen bağlantı ve açık tabloları düzleştirir.
pub fn rows(connections: &[Connection], states: &std::collections::HashMap<String, ConnectionState>) -> Vec<Row> {
    let mut out = Vec::new();
    for connection in connections {
        let state = states.get(&connection.name).cloned().unwrap_or_default();
        let parent = out.len();
        let mut detail = connection.ty.display_name().to_string();
        if state.loading {
            detail.push_str(" · Loading…");
        } else if let Some(error) = &state.error {
            detail = error.clone();
        } else if state.expanded && state.loaded && state.tables.is_empty() {
            detail.push_str(" · No tables");
        }
        out.push(Row {
            kind: RowKind::Connection(connection.name.clone()),
            label: connection.name.clone(),
            detail,
            depth: 0,
            parent: None,
            error: state.error.is_some(),
        });
        if state.expanded && !state.loading {
            for table in state.tables {
                let qualified = table.qualified();
                out.push(Row {
                    label: qualified.clone(),
                    detail: table.kind,
                    kind: RowKind::Table { connection: connection.name.clone(), table: qualified },
                    depth: 1,
                    parent: Some(parent),
                    error: false,
                });
            }
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Add,
    OpenConsole(String),
    EditConnection(String),
    DeleteConnection(String),
    Refresh(String),
    OpenData(String, String),
    Structure(String, String),
}

/// Sağ tık / `m` menüsü; listede her zaman New data source vardır.
pub fn menu(row: Option<&Row>) -> Vec<(&'static str, Action)> {
    let mut items = Vec::new();
    match row.map(|r| &r.kind) {
        Some(RowKind::Connection(name)) => {
            items.push(("Open query console", Action::OpenConsole(name.clone())));
            items.push(("Edit connection…", Action::EditConnection(name.clone())));
            items.push(("Refresh", Action::Refresh(name.clone())));
            items.push(("Delete connection…", Action::DeleteConnection(name.clone())));
        }
        Some(RowKind::Table { connection, table }) => {
            items.push(("Open data", Action::OpenData(connection.clone(), table.clone())));
            items.push(("Edit structure", Action::Structure(connection.clone(), table.clone())));
        }
        None => {}
    }
    items.push(("New data source…", Action::Add));
    items
}

/// İmleç satırının görünür listedeki dizini.
pub fn cursor_index(rows: &[Row], cursor: usize) -> usize {
    cursor.min(rows.len().saturating_sub(1))
}
