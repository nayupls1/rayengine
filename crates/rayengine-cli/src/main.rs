//! Project scaffolding and Cargo tooling with structured agent-readable results.

use clap::{Parser, Subcommand, ValueEnum};
use rayengine_core::manifest::{ProjectManifest, ResolvedManifest};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

#[derive(Parser)]
#[command(version, about = "Small Rust game projects over raylib")]
struct Cli {
    /// Emit one versioned JSON result; Cargo diagnostics are embedded in it.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Create a playable starter in a new directory.
    New {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "2d")]
        kind: Kind,
        /// Override the package name (default: directory name).
        #[arg(long)]
        name: Option<String>,
        /// Use a local SDK crate instead of the published SDK version.
        #[arg(long)]
        sdk_path: Option<PathBuf>,
    },
    /// Create an optional Cargo plugin library in a new directory.
    NewPlugin {
        path: PathBuf,
        /// Override the package name (default: directory name).
        #[arg(long)]
        name: Option<String>,
        /// Use a local SDK crate instead of the published SDK version.
        #[arg(long)]
        sdk_path: Option<PathBuf>,
    },
    /// Inspect project packages without compiling.
    Info {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Select a named rayengine.toml profile.
        #[arg(long)]
        profile: Option<String>,
    },
    /// Type-check a game project.
    Check {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Select a named rayengine.toml profile.
        #[arg(long)]
        profile: Option<String>,
    },
    /// Build a game project.
    Build {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        release: bool,
        /// Select a named rayengine.toml profile (independent of Cargo --release).
        #[arg(long)]
        profile: Option<String>,
    },
    /// Run a game; remaining arguments are passed to its binary.
    Run {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        release: bool,
        /// Select a named rayengine.toml profile (independent of Cargo --release).
        #[arg(long)]
        profile: Option<String>,
        /// Override project.executable with a Cargo binary target.
        #[arg(long)]
        bin: Option<String>,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Check Linux native build prerequisites without modifying the system.
    Doctor,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    #[value(name = "2d")]
    TwoD,
    #[value(name = "3d")]
    ThreeD,
}

struct Failure {
    code: &'static str,
    message: String,
    details: Value,
}
impl Failure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Value::Null,
        }
    }
}
type Result<T> = std::result::Result<T, Failure>;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    let json_requested = args
        .iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == "--json");
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::SUCCESS;
            }
            if json_requested {
                println!(
                    "{}",
                    json!({ "schema_version": 1, "ok": false, "command": null,
                    "error": { "code": "invalid_arguments", "message": error.to_string(), "details": null } })
                );
            } else {
                let _ = error.print();
            }
            return ExitCode::from(2);
        }
    };
    let command = match &cli.command {
        Action::New { .. } => "new",
        Action::NewPlugin { .. } => "new-plugin",
        Action::Info { .. } => "info",
        Action::Check { .. } => "check",
        Action::Build { .. } => "build",
        Action::Run { .. } => "run",
        Action::Doctor => "doctor",
    };
    match execute(cli.command, cli.json) {
        Ok(data) => {
            if cli.json {
                println!(
                    "{}",
                    json!({ "schema_version": 1, "ok": true, "command": command, "data": data })
                );
            } else {
                print_success(command, &data);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if cli.json {
                println!(
                    "{}",
                    json!({ "schema_version": 1, "ok": false, "command": command,
                "error": { "code": error.code, "message": error.message, "details": error.details } })
                );
            } else {
                eprintln!("rayengine: {}", error.message);
                if let Some(stderr) = error.details.get("stderr").and_then(Value::as_str) {
                    eprint!("{stderr}");
                }
            }
            ExitCode::FAILURE
        }
    }
}

fn execute(action: Action, json_mode: bool) -> Result<Value> {
    match action {
        Action::New {
            path,
            kind,
            name,
            sdk_path,
        } => create_project(&path, kind, name, sdk_path),
        Action::NewPlugin {
            path,
            name,
            sdk_path,
        } => create_package(&path, Template::Plugin, name, sdk_path),
        Action::Info { path, profile } => {
            let manifest = manifest(&path)?;
            let (project, profiles) = project_manifest(&path, profile.as_deref())?;
            let output = Command::new("cargo")
                .args([
                    "metadata",
                    "--no-deps",
                    "--format-version",
                    "1",
                    "--manifest-path",
                ])
                .arg(&manifest)
                .output()
                .map_err(process_error)?;
            if !output.status.success() {
                return Err(cargo_failure(output, "metadata failed"));
            }
            let metadata: Value = serde_json::from_slice(&output.stdout)
                .map_err(|e| Failure::new("invalid_metadata", e.to_string()))?;
            Ok(
                json!({ "engine_version": env!("CARGO_PKG_VERSION"), "manifest": manifest,
                "workspace_root": metadata["workspace_root"], "packages": metadata["packages"],
                "project_manifest": project, "profiles": profiles }),
            )
        }
        Action::Check { path, profile } => cargo_task(
            "check",
            &path,
            false,
            profile.as_deref(),
            None,
            &[],
            json_mode,
        ),
        Action::Build {
            path,
            release,
            profile,
        } => cargo_task(
            "build",
            &path,
            release,
            profile.as_deref(),
            None,
            &[],
            json_mode,
        ),
        Action::Run {
            path,
            release,
            profile,
            bin,
            args,
        } => cargo_task(
            "run",
            &path,
            release,
            profile.as_deref(),
            bin.as_deref(),
            &args,
            json_mode,
        ),
        Action::Doctor => doctor(),
    }
}

fn manifest(path: &Path) -> Result<PathBuf> {
    let manifest = if path
        .file_name()
        .is_some_and(|name| name == "rayengine.toml")
    {
        path.canonicalize().map_err(|e| {
            Failure::new(
                "invalid_project_manifest",
                format!("{}: {e}", path.display()),
            )
        })?;
        path.with_file_name("Cargo.toml")
    } else if path.is_file() {
        path.to_path_buf()
    } else {
        path.join("Cargo.toml")
    };
    manifest
        .canonicalize()
        .map_err(|e| Failure::new("missing_manifest", format!("{}: {e}", manifest.display())))
}

fn project_manifest(
    path: &Path,
    profile: Option<&str>,
) -> Result<(Option<ResolvedManifest>, Vec<String>)> {
    let manifest = ProjectManifest::load_optional(path)
        .map_err(|e| Failure::new("invalid_project_manifest", e.to_string()))?;
    match manifest {
        Some(manifest) => {
            let profiles = manifest.profiles().map(str::to_owned).collect();
            let project = manifest
                .resolve(profile)
                .map_err(|e| Failure::new("invalid_project_manifest", e.to_string()))?;
            Ok((Some(project), profiles))
        }
        None if profile.is_some() => Err(Failure::new(
            "invalid_project_manifest",
            "a profile requires rayengine.toml",
        )),
        None => Ok((None, vec![])),
    }
}

fn create_project(
    path: &Path,
    kind: Kind,
    name: Option<String>,
    sdk_path: Option<PathBuf>,
) -> Result<Value> {
    create_package(path, Template::Game(kind), name, sdk_path)
}

#[derive(Clone, Copy)]
enum Template {
    Game(Kind),
    Plugin,
}

fn create_package(
    path: &Path,
    template: Template,
    name: Option<String>,
    sdk_path: Option<PathBuf>,
) -> Result<Value> {
    let name = name
        .or_else(|| {
            path.file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .ok_or_else(|| Failure::new("invalid_name", "specify a package name with --name"))?;
    validate_name(&name)?;
    let sdk = sdk_path
        .map(|path| {
            let sdk = path.canonicalize().map_err(|_| {
                Failure::new(
                    "missing_sdk",
                    "--sdk-path must point to the rayengine SDK crate",
                )
            })?;
            let sdk_manifest = fs::read_to_string(sdk.join("Cargo.toml"))
                .map_err(|e| Failure::new("missing_sdk", e.to_string()))?;
            if !sdk_manifest
                .lines()
                .map(str::trim)
                .skip_while(|line| *line != "[package]")
                .skip(1)
                .take_while(|line| !line.starts_with('['))
                .any(|line| line == "name = \"rayengine\"")
            {
                return Err(Failure::new(
                    "invalid_sdk",
                    "--sdk-path must point to crates/rayengine, not the workspace root",
                ));
            }
            if sdk.to_str().is_none() {
                return Err(Failure::new("invalid_sdk", "SDK path must be UTF-8"));
            }
            Ok(sdk)
        })
        .transpose()?;
    let dependency = match &sdk {
        Some(path) => format!(
            "{{ path = {} }}",
            serde_json::to_string(path.to_str().expect("validated UTF-8"))
                .expect("string serialization")
        ),
        None => format!("\"{}\"", env!("CARGO_PKG_VERSION")),
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    // Atomic: rejects existing directories, files and symlinks.
    fs::create_dir(path).map_err(|e| {
        Failure::new(
            "destination_exists_or_unwritable",
            format!("{}: {e}", path.display()),
        )
    })?;
    fs::create_dir(path.join("src")).map_err(io_error)?;
    // Nested standalone plugins are explicit dependencies, not automatically
    // adopted members of a generated game's workspace.
    let workspace = match template {
        Template::Game(_) => "[workspace]\nexclude = [\"plugins\"]\n",
        Template::Plugin => "[workspace]\n",
    };
    let cargo = format!(
        "[package]\nname = {}\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.89\"\n\n[dependencies]\nrayengine = {dependency}\n\n{workspace}",
        serde_json::to_string(&name).expect("string serialization")
    );
    fs::write(path.join("Cargo.toml"), cargo).map_err(io_error)?;
    let (source_path, source, kind) = match template {
        Template::Game(Kind::TwoD) => ("src/main.rs", include_str!("templates/2d.rs"), "2d"),
        Template::Game(Kind::ThreeD) => ("src/main.rs", include_str!("templates/3d.rs"), "3d"),
        Template::Plugin => ("src/lib.rs", include_str!("templates/plugin.rs"), "plugin"),
    };
    fs::write(path.join(source_path), source).map_err(io_error)?;
    fs::write(path.join(".gitignore"), "/target/\n/artifacts/\n").map_err(io_error)?;
    let readme = match template {
        Template::Game(_) => format!(
            "# {name}\n\nA rayengine game.\n\n```sh\ncargo run\ncargo check\ncargo run -- --frames 60 --screenshot artifacts/frame.png\n```\n\nrayengine.toml describes runtime defaults and asset discovery; Cargo.toml
still describes the Rust build. Inspect with `rayengine info . --profile dev`.
Run with `rayengine run . --profile dev -- --frames 60`. Direct `cargo run`
loads the base manifest; set RAYENGINE_PROFILE=dev to select a profile.
Paths are relative to the manifest, even when invoked from another directory.

The SDK dependency can be changed in Cargo.toml. Keep game state in\nordinary Rust components; fixed_update handles simulation and draw handles\ninterpolated rendering. See rayengine's rustdoc guides for the API.\n"
        ),
        Template::Plugin => format!(
            "# {name}\n\nAn optional rayengine Cargo plugin.\n\n```sh\ncargo check\ncargo test\ncargo doc --no-deps\n```\n\nAdd this library as a path dependency in the consuming game's Cargo.toml.\nStore MyPlugin and PluginState in your game, then call its Plugin hooks\nexplicitly from Game. The engine does not register or invoke plugins.\nConfigure action IDs in the game; keep GPU work on the render thread.\nThe SDK dependency must match the SDK source/version used by the game.\nIf nested in an existing workspace, exclude this path in its root workspace\nor remove this library's [workspace] and add it to the root members.\nGenerated rayengine games already exclude plugins.\nSee rayengine's plugins rustdoc guide for composition, ownership, and cleanup.\n"
        ),
    };
    fs::write(path.join("README.md"), readme).map_err(io_error)?;
    let mut files = vec!["Cargo.toml", source_path, ".gitignore", "README.md"];
    if matches!(template, Template::Game(_)) {
        let project_manifest = include_str!("templates/rayengine.toml").replace("{{name}}", &name);
        fs::write(path.join("rayengine.toml"), project_manifest).map_err(io_error)?;
        fs::create_dir(path.join("assets")).map_err(io_error)?;
        fs::write(path.join("assets/.gitkeep"), "").map_err(io_error)?;
        files.extend(["rayengine.toml", "assets/.gitkeep"]);
    }
    let project = path.canonicalize().map_err(io_error)?;
    Ok(json!({ "path": project, "name": name, "kind": kind,
        "sdk_path": sdk, "files": files }))
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name.as_bytes()[0].is_ascii_alphabetic()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || ["rayengine", "rayengine-core", "rayengine-cli"].contains(&name)
    {
        Err(Failure::new(
            "invalid_name",
            "package name must start with an ASCII letter and contain letters, digits, '-' or '_' (engine package names are reserved)",
        ))
    } else {
        Ok(())
    }
}

fn cargo_task(
    task: &str,
    path: &Path,
    release: bool,
    profile: Option<&str>,
    bin: Option<&str>,
    args: &[String],
    json_mode: bool,
) -> Result<Value> {
    let manifest = manifest(path)?;
    let (project, _) = project_manifest(path, profile)?;
    let mut command = Command::new("cargo");
    command
        .arg(task)
        .arg("--manifest-path")
        .arg(&manifest)
        .current_dir(manifest.parent().expect("manifest has parent"));
    if release {
        command.arg("--release");
    }
    if json_mode {
        command.arg("--message-format=json");
    }
    if task == "run" {
        if let Some(target) = bin.or_else(|| {
            project
                .as_ref()
                .and_then(|p| p.settings.project.executable.as_deref())
        }) {
            command.arg("--bin").arg(target);
        }
        // Pass an explicit file/profile to opt-in runtimes, independent of cwd.
        command
            .env_remove("RAYENGINE_MANIFEST")
            .env_remove("RAYENGINE_PROFILE");
        if let Some(project) = &project {
            command.env("RAYENGINE_MANIFEST", &project.path);
            if let Some(profile) = &project.profile {
                command.env("RAYENGINE_PROFILE", profile);
            }
        }
        command.arg("--").args(args);
    }
    if !json_mode {
        let status = command.status().map_err(process_error)?;
        if !status.success() {
            return Err(Failure::new(
                "cargo_failed",
                format!("cargo {task} exited with {status}"),
            ));
        }
        return Ok(
            json!({ "manifest": manifest, "release": release, "project_manifest": project }),
        );
    }
    let output = command.output().map_err(process_error)?;
    if !output.status.success() {
        return Err(cargo_failure(output, &format!("cargo {task} failed")));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut diagnostics = Vec::new();
    let mut game_output = Vec::new();
    for line in stdout.lines() {
        match serde_json::from_str::<Value>(line) {
            Ok(value) if value.get("reason").is_some() => diagnostics.push(value),
            _ => game_output.push(line),
        }
    }
    Ok(
        json!({ "manifest": manifest, "release": release, "project_manifest": project, "diagnostics": diagnostics,
        "stdout": game_output.join("\n"), "stderr": String::from_utf8_lossy(&output.stderr) }),
    )
}

fn doctor() -> Result<Value> {
    let mut checks = Vec::new();
    for (name, args) in [
        ("rustc", &["--version"][..]),
        ("cargo", &["--version"]),
        ("cmake", &["--version"]),
        ("clang", &["--version"]),
    ] {
        let output = Command::new(name).args(args).output();
        let (ok, detail) = match output {
            Ok(out) => (
                out.status.success(),
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string(),
            ),
            Err(e) => (false, e.to_string()),
        };
        checks.push(json!({ "name": name, "required": true, "ok": ok, "detail": detail }));
    }
    if cfg!(target_os = "linux") {
        for (name, required, packages) in [
            (
                "X11/OpenGL/audio headers",
                true,
                &["x11", "xrandr", "xinerama", "xcursor", "xi", "gl", "alsa"][..],
            ),
            ("Wayland headers", false, &["wayland-client", "xkbcommon"]),
        ] {
            let ok = Command::new("pkg-config")
                .arg("--exists")
                .args(packages)
                .status()
                .is_ok_and(|s| s.success());
            checks.push(json!({ "name": name, "required": required, "ok": ok, "detail": packages.join(", ") }));
        }
    }
    let data = json!({ "engine_version": env!("CARGO_PKG_VERSION"), "platform": std::env::consts::OS, "checks": checks });
    if checks
        .iter()
        .any(|c| c["required"] == true && c["ok"] == false)
    {
        Err(Failure {
            code: "missing_prerequisites",
            message: "native build prerequisites are missing; see installation guide".into(),
            details: data,
        })
    } else {
        Ok(data)
    }
}

fn cargo_failure(output: std::process::Output, message: &str) -> Failure {
    Failure {
        code: "cargo_failed",
        message: message.into(),
        details: json!({
            "exit_code": output.status.code(), "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "diagnostics": String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| serde_json::from_str::<Value>(line).ok()).collect::<Vec<_>>()
        }),
    }
}
fn io_error(error: std::io::Error) -> Failure {
    Failure::new("io_failed", error.to_string())
}
fn process_error(error: std::io::Error) -> Failure {
    Failure::new("process_failed", error.to_string())
}

fn print_success(command: &str, data: &Value) {
    match command {
        "new" | "new-plugin" => println!(
            "Created {} at {}\nCheck it with: cargo check --manifest-path {}/Cargo.toml",
            data["name"].as_str().unwrap_or("game"),
            data["path"].as_str().unwrap_or(""),
            data["path"].as_str().unwrap_or("")
        ),
        "doctor" => {
            println!(
                "rayengine {} / {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS
            );
            if let Some(checks) = data["checks"].as_array() {
                for check in checks {
                    println!(
                        "{} {}: {}",
                        if check["ok"] == true {
                            "ok"
                        } else {
                            "optional/missing"
                        },
                        check["name"].as_str().unwrap_or(""),
                        check["detail"].as_str().unwrap_or("")
                    );
                }
            }
        }
        "info" => println!(
            "{}",
            serde_json::to_string_pretty(data).expect("JSON value")
        ),
        _ => println!("cargo {command} completed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_or_ambiguous_package_names_are_rejected() {
        for name in [
            "",
            "../game",
            "3game",
            "has space",
            "rayengine",
            "line\nbreak",
        ] {
            assert!(validate_name(name).is_err());
        }
        assert!(validate_name("my-game_2").is_ok());
    }

    #[test]
    fn creation_never_overwrites_an_existing_project() {
        let path = std::env::temp_dir().join(format!("rayengine-cli-test-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep.txt"), "existing user data").unwrap();
        let error = create_project(&path, Kind::TwoD, Some("test-game".into()), None).unwrap_err();
        assert_eq!(error.code, "destination_exists_or_unwritable");
        assert_eq!(
            fs::read_to_string(path.join("keep.txt")).unwrap(),
            "existing user data"
        );
        fs::remove_file(path.join("keep.txt")).unwrap();
        fs::remove_dir(path).unwrap();
    }
}
