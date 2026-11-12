use std::io::{BufReader, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui::{self, PointerButton, Rect, vec2};
use less_core::command::Command;
use less_input::Mods;
use less_lang::HighlightKind;
use less_lang::lsp::jsonrpc::read_message;
use less_lang::lsp::uri::path_to_uri;

use super::text::{Edit, apply_to_rope, char_of_pos, pos_of_char, semantic_kind, semantic_spans};
use super::*;
use crate::editor::view::{self, ViewOptions};
use crate::panels::Request;
use crate::panels::testing::Harness;

fn session() -> Session {
    Session::new(Box::new(MemoryClipboard::default()))
}

fn send(w: &mut impl Write, body: &str) {
    write!(w, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    w.flush().unwrap();
}

fn range(l0: u32, c0: u32, l1: u32, c1: u32) -> String {
    format!(r#"{{"start":{{"line":{l0},"character":{c0}}},"end":{{"line":{l1},"character":{c1}}}}}"#)
}

/// Geçici proje: `a.ts` (açılacak), `b.ts` (diskte), `other.ts` (tanım hedefi).
struct Project {
    root: PathBuf,
    a: PathBuf,
    b: PathBuf,
    other: PathBuf,
}

impl Project {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("less-b5-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let (a, b, other) = (root.join("a.ts"), root.join("b.ts"), root.join("other.ts"));
        std::fs::write(&a, "let abc = 1;\nfoo(abc);\n").unwrap();
        std::fs::write(&b, "abc();\n").unwrap();
        std::fs::write(&other, "x\ny\n    abc\n").unwrap();
        Self { root, a, b, other }
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Süreçsiz sahte dil sunucusu (boru üstünden). Gördüğü yöntemleri (didChange'de aralık + metin) `seen`'e yazar.
fn fake_lang(p: &Project) -> (Lang, Receiver<String>) {
    let (client_read, mut w) = std::io::pipe().unwrap();
    let (server_read, client_write) = std::io::pipe().unwrap();
    let (tx, rx) = mpsc::channel();
    let (a, b, other) = (path_to_uri(&p.a), path_to_uri(&p.b), path_to_uri(&p.other));
    thread::spawn(move || {
        let mut reader = BufReader::new(server_read);
        while let Ok(Some(msg)) = read_message(&mut reader) {
            let method = msg["method"].as_str().unwrap_or("").to_owned();
            let id = msg.get("id").map(ToString::to_string).unwrap_or_default();
            let mut seen = method.clone();
            if method == "textDocument/didChange" {
                let c = &msg["params"]["contentChanges"][0];
                let n = |v: &str, k: &str| c["range"][v][k].as_u64().unwrap_or(99);
                seen = format!(
                    "didChange {}:{}-{}:{} {}",
                    n("start", "line"),
                    n("start", "character"),
                    n("end", "line"),
                    n("end", "character"),
                    c["text"].as_str().unwrap_or("")
                );
            }
            let _ = tx.send(seen);
            let reply = |w: &mut std::io::PipeWriter, result: &str| {
                send(w, &format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{result}}}"#));
            };
            match method.as_str() {
                "initialize" => reply(
                    &mut w,
                    r#"{"capabilities":{"textDocumentSync":2,"hoverProvider":true,"definitionProvider":true,
                    "referencesProvider":true,"renameProvider":true,"codeActionProvider":true,
                    "executeCommandProvider":{"commands":["fake.run"]}}}"#,
                ),
                "textDocument/didOpen" | "textDocument/didChange" => {
                    let uri = msg["params"]["textDocument"]["uri"].as_str().unwrap_or("").to_owned();
                    send(
                        &mut w,
                        &format!(
                            r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{{"uri":"{uri}",
                            "diagnostics":[{{"range":{},"severity":1,"message":"Cannot find name 'foo'","source":"fake"}},
                            {{"range":{},"severity":2,"message":"unused"}}]}}}}"#,
                            range(1, 0, 1, 3),
                            range(0, 4, 0, 7)
                        ),
                    );
                }
                "textDocument/hover" => reply(&mut w, r#"{"contents":{"kind":"markdown","value":"**doc**"}}"#),
                "textDocument/definition" => {
                    reply(&mut w, &format!(r#"[{{"uri":"{other}","range":{}}}]"#, range(2, 4, 2, 7)))
                }
                "textDocument/references" => reply(
                    &mut w,
                    &format!(
                        r#"[{{"uri":"{a}","range":{}}},{{"uri":"{a}","range":{}}}]"#,
                        range(0, 4, 0, 7),
                        range(1, 4, 1, 7)
                    ),
                ),
                "textDocument/rename" => {
                    let name = msg["params"]["newName"].as_str().unwrap_or("").to_owned();
                    reply(
                        &mut w,
                        &format!(
                            r#"{{"changes":{{"{a}":[{{"range":{},"newText":"{name}"}},{{"range":{},"newText":"{name}"}}],
                            "{b}":[{{"range":{},"newText":"{name}"}}]}}}}"#,
                            range(0, 4, 0, 7),
                            range(1, 4, 1, 7),
                            range(0, 0, 0, 3)
                        ),
                    )
                }
                "textDocument/codeAction" => reply(
                    &mut w,
                    &format!(
                        r#"[{{"title":"Use const","kind":"quickfix","edit":{{"changes":{{"{a}":[{{"range":{},"newText":"const"}}]}}}}}},
                        {{"title":"Run fake","command":"fake.run","arguments":[1]}}]"#,
                        range(0, 0, 0, 3)
                    ),
                ),
                "workspace/executeCommand" => {
                    send(
                        &mut w,
                        &format!(
                            r#"{{"jsonrpc":"2.0","id":"ae","method":"workspace/applyEdit","params":{{"edit":{{"changes":
                            {{"{a}":[{{"range":{},"newText":"// ran\n"}}]}}}}}}}}"#,
                            range(0, 0, 0, 0)
                        ),
                    );
                    reply(&mut w, "null");
                }
                "shutdown" => reply(&mut w, "null"),
                "exit" => break,
                _ => {}
            }
        }
    });
    let config = LspConfig {
        path_dirs: vec![],
        extra_bin_dirs: vec![],
        cache_dir: p.root.join("cache"),
        jdk_search: less_lang::java::JdkSearch { homebrew_opt: vec![], jvm_dirs: vec![] },
        spring_boot: false,
        java_on_demand: false,
    };
    let mut lsp = LspManager::new(config);
    let settings = serde_json::Value::Object(Default::default());
    lsp.connect_streams("vtsls", &p.root, client_read, client_write, settings);
    (Lang::with_lsp(std::slice::from_ref(&p.root), lsp), rx)
}

fn pump(lang: &mut Lang, h: &mut Harness, until: impl Fn(&Lang) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !until(lang) && Instant::now() < deadline {
        lang.tick(&mut h.cx("lang"));
        thread::sleep(Duration::from_millis(5));
    }
    assert!(until(lang), "timed out waiting for the fake server");
}

fn wait_seen(rx: &Receiver<String>, prefix: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(s) = rx.recv_timeout(Duration::from_millis(50))
            && s.starts_with(prefix)
        {
            return s;
        }
    }
    panic!("server never saw {prefix}");
}

fn open(p: &Project, h: &mut Harness) -> (Lang, Receiver<String>, EditorTab) {
    let (mut lang, rx) = fake_lang(p);
    let tab = EditorTab::load(&p.a).unwrap();
    lang.opened(&tab, &mut h.cx("lang"));
    pump(&mut lang, h, |l| l.lsp.diagnostics().counts(&p.a).total() == 2);
    (lang, rx, tab)
}

#[test]
fn diagnostics_counter_jump_and_incremental_sync_31() {
    let p = Project::new("diag");
    let mut h = Harness::new(vec![p.root.clone()]);
    let (mut lang, rx, mut tab) = open(&p, &mut h);
    let mut s = session();

    let parts: Vec<String> = ui::counter_parts(&lang.lsp.diagnostics().counts(&p.a)).into_iter().map(|x| x.0).collect();
    assert_eq!(parts, ["\u{2716} 1", "\u{26a0} 1", "\u{2191}", "\u{2193}"]);
    assert_eq!(ui::counter_parts(&Default::default())[0].0, "\u{2713}");

    assert!(lang.event(&Event::Unmapped("]d"), &mut tab, &mut s, &mut h.cx("lang")));
    assert_eq!(tab.editor.cursor_line_col(), (1, 0), "arrows go to the highest severity first");
    assert!(lang.event(&Event::Unmapped("]w"), &mut tab, &mut s, &mut h.cx("lang")));
    assert_eq!(tab.editor.cursor_line_col(), (0, 4));
    lang.hit(Hit::Line(1), &mut tab, &mut h.cx("lang"));
    assert_eq!(tab.editor.cursor_line_col(), (1, 0));

    tab.editor.set_selection(Selection::point(tab.editor.buffer.rope().line_to_char(1) + 4));
    lang.apply_edits(&mut tab, &[], &mut s, &mut h.cx("lang"));
    let edit = tab.apply(Command::InsertText("\u{e9}x".into()), &mut s).edit.unwrap();
    lang.changed(&tab, &edit, &mut h.cx("lang"));
    assert_eq!(wait_seen(&rx, "didChange"), "didChange 1:4-1:4 \u{e9}x");
    let edit = tab.apply(Command::InsertText("y".into()), &mut s).edit.unwrap();
    lang.changed(&tab, &edit, &mut h.cx("lang"));
    assert_eq!(wait_seen(&rx, "didChange"), "didChange 1:6-1:6 y", "UTF-16 columns after a non-ASCII insert");
    assert_eq!(lang.docs[&p.a], tab.editor.text());
}

#[test]
fn hover_tooltip_joins_line_diagnostics_and_doc_drops_stale_52_70() {
    let p = Project::new("hover");
    let mut h = Harness::new(vec![p.root.clone()]);
    let (mut lang, _rx, mut tab) = open(&p, &mut h);
    let mut s = session();
    tab.editor.set_selection(Selection::point(tab.editor.buffer.rope().line_to_char(1) + 1));

    assert!(lang.event(&Event::Unmapped("K"), &mut tab, &mut s, &mut h.cx("lang")));
    let tip = lang.tip.clone().unwrap();
    assert_eq!(tip.anchor, Anchor::Cursor);
    assert!(tip.tooltip.lines[0].text.contains("Cannot find name 'foo'"));
    pump(&mut lang, &mut h, |l| l.tip.as_ref().is_some_and(|t| t.hover.is_none()));
    let tip = lang.tip.clone().unwrap();
    assert_eq!(tip.tooltip.doc.as_deref(), Some("**doc**"));
    assert_eq!(tip.tooltip.lines.last().unwrap().style, less_lang::TooltipStyle::Separator);

    assert!(!lang.event(&Event::Key(KeyEvent::char('j')), &mut tab, &mut s, &mut h.cx("lang")));
    assert!(lang.tip.is_none(), "a real key closes the tooltip");

    assert!(lang.event(&Event::Unmapped("K"), &mut tab, &mut s, &mut h.cx("lang")));
    lang.event(&Event::Key(KeyEvent::char('j')), &mut tab, &mut s, &mut h.cx("lang"));
    pump(&mut lang, &mut h, |l| l.pending.is_empty());
    assert!(lang.tip.is_none(), "a late hover response is dropped");
}

#[test]
fn definition_goes_to_column_and_references_open_a_list_13() {
    let p = Project::new("def");
    let mut h = Harness::new(vec![p.root.clone()]);
    let (mut lang, _rx, mut tab) = open(&p, &mut h);
    let mut s = session();

    assert!(lang.event(&Event::Unmapped("gd"), &mut tab, &mut s, &mut h.cx("lang")));
    pump(&mut lang, &mut h, |l| l.goto.is_some());
    assert!(h.requests.contains(&Request::OpenFile { path: p.other.clone(), line: Some(2) }));
    let mut other = EditorTab::load(&p.other).unwrap();
    lang.sync_tab(&mut other, &mut h.cx("lang"));
    lang.sync_tab(&mut other, &mut h.cx("lang"));
    assert_eq!(other.editor.cursor_line_col(), (2, 4), "column applied one frame after the host opened the file");

    assert!(lang.event(&Event::Unmapped("gr"), &mut tab, &mut s, &mut h.cx("lang")));
    pump(&mut lang, &mut h, |l| l.menu.is_some());
    let labels: Vec<String> = lang.menu.as_ref().unwrap().items.iter().map(|i| i.0.clone()).collect();
    assert_eq!(labels, ["a.ts:1  let abc = 1;", "a.ts:2  foo(abc);"]);
    assert!(lang.event(&Event::Key(KeyEvent::plain(Key::Down)), &mut tab, &mut s, &mut h.cx("lang")));
    assert!(lang.event(&Event::Key(KeyEvent::plain(Key::Enter)), &mut tab, &mut s, &mut h.cx("lang")));
    assert!(lang.menu.is_none());
    assert_eq!(lang.goto.as_ref().map(|g| g.1), Some(Pos { line: 1, character: 4 }));
}

#[test]
fn rename_updates_open_tab_in_one_undo_step_and_closed_file_on_disk_13() {
    let p = Project::new("rename");
    let mut h = Harness::new(vec![p.root.clone()]);
    let (mut lang, rx, mut tab) = open(&p, &mut h);
    let mut s = session();
    tab.editor.set_selection(Selection::point(5));

    assert!(lang.event(&Event::Leader("cr"), &mut tab, &mut s, &mut h.cx("lang")), "Space c r = rename");
    let mut prompt = lang.rename.take().unwrap();
    assert_eq!(prompt.text, "abc");
    prompt.text = "total".into();
    lang.submit_rename(prompt, &tab, &mut h.cx("lang"));
    pump(&mut lang, &mut h, |l| l.deferred.contains_key(&p.a));
    assert_eq!(std::fs::read_to_string(&p.b).unwrap(), "total();\n");

    lang.sync_tab(&mut tab, &mut h.cx("lang"));
    assert_eq!(tab.editor.text(), "let total = 1;\nfoo(total);");
    assert!(wait_seen(&rx, "didChange").starts_with("didChange 1:4-1:7"), "edits are sent bottom-up");
    tab.apply(Command::Undo { count: 1 }, &mut s);
    assert_eq!(tab.editor.text(), "let abc = 1;\nfoo(abc);");
}

#[test]
fn code_action_menu_applies_edit_and_runs_command_13() {
    let p = Project::new("action");
    let mut h = Harness::new(vec![p.root.clone()]);
    let (mut lang, rx, mut tab) = open(&p, &mut h);

    assert!(lang.event(&Event::Leader("ca"), &mut tab, &mut session(), &mut h.cx("lang")), "Space c a");
    pump(&mut lang, &mut h, |l| l.menu.is_some());
    let labels: Vec<String> = lang.menu.as_ref().unwrap().items.iter().map(|i| i.0.clone()).collect();
    assert_eq!(labels, ["Use const", "Run fake"]);

    lang.run_action(Action::Apply(0), &mut tab, &mut h.cx("lang"));
    lang.sync_tab(&mut tab, &mut h.cx("lang"));
    assert_eq!(tab.editor.text(), "const abc = 1;\nfoo(abc);");

    lang.run_action(Action::Apply(1), &mut tab, &mut h.cx("lang"));
    wait_seen(&rx, "workspace/executeCommand");
    pump(&mut lang, &mut h, |l| l.deferred.contains_key(&p.a));
    lang.sync_tab(&mut tab, &mut h.cx("lang"));
    assert_eq!(tab.editor.text(), "// ran\nconst abc = 1;\nfoo(abc);", "server applyEdit after executeCommand");
}

#[test]
fn cmd_f8_and_gutter_click_toggle_breakpoints_right_click_menu_92_13() {
    let p = Project::new("bp");
    let mut h = Harness::new(vec![p.root.clone()]);
    let mut lang = Lang::new(std::slice::from_ref(&p.root));
    let mut tab = EditorTab::load(&p.a).unwrap();
    let mut s = session();
    let cmd_f8 = KeyEvent::new(Key::F(8), Mods { cmd: true, ..Mods::NONE });

    assert!(lang.event(&Event::Shortcut(cmd_f8), &mut tab, &mut s, &mut h.cx("lang")));
    let toggle = Request::Send { topic: TOPIC_TOGGLE_BREAKPOINT.into(), data: format!("{}:1", p.a.display()) };
    assert_eq!(h.requests, std::slice::from_ref(&toggle));
    assert!(lang.breakpoints.contains(&(p.a.clone(), 0)));
    lang.event(&Event::Shortcut(cmd_f8), &mut tab, &mut s, &mut h.cx("lang"));
    assert!(lang.breakpoints.is_empty());

    for msg in [format!("{}:2,5", p.a.display()), format!("{}:9", p.b.display()), format!("{}:2", p.a.display())] {
        lang.event(&Event::Message { topic: TOPIC_BREAKPOINTS, data: &msg }, &mut tab, &mut s, &mut h.cx("lang"));
    }
    assert_eq!(lang.breakpoints, HashSet::from([(p.a.clone(), 1), (p.b.clone(), 8)]), "B2 format replaces per file");

    h.requests.clear();
    let ctx = egui::Context::default();
    let frame = |events: Vec<egui::Event>, lang: &mut Lang, tab: &mut EditorTab, h: &mut Harness| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 400.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let opts = ViewOptions { numbers: true, relative_numbers: false, focused: true };
            let (resp, geo) = view::show(ui, &h.palette.clone(), tab, &s, opts);
            let mut cx = h.cx("lang");
            lang.paint(tab, &geo, ui, &mut cx);
            lang.pointer(tab, &geo, &resp, &mut session(), &mut cx);
        });
        out.textures_delta.clear();
    };
    let click = |pos: egui::Pos2, button| {
        [true, false].map(|pressed| egui::Event::PointerButton { pos, button, pressed, modifiers: Default::default() })
    };
    let gutter = egui::pos2(6.0, 4.0);
    frame(vec![egui::Event::PointerMoved(gutter)], &mut lang, &mut tab, &mut h);
    for e in click(gutter, PointerButton::Primary) {
        frame(vec![e], &mut lang, &mut tab, &mut h);
    }
    assert!(h.requests.contains(&toggle), "gutter click on line 1: {:?}", h.requests);
    assert!(lang.breakpoints.contains(&(p.a.clone(), 0)));

    let text = egui::pos2(200.0, 4.0);
    frame(vec![egui::Event::PointerMoved(text)], &mut lang, &mut tab, &mut h);
    for e in click(text, PointerButton::Secondary) {
        frame(vec![e], &mut lang, &mut tab, &mut h);
    }
    let menu = lang.menu.as_ref().expect("right click opens the context menu");
    assert_eq!(menu.items[0].1, Action::Definition);
    assert!(menu.items.iter().any(|i| i.1 == Action::Rename) && menu.items.iter().any(|i| i.1 == Action::CodeActions));

    h.requests.clear();
    tab.apply(Command::SetCursorMode(CursorMode::Bar), &mut s);
    tab.editor.set_selection(Selection::single(Range::new(4, 7)));
    lang.run_action(Action::SendToAi, &mut tab, &mut h.cx("lang"));
    lang.run_action(Action::AddFileToAi, &mut tab, &mut h.cx("lang"));
    let sent = |d: &str| Request::Send { topic: "ai.send".into(), data: d.into() };
    assert_eq!(h.requests, [sent("abc"), sent("@a.ts ")]);
}

#[test]
fn utf16_positions_and_edit_order() {
    let rope = ropey::Rope::from_str("a\u{1F600}b\nxy");
    assert_eq!(pos_of_char(&rope, 2), Pos { line: 0, character: 3 });
    assert_eq!(char_of_pos(&rope, Pos { line: 0, character: 3 }), 2);
    assert_eq!(char_of_pos(&rope, Pos { line: 9, character: 0 }), rope.len_chars());
    let mut rope = ropey::Rope::from_str("ab");
    let at = |c| Pos { line: 0, character: c };
    let e = |s, e, t: &str| Edit { start: at(s), end: at(e), text: t.into() };
    apply_to_rope(&mut rope, &[e(1, 1, "1"), e(1, 1, "2"), e(0, 1, "A")]);
    assert_eq!(rope.to_string(), "A12b", "same-position inserts keep LSP order");
}

#[test]
fn semantic_tokens_map_to_theme_kinds_71() {
    assert_eq!(semantic_kind("method", &[]), Some(HighlightKind::Method));
    assert_eq!(semantic_kind("property", &["static", "readonly"]), Some(HighlightKind::Constant));
    assert_eq!(semantic_kind("unknownThing", &[]), None);
    let types = ["class", "variable"];
    let spans = semantic_spans(&[[1, 2, 3, 0, 0], [0, 5, 1, 1, 1], [2, 1, 1, 7, 0]], &types, &["readonly"]);
    assert_eq!(spans, [(1, 2, 3, HighlightKind::Type), (1, 7, 1, HighlightKind::Constant)]);
}

/// Gerçek vtsls ile `Lang` kancaları (`opened` + `tick`) üstünden sayaç dolar. Kurulu sunucuya bağlı: `#[ignore]`.
/// `LESS_REAL_FILE` verilirse o dosya açılır (diskte problem olmalı; ipucu `•` da sayılır).
#[test]
#[ignore]
fn real_server_fills_counter() {
    let p = Project::new("real");
    std::fs::write(p.root.join("pnpm-lock.yaml"), "").unwrap();
    std::fs::write(&p.a, "const n: number = \"text\";\nexport {};\n").unwrap();
    let file = std::env::var_os("LESS_REAL_FILE").map_or(p.a.clone(), PathBuf::from);
    let mut h = Harness::new(vec![p.root.clone()]);
    let mut lang = Lang::new(std::slice::from_ref(&p.root));
    let tab = EditorTab::load(&file).unwrap();
    lang.opened(&tab, &mut h.cx("lang"));
    let deadline = Instant::now() + Duration::from_secs(240);
    while lang.lsp.diagnostics().counts(&file).total() == 0 && Instant::now() < deadline {
        lang.tick(&mut h.cx("lang"));
        thread::sleep(Duration::from_millis(50));
    }
    let parts: Vec<String> =
        ui::counter_parts(&lang.lsp.diagnostics().counts(&file)).into_iter().map(|x| x.0).collect();
    eprintln!("counter {parts:?}, requests {:?}", h.requests);
    lang.lsp.shutdown_all(Duration::from_secs(2));
    assert_ne!(parts, ["\u{2713}"]);
}

/// Tamamlama + imza yardımı veren sahte sunucu; `c.ts` için.
fn completion_lang(p: &Project) -> (Lang, Receiver<String>) {
    let (client_read, mut w) = std::io::pipe().unwrap();
    let (server_read, client_write) = std::io::pipe().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(server_read);
        while let Ok(Some(msg)) = read_message(&mut reader) {
            let method = msg["method"].as_str().unwrap_or("").to_owned();
            let id = msg.get("id").map(ToString::to_string).unwrap_or_default();
            let _ = tx.send(format!("{method} {}", msg["params"]["context"]));
            let reply = |w: &mut std::io::PipeWriter, result: &str| {
                send(w, &format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{result}}}"#));
            };
            match method.as_str() {
                "initialize" => reply(
                    &mut w,
                    r#"{"capabilities":{"textDocumentSync":2,
                    "completionProvider":{"triggerCharacters":["."],"resolveProvider":true},
                    "signatureHelpProvider":{"triggerCharacters":["(",","]}}}"#,
                ),
                "textDocument/completion" => reply(
                    &mut w,
                    &format!(
                        r#"{{"isIncomplete":false,"items":[
                        {{"label":"alpha","kind":6,"sortText":"1","textEdit":{{"range":{},"newText":"alpha"}}}},
                        {{"label":"alphabet","kind":3,"sortText":"2","insertTextFormat":2,"insertText":"alphabet(${{1:x}})$0",
                          "additionalTextEdits":[{{"range":{},"newText":"import {{ alphabet }} from './abc';\n"}}]}},
                        {{"label":"beta","kind":6}}]}}"#,
                        range(1, 0, 1, 2),
                        range(0, 0, 0, 0)
                    ),
                ),
                "completionItem/resolve" => {
                    let mut item = msg["params"].clone();
                    item["documentation"] = serde_json::json!({ "kind": "markdown", "value": "Alpha docs" });
                    item["detail"] = serde_json::json!("const alpha: number");
                    reply(&mut w, &item.to_string());
                }
                "textDocument/signatureHelp" => reply(
                    &mut w,
                    r#"{"signatures":[{"label":"alphabet(x: number, y: string)","parameters":[{"label":"x: number"},{"label":[20,29]}]}],"activeParameter":1}"#,
                ),
                "shutdown" => reply(&mut w, "null"),
                "exit" => break,
                _ => {}
            }
        }
    });
    let config = LspConfig {
        path_dirs: vec![],
        extra_bin_dirs: vec![],
        cache_dir: p.root.join("cache"),
        jdk_search: less_lang::java::JdkSearch { homebrew_opt: vec![], jvm_dirs: vec![] },
        spring_boot: false,
        java_on_demand: false,
    };
    let mut lsp = LspManager::new(config);
    lsp.connect_streams("vtsls", &p.root, client_read, client_write, serde_json::json!({}));
    (Lang::with_lsp(std::slice::from_ref(&p.root), lsp), rx)
}

/// Yazılan karakter: önce lang (motordan önce), sonra motorun ekleyeceği metin + `changed`.
fn type_char(lang: &mut Lang, tab: &mut EditorTab, s: &mut Session, h: &mut Harness, c: char) {
    assert!(!lang.event(&Event::Key(KeyEvent::char(c)), tab, s, &mut h.cx("lang")), "typing is not consumed");
    let edit = tab.apply(Command::InsertText(c.to_string()), s).edit.unwrap();
    lang.changed(tab, &edit, &mut h.cx("lang"));
}

fn labels(lang: &Lang) -> Vec<String> {
    let c = lang.completion.as_ref().unwrap();
    c.shown.iter().map(|&i| c.items[i].item.label.clone()).collect()
}

#[test]
fn completion_filters_resolves_and_applies_edits_in_one_undo_step() {
    let p = Project::new("complete");
    let c = p.root.join("c.ts");
    std::fs::write(&c, "let alpha = 1;\nal\n").unwrap();
    let mut h = Harness::new(vec![p.root.clone()]);
    let (mut lang, rx) = completion_lang(&p);
    let mut tab = EditorTab::load(&c).unwrap();
    lang.opened(&tab, &mut h.cx("lang"));
    let mut s = session();
    tab.apply(Command::SetCursorMode(CursorMode::Bar), &mut s);
    tab.editor.set_selection(Selection::point(17));
    pump(&mut lang, &mut h, |l| l.lsp.server_states().iter().any(|(_, st)| *st == ServerState::Running));
    assert_eq!(lang.active_servers(), [ServerStatus { name: "vtsls".into(), state: ServerState::Running }]);

    let ctrl_space = KeyEvent::new(Key::Char(' '), Mods { ctrl: true, ..Mods::default() });
    assert!(lang.event(&Event::Key(ctrl_space), &mut tab, &mut s, &mut h.cx("lang")), "Ctrl+Space");
    assert!(wait_seen(&rx, "textDocument/completion").contains(r#""triggerKind":1"#));
    pump(&mut lang, &mut h, |l| l.completion.as_ref().is_some_and(|c| c.pending.is_empty()));
    lang.completion_frame(&tab, &mut h.cx("lang"));
    assert_eq!(labels(&lang), ["alpha", "alphabet"], "fuzzy filter by the typed prefix");
    pump(&mut lang, &mut h, |l| {
        l.completion.as_ref().is_some_and(|c| complete::documentation(&c.items[0].item).is_some())
    });
    assert_eq!(lang.completion.as_ref().unwrap().items[0].item.detail.as_deref(), Some("const alpha: number"));

    type_char(&mut lang, &mut tab, &mut s, &mut h, 'p');
    lang.completion_frame(&tab, &mut h.cx("lang"));
    assert_eq!(labels(&lang), ["alpha", "alphabet"]);
    assert!(lang.debounce.is_none(), "complete list: no new request while filtering");

    let down = KeyEvent::plain(Key::Down);
    assert!(lang.event(&Event::Key(down), &mut tab, &mut s, &mut h.cx("lang")));
    assert!(lang.event(&Event::Key(KeyEvent::plain(Key::Enter)), &mut tab, &mut s, &mut h.cx("lang")));
    assert!(lang.completion.is_none());
    assert_eq!(tab.editor.text(), "import { alphabet } from './abc';\nlet alpha = 1;\nalphabet(x)");
    assert_eq!(tab.editor.cursor_line_col(), (2, 11), "caret after the inserted text (snippet placeholders stripped)");
    tab.apply(Command::Undo { count: 1 }, &mut s);
    assert_eq!(tab.editor.text(), "let alpha = 1;\nalp", "item + auto-import are one undo step");
    tab.editor.set_selection(Selection::point(18));

    // textEdit aralığı istek anına göre: sonradan yazılan harfi de kapsar.
    assert!(lang.event(&Event::Key(ctrl_space), &mut tab, &mut s, &mut h.cx("lang")));
    pump(&mut lang, &mut h, |l| l.completion.as_ref().is_some_and(|c| c.pending.is_empty()));
    type_char(&mut lang, &mut tab, &mut s, &mut h, 'h');
    lang.completion_frame(&tab, &mut h.cx("lang"));
    assert_eq!(labels(&lang), ["alpha", "alphabet"]);
    assert!(lang.event(&Event::Key(KeyEvent::plain(Key::Tab)), &mut tab, &mut s, &mut h.cx("lang")), "Tab accepts");
    assert_eq!(tab.editor.text(), "let alpha = 1;\nalpha");

    // Esc: Vim'de liste kapanır, tuş motora gider (insert'ten çıkış).
    assert!(lang.event(&Event::Key(ctrl_space), &mut tab, &mut s, &mut h.cx("lang")));
    pump(&mut lang, &mut h, |l| l.completion.as_ref().is_some_and(|c| c.pending.is_empty()));
    lang.completion_frame(&tab, &mut h.cx("lang"));
    assert!(!lang.event(&Event::Key(KeyEvent::plain(Key::Esc)), &mut tab, &mut s, &mut h.cx("lang")));
    assert!(lang.completion.is_none());

    // Tetikleyici karakter hemen ister; kelime karakteri gecikmeli.
    type_char(&mut lang, &mut tab, &mut s, &mut h, '.');
    while !wait_seen(&rx, "textDocument/completion").contains(r#""triggerCharacter":".""#) {}
    lang.completion = None;
    type_char(&mut lang, &mut tab, &mut s, &mut h, 'x');
    assert!(lang.debounce.is_some() && lang.completion.is_none());
    thread::sleep(Duration::from_millis(120));
    lang.completion_frame(&tab, &mut h.cx("lang"));
    assert!(lang.debounce.is_none() && lang.completion.is_some(), "debounced request sent");
    assert_eq!(lang.completion.as_ref().unwrap().anchor, tab.editor.cursor() - 1);
}

#[test]
fn signature_help_on_paren_and_comma_closes_on_line_change() {
    let p = Project::new("signature");
    let c = p.root.join("c.ts");
    std::fs::write(&c, "alphabet\n").unwrap();
    let mut h = Harness::new(vec![p.root.clone()]);
    let (mut lang, _rx) = completion_lang(&p);
    let mut tab = EditorTab::load(&c).unwrap();
    lang.opened(&tab, &mut h.cx("lang"));
    let mut s = session();
    tab.apply(Command::SetCursorMode(CursorMode::Bar), &mut s);
    tab.editor.set_selection(Selection::point(8));
    pump(&mut lang, &mut h, |l| l.lsp.server_states().iter().any(|(_, st)| *st == ServerState::Running));

    type_char(&mut lang, &mut tab, &mut s, &mut h, '(');
    pump(&mut lang, &mut h, |l| l.signature.as_ref().is_some_and(|s| s.help.is_some()));
    let view = lang.signature.as_ref().unwrap().help.clone().unwrap();
    assert_eq!(view.label, "alphabet(x: number, y: string)");
    assert_eq!(view.active.map(|(a, b)| &view.label[a..b]), Some("y: string"), "UTF-16 label offsets");
    type_char(&mut lang, &mut tab, &mut s, &mut h, ',');
    assert!(lang.signature.as_ref().unwrap().pending.is_some(), "',' re-triggers");
    lang.completion_frame(&tab, &mut h.cx("lang"));
    assert!(lang.signature.is_some());
    tab.apply(Command::InsertText("\n".into()), &mut s);
    lang.completion_frame(&tab, &mut h.cx("lang"));
    assert!(lang.signature.is_none(), "moving to another line closes it");
}

#[test]
fn fuzzy_scores_and_snippet_stripping() {
    use complete::{fuzzy, strip_snippet};
    assert!(fuzzy("gd", "getData").unwrap() > fuzzy("gd", "lodged").unwrap(), "word boundaries win");
    assert!(fuzzy("get", "getData").unwrap() > fuzzy("get", "forget").unwrap(), "prefix wins");
    assert_eq!(fuzzy("xz", "getData"), None);
    assert_eq!(fuzzy("", "x"), Some(0));
    assert_eq!(strip_snippet("foo(${1:a}, ${2:b})$0"), "foo(a, b)");
    assert_eq!(strip_snippet("${1|public,private|} class ${TM_FILENAME_BASE} {\n\t$0\n}"), "public class  {\n\t\n}");
    assert_eq!(strip_snippet("a ${1:x ${2:y}} \\$z"), "a x y $z");
}

#[test]
fn completion_popup_renders_headless_and_click_accepts() {
    let p = Project::new("complete-ui");
    let c = p.root.join("c.ts");
    std::fs::write(&c, "let alpha = 1;\nal\n").unwrap();
    let mut h = Harness::new(vec![p.root.clone()]);
    let (mut lang, _rx) = completion_lang(&p);
    let mut tab = EditorTab::load(&c).unwrap();
    lang.opened(&tab, &mut h.cx("lang"));
    let s = session();
    tab.apply(Command::SetCursorMode(CursorMode::Bar), &mut session());
    tab.editor.set_selection(Selection::point(17));
    pump(&mut lang, &mut h, |l| l.lsp.server_states().iter().any(|(_, st)| *st == ServerState::Running));
    lang.request_completion(&tab, None, true, &mut h.cx("lang"));
    pump(&mut lang, &mut h, |l| l.completion.as_ref().is_some_and(|c| c.pending.is_empty()));

    let ctx = egui::Context::default();
    let geo_at = std::cell::Cell::new((egui::Pos2::ZERO, 0.0));
    let frame = |events: Vec<egui::Event>, lang: &mut Lang, tab: &mut EditorTab, h: &mut Harness| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 500.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let opts = ViewOptions { numbers: true, relative_numbers: false, focused: true };
            let (_, geo) = view::show(ui, &h.palette.clone(), tab, &s, opts);
            geo_at.set((geo.pos_of(tab.editor.buffer.rope(), 15).unwrap(), geo.line_h));
            lang.paint(tab, &geo, ui, &mut h.cx("lang"));
        });
        out.textures_delta.clear();
    };
    frame(vec![], &mut lang, &mut tab, &mut h);
    frame(vec![], &mut lang, &mut tab, &mut h);
    assert_eq!(labels(&lang), ["alpha", "alphabet"]);
    let (anchor, line_h) = geo_at.get();
    let row2 = egui::pos2(anchor.x + 60.0, anchor.y + line_h + 2.0 + 5.0 + 30.0);
    frame(vec![egui::Event::PointerMoved(row2)], &mut lang, &mut tab, &mut h);
    for pressed in [true, false] {
        let e = egui::Event::PointerButton {
            pos: row2,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(vec![e], &mut lang, &mut tab, &mut h);
    }
    assert!(lang.completion.is_none(), "click accepts");
    assert!(tab.editor.text().ends_with("alphabet(x)"), "{}", tab.editor.text());
}
