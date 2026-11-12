//! Dosya yolu ↔ `file://` URI ve bayt ↔ UTF-16 sütun dönüşümleri.

use std::path::{Path, PathBuf};

/// Mutlak yolu `file://` URI'sine çevirir (RFC 3986: ayrılmamış karakterler ve `/` dışındakiler yüzde kodlanır).
pub fn path_to_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `file://` URI'sini yola çevirir; başka şema ise `None`.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

/// Satır içindeki bayt sütununu LSP UTF-16 karakter sütununa çevirir.
pub fn byte_to_utf16(line: &str, byte_col: usize) -> u32 {
    let end = floor_char_boundary(line, byte_col);
    line[..end].chars().map(|c| c.len_utf16() as u32).sum()
}

/// LSP UTF-16 sütununu satır içi bayt sütununa çevirir (satır sonunu aşarsa satır uzunluğu).
pub fn utf16_to_byte(line: &str, utf16_col: u32) -> usize {
    let mut units = 0u32;
    for (i, c) in line.char_indices() {
        if units >= utf16_col {
            return i;
        }
        units += c.len_utf16() as u32;
    }
    line.len()
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_roundtrip() {
        let p = Path::new("/Users/me/My Project/çalış#1.java");
        let uri = path_to_uri(p);
        assert_eq!(uri, "file:///Users/me/My%20Project/%C3%A7al%C4%B1%C5%9F%231.java");
        assert_eq!(uri_to_path(&uri).unwrap(), p);
        assert_eq!(uri_to_path("jdt://contents/x"), None);
    }

    #[test]
    fn utf16_columns() {
        let line = "a😀ç=1";
        assert_eq!(byte_to_utf16(line, 1), 1);
        assert_eq!(byte_to_utf16(line, 5), 3);
        assert_eq!(byte_to_utf16(line, 7), 4);
        assert_eq!(utf16_to_byte(line, 3), 5);
        assert_eq!(utf16_to_byte(line, 4), 7);
        assert_eq!(utf16_to_byte(line, 99), line.len());
    }
}
