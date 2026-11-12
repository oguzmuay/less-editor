//! Sürücü yöneticisi: ikiliyi bulur, kurulum planı çıkarır, onaylanırsa indirir (SHA-256 doğrulamalı).
//! Onay bayrağı olmadan hiçbir şey indirilmez (PLAN §3).

use crate::connections::data_dir;
use crate::types::DbType;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Sürücü sürümü: indirme etiketi `drivers-v<ver>`.
pub const DRIVERS_VERSION: &str = "0.1.0";

/// Sürücülerin kurulduğu klasör.
pub fn drivers_dir() -> PathBuf {
    data_dir().join("drivers")
}

/// Varsayılan manifest adresi (GitHub Release).
pub fn manifest_url() -> String {
    format!("https://github.com/oguzmuay/less-editor/releases/download/drivers-v{DRIVERS_VERSION}/manifest.json")
}

/// Bu makinenin hedef adı (`aarch64-apple-darwin` gibi).
pub fn current_target() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "macos" => format!("{arch}-apple-darwin"),
        "linux" => format!("{arch}-unknown-linux-gnu"),
        other => format!("{arch}-{other}"),
    }
}

/// Sürücü ikilisini arar: önce çalışan exe'nin yanı (geliştirme: `cargo run`), sonra veri klasörü.
pub fn find_driver(ty: DbType) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    find_driver_in(ty, exe_dir.as_deref(), &drivers_dir())
}

/// `find_driver`'ın klasörleri verilebilen hali.
pub fn find_driver_in(ty: DbType, dev_dir: Option<&Path>, installed_dir: &Path) -> Option<PathBuf> {
    let bin = ty.driver_binary();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = dev_dir {
        candidates.push(d.join(&bin));
        if d.file_name().is_some_and(|n| n == "deps")
            && let Some(parent) = d.parent()
        {
            candidates.push(parent.join(&bin));
        }
    }
    candidates.push(installed_dir.join(&bin));
    candidates.into_iter().find(|p| p.is_file())
}

/// Manifest: hedef → ikili adı → indirme bilgisi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// `hedef → ikili adı → dosya`.
    pub drivers: BTreeMap<String, BTreeMap<String, ManifestEntry>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub url: String,
    pub sha256: String,
}

/// Kullanıcıya "Install driver?" sorusunda gösterilecek plan.
#[derive(Debug, Clone, PartialEq)]
pub struct InstallPlan {
    pub db_type: DbType,
    pub binary: String,
    pub url: String,
    pub sha256: String,
    pub dest: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstallError {
    /// Onay verilmedi; hiçbir şey indirilmedi.
    NotApproved,
    Network(String),
    NoDriverForTarget(String),
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
    Io(String),
    BadManifest(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NotApproved => write!(f, "Driver install was not approved"),
            InstallError::Network(m) => write!(f, "Download failed: {m}"),
            InstallError::NoDriverForTarget(m) => write!(f, "No driver available for {m}"),
            InstallError::ChecksumMismatch { expected, actual } => {
                write!(f, "Checksum mismatch (expected {expected}, got {actual}); the download was discarded")
            }
            InstallError::Io(m) => write!(f, "Could not write the driver: {m}"),
            InstallError::BadManifest(m) => write!(f, "Invalid driver manifest: {m}"),
        }
    }
}

impl std::error::Error for InstallError {}

/// Manifest metnini ayrıştırır.
pub fn parse_manifest(text: &str) -> Result<Manifest, InstallError> {
    serde_json::from_str(text).map_err(|e| InstallError::BadManifest(e.to_string()))
}

/// Manifest içinden türün planını çıkarır (ağ yok).
pub fn plan_from_manifest(
    ty: DbType,
    manifest: &Manifest,
    target: &str,
    dest_dir: &Path,
) -> Result<InstallPlan, InstallError> {
    let binary = ty.driver_binary();
    let entry = manifest
        .drivers
        .get(target)
        .and_then(|m| m.get(&binary))
        .ok_or_else(|| InstallError::NoDriverForTarget(format!("{} on {target}", ty.display_name())))?;
    Ok(InstallPlan {
        db_type: ty,
        dest: dest_dir.join(&binary),
        binary,
        url: entry.url.clone(),
        sha256: entry.sha256.to_ascii_lowercase(),
    })
}

/// Manifesti indirir (küçük JSON; onaydan önce yalnız bu istek yapılır) ve planı çıkarır.
pub fn plan_install(ty: DbType) -> Result<InstallPlan, InstallError> {
    let body = fetch(&manifest_url(), 1024 * 1024)?;
    let text = String::from_utf8(body).map_err(|e| InstallError::BadManifest(e.to_string()))?;
    plan_from_manifest(ty, &parse_manifest(&text)?, &current_target(), &drivers_dir())
}

/// Hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn fetch(url: &str, max: u64) -> Result<Vec<u8>, InstallError> {
    if let Some(path) = url.strip_prefix("file://") {
        return std::fs::read(path).map_err(|e| InstallError::Network(e.to_string()));
    }
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| InstallError::Network(e.to_string()))?;
    let mut buf = Vec::new();
    resp.into_reader().take(max).read_to_end(&mut buf).map_err(|e| InstallError::Network(e.to_string()))?;
    Ok(buf)
}

/// Planı uygular: indirir, SHA-256 doğrular, `chmod +x`, atomik rename. `approved == false` ise hiçbir ağ isteği yapılmaz.
pub fn install(plan: &InstallPlan, approved: bool) -> Result<PathBuf, InstallError> {
    if !approved {
        return Err(InstallError::NotApproved);
    }
    let bytes = fetch(&plan.url, 256 * 1024 * 1024)?;
    install_bytes(plan, &bytes)
}

/// İndirilmiş baytları doğrulayıp yerleştirir (testlerde ağsız kullanılır).
pub fn install_bytes(plan: &InstallPlan, bytes: &[u8]) -> Result<PathBuf, InstallError> {
    let actual = sha256_hex(bytes);
    if actual != plan.sha256 {
        return Err(InstallError::ChecksumMismatch { expected: plan.sha256.clone(), actual });
    }
    let io = |e: std::io::Error| InstallError::Io(e.to_string());
    if let Some(dir) = plan.dest.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = plan.dest.with_extension("part");
    std::fs::write(&tmp, bytes).map_err(io)?;
    set_executable(&tmp).map_err(io)?;
    std::fs::rename(&tmp, &plan.dest).map_err(io)?;
    Ok(plan.dest.clone())
}

fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(sha: &str) -> Manifest {
        let mut inner = BTreeMap::new();
        inner.insert(
            "less-driver-postgres".to_string(),
            ManifestEntry { url: "file:///nowhere".into(), sha256: sha.into() },
        );
        let mut drivers = BTreeMap::new();
        drivers.insert("aarch64-apple-darwin".to_string(), inner);
        Manifest { version: "0.1.0".into(), drivers }
    }

    #[test]
    fn sha256_known_value() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn plan_from_manifest_ok_and_missing() {
        let dir = Path::new("/x/drivers");
        let m = manifest("ABCDEF");
        let p = plan_from_manifest(DbType::Postgresql, &m, "aarch64-apple-darwin", dir).unwrap();
        assert_eq!(p.dest, dir.join("less-driver-postgres"));
        assert_eq!(p.sha256, "abcdef");
        assert!(matches!(
            plan_from_manifest(DbType::Sqlite, &m, "aarch64-apple-darwin", dir),
            Err(InstallError::NoDriverForTarget(_))
        ));
        assert!(plan_from_manifest(DbType::Postgresql, &m, "x86_64-unknown-linux-gnu", dir).is_err());
    }

    #[test]
    fn manifest_json_roundtrip() {
        let m = manifest("aa");
        let text = serde_json::to_string(&m).unwrap();
        assert_eq!(parse_manifest(&text).unwrap(), m);
        assert!(parse_manifest("nope").is_err());
    }

    #[test]
    fn install_requires_approval_and_does_not_touch_network() {
        let dir = tempfile::tempdir().unwrap();
        let plan = InstallPlan {
            db_type: DbType::Postgresql,
            binary: "less-driver-postgres".into(),
            url: "https://invalid.invalid/x".into(),
            sha256: sha256_hex(b"x"),
            dest: dir.path().join("less-driver-postgres"),
        };
        assert_eq!(install(&plan, false), Err(InstallError::NotApproved));
        assert!(!plan.dest.exists());
    }

    #[test]
    fn install_verifies_checksum_and_chmods() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"#!/bin/sh\necho hi\n";
        let mut plan = InstallPlan {
            db_type: DbType::Sqlite,
            binary: "less-driver-sqlite".into(),
            url: "file:///unused".into(),
            sha256: sha256_hex(b"other"),
            dest: dir.path().join("drivers/less-driver-sqlite"),
        };
        assert!(matches!(install_bytes(&plan, bytes), Err(InstallError::ChecksumMismatch { .. })));
        assert!(!plan.dest.exists());
        plan.sha256 = sha256_hex(bytes);
        let out = install_bytes(&plan, bytes).unwrap();
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o111, 0o111);
        assert_eq!(std::fs::read(&out).unwrap(), bytes);
        assert!(!plan.dest.with_extension("part").exists());
    }

    #[test]
    fn install_from_file_url() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.bin");
        std::fs::write(&src, b"binary").unwrap();
        let plan = InstallPlan {
            db_type: DbType::Sqlite,
            binary: "less-driver-sqlite".into(),
            url: format!("file://{}", src.display()),
            sha256: sha256_hex(b"binary"),
            dest: dir.path().join("out/less-driver-sqlite"),
        };
        assert!(install(&plan, true).is_ok());
        let bad = InstallPlan { sha256: sha256_hex(b"zzz"), ..plan };
        assert!(matches!(install(&bad, true), Err(InstallError::ChecksumMismatch { .. })));
    }

    #[test]
    fn find_driver_order() {
        let dev = tempfile::tempdir().unwrap();
        let inst = tempfile::tempdir().unwrap();
        assert!(find_driver_in(DbType::Sqlite, Some(dev.path()), inst.path()).is_none());
        std::fs::write(inst.path().join("less-driver-sqlite"), b"").unwrap();
        assert_eq!(
            find_driver_in(DbType::Sqlite, Some(dev.path()), inst.path()),
            Some(inst.path().join("less-driver-sqlite"))
        );
        std::fs::write(dev.path().join("less-driver-sqlite"), b"").unwrap();
        assert_eq!(
            find_driver_in(DbType::Sqlite, Some(dev.path()), inst.path()),
            Some(dev.path().join("less-driver-sqlite"))
        );
    }
}
