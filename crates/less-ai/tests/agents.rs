use std::collections::BTreeMap;
use std::path::Path;

use less_ai::accounts::Accounts;
use less_ai::agents::{AgentKind, AiState, LaunchRequest, McpLaunch, gemini_settings, launch, next_name};
use less_ai::mailbox::{Mailbox, NudgeLimiter, Recipient, prepare};

fn req<'a>(kind: AgentKind, cwd: &'a Path) -> LaunchRequest<'a> {
    LaunchRequest { kind, cwd, used_names: vec![], model: None, mcp: None, claude_cmd: None, extra_env: vec![] }
}

fn mcp() -> McpLaunch {
    McpLaunch {
        command: "/bin/less-rs".into(),
        args: vec!["mcp".into()],
        env: BTreeMap::from([("LESS_SOCKET".to_string(), "/tmp/s".to_string())]),
    }
}

#[test]
fn argv_builders_per_agent_64_65() {
    let cwd = Path::new("/p");
    let c = launch(&req(AgentKind::Claude, cwd));
    assert_eq!(c.name, "claude-1");
    assert_eq!(c.argv[0], "claude");
    assert_eq!(c.argv[1], "--append-system-prompt");
    assert!(c.argv[2].starts_with("You are claude-1,"));
    assert!(c.argv[2].contains("/p/.less/ai/inbox/claude-1.md"));
    assert_eq!(c.env["LESS_AI_DIR"], "/p/.less/ai");
    assert_eq!(c.env["LESS_AI_NAME"], "claude-1");

    let x = launch(&LaunchRequest { used_names: vec!["codex-1", "codex-3"], ..req(AgentKind::Codex, cwd) });
    assert_eq!(x.name, "codex-2");
    assert_eq!(x.argv[..2], ["codex".to_string(), "-c".to_string()]);
    assert!(x.argv[2].starts_with("developer_instructions=\"You are codex-2,"));

    let g = launch(&req(AgentKind::Gemini, cwd));
    assert_eq!(g.argv[1], "-i");
    assert!(g.argv[2].ends_with("\n\nReply only with: ready."));

    let o = launch(&LaunchRequest { model: Some("qwen"), ..req(AgentKind::Ollama, cwd) });
    assert_eq!(o.argv, ["ollama", "run", "qwen"]);
    assert_eq!(next_name(AgentKind::Ollama, ["ollama-1"]), "ollama-2");
}

#[test]
fn mcp_attach_args_67() {
    let cwd = Path::new("/p");
    let m = mcp();
    let c = launch(&LaunchRequest { mcp: Some(&m), ..req(AgentKind::Claude, cwd) });
    let n = c.argv.len();
    assert_eq!(c.argv[n - 2], "--mcp-config");
    let v: serde_json::Value = serde_json::from_str(&c.argv[n - 1]).unwrap();
    assert_eq!(v["mcpServers"]["less"]["type"], "stdio");
    assert_eq!(v["mcpServers"]["less"]["command"], "/bin/less-rs");
    assert_eq!(v["mcpServers"]["less"]["env"]["LESS_SOCKET"], "/tmp/s");
    assert!(c.text.contains("Your MCP server `less`"));

    let x = launch(&LaunchRequest { mcp: Some(&m), ..req(AgentKind::Codex, cwd) });
    let joined = x.argv.join(" ");
    assert!(joined.contains("-c mcp_servers.less.command=\"/bin/less-rs\""));
    assert!(joined.contains("-c mcp_servers.less.args=[\"mcp\"]"));
    assert!(joined.contains("-c mcp_servers.less.env={ LESS_SOCKET = \"/tmp/s\" }"));
    assert!(joined.ends_with("-c mcp_servers.less.tool_timeout_sec=900"));

    let o = launch(&LaunchRequest { mcp: Some(&m), ..req(AgentKind::Ollama, cwd) });
    assert_eq!(o.argv.len(), 3);
}

#[test]
fn gemini_settings_merge_67() {
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("settings.json");
    std::fs::write(&f, r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#).unwrap();
    assert!(gemini_settings(&f, &mcp()).unwrap());
    assert!(!gemini_settings(&f, &mcp()).unwrap(), "second call is a no-op");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
    assert_eq!(v["theme"], "dark");
    assert_eq!(v["mcpServers"]["other"]["command"], "x");
    assert_eq!(v["mcpServers"]["less"]["env"]["LESS_AI_NAME"], "$LESS_AI_NAME");
    assert_eq!(v["mcpServers"]["less"]["env"]["LESS_SOCKET"], "$LESS_SOCKET");
    std::fs::write(&f, "not json").unwrap();
    assert!(gemini_settings(&f, &mcp()).is_err());
}

#[test]
fn ai_state_default_and_ollama_model_64() {
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("ai.json");
    let mut st = AiState::load_from(&f, None);
    assert_eq!(st.default_kind(), AgentKind::Claude);
    assert_eq!(st.ollama_model(), "llama3.2");
    st.choose(AgentKind::Ollama, Some(" qwen ")).unwrap();
    let st = AiState::load_from(&f, None);
    assert_eq!(st.default_kind(), AgentKind::Ollama);
    assert_eq!(st.ollama_model(), "qwen");
    let legacy = tmp.path().join("legacy.json");
    std::fs::write(&legacy, r#"{"default":"codex"}"#).unwrap();
    assert_eq!(AiState::load_from(&tmp.path().join("none.json"), Some(&legacy)).default_kind(), AgentKind::Codex);
}

#[test]
fn mailbox_prepare_readme_and_gitignore_65() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(".less/ai");
    prepare(&dir, &[("claude-1".into(), AgentKind::Claude), ("codex-1".into(), AgentKind::Codex)]).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join(".gitignore")).unwrap().trim(), "*");
    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(readme.contains("- `claude-1` (Claude)"));
    assert!(readme.contains("- `codex-1` (Codex)"));
    assert!(dir.join("inbox").is_dir() && dir.join("shared").is_dir());
}

#[test]
fn nudge_rate_limit_6_per_10_min_65() {
    let mut l = NudgeLimiter::default();
    for i in 0..6 {
        assert!(l.allow("a", 1000 + i));
    }
    assert!(!l.allow("a", 1100));
    assert!(l.allow("b", 1100), "limit is per AI");
    assert!(!l.allow("a", 1599));
    assert!(l.allow("a", 1600), "oldest hit left the window");
}

#[test]
fn mailbox_delivery_idle_only_and_ollama_text_65() {
    let tmp = tempfile::tempdir().unwrap();
    let inbox = tmp.path().join(".less/ai/inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::write(inbox.join("ollama-1.md"), "## from claude-1\nhello\n  there\n").unwrap();
    let mut mb = Mailbox::new();
    mb.mark_pending("claude-1");
    mb.mark_pending("ollama-1");
    let r = |name: &str, kind, idle| Recipient { name: name.into(), kind, cwd: tmp.path().into(), idle };
    let busy = [r("claude-1", AgentKind::Claude, false)];
    assert!(mb.deliver(&busy, 0).is_empty());
    assert!(mb.has_mail("claude-1"));
    let out = mb.deliver(&[r("claude-1", AgentKind::Claude, true), r("ollama-1", AgentKind::Ollama, true)], 0);
    assert_eq!(out.len(), 2);
    assert!(out[0].text.starts_with("You have new mail in "));
    assert!(out[0].text.ends_with("inbox/claude-1.md — read the new messages at the end and act on them."));
    assert_eq!(out[1].text, "## from claude-1 hello there");
    assert!(!mb.has_mail("claude-1"));
    std::fs::OpenOptions::new()
        .append(true)
        .open(inbox.join("ollama-1.md"))
        .and_then(|mut f| std::io::Write::write_all(&mut f, b"second\n"))
        .unwrap();
    mb.mark_pending("ollama-1");
    let out = mb.deliver(&[r("ollama-1", AgentKind::Ollama, true)], 1);
    assert_eq!(out[0].text, "second", "only the new part goes to Ollama");
}

#[test]
fn mailbox_watch_marks_pending_65() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(".less/ai");
    prepare(&dir, &[]).unwrap();
    let mut mb = Mailbox::new();
    mb.watch(&dir).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    std::fs::write(dir.join("inbox/codex-1.md"), "hi").unwrap();
    let start = std::time::Instant::now();
    while !mb.has_mail("codex-1") && start.elapsed() < std::time::Duration::from_secs(5) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(mb.has_mail("codex-1"));
}

#[test]
fn accounts_detect_select_add_and_env_63() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude-work")).unwrap();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    let file = home.path().join("data/claude_accounts.json");
    let mut acc = Accounts::load_from(&file, None, home.path());
    let names: Vec<&str> = acc.list().iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["claude", "claude-work"]);
    assert!(acc.env(&acc.current()).is_empty());
    acc.select("claude-work").unwrap();
    let work = acc.current();
    assert_eq!(acc.env(&work), [("CLAUDE_CONFIG_DIR".to_string(), work.dir.to_string_lossy().to_string())]);
    assert!(acc.add("claude-work", None).is_err());
    acc.add("other", None).unwrap();
    let again = Accounts::load_from(&file, None, home.path());
    assert_eq!(again.current().name, "other");
    assert_eq!(again.current().dir, home.path().join(".other"));
    assert!(again.is_multi());

    let locks = home.path().join(".claude/ide");
    std::fs::create_dir_all(&locks).unwrap();
    again.link_ide(&work, &locks, 1234).unwrap();
    let link = work.dir.join("ide/1234.lock");
    assert_eq!(std::fs::read_link(link).unwrap(), locks.join("1234.lock"));
}
