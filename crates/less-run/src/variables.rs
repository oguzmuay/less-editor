//! Debug Variables modeli (#91, #97): o anki frame'in scope'ları ve değişkenleri `Name | Value | Type` ağacı.
//!
//! Değer düzenleme DAP `setVariable` (yoksa `setExpression`) ile; başarılı yanıttaki değer bu duraklama boyunca
//! `overrides`'ta tutulur (js-debug aynı duraklamada listeyi önbellekten verir). Eleman silme / ekleme `evaluate`
//! ifadesiyle, dile göre (JS `splice`/`delete`/`push`, Java `remove`/`add`, Python `pop`/`append`).

use std::collections::{HashMap, HashSet};

use crate::dap::client::{DapClient, DapError};
use crate::dap::manager::Lang;
use crate::dap::types::{Capabilities, Scope, Variable};

/// Değişkenleri getiren kaynak (DAP istemcisi ya da test sahtesi).
pub trait VariableSource {
    fn scopes(&self, frame_id: i64) -> Result<Vec<Scope>, DapError>;
    fn variables(&self, reference: i64) -> Result<Vec<Variable>, DapError>;
}

impl VariableSource for DapClient {
    fn scopes(&self, frame_id: i64) -> Result<Vec<Scope>, DapError> {
        DapClient::scopes(self, frame_id)
    }

    fn variables(&self, reference: i64) -> Result<Vec<Variable>, DapError> {
        DapClient::variables(self, reference)
    }
}

/// Ağaç düğümü.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarNode {
    pub is_scope: bool,
    pub name: String,
    pub value: Option<String>,
    pub type_: Option<String>,
    /// `variablesReference` (0 = alt öğe yok).
    pub reference: i64,
    pub evaluate_name: Option<String>,
    pub parent_ref: Option<i64>,
    /// Kalıcı yol (`scope\0ad\0ad…`); açık/kapalı ve değişim takibi bununla.
    pub path: String,
    pub depth: usize,
    pub expensive: bool,
    pub children: Vec<VarNode>,
}

impl VarNode {
    /// Liste elemanı dizini: ad yalnız sayı (js-debug `0`) ya da `[0]` (java-debug).
    pub fn index(&self) -> Option<usize> {
        let n = self.name.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(&self.name);
        n.parse().ok()
    }
}

/// Tablo satırı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarRow {
    pub path: String,
    pub depth: usize,
    pub is_scope: bool,
    /// Ad hücresi: girinti + `▸`/`▾` + ad (liste elemanı `[i]`).
    pub name_cell: String,
    /// Tek satırlık değer (`\n` → `↵`).
    pub value: String,
    pub type_: String,
    pub expandable: bool,
    pub expanded: bool,
    /// Son duraktan beri değişti (sarı).
    pub changed: bool,
}

/// Panel durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewStatus {
    #[default]
    Idle,
    Paused,
    Running,
}

/// Değer değiştirme isteği.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetRequest {
    SetVariable { parent_ref: i64, name: String, value: String },
    SetExpression { expression: String, value: String, frame_id: i64 },
}

/// Variables modeli.
#[derive(Debug, Clone, Default)]
pub struct VariablesModel {
    expanded: HashSet<String>,
    seen: HashSet<String>,
    roots: Vec<VarNode>,
    current: HashMap<String, String>,
    baseline: HashMap<String, String>,
    overrides: HashMap<String, String>,
    pub status: ViewStatus,
}

const SEP: char = '\0';

impl VariablesModel {
    pub fn new() -> VariablesModel {
        VariablesModel::default()
    }

    /// Kök düğümler (scope'lar).
    pub fn roots(&self) -> &[VarNode] {
        &self.roots
    }

    /// Oturum durdu: önceki değerler karşılaştırma tabanı olur, düzeltmeler silinir.
    pub fn on_stopped(&mut self) {
        self.baseline = std::mem::take(&mut self.current);
        self.overrides.clear();
    }

    /// Program devam etti (tablo soluk).
    pub fn on_continued(&mut self) {
        self.status = ViewStatus::Running;
        self.overrides.clear();
    }

    /// Oturum bitti.
    pub fn on_ended(&mut self) {
        self.status = ViewStatus::Idle;
        self.roots.clear();
    }

    /// Frame'in scope'larını ve açık değişkenleri yeniden yükler. `frame_id` yoksa (çalışıyor) yalnız durum değişir.
    /// Scope'lar ilk görüldüklerinde `expensive` değilse açık gelir.
    pub fn refresh(&mut self, src: &dyn VariableSource, frame_id: Option<i64>) -> Result<(), DapError> {
        let Some(frame) = frame_id else {
            self.status = ViewStatus::Running;
            return Ok(());
        };
        self.status = ViewStatus::Paused;
        let scopes = src.scopes(frame)?;
        self.current.clear();
        let mut roots = Vec::new();
        for s in scopes {
            if self.seen.insert(s.name.clone()) && !s.expensive {
                self.expanded.insert(s.name.clone());
            }
            let mut node = VarNode {
                is_scope: true,
                name: s.name.clone(),
                value: None,
                type_: None,
                reference: s.variables_reference,
                evaluate_name: None,
                parent_ref: None,
                path: s.name.clone(),
                depth: 0,
                expensive: s.expensive,
                children: Vec::new(),
            };
            self.load(src, &mut node);
            roots.push(node);
        }
        self.roots = roots;
        Ok(())
    }

    fn load(&mut self, src: &dyn VariableSource, node: &mut VarNode) {
        if node.reference == 0 || !self.expanded.contains(&node.path) {
            return;
        }
        let vars = src.variables(node.reference).unwrap_or_default();
        node.children = vars
            .into_iter()
            .map(|v| {
                let path = format!("{}{SEP}{}", node.path, v.name);
                let value = self.overrides.get(&path).cloned().unwrap_or(v.value);
                self.current.insert(path.clone(), value.clone());
                VarNode {
                    is_scope: false,
                    name: v.name,
                    value: Some(value),
                    type_: v.type_,
                    reference: v.variables_reference,
                    evaluate_name: v.evaluate_name,
                    parent_ref: Some(node.reference),
                    path,
                    depth: node.depth + 1,
                    expensive: false,
                    children: Vec::new(),
                }
            })
            .collect();
        let mut children = std::mem::take(&mut node.children);
        for c in &mut children {
            self.load(src, c);
        }
        node.children = children;
    }

    /// Düğüm (yolla).
    pub fn node(&self, path: &str) -> Option<&VarNode> {
        fn find<'a>(nodes: &'a [VarNode], path: &str) -> Option<&'a VarNode> {
            nodes.iter().find_map(|n| if n.path == path { Some(n) } else { find(&n.children, path) })
        }
        find(&self.roots, path)
    }

    /// Düğümün ebeveyni.
    pub fn parent(&self, path: &str) -> Option<&VarNode> {
        let (p, _) = path.rsplit_once(SEP)?;
        self.node(p)
    }

    /// Aç/kapa (sonra `refresh` gerekir).
    pub fn toggle(&mut self, path: &str) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_string());
        }
    }

    /// Açık mı.
    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded.contains(path)
    }

    /// Görünen tablo satırları.
    pub fn rows(&self) -> Vec<VarRow> {
        let mut out = Vec::new();
        fn add(m: &VariablesModel, n: &VarNode, out: &mut Vec<VarRow>) {
            let expanded = m.expanded.contains(&n.path);
            let icon = if n.reference > 0 { if expanded { "▾ " } else { "▸ " } } else { "  " };
            let name = match (n.is_scope, n.index()) {
                (false, Some(i)) => format!("[{i}]"),
                _ => n.name.clone(),
            };
            let value = n.value.clone().unwrap_or_default();
            out.push(VarRow {
                path: n.path.clone(),
                depth: n.depth,
                is_scope: n.is_scope,
                name_cell: format!("{}{icon}{name}", "  ".repeat(n.depth)),
                value: value.replace("\r\n", "↵").replace('\n', "↵"),
                type_: n.type_.clone().unwrap_or_default(),
                expandable: n.reference > 0,
                expanded,
                changed: !n.is_scope && m.baseline.get(&n.path).is_some_and(|b| *b != value),
            });
            if expanded {
                for c in &n.children {
                    add(m, c, out);
                }
            }
        }
        for r in &self.roots {
            add(self, r, &mut out);
        }
        out
    }

    /// Değer değiştirme isteği: `setVariable` (yetenek + ebeveyn), yoksa `setExpression` (`evaluateName`).
    pub fn set_request(
        &self,
        path: &str,
        value: &str,
        caps: &Capabilities,
        frame_id: i64,
    ) -> Result<SetRequest, String> {
        let n = self.node(path).filter(|n| !n.is_scope).ok_or("Not a variable")?;
        if caps.supports_set_variable
            && let Some(parent_ref) = n.parent_ref
        {
            return Ok(SetRequest::SetVariable { parent_ref, name: n.name.clone(), value: value.to_string() });
        }
        if caps.supports_set_expression
            && let Some(expr) = &n.evaluate_name
        {
            return Ok(SetRequest::SetExpression { expression: expr.clone(), value: value.to_string(), frame_id });
        }
        Err("This debugger can't change variables".into())
    }

    /// İsteği istemciyle gönderir (bloklar); başarılıysa yanıttaki değeri düzeltme olarak kaydeder.
    pub fn apply_set(&mut self, client: &DapClient, path: &str, req: &SetRequest) -> Result<String, DapError> {
        let resp = match req {
            SetRequest::SetVariable { parent_ref, name, value } => client.set_variable(*parent_ref, name, value)?,
            SetRequest::SetExpression { expression, value, frame_id } => {
                client.set_expression(expression, value, *frame_id)?
            }
        };
        let fallback = match req {
            SetRequest::SetVariable { value, .. } | SetRequest::SetExpression { value, .. } => value.clone(),
        };
        let v = if resp.value.is_empty() { fallback } else { resp.value };
        self.record_set(path, &v);
        Ok(v)
    }

    /// Başarılı değer değişikliğini bu duraklama için kaydeder.
    pub fn record_set(&mut self, path: &str, value: &str) {
        self.overrides.insert(path.to_string(), value.to_string());
    }

    /// Eleman silinince / eklenince listenin altındaki düzeltmeler silinir (dizinler kayar).
    pub fn clear_overrides_under(&mut self, parent_path: &str) {
        let prefix = format!("{parent_path}{SEP}");
        self.overrides.retain(|k, _| !k.starts_with(&prefix));
    }

    /// `n` / Del: null değeri (Python'da `None`).
    pub fn null_literal(lang: Option<Lang>) -> &'static str {
        if lang == Some(Lang::Python) { "None" } else { "null" }
    }

    /// `d`: elemanı listesinden / nesnesinden silen ifade ve ebeveyn yolu.
    pub fn delete_expr(&self, path: &str, lang: Option<Lang>) -> Result<(String, String), String> {
        let n = self.node(path).filter(|n| !n.is_scope).ok_or("Not a variable")?;
        let parent = self.parent(path).filter(|p| !p.is_scope);
        let target = parent
            .and_then(|p| p.evaluate_name.clone())
            .ok_or("Only elements of a list, map or object can be deleted")?;
        let key = serde_json::to_string(&n.name).unwrap_or_default();
        let i = n.index();
        let expr = match lang {
            Some(Lang::Js) => match i {
                Some(i) => format!("{target}.splice({i}, 1)"),
                None => format!("delete {target}[{key}]"),
            },
            Some(Lang::Java) => match i {
                Some(i) => format!("{target}.remove({i})"),
                None => format!("{target}.remove({key})"),
            },
            Some(Lang::Python) => format!("{target}.pop({})", i.map(|i| i.to_string()).unwrap_or(key)),
            None => return Err("Deleting elements is not supported for this debugger".into()),
        };
        Ok((expr, parent.map(|p| p.path.clone()).unwrap_or_default()))
    }

    /// `a`: eklenecek liste (imleç listede ya da elemanındayken) — yol ve `evaluateName`.
    pub fn append_target(&self, path: &str) -> Result<(String, String), String> {
        let n = self.node(path).filter(|n| !n.is_scope).ok_or("Put the cursor on a list to add an element")?;
        let list = if n.reference > 0 && n.evaluate_name.is_some() && n.index().is_none() {
            Some(n)
        } else {
            self.parent(path)
        };
        match list.filter(|l| !l.is_scope) {
            Some(l) if l.evaluate_name.is_some() => Ok((l.path.clone(), l.evaluate_name.clone().unwrap_or_default())),
            _ => Err("Put the cursor on a list to add an element".into()),
        }
    }

    /// Listeye ekleme ifadesi.
    pub fn append_expr(list_expr: &str, value: &str, lang: Option<Lang>) -> Result<String, String> {
        let method = match lang {
            Some(Lang::Js) => "push",
            Some(Lang::Java) => "add",
            Some(Lang::Python) => "append",
            None => return Err("Adding elements is not supported for this debugger".into()),
        };
        Ok(format!("{list_expr}.{method}({value})"))
    }

    /// Ekleme sonrası listeyi açık tut.
    pub fn expand(&mut self, path: &str) {
        self.expanded.insert(path.to_string());
    }
}
