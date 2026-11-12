//! Durum satırı için sistem özeti (#73): CPU %, RAM kullanılan / toplam, ev klasörü diskinde boş alan, yerel IPv4.
//! Harici komutlar (`ps`, `vm_stat`, `sysctl`, `df`, `ifconfig`) ve `/proc` okunur; ayrıştırıcılar saf ve testlidir.

use std::process::Command;

/// Bir ölçümün sonucu; ölçülemeyen alan `None`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub cpu_percent: Option<u32>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub disk_free: Option<u64>,
    pub ip: Option<String>,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Bayt değerini kısa GB metnine çevirir (`9.8G`, 100 GB üstünde ondalıksız).
pub fn gb(bytes: u64) -> String {
    let g = bytes as f64 / 1024f64.powi(3);
    if g >= 100.0 { format!("{g:.0}G") } else { format!("{g:.1}G") }
}

/// `ps -A -o %cpu` çıktısı ve çekirdek sayısından toplam CPU yüzdesi (0..100).
pub fn parse_ps_cpu(out: &str, cores: u32) -> Option<u32> {
    let sum: f64 = out.lines().filter_map(|l| l.trim().parse::<f64>().ok()).sum();
    (cores > 0).then(|| (sum / cores as f64).round().clamp(0.0, 100.0) as u32)
}

/// `vm_stat` çıktısından Activity Monitor "Memory Used"a yakın değer: uygulama + wired + sıkıştırılmış (#73).
pub fn parse_vm_stat(out: &str, total: u64) -> Option<u64> {
    let page: u64 = out.split("page size of ").nth(1)?.split_whitespace().next()?.parse().ok()?;
    let pages = |name: &str| -> u64 {
        out.lines()
            .find(|l| l.trim_start().starts_with(name))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|v| v.trim().trim_end_matches('.').parse().ok())
            .unwrap_or(0)
    };
    let anon = pages("Anonymous pages");
    let app = if anon > 0 { anon.saturating_sub(pages("Pages purgeable")) } else { pages("Pages active") };
    let used = (app + pages("Pages wired down") + pages("Pages occupied by compressor")) * page;
    Some(used.min(total))
}

/// `/proc/meminfo` metninden `(kullanılan, toplam)` bayt (MemTotal − MemAvailable).
pub fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let kb = |key: &str| -> Option<u64> {
        text.lines().find(|l| l.starts_with(key))?.split_whitespace().nth(1)?.parse::<u64>().ok().map(|v| v * 1024)
    };
    let total = kb("MemTotal:")?;
    Some((total.saturating_sub(kb("MemAvailable:")?), total))
}

/// `df -k` çıktısından boş bayt (son satırın 4. alanı).
pub fn parse_df(out: &str) -> Option<u64> {
    let line = out.lines().rfind(|l| !l.trim().is_empty())?;
    let avail: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail * 1024)
}

/// `ifconfig <arayüz>` çıktısından ilk IPv4.
pub fn parse_ifconfig_inet(out: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        (it.next() == Some("inet")).then(|| it.next().map(str::to_owned)).flatten().filter(|a| !a.starts_with("127."))
    })
}

fn local_ip() -> Option<String> {
    if cfg!(target_os = "macos") {
        for iface in ["en0", "en1"] {
            if let Some(ip) = run("ifconfig", &[iface]).and_then(|o| parse_ifconfig_inet(&o)) {
                return Some(ip);
            }
        }
        None
    } else {
        run("hostname", &["-I"])?
            .split_whitespace()
            .find(|a| a.contains('.') && !a.starts_with("127."))
            .map(str::to_owned)
    }
}

fn memory() -> (Option<u64>, Option<u64>) {
    if cfg!(target_os = "macos") {
        let total = run("sysctl", &["-n", "hw.memsize"]).and_then(|s| s.trim().parse::<u64>().ok());
        let used = total.and_then(|t| parse_vm_stat(&run("vm_stat", &[])?, t));
        (used, total)
    } else {
        match std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| parse_meminfo(&t)) {
            Some((u, t)) => (Some(u), Some(t)),
            None => (None, None),
        }
    }
}

fn cpu() -> Option<u32> {
    let cores = std::thread::available_parallelism().ok()?.get() as u32;
    parse_ps_cpu(&run("ps", &["-A", "-o", "%cpu"])?, cores)
}

/// Şu anki sistem durumu (birkaç alt süreç çalıştırır; arka plan iş parçacığında çağır, ~5 sn'de bir).
pub fn sample() -> Snapshot {
    let home = less_home();
    let (mem_used, mem_total) = memory();
    Snapshot {
        cpu_percent: cpu(),
        mem_used,
        mem_total,
        disk_free: run("df", &["-k", &home]).and_then(|o| parse_df(&o)),
        ip: local_ip(),
    }
}

fn less_home() -> String {
    crate::paths::home().to_string_lossy().into_owned()
}

impl Snapshot {
    /// Durum satırı metni: `CPU 12%  RAM 9.8G/16.0G  Disk 120G  Leaked IP 192.168.1.5`. Ölçülemeyen parça atlanır.
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        if let Some(c) = self.cpu_percent {
            parts.push(format!("CPU {c}%"));
        }
        if let (Some(u), Some(t)) = (self.mem_used, self.mem_total) {
            parts.push(format!("RAM {}/{}", gb(u), gb(t)));
        }
        if let Some(d) = self.disk_free {
            parts.push(format!("Disk {} free", gb(d)));
        }
        if let Some(ip) = &self.ip {
            parts.push(format!("Leaked IP {ip}"));
        }
        parts.join("  ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gb_formats_73() {
        assert_eq!(gb(10 * 1024u64.pow(3)), "10.0G");
        assert_eq!(gb(150 * 1024u64.pow(3)), "150G");
    }

    #[test]
    fn ps_cpu_sum_over_cores_73() {
        assert_eq!(parse_ps_cpu("%CPU\n 50.0\n150.0\n", 4), Some(50));
        assert_eq!(parse_ps_cpu("%CPU\n900.0\n", 4), Some(100));
        assert_eq!(parse_ps_cpu("x", 0), None);
    }

    #[test]
    fn vm_stat_semantics_73() {
        let out = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:      100.\n\
                   Pages active:    5000.\nAnonymous pages:   1000.\nPages purgeable:   200.\n\
                   Pages wired down:  300.\nPages occupied by compressor: 100.\n";
        assert_eq!(parse_vm_stat(out, u64::MAX), Some((800 + 300 + 100) * 16384));
        assert_eq!(parse_vm_stat(out, 1000), Some(1000));
    }

    #[test]
    fn meminfo_73() {
        let t = "MemTotal: 1000 kB\nMemFree: 1 kB\nMemAvailable: 400 kB\n";
        assert_eq!(parse_meminfo(t), Some((600 * 1024, 1000 * 1024)));
    }

    #[test]
    fn df_and_ifconfig_73() {
        let df = "Filesystem 1024-blocks Used Available Capacity Mounted\n/dev/x 100 50 50 50% /\n";
        assert_eq!(parse_df(df), Some(50 * 1024));
        let ifc = "en0: flags=8863\n\tinet6 fe80::1\n\tinet 192.168.1.5 netmask 0xffffff00\n";
        assert_eq!(parse_ifconfig_inet(ifc).as_deref(), Some("192.168.1.5"));
        assert_eq!(parse_ifconfig_inet("\tinet 127.0.0.1 netmask"), None);
    }

    #[test]
    fn text_skips_missing_parts_73() {
        let s = Snapshot { cpu_percent: Some(12), ip: Some("10.0.0.2".into()), ..Snapshot::default() };
        assert_eq!(s.text(), "CPU 12%  Leaked IP 10.0.0.2");
    }
}
