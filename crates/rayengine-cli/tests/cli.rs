//! Process-level checks for JSON responses and preserved compiler errors.

use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("rayengine-{name}-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn argument_errors_are_json_when_requested() {
    let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
        .args(["--json", "new"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "invalid_arguments");
    assert!(output.stderr.is_empty());
}

#[test]
fn new_reports_files_and_preserves_existing_destination() {
    let scratch = Scratch::new("cli-new");
    for kind in ["2d", "3d"] {
        let project = scratch.0.join(format!("game-{kind}"));
        let invoke = || {
            Command::new(env!("CARGO_BIN_EXE_rayengine"))
                .args(["--json", "new"])
                .arg(&project)
                .args(["--kind", kind])
                .current_dir(&scratch.0)
                .output()
                .unwrap()
        };
        let output = invoke();
        assert!(output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["schema_version"], 1);
        assert_eq!(response["data"]["kind"], kind);
        assert!(response["data"]["sdk_path"].is_null());
        let manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
        assert!(manifest.contains(&format!("rayengine = \"{}\"", env!("CARGO_PKG_VERSION"))));
        assert!(!manifest.contains("path ="));
        assert!(project.join("Cargo.toml").is_file());
        let loaded =
            rayengine_core::manifest::ProjectManifest::load(project.join("rayengine.toml"))
                .unwrap();
        let base = loaded.resolve(None).unwrap();
        assert_eq!(
            base.settings.project.executable.as_deref(),
            Some(format!("game-{kind}").as_str())
        );
        assert!(base.discover_assets().unwrap().is_empty());
        assert!(!loaded.resolve(Some("dev")).unwrap().settings.render.vsync);
        assert_eq!(
            loaded
                .resolve(Some("pixel"))
                .unwrap()
                .settings
                .render
                .scale_mode,
            rayengine_core::manifest::ScaleMode::IntegerFit
        );
        assert!(
            response["data"]["files"]
                .as_array()
                .unwrap()
                .contains(&Value::from("rayengine.toml"))
        );

        let previous = fs::read(project.join("src/main.rs")).unwrap();
        let output = invoke();
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(fs::read(project.join("src/main.rs")).unwrap(), previous);
    }
}

#[test]
fn check_retains_structured_compiler_diagnostics_and_exit_status() {
    let scratch = Scratch::new("cli-check");
    fs::create_dir(scratch.0.join("src")).unwrap();
    fs::write(scratch.0.join("Cargo.toml"), "[package]\nname = \"invalid-game\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").unwrap();
    fs::write(
        scratch.0.join("src/main.rs"),
        "fn main() { let value: u32 = \"wrong type\"; }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
        .args(["--json", "check"])
        .arg(&scratch.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["ok"], false);
    let diagnostics = response["error"]["details"]["diagnostics"]
        .as_array()
        .unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["reason"] == "compiler-message" && d["message"]["level"] == "error")
    );
    assert!(
        output.stderr.is_empty(),
        "JSON CLI must not leak child diagnostics outside its result"
    );
}

#[test]
fn new_plugin_reports_library_and_rejects_existing_paths_and_invalid_sdk() {
    let scratch = Scratch::new("cli-plugin");
    let project = scratch.0.join("plugins/my-plugin");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_rayengine"))
            .args(["--json", "new-plugin"])
            .arg(&project)
            .args(["--name", "my-plugin"])
            .output()
            .unwrap()
    };
    let output = invoke();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["command"], "new-plugin");
    assert_eq!(response["data"]["kind"], "plugin");
    assert!(response["data"]["sdk_path"].is_null());
    let manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(manifest.contains(&format!("rayengine = \"{}\"", env!("CARGO_PKG_VERSION"))));
    assert!(
        response["data"]["files"]
            .as_array()
            .unwrap()
            .contains(&Value::from("src/lib.rs"))
    );
    assert!(!project.join("src/main.rs").exists());
    let source = fs::read(project.join("src/lib.rs")).unwrap();
    let output = invoke();
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        response["error"]["code"],
        "destination_exists_or_unwritable"
    );
    assert_eq!(fs::read(project.join("src/lib.rs")).unwrap(), source);

    let rejected = scratch.0.join("invalid-plugin");
    let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
        .args(["--json", "new-plugin"])
        .arg(&rejected)
        .arg("--sdk-path")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!output.status.success());
    assert_eq!(response["error"]["code"], "invalid_sdk");
    assert!(
        !rejected.exists(),
        "SDK validation precedes filesystem writes"
    );
}

#[test]
fn explicit_sdk_path_is_validated_and_preserved() {
    let scratch = Scratch::new("cli-local-sdk");
    let sdk = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("rayengine")
        .canonicalize()
        .unwrap();
    let project = scratch.0.join("local-game");
    let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
        .args(["--json", "new"])
        .arg(&project)
        .arg("--sdk-path")
        .arg(&sdk)
        .output()
        .unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["sdk_path"], sdk.to_str().unwrap());
    let manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(manifest.contains(&format!(
        "rayengine = {{ path = {} }}",
        serde_json::to_string(sdk.to_str().unwrap()).unwrap()
    )));

    let rejected = scratch.0.join("missing-sdk-game");
    let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
        .args(["--json", "new"])
        .arg(&rejected)
        .arg("--sdk-path")
        .arg(scratch.0.join("missing"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["code"], "missing_sdk");
    assert!(!rejected.exists());
}

#[test]
fn manifest_inspection_check_and_run_agree_from_another_directory() {
    let scratch = Scratch::new("cli-manifest");
    let project = scratch.0.join("project");
    fs::create_dir_all(project.join("src/bin")).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = 'manifest-test'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .unwrap();
    // A display-independent executable reports what CLI run supplies to runtimes.
    fs::write(project.join("src/bin/selected.rs"), r#"fn main() {
        println!("{}|{}|{}", std::env::var("RAYENGINE_MANIFEST").unwrap(), std::env::var("RAYENGINE_PROFILE").unwrap(), std::env::args().nth(1).unwrap());
    }"#).unwrap();
    fs::write(
        project.join("src/bin/override.rs"),
        "fn main() { println!(\"OVERRIDE\"); }",
    )
    .unwrap();
    fs::write(project.join("rayengine.toml"), "schema_version = 1\n[project]\nexecutable = 'selected'\n[assets]\nroots = ['assets']\n[fonts.ui]\npath = 'fonts/ui.ttf'\n[profiles.dev.window]\nsize = [800, 600]\n").unwrap();
    let invoke = |action: &str, path: &std::path::Path, extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
            .args(["--json", action])
            .arg(path)
            .args(extra)
            .current_dir(&scratch.0)
            .output()
            .unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.status.success(), "{response}");
        assert!(output.stderr.is_empty());
        response["data"].clone()
    };
    let info = invoke("info", &project, &["--profile", "dev"]);
    let via_file = invoke(
        "info",
        &project.join("rayengine.toml"),
        &["--profile", "dev"],
    );
    let via_cargo = invoke("info", &project.join("Cargo.toml"), &["--profile", "dev"]);
    let loaded = rayengine_core::manifest::ProjectManifest::load(project.join("rayengine.toml"))
        .unwrap()
        .resolve(Some("dev"))
        .unwrap();
    let expected = serde_json::to_value(loaded).unwrap();
    assert_eq!(info["project_manifest"], expected);
    assert_eq!(via_file["project_manifest"], expected);
    assert_eq!(via_cargo["project_manifest"], expected);
    #[cfg(unix)]
    {
        let alias = scratch.0.join("alias");
        fs::create_dir(&alias).unwrap();
        std::os::unix::fs::symlink(project.join("Cargo.toml"), alias.join("Cargo.toml")).unwrap();
        std::os::unix::fs::symlink(project.join("rayengine.toml"), alias.join("rayengine.toml"))
            .unwrap();
        let via_alias = invoke("info", &alias.join("Cargo.toml"), &["--profile", "dev"]);
        let runtime =
            rayengine_core::manifest::ProjectManifest::load_optional(alias.join("Cargo.toml"))
                .unwrap()
                .unwrap()
                .resolve(Some("dev"))
                .unwrap();
        assert_eq!(
            via_alias["project_manifest"],
            serde_json::to_value(runtime).unwrap()
        );
        assert_eq!(
            invoke("info", &alias.join("rayengine.toml"), &["--profile", "dev"])["project_manifest"],
            expected
        );
    }

    assert_eq!(
        info["project_manifest"]["settings"]["assets"]["roots"][0],
        project.join("assets").to_str().unwrap()
    );
    assert_eq!(
        info["project_manifest"]["settings"]["fonts"]["ui"]["path"],
        project.join("fonts/ui.ttf").to_str().unwrap()
    );
    assert_eq!(info["profiles"], serde_json::json!(["dev"]));
    let check = invoke("check", &project, &["--profile", "dev"]);
    assert_eq!(check["project_manifest"], expected);
    let run = invoke("run", &project, &["--profile", "dev", "--", "argument"]);
    assert_eq!(run["project_manifest"], expected);
    assert_eq!(
        run["stdout"],
        format!("{}|dev|argument", project.join("rayengine.toml").display())
    );
    let run = invoke("run", &project, &["--profile", "dev", "--bin", "override"]);
    assert_eq!(run["stdout"], "OVERRIDE");
}

#[test]
fn invalid_manifests_fail_before_cargo_and_missing_optional_manifests_work() {
    let scratch = Scratch::new("cli-manifest-errors");
    fs::create_dir(scratch.0.join("src")).unwrap();
    fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname = 'valid-game'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .unwrap();
    fs::write(scratch.0.join("src/main.rs"), "fn main() {}\n").unwrap();
    let invoke = |action: &str, path: &std::path::Path, extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
            .args(["--json", action])
            .arg(path)
            .args(extra)
            .output()
            .unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.stderr.is_empty());
        (output.status.success(), response)
    };
    let (ok, response) = invoke("info", &scratch.0, &[]);
    assert!(ok);
    assert!(response["data"]["project_manifest"].is_null());
    assert!(invoke("check", &scratch.0, &[]).0);
    assert!(!invoke("info", &scratch.0, &["--profile", "dev"]).0);
    assert!(!invoke("info", &scratch.0.join("rayengine.toml"), &[]).0);
    for source in [
        "schema_version = 999",
        "schema_version = 1\n[render]\nunknown_setting = true",
        "schema_version = 1\n[profiles.unselected.window]\nsize = [0, 1]",
    ] {
        fs::write(scratch.0.join("rayengine.toml"), source).unwrap();
        for action in ["info", "check", "build", "run"] {
            let (ok, response) = invoke(action, &scratch.0, &[]);
            assert!(!ok, "{action}: {response}");
            assert_eq!(response["error"]["code"], "invalid_project_manifest");
            assert!(
                response["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("rayengine.toml")
            );
            assert!(response["error"]["details"].is_null());
        }
    }
}

fn cli(arguments: &[&str], path: Option<&std::path::Path>) -> (bool, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rayengine"));
    command.arg("--json").args(arguments);
    if let Some(path) = path {
        command.arg(path);
    }
    let output = command.output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stdout)));
    assert_eq!(response["schema_version"], 1);
    assert_eq!(response["ok"], output.status.success());
    (output.status.success(), response)
}

fn cpu_game(path: &std::path::Path, source: &str) {
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = 'lifecycle-game'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .unwrap();
    fs::write(path.join("src/main.rs"), source).unwrap();
}

#[test]
fn templates_list_and_generate_every_starter_and_report_bad_arguments() {
    let scratch = Scratch::new("all-templates");
    let (ok, response) = cli(&["templates"], None);
    assert!(ok);
    let templates = response["data"]["templates"].as_array().unwrap();
    assert_eq!(templates.len(), 4);
    let sdk = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("rayengine");
    for template in templates {
        let name = template["name"].as_str().unwrap();
        let project = scratch.0.join(format!("game-{name}"));
        let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
            .args(["--json", "new"])
            .arg(&project)
            .args(["--template", name, "--sdk-path"])
            .arg(&sdk)
            .output()
            .unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.status.success(), "{response}");
        assert_eq!(response["data"]["kind"], name);
        if ["topdown", "platformer"].contains(&name) {
            assert!(
                fs::read_to_string(project.join("Cargo.toml"))
                    .unwrap()
                    .contains("rayengine-tilemap")
            );
            assert!(project.join("assets/level.toml").is_file());
        }
    }
    for args in [
        vec!["templates", "extra"],
        vec!["new", "game", "--template", "nope"],
        vec!["new", "game", "--template", "topdown", "--kind", "3d"],
    ] {
        assert_eq!(cli(&args, None).1["error"]["code"], "invalid_arguments");
    }
}

#[test]
fn plugin_edits_preserve_comments_config_and_match_sources() {
    let scratch = Scratch::new("plugin-edit");
    let game = scratch.0.join("game");
    let sdk = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("rayengine");
    let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
        .args(["--json", "new"])
        .arg(&game)
        .arg("--sdk-path")
        .arg(&sdk)
        .output()
        .unwrap();
    assert!(output.status.success());
    let config = game.join("rayengine.toml");
    fs::write(&config, format!("{}\n# keep my settings\n[plugins.tilemap]\nlevel = 'custom.toml'\n[profiles.dev.plugins.tilemap]\nspeed = 12\n[plugins.other]\nvalue = 42\n", fs::read_to_string(&config).unwrap())).unwrap();
    for name in ["particles", "voxel", "beacons", "tilemap"] {
        let (ok, result) = cli(&["add", name], Some(&game));
        assert!(ok, "{result}");
        assert_eq!(result["data"]["plugin"], name);
        assert!(result["data"]["plugin_path"].is_string());
        assert_eq!(
            cli(&["add", name], Some(&game)).1["error"]["code"],
            "plugin_already_added"
        );
    }
    let source = fs::read_to_string(&config).unwrap();
    assert!(source.contains("# keep my settings") && source.contains("custom.toml"));
    let (ok, result) = cli(&["remove", "rayengine-tilemap"], Some(&game));
    assert!(ok, "{result}");
    let loaded = rayengine_core::manifest::ProjectManifest::load(&config).unwrap();
    assert!(
        !loaded
            .resolve(Some("dev"))
            .unwrap()
            .settings
            .plugins
            .contains_key("tilemap")
    );
    assert!(
        loaded
            .resolve(None)
            .unwrap()
            .settings
            .plugins
            .contains_key("other")
    );
    assert_eq!(
        cli(&["remove", "tilemap"], Some(&game)).1["error"]["code"],
        "plugin_not_added"
    );
    assert_eq!(
        cli(&["add", "unknown"], Some(&game)).1["error"]["code"],
        "unknown_plugin"
    );
    assert_eq!(
        cli(&["remove", "unknown"], Some(&game)).1["error"]["code"],
        "unknown_plugin"
    );

    let registry = scratch.0.join("registry");
    assert!(cli(&["new"], Some(&registry)).0);
    assert!(cli(&["add", "voxel"], Some(&registry)).0);
    assert!(
        fs::read_to_string(registry.join("Cargo.toml"))
            .unwrap()
            .contains(&format!("version = \"={}\"", env!("CARGO_PKG_VERSION")))
    );
}

#[test]
fn bundle_relocates_assets_fonts_profiles_and_clean_preserves_unrelated_files() {
    let scratch = Scratch::new("bundle");
    let game = scratch.0.join("game");
    cpu_game(
        &game,
        r#"fn main() {
        let manifest = std::env::var("RAYENGINE_MANIFEST").unwrap();
        let root = std::path::Path::new(&manifest).parent().unwrap();
        assert_eq!(std::fs::read_to_string(root.join("assets/level.txt")).unwrap(), "selected");
        assert!(std::env::var("RAYENGINE_PROFILE").is_err());
        println!("{}", std::env::args().nth(1).unwrap());
    }"#,
    );
    fs::create_dir(game.join("assets")).unwrap();
    fs::create_dir(game.join("release-assets")).unwrap();
    fs::write(game.join("assets/level.txt"), "base").unwrap();
    fs::write(game.join("release-assets/level.txt"), "selected").unwrap();
    fs::write(game.join("release-assets/secret.bak"), "excluded").unwrap();
    fs::write(game.join("font.ttf"), "font fixture").unwrap();
    fs::write(game.join("LICENSE"), "game license").unwrap();
    fs::write(game.join("rayengine.toml"), "schema_version = 1\n[project]\nexecutable = 'lifecycle-game'\n[assets]\nroots = ['assets']\nexclude = ['*.bak']\n[fonts.ui]\npath = 'font.ttf'\n[profiles.ship.assets]\nroots = ['release-assets']\n[profiles.ship.render]\nvsync = false\n").unwrap();
    let (ok, response) = cli(&["bundle", "--profile", "ship"], Some(&game));
    assert!(ok, "{response}");
    assert_eq!(response["command"], "package");
    let folder = PathBuf::from(response["data"]["folder"].as_str().unwrap());
    let archive = PathBuf::from(response["data"]["archive"].as_str().unwrap());
    assert!(!folder.join("assets/secret.bak").exists());
    assert_eq!(
        fs::read_to_string(folder.join("LICENSE")).unwrap(),
        "game license"
    );
    let bundle = rayengine_core::manifest::ProjectManifest::load(folder.join("rayengine.toml"))
        .unwrap()
        .resolve(None)
        .unwrap();
    assert_eq!(
        bundle.asset("level.txt").unwrap(),
        folder.join("assets/level.txt")
    );
    assert_eq!(
        fs::read_to_string(&bundle.settings.fonts["ui"].path).unwrap(),
        "font fixture"
    );
    assert!(!bundle.settings.render.vsync);
    let extract = scratch.0.join("extracted");
    fs::create_dir(&extract).unwrap();
    assert!(
        Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&extract)
            .status()
            .unwrap()
            .success()
    );
    let launch = extract.join(folder.file_name().unwrap()).join("launch");
    let output = Command::new(launch)
        .arg("argument")
        .current_dir(&scratch.0)
        .env("RAYENGINE_PROFILE", "unrelated")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "argument");
    assert_eq!(
        cli(&["package", "--profile", "ship"], Some(&game)).1["error"]["code"],
        "bundle_exists"
    );
    fs::write(game.join("bundles/keep.txt"), "user data").unwrap();
    let (ok, result) = cli(&["clean"], Some(&game));
    assert!(ok, "{result}");
    assert!(!archive.exists() && !folder.exists());
    assert!(!game.join("target").exists());
    assert_eq!(
        fs::read_to_string(game.join("bundles/keep.txt")).unwrap(),
        "user data"
    );
    fs::write(game.join("src/main.rs"), "broken rust!").unwrap();
    let (ok, result) = cli(&["package"], Some(&game));
    assert!(!ok);
    assert_eq!(result["error"]["code"], "cargo_failed");
    assert_eq!(fs::read_dir(game.join("bundles")).unwrap().count(), 1);
    assert_eq!(
        cli(&["clean"], Some(&scratch.0.join("missing"))).1["error"]["code"],
        "missing_manifest"
    );
}

#[test]
fn watch_debounces_restarts_and_emits_one_completion_result() {
    let scratch = Scratch::new("watch");
    cpu_game(
        &scratch.0,
        r#"fn main() {
        std::fs::create_dir_all("artifacts").unwrap();
        std::fs::write("artifacts/started", "running").unwrap();
        println!("started");
        loop { std::thread::sleep(std::time::Duration::from_millis(50)); }
    }"#,
    );
    let child = Command::new(env!("CARGO_BIN_EXE_rayengine"))
        .args(["--json", "watch"])
        .arg(&scratch.0)
        .args([
            "--cycles",
            "2",
            "--debounce-ms",
            "300",
            "--timeout-ms",
            "20000",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    while !scratch.0.join("artifacts/started").exists() {
        assert!(started.elapsed().as_secs() < 15, "watch did not launch");
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // Two writes inside a debounce window must produce one restart.
    let source = scratch.0.join("src/main.rs");
    let original = fs::read_to_string(&source).unwrap();
    fs::write(&source, format!("{original}\n// first edit\n")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));
    fs::write(&source, format!("{original}\n// latest edit\n")).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.stderr.is_empty());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{response}");
    assert_eq!(response["data"]["cycle_count"], 2);
    assert_eq!(response["data"]["stopped"], "cycle_limit");
    assert_eq!(
        response["data"]["cycles"][0]["game_output"]["stdout"],
        "started\n"
    );
    assert_eq!(response["data"]["cycles"][1]["ok"], true);
    fs::write(&source, "bad code!").unwrap();
    let (ok, result) = cli(
        &["watch", "--cycles", "1", "--timeout-ms", "5000"],
        Some(&scratch.0),
    );
    assert!(!ok);
    assert_eq!(result["error"]["code"], "watch_failed");
    assert_eq!(
        result["error"]["details"]["cycles"][0]["error"]["code"],
        "cargo_failed"
    );
}

#[test]
fn doctor_hints_are_structured_on_success_and_failure_without_execution() {
    let scratch = Scratch::new("doctor-hints");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["rustc", "cargo", "cmake", "clang", "pkg-config"] {
            let path = scratch.0.join(name);
            fs::write(&path, "#!/bin/sh\necho fixture\n").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
            .args(["--json", "doctor", "--fix-hints"])
            .env("PATH", &scratch.0)
            .output()
            .unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.status.success());
        assert!(!response["data"]["fix_hints"].as_array().unwrap().is_empty());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_rayengine"))
        .args(["--json", "doctor", "--fix-hints"])
        .env("PATH", "")
        .output()
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(response["error"]["code"], "missing_prerequisites");
    assert!(
        !response["error"]["details"]["fix_hints"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
