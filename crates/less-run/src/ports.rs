//! TCP port yardımcıları: dinlenen portlar (`lsof`), boş port arama, portu dinleyen süreç.

use std::collections::HashSet;
use std::net::TcpListener;
use std::process::Command;

/// `lsof -Fn` çıktısındaki `n…:PORT` satırlarından port kümesi.
pub fn parse_lsof_ports(out: &str) -> HashSet<u16> {
    out.lines()
        .filter_map(|l| l.strip_prefix('n'))
        .filter_map(|n| n.rsplit_once(':').and_then(|(_, p)| p.parse().ok()))
        .collect()
}

/// Sistemde TCP dinleyen portlar (lsof yoksa boş küme).
pub fn listening_ports() -> HashSet<u16> {
    Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fn"])
        .output()
        .map(|o| parse_lsof_ports(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// Port şu an dinleniyor mu (`lsof`).
pub fn is_listening(port: u16) -> bool {
    listening_pid(port).is_some()
}

/// Portu dinleyen sürecin pid'i (`lsof -t`).
pub fn listening_pid(port: u16) -> Option<u32> {
    let out = Command::new("lsof").args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.trim().parse().ok())
}

/// 127.0.0.1:port'a bağlanılabiliyor mu.
pub fn can_bind(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// `base`'den başlayarak (en çok 500 port) boş TCP portu: `busy`'de (lsof) ve `taken`'da (panelin başka öğelerine
/// ayrılmış) olmayan ve 127.0.0.1'e bağlanabilen ilk port.
pub fn free_port_with(base: u16, busy: &HashSet<u16>, taken: &HashSet<u16>) -> Option<u16> {
    (base..=base.saturating_add(500)).find(|p| !busy.contains(p) && !taken.contains(p) && can_bind(*p))
}

/// `free_port_with` + güncel `lsof` listesi.
pub fn free_port(base: u16, taken: &HashSet<u16>) -> Option<u16> {
    free_port_with(base, &listening_ports(), taken)
}

/// İşletim sisteminin verdiği geçici boş port (adaptör sunucusu için).
pub fn ephemeral_port() -> Option<u16> {
    TcpListener::bind(("127.0.0.1", 0)).ok()?.local_addr().ok().map(|a| a.port())
}
