//! Süreç çıktısının satır bazlı çözümlemesi: ANSI temizleme, parça sınırında satır birleştirme, hazır olma / port /
//! debug hedefi yakalama ve `(Y/n)` sorusu tespiti (#77).

use std::sync::LazyLock;

use regex::Regex;

static ANSI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-9;?]*[A-Za-z@]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b[()][A-Za-z0-9]|\x1b[=>]|[\r\x07]")
        .unwrap()
});
static SPRING_READY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Started \S+ in [0-9.]+ seconds").unwrap());
static JDWP_READY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Listening for transport dt_socket at address: (\d+)").unwrap());
static NODE_INSPECTOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Debugger listening on ws://[^/]*:(\d+)/").unwrap());
static PORTS: LazyLock<[Regex; 3]> = LazyLock::new(|| {
    [
        Regex::new(r"started on port[s]?[^0-9]*(\d+)").unwrap(),
        Regex::new(r"localhost:(\d+)").unwrap(),
        Regex::new(r"127\.0\.0\.1:(\d+)").unwrap(),
    ]
});
static CONFIRM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\(\s*[Yy]e?s?\s*/\s*[Nn]o?\s*\)|\[\s*[Yy]e?s?\s*/\s*[Nn]o?\s*\])$").unwrap());
static LEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[?›\s]+").unwrap());
static CHOICE_TAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*›?\s*[(\[][^)\]]*[)\]]$").unwrap());

/// Bitmemiş satır bu boyu aşınca tam satır sayılır.
const MAX_PARTIAL: usize = 4096;

/// ANSI kaçış dizilerini, `\r` ve zil karakterini atar, kenar boşluklarını kırpar.
pub fn plain(s: &str) -> String {
    ANSI.replace_all(s, "").trim().to_string()
}

/// ANSI dizilerini atar ama boşlukları korur (log satırı görünümü).
pub fn strip_ansi(s: &str) -> String {
    ANSI.replace_all(s, "").into_owned()
}

/// Bayt akışını satırlara böler; parça sınırında bölünen satır bir sonraki parçayla birleşir.
#[derive(Debug, Default, Clone)]
pub struct LineSplitter {
    partial: Vec<u8>,
}

impl LineSplitter {
    /// Yeni baytları ekler, tamamlanan satırları (`\r\n` / `\n` atılmış, UTF-8 kayıplı) döndürür.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.partial.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(i) = self.partial.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.partial.drain(..=i).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(String::from_utf8_lossy(&line).into_owned());
        }
        if self.partial.len() > MAX_PARTIAL {
            lines.push(String::from_utf8_lossy(&std::mem::take(&mut self.partial)).into_owned());
        }
        lines
    }

    /// Henüz `\n` gelmemiş son satır.
    pub fn partial(&self) -> String {
        String::from_utf8_lossy(&self.partial).into_owned()
    }

    /// Bitmemiş satırı boşaltır (süreç kapanınca).
    pub fn take_partial(&mut self) -> Option<String> {
        (!self.partial.is_empty()).then(|| String::from_utf8_lossy(&std::mem::take(&mut self.partial)).into_owned())
    }
}

/// Uygulamanın hazır olduğunu gösteren satır mı (Spring: `Started X in N seconds`). Node script'leri hazır deseni
/// taşımaz (başlayınca "running").
pub fn spring_ready(line: &str) -> bool {
    SPRING_READY.is_match(line)
}

/// JDWP dinleme satırındaki port.
pub fn jdwp_port(line: &str) -> Option<u16> {
    JDWP_READY.captures(line)?.get(1)?.as_str().parse().ok()
}

/// Node inspector satırındaki port (`Debugger listening on ws://127.0.0.1:PORT/…`).
pub fn inspector_port(line: &str) -> Option<u16> {
    NODE_INSPECTOR.captures(line)?.get(1)?.as_str().parse().ok()
}

/// Uygulama portu (`started on port 8080`, `localhost:3000`, `127.0.0.1:5173`); `ws://` içeren satırlar (inspector)
/// atlanır.
pub fn app_port(line: &str) -> Option<String> {
    if line.contains("ws://") {
        return None;
    }
    PORTS.iter().find_map(|re| re.captures(line).and_then(|c| c.get(1)).map(|m| m.as_str().to_string()))
}

/// Temizlenmiş bitmemiş satır bir onay sorusuyla (`(Y/n)`, `[y/N]`, `(yes/no)`) mı bitiyor.
pub fn is_confirm(tail: &str) -> bool {
    CONFIRM.is_match(tail)
}

/// Kullanıcıya gösterilecek soru metni: önceki satır bağlamı (sorudan farklıysa) + `?`/`›` önekleri ve seçenek
/// parantezi atılmış soru.
pub fn confirm_question(tail: &str, last_line: Option<&str>) -> String {
    let context = match last_line {
        Some(l) if l != tail => format!("{} ", LEAD.replace(l, "")),
        _ => String::new(),
    };
    let q = LEAD.replace(tail, "");
    format!("{context}{}", CHOICE_TAIL.replace(&q, ""))
}

/// Onay sorusu takibi: aynı soru bir kez sorulur, yeni tam satır gelince sıfırlanır.
#[derive(Debug, Default, Clone)]
pub struct PromptTracker {
    last_line: Option<String>,
    asked: Option<String>,
    answered: bool,
}

impl PromptTracker {
    /// Tam bir satır geldi (boş değilse bağlam olur, bekleyen soru düşer).
    pub fn on_line(&mut self, line: &str) {
        let text = plain(line);
        if !text.is_empty() {
            self.last_line = Some(text);
            self.asked = None;
            self.answered = false;
        }
    }

    /// Bitmemiş son satırı denetler; yeni bir soru varsa metnini döndürür.
    pub fn check(&mut self, partial: &str) -> Option<String> {
        let tail = plain(partial);
        if !is_confirm(&tail) || self.asked.as_deref() == Some(tail.as_str()) {
            return None;
        }
        let q = confirm_question(&tail, self.last_line.as_deref());
        self.asked = Some(tail);
        self.answered = false;
        Some(q)
    }

    /// Cevap bekleyen soru var mı (sorulmuş, cevaplanmamış, arkasından yeni satır gelmemiş).
    pub fn pending(&self) -> bool {
        self.asked.is_some() && !self.answered
    }

    /// Soru cevaplandı; aynı soru yine sorulmaz.
    pub fn answered(&mut self) {
        self.answered = true;
    }

    /// Yeni çalıştırma.
    pub fn reset(&mut self) {
        *self = PromptTracker::default();
    }
}
