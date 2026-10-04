//! Shared Cargo inspection and conservative cleanup.
use super::*;

pub(crate) fn metadata(manifest: &Path, dependencies: bool) -> Result<Value> {
    metadata_with_features(manifest, dependencies, &[])
}

pub(crate) fn metadata_with_features(
    manifest: &Path,
    dependencies: bool,
    features: &[String],
) -> Result<Value> {
    let mut command = Command::new("cargo");
    command
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(manifest)
        .current_dir(manifest.parent().expect("manifest parent"));
    if !dependencies {
        command.arg("--no-deps");
    }
    if !features.is_empty() {
        command.arg("--features").arg(features.join(","));
    }
    if dependencies {
        command
            .arg("--filter-platform")
            .arg(host_target(manifest.parent().expect("manifest parent"))?);
    }
    let output = command.output().map_err(process_error)?;
    if !output.status.success() {
        return Err(cargo_failure(output, "cargo metadata failed"));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| Failure::new("invalid_metadata", e.to_string()))
}

pub(crate) fn project_package<'a>(metadata: &'a Value, manifest: &Path) -> Result<&'a Value> {
    metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages.iter().find(|p| {
                p["manifest_path"]
                    .as_str()
                    .is_some_and(|s| Path::new(s) == manifest)
            })
        })
        .ok_or_else(|| {
            Failure::new(
                "invalid_package",
                "select a package Cargo.toml, not a virtual workspace",
            )
        })
}

pub(crate) fn binary(
    package: &Value,
    project: Option<&ResolvedManifest>,
    requested: Option<&str>,
) -> Result<String> {
    let targets: Vec<_> = package["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| {
            t["kind"]
                .as_array()
                .is_some_and(|k| k.contains(&json!("bin")))
        })
        .filter_map(|t| t["name"].as_str())
        .collect();
    let selected = requested
        .or_else(|| project.and_then(|p| p.settings.project.executable.as_deref()))
        .or_else(|| package["default_run"].as_str())
        .or_else(|| (targets.len() == 1).then(|| targets[0]));
    match selected {
        Some(name) if targets.contains(&name) => Ok(name.into()),
        _ => Err(Failure::new(
            "invalid_binary",
            "select a valid executable with --bin or project.executable",
        )),
    }
}

pub(crate) fn clean(path: &Path, output: Option<&Path>) -> Result<Value> {
    let manifest = manifest(path)?;
    let root = manifest.parent().expect("manifest parent");
    let output_dir = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("bundles"));
    // A marker belongs to this exact project and lists only generated entries.
    // Never recursively delete a caller-provided output directory itself.
    let mut removed = vec![];
    if output_dir.exists() {
        for entry in fs::read_dir(&output_dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            if entry.file_type().map_err(io_error)?.is_symlink()
                || !entry.file_type().map_err(io_error)?.is_dir()
            {
                continue;
            }
            let marker = entry.path().join(".rayengine-bundle.json");
            let Ok(source) = fs::read(&marker) else {
                continue;
            };
            let Ok(marker) = serde_json::from_slice::<Value>(&source) else {
                continue;
            };
            if marker["source_manifest"] != json!(manifest) {
                continue;
            }
            let archive =
                output_dir.join(format!("{}.tar.gz", entry.file_name().to_string_lossy()));
            fs::remove_dir_all(entry.path()).map_err(io_error)?;
            removed.push(entry.path());
            if archive.is_file() {
                fs::remove_file(&archive).map_err(io_error)?;
                removed.push(archive);
            }
        }
    }
    let output = Command::new("cargo")
        .args(["clean", "--manifest-path"])
        .arg(&manifest)
        .current_dir(root)
        .output()
        .map_err(process_error)?;
    if !output.status.success() {
        return Err(cargo_failure(output, "cargo clean failed"));
    }
    Ok(
        json!({"manifest": manifest, "removed_bundles": removed, "stdout": String::from_utf8_lossy(&output.stdout), "stderr": String::from_utf8_lossy(&output.stderr)}),
    )
}

pub(crate) fn fix_hints() -> Vec<String> {
    let release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let distro = release
        .lines()
        .find_map(|line| line.strip_prefix("ID="))
        .unwrap_or("")
        .trim_matches('"');
    let command = match distro {
        "arch" | "manjaro" | "endeavouros" => {
            "sudo pacman -S --needed base-devel cmake clang pkgconf libx11 libxrandr libxinerama libxcursor libxi mesa alsa-lib wayland libxkbcommon"
        }
        "ubuntu" | "debian" | "linuxmint" | "pop" => {
            "sudo apt install build-essential cmake clang pkg-config libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev libgl1-mesa-dev libasound2-dev libwayland-dev libxkbcommon-dev"
        }
        "fedora" | "rhel" | "centos" => {
            "sudo dnf install gcc gcc-c++ cmake clang pkgconf-pkg-config libX11-devel libXrandr-devel libXinerama-devel libXcursor-devel libXi-devel mesa-libGL-devel alsa-lib-devel wayland-devel libxkbcommon-devel"
        }
        "opensuse-tumbleweed" | "opensuse-leap" => {
            "sudo zypper install gcc gcc-c++ cmake clang pkg-config libX11-devel libXrandr-devel libXinerama-devel libXcursor-devel libXi-devel Mesa-libGL-devel alsa-devel wayland-devel libxkbcommon-devel"
        }
        _ => {
            "Install a C/C++ compiler, CMake, clang, pkg-config, X11/OpenGL/ALSA development headers; optional Wayland/xkbcommon headers. Install Rust 1.89+ via https://rustup.rs/"
        }
    };
    vec![
        command.into(),
        "Rust toolchain: https://rustup.rs/ (rustc and cargo 1.89 or newer)".into(),
    ]
}

pub(crate) fn host_target(root: &Path) -> Result<String> {
    let rustc = Command::new("rustc")
        .arg("-vV")
        .current_dir(root)
        .output()
        .map_err(process_error)?;
    if !rustc.status.success() {
        return Err(Failure::new(
            "process_failed",
            String::from_utf8_lossy(&rustc.stderr),
        ));
    }
    String::from_utf8_lossy(&rustc.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| Failure::new("process_failed", "rustc did not report a host target"))
}
