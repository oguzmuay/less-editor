use std::collections::BTreeMap;
use std::io::{BufReader, Cursor, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use less_run::dap::protocol::{encode, read_message};
use less_run::dap::types::SourceBreakpoint;
use less_run::dap::{DebugEvent, DebugManager, Lang, StepCommand};
use less_run::variables::{SetRequest, VariablesModel};
use serde_json::{Value, json};

type Log = Arc<Mutex<Vec<String>>>;

/// Sahte js-debug: kök bağlantı attach'ten sonra `startDebugging` ister, çocuk bağlantı breakpoint'te durur.
fn fake_adapter() -> (SocketAddr, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let log: Log = Arc::default();
    let shared = log.clone();
    thread::spawn(move || {
        for (n, conn) in listener.incoming().enumerate() {
            let Ok(conn) = conn else { return };
            let log = shared.clone();
            thread::spawn(move || serve(n, conn, log));
        }
    });
    (addr, log)
}

fn serve(conn_no: usize, stream: TcpStream, log: Log) {
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    let mut seq = 1000;
    let mut send = |w: &mut TcpStream, mut msg: Value| {
        seq += 1;
        msg["seq"] = json!(seq);
        w.write_all(&encode(&msg)).unwrap();
    };
    let (mut attach_seq, mut child) = (0, false);
    while let Ok(Some(msg)) = read_message(&mut r) {
        let command = msg["command"].as_str().unwrap_or_default().to_string();
        let args = msg["arguments"].clone();
        if msg["type"] == "response" {
            log.lock().unwrap().push(format!("{conn_no}:resp:{command}"));
            continue;
        }
        let entry = match command.as_str() {
            "setBreakpoints" => format!("{conn_no}:setBreakpoints:{}:{}", args["source"]["path"], args["breakpoints"]),
            _ => format!("{conn_no}:{command}"),
        };
        log.lock().unwrap().push(entry);
        let ok = |body: Value| json!({ "type": "response", "request_seq": msg["seq"], "command": command, "success": true, "body": body });
        let reply = match command.as_str() {
            "initialize" => ok(json!({ "supportsConfigurationDoneRequest": true, "supportsSetVariable": true })),
            "attach" => {
                attach_seq = msg["seq"].as_i64().unwrap();
                child = args["__child"] == true;
                send(&mut w, json!({ "type": "event", "event": "initialized" }));
                continue;
            }
            "setBreakpoints" => {
                let bps: Vec<Value> = args["breakpoints"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| json!({ "verified": true, "line": b["line"] }))
                    .collect();
                ok(json!({ "breakpoints": bps }))
            }
            "configurationDone" => {
                send(&mut w, ok(Value::Null));
                send(
                    &mut w,
                    json!({ "type": "response", "request_seq": attach_seq, "command": "attach", "success": true }),
                );
                if child {
                    send(
                        &mut w,
                        json!({ "type": "event", "event": "stopped", "body": { "reason": "breakpoint", "threadId": 1 } }),
                    );
                } else {
                    let cfg = json!({ "type": "pwa-node", "name": "app [worker]", "__child": true });
                    send(
                        &mut w,
                        json!({ "type": "request", "command": "startDebugging", "arguments": { "request": "attach", "configuration": cfg } }),
                    );
                }
                continue;
            }
            "stackTrace" => ok(json!({ "stackFrames": [
                { "id": 7, "name": "main", "source": { "path": "/x/app.js" }, "line": 3, "column": 1 }
            ] })),
            "scopes" => ok(json!({ "scopes": [
                { "name": "Local", "variablesReference": 10, "expensive": false },
                { "name": "Global", "variablesReference": 20, "expensive": true }
            ] })),
            "variables" => match args["variablesReference"].as_i64() {
                Some(10) => ok(json!({ "variables": [
                    { "name": "count", "value": "1", "type": "number", "variablesReference": 0, "evaluateName": "count" },
                    { "name": "list", "value": "Array(2)", "type": "object", "variablesReference": 11, "evaluateName": "list" }
                ] })),
                Some(11) => ok(json!({ "variables": [
                    { "name": "0", "value": "'a'", "type": "string", "evaluateName": "list[0]" },
                    { "name": "1", "value": "'b\nc'", "type": "string", "evaluateName": "list[1]" }
                ] })),
                _ => ok(json!({ "variables": [] })),
            },
            "setVariable" => ok(json!({ "value": args["value"], "type": "number" })),
            "evaluate" => ok(json!({ "result": "3", "variablesReference": 0 })),
            "continue" => {
                send(&mut w, ok(json!({ "allThreadsContinued": true })));
                send(&mut w, json!({ "type": "event", "event": "continued", "body": { "threadId": 1 } }));
                continue;
            }
            "disconnect" => {
                send(&mut w, ok(Value::Null));
                return;
            }
            _ => {
                json!({ "type": "response", "request_seq": msg["seq"], "command": command, "success": false, "message": "unsupported" })
            }
        };
        send(&mut w, reply);
    }
}

fn poll_until(mgr: &mut DebugManager, done: impl Fn(&[DebugEvent]) -> bool) -> Vec<DebugEvent> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        events.extend(mgr.poll());
        if done(&events) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    events
}

#[test]
fn framing_roundtrip_and_errors_34() {
    let mut bytes = encode(&json!({ "seq": 1, "type": "event", "event": "ü" }));
    bytes.extend(b"content-length: 2\r\nContent-Type: x\r\n\r\n{}");
    let mut r = Cursor::new(bytes);
    assert_eq!(read_message(&mut r).unwrap().unwrap()["event"], "ü");
    assert_eq!(read_message(&mut r).unwrap().unwrap(), json!({}));
    assert!(read_message(&mut r).unwrap().is_none(), "clean end of stream");
    assert!(read_message(&mut Cursor::new(b"X: 1\r\n\r\n{}".to_vec())).is_err(), "missing Content-Length");
    assert!(read_message(&mut Cursor::new(b"Content-Length: 9\r\n\r\n{}".to_vec())).is_err(), "truncated body");
}

#[test]
fn fake_adapter_child_session_stop_variables_continue_detach_91_97() {
    let (addr, log) = fake_adapter();
    let mut mgr = DebugManager::new();
    let bp = SourceBreakpoint { line: 3, condition: None };
    mgr.replace_breakpoints(BTreeMap::from([(PathBuf::from("/x/app.js"), vec![bp])]));
    let rid = "node:/x:dev";
    mgr.attach_addr(rid, addr, "pwa-node", json!({ "type": "pwa-node", "request": "attach", "name": "app" }));
    let events = poll_until(&mut mgr, |ev| ev.iter().any(|e| matches!(e, DebugEvent::Stopped { .. })));

    let started: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            DebugEvent::SessionStarted { session, parent, name, .. } => Some((*session, *parent, name.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(started.len(), 2, "{events:?}");
    let (root, child) = (started[0].0, started[1].0);
    assert_eq!(started[0].1, None);
    assert_eq!(started[1], (child, Some(root), "app [worker]".to_string()));
    assert!(events.iter().any(|e| matches!(e, DebugEvent::State { state: None, .. })));
    assert_eq!(mgr.root_session_count(rid), 1);
    assert_eq!(mgr.sessions_for(rid).len(), 2);
    assert!(mgr.attached_or_attaching(rid));

    let s = mgr.stopped_session().expect("child is stopped");
    assert_eq!(s.id, child);
    assert_eq!(s.lang(), Some(Lang::Js));
    assert_eq!(s.frame_id(), Some(7));
    let (client, caps, frame) = (s.client.clone(), s.capabilities.clone(), s.frame_id());

    let mut vars = VariablesModel::new();
    vars.refresh(&client, frame).unwrap();
    let names: Vec<_> = vars.rows().iter().map(|r| r.name_cell.clone()).collect();
    assert_eq!(names, ["▾ Local", "    count", "  ▸ list", "▸ Global"], "expensive scope starts collapsed");
    vars.toggle("Local\0list");
    vars.refresh(&client, frame).unwrap();
    let rows = vars.rows();
    assert_eq!(rows[3].name_cell, "      [0]");
    assert_eq!(rows[4].value, "'b↵c'");

    let req = vars.set_request("Local\0count", "5", &caps, 7).unwrap();
    assert_eq!(req, SetRequest::SetVariable { parent_ref: 10, name: "count".into(), value: "5".into() });
    assert_eq!(vars.apply_set(&client, "Local\0count", &req).unwrap(), "5");
    vars.refresh(&client, frame).unwrap();
    assert_eq!(vars.node("Local\0count").unwrap().value.as_deref(), Some("5"), "override survives a cached list");
    vars.on_stopped();
    vars.refresh(&client, frame).unwrap();
    let count = vars.rows().into_iter().find(|r| r.path == "Local\0count").unwrap();
    assert_eq!((count.value.as_str(), count.changed), ("1", true), "new stop drops overrides, marks change");
    assert_eq!(client.evaluate("list.length", frame, "repl").unwrap().value, "3");

    mgr.set_breakpoints(&PathBuf::from("/x/b.js"), vec![SourceBreakpoint { line: 9, condition: Some("x>1".into()) }]);
    mgr.step(child, StepCommand::Continue);
    poll_until(&mut mgr, |ev| ev.iter().any(|e| matches!(e, DebugEvent::Continued { session } if *session == child)));
    assert!(mgr.stopped_session().is_none());

    mgr.detach(rid);
    let ended =
        poll_until(&mut mgr, |ev| ev.iter().filter(|e| matches!(e, DebugEvent::SessionEnded { .. })).count() == 2);
    assert_eq!(ended.iter().filter(|e| matches!(e, DebugEvent::SessionEnded { .. })).count(), 2, "{ended:?}");
    assert!(!mgr.attached_or_attaching(rid));

    let log = log.lock().unwrap().clone();
    for conn in ["0", "1"] {
        let pos = |c: &str| log.iter().position(|l| l.starts_with(&format!("{conn}:{c}"))).unwrap_or(usize::MAX);
        assert!(pos("initialize") < pos("attach"), "{log:?}");
        assert!(pos("setBreakpoints") < pos("configurationDone"), "{log:?}");
    }
    assert!(log.contains(&r#"0:setBreakpoints:"/x/app.js":[{"line":3}]"#.to_string()), "{log:?}");
    assert!(log.contains(&"0:resp:startDebugging".to_string()));
    assert!(log.iter().any(|l| l.contains(r#"setBreakpoints:"/x/b.js":[{"condition":"x>1","line":9}]"#)), "{log:?}");
    assert!(log.contains(&"1:setVariable".to_string()));
}

#[test]
fn attach_to_closed_port_fails_with_message_34() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let mut mgr = DebugManager::new();
    mgr.attach_addr("x", port, "pwa-node", json!({ "request": "attach" }));
    let events = poll_until(&mut mgr, |ev| ev.iter().any(|e| matches!(e, DebugEvent::Failed { .. })));
    let msg = events.iter().find_map(|e| match e {
        DebugEvent::Failed { message, .. } => Some(message.clone()),
        _ => None,
    });
    assert!(msg.unwrap().starts_with("Could not connect to the debug adapter"));
    assert!(!mgr.attached_or_attaching("x"));
}
