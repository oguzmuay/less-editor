//! LSP taban protokolü: `Content-Length` başlıklı JSON-RPC çerçeveleri.

use std::io::{self, BufRead, Write};

use serde_json::Value;

/// Bir mesajı çerçeveleyip yazar ve boşaltır.
pub fn write_message(w: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

/// Bir çerçeve okur. Akış temiz biterse `Ok(None)`. Bilinmeyen başlıklar (`Content-Type`) yok sayılır.
pub fn read_message(r: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return if length.is_none() { Ok(None) } else { Err(io::ErrorKind::UnexpectedEof.into()) };
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = Some(value.trim().parse().map_err(|_| invalid("bad Content-Length"))?);
        }
    }
    let mut body = vec![0; length.unwrap_or(0)];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(|e| invalid(&e.to_string()))
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Gelen mesajın türü.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Request { id: Value, method: String, params: Value },
    Notification { method: String, params: Value },
    Response { id: Value, result: Result<Value, ResponseError> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
}

impl Message {
    pub fn classify(value: Value) -> Option<Message> {
        let Value::Object(mut obj) = value else { return None };
        let method = obj.remove("method").and_then(|m| m.as_str().map(str::to_string));
        let id = obj.remove("id");
        let params = obj.remove("params").unwrap_or(Value::Null);
        match (method, id) {
            (Some(method), Some(id)) => Some(Message::Request { id, method, params }),
            (Some(method), None) => Some(Message::Notification { method, params }),
            (None, Some(id)) => {
                let result = match obj.remove("error") {
                    Some(err) => Err(ResponseError {
                        code: err.get("code").and_then(Value::as_i64).unwrap_or(0),
                        message: err.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
                    }),
                    None => Ok(obj.remove("result").unwrap_or(Value::Null)),
                };
                Some(Message::Response { id, result })
            }
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn frames_roundtrip_with_extra_headers() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({"jsonrpc": "2.0", "id": 1, "method": "x", "params": {"ü": "ş"}})).unwrap();
        let text = String::from_utf8(buf.clone()).unwrap();
        let body_len = text.split("\r\n\r\n").nth(1).unwrap().len();
        assert!(text.starts_with(&format!("Content-Length: {body_len}\r\n\r\n")));
        buf.extend_from_slice(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 13\r\n\r\n");
        buf.extend_from_slice(br#"{"result":42}"#);
        let mut r = Cursor::new(buf);
        let first = read_message(&mut r).unwrap().unwrap();
        assert_eq!(first["params"]["ü"], "ş");
        let second = read_message(&mut r).unwrap().unwrap();
        assert_eq!(second, json!({"result": 42}));
        assert!(read_message(&mut r).unwrap().is_none());
    }

    #[test]
    fn truncated_body_is_error() {
        let mut r = Cursor::new(b"Content-Length: 50\r\n\r\n{}".to_vec());
        assert!(read_message(&mut r).is_err());
    }

    #[test]
    fn invalid_frames_are_rejected() {
        let mut bad_length = Cursor::new(b"Content-Length: nope\r\n\r\n{}".to_vec());
        assert_eq!(read_message(&mut bad_length).unwrap_err().kind(), io::ErrorKind::InvalidData);
        let mut bad_json = Cursor::new(b"Content-Length: 1\r\n\r\n{".to_vec());
        assert_eq!(read_message(&mut bad_json).unwrap_err().kind(), io::ErrorKind::InvalidData);
        let mut partial_header = Cursor::new(b"Content-Length: 2\r\n".to_vec());
        assert_eq!(read_message(&mut partial_header).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn classifies_messages() {
        let m = Message::classify(json!({"id": 3, "error": {"code": -32601, "message": "nope"}})).unwrap();
        assert!(matches!(m, Message::Response { result: Err(ResponseError { code: -32601, .. }), .. }));
        let n = Message::classify(json!({"method": "window/logMessage", "params": {}})).unwrap();
        assert!(matches!(n, Message::Notification { .. }));
        let q = Message::classify(json!({"id": "a", "method": "workspace/configuration", "params": { "items": [] }}))
            .unwrap();
        assert!(
            matches!(q, Message::Request { id, method, params } if id == json!("a") && method == "workspace/configuration" && params["items"] == json!([]))
        );
        let ok = Message::classify(json!({"id": 4, "result": { "ok": true }})).unwrap();
        assert!(matches!(ok, Message::Response { id, result: Ok(value) } if id == json!(4) && value["ok"] == true));
        assert!(Message::classify(json!(["not", "an", "object"])).is_none());
    }
}
