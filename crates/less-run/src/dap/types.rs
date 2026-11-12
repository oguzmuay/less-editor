//! DAP tipleri (yalnız kullanılan alanlar).

use serde::{Deserialize, Serialize};

/// Adaptör yetenekleri (`initialize` yanıtı).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Capabilities {
    pub supports_configuration_done_request: bool,
    pub supports_set_variable: bool,
    pub supports_set_expression: bool,
    pub supports_conditional_breakpoints: bool,
    pub supports_evaluate_for_hovers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Thread {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Source {
    pub name: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub line: i64,
    #[serde(default)]
    pub column: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub name: String,
    #[serde(default)]
    pub variables_reference: i64,
    #[serde(default)]
    pub expensive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    #[serde(default)]
    pub variables_reference: i64,
    #[serde(default)]
    pub evaluate_name: Option<String>,
}

/// `setBreakpoints` isteğindeki breakpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBreakpoint {
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// Adaptörün doğruladığı breakpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Breakpoint {
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub line: Option<i64>,
    #[serde(default)]
    pub message: Option<String>,
}

/// `evaluate` / `setVariable` / `setExpression` yanıtının değeri.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueResponse {
    #[serde(default, alias = "result")]
    pub value: String,
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    #[serde(default)]
    pub variables_reference: i64,
}
