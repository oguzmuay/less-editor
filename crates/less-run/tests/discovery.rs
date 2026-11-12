use std::fs;
use std::path::{Path, PathBuf};

use less_run::command::{BuildContext, Mode, build, jdwp_options};
use less_run::config::{ConfigStore, RunConfig, format_env, parse_env};
use less_run::model::{BuildTool, Child, ItemKind, PackageManager};
use less_run::providers::Provider;
use less_run::providers::{NodeProvider, SpringProvider, default_providers, discover, ordered_scripts};

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Maven (mvnw) + Gradle (Kotlin) Spring modülleri, iki Node paketi (pnpm / yarn) ve atlanması gereken klasörler.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(&r.join("api/pom.xml"), "<project/>");
    write(&r.join("api/mvnw"), "#!/bin/sh\n");
    write(
        &r.join("api/src/main/java/com/acme/ApiApplication.java"),
        "package com.acme;\n\n@SpringBootApplication\npublic class ApiApplication {}\n",
    );
    write(&r.join("api/target/classes/Copy.java"), "@SpringBootApplication class Copy {}");
    write(&r.join("worker/build.gradle.kts"), "");
    write(&r.join("worker/src/main/kotlin/Worker.kt"), "@SpringBootApplication\nclass Worker\n");
    write(&r.join("pnpm-lock.yaml"), "");
    write(
        &r.join("package.json"),
        r#"{ "name": "root-app", "scripts": { "start": "vite", "build": "vite build", "dev": "vite --host" } }"#,
    );
    write(&r.join("web/admin/yarn.lock"), "");
    write(&r.join("web/admin/package.json"), r#"{ "scripts": { "serve": "node s.js" } }"#);
    write(&r.join("web/empty/package.json"), r#"{ "name": "noscripts" }"#);
    write(&r.join("node_modules/dep/package.json"), r#"{ "scripts": { "x": "y" } }"#);
    write(&r.join(".hidden/package.json"), r#"{ "scripts": { "x": "y" } }"#);
    dir
}

#[test]
fn spring_discovery_finds_apps_and_skips_build_dirs_16() {
    let dir = fixture();
    let g = SpringProvider.discover(dir.path()).unwrap();
    let items = g.items();
    assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["ApiApplication", "Worker"]);
    let api = items[0];
    assert_eq!(api.cwd, dir.path().join("api"));
    assert!(api.id.starts_with("spring:") && api.id.ends_with("ApiApplication.java"));
    assert!(matches!(api.kind, ItemKind::Spring { tool: BuildTool::Maven, .. }));
    assert!(api.can_attach());
    assert!(matches!(items[1].kind, ItemKind::Spring { tool: BuildTool::Gradle, .. }));
    assert!(!items[1].can_attach(), "Kotlin main: port only, no attach (#34)");
    let ItemKind::Spring { file, .. } = &api.kind else { unreachable!() };
    assert_eq!(less_run::providers::main_class(file), "com.acme.ApiApplication");
}

#[test]
fn node_discovery_script_order_and_package_manager_16() {
    let dir = fixture();
    let g = NodeProvider.discover(dir.path()).unwrap();
    let pkgs: Vec<_> = g
        .children
        .iter()
        .map(|c| match c {
            Child::Group(g) => g,
            Child::Item(_) => panic!("packages expected"),
        })
        .collect();
    assert_eq!(pkgs.len(), 2, "no-script package, node_modules and hidden dirs are skipped");
    assert_eq!(pkgs[0].label, "root-app");
    assert_eq!(pkgs[0].detail.as_deref(), Some(". · pnpm"));
    let scripts: Vec<_> = pkgs[0]
        .children
        .iter()
        .map(|c| match c {
            Child::Item(i) => i.label.clone(),
            Child::Group(_) => panic!(),
        })
        .collect();
    assert_eq!(scripts, ["start", "build", "dev"], "file order, not alphabetical");
    assert_eq!(pkgs[1].label, "web/admin");
    assert_eq!(pkgs[1].detail.as_deref(), Some("web/admin · yarn"));
    let Child::Item(serve) = &pkgs[1].children[0] else { panic!() };
    assert_eq!(serve.id, format!("node:{}:serve", dir.path().join("web/admin").display()));
    assert!(matches!(&serve.kind, ItemKind::Node { pm: PackageManager::Yarn, script } if script == "serve"));
}

#[test]
fn ordered_scripts_ignores_values_matching_names_16() {
    let text = r#"{"scripts":{"b":"a","a":"x"}}"#;
    assert_eq!(ordered_scripts(text, &["a".into(), "b".into()]), ["b", "a"]);
}

#[test]
fn multi_root_details_get_root_prefix_26() {
    let a = fixture();
    let b = tempfile::tempdir().unwrap();
    write(&b.path().join("package.json"), r#"{ "name": "other", "scripts": { "t": "x" } }"#);
    let roots: Vec<PathBuf> = vec![a.path().to_path_buf(), b.path().to_path_buf()];
    let groups = discover(&default_providers(), &roots);
    let node = groups.iter().find(|g| g.id == "node").unwrap();
    let base_b = b.path().file_name().unwrap().to_string_lossy().to_string();
    let Child::Group(last) = node.children.last().unwrap() else { panic!() };
    assert_eq!(last.detail.as_deref(), Some(format!("{base_b} › . · npm").as_str()));
    let spring = groups.iter().find(|g| g.id == "spring").unwrap();
    let Child::Item(api) = &spring.children[0] else { panic!() };
    assert!(api.detail.as_deref().unwrap().contains(" › maven · api"));
}

fn ctx<'a>(mode: Mode, port: Option<u16>, init: &'a Path) -> BuildContext<'a> {
    BuildContext { mode, debug_port: port, gradle_init: init, suspend: false, inherited_node_options: None }
}

#[test]
fn spring_command_merges_vm_options_and_jdwp_34_69() {
    let dir = fixture();
    let g = SpringProvider.discover(dir.path()).unwrap();
    let items = g.items();
    let cfg = RunConfig {
        profile: Some("dev".into()),
        env: Some([("A".to_string(), "1".to_string())].into()),
        vm_options: Some("-Xmx1g".into()),
        args: Some("--server.port=8081".into()),
    };
    let init = PathBuf::from("/tmp/jdwp.gradle");
    let maven = build(items[0], &cfg, &ctx(Mode::Debug, Some(5006), &init));
    assert_eq!(
        maven.argv,
        [
            dir.path().join("api/mvnw").display().to_string(),
            "spring-boot:run".into(),
            "-Dspring-boot.run.profiles=dev".into(),
            format!("-Dspring-boot.run.jvmArguments=-Xmx1g -agentlib:jdwp={}", jdwp_options(5006, false)),
            "-Dspring-boot.run.arguments=--server.port=8081".into(),
        ]
    );
    assert_eq!(maven.env.get("A").map(String::as_str), Some("1"));
    assert!(
        jdwp_options(5006, false).contains("address=127.0.0.1:5006") && jdwp_options(1, false).contains("suspend=n")
    );
    let run = build(items[0], &RunConfig::default(), &ctx(Mode::Run, None, &init));
    assert_eq!(run.argv.len(), 2);
    let gradle = build(items[1], &cfg, &ctx(Mode::Debug, Some(5007), &init));
    assert_eq!(
        gradle.argv,
        [
            "gradle".to_string(),
            "bootRun".into(),
            "--args=--spring.profiles.active=dev --server.port=8081".into(),
            "--init-script".into(),
            "/tmp/jdwp.gradle".into(),
            format!("-PrunnerJdwp={}", jdwp_options(5007, false)),
            "-PrunnerJvmArgs=-Xmx1g".into(),
        ]
    );
    let plain = build(items[1], &RunConfig::default(), &ctx(Mode::Run, None, &init));
    assert_eq!(plain.argv, ["gradle", "bootRun"]);
}

#[test]
fn node_command_args_and_node_options_69() {
    let dir = fixture();
    let g = NodeProvider.discover(dir.path()).unwrap();
    let item = g.items()[0].clone();
    let init = PathBuf::from("/x");
    let cfg = RunConfig {
        vm_options: Some("--max-old-space-size=4096".into()),
        args: Some("--port  3001 -v".into()),
        ..Default::default()
    };
    let run = build(&item, &cfg, &ctx(Mode::Run, None, &init));
    assert_eq!(run.argv, ["pnpm", "run", "start", "--", "--port", "3001", "-v"]);
    assert_eq!(run.env["NODE_OPTIONS"], "--max-old-space-size=4096");
    let dbg = build(&item, &cfg, &ctx(Mode::Debug, None, &init));
    assert_eq!(dbg.env["NODE_OPTIONS"], "--max-old-space-size=4096 --inspect=127.0.0.1:0");
    let mut c = ctx(Mode::Debug, None, &init);
    c.inherited_node_options = Some("--enable-source-maps".into());
    let dbg = build(&item, &RunConfig::default(), &c);
    assert_eq!(dbg.env["NODE_OPTIONS"], "--enable-source-maps --inspect=127.0.0.1:0");
    assert_eq!(dbg.argv, ["pnpm", "run", "start"]);
}

#[test]
fn config_store_imports_nvim_file_read_only_69() {
    let dir = tempfile::tempdir().unwrap();
    let nvim = dir.path().join("nvim/run_configs.json");
    write(&nvim, r#"{"node:/a:start":{"args":"--x","env":{"K":"v"}},"bad":5}"#);
    let own = dir.path().join("rs/run_configs.json");
    let mut store = ConfigStore::open(own.clone(), &[dir.path().join("missing.json"), nvim.clone()]);
    assert_eq!(store.get("node:/a:start").args.as_deref(), Some("--x"));
    let saved = store
        .set(
            "spring:/b",
            &RunConfig { profile: Some(" dev ".into()), args: Some("  ".into()), ..Default::default() },
            true,
        )
        .unwrap();
    assert_eq!(saved, RunConfig { profile: Some("dev".into()), ..Default::default() });
    let cleared =
        store.set("spring:/c", &RunConfig { profile: Some("p".into()), ..Default::default() }, false).unwrap();
    assert!(cleared.is_empty(), "profile dropped for items without profiles");
    let reopened = ConfigStore::open(own, &[]);
    assert_eq!(reopened.get("spring:/b").profile.as_deref(), Some("dev"));
    assert_eq!(reopened.get("node:/a:start").env.unwrap()["K"], "v", "imported entries are carried over");
    assert_eq!(fs::read_to_string(nvim).unwrap(), r#"{"node:/a:start":{"args":"--x","env":{"K":"v"}},"bad":5}"#);
}

#[test]
fn env_field_parse_and_format_69() {
    let env = parse_env("A=1; B_2 = x=y ;").unwrap();
    assert_eq!(format_env(Some(&env)), "A=1; B_2=x=y");
    assert!(parse_env("1A=2").unwrap_err().contains("bad: 1A=2"));
    assert!(parse_env("noeq").is_err());
}
