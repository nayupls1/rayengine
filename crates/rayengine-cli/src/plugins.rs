//! First-party dependency edits preserve authored TOML and plugin configuration.
use super::*;
use toml_edit::{DocumentMut, Item, Table, value};

pub(crate) const NAMES: &[&str] = &["particles", "voxel", "beacons", "tilemap"];

pub(crate) fn edit(
    path: &Path,
    name: &str,
    add: bool,
    local: Option<&Path>,
    features: &[String],
) -> Result<Value> {
    let name = name.strip_prefix("rayengine-").unwrap_or(name);
    if !NAMES.contains(&name) {
        return Err(Failure::new(
            "unknown_plugin",
            format!("unknown plugin `{name}`; available: {}", NAMES.join(", ")),
        ));
    }
    let manifest = manifest(path)?;
    let root = manifest.parent().expect("manifest parent");
    let (project, _) = project_manifest(path, None)?;
    let project_path = project
        .as_ref()
        .map(|p| p.path.clone())
        .unwrap_or_else(|| root.join("rayengine.toml"));
    let mut cargo = document(&manifest)?;
    let mut config = if project_path.exists() {
        document(&project_path)?
    } else {
        let mut doc = DocumentMut::new();
        doc["schema_version"] = value(1);
        doc
    };
    let dependency = format!("rayengine-{name}");
    let meta = lifecycle::metadata(&manifest, false)?;
    let package = lifecycle::project_package(&meta, &manifest)?;
    // Renamed dependencies must be edited under their authored key.
    let existing = package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["name"] == dependency && d["kind"].is_null() && d["target"].is_null());
    let key = existing
        .and_then(|d| d["rename"].as_str())
        .unwrap_or(&dependency)
        .to_string();
    let mut selected_path = None;
    if add {
        if existing.is_some()
            || cargo
                .get("dependencies")
                .and_then(|d| d.get(&key))
                .is_some()
        {
            return Err(Failure::new(
                "plugin_already_added",
                format!("{dependency} is already a dependency"),
            ));
        }
        if features.iter().any(|f| {
            f.is_empty()
                || !f
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        }) {
            return Err(Failure::new(
                "invalid_plugin",
                "plugin features must be nonempty feature names",
            ));
        }
        let sdk = package["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| d["name"] == "rayengine" && d["kind"].is_null() && d["target"].is_null())
            .ok_or_else(|| {
                Failure::new("missing_sdk", "the package needs a rayengine dependency")
            })?;
        let inferred = sdk["path"]
            .as_str()
            .map(|s| Path::new(s).join("../../plugins").join(name));
        let local = local.map(Path::to_path_buf).or(inferred);
        let mut entry = Table::new();
        if let Some(local) = local {
            let local = local.canonicalize().map_err(|e| {
                Failure::new(
                    "invalid_plugin",
                    format!(
                        "{}: {e}; supply --plugin-path for this SDK",
                        local.display()
                    ),
                )
            })?;
            let plugin_manifest = local.join("Cargo.toml").canonicalize().map_err(io_error)?;
            let plugin_meta = lifecycle::metadata(&plugin_manifest, false)?;
            let plugin = lifecycle::project_package(&plugin_meta, &plugin_manifest)?;
            if plugin["name"] != dependency {
                return Err(Failure::new(
                    "invalid_plugin",
                    format!("--plugin-path must point to {dependency}"),
                ));
            }
            if features.iter().any(|f| plugin["features"].get(f).is_none()) {
                return Err(Failure::new(
                    "invalid_plugin",
                    "requested feature does not exist in the local plugin",
                ));
            }
            // Local source trees must agree so games do not acquire two incompatible SDKs.
            for dep in plugin["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["name"] == "rayengine" || d["name"] == "rayengine-core")
            {
                let expected_path = sdk["path"].as_str().map(|s| {
                    if dep["name"] == "rayengine-core" {
                        Path::new(s).with_file_name("rayengine-core")
                    } else {
                        PathBuf::from(s)
                    }
                });
                let compatible = match (expected_path, dep["path"].as_str()) {
                    (Some(expected), Some(actual)) => {
                        match (expected.canonicalize(), Path::new(actual).canonicalize()) {
                            (Ok(expected), Ok(actual)) => expected == actual,
                            _ => false,
                        }
                    }
                    (None, None) => sdk["req"] == dep["req"] && sdk["source"] == dep["source"],
                    _ => false,
                };
                if !compatible {
                    return Err(Failure::new(
                        "plugin_sdk_mismatch",
                        "plugin and game must use the same SDK source/version",
                    ));
                }
            }
            entry["path"] = value(
                local
                    .to_str()
                    .ok_or_else(|| Failure::new("invalid_plugin", "plugin path must be UTF-8"))?,
            );
            selected_path = Some(local);
        } else {
            if !sdk["source"]
                .as_str()
                .is_some_and(|source| source.starts_with("registry+"))
            {
                return Err(Failure::new(
                    "invalid_sdk",
                    "non-registry SDK sources require a compatible --plugin-path; version numbers alone do not identify the SDK source",
                ));
            }
            let req = sdk["req"]
                .as_str()
                .unwrap_or("")
                .trim_start_matches(['^', '~', '=']);
            if req.split('.').count() != 3 || !req.bytes().all(|c| c.is_ascii_digit() || c == b'.')
            {
                return Err(Failure::new(
                    "invalid_sdk",
                    "use a concrete SDK version or --plugin-path",
                ));
            }
            entry["version"] = value(format!("={req}"));
        }
        if cargo
            .get("dependencies")
            .is_some_and(|d| d.as_table_like().is_none())
        {
            return Err(Failure::new(
                "invalid_cargo_manifest",
                "dependencies must be a TOML table",
            ));
        }
        // Do not replace a user-configured dependency on a repeated add.
        if cargo
            .get("dependencies")
            .and_then(|d| d.get(&key))
            .is_some()
        {
            return Err(Failure::new(
                "plugin_already_added",
                format!("{dependency} is already a dependency"),
            ));
        }
        if !features.is_empty() {
            entry["features"] = value(features.iter().cloned().collect::<toml_edit::Array>());
        }
        if cargo.get("dependencies").is_none() {
            cargo["dependencies"] = Item::Table(Table::new());
        }
        cargo["dependencies"]
            .as_table_like_mut()
            .expect("validated dependencies")
            .insert(&dependency, Item::Table(entry));
        if config.get("plugins").is_none() {
            config["plugins"] = Item::Table(Table::new());
        }
        let namespaces = config["plugins"]
            .as_table_like_mut()
            .ok_or_else(|| Failure::new("invalid_project_manifest", "plugins must be a table"))?;
        if !namespaces.contains_key(name) {
            namespaces.insert(name, Item::Table(Table::new()));
        }
    } else {
        let removed = cargo
            .get_mut("dependencies")
            .and_then(Item::as_table_like_mut)
            .and_then(|t| t.remove(&key));
        if removed.is_none() {
            return Err(Failure::new(
                "plugin_not_added",
                format!("{dependency} is not a direct dependency"),
            ));
        }
        remove_feature_references(&mut cargo, &key);
        remove_namespace(&mut config, name);
    }
    // Stage both files completely before replacing either authored manifest.
    let write_config = add || project.is_some();
    let mut cargo_file = Replacement::stage(&manifest, cargo.to_string().as_bytes())?;
    let mut project_file = if write_config {
        Some(Replacement::stage(
            &project_path,
            config.to_string().as_bytes(),
        )?)
    } else {
        None
    };
    cargo_file.install()?;
    // Cargo checks feature references and inherited workspace declarations.
    if let Err(error) = lifecycle::metadata(&manifest, false) {
        cargo_file.rollback()?;
        return Err(error);
    }
    if let Some(project_file) = &mut project_file {
        if let Err(error) = project_file.install() {
            cargo_file.rollback()?;
            return Err(error);
        }
        project_file.finished = true;
    }
    cargo_file.finished = true;
    Ok(
        json!({"manifest": manifest, "project_manifest": write_config.then_some(project_path), "plugin": name, "dependency": dependency, "plugin_path": selected_path, "features": features}),
    )
}

fn remove_namespace(doc: &mut DocumentMut, name: &str) {
    if let Some(plugins) = doc.get_mut("plugins").and_then(Item::as_table_like_mut) {
        plugins.remove(name);
    }
    if let Some(profiles) = doc.get_mut("profiles").and_then(Item::as_table_like_mut) {
        for (_, profile) in profiles.iter_mut() {
            if let Some(plugins) = profile.get_mut("plugins").and_then(Item::as_table_like_mut) {
                plugins.remove(name);
            }
        }
    }
}

pub(crate) fn document(path: &Path) -> Result<DocumentMut> {
    fs::read_to_string(path)
        .map_err(io_error)?
        .parse()
        .map_err(|e| Failure::new("invalid_cargo_manifest", format!("{}: {e}", path.display())))
}

fn remove_feature_references(doc: &mut DocumentMut, dependency: &str) {
    let Some(features) = doc.get_mut("features").and_then(Item::as_table_like_mut) else {
        return;
    };
    // An explicitly declared feature with the dependency's name remains valid;
    // implicit optional-dependency features disappear with their dependency.
    let explicit_feature = features.contains_key(dependency);
    for (_, feature) in features.iter_mut() {
        if let Some(values) = feature.as_array_mut() {
            values.retain(|value| {
                value.as_str().is_none_or(|name| {
                    name != format!("dep:{dependency}")
                        && !name.starts_with(&format!("{dependency}/"))
                        && !name.starts_with(&format!("{dependency}?/"))
                        && (name != dependency || explicit_feature)
                })
            });
        }
    }
}

/// An adjacent staged replacement and an inode backup for rollback without writes.
struct Replacement {
    path: PathBuf,
    temporary: PathBuf,
    backup: Option<PathBuf>,
    installed: bool,
    finished: bool,
}
impl Replacement {
    fn stage(path: &Path, contents: &[u8]) -> Result<Self> {
        use std::{
            io::Write,
            sync::atomic::{AtomicU64, Ordering},
        };
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let parent = path.parent().expect("manifest parent");
        let temporary = parent.join(format!(".rayengine-edit-{}-{id}.tmp", std::process::id()));
        // Exclusive creation never removes another writer's temporary file.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(io_error)?;
        let mut replacement = Self {
            path: path.to_path_buf(),
            temporary,
            backup: None,
            installed: false,
            finished: false,
        };
        if path.exists() {
            let backup = parent.join(format!(".rayengine-edit-{}-{id}.bak", std::process::id()));
            fs::hard_link(path, &backup).map_err(io_error)?;
            replacement.backup = Some(backup);
            file.set_permissions(fs::metadata(path).map_err(io_error)?.permissions())
                .map_err(io_error)?;
        }
        file.write_all(contents).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        Ok(replacement)
    }
    fn install(&mut self) -> Result<()> {
        fs::rename(&self.temporary, &self.path).map_err(io_error)?;
        self.installed = true;
        Ok(())
    }
    fn rollback(&mut self) -> Result<()> {
        if !self.installed {
            return Ok(());
        }
        let result = if let Some(backup) = &self.backup {
            fs::rename(backup, &self.path)
        } else {
            fs::remove_file(&self.path)
        };
        result.map_err(|error| {
            Failure::new(
                "io_failed",
                format!(
                    "could not restore {}: {error}; original remains at {}",
                    self.path.display(),
                    self.backup.as_deref().unwrap_or(&self.path).display()
                ),
            )
        })?;
        self.installed = false;
        Ok(())
    }
}
impl Drop for Replacement {
    fn drop(&mut self) {
        // Keep the backup if rollback itself failed, so original bytes survive.
        if self.installed && !self.finished && self.rollback().is_err() {
            return;
        }
        let _ = fs::remove_file(&self.temporary);
        if let Some(backup) = &self.backup {
            let _ = fs::remove_file(backup);
        }
    }
}
