//! jdtls desteği (#25): JDK bulma, sunucu Java'sı (25+), `java.configuration.runtimes`, proje kökü, çalışma klasörü.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Güncel jdtls'in çalışmak için istediği en düşük Java sürümü (Java 21 ile `exit code 13`).
pub const JDTLS_MIN_JAVA: u32 = 25;

/// Projeler için varsayılan JDK (pom'da sürüm yoksa).
pub const DEFAULT_PROJECT_JAVA: u32 = 21;

const BUILD_FILES: &[&str] = &["pom.xml", "build.gradle", "build.gradle.kts"];

/// JDK arama yerleri. Varsayılan: Homebrew `openjdk*` (Apple Silicon ve Intel) ve `/Library/Java/JavaVirtualMachines`.
#[derive(Debug, Clone)]
pub struct JdkSearch {
    /// Altında `openjdk*/libexec/openjdk.jdk/Contents/Home` aranan Homebrew `opt` klasörleri.
    pub homebrew_opt: Vec<PathBuf>,
    /// Altında `*/Contents/Home` aranan klasörler.
    pub jvm_dirs: Vec<PathBuf>,
}

impl Default for JdkSearch {
    fn default() -> Self {
        JdkSearch {
            homebrew_opt: vec!["/opt/homebrew/opt".into(), "/usr/local/opt".into()],
            jvm_dirs: vec!["/Library/Java/JavaVirtualMachines".into()],
        }
    }
}

/// Bulunan JDK'lar: ana sürüm → JAVA_HOME (her sürümden ilk bulunan).
pub fn find_jdks(search: &JdkSearch) -> BTreeMap<u32, PathBuf> {
    let mut homes = Vec::new();
    for opt in &search.homebrew_opt {
        for entry in sorted_dir(opt) {
            let name = entry.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if name.starts_with("openjdk") {
                homes.push(entry.join("libexec/openjdk.jdk/Contents/Home"));
            }
        }
    }
    for dir in &search.jvm_dirs {
        homes.extend(sorted_dir(dir).into_iter().map(|e| e.join("Contents/Home")));
    }
    let mut jdks = BTreeMap::new();
    for home in homes {
        let Some(major) = fs::read_to_string(home.join("release")).ok().and_then(|r| release_major(&r)) else {
            continue;
        };
        if home.join("bin/java").exists() {
            jdks.entry(major).or_insert(home);
        }
    }
    jdks
}

fn sorted_dir(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    out.sort();
    out
}

/// `release` dosyasındaki `JAVA_VERSION="26.0.1"` satırından ana sürüm (`1.8.0` → 8).
pub fn release_major(release: &str) -> Option<u32> {
    let line = release.lines().find_map(|l| l.trim().strip_prefix("JAVA_VERSION="))?;
    let version = line.trim().trim_matches('"');
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 { parts.next()?.parse().ok() } else { Some(first) }
}

/// jdtls'i çalıştıracak en yeni JDK'nın `bin/java` yolu (en az [`JDTLS_MIN_JAVA`]); yoksa `None`
/// (arayüz "jdtls requires Java 25+…" uyarısını gösterir).
pub fn jdtls_java(jdks: &BTreeMap<u32, PathBuf>) -> Option<PathBuf> {
    jdks.range(JDTLS_MIN_JAVA..).next_back().map(|(_, home)| home.join("bin/java"))
}

/// JDK'lar → jdtls `java.configuration.runtimes` (JavaSE-17, JavaSE-21 varsayılan, JavaSE-26…).
pub fn java_runtimes(jdks: &BTreeMap<u32, PathBuf>) -> Value {
    Value::Array(
        jdks.iter()
            .map(|(major, home)| {
                let mut rt = json!({ "name": format!("JavaSE-{major}"), "path": home.to_string_lossy() });
                if *major == DEFAULT_PROJECT_JAVA {
                    rt["default"] = json!(true);
                }
                rt
            })
            .collect(),
    )
}

/// `path`'ten yukarı doğru, `names`'ten birini içeren ilk klasör.
pub fn find_root_with(path: &Path, names: &[&str]) -> Option<PathBuf> {
    let start = if path.is_dir() { path } else { path.parent()? };
    start.ancestors().find(|dir| names.iter().any(|n| dir.join(n).exists())).map(Path::to_path_buf)
}

/// Dosya bir Spring Boot projesinde mi: yukarı doğru herhangi bir derleme dosyası `spring-boot` içeriyor mu.
pub fn is_spring_boot(path: &Path) -> bool {
    path.ancestors()
        .skip(1)
        .any(|dir| BUILD_FILES.iter().any(|f| fs::read_to_string(dir.join(f)).is_ok_and(|s| s.contains("spring-boot"))))
}

/// Monorepo'da Java dosyasının proje kökü: Gradle settings → iç içe pom/build.gradle zincirinin en üstü → mvnw/gradlew/.git.
pub fn java_root(path: &Path) -> Option<PathBuf> {
    if let Some(root) = find_root_with(path, &["settings.gradle", "settings.gradle.kts"]) {
        return Some(root);
    }
    if let Some(mut dir) = find_root_with(path, BUILD_FILES) {
        while let Some(parent) = dir.parent() {
            if !BUILD_FILES.iter().any(|f| parent.join(f).exists()) {
                break;
            }
            dir = parent.to_path_buf();
        }
        return Some(dir);
    }
    find_root_with(path, &["mvnw", "gradlew", ".git"])
}

/// jdtls proje adı: `<klasör>-<yolun sha256'sının ilk 8 hanesi>` (aynı adlı `backend`'ler çakışmasın).
pub fn project_name(root: &Path) -> String {
    let base = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "root".into());
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{base}-{hex}")
}

/// Mason paylaşım klasöründen (ör. `~/.local/share/nvim/mason/share`) jdtls bundle'ları: java-debug-adapter ve
/// java-test jar'ları; OSGi olmayanlar (runner, jacoco) ve jdtls'in zaten içerdikleri çıkarılır.
pub fn jdtls_bundles(mason_share: &Path) -> Vec<PathBuf> {
    const SKIP: &[&str] = &["com.microsoft.java.test.runner-jar-with-dependencies.jar", "jacocoagent.jar"];
    let plugins = mason_share.join("jdtls/plugins");
    let mut jars: Vec<PathBuf> = sorted_dir(&mason_share.join("java-debug-adapter"))
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("com.microsoft.java.debug.plugin")))
        .collect();
    jars.extend(sorted_dir(&mason_share.join("java-test")));
    jars.retain(|jar| {
        let Some(name) = jar.file_name() else { return false };
        jar.extension().is_some_and(|e| e == "jar")
            && !SKIP.contains(&name.to_string_lossy().as_ref())
            && !plugins.join(name).exists()
    });
    jars
}

/// jdtls başlatma bilgisi.
#[derive(Debug, Clone, PartialEq)]
pub struct JdtlsLaunch {
    pub args: Vec<String>,
    /// `~/.cache/less-rs/jdtls/<ad>-<sha8>`; altında `config/` ve `workspace/`.
    pub cache_dir: PathBuf,
    pub settings: Value,
    pub init_options: Value,
    /// Sunucu için Java 25+ bulunamadı (jdtls betiği PATH'teki java'yı dener, büyük olasılıkla kapanır).
    pub missing_java: bool,
}

/// jdtls argümanları ve ayarları. `mason_share` verilirse lombok ajanı ve bundle'lar eklenir. `on_demand`: yalnız açılan
/// dosyanın modülü içe aktarılır, diğer modüller `~/.m2` jar'larından okunur (jdtls 1.64+; eskisi ayarı yok sayar).
/// Full modun çalışma alanı tüm modülleri içerdiği için iki mod ayrı önbellek klasörü kullanır.
pub fn jdtls_launch(
    root: &Path,
    cache_base: &Path,
    jdks: &BTreeMap<u32, PathBuf>,
    mason_share: Option<&Path>,
    on_demand: bool,
) -> JdtlsLaunch {
    let name = project_name(root);
    let cache_dir = cache_base.join("jdtls").join(if on_demand { format!("{name}-ondemand") } else { name });
    let mut args = Vec::new();
    let java = jdtls_java(jdks);
    if let Some(java) = &java {
        args.push(format!("--java-executable={}", java.display()));
    }
    if let Some(lombok) = mason_share.map(|s| s.join("jdtls/lombok.jar")).filter(|p| p.exists()) {
        args.push(format!("--jvm-arg=-javaagent:{}", lombok.display()));
    }
    args.extend([
        "-configuration".to_string(),
        cache_dir.join("config").display().to_string(),
        "-data".to_string(),
        cache_dir.join("workspace").display().to_string(),
        "--jvm-arg=-Djava.import.generatesMetadataFilesAtProjectRoot=false".to_string(),
    ]);
    let bundles: Vec<String> =
        mason_share.map(jdtls_bundles).unwrap_or_default().iter().map(|p| p.display().to_string()).collect();
    let settings = json!({ "java": {
        "configuration": { "runtimes": java_runtimes(jdks) },
        "import": { "mode": if on_demand { "ondemand" } else { "full" } },
    } });
    JdtlsLaunch {
        args,
        cache_dir,
        init_options: json!({
            "bundles": bundles,
            "settings": settings,
            "extendedClientCapabilities": { "classFileContentsSupport": true },
        }),
        settings,
        missing_java: java.is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_jdk(home: &Path, version: &str) {
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("bin/java"), "").unwrap();
        fs::write(home.join("release"), format!("IMPLEMENTOR=\"x\"\nJAVA_VERSION=\"{version}\"\n")).unwrap();
    }

    #[test]
    fn finds_jdks_and_picks_25_plus_for_server_25() {
        let tmp = tempfile::tempdir().unwrap();
        let opt = tmp.path().join("opt");
        let jvm = tmp.path().join("jvm");
        fake_jdk(&opt.join("openjdk/libexec/openjdk.jdk/Contents/Home"), "26.0.1");
        fake_jdk(&opt.join("openjdk@21/libexec/openjdk.jdk/Contents/Home"), "21.0.5");
        fake_jdk(&jvm.join("zulu-17.jdk/Contents/Home"), "17.0.2");
        fs::create_dir_all(jvm.join("broken.jdk/Contents/Home")).unwrap();
        fs::create_dir_all(opt.join("python@3/libexec")).unwrap();
        let jdks = find_jdks(&JdkSearch { homebrew_opt: vec![opt.clone()], jvm_dirs: vec![jvm] });
        assert_eq!(jdks.keys().copied().collect::<Vec<_>>(), vec![17, 21, 26]);
        assert_eq!(jdtls_java(&jdks), Some(opt.join("openjdk/libexec/openjdk.jdk/Contents/Home/bin/java")));
        let rts = java_runtimes(&jdks);
        assert_eq!(rts[1]["name"], "JavaSE-21");
        assert_eq!(rts[1]["default"], true);
        assert!(rts[2].get("default").is_none());
        let old: BTreeMap<u32, PathBuf> = [(21, PathBuf::from("/x"))].into();
        assert_eq!(jdtls_java(&old), None);
        assert_eq!(release_major("JAVA_VERSION=\"1.8.0_392\""), Some(8));
    }

    #[test]
    fn java_root_rules_25() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        fs::create_dir_all(t.join(".git")).unwrap();
        let be = t.join("backend");
        fs::create_dir_all(be.join("api/src/main/java")).unwrap();
        fs::write(be.join("pom.xml"), "").unwrap();
        fs::write(be.join("api/pom.xml"), "").unwrap();
        let file = be.join("api/src/main/java/A.java");
        fs::write(&file, "").unwrap();
        assert_eq!(java_root(&file), Some(be.clone()));

        let gr = t.join("gradle-app");
        fs::create_dir_all(gr.join("core/src")).unwrap();
        fs::write(gr.join("settings.gradle.kts"), "").unwrap();
        fs::write(gr.join("core/build.gradle.kts"), "").unwrap();
        assert_eq!(java_root(&gr.join("core/src/B.java")), Some(gr));

        let loose = t.join("scripts");
        fs::create_dir_all(&loose).unwrap();
        assert_eq!(java_root(&loose.join("C.java")), Some(t.to_path_buf()));
    }

    #[test]
    fn project_name_and_launch_25() {
        let root = Path::new("/work/backend");
        let name = project_name(root);
        assert!(name.starts_with("backend-") && name.len() == "backend-".len() + 8, "{name}");
        assert_ne!(name, project_name(Path::new("/other/backend")));

        let tmp = tempfile::tempdir().unwrap();
        let share = tmp.path().join("share");
        fs::create_dir_all(share.join("jdtls/plugins")).unwrap();
        fs::create_dir_all(share.join("java-test")).unwrap();
        fs::create_dir_all(share.join("java-debug-adapter")).unwrap();
        fs::write(share.join("java-debug-adapter/com.microsoft.java.debug.plugin-0.53.jar"), "").unwrap();
        fs::write(share.join("java-test/jacocoagent.jar"), "").unwrap();
        fs::write(share.join("java-test/com.microsoft.java.test.plugin-0.43.jar"), "").unwrap();
        fs::write(share.join("java-test/org.objectweb.asm.jar"), "").unwrap();
        fs::write(share.join("jdtls/plugins/org.objectweb.asm.jar"), "").unwrap();
        let bundles = jdtls_bundles(&share);
        let names: Vec<_> = bundles.iter().map(|b| b.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["com.microsoft.java.debug.plugin-0.53.jar", "com.microsoft.java.test.plugin-0.43.jar"]);

        let jdks: BTreeMap<u32, PathBuf> = [(26, PathBuf::from("/jdk26"))].into();
        let launch = jdtls_launch(root, Path::new("/cache/less-rs"), &jdks, Some(&share), false);
        assert_eq!(launch.args[0], "--java-executable=/jdk26/bin/java");
        assert!(launch.args.contains(&"--jvm-arg=-Djava.import.generatesMetadataFilesAtProjectRoot=false".into()));
        assert_eq!(launch.cache_dir, PathBuf::from(format!("/cache/less-rs/jdtls/{name}")));
        assert!(!launch.missing_java);
        assert_eq!(launch.init_options["bundles"].as_array().unwrap().len(), 2);
        let od = jdtls_launch(root, Path::new("/cache/less-rs"), &jdks, Some(&share), true);
        assert_eq!(od.cache_dir, PathBuf::from(format!("/cache/less-rs/jdtls/{name}-ondemand")));
        assert_eq!(od.init_options["settings"]["java"]["import"]["mode"], "ondemand");
    }
}
