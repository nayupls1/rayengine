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
