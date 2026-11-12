use std::collections::HashSet;
use std::net::TcpListener;

use less_run::output::*;
use less_run::ports::{free_port_with, parse_lsof_ports};

#[test]
fn ansi_is_stripped_16() {
    assert_eq!(plain("\x1b[32m  INFO\x1b[0m done\r\x07 "), "INFO done");
    assert_eq!(plain("\x1b]0;title\x07hello"), "hello");
    assert_eq!(plain("\x1b]8;;http://x\x1b\\link"), "link");
    assert_eq!(strip_ansi("\x1b[1;31m  x \x1b[m"), "  x ");
}

#[test]
fn line_splitter_joins_chunks_16() {
    let mut s = LineSplitter::default();
    assert!(s.push(b"hel").is_empty());
    assert_eq!(s.push(b"lo\r\nwor"), ["hello"]);
    assert_eq!(s.partial(), "wor");
    assert_eq!(s.push(b"ld\n\n"), ["world", ""]);
    assert_eq!(s.take_partial(), None);
    let long = vec![b'x'; 5000];
    assert_eq!(s.push(&long).len(), 1, "very long unfinished line is flushed");
}

#[test]
fn ready_port_and_debug_targets_34() {
    assert!(spring_ready("Started ApiApplication in 3.456 seconds (process running for 4.1)"));
    assert!(!spring_ready("Starting ApiApplication"));
    assert_eq!(jdwp_port("Listening for transport dt_socket at address: 5005"), Some(5005));
    assert_eq!(inspector_port("Debugger listening on ws://127.0.0.1:53211/0f1e-ab"), Some(53211));
    assert_eq!(app_port("Tomcat started on port 8080 (http) with context path '/'"), Some("8080".into()));
    assert_eq!(app_port("Tomcat started on ports 8081 (http)"), Some("8081".into()));
    assert_eq!(app_port("  ➜  Local:   http://localhost:5173/"), Some("5173".into()));
    assert_eq!(app_port("Listening on 127.0.0.1:4000"), Some("4000".into()));
    assert_eq!(app_port("Debugger listening on ws://127.0.0.1:9229/x"), None);
}

#[test]
fn confirm_prompt_detection_77() {
    assert!(is_confirm("? Would you like to run the app on another port instead? › (Y/n)"));
    assert!(is_confirm("Continue? [y/N]"));
    assert!(is_confirm("Proceed (yes/no)"));
    assert!(!is_confirm("(Y/n) is the default"));
    let mut t = PromptTracker::default();
    t.on_line("\x1b[33mSomething is already running on port 3000.\x1b[0m");
    let q = t.check("\x1b[36m?\x1b[39m Would you like to run the app on another port instead? › (Y/n)").unwrap();
    assert_eq!(q, "Something is already running on port 3000. Would you like to run the app on another port instead?");
    assert!(t.pending());
    assert_eq!(t.check("? Would you like to run the app on another port instead? › (Y/n)"), None, "asked once");
    t.answered();
    assert!(!t.pending());
    assert_eq!(t.check("? Would you like to run the app on another port instead? › (Y/n)"), None);
    t.on_line("Starting…");
    assert!(t.check("Again? [y/N]").is_some(), "a new line resets");
    assert_eq!(confirm_question("Go? (Y/n)", Some("Go? (Y/n)")), "Go?");
}

#[test]
fn free_port_skips_busy_and_taken_34() {
    let busy_listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let base = busy_listener.local_addr().unwrap().port();
    let taken: HashSet<u16> = [base.saturating_add(1)].into();
    let port = free_port_with(base, &HashSet::new(), &taken).unwrap();
    assert!(port != base && port != base + 1, "bound and taken ports are skipped (got {port})");
    let busy: HashSet<u16> = [port].into();
    let next = free_port_with(base, &busy, &taken).unwrap();
    assert_ne!(next, port);
}

#[test]
fn lsof_output_parsing_34() {
    let out = "p123\nf10\nn127.0.0.1:5005\nf11\nn*:8080\nn[::1]:9229\n";
    assert_eq!(parse_lsof_ports(out), [5005, 8080, 9229].into());
}
