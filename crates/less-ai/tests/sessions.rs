use std::io::Write;
use std::path::Path;

use less_ai::sessions::{SessionReader, Status, escape_cwd, fmt_tokens, one_line, process_info, transcript_path};

fn write(path: &Path, s: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, s).unwrap();
}

fn append(path: &Path, s: &str) {
    std::fs::OpenOptions::new().append(true).open(path).unwrap().write_all(s.as_bytes()).unwrap();
}

const CWD: &str = "/Users/me/my_proj.x";

fn setup(dir: &Path, extra: &str) -> std::path::PathBuf {
    write(
        &dir.join("sessions/42.json"),
        &format!(r#"{{"pid":42,"sessionId":"abc","cwd":"{CWD}","status":"busy","waitingFor":null{extra}}}"#),
    );
    dir.join("projects").join(escape_cwd(CWD)).join("abc.jsonl")
}

fn assistant(id: &str, input: u64, output: u64, content: &str) -> String {
    format!(
        r#"{{"parentUuid":null,"type":"assistant","message":{{"id":"{id}","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_read_input_tokens":100,"cache_creation_input_tokens":10}},"content":[{content}]}}}}"#
    ) + "\n"
}

#[test]
fn escape_and_process_info_27() {
    assert_eq!(escape_cwd(CWD), "-Users-me-my-proj-x");
    let tmp = tempfile::tempdir().unwrap();
    setup(tmp.path(), r#","name":"n","nameSource":"user","unknown":[1]"#);
    let info = process_info(42, tmp.path()).unwrap();
    assert_eq!(info.status, Some(Status::Busy));
    assert_eq!(info.session_id.as_deref(), Some("abc"));
    assert_eq!(info.name_source.as_deref(), Some("user"));
    assert!(process_info(7, tmp.path()).is_none());
}

#[test]
fn token_dedup_by_message_id_27() {
    let tmp = tempfile::tempdir().unwrap();
    let path = setup(tmp.path(), "");
    let text = r#"{"type":"text","text":"Hello\nworld"}"#;
    let tool = r#"{"type":"tool_use","name":"Read","input":{"file_path":"/a/b/Main.java"}}"#;
    let body = assistant("m1", 5, 7, text) + &assistant("m1", 5, 7, tool) + &assistant("m2", 1, 2, "");
    write(&path, &body);
    let mut r = SessionReader::new();
    let s = r.snapshot(42, tmp.path()).unwrap();
    assert_eq!(s.usage.input, 6);
    assert_eq!(s.usage.output, 9);
    assert_eq!(s.usage.cache_read, 200);
    assert_eq!(s.usage.cache_create, 20);
    assert_eq!(s.total, 6 + 9 + 200 + 20);
    assert_eq!(s.context, 1 + 100 + 10);
    assert_eq!(s.last_text.as_deref(), Some("Hello"));
    assert_eq!(s.last_tool.as_deref(), Some("Read: Main.java"));
}

#[test]
fn incremental_append_and_partial_lines_27() {
    let tmp = tempfile::tempdir().unwrap();
    let path = setup(tmp.path(), "");
    let line = assistant("m1", 10, 0, "");
    let (a, b) = line.split_at(30);
    write(&path, a);
    let mut r = SessionReader::new();
    assert_eq!(r.snapshot(42, tmp.path()).unwrap().usage.input, 0);
    append(&path, b);
    assert_eq!(r.snapshot(42, tmp.path()).unwrap().usage.input, 10);
    append(&path, &assistant("m1", 12, 0, ""));
    assert_eq!(r.snapshot(42, tmp.path()).unwrap().usage.input, 12, "same id replaces old usage");
    append(
        &path,
        "{\"type\":\"user\",\"message\":{\"content\":\"  fix the   bug\\nplease\"}}\n\
         {\"type\":\"user\",\"message\":{\"content\":\"<command>x</command>\"}}\n\
         {\"type\":\"user\",\"message\":{\"content\":[{\"tool_use_id\":\"t\",\"type\":\"tool_result\"}]}}\n",
    );
    let s = r.snapshot(42, tmp.path()).unwrap();
    assert_eq!(s.last_prompt.as_deref(), Some("fix the bug"));
    let st = r.read(&path);
    assert_eq!(st.offset, std::fs::metadata(&path).unwrap().len());
    std::fs::write(&path, assistant("z", 1, 0, "")).unwrap();
    assert_eq!(r.snapshot(42, tmp.path()).unwrap().usage.input, 1, "shrunk file is re-read");
}

#[test]
fn title_priority_custom_user_ai_27() {
    let tmp = tempfile::tempdir().unwrap();
    let path = setup(tmp.path(), r#","name":"User name","nameSource":"user""#);
    write(&path, "{\"type\":\"ai-title\",\"aiTitle\":\"AI title\"}\n");
    let mut r = SessionReader::new();
    assert_eq!(r.snapshot(42, tmp.path()).unwrap().title.as_deref(), Some("User name"));
    append(&path, "{\"type\":\"custom-title\",\"customTitle\":\"Renamed\"}\n");
    assert_eq!(r.snapshot(42, tmp.path()).unwrap().title.as_deref(), Some("Renamed"));

    let tmp2 = tempfile::tempdir().unwrap();
    let path2 = setup(tmp2.path(), r#","name":"auto","nameSource":"ai""#);
    write(&path2, "{\"type\":\"ai-title\",\"aiTitle\":\"AI title\"}\n");
    assert_eq!(SessionReader::new().snapshot(42, tmp2.path()).unwrap().title.as_deref(), Some("AI title"));
}

#[test]
fn transcript_glob_fallback_and_subagents_27() {
    let tmp = tempfile::tempdir().unwrap();
    setup(tmp.path(), "");
    let other = tmp.path().join("projects/-elsewhere/abc.jsonl");
    write(&other, &assistant("m", 3, 0, ""));
    write(&tmp.path().join("projects/-elsewhere/abc/subagents/s1.jsonl"), &assistant("s", 1, 1, ""));
    let info = process_info(42, tmp.path()).unwrap();
    assert_eq!(transcript_path(&info, tmp.path()), Some(other));
    let s = SessionReader::new().snapshot(42, tmp.path()).unwrap();
    assert_eq!(s.subagent_count, 1);
    assert_eq!(s.subagents, 1 + 1 + 100 + 10);
}

#[test]
fn formatting_helpers_60() {
    assert_eq!(fmt_tokens(950), "950");
    assert_eq!(fmt_tokens(12_345), "12.3K");
    assert_eq!(fmt_tokens(2_345_678), "2.35M");
    assert_eq!(one_line("\n  a   b \nc", 120).as_deref(), Some("a b"));
    assert_eq!(one_line("abcdef", 4).as_deref(), Some("abc…"));
    assert_eq!(one_line("  \n ", 4), None);
}
