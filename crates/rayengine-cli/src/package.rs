//! Relocatable Linux release bundles using Cargo's reported executable paths.
use super::*;
use std::collections::BTreeSet;

struct Outputs {
    folder: PathBuf,
    archive: PathBuf,
    committed: bool,
}
impl Drop for Outputs {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.folder);
            let _ = fs::remove_file(&self.archive);
        }
    }
}

pub(crate) fn package(
    path: &Path,
    profile: Option<&str>,
    requested: Option<&str>,
    output: Option<&Path>,
    features: &[String],
) -> Result<Value> {
    if !cfg!(target_os = "linux") {
        return Err(Failure::new(
            "unsupported_platform",
            "package currently supports Linux only",
        ));
    }
    let manifest = manifest(path)?;
    let root = manifest.parent().expect("manifest parent");
    let (project, _) = project_manifest(path, profile)?;
    let target = lifecycle::host_target(root)?;
    if !target.contains("-linux-") {
        return Err(Failure::new(
            "unsupported_platform",
            "package requires a native Linux Rust toolchain",
        ));
    }
    let architecture = target.split('-').next().expect("target architecture");
    let meta = lifecycle::metadata(&manifest, false)?;
    let pkg = lifecycle::project_package(&meta, &manifest)?;
    let bin = lifecycle::binary(pkg, project.as_ref(), requested)?;
    let name = pkg["name"]
        .as_str()
        .ok_or_else(|| Failure::new("invalid_metadata", "missing package name"))?;
    let version = pkg["version"]
        .as_str()
        .ok_or_else(|| Failure::new("invalid_metadata", "missing package version"))?;
    let bundle_name = format!(
        "{name}-{version}-linux-{}{}",
        architecture,
        profile.map(|s| format!("-{s}")).unwrap_or_default()
    );
    if !bundle_name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
    {
        return Err(Failure::new(
            "invalid_package",
            "package identity must be safe for a folder name",
        ));
    }
    let output = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("bundles"));
    fs::create_dir_all(&output).map_err(io_error)?;
    let output = output.canonicalize().map_err(io_error)?;
    let folder = output.join(&bundle_name);
    let archive = output.join(format!("{bundle_name}.tar.gz"));
    if folder.symlink_metadata().is_ok() || archive.symlink_metadata().is_ok() {
        return Err(Failure::new(
            "bundle_exists",
            "bundle already exists; clean it or choose a different --output",
        ));
    }
    if let Some(project) = &project {
        for root in &project.settings.assets.roots {
            let root = canonical_future_path(root);
            if output.starts_with(&root) || root.starts_with(&folder) || root == archive {
                return Err(Failure::new(
                    "invalid_package",
                    "bundle output must be outside declared asset roots; choose another --output",
                ));
            }
        }
        if project.settings.fonts.values().any(|font| {
            let path = canonical_future_path(&font.path);
            path.starts_with(&folder) || path == archive
        }) {
            return Err(Failure::new(
                "invalid_assets",
                "fonts cannot refer to generated bundle outputs",
            ));
        }
    }
    fs::create_dir(&folder).map_err(io_error)?;
    let mut outputs = Outputs {
        folder: folder.clone(),
        archive: archive.clone(),
        committed: false,
    };
    // Reserve the archive exclusively as well; a raced existing file is preserved.
    let archive_file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive)
    {
        Ok(file) => file,
        Err(e) => {
            outputs.archive = PathBuf::new();
            return Err(io_error(e));
        }
    };
    let mut build_command = Command::new("cargo");
    build_command
        .args([
            "build",
            "--release",
            "--message-format=json",
            "--manifest-path",
        ])
        .arg(&manifest)
        .args(["--bin", &bin, "--target", &target])
        .current_dir(root);
    if !features.is_empty() {
        build_command.arg("--features").arg(features.join(","));
    }
    let build = build_command.output().map_err(process_error)?;
    if !build.status.success() {
        return Err(cargo_failure(build, "release build failed"));
    }
    let diagnostics: Vec<Value> = String::from_utf8_lossy(&build.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let executable = diagnostics
        .iter()
        .find(|d| {
            d["reason"] == "compiler-artifact"
                && d["package_id"] == pkg["id"]
                && d["target"]["name"] == bin
                && d["executable"].is_string()
        })
        .and_then(|d| d["executable"].as_str())
        .ok_or_else(|| {
            Failure::new(
                "missing_executable",
                "Cargo did not report the selected release executable",
            )
        })?;
    fs::create_dir(folder.join("bin")).map_err(io_error)?;
    fs::copy(executable, folder.join("bin/game")).map_err(io_error)?;
    let mut assets = vec![];
    let mut bundled_roots = vec![];
    let mut config = if let Some(project) = &project {
        let mut table: toml::Table = fs::read_to_string(&project.path)
            .map_err(io_error)?
            .parse()
            .map_err(|e| Failure::new("invalid_project_manifest", format!("{e}")))?;
        table.remove("schema_version");
        let profiles = table.remove("profiles");
        if let Some(profile) = profile
            && let Some(overlay) = profiles
                .as_ref()
                .and_then(|p| p.get(profile))
                .and_then(toml::Value::as_table)
        {
            merge(&mut table, overlay);
        }
        let mut logical_names = BTreeSet::new();
        // Discovery yields physical paths; overlapping roots can give one file
        // multiple logical names. Keep the declaring root while discovering.
        for (index, root) in project.settings.assets.roots.iter().enumerate() {
            // Separate directories preserve valid file/directory overlaps across roots.
            let bundled_root = format!("assets/{index}");
            fs::create_dir_all(folder.join(&bundled_root)).map_err(io_error)?;
            bundled_roots.push(toml::Value::String(bundled_root.clone()));
            let mut discovery = project.clone();
            discovery.settings.assets.roots = vec![root.clone()];
            for source in discovery
                .discover_assets()
                .map_err(|e| Failure::new("invalid_assets", e.to_string()))?
            {
                let relative = source
                    .strip_prefix(root)
                    .map_err(|_| Failure::new("invalid_assets", "asset outside declared root"))?;
                if !logical_names.insert(relative.to_path_buf()) {
                    continue;
                }
                let destination = Path::new(&bundled_root).join(relative);
                copy_file(&source, &folder.join(&destination))?;
                assets.push(destination);
            }
        }
        // Fonts are explicit file declarations independent of asset exclusions.
        if let Some(fonts) = table.get_mut("fonts").and_then(toml::Value::as_table_mut) {
            for (index, (name, font)) in fonts.iter_mut().enumerate() {
                let source = &project.settings.fonts[name].path;
                let destination = PathBuf::from(format!("fonts/{index}.font"));
                copy_file(source, &folder.join(&destination))?;
                font.as_table_mut().expect("validated font").insert(
                    "path".into(),
                    toml::Value::String(destination.to_string_lossy().into()),
                );
                assets.push(destination);
            }
        }
        table
    } else {
        toml::Table::new()
    };
    config.insert("schema_version".into(), toml::Value::Integer(1));
    let mut asset_table = toml::Table::new();
    asset_table.insert("roots".into(), toml::Value::Array(bundled_roots));
    config.insert("assets".into(), toml::Value::Table(asset_table));
    fs::create_dir_all(folder.join("assets")).map_err(io_error)?;
    fs::write(
        folder.join("rayengine.toml"),
        toml::to_string_pretty(&config)
            .map_err(|e| Failure::new("invalid_project_manifest", e.to_string()))?,
    )
    .map_err(io_error)?;
    // Runtime paths come from the launcher, independent of build-time paths/cwd.
    fs::write(folder.join("launch"), "#!/bin/sh\nset -eu\ncd -- \"$(dirname -- \"$0\")\"\nexport RAYENGINE_MANIFEST=\"$PWD/rayengine.toml\"\nunset RAYENGINE_PROFILE\nexec ./bin/game \"$@\"\n").map_err(io_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(folder.join("launch"), fs::Permissions::from_mode(0o755))
            .map_err(io_error)?;
        fs::set_permissions(folder.join("bin/game"), fs::Permissions::from_mode(0o755))
            .map_err(io_error)?;
    }
    let libraries = Command::new("ldd")
        .arg(executable)
        .output()
        .map_err(process_error)?;
    let libraries_text = format!(
        "{}{}",
        String::from_utf8_lossy(&libraries.stdout),
        String::from_utf8_lossy(&libraries.stderr)
    );
    if libraries_text.contains("not found")
        || (!libraries.status.success()
            && !libraries_text.contains("not a dynamic executable")
            && !libraries_text.contains("statically linked"))
    {
        return Err(Failure::new("runtime_libraries_failed", libraries_text));
    }
    fs::write(folder.join("runtime-libraries.txt"), &libraries_text).map_err(io_error)?;
    let notices = dependency_notices(&manifest, pkg, &folder, features)?;
    for entry in fs::read_dir(root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().map_err(io_error)?.is_file()
            && ["LICENSE", "COPYING", "NOTICE"]
                .iter()
                .any(|p| name.starts_with(p))
        {
            fs::copy(entry.path(), folder.join(&name)).map_err(io_error)?;
        }
    }
    if let Some(license) = pkg["license_file"].as_str() {
        let source = root.join(license);
        copy_file(&source, &folder.join("PROJECT_LICENSE"))?;
    }
    fs::write(folder.join("README.txt"), format!("{name} {version}\n\nRun ./launch [game arguments] from any working directory.\nLinux {} release; dynamic libraries and build-host paths are listed in runtime-libraries.txt.\nInstall the corresponding runtime packages on the destination machine. For raylib/X11 games these normally include glibc, libgcc, libX11, libXrandr, libXinerama, libXcursor, libXi, OpenGL/Mesa and ALSA; Wayland builds also need Wayland and xkbcommon. A display and working OpenGL driver are required.\nBuild on the oldest supported Linux/glibc baseline; this bundle does not include system libraries or promise compatibility with older glibc.\nUse rayengine manifest asset lookup (and declared fonts) for relocatable assets. Compile-time embedded assets stay in the binary. Game-owned absolute paths in extension tables are not rewritten.\nThe selected project profile is baked into rayengine.toml.\n", std::env::consts::ARCH)).map_err(io_error)?;
    let provenance = json!({"schema_version":1, "source_manifest":manifest, "target":target, "features":features, "name":name, "version":version, "binary":bin, "profile":profile, "assets":assets, "runtime_libraries":libraries_text, "notices":notices});
    fs::write(
        folder.join(".rayengine-bundle.json"),
        serde_json::to_vec_pretty(&provenance).expect("JSON"),
    )
    .map_err(io_error)?;
    let tar = Command::new("tar")
        .args(["-czf", "-", "--"])
        .arg(&bundle_name)
        .current_dir(&output)
        .stdout(archive_file)
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(process_error)?;
    if !tar.status.success() {
        return Err(Failure::new(
            "archive_failed",
            String::from_utf8_lossy(&tar.stderr),
        ));
    }
    outputs.committed = true;
    Ok(
        json!({"manifest":manifest, "folder":folder, "archive":archive, "binary":bin, "profile":profile, "target":target, "features":features, "release":true, "assets":assets, "runtime_libraries":libraries_text, "notices":notices, "diagnostics":diagnostics, "stderr":String::from_utf8_lossy(&build.stderr)}),
    )
}

fn merge(base: &mut toml::Table, overlay: &toml::Table) {
    for (key, value) in overlay {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(base)), toml::Value::Table(overlay)) => merge(base, overlay),
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        return Err(Failure::new(
            "invalid_assets",
            format!("{} is not a regular file", source.display()),
        ));
    }
    fs::create_dir_all(destination.parent().expect("destination parent")).map_err(io_error)?;
    fs::copy(source, destination).map_err(io_error)?;
    Ok(())
}

fn dependency_notices(
    manifest: &Path,
    package: &Value,
    folder: &Path,
    features: &[String],
) -> Result<Vec<Value>> {
    let meta = lifecycle::metadata_with_features(manifest, true, features)?;
    let mut pending = vec![package["id"].as_str().expect("package id").to_string()];
    let mut ids = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !ids.insert(id.clone()) {
            continue;
        }
        if let Some(node) = meta["resolve"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|n| n["id"] == id)
        {
            for dep in node["deps"].as_array().into_iter().flatten() {
                if dep["dep_kinds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|k| k["kind"].is_null())
                {
                    pending.push(dep["pkg"].as_str().expect("dependency id").to_string());
                }
            }
        }
    }
    let mut notices = vec![];
    for pkg in meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["id"].as_str().is_some_and(|id| ids.contains(id)))
    {
        let root = Path::new(pkg["manifest_path"].as_str().expect("package manifest"))
            .parent()
            .expect("parent");
        let destination = folder.join("THIRD_PARTY_NOTICES").join(format!(
            "{}-{}",
            pkg["name"].as_str().expect("name"),
            pkg["version"].as_str().expect("version")
        ));
        fs::create_dir_all(&destination).map_err(io_error)?;
        let mut sources = BTreeSet::new();
        for entry in fs::read_dir(root).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            if entry.file_type().map_err(io_error)?.is_file()
                && ["LICENSE", "COPYING", "NOTICE"]
                    .iter()
                    .any(|p| entry.file_name().to_string_lossy().starts_with(p))
            {
                sources.insert(entry.path());
            }
        }
        if let Some(file) = pkg["license_file"].as_str() {
            sources.insert(root.join(file));
        }
        // Workspace crates often share the root license rather than copying it.
        if pkg["source"]
            .as_str()
            .is_none_or(|source| !source.starts_with("registry+"))
            && sources.is_empty()
        {
            // An external path dependency may belong to another workspace.
            // Resolve its own shared license, never the consuming game's license.
            let own_meta = lifecycle::metadata(&root.join("Cargo.toml"), false)?;
            if let Some(workspace) = own_meta["workspace_root"].as_str() {
                for entry in fs::read_dir(workspace).map_err(io_error)? {
                    let entry = entry.map_err(io_error)?;
                    if entry.file_type().map_err(io_error)?.is_file()
                        && ["LICENSE", "COPYING", "NOTICE"]
                            .iter()
                            .any(|prefix| entry.file_name().to_string_lossy().starts_with(prefix))
                    {
                        sources.insert(entry.path());
                    }
                }
            }
        }
        if pkg["name"] == "raylib-sys" {
            sources.insert(root.join("raylib/LICENSE"));
            sources.insert(root.join("raylib/src/external/glfw/LICENSE.md"));
            for entry in fs::read_dir(root.join("raylib/src/external")).map_err(io_error)? {
                let entry = entry.map_err(io_error)?;
                if entry.file_type().map_err(io_error)?.is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|s| s == "h" || s == "c")
                {
                    sources.insert(entry.path());
                }
            }
        }
        let mut files = vec![];
        for (index, source) in sources.iter().enumerate() {
            let target = destination.join(format!(
                "{index}-{}",
                source
                    .file_name()
                    .expect("license filename")
                    .to_string_lossy()
            ));
            copy_file(source, &target)?;
            files.push(
                target
                    .strip_prefix(folder)
                    .expect("below folder")
                    .to_path_buf(),
            );
        }
        notices.push(json!({"name":pkg["name"], "version":pkg["version"], "license":pkg["license"], "repository":pkg["repository"], "files":files}));
    }
    fs::create_dir_all(folder.join("THIRD_PARTY_NOTICES")).map_err(io_error)?;
    fs::write(
        folder.join("THIRD_PARTY_NOTICES/metadata.json"),
        serde_json::to_vec_pretty(&json!({"schema_version":1,"packages":notices})).expect("JSON"),
    )
    .map_err(io_error)?;
    Ok(notices)
}

// Resolve existing symlinked parents as well as declarations that precede creation.
fn canonical_future_path(path: &Path) -> PathBuf {
    for ancestor in path.ancestors() {
        if let Ok(mut canonical) = ancestor.canonicalize() {
            for component in path
                .strip_prefix(ancestor)
                .expect("path ancestor")
                .components()
            {
                match component {
                    std::path::Component::ParentDir => {
                        canonical.pop();
                    }
                    std::path::Component::Normal(name) => canonical.push(name),
                    _ => (),
                }
            }
            return canonical;
        }
    }
    path.to_path_buf()
}
