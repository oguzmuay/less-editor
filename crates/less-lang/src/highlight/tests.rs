use super::*;

/// Satırları `metin=tür` belirteçlerine çevirir (anlık görüntü karşılaştırması için).
fn snapshot(lang: Language, text: &str) -> Vec<String> {
    let syntax = Syntax::new(lang, text).expect("grammar");
    let rows = text.lines().count();
    let lines: Vec<&str> = text.lines().collect();
    syntax
        .highlight_lines(text, 0..rows)
        .iter()
        .enumerate()
        .map(|(i, spans)| {
            spans
                .iter()
                .map(|s| format!("{}={}", &lines[i][s.start..s.end], s.kind.name()))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

#[test]
fn all_grammars_compile() {
    for lang in Language::ALL {
        assert_eq!(grammar_error(lang), None, "{lang:?}");
        assert!(Syntax::new(lang, "").is_some(), "{lang:?}");
    }
}

fn has(line: &str, token: &str) -> bool {
    line.split(' ').any(|t| t == token)
}

#[test]
fn java_snapshot() {
    let src = "// hi\npublic class Foo {\n  @Override\n  String s() { return \"x\"; }\n}\n";
    let snap = snapshot(Language::Java, src);
    assert_eq!(
        snap,
        [
            "// hi=comment",
            "public=keyword class=keyword Foo=type",
            "@=operator Override=attribute",
            "String=type s=function.method return=keyword \"x\"=string",
            "",
        ]
    );
}

#[test]
fn typescript_and_tsx_snapshot() {
    let ts = snapshot(Language::TypeScript, "const x: number = 1;\n");
    assert_eq!(ts, ["const=keyword x=variable number=type.builtin ==operator 1=number ;=punctuation"]);
    let tsx = snapshot(Language::Tsx, "const A = <div className=\"x\">hi</div>;\n");
    assert_eq!(
        tsx,
        [
            "const=keyword A=type ==operator <=punctuation.bracket div=tag className=attribute ==operator \"x\"=string >=punctuation.bracket </=punctuation.bracket div=tag >=punctuation.bracket ;=punctuation",
        ]
    );
}

#[test]
fn markdown_inline_and_fenced_injection() {
    let snap = snapshot(Language::Markdown, "# Title\n\nSome *em* and `code`.\n\n```java\nint x = 1;\n```\n");
    assert_eq!(
        snap,
        [
            "#=punctuation.special Title=markup.heading",
            "",
            "*=punctuation em=markup.italic *=punctuation `=punctuation code=markup.raw `=punctuation",
            "",
            "```=punctuation java=markup.raw",
            "int=type.builtin x=variable 1=number",
            "```=punctuation",
        ]
    );
}

#[test]
fn sql_lua_json_snapshot() {
    let sql = snapshot(Language::Sql, "SELECT id FROM users; -- c\n");
    assert_eq!(sql, ["SELECT=keyword id=property FROM=keyword users=type ;=punctuation -- c=comment"]);
    let lua = snapshot(Language::Lua, "function f() return true end\n");
    assert_eq!(lua[0], "function=keyword f=function ()=punctuation.bracket return=keyword true=boolean end=keyword");
    let json = snapshot(Language::Json, "{\"a\": 1}\n");
    assert!(has(&json[0], "1=number"), "{}", json[0]);
}

fn assert_incremental_matches_fresh(lang: Language, old: &str, start: usize, old_end: usize, inserted: &str) {
    let edit = TextEdit::compute(old, start, old_end, inserted);
    let mut new = String::with_capacity(old.len() - (old_end - start) + inserted.len());
    new.push_str(&old[..start]);
    new.push_str(inserted);
    new.push_str(&old[old_end..]);
    let mut syntax = Syntax::new(lang, old).unwrap();
    syntax.update(&edit, new.as_str());
    let fresh = Syntax::new(lang, new.as_str()).unwrap();
    let rows = new.lines().count();
    assert_eq!(syntax.highlight_lines(new.as_str(), 0..rows), fresh.highlight_lines(new.as_str(), 0..rows), "{lang:?}");
}

#[test]
fn incremental_update_matches_fresh_parse() {
    let java = "class A {\n  int x = 1;\n}\n";
    let java_at = java.find('1').unwrap();
    assert_incremental_matches_fresh(Language::Java, java, java_at, java_at + 1, "\"s\"");

    let ts = "const x = 1;\nconst y = 2;\n";
    assert_incremental_matches_fresh(Language::TypeScript, ts, 0, 0, "export ");

    let markdown = "# Title\n\n```java\nint x = 1;\n```\n";
    let markdown_at = markdown.find('1').unwrap();
    assert_incremental_matches_fresh(Language::Markdown, markdown, markdown_at, markdown_at + 1, "value");

    let sql = "SELECT id FROM users;\n";
    let sql_at = sql.find("id").unwrap();
    assert_incremental_matches_fresh(Language::Sql, sql, sql_at, sql_at + 2, "name,\n    email");

    let edit = TextEdit::compute(java, java_at, java_at + 1, "\"s\"");
    assert_eq!(edit.new_end, TextPoint { row: 1, column: 13 });
}

#[test]
fn partial_row_range() {
    let src = "a = 1\nb = \"x\"\n";
    let syntax = Syntax::new(Language::Toml, src).unwrap();
    let rows = syntax.highlight_lines(src, 1..2);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].iter().any(|s| s.kind == HighlightKind::String && &src[6 + s.start..6 + s.end] == "\"x\""));
}
