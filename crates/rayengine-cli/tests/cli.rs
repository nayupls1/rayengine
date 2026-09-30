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
                .output()
                .unwrap()
        };
        let output = invoke();
        assert!(output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["schema_version"], 1);
        assert_eq!(response["data"]["kind"], kind);
        assert!(project.join("Cargo.toml").is_file());
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
