use std::io::BufReader;
use std::sync::mpsc;
use std::thread;

use super::jsonrpc::{read_message, write_message};
use super::*;

/// Süreçsiz sahte sunucu: boru üstünden konuşur, gördüğü mesaj adlarını `seen` kanalına yollar.
fn fake_server(
    reader: std::io::PipeReader,
    mut writer: std::io::PipeWriter,
    seen: mpsc::Sender<Value>,
    sync_kind: u64,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        while let Ok(Some(msg)) = read_message(&mut reader) {
            let method = msg["method"].as_str().unwrap_or("").to_string();
            let id = msg.get("id").cloned();
            let _ = seen.send(msg.clone());
            let reply = |w: &mut std::io::PipeWriter, result: Value| {
                write_message(w, &json!({ "jsonrpc": "2.0", "id": id.clone().unwrap(), "result": result })).unwrap();
            };
            match method.as_str() {
                "initialize" => {
                    write_message(
                        &mut writer,
                        &json!({ "jsonrpc": "2.0", "id": "cfg", "method": "workspace/configuration",
                        "params": { "items": [{ "section": "fake.level" }] } }),
                    )
                    .unwrap();
                    reply(
                        &mut writer,
                        json!({ "capabilities": {
                            "textDocumentSync": sync_kind, "hoverProvider": true, "definitionProvider": true
                        }}),
                    );
                }
                "textDocument/didOpen" | "textDocument/didChange" => {
                    let uri = msg["params"]["textDocument"]["uri"].clone();
                    let version = msg["params"]["textDocument"]["version"].as_i64().unwrap();
                    write_message(
                        &mut writer,
                        &json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                        "params": { "uri": uri, "diagnostics": [{
                            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
                            "severity": 1, "message": format!("v{version}"), "source": "fake"
                        }]}}),
                    )
                    .unwrap();
                }
                "textDocument/hover" => {
                    reply(&mut writer, json!({ "contents": { "kind": "markdown", "value": "**doc**" } }))
                }
                "textDocument/definition" => reply(
                    &mut writer,
                    json!([{
                        "targetUri": "file:///tmp/x/B.ts",
                        "targetRange": { "start": { "line": 1, "character": 0 }, "end": { "line": 3, "character": 0 } },
                        "targetSelectionRange": { "start": { "line": 2, "character": 4 }, "end": { "line": 2, "character": 7 } }
                    }]),
                ),
                "shutdown" => reply(&mut writer, Value::Null),
                "exit" => break,
                _ => {}
            }
        }
    })
}

/// Yeteneklerin tamamını destekleyen sahte sunucu; isteklere türlü yanıtlar ve sunucu olayları üretir.
fn full_server(
    reader: std::io::PipeReader,
    mut writer: std::io::PipeWriter,
    seen: mpsc::Sender<Value>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        while let Ok(Some(msg)) = read_message(&mut reader) {
            let method = msg["method"].as_str().unwrap_or("").to_string();
            let id = msg.get("id").cloned();
            let _ = seen.send(msg.clone());
            let reply = |w: &mut std::io::PipeWriter, result: Value| {
                write_message(w, &json!({ "jsonrpc": "2.0", "id": id.clone().unwrap(), "result": result })).unwrap();
            };
            let range = json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } });
            match method.as_str() {
                "initialize" => reply(
                    &mut writer,
                    json!({ "capabilities": {
                        "textDocumentSync": { "change": 2, "save": { "includeText": true } },
                        "hoverProvider": true,
                        "referencesProvider": true,
                        "implementationProvider": true,
                        "typeDefinitionProvider": true,
                        "renameProvider": true,
                        "codeActionProvider": true,
                        "completionProvider": {},
                        "documentSymbolProvider": true,
                        "documentFormattingProvider": true,
                        "semanticTokensProvider": { "legend": { "tokenTypes": ["type"], "tokenModifiers": [] } },
                        "executeCommandProvider": { "commands": ["fix.apply"] }
                    }}),
                ),
                "initialized" => {
                    for message in [
                        json!({ "jsonrpc": "2.0", "id": "apply", "method": "workspace/applyEdit", "params": {
                            "label": "Apply fix", "edit": { "changes": { "file:///tmp/x/B.ts": [
                                { "range": range, "newText": "fixed" }
                            ] } }
                        }}),
                        json!({ "jsonrpc": "2.0", "id": "choose", "method": "window/showMessageRequest", "params": {
                            "type": 2, "message": "choose", "actions": [{ "title": "Yes" }]
                        }}),
                        json!({ "jsonrpc": "2.0", "method": "window/showMessage", "params": { "type": 1, "message": "shown" } }),
                        json!({ "jsonrpc": "2.0", "method": "window/logMessage", "params": { "type": 4, "message": "logged" } }),
                        json!({ "jsonrpc": "2.0", "method": "$/progress", "params": { "token": "work", "value": {
                            "kind": "begin", "title": "Indexing", "message": "half", "percentage": 50
                        } }}),
                        json!({ "jsonrpc": "2.0", "method": "$/progress", "params": { "token": "work", "value": { "kind": "end" } } }),
                    ] {
                        write_message(&mut writer, &message).unwrap();
                    }
                }
                "textDocument/hover" => write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0", "id": id, "error": { "code": -32803, "message": "hover failed" }
                    }),
                )
                .unwrap(),
                "textDocument/references" | "textDocument/implementation" | "textDocument/typeDefinition" => {
                    reply(&mut writer, json!([{ "uri": "file:///tmp/x/B.ts", "range": range }]));
                }
                "textDocument/rename" => reply(
                    &mut writer,
                    json!({ "changes": { "file:///tmp/x/B.ts": [
                    { "range": range, "newText": "renamed" }
                ] } }),
                ),
                "textDocument/codeAction" => reply(&mut writer, json!([{ "title": "Fix", "kind": "quickfix" }])),
                "textDocument/completion" => {
                    reply(&mut writer, json!({ "isIncomplete": true, "items": [{ "label": "alpha" }] }));
                }
                "textDocument/documentSymbol" => reply(
                    &mut writer,
                    json!([{
                        "name": "Foo", "kind": 5, "range": range, "selectionRange": range
                    }]),
                ),
                "textDocument/formatting" => reply(&mut writer, json!([{ "range": range, "newText": "formatted" }])),
                "textDocument/semanticTokens/full" => reply(&mut writer, json!({ "data": [0, 0, 1, 0, 0] })),
                "workspace/executeCommand" => reply(&mut writer, json!({ "ok": true })),
                "shutdown" => reply(&mut writer, Value::Null),
                "exit" => break,
                _ => {}
            }
        }
    })
}

fn collect(mgr: &mut LspManager, until: impl Fn(&[LspEvent]) -> bool) -> Vec<LspEvent> {
    let mut all = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !until(&all) && Instant::now() < deadline {
        all.extend(mgr.poll_timeout(Duration::from_millis(100)));
    }
    all
}

fn collect_seen(rx: &mpsc::Receiver<Value>, until: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    let mut all = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !until(&all) && Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => all.push(message),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    all
}

fn response(events: &[LspEvent], id: RequestId) -> &LspResult {
    events
        .iter()
        .find_map(|event| match event {
            LspEvent::Response { id: actual, result, .. } if *actual == id => Some(result),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing response for {id:?}: {events:?}"))
}

fn empty_config(dir: &Path) -> LspConfig {
    LspConfig {
        path_dirs: vec![],
        extra_bin_dirs: vec![],
        cache_dir: dir.join("cache"),
        jdk_search: crate::java::JdkSearch { homebrew_opt: vec![], jvm_dirs: vec![] },
        spring_boot: false,
        java_on_demand: false,
    }
}

#[test]
fn fake_server_lifecycle_incremental_sync_hover_definition_70() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let (client_read, server_write) = std::io::pipe().unwrap();
    let (server_read, client_write) = std::io::pipe().unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let handle = fake_server(server_read, server_write, seen_tx, 2);

    let mut mgr = LspManager::new(empty_config(&root));
    let key = mgr.connect_streams("vtsls", &root, client_read, client_write, json!({ "fake": { "level": 7 } }));
    let file = root.join("a.ts");
    let text = "let a = 1;\n";
    mgr.open(&file, Language::TypeScript, text);
    let change = ContentChange::from_edit(text, 8, 9, "2");
    mgr.change(&file, &[change], || "let a = 2;\n".into());
    let hover = mgr.hover(&file, Position { line: 0, character: 4 }).unwrap();

    let events = collect(&mut mgr, |ev| ev.iter().any(|e| matches!(e, LspEvent::Response { .. })));
    assert!(events.contains(&LspEvent::ServerStarted { key: key.clone() }));
    let hover_ev = events.iter().find_map(|e| match e {
        LspEvent::Response { id, result: LspResult::Hover { raw, .. }, .. } if *id == hover => Some(raw.clone()),
        _ => None,
    });
    let tip = crate::tooltip::assemble(&[], hover_ev.as_slice());
    assert_eq!(tip.doc.as_deref(), Some("**doc**"));

    let def = mgr.definition(&file, Position { line: 0, character: 4 }).unwrap();
    let events = collect(&mut mgr, |ev| ev.iter().any(|e| matches!(e, LspEvent::Response { id, .. } if *id == def)));
    let locs = events.iter().find_map(|e| match e {
        LspEvent::Response { result: LspResult::Locations(l), .. } => Some(l.clone()),
        _ => None,
    });
    let locs = locs.unwrap();
    assert_eq!(locs[0].path, PathBuf::from("/tmp/x/B.ts"));
    assert_eq!(locs[0].range.start, Position { line: 2, character: 4 });
    assert!(mgr.diagnostics().get(&file).iter().any(|d| d.message == "v1"), "didOpen queued until initialized");

    let seen: Vec<Value> = seen_rx.try_iter().collect();
    let methods: Vec<&str> = seen.iter().filter_map(|m| m["method"].as_str()).collect();
    let pos = |m: &str| methods.iter().position(|x| *x == m).unwrap_or(usize::MAX);
    assert!(pos("initialize") < pos("initialized") && pos("initialized") < pos("textDocument/didOpen"));
    let cfg_reply = seen.iter().find(|m| m["id"] == "cfg").expect("configuration answered by reader thread");
    assert_eq!(cfg_reply["result"], json!([7]));
    let did_open = seen.iter().find(|m| m["method"] == "textDocument/didOpen").unwrap();
    assert_eq!(did_open["params"]["textDocument"]["text"], "let a = 2;\n", "queued didOpen carries latest text");

    mgr.change(&file, &[ContentChange::from_edit("let a = 2;\n", 0, 3, "const")], String::new);
    collect(&mut mgr, |ev| ev.iter().any(|e| matches!(e, LspEvent::Diagnostics { .. })));
    let seen: Vec<Value> = seen_rx.try_iter().collect();
    let change = seen.iter().find(|m| m["method"] == "textDocument/didChange").unwrap();
    assert_eq!(change["params"]["contentChanges"][0]["range"]["end"], json!({ "line": 0, "character": 3 }));
    assert_eq!(change["params"]["textDocument"]["version"], 2);
    assert_eq!(mgr.diagnostics().counts(&file).errors, 1);

    mgr.shutdown_all(Duration::from_secs(2));
    handle.join().unwrap();
}

#[test]
fn full_sync_server_gets_whole_text() {
    let tmp = tempfile::tempdir().unwrap();
    let (client_read, server_write) = std::io::pipe().unwrap();
    let (server_read, client_write) = std::io::pipe().unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let handle = fake_server(server_read, server_write, seen_tx, 1);
    let mut mgr = LspManager::new(empty_config(tmp.path()));
    mgr.connect_streams("vtsls", tmp.path(), client_read, client_write, json!({}));
    let file = tmp.path().join("b.ts");
    mgr.open(&file, Language::TypeScript, "x");
    collect(&mut mgr, |ev| ev.iter().any(|e| matches!(e, LspEvent::Diagnostics { .. })));
    mgr.change(&file, &[ContentChange::from_edit("x", 1, 1, "y")], || "xy".into());
    collect(&mut mgr, |ev| ev.iter().any(|e| matches!(e, LspEvent::Diagnostics { .. })));
    let change = seen_rx.try_iter().find(|m| m["method"] == "textDocument/didChange").unwrap();
    assert_eq!(change["params"]["contentChanges"], json!([{ "text": "xy" }]));
    assert!(mgr.document_symbols(&file).is_none(), "unsupported capability is not requested");
    drop(mgr);
    handle.join().unwrap();
}

#[test]
fn request_response_types_server_events_and_poll_flow_70() {
    let tmp = tempfile::tempdir().unwrap();
    let (client_read, server_write) = std::io::pipe().unwrap();
    let (server_read, client_write) = std::io::pipe().unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let handle = full_server(server_read, server_write, seen_tx);

    let mut mgr = LspManager::new(empty_config(tmp.path()));
    let key = mgr.connect_streams("vtsls", tmp.path(), client_read, client_write, json!({}));
    let file = tmp.path().join("a.ts");
    mgr.open(&file, Language::TypeScript, "const a = 1;\n");
    let pos = Position { line: 0, character: 6 };
    let hover = mgr.hover(&file, pos).unwrap();
    let references = mgr.references(&file, pos, true).unwrap();
    let implementation = mgr.implementation(&file, pos).unwrap();
    let type_definition = mgr.type_definition(&file, pos).unwrap();
    let rename = mgr.rename(&file, pos, "renamed").unwrap();
    let code_actions = mgr.code_actions(&file, Range::default(), &[]).unwrap();
    let completion = mgr.completion(&file, pos).unwrap();
    let symbols = mgr.document_symbols(&file).unwrap();
    let formatting = mgr.formatting(&file, 2, true).unwrap();
    let semantic_tokens = mgr.semantic_tokens(&file).unwrap();
    let execute = mgr.execute_command(&file, "fix.apply", json!([1])).unwrap();
    let ids = [
        hover,
        references,
        implementation,
        type_definition,
        rename,
        code_actions,
        completion,
        symbols,
        formatting,
        semantic_tokens,
        execute,
    ];
    let events = collect(&mut mgr, |events| {
        ids.iter()
            .all(|id| events.iter().any(|event| matches!(event, LspEvent::Response { id: actual, .. } if actual == id)))
    });

    assert!(events.contains(&LspEvent::ServerStarted { key: key.clone() }));
    assert!(
        matches!(response(&events, hover), LspResult::Error { code: -32803, message } if message == "hover failed")
    );
    for id in [references, implementation, type_definition] {
        let LspResult::Locations(locations) = response(&events, id) else { panic!("wrong location response") };
        assert_eq!(locations[0].path, PathBuf::from("/tmp/x/B.ts"));
    }
    let LspResult::Rename(Some(edit)) = response(&events, rename) else { panic!("wrong rename response") };
    let files = workspace_edit_files(edit);
    assert_eq!(files[0].0, PathBuf::from("/tmp/x/B.ts"));
    assert_eq!(files[0].1[0].new_text, "renamed");
    let LspResult::CodeActions(actions) = response(&events, code_actions) else { panic!("wrong code action response") };
    assert_eq!(actions.len(), 1);
    let LspResult::Completion { items, incomplete } = response(&events, completion) else {
        panic!("wrong completion response")
    };
    assert_eq!(items[0].label, "alpha");
    assert!(incomplete);
    assert!(matches!(response(&events, symbols), LspResult::Symbols(Some(_))));
    let LspResult::Formatting(edits) = response(&events, formatting) else { panic!("wrong formatting response") };
    assert_eq!(edits[0].new_text, "formatted");
    let LspResult::SemanticTokens(Some(tokens)) = response(&events, semantic_tokens) else {
        panic!("wrong semantic token response")
    };
    assert_eq!(tokens.data.len(), 1);
    assert_eq!(response(&events, execute), &LspResult::Raw(json!({ "ok": true })));
    assert_eq!(mgr.semantic_legend(&key).unwrap()["tokenTypes"], json!(["type"]));

    let apply = events.iter().find_map(|event| match event {
        LspEvent::ApplyEdit { label, edit: Some(edit), .. } => Some((label.clone(), workspace_edit_files(edit))),
        _ => None,
    });
    let (label, files) = apply.expect("applyEdit event");
    assert_eq!(label.as_deref(), Some("Apply fix"));
    assert_eq!(files[0].1[0].new_text, "fixed");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LspEvent::ShowMessage { level: 2, message, .. } if message == "choose"))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LspEvent::ShowMessage { level: 1, message, .. } if message == "shown"))
    );
    assert!(events.iter().any(|event| matches!(event, LspEvent::LogMessage { message, .. } if message == "logged")));
    assert!(events.iter().any(|event| matches!(event, LspEvent::Progress { title: Some(title), percentage: Some(50), done: false, .. } if title == "Indexing")));
    assert!(events.iter().any(|event| matches!(event, LspEvent::Progress { done: true, .. })));

    mgr.save(&file, Some("const a = 2;\n"));
    mgr.close(&file);
    assert_eq!(mgr.document_language(&file), None);
    let seen =
        collect_seen(&seen_rx, |messages| messages.iter().any(|message| message["method"] == "textDocument/didClose"));
    let save = seen.iter().find(|message| message["method"] == "textDocument/didSave").unwrap();
    assert_eq!(save["params"]["text"], "const a = 2;\n");
    let reference = seen.iter().find(|message| message["method"] == "textDocument/references").unwrap();
    assert_eq!(reference["params"]["context"]["includeDeclaration"], true);
    let rename = seen.iter().find(|message| message["method"] == "textDocument/rename").unwrap();
    assert_eq!(rename["params"]["newName"], "renamed");
    assert!(seen.iter().any(|message| message["id"] == "apply" && message["result"]["applied"] == true));
    assert!(seen.iter().any(|message| message["id"] == "choose" && message["result"].is_null()));

    mgr.shutdown_all(Duration::from_secs(2));
    handle.join().unwrap();
}

#[test]
fn missing_server_reports_once() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mgr = LspManager::new(empty_config(tmp.path()));
    mgr.open(&tmp.path().join("A.java"), Language::Java, "class A {}");
    mgr.open(&tmp.path().join("B.java"), Language::Java, "class B {}");
    mgr.open(&tmp.path().join("q.sql"), Language::Sql, "select 1");
    let events = mgr.poll();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(matches!(&events[0], LspEvent::ServerNotFound { server: "jdtls", hint, .. } if hint.contains("jdtls")));
    assert!(mgr.hover(&tmp.path().join("A.java"), Position::default()).is_none());
}

#[test]
fn spawned_process_exit_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("marksman");
    std::fs::write(&script, "#!/bin/sh\necho boom >&2\nexit 3\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = empty_config(tmp.path());
    config.path_dirs = vec![bin];
    let mut mgr = LspManager::new(config);
    let file = tmp.path().join("README.md");
    mgr.open(&file, Language::Markdown, "# x");
    let events = collect(&mut mgr, |ev| ev.iter().any(|e| matches!(e, LspEvent::ServerExited { .. })));
    let exited = events.iter().find_map(|e| match e {
        LspEvent::ServerExited { code, stderr, .. } => Some((*code, stderr.clone())),
        _ => None,
    });
    let (code, _stderr) = exited.expect("exit event");
    assert_eq!(code, Some(3));
    assert!(mgr.server_of(&file).is_none());
}

#[test]
fn lsp_position_counts_utf16() {
    let text = "ab\nç😀x\n";
    assert_eq!(lsp_position(text, 3), Position { line: 1, character: 0 });
    assert_eq!(lsp_position(text, 9), Position { line: 1, character: 3 });
    assert_eq!(lsp_position(text, text.len()), Position { line: 2, character: 0 });
}

#[test]
fn workspace_edit_normalizes_changes_and_document_changes() {
    let edit: WorkspaceEdit = serde_json::from_value(json!({
        "changes": { "file:///a.ts": [{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } }, "newText": "b" }] },
        "documentChanges": [{ "textDocument": { "uri": "file:///c.ts", "version": 1 },
            "edits": [{ "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 0 } }, "newText": "x" }] }]
    }))
    .unwrap();
    let files = workspace_edit_files(&edit);
    let paths: Vec<_> = files.iter().map(|(p, _)| p.to_string_lossy().into_owned()).collect();
    assert_eq!(paths, ["/a.ts", "/c.ts"]);
}

/// Gerçek vtsls (PATH ya da Mason) ile uçtan uca: tür hatası diagnostik olarak gelir. Kurulu sunucuya bağlı
/// olduğu için `#[ignore]`; `cargo test -p less-lang real_vtsls -- --ignored`. `LESS_REAL_TS_FILE` verilirse o
/// dosya (gerçek proje kökü, monorepo) açılır; hata yalnız `didOpen` metnine eklenir, disk değişmez.
#[test]
#[ignore]
fn real_vtsls_reports_type_error() {
    let tmp = tempfile::tempdir().unwrap();
    let file = match std::env::var_os("LESS_REAL_TS_FILE") {
        Some(f) => PathBuf::from(f),
        None => {
            let root = tmp.path().canonicalize().unwrap();
            std::fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
            std::fs::write(root.join("tsconfig.json"), r#"{ "compilerOptions": { "strict": true } }"#).unwrap();
            let file = root.join("src/a.ts");
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "export {};\n").unwrap();
            file
        }
    };
    let error = "not assignable";
    real_server_reports(&file, "\nconst lessProbe: number = \"text\";\n", error, Duration::from_secs(90));
}

/// Gerçek jdtls ile: `LESS_REAL_JAVA_FILE` (Maven/Gradle projesindeki bir `.java`) açılır, sonuna tür hatası
/// eklenir (yalnız `didOpen` metni). jdtls ilk açılışta 30–120 sn sürer.
#[test]
#[ignore]
fn real_jdtls_reports_type_error() {
    let Some(file) = std::env::var_os("LESS_REAL_JAVA_FILE").map(PathBuf::from) else {
        panic!("set LESS_REAL_JAVA_FILE to a .java file in a Maven/Gradle project");
    };
    let probe = "\nclass LessProbe { int x = \"text\"; }\n";
    real_server_reports(&file, probe, "cannot convert from String to int", Duration::from_secs(240));
}

/// Gerçek ESLint sunucusuyla: `LESS_REAL_ESLINT_FILE` (eslint yapılandırması + typescript-eslint olan projede bir
/// `.ts`/`.tsx`) açılır, sonuna `any` eklenir; `no-explicit-any` uyarısı vtsls'inkilerle aynı depoya gelir.
#[test]
#[ignore]
fn real_eslint_reports_warning() {
    let Some(file) = std::env::var_os("LESS_REAL_ESLINT_FILE").map(PathBuf::from) else {
        panic!("set LESS_REAL_ESLINT_FILE to a .ts file in a project with an eslint config");
    };
    real_server_reports(&file, "\nexport const lessProbe: any = 1;\n", "Unexpected any", Duration::from_secs(90));
}

/// Teşhis: `LESS_REAL_JAVA_FILE` gerçek jdtls (+ kuruluysa Spring Boot) ile açılır, ilk import bitip diagnostikler 20 sn
/// değişmeyene kadar beklenir ve yazdırılır. jdtls çalışma klasörü `LESS_REAL_CACHE` (açık LESS'in kilidine dokunmaz).
#[test]
#[ignore]
fn real_jdtls_dump_diagnostics() {
    let file = PathBuf::from(std::env::var_os("LESS_REAL_JAVA_FILE").expect("set LESS_REAL_JAVA_FILE"));
    let mut config = LspConfig::default();
    if let Some(cache) = std::env::var_os("LESS_REAL_CACHE") {
        config.cache_dir = PathBuf::from(cache);
    }
    let lang = Language::from_path(&file).unwrap();
    let mut mgr = LspManager::new(config);
    let extra = std::env::var("LESS_REAL_APPEND").unwrap_or_default();
    mgr.open(&file, lang, &(std::fs::read_to_string(&file).unwrap() + &extra));
    let (deadline, mut last_change, mut last) = (Instant::now() + Duration::from_secs(600), Instant::now(), 0);
    while Instant::now() < deadline {
        for e in mgr.poll_timeout(Duration::from_millis(500)) {
            match e {
                LspEvent::Progress { key, message: Some(m), .. } => eprintln!("[{}] {m}", key.server),
                LspEvent::ServerExited { key, stderr, .. } => eprintln!("[{}] exited {stderr:?}", key.server),
                LspEvent::ServerStarted { key } => eprintln!("[{}] started", key.server),
                _ => {}
            }
        }
        let n = mgr.diagnostics().get(&file).len();
        if n != last {
            (last, last_change) = (n, Instant::now());
        }
        // Ready'den sonra derleme ve bağımlılık güncellemesi sürer; diagnostikler 90 sn değişmeyince dur.
        if last > 0 && last_change.elapsed() > Duration::from_secs(90) {
            break;
        }
    }
    for d in mgr.diagnostics().get(&file) {
        eprintln!("DIAG {:?} {}:{} {:?} {}", d.severity, d.start.line + 1, d.start.character, d.source, d.message);
    }
    if let Some((line, col)) = std::env::var("LESS_REAL_GD")
        .ok()
        .and_then(|v| v.split_once(':').map(|(l, c)| (l.parse::<u32>().unwrap() - 1, c.parse().unwrap())))
    {
        let id = mgr.definition(&file, Position { line, character: col }).unwrap();
        let events =
            collect(&mut mgr, |e| e.iter().any(|ev| matches!(ev, LspEvent::Response { id: a, .. } if *a == id)));
        eprintln!("GD {:?}", response(&events, id));
    }
    let args = json!([uri::path_to_uri(&file), "{\"scope\":\"test\"}"]);
    if let Some(id) = mgr.execute_command(&file, "java.project.getClasspaths", args) {
        let events =
            collect(&mut mgr, |e| e.iter().any(|ev| matches!(ev, LspEvent::Response { id: a, .. } if *a == id)));
        match response(&events, id) {
            LspResult::Raw(v) => v["classpaths"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .for_each(|c| eprintln!("CP {c}")),
            other => eprintln!("CP error {other:?}"),
        }
    }
    mgr.shutdown_all(Duration::from_secs(2));
}

/// Gerçek Spring Boot uçtan uca: `LESS_REAL_JAVA_FILE` (jdtls'i başlatır) ve `LESS_REAL_CONFIG_FILE`
/// (`application.properties` / `.yml`, sonuna `LESS_REAL_PROBE` eklenir, ör. `server.po`) açılır. Spring classpath'i
/// jdtls'ten (aktarma) alıp `LESS_REAL_WANT` (ör. `server.port`) önerene kadar tamamlama istenir; ilk satırda hover.
#[test]
#[ignore]
fn real_spring_boot_config_completion_and_hover() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let (java, config) = (PathBuf::from(var("LESS_REAL_JAVA_FILE")), PathBuf::from(var("LESS_REAL_CONFIG_FILE")));
    let (probe, want) = (var("LESS_REAL_PROBE"), var("LESS_REAL_WANT"));
    let mut lsp_config = LspConfig::default();
    if let Some(cache) = std::env::var_os("LESS_REAL_CACHE") {
        lsp_config.cache_dir = PathBuf::from(cache);
    }
    let mut mgr = LspManager::new(lsp_config);
    mgr.open(&java, Language::Java, &std::fs::read_to_string(&java).unwrap());
    let text = std::fs::read_to_string(&config).unwrap().trim_end().to_string() + "\n" + &probe;
    let lang = Language::from_path(&config).unwrap();
    mgr.open(&config, lang, &text);
    eprintln!("config server: {:?}", mgr.server_of(&config));
    let end = Position { line: text.lines().count() as u32 - 1, character: probe.len() as u32 };
    let deadline = Instant::now() + Duration::from_secs(600);
    let (mut pending, mut found, mut next) = (None, false, Instant::now());
    while !found && Instant::now() < deadline {
        if pending.is_none() && Instant::now() >= next {
            pending = mgr.complete(&config, end, None).first().copied();
            next = Instant::now() + Duration::from_secs(10);
        }
        for e in mgr.poll_timeout(Duration::from_millis(300)) {
            match e {
                LspEvent::Response { id, result: LspResult::Completion { items, .. }, .. } if Some(id) == pending => {
                    pending = None;
                    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).take(8).collect();
                    eprintln!("completion ({} items): {labels:?}", items.len());
                    found = items.iter().any(|i| i.label == want);
                }
                LspEvent::Response { id, .. } if Some(id) == pending => pending = None,
                LspEvent::ServerExited { key, stderr, .. } => eprintln!("[{}] exited {stderr:?}", key.server),
                LspEvent::ShowMessage { key, message, .. } => eprintln!("[{}] {message}", key.server),
                _ => {}
            }
        }
    }
    let hover = mgr.hover(&config, Position { line: 0, character: 3 });
    let events =
        collect(&mut mgr, |e| e.iter().any(|ev| matches!(ev, LspEvent::Response { id, .. } if Some(*id) == hover)));
    if let Some(LspResult::Hover { raw, .. }) = hover.map(|h| response(&events, h)) {
        eprintln!("HOVER {:?}", crate::tooltip::assemble(&[], std::slice::from_ref(raw)).doc);
    }
    for d in mgr.diagnostics().get(&config) {
        eprintln!("DIAG {}:{} {:?} {}", d.start.line + 1, d.start.character, d.source, d.message);
    }
    mgr.shutdown_all(Duration::from_secs(2));
    assert!(found, "{want} never offered");
}

/// `file`'ı `probe` eklenmiş metinle açar, `error` içeren bir diagnostik gelene kadar bekler.
fn real_server_reports(file: &Path, probe: &str, error: &str, timeout: Duration) {
    let file = file.to_path_buf();
    let lang = Language::from_path(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap() + probe;
    let mut mgr = LspManager::new(LspConfig::default());
    mgr.open(&file, lang, &text);
    let mut all = Vec::new();
    let deadline = Instant::now() + timeout;
    let found = |m: &LspManager| m.diagnostics().get(&file).iter().any(|d| d.message.contains(error));
    while !found(&mgr) && Instant::now() < deadline {
        all.extend(mgr.poll_timeout(Duration::from_millis(200)));
        if all.iter().any(|e| matches!(e, LspEvent::ServerNotFound { .. } | LspEvent::ServerExited { .. })) {
            break;
        }
    }
    let diags: Vec<Diagnostic> = mgr.diagnostics().get(&file).into_iter().cloned().collect();
    eprintln!("root {:?}", mgr.server_of(&file));
    for d in &diags {
        eprintln!("{:?} {}:{} {:?} {}", d.severity, d.start.line, d.start.character, d.source, d.message);
    }
    mgr.shutdown_all(Duration::from_secs(2));
    assert!(diags.iter().any(|d| d.message.contains(error)), "diags {diags:?}\nevents {all:#?}");
}

/// Pull diagnostik (`diagnosticProvider`, ESLint): açılışta ve her değişiklikte `textDocument/diagnostic` istenir,
/// `full` yanıtı depoya girer.
#[test]
fn pull_diagnostics_after_open_and_change() {
    let tmp = tempfile::tempdir().unwrap();
    let (client_read, server_write) = std::io::pipe().unwrap();
    let (server_read, client_write) = std::io::pipe().unwrap();
    thread::spawn(move || {
        let mut reader = BufReader::new(server_read);
        let mut writer = server_write;
        let mut n = 0;
        while let Ok(Some(msg)) = read_message(&mut reader) {
            let id = msg.get("id").cloned();
            let result = match msg["method"].as_str() {
                Some("initialize") => json!({ "capabilities": { "textDocumentSync": 2, "diagnosticProvider": {} } }),
                Some("textDocument/diagnostic") => {
                    n += 1;
                    let d = json!({ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
                        "severity": 2, "message": format!("pull {n}") });
                    json!({ "kind": "full", "items": [d] })
                }
                _ => continue,
            };
            write_message(&mut writer, &json!({ "jsonrpc": "2.0", "id": id.unwrap(), "result": result })).unwrap();
        }
    });
    let mut mgr = LspManager::new(empty_config(tmp.path()));
    mgr.connect_streams("vtsls", tmp.path(), client_read, client_write, json!({}));
    let file = tmp.path().join("a.ts");
    mgr.open(&file, Language::TypeScript, "x\n");
    let has = |m: &LspManager, msg: &str| m.diagnostics().get(&file).iter().any(|d| d.message == msg);
    let _ = collect(&mut mgr, |ev| ev.iter().any(|e| matches!(e, LspEvent::Diagnostics { .. })));
    assert!(has(&mgr, "pull 1"), "{:?}", mgr.diagnostics().get(&file));
    mgr.change(&file, &[ContentChange { range: None, text: "y\n".into() }], || "y\n".into());
    let _ = collect(&mut mgr, |ev| ev.iter().any(|e| matches!(e, LspEvent::Diagnostics { .. })));
    assert!(has(&mgr, "pull 2"));
    assert_eq!(mgr.diagnostics().counts(&file).warnings, 1);
}

/// Yetenekleri ve yanıtları verilen küçük sahte sunucu (yöntem → sonuç); `$/progress` için `progress` yöntemi
/// gelirse bir `begin` yollar.
fn scripted_server(
    reader: std::io::PipeReader,
    mut writer: std::io::PipeWriter,
    capabilities: Value,
    replies: Vec<(&'static str, Value)>,
    seen: mpsc::Sender<Value>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        while let Ok(Some(msg)) = read_message(&mut reader) {
            let method = msg["method"].as_str().unwrap_or("").to_string();
            let _ = seen.send(msg.clone());
            let result = match method.as_str() {
                "initialize" => Some(json!({ "capabilities": capabilities })),
                "initialized" => {
                    let begin = json!({ "jsonrpc": "2.0", "method": "$/progress", "params": { "token": 7, "value": {
                        "kind": "begin", "title": "Indexing" } } });
                    write_message(&mut writer, &begin).unwrap();
                    None
                }
                "exit" => break,
                m => replies.iter().find(|(k, _)| *k == m).map(|(_, v)| v.clone()),
            };
            if let (Some(result), Some(id)) = (result, msg.get("id")) {
                write_message(&mut writer, &json!({ "jsonrpc": "2.0", "id": id, "result": result })).unwrap();
            }
        }
    })
}

#[test]
fn completion_goes_to_extra_servers_resolve_and_signature_help_and_states() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::write(root.join("tailwind.config.js"), "").unwrap();
    std::fs::write(root.join("package-lock.json"), "").unwrap();
    let mut mgr = LspManager::new(empty_config(&root));
    let (seen_tx, seen_rx) = mpsc::channel();

    let (c1r, s1w) = std::io::pipe().unwrap();
    let (s1r, c1w) = std::io::pipe().unwrap();
    let main_caps = json!({
        "textDocumentSync": 2,
        "completionProvider": { "triggerCharacters": ["."], "resolveProvider": true },
        "signatureHelpProvider": { "triggerCharacters": ["(", ","] }
    });
    let main_replies = vec![
        ("textDocument/completion", json!({ "isIncomplete": false, "items": [{ "label": "alpha", "data": 1 }] })),
        ("completionItem/resolve", json!({ "label": "alpha", "documentation": "Alpha docs" })),
        ("textDocument/signatureHelp", json!({ "signatures": [{ "label": "f(a, b)" }], "activeParameter": 1 })),
    ];
    let h1 = scripted_server(s1r, s1w, main_caps, main_replies, seen_tx.clone());
    let main = mgr.connect_streams("vtsls", &root, c1r, c1w, json!({}));
    assert_eq!(mgr.server_states(), vec![(main.clone(), ServerState::Starting)]);

    let (c2r, s2w) = std::io::pipe().unwrap();
    let (s2r, c2w) = std::io::pipe().unwrap();
    let tw_caps = json!({ "textDocumentSync": 1, "completionProvider": { "triggerCharacters": ["-", " "] } });
    let tw_replies = vec![("textDocument/completion", json!([{ "label": "bg-red-500", "kind": 21 }]))];
    let h2 = scripted_server(s2r, s2w, tw_caps, tw_replies, seen_tx);
    let tw = mgr.connect_streams("tailwindcss", &root, c2r, c2w, json!({}));

    let file = root.join("a.tsx");
    mgr.open(&file, Language::Tsx, "<div className=\"bg\" />\n");
    let started = collect(&mut mgr, |e| e.iter().filter(|e| matches!(e, LspEvent::ServerStarted { .. })).count() == 2);
    assert_eq!(started.iter().filter(|e| matches!(e, LspEvent::ServerStarted { .. })).count(), 2);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !mgr.server_states().iter().all(|(_, s)| *s == ServerState::Busy) && Instant::now() < deadline {
        mgr.poll_timeout(Duration::from_millis(50));
    }
    assert!(mgr.server_states().iter().all(|(_, s)| *s == ServerState::Busy), "{:?}", mgr.server_states());

    let mut triggers = mgr.completion_triggers(&file);
    triggers.sort();
    assert_eq!(triggers, [" ", "-", "."]);
    assert_eq!(mgr.signature_triggers(&file), ["(", ","]);

    let pos = Position { line: 0, character: 17 };
    let ids = mgr.complete(&file, pos, None);
    assert_eq!(ids.len(), 2, "main + tailwind");
    assert_eq!(mgr.complete(&file, pos, Some(".")).len(), 1, "tailwind does not trigger on '.'");
    let events = collect(&mut mgr, |e| {
        ids.iter().all(|id| e.iter().any(|ev| matches!(ev, LspEvent::Response { id: a, .. } if a == id)))
    });
    let LspResult::Completion { items, .. } = response(&events, ids[1]) else { panic!() };
    assert_eq!(items[0].label, "bg-red-500");
    let from = events.iter().find_map(|e| match e {
        LspEvent::Response { id, key, .. } if *id == ids[0] => Some(key.clone()),
        _ => None,
    });
    assert_eq!(from.as_ref(), Some(&main));

    let resolve = mgr.resolve_completion(&main, json!({ "label": "alpha", "data": 1 })).unwrap();
    assert!(mgr.resolve_completion(&tw, json!({ "label": "x" })).is_none(), "no resolveProvider");
    let sig = mgr.signature_help(&file, pos, Some("(")).unwrap();
    let events = collect(&mut mgr, |e| {
        [resolve, sig].iter().all(|id| e.iter().any(|ev| matches!(ev, LspEvent::Response { id: a, .. } if a == id)))
    });
    assert!(matches!(response(&events, resolve), LspResult::Raw(v) if v["documentation"] == "Alpha docs"));
    assert!(matches!(response(&events, sig), LspResult::Raw(v) if v["activeParameter"] == 1));
    let seen = collect_seen(&seen_rx, |m| m.iter().any(|m| m["method"] == "textDocument/signatureHelp"));
    let ctx = seen.iter().find(|m| m["method"] == "textDocument/completion").unwrap();
    assert_eq!(ctx["params"]["context"]["triggerKind"], 1);

    let (dead_r, dead_w) = std::io::pipe().unwrap();
    let (_keep, sink) = std::io::pipe().unwrap();
    let dead = mgr.connect_streams("gopls", &root, dead_r, sink, json!({}));
    drop(dead_w);
    collect(&mut mgr, |e| e.iter().any(|e| matches!(e, LspEvent::ServerExited { .. })));
    assert_eq!(mgr.server_states().last(), Some(&(dead, ServerState::Exited)));

    mgr.shutdown_all(Duration::from_millis(300));
    h1.join().unwrap();
    h2.join().unwrap();
}

/// Gerçek sunucuyla tamamlama + resolve + imza yardımı: `dir` içinde `name` dosyası `text` ile açılır, `at`
/// konumunda tamamlama istenir, `want` etiketi beklenir.
fn real_completion(
    lang: Language,
    dir: &Path,
    name: &str,
    text: &str,
    at: Position,
    want: &str,
    sig_at: Option<Position>,
) {
    let mut mgr = LspManager::new(LspConfig::default());
    let file = dir.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, text).unwrap();
    mgr.open(&file, lang, text);
    let started = collect(&mut mgr, |e| {
        e.iter().any(|e| matches!(e, LspEvent::ServerStarted { .. } | LspEvent::ServerNotFound { .. }))
    });
    assert!(started.iter().any(|e| matches!(e, LspEvent::ServerStarted { .. })), "{started:?}");
    let t0 = Instant::now();
    let mut found = None;
    while found.is_none() && t0.elapsed() < Duration::from_secs(60) {
        let ids = mgr.complete(&file, at, None);
        let events = collect(&mut mgr, |e| {
            ids.iter().all(|id| e.iter().any(|ev| matches!(ev, LspEvent::Response { id: a, .. } if a == id)))
        });
        for e in events {
            if let LspEvent::Response { key, result: LspResult::Completion { items, .. }, .. } = e
                && let Some(item) = items.into_iter().find(|i| i.label == want)
            {
                found = Some((key, item));
            }
        }
    }
    let (key, item) = found.unwrap_or_else(|| panic!("{want} not completed"));
    eprintln!("{}: completion '{want}' after {:?}", key.server, t0.elapsed());
    if let Some(id) = mgr.resolve_completion(&key, serde_json::to_value(&item).unwrap()) {
        let events =
            collect(&mut mgr, |e| e.iter().any(|ev| matches!(ev, LspEvent::Response { id: a, .. } if *a == id)));
        let LspResult::Raw(v) = response(&events, id) else { panic!("resolve") };
        eprintln!(
            "resolved: detail={:?} doc={}",
            v["detail"],
            v["documentation"].to_string().chars().take(80).collect::<String>()
        );
    }
    if let Some(sig_at) = sig_at {
        let id = mgr.signature_help(&file, sig_at, Some("(")).expect("signatureHelpProvider");
        let events =
            collect(&mut mgr, |e| e.iter().any(|ev| matches!(ev, LspEvent::Response { id: a, .. } if *a == id)));
        let LspResult::Raw(v) = response(&events, id) else { panic!("signature") };
        let label = v["signatures"][0]["label"].as_str().unwrap_or_default().to_string();
        eprintln!("signature: {label}");
        assert!(!label.is_empty(), "{v}");
    }
    mgr.shutdown_all(Duration::from_secs(2));
}

/// `cargo test -p less-lang real_completion -- --ignored --nocapture` (vtsls ve pyright/basedpyright kurulu olmalı).
#[test]
#[ignore]
fn real_completion_vtsls_and_python() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::write(root.join("package-lock.json"), "").unwrap();
    let ts = "function alphaBeta(x: number, y: string) { return x; }\nalph\nalphaBeta(\n";
    real_completion(
        Language::TypeScript,
        &root,
        "src/a.ts",
        ts,
        Position { line: 1, character: 4 },
        "alphaBeta",
        Some(Position { line: 2, character: 10 }),
    );
    std::fs::write(root.join("pyproject.toml"), "").unwrap();
    let py = "import os\nos.pat\nprint(\n";
    real_completion(
        Language::Python,
        &root,
        "main.py",
        py,
        Position { line: 1, character: 6 },
        "path",
        Some(Position { line: 2, character: 6 }),
    );
}

/// [`scripted_server`] gibi; ayrıca `initialized` gelince `after_init` mesajlarını (sunucu istekleri) yollar.
fn chatty_server(
    reader: std::io::PipeReader,
    mut writer: std::io::PipeWriter,
    capabilities: Value,
    replies: Vec<(&'static str, Value)>,
    after_init: Vec<Value>,
    seen: mpsc::Sender<Value>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        while let Ok(Some(msg)) = read_message(&mut reader) {
            let method = msg["method"].as_str().unwrap_or("").to_string();
            let _ = seen.send(msg.clone());
            let result = match method.as_str() {
                "initialize" => Some(json!({ "capabilities": capabilities })),
                "initialized" => {
                    after_init.iter().for_each(|m| write_message(&mut writer, m).unwrap());
                    None
                }
                "exit" => break,
                m => replies.iter().find(|(k, _)| *k == m).map(|(_, v)| v.clone()),
            };
            if let (Some(result), Some(id)) = (result, msg.get("id").filter(|_| !method.is_empty())) {
                write_message(&mut writer, &json!({ "jsonrpc": "2.0", "id": id, "result": result })).unwrap();
            }
        }
    })
}

#[test]
fn spring_boot_relays_java_data_and_classpath_and_merges_hover() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::write(root.join("pom.xml"), "<artifactId>spring-boot-starter-parent</artifactId>").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    let mut mgr = LspManager::new(LspConfig { spring_boot: true, ..empty_config(&root) });
    let caps = json!({ "textDocumentSync": 1, "hoverProvider": true });
    let request = |id: &str, method: &str, params: Value| json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });

    let (jdt_tx, jdt_seen) = mpsc::channel();
    let (c1r, s1w) = std::io::pipe().unwrap();
    let (s1r, c1w) = std::io::pipe().unwrap();
    let h1 = chatty_server(
        s1r,
        s1w,
        caps.clone(),
        vec![
            ("textDocument/hover", json!({ "contents": { "kind": "markdown", "value": "jdt" } })),
            ("workspace/executeCommand", json!({ "name": "Foo" })),
        ],
        vec![
            request(
                "ls",
                "workspace/executeClientCommand",
                json!({ "command": "vscode-spring-boot.ls.start", "arguments": [] }),
            ),
            request(
                "cp",
                "workspace/executeClientCommand",
                json!({ "command": "sts4.classpath.abcdefgh", "arguments": ["file:///p", "p", false] }),
            ),
        ],
        jdt_tx,
    );
    mgr.connect_streams("jdtls", &root, c1r, c1w, json!({}));

    let (spring_tx, spring_seen) = mpsc::channel();
    let (c2r, s2w) = std::io::pipe().unwrap();
    let (s2r, c2w) = std::io::pipe().unwrap();
    let h2 = chatty_server(
        s2r,
        s2w,
        caps,
        vec![
            ("textDocument/hover", json!({ "contents": { "kind": "markdown", "value": "bean" } })),
            ("workspace/executeCommand", json!("done")),
        ],
        vec![request("j1", "sts/javaType", json!({ "bindingKey": "LFoo;" }))],
        spring_tx,
    );
    mgr.connect_streams("spring_boot", &root, c2r, c2w, json!({}));

    let file = root.join("src/A.java");
    mgr.open(&file, Language::Java, "class A {}\n");
    collect(&mut mgr, |e| e.iter().filter(|e| matches!(e, LspEvent::ServerStarted { .. })).count() == 2);

    let (mut to_jdt, mut to_spring) = (Vec::new(), Vec::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    let done = |jdt: &[Value], spring: &[Value]| {
        jdt.iter().any(|m| m["id"] == "cp" && m.get("result").is_some())
            && spring.iter().any(|m| m["id"] == "j1" && m.get("result").is_some())
    };
    while !done(&to_jdt, &to_spring) && Instant::now() < deadline {
        mgr.poll_timeout(Duration::from_millis(20));
        to_jdt.extend(jdt_seen.try_iter());
        to_spring.extend(spring_seen.try_iter());
    }
    let java_type = to_jdt.iter().find(|m| m["params"]["command"] == "sts.java.type").expect("sts/javaType → jdtls");
    assert_eq!(java_type["params"]["arguments"], json!([{ "bindingKey": "LFoo;" }]));
    assert_eq!(to_jdt.iter().find(|m| m["id"] == "ls").unwrap()["result"], Value::Null, "ls.start answered");
    assert_eq!(
        to_jdt.iter().find(|m| m["id"] == "cp").unwrap()["result"],
        "done",
        "classpath callback answered by Spring"
    );
    let all = to_spring;
    assert_eq!(all.iter().find(|m| m["id"] == "j1").unwrap()["result"], json!({ "name": "Foo" }));
    let commands: Vec<&Value> =
        all.iter().filter(|m| m["method"] == "workspace/executeCommand").map(|m| &m["params"]).collect();
    assert!(
        commands
            .iter()
            .any(|p| p["command"] == "sts.vscode-spring-boot.enableClasspathListening"
                && p["arguments"] == json!([true]))
    );
    assert!(commands.iter().any(|p| p["command"] == "sts4.classpath.abcdefgh" && p["arguments"][1] == "p"));
    assert!(all.iter().any(|m| m["method"] == "textDocument/didOpen"), "Spring gets Java documents");

    let hover = mgr.hover(&file, Position { line: 0, character: 6 }).unwrap();
    let events = collect(&mut mgr, |e| e.iter().any(|ev| matches!(ev, LspEvent::Response { id, .. } if *id == hover)));
    let LspResult::Hover { raw, .. } = response(&events, hover) else { panic!() };
    let tip = crate::tooltip::assemble(&[], std::slice::from_ref(raw));
    assert_eq!(tip.doc.as_deref(), Some("jdt\n\nbean"), "one merged hover from both servers");
    assert_eq!(events.iter().filter(|e| matches!(e, LspEvent::Response { id, .. } if *id == hover)).count(), 1);

    mgr.shutdown_all(Duration::from_millis(300));
    h1.join().unwrap();
    h2.join().unwrap();
}
