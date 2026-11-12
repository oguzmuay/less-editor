use std::path::Path;

use less_run::breakpoints::BreakpointStore;
use less_run::dap::Lang;
use less_run::dap::client::DapError;
use less_run::dap::types::{Capabilities, Scope, SourceBreakpoint, Variable};
use less_run::variables::{SetRequest, VariableSource, VariablesModel, ViewStatus};

/// java-debug benzeri sahte kaynak: `this.items` listesi (`[0]` adları) ve `this.map`.
struct Java;

fn var(name: &str, value: &str, reference: i64, eval: Option<&str>) -> Variable {
    Variable {
        name: name.into(),
        value: value.into(),
        type_: Some("T".into()),
        variables_reference: reference,
        evaluate_name: eval.map(String::from),
    }
}

impl VariableSource for Java {
    fn scopes(&self, _frame: i64) -> Result<Vec<Scope>, DapError> {
        Ok(vec![Scope { name: "Local".into(), variables_reference: 1, expensive: false }])
    }

    fn variables(&self, reference: i64) -> Result<Vec<Variable>, DapError> {
        Ok(match reference {
            1 => vec![
                var("items", "ArrayList (size=2)", 2, Some("this.items")),
                var("map", "HashMap", 3, Some("this.map")),
                var("n", "4", 0, None),
            ],
            2 => vec![var("[0]", "\"a\"", 0, Some("this.items.get(0)")), var("[1]", "\"b\"", 0, None)],
            3 => vec![var("key", "1", 0, Some("this.map.get(\"key\")"))],
            _ => vec![],
        })
    }
}

fn model() -> VariablesModel {
    let mut m = VariablesModel::new();
    m.refresh(&Java, Some(1)).unwrap();
    m.toggle("Local\0items");
    m.toggle("Local\0map");
    m.refresh(&Java, Some(1)).unwrap();
    m
}

#[test]
fn delete_and_append_expressions_per_language_97() {
    let m = model();
    assert_eq!(m.status, ViewStatus::Paused);
    let el = "Local\0items\0[1]";
    assert_eq!(m.delete_expr(el, Some(Lang::Java)).unwrap(), ("this.items.remove(1)".into(), "Local\0items".into()));
    assert_eq!(m.delete_expr(el, Some(Lang::Js)).unwrap().0, "this.items.splice(1, 1)");
    assert_eq!(m.delete_expr(el, Some(Lang::Python)).unwrap().0, "this.items.pop(1)");
    let key = "Local\0map\0key";
    assert_eq!(m.delete_expr(key, Some(Lang::Java)).unwrap().0, r#"this.map.remove("key")"#);
    assert_eq!(m.delete_expr(key, Some(Lang::Js)).unwrap().0, r#"delete this.map["key"]"#);
    assert_eq!(m.delete_expr(key, Some(Lang::Python)).unwrap().0, r#"this.map.pop("key")"#);
    assert!(m.delete_expr("Local\0n", Some(Lang::Java)).is_err(), "scope members are not deletable");
    assert!(m.delete_expr(el, None).is_err());

    assert_eq!(m.append_target(el).unwrap(), ("Local\0items".into(), "this.items".into()));
    assert_eq!(m.append_target("Local\0items").unwrap().1, "this.items");
    assert!(m.append_target("Local\0n").is_err());
    assert_eq!(VariablesModel::append_expr("this.items", "\"c\"", Some(Lang::Java)).unwrap(), r#"this.items.add("c")"#);
    assert_eq!(VariablesModel::append_expr("xs", "1", Some(Lang::Python)).unwrap(), "xs.append(1)");
    assert_eq!(VariablesModel::null_literal(Some(Lang::Python)), "None");
    assert_eq!(VariablesModel::null_literal(Some(Lang::Java)), "null");
    assert_eq!(Lang::detect("java", None), Some(Lang::Java));
    assert_eq!(Lang::detect("codelldb", Some("/a/b.py")), Some(Lang::Python));
    assert_eq!(Lang::detect("codelldb", Some("/a/b.rs")), None);
}

#[test]
fn set_request_falls_back_to_set_expression_and_overrides_clear_91() {
    let mut m = model();
    let el = "Local\0items\0[0]";
    let expr_only = Capabilities { supports_set_expression: true, ..Default::default() };
    assert_eq!(
        m.set_request(el, "\"z\"", &expr_only, 9).unwrap(),
        SetRequest::SetExpression { expression: "this.items.get(0)".into(), value: "\"z\"".into(), frame_id: 9 }
    );
    assert!(m.set_request("Local\0items\0[1]", "1", &expr_only, 9).is_err(), "no evaluateName");
    assert!(m.set_request(el, "1", &Capabilities::default(), 9).is_err());
    assert!(m.set_request("Local", "1", &expr_only, 9).is_err(), "scopes are not editable");

    m.record_set(el, "\"z\"");
    m.record_set("Local\0n", "5");
    m.clear_overrides_under("Local\0items");
    m.refresh(&Java, Some(1)).unwrap();
    assert_eq!(m.node(el).unwrap().value.as_deref(), Some("\"a\""), "element override dropped (indexes shift)");
    assert_eq!(m.node("Local\0n").unwrap().value.as_deref(), Some("5"));

    m.on_continued();
    assert_eq!(m.status, ViewStatus::Running);
    m.refresh(&Java, None).unwrap();
    assert_eq!(m.status, ViewStatus::Running);
    assert!(!m.roots().is_empty(), "running keeps the last table (dimmed)");
    m.on_ended();
    assert!(m.roots().is_empty() && m.status == ViewStatus::Idle);
}

#[test]
fn breakpoints_toggle_condition_and_persist_per_project_92() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("breakpoints.json");
    let (a, b) = (Path::new("/p/A.java"), Path::new("/p/B.java"));
    let mut s = BreakpointStore::open(file.clone(), Path::new("/p"));
    assert!(s.toggle(b, 4).unwrap());
    assert!(s.toggle(a, 10).unwrap());
    assert!(s.toggle(a, 2).unwrap());
    s.set_condition(a, 10, Some(" i > 3 ")).unwrap();
    s.set_condition(a, 20, Some("")).unwrap();
    let mut other = BreakpointStore::open(file.clone(), Path::new("/q"));
    other.toggle(a, 1).unwrap();

    let s = BreakpointStore::open(file.clone(), Path::new("/p"));
    let lines: Vec<_> = s.all().iter().map(|e| (e.file.clone(), e.line)).collect();
    assert_eq!(lines, [(a.into(), 2), (a.into(), 10), (a.into(), 20), (b.into(), 4)], "sorted, reloaded from disk");
    assert_eq!(s.for_file(a)[1], SourceBreakpoint { line: 10, condition: Some("i > 3".into()) });
    assert_eq!(s.for_file(a)[2].condition, None, "empty condition = unconditional");
    assert_eq!(s.by_file().len(), 2);

    let mut s = s;
    assert!(!s.toggle(a, 2).unwrap(), "second toggle removes");
    s.clear_file(a).unwrap();
    s.clear_file(b).unwrap();
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(json["version"], 1);
    assert!(json["projects"].get("/p").is_none(), "empty project is removed");
    assert_eq!(json["projects"]["/q"][0]["line"], 1, "other projects untouched");
}
