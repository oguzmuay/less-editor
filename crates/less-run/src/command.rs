//! Öğe + ayar + mod → çalıştırılacak komut (#34, #69). Lua `build_cmd` / `start` ile birebir.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::RunConfig;
use crate::model::{BuildTool, ItemKind, ItemSpec};
use crate::providers::walk_find_upward;

/// Gradle `bootRun`'a JDWP / VM argümanı ekleyen init script'i (`-PrunnerJdwp`, `-PrunnerJvmArgs`).
pub const JDWP_GRADLE: &str = include_str!("providers/jdwp.gradle");

/// İlk JDWP portu; doluysa sıradaki boş port seçilir.
pub const JDWP_BASE_PORT: u16 = 5005;

/// Çalıştırma modu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Run,
    Debug,
}

/// Başlatılacak süreç: argv, klasör ve (devralınan ortama eklenecek) değişkenler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
}

/// Komut kurulumunun ortam bağımlı girdileri.
#[derive(Debug, Clone)]
pub struct BuildContext<'a> {
    pub mode: Mode,
    /// Debug modunda JDWP portu (Spring); Node için kullanılmaz.
    pub debug_port: Option<u16>,
    /// `jdwp.gradle`'ın diskteki yolu (`ensure_gradle_init`).
    pub gradle_init: &'a Path,
    /// JVM debugger bağlanana kadar beklesin mi (`suspend=y`); varsayılan false.
    pub suspend: bool,
    /// Süreci başlatan ortamdaki `NODE_OPTIONS` (debug modunda VM seçeneği yoksa taban olur).
    pub inherited_node_options: Option<String>,
}

/// JDWP ajanının seçenekleri. Yalnız 127.0.0.1: JDWP kimlik doğrulamasızdır, ağa açmak uzaktan kod çalıştırmaya
/// izin verir.
pub fn jdwp_options(port: u16, suspend: bool) -> String {
    format!("transport=dt_socket,server=y,suspend={},address=127.0.0.1:{port}", if suspend { "y" } else { "n" })
}

/// `jdwp.gradle`'ı `dir` altına yazar (içerik farklıysa) ve yolunu döndürür.
pub fn ensure_gradle_init(dir: &Path) -> io::Result<PathBuf> {
    let path = dir.join("jdwp.gradle");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(JDWP_GRADLE) {
        std::fs::create_dir_all(dir)?;
        std::fs::write(&path, JDWP_GRADLE)?;
    }
    Ok(path)
}

fn wrapper_or(cwd: &Path, wrapper: &str, global: &str) -> String {
    walk_find_upward(cwd, wrapper).map(|p| p.display().to_string()).unwrap_or_else(|| global.to_string())
}

/// Öğenin komutunu kurar. Spring Maven: `-Dspring-boot.run.profiles`, VM seçenekleri + JDWP tek
/// `-Dspring-boot.run.jvmArguments`, argümanlar `-Dspring-boot.run.arguments`; Gradle: profil + argümanlar `--args`,
/// JDWP / VM seçenekleri init script'le. Node: `<pm> run <script> [-- args…]` (argümanlar boşlukla bölünür),
/// VM seçenekleri `NODE_OPTIONS`, debug'da `--inspect=127.0.0.1:0` eklenir.
pub fn build(item: &ItemSpec, cfg: &RunConfig, ctx: &BuildContext) -> CommandSpec {
    let mut env: BTreeMap<String, String> = cfg.env.clone().unwrap_or_default();
    let debug_port = (ctx.mode == Mode::Debug).then_some(ctx.debug_port).flatten();
    let vm = cfg.vm_options.as_deref().filter(|s| !s.is_empty());
    let args = cfg.args.as_deref().filter(|s| !s.is_empty());
    let argv = match &item.kind {
        ItemKind::Spring { tool: BuildTool::Maven, .. } => {
            let mut cmd = vec![wrapper_or(&item.cwd, "mvnw", "mvn"), "spring-boot:run".to_string()];
            if let Some(p) = &cfg.profile {
                cmd.push(format!("-Dspring-boot.run.profiles={p}"));
            }
            let jdwp = debug_port.map(|p| format!(" -agentlib:jdwp={}", jdwp_options(p, ctx.suspend)));
            let jvm = format!("{}{}", vm.unwrap_or(""), jdwp.unwrap_or_default());
            let jvm = jvm.trim();
            if !jvm.is_empty() {
                cmd.push(format!("-Dspring-boot.run.jvmArguments={jvm}"));
            }
            if let Some(a) = args {
                cmd.push(format!("-Dspring-boot.run.arguments={a}"));
            }
            cmd
        }
        ItemKind::Spring { tool: BuildTool::Gradle, .. } => {
            let mut cmd = vec![wrapper_or(&item.cwd, "gradlew", "gradle"), "bootRun".to_string()];
            let profile = cfg.profile.as_ref().map(|p| format!("--spring.profiles.active={p}")).unwrap_or_default();
            let app_args = format!("{profile} {}", args.unwrap_or(""));
            let app_args = app_args.trim();
            if !app_args.is_empty() {
                cmd.push(format!("--args={app_args}"));
            }
            if debug_port.is_some() || vm.is_some() {
                cmd.push("--init-script".into());
                cmd.push(ctx.gradle_init.display().to_string());
            }
            if let Some(p) = debug_port {
                cmd.push(format!("-PrunnerJdwp={}", jdwp_options(p, ctx.suspend)));
            }
            if let Some(v) = vm {
                cmd.push(format!("-PrunnerJvmArgs={v}"));
            }
            cmd
        }
        ItemKind::Node { pm, script } => {
            let mut cmd = vec![pm.command().to_string(), "run".to_string(), script.clone()];
            if let Some(a) = args {
                cmd.push("--".into());
                cmd.extend(a.split_whitespace().map(String::from));
            }
            cmd
        }
    };
    if let (Some(var), Some(v)) = (item.vm_env(), vm) {
        env.insert(var.to_string(), v.to_string());
    }
    if ctx.mode == Mode::Debug && matches!(item.kind, ItemKind::Node { .. }) {
        let base = vm.map(String::from).or_else(|| ctx.inherited_node_options.clone()).unwrap_or_default();
        env.insert("NODE_OPTIONS".into(), format!("{base} --inspect=127.0.0.1:0").trim().to_string());
    }
    CommandSpec { argv, cwd: item.cwd.clone(), env }
}
