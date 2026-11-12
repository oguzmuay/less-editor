//! Maven paneli UI dışı mantığı: `pom.xml` keşfi, modül adı ve plugin goal'ları, komut kurulumu ve yeni proje iskeleti.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{fs, io};

use regex::Regex;

/// Her modülde gösterilen lifecycle fazları.
pub const LIFECYCLE: [&str; 9] =
    ["clean", "validate", "compile", "test", "package", "verify", "install", "site", "deploy"];

/// Bilinen plugin önekleri ve sık kullanılan goal'ları; pom'daki `<goal>`'larla birleşir.
const KNOWN_GOALS: &[(&str, &[&str])] = &[
    ("spring-boot", &["run", "start", "stop", "repackage", "build-image", "build-info"]),
    ("compiler", &["compile", "testCompile"]),
    ("surefire", &["test"]),
    ("failsafe", &["integration-test", "verify"]),
    ("jar", &["jar", "test-jar"]),
    ("war", &["war", "exploded"]),
    ("source", &["jar"]),
    ("javadoc", &["javadoc", "jar"]),
    ("dependency", &["tree", "analyze", "resolve", "copy-dependencies"]),
    ("exec", &["java", "exec"]),
    ("jib", &["build", "dockerBuild", "buildTar"]),
    ("flyway", &["migrate", "info", "validate", "clean", "repair"]),
    ("liquibase", &["update", "status", "rollback"]),
    ("quarkus", &["dev", "build", "test"]),
    ("javafx", &["run", "jlink"]),
    ("spotless", &["apply", "check"]),
    ("jacoco", &["prepare-agent", "report", "check"]),
    ("shade", &["shade"]),
    ("assembly", &["single"]),
    ("versions", &["display-dependency-updates", "display-plugin-updates", "set"]),
];

/// Pom'da tanımlı olmasa da her projede çalışan araç goal'ları.
pub const TOOLS: [&str; 3] = ["dependency:tree", "help:effective-pom", "versions:display-dependency-updates"];

/// Bir plugin ve gösterilecek goal'ları.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plugin {
    /// Komut satırı öneki (`spring-boot`, `compiler`).
    pub prefix: String,
    pub goals: Vec<String>,
}

/// Bir `pom.xml` modülü.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub dir: PathBuf,
    /// Projenin kendi `artifactId`'si (yoksa klasör adı).
    pub name: String,
    pub plugins: Vec<Plugin>,
}

static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
static PARENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<parent>.*?</parent>").unwrap());
static NOISE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<pluginManagement>.*?</pluginManagement>|<dependencies>.*?</dependencies>|<dependencyManagement>.*?</dependencyManagement>|<profiles>.*?</profiles>|<reporting>.*?</reporting>").unwrap()
});
static ARTIFACT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<artifactId>\s*([^<\s]+)\s*</artifactId>").unwrap());
static PLUGIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<plugin>(.*?)</plugin>").unwrap());
static GOAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<goal>\s*([^<\s]+)\s*</goal>").unwrap());

/// Köklerdeki tüm `pom.xml` modülleri (`target/`, `node_modules/`, gizli klasörler atlanır), yola göre sıralı.
pub fn discover(roots: &[PathBuf]) -> Vec<Module> {
    let mut out: Vec<Module> = roots
        .iter()
        .flat_map(|root| {
            less_run::providers::find_files(root, &["target", "node_modules", "build"], &|p| {
                p.file_name().is_some_and(|n| n == "pom.xml")
            })
        })
        .filter_map(|pom| {
            let dir = pom.parent()?.to_path_buf();
            let text = fs::read_to_string(&pom).ok()?;
            let fallback = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let (name, plugins) = parse(&text);
            Some(Module { dir, name: name.unwrap_or(fallback), plugins })
        })
        .collect();
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    out
}

/// Pom metninden projenin `artifactId`'si ve `<build><plugins>` altındaki plugin'ler (pluginManagement, profiller ve
/// bağımlılıklar sayılmaz). Goal'ı bilinmeyen ve pom'da da goal yazmayan plugin atlanır.
pub fn parse(text: &str) -> (Option<String>, Vec<Plugin>) {
    let text = COMMENT.replace_all(text, "");
    let text = PARENT.replace_all(&text, "");
    let text = NOISE.replace_all(&text, "");
    let mut blocks = PLUGIN.captures_iter(&text).map(|c| c.get(0).map_or(0..0, |m| m.range())).peekable();
    let first_plugin = blocks.peek().map_or(text.len(), |r| r.start);
    let name = ARTIFACT.captures(&text[..first_plugin]).map(|c| c[1].to_string());
    let mut plugins: Vec<Plugin> = Vec::new();
    for range in blocks {
        let block = &text[range];
        let Some(artifact) = ARTIFACT.captures(block).map(|c| c[1].to_string()) else { continue };
        let prefix = prefix(&artifact);
        let mut goals: Vec<String> = KNOWN_GOALS
            .iter()
            .find(|(p, _)| *p == prefix)
            .map(|(_, g)| g.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        for goal in GOAL.captures_iter(block).map(|c| c[1].to_string()) {
            if !goals.contains(&goal) {
                goals.push(goal);
            }
        }
        if goals.is_empty() || plugins.iter().any(|p| p.prefix == prefix) {
            continue;
        }
        plugins.push(Plugin { prefix, goals });
    }
    (name, plugins)
}

/// Plugin artifactId'sinden komut öneki: `maven-x-plugin` → `x`, `x-maven-plugin` → `x`.
pub fn prefix(artifact: &str) -> String {
    let a = artifact.strip_prefix("maven-").and_then(|s| s.strip_suffix("-plugin"));
    a.or_else(|| artifact.strip_suffix("-maven-plugin")).unwrap_or(artifact).to_string()
}

/// `mvn` komutu: modülden yukarı doğru `mvnw` varsa o, yoksa PATH'teki `mvn`. Goal boşlukla bölünür
/// (`clean install`).
pub fn command(dir: &Path, goal: &str) -> less_run::CommandSpec {
    let mvn = less_run::providers::find_upward(dir, "mvnw")
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "mvn".to_string());
    let mut argv = vec![mvn];
    argv.extend(goal.split_whitespace().map(str::to_string));
    less_run::CommandSpec { argv, cwd: dir.to_path_buf(), env: Default::default() }
}

/// `grup:artifact` (ya da yalnız `artifact`, grup `com.example`) çözümü; geçersizse açıklayan hata.
pub fn coordinates(spec: &str) -> Result<(String, String), String> {
    let spec = spec.trim();
    let (group, artifact) = spec.split_once(':').unwrap_or(("com.example", spec));
    let java_ident = |s: &str| {
        s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if !group.split('.').all(java_ident) {
        return Err(format!("Invalid groupId '{group}' (use a Java package like com.example)"));
    }
    if artifact.is_empty() || !artifact.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
        return Err(format!("Invalid artifactId '{artifact}' (letters, digits, - _ .)"));
    }
    Ok((group.to_string(), artifact.to_string()))
}

/// Yeni Maven projesi: `root` boşsa (gizli dosyalar hariç) köke, değilse `root/<artifactId>/` altına `pom.xml`,
/// `App.java` ve `AppTest.java` yazar. Var olan klasörün üstüne yazmaz. Oluşan `pom.xml`'in yolunu döndürür.
pub fn create_project(root: &Path, group: &str, artifact: &str) -> io::Result<PathBuf> {
    let empty = fs::read_dir(root)?.flatten().all(|e| e.file_name().to_string_lossy().starts_with('.'));
    let dir = if empty { root.to_path_buf() } else { root.join(artifact) };
    if dir.join("pom.xml").exists() || (!empty && dir.exists()) {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} already exists", dir.display())));
    }
    let pkg = group.replace('.', "/");
    let main = dir.join("src/main/java").join(&pkg);
    let test = dir.join("src/test/java").join(&pkg);
    fs::create_dir_all(&main)?;
    fs::create_dir_all(&test)?;
    fs::write(dir.join("pom.xml"), pom_xml(group, artifact))?;
    fs::write(
        main.join("App.java"),
        format!(
            "package {group};\n\npublic class App {{\n    public static void main(String[] args) {{\n        System.out.println(\"Hello from {artifact}\");\n    }}\n}}\n"
        ),
    )?;
    fs::write(
        test.join("AppTest.java"),
        format!(
            "package {group};\n\nimport static org.junit.jupiter.api.Assertions.assertTrue;\n\nimport org.junit.jupiter.api.Test;\n\nclass AppTest {{\n    @Test\n    void runs() {{\n        assertTrue(true);\n    }}\n}}\n"
        ),
    )?;
    Ok(dir.join("pom.xml"))
}

fn pom_xml(group: &str, artifact: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>{group}</groupId>
    <artifactId>{artifact}</artifactId>
    <version>1.0-SNAPSHOT</version>
    <packaging>jar</packaging>

    <properties>
        <maven.compiler.release>21</maven.compiler.release>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    </properties>

    <dependencies>
        <dependency>
            <groupId>org.junit.jupiter</groupId>
            <artifactId>junit-jupiter</artifactId>
            <version>5.11.4</version>
            <scope>test</scope>
        </dependency>
    </dependencies>

    <build>
        <plugins>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-surefire-plugin</artifactId>
                <version>3.5.2</version>
            </plugin>
            <plugin>
                <groupId>org.codehaus.mojo</groupId>
                <artifactId>exec-maven-plugin</artifactId>
                <version>3.5.0</version>
                <configuration>
                    <mainClass>{group}.App</mainClass>
                </configuration>
            </plugin>
        </plugins>
    </build>
</project>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const POM: &str = r#"<project>
  <parent><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-parent</artifactId></parent>
  <!-- <artifactId>commented</artifactId> -->
  <artifactId>api</artifactId>
  <dependencies><dependency><artifactId>lombok</artifactId></dependency></dependencies>
  <build>
    <pluginManagement><plugins><plugin><artifactId>maven-war-plugin</artifactId></plugin></plugins></pluginManagement>
    <plugins>
      <plugin><groupId>org.springframework.boot</groupId><artifactId>spring-boot-maven-plugin</artifactId></plugin>
      <plugin>
        <artifactId>openapi-generator-maven-plugin</artifactId>
        <executions><execution><goals><goal>generate</goal></goals></execution></executions>
      </plugin>
      <plugin><artifactId>some-unknown-plugin</artifactId></plugin>
    </plugins>
  </build>
</project>"#;

    #[test]
    fn parses_artifact_and_active_plugins() {
        let (name, plugins) = parse(POM);
        assert_eq!(name.as_deref(), Some("api"));
        let prefixes: Vec<_> = plugins.iter().map(|p| p.prefix.as_str()).collect();
        assert_eq!(prefixes, ["spring-boot", "openapi-generator"]);
        assert!(plugins[0].goals.contains(&"run".to_string()));
        assert_eq!(plugins[1].goals, ["generate"]);
    }

    #[test]
    fn prefix_follows_maven_naming() {
        assert_eq!(prefix("maven-surefire-plugin"), "surefire");
        assert_eq!(prefix("jib-maven-plugin"), "jib");
        assert_eq!(prefix("odd"), "odd");
    }

    #[test]
    fn coordinates_validate_and_default_group() {
        assert_eq!(coordinates("demo").unwrap(), ("com.example".into(), "demo".into()));
        assert_eq!(coordinates("tr.com.iknow:my-app").unwrap(), ("tr.com.iknow".into(), "my-app".into()));
        assert!(coordinates("1bad:x").is_err());
        assert!(coordinates("a.b:").is_err());
        assert!(coordinates("a:b/../c").is_err());
    }

    #[test]
    fn creates_project_in_empty_root_or_subfolder_and_discovers_it() {
        let t = tempfile::tempdir().unwrap();
        let pom = create_project(t.path(), "com.acme", "demo").unwrap();
        assert_eq!(pom, t.path().join("pom.xml"));
        assert!(t.path().join("src/main/java/com/acme/App.java").is_file());
        let sub = create_project(t.path(), "com.acme", "lib").unwrap();
        assert_eq!(sub, t.path().join("lib/pom.xml"));
        assert!(create_project(t.path(), "com.acme", "lib").is_err());
        let found = discover(&[t.path().to_path_buf()]);
        let names: Vec<_> = found.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["demo", "lib"]);
        assert_eq!(found[0].plugins.iter().map(|p| p.prefix.as_str()).collect::<Vec<_>>(), ["surefire", "exec"]);
        let cmd = command(&found[0].dir, "clean install");
        assert_eq!(cmd.argv, ["mvn", "clean", "install"]);
    }
}
