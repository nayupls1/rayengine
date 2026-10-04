//! Polling watcher with debounced builds and owned child-process cleanup.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) struct Options {
    pub path: PathBuf,
    pub release: bool,
    pub profile: Option<String>,
    pub bin: Option<String>,
    pub debounce_ms: u64,
    pub cycles: Option<u32>,
    pub timeout_ms: Option<u64>,
    pub args: Vec<String>,
    pub features: Vec<String>,
    pub json_mode: bool,
}

struct Process {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
    stopped: bool,
}
impl Process {
    fn spawn(mut command: Command, logs: &Path, label: &str) -> Result<Self> {
        let stdout = logs.join(format!("{label}.stdout"));
        let stderr = logs.join(format!("{label}.stderr"));
        command
            .stdout(Stdio::from(fs::File::create(&stdout).map_err(io_error)?))
            .stderr(Stdio::from(fs::File::create(&stderr).map_err(io_error)?));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        Ok(Self {
            child: command.spawn().map_err(process_error)?,
            stdout,
            stderr,
            stopped: false,
        })
    }
    fn wait_until(
        &mut self,
        stopped: impl Fn() -> bool,
    ) -> Result<Option<std::process::ExitStatus>> {
        loop {
            if let Some(status) = self.child.try_wait().map_err(process_error)? {
                return Ok(Some(status));
            }
            if stopped() {
                self.stop();
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    fn output(&self) -> Value {
        json!({"stdout": fs::read_to_string(&self.stdout).unwrap_or_default(), "stderr": fs::read_to_string(&self.stderr).unwrap_or_default()})
    }
    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        #[cfg(unix)]
        {
            // Kill the owned group, including descendants, even if the leader exited.
            use nix::{
                sys::signal::{Signal, killpg},
                unistd::Pid,
            };
            let group = Pid::from_raw(self.child.id() as i32);
            let _ = killpg(group, Signal::SIGTERM);
            thread::sleep(Duration::from_millis(20));
            let _ = killpg(group, Signal::SIGKILL);
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}
struct Logs(PathBuf);
impl Drop for Logs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

type Snapshot = BTreeMap<PathBuf, u64>;
fn snapshot(roots: &[PathBuf], targets: &BTreeSet<PathBuf>) -> Result<Snapshot> {
    fn visit(path: &Path, targets: &BTreeSet<PathBuf>, files: &mut Snapshot) -> Result<()> {
        if targets.iter().any(|target| path.starts_with(target)) {
            return Ok(());
        }
        let metadata = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io_error(e)),
        };
        if metadata.is_symlink() {
            return Ok(());
        }
        if metadata.is_dir() {
            if path.file_name().is_some_and(|s| {
                ["target", "bundles", "artifacts", ".git"]
                    .iter()
                    .any(|n| s == *n)
            }) {
                return Ok(());
            }
            for entry in fs::read_dir(path).map_err(io_error)? {
                visit(&entry.map_err(io_error)?.path(), targets, files)?;
            }
        } else if metadata.is_file() {
            let contents = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(io_error(e)),
            };
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            contents.hash(&mut hash);
            files.insert(path.to_path_buf(), hash.finish());
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    for root in roots {
        visit(root, targets, &mut files)?;
    }
    Ok(files)
}

pub(crate) fn watch(options: Options) -> Result<Value> {
    let manifest = manifest(&options.path)?;
    let root = manifest.parent().expect("manifest parent");
    let (project, _) = project_manifest(&options.path, options.profile.as_deref())?;
    let meta = lifecycle::metadata_with_features(&manifest, true, &options.features)?;
    let package = lifecycle::project_package(&meta, &manifest)?;
    let bin = lifecycle::binary(package, project.as_ref(), options.bin.as_deref())?;
    let mut roots = watch_roots(&meta, project.as_ref());
    let mut targets = BTreeSet::from([PathBuf::from(
        meta["target_directory"].as_str().expect("target directory"),
    )]);
    let mut seen = snapshot(&roots, &targets)?;
    let started = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))
        .map_err(|e| Failure::new("watch_failed", e.to_string()))?;
    let logs = Logs(std::env::temp_dir().join(format!("rayengine-watch-{}", std::process::id())));
    fs::create_dir(&logs.0).map_err(io_error)?;
    let stopped = || {
        stop.load(Ordering::SeqCst)
            || options
                .timeout_ms
                .is_some_and(|ms| started.elapsed() >= Duration::from_millis(ms))
    };
    let mut cycles: Vec<Value> = vec![];
    let mut total = 0;
    let mut game: Option<Process> = None;
    let mut pending = Some(Instant::now() - Duration::from_millis(options.debounce_ms));
    let mut changed = vec![];
    while !stopped() {
        if pending.is_some_and(|at| at.elapsed() >= Duration::from_millis(options.debounce_ms)) {
            pending = None;
            if let Some(mut process) = game.take() {
                process.stop();
                if let Some(cycle) = cycles.last_mut() {
                    cycle["game_output"] = process.output();
                }
            }
            total += 1;
            if !options.json_mode {
                eprintln!(
                    "watch: cycle {total}: rebuilding {} changed file(s)",
                    changed.len()
                );
            }
            let result = (|| -> Result<(Value, Option<Process>)> {
                let (project, _) = project_manifest(&options.path, options.profile.as_deref())?;
                // Re-inspect targets after manifest edits, including executable changes.
                let mut inspect = Command::new("cargo");
                inspect
                    .args(["metadata", "--format-version", "1", "--manifest-path"])
                    .arg(&manifest)
                    .current_dir(root);
                if !options.features.is_empty() {
                    inspect.arg("--features").arg(options.features.join(","));
                }
                let mut inspect = Process::spawn(inspect, &logs.0, "metadata")?;
                let status = inspect.wait_until(stopped)?;
                if status.is_none() {
                    return Ok((
                        json!({"ok":false,"error":{"code":"watch_interrupted","message":"metadata interrupted","details":inspect.output()}}),
                        None,
                    ));
                }
                let output = inspect.output();
                if !status.expect("checked status").success() {
                    return Ok((
                        json!({"ok":false,"error":{"code":"cargo_failed","message":"metadata failed","details":output}}),
                        None,
                    ));
                }
                let meta: Value = serde_json::from_str(output["stdout"].as_str().unwrap_or(""))
                    .map_err(|e| Failure::new("invalid_metadata", e.to_string()))?;
                let previous_roots =
                    std::mem::replace(&mut roots, watch_roots(&meta, project.as_ref()));
                targets.insert(PathBuf::from(
                    meta["target_directory"].as_str().ok_or_else(|| {
                        Failure::new("invalid_metadata", "missing target directory")
                    })?,
                ));
                // Keep prior build directories excluded and avoid treating added/removed
                // watch roots as edits. Existing watched content keeps its old hash so
                // edits during metadata/build still cause another cycle.
                seen.retain(|path, _| {
                    roots.iter().any(|root| path.starts_with(root))
                        && !targets.iter().any(|target| path.starts_with(target))
                });
                for (path, hash) in snapshot(&roots, &targets)? {
                    if !previous_roots.iter().any(|root| path.starts_with(root)) {
                        seen.insert(path, hash);
                    }
                }
                let package = lifecycle::project_package(&meta, &manifest)?;
                let selected =
                    lifecycle::binary(package, project.as_ref(), options.bin.as_deref())?;
                let mut build = Command::new("cargo");
                build
                    .args(["build", "--message-format=json", "--manifest-path"])
                    .arg(&manifest)
                    .args(["--bin", &selected])
                    .current_dir(root);
                if !options.features.is_empty() {
                    build.arg("--features").arg(options.features.join(","));
                }
                if options.release {
                    build.arg("--release");
                }
                let mut build = Process::spawn(build, &logs.0, "build")?;
                let status = build.wait_until(stopped)?;
                let output = build.output();
                let diagnostics: Vec<Value> = output["stdout"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .filter_map(|s| serde_json::from_str(s).ok())
                    .collect();
                if status.is_none() {
                    return Ok((
                        json!({"ok":false,"error":{"code":"watch_interrupted","message":"build interrupted","details":output},"diagnostics":diagnostics}),
                        None,
                    ));
                }
                if !status.expect("checked status").success() {
                    return Ok((
                        json!({"ok":false,"error":{"code":"cargo_failed","message":"watch build failed","details":output},"diagnostics":diagnostics}),
                        None,
                    ));
                }
                let _executable = diagnostics
                    .iter()
                    .find(|d| {
                        d["reason"] == "compiler-artifact"
                            && d["package_id"] == package["id"]
                            && d["target"]["name"] == selected
                            && d["executable"].is_string()
                    })
                    .and_then(|d| d["executable"].as_str())
                    .ok_or_else(|| {
                        Failure::new(
                            "missing_executable",
                            "Cargo did not report the selected executable",
                        )
                    })?;
                // Cargo applies configured target runners and their wrappers, as run does.
                let mut command = Command::new("cargo");
                command
                    .args(["run", "--quiet", "--manifest-path"])
                    .arg(&manifest)
                    .args(["--bin", &selected]);
                if options.release {
                    command.arg("--release");
                }
                if !options.features.is_empty() {
                    command.arg("--features").arg(options.features.join(","));
                }
                command
                    .arg("--")
                    .args(&options.args)
                    .current_dir(root)
                    .env_remove("RAYENGINE_MANIFEST")
                    .env_remove("RAYENGINE_PROFILE");
                if let Some(project) = &project {
                    command.env("RAYENGINE_MANIFEST", &project.path);
                    if let Some(profile) = &project.profile {
                        command.env("RAYENGINE_PROFILE", profile);
                    }
                }
                let process = Process::spawn(command, &logs.0, "game")?;
                Ok((
                    json!({"ok":true,"binary":selected,"diagnostics":diagnostics,"build_output":output,"game_output":null,"game_exit_code":null}),
                    Some(process),
                ))
            })();
            let (mut cycle, process) = match result {
                Ok(result) => result,
                Err(e) => (
                    json!({"ok":false,"error":{"code":e.code,"message":e.message,"details":e.details}}),
                    None,
                ),
            };
            cycle["cycle"] = json!(total);
            cycle["changed"] = json!(changed);
            changed.clear();
            if !options.json_mode {
                eprintln!(
                    "watch: cycle {total}: {}",
                    if cycle["ok"] == true {
                        "restarted"
                    } else {
                        "failed; waiting for changes"
                    }
                );
            }
            if !options.json_mode && cycle["ok"] == false {
                report_failure(&cycle);
            }
            game = process;
            cycles.push(cycle);
            if cycles.len() > 32 {
                cycles.remove(0);
            }
            if options.cycles.is_some_and(|limit| total >= limit) {
                break;
            }
        }
        if let Some(process) = &mut game
            && let Some(status) = process.child.try_wait().map_err(process_error)?
        {
            if let Some(cycle) = cycles.last_mut() {
                cycle["game_exit_code"] = json!(status.code());
                cycle["game_output"] = process.output();
                if !status.success() {
                    cycle["ok"] = json!(false);
                    cycle["error"] = json!({"code":"game_failed","message":format!("game exited with {status}"),"details":null});
                    if !options.json_mode {
                        report_failure(cycle);
                    }
                }
            }
            if !options.json_mode {
                eprintln!("watch: game exited with {status}; waiting for changes");
            }
            game = None;
        }
        let current = snapshot(&roots, &targets)?;
        if current != seen {
            changed.extend(
                current
                    .keys()
                    .chain(seen.keys())
                    .filter(|p| current.get(*p) != seen.get(*p))
                    .cloned(),
            );
            changed.sort();
            changed.dedup();
            seen = current;
            pending = Some(Instant::now());
        }
        thread::sleep(Duration::from_millis(50));
    }
    if let Some(mut process) = game {
        process.stop();
        if let Some(cycle) = cycles.last_mut() {
            cycle["game_output"] = process.output();
        }
    }
    let data = json!({"manifest":manifest,"binary":bin,"profile":options.profile,"release":options.release,"features":options.features,"cycle_count":total,"cycles":cycles,"stopped":if stop.load(Ordering::SeqCst) {"signal"} else if options.cycles.is_some_and(|limit| total >= limit) {"cycle_limit"} else {"timeout"}});
    if cycles
        .last()
        .is_some_and(|c| c["ok"] == false && c["error"]["code"] != "watch_interrupted")
    {
        Err(Failure {
            code: "watch_failed",
            message: "the final watch cycle failed".into(),
            details: data,
        })
    } else {
        Ok(data)
    }
}

fn watch_roots(meta: &Value, project: Option<&ResolvedManifest>) -> Vec<PathBuf> {
    let mut roots: Vec<_> = meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["source"].is_null())
        .filter_map(|p| {
            p["manifest_path"]
                .as_str()
                .and_then(|s| Path::new(s).parent())
                .map(Path::to_path_buf)
        })
        .collect();
    if let Some(workspace) = meta["workspace_root"].as_str() {
        for file in [
            "Cargo.toml",
            "Cargo.lock",
            ".cargo",
            "rust-toolchain",
            "rust-toolchain.toml",
        ] {
            roots.push(Path::new(workspace).join(file));
        }
    }
    if let Some(project) = project {
        roots.extend(project.settings.assets.roots.iter().cloned());
        roots.extend(project.settings.fonts.values().map(|f| f.path.clone()));
        roots.push(project.path.clone());
    }
    roots.sort();
    roots.dedup();
    roots
}

fn report_failure(cycle: &Value) {
    if let Some(message) = cycle["error"]["message"].as_str() {
        eprintln!("watch: {message}");
    }
    for diagnostic in cycle["diagnostics"].as_array().into_iter().flatten() {
        if let Some(rendered) = diagnostic["message"]["rendered"].as_str() {
            eprint!("{rendered}");
        }
    }
    for stderr in [
        cycle["error"]["details"]["stderr"].as_str(),
        cycle["game_output"]["stderr"].as_str(),
    ]
    .into_iter()
    .flatten()
    {
        eprint!("{stderr}");
    }
}
