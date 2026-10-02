//! Versioned, optional project descriptions shared by tools and the runtime.
//!
//! Loading performs CPU/file-system work only. Native resources must still be
//! loaded explicitly on their owning render/audio thread.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Component, Path, PathBuf},
};

/// Current project schema version, independent of the SDK/crate version.
pub const SCHEMA_VERSION: u32 = 1;
/// Conventional manifest filename.
pub const FILE_NAME: &str = "rayengine.toml";
/// Open TOML namespace; games and plugins deserialize and validate their own data.
pub type Table = toml::Table;

/// Actionable project error, including the manifest path when loading a file.
#[derive(Debug)]
pub struct ManifestError(pub String);
impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ManifestError {}
type Result<T> = std::result::Result<T, ManifestError>;
fn error(message: impl Into<String>) -> ManifestError {
    ManifestError(message.into())
}

/// Project identity. Cargo still controls Rust packages and builds.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Project {
    /// Optional display/project name.
    pub name: Option<String>,
    /// Default Cargo binary target for `rayengine run`.
    pub executable: Option<String>,
}

/// Discovery roots, searched in declaration order; paths are manifest-relative.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AssetSettings {
    /// Directories containing assets. Defaults to `["assets"]`; `[]` disables discovery.
    pub roots: Vec<PathBuf>,
    /// Case-sensitive globs relative to each root; `*` cannot cross `/`, `**` can.
    pub exclude: Vec<String>,
}
impl Default for AssetSettings {
    fn default() -> Self {
        Self {
            roots: vec!["assets".into()],
            exclude: vec![],
        }
    }
}

/// Initial window settings.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct WindowSettings {
    /// Window title, without NUL.
    pub title: String,
    /// Logical dimensions, each in `1..=8192`.
    pub size: [u32; 2],
}
impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            title: "rayengine game".into(),
            size: [1280, 720],
        }
    }
}

/// Content scaling policy, matching the runtime viewport policies.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScaleMode {
    /// Preserve reference aspect ratio with bars.
    #[default]
    Fit,
    /// Expand the view to fill the window.
    Expand,
    /// Render at reference resolution and use integer scaling.
    IntegerFit,
}
/// Renderer defaults and supported offscreen world quality settings.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RenderSettings {
    /// Logical reference size, each finite and in `1..=8192`.
    pub reference_size: [f32; 2],
    /// Fit, expand, or integer-fit scaling.
    pub scale_mode: ScaleMode,
    /// World pixels per native content pixel in each dimension: 1 or 2.
    pub render_scale: f32,
    /// Offscreen world edge filter; native UI is composed afterwards.
    pub anti_aliasing: crate::quality::AntiAliasing,
    /// Framerate cap, `0..=1000`; zero is uncapped.
    pub target_fps: u32,
    /// Request driver display synchronization.
    pub vsync: bool,
    /// RGBA color outside the content viewport.
    pub bar_color: [u8; 4],
}
impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            reference_size: [960.0, 540.0],
            scale_mode: ScaleMode::Fit,
            render_scale: 1.0,
            anti_aliasing: crate::quality::AntiAliasing::None,
            target_fps: 120,
            vsync: true,
            bar_color: [9, 14, 24, 255],
        }
    }
}
/// Simulation/audio defaults.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeSettings {
    /// Fixed updates per second, `1..=1000`.
    pub fixed_hz: u32,
    /// Maximum simulation steps before rendering, `1..=1000`.
    pub max_catch_up: u32,
    /// Initialize audio before game initialization.
    pub audio: bool,
}
impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            fixed_hz: 120,
            max_catch_up: 8,
            audio: false,
        }
    }
}
/// Font atlas sampling policy, shared with the custom-font integration.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FontFilter {
    /// Smooth UI text.
    #[default]
    Linear,
    /// Intentional pixel-style text.
    Nearest,
}
/// Named font declaration. This describes a resource; it does not load an atlas.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FontDeclaration {
    /// Font file, resolved relative to the manifest, independently of asset roots.
    pub path: PathBuf,
    /// Base rasterization size, `1..=512`; default 32. DPI policy belongs to the font loader.
    #[serde(default = "font_size")]
    pub raster_size: u32,
    /// Atlas filtering; defaults to linear.
    #[serde(default)]
    pub filter: FontFilter,
    /// Optional Unicode scalar values; omitted means the loader's default coverage.
    pub glyphs: Option<Vec<u32>>,
}
fn font_size() -> u32 {
    32
}

/// Fully defaulted, validated settings for the base plus one selected profile.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Project identity and default Cargo target.
    pub project: Project,
    /// Asset selection rules.
    pub assets: AssetSettings,
    /// Window defaults.
    pub window: WindowSettings,
    /// Rendering defaults.
    pub render: RenderSettings,
    /// Simulation/audio defaults.
    pub runtime: RuntimeSettings,
    /// Font names map to declarations. Native loading remains explicit.
    pub fonts: BTreeMap<String, FontDeclaration>,
    /// Reserved packaging namespace; stored without acting on it.
    pub package: Table,
    /// Game-owned settings; the game validates these.
    pub game: Table,
    /// Explicit plugin namespaces, without a mandatory registry.
    pub plugins: BTreeMap<String, Table>,
}

/// Parsed manifest, with its canonical file location and validated profiles.
#[derive(Clone, Debug)]
pub struct ProjectManifest {
    path: PathBuf,
    base: Table,
    profiles: BTreeMap<String, Table>,
}
/// Resolved project description suitable for CLI inspection and runtime use.
#[derive(Clone, Debug, Serialize)]
pub struct ResolvedManifest {
    /// Version of the project schema.
    pub schema_version: u32,
    /// Canonical manifest path.
    pub path: PathBuf,
    /// Selected named profile, or `None` for the base.
    pub profile: Option<String>,
    /// Validated settings, including defaults and absolute asset/font paths.
    pub settings: Settings,
    #[serde(skip)]
    declared: Table,
}

impl ProjectManifest {
    /// Loads an explicitly named file; absence, invalid syntax and invalid profiles fail.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let requested = path.as_ref();
        let path = requested
            .canonicalize()
            .map_err(|e| error(format!("{}: {e}", requested.display())))?;
        let source =
            fs::read_to_string(&path).map_err(|e| error(format!("{}: {e}", path.display())))?;
        Self::parse_at(&source, path)
    }

    /// Loads a sibling `rayengine.toml` for a directory or Cargo manifest.
    /// An explicit `rayengine.toml` must exist. No upward search is performed.
    /// Only a missing optional sibling yields `None`; other I/O errors fail.
    pub fn load_optional(path: impl AsRef<Path>) -> Result<Option<Self>> {
        let path = path.as_ref();
        if path.file_name().is_some_and(|name| name == FILE_NAME) {
            return Self::load(path).map(Some);
        }
        if path.file_name().is_none_or(|name| name != "Cargo.toml") && !path.is_dir() {
            return Err(error(format!(
                "{}: expected a project directory, Cargo.toml, or rayengine.toml",
                path.display()
            )));
        }
        let manifest = if path.is_dir() {
            path.join(FILE_NAME)
        } else {
            path.parent().unwrap_or(Path::new(".")).join(FILE_NAME)
        };
        match fs::symlink_metadata(&manifest) {
            Ok(_) => Self::load(manifest).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(error(format!("{}: {e}", manifest.display()))),
        }
    }

    fn parse_at(source: &str, path: PathBuf) -> Result<Self> {
        let parse = || -> Result<Self> {
            let mut base: Table = toml::from_str(source).map_err(|e| error(e.to_string()))?;
            let version = base.remove("schema_version").and_then(|v| v.as_integer());
            if version != Some(i64::from(SCHEMA_VERSION)) {
                return Err(error(format!(
                    "unsupported or missing schema_version; expected {SCHEMA_VERSION}, got {version:?}"
                )));
            }
            let profiles = match base.remove("profiles") {
                None => BTreeMap::new(),
                Some(value) => value
                    .try_into::<BTreeMap<String, Table>>()
                    .map_err(|e| error(format!("profiles: {e}")))?,
            };
            let manifest = Self {
                path: path.clone(),
                base,
                profiles,
            };
            manifest.resolve_inner(None)?;
            for name in manifest.profiles.keys() {
                validate_name(name, "profile")?;
                let table = &manifest.profiles[name];
                if table.contains_key("project") {
                    return Err(error(format!(
                        "profiles.{name}.project: project identity cannot be overridden"
                    )));
                }
                manifest
                    .resolve_inner(Some(name))
                    .map_err(|e| error(format!("profiles.{name}: {e}")))?;
            }
            Ok(manifest)
        };
        parse().map_err(|e| error(format!("{}: {e}", path.display())))
    }

    /// Applies one profile over the shared base. Tables merge recursively;
    /// scalars and arrays replace. Profiles do not inherit from one another.
    pub fn resolve(&self, profile: Option<&str>) -> Result<ResolvedManifest> {
        self.resolve_inner(profile)
            .map_err(|e| error(format!("{}: {e}", self.path.display())))
    }
    fn resolve_inner(&self, profile: Option<&str>) -> Result<ResolvedManifest> {
        let mut declared = self.base.clone();
        if let Some(name) = profile {
            let overlay = self.profiles.get(name).ok_or_else(|| {
                error(format!(
                    "unknown profile `{name}`; available: {}",
                    self.profiles.keys().cloned().collect::<Vec<_>>().join(", ")
                ))
            })?;
            merge(&mut declared, overlay);
        }
        let mut settings: Settings = toml::Value::Table(declared.clone())
            .try_into()
            .map_err(|e| error(e.to_string()))?;
        validate(&settings)?;
        let root = self.path.parent().expect("canonical manifest has parent");
        for path in &mut settings.assets.roots {
            *path = resolve_path(root, path);
        }
        for font in settings.fonts.values_mut() {
            font.path = resolve_path(root, &font.path);
        }
        Ok(ResolvedManifest {
            schema_version: SCHEMA_VERSION,
            path: self.path.clone(),
            profile: profile.map(str::to_owned),
            settings,
            declared,
        })
    }
    /// Lists profile names in deterministic order.
    pub fn profiles(&self) -> impl Iterator<Item = &str> {
        self.profiles.keys().map(String::as_str)
    }
}
fn merge(base: &mut Table, overlay: &Table) {
    for (key, value) in overlay {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(base)), toml::Value::Table(overlay)) => merge(base, overlay),
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}
fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    // Do not canonicalize optional assets: declarations can precede file creation.
    // Preserve `..` so OS symlink traversal semantics are not changed.
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}
fn validate_name(name: &str, kind: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(error(format!(
            "{kind} name `{name}` must contain ASCII letters, digits, '-' or '_'"
        )));
    }
    Ok(())
}
fn validate_path(path: &Path, field: &str) -> Result<()> {
    if path.as_os_str().is_empty() || path.to_str().is_none_or(|s| s.contains('\0')) {
        return Err(error(format!(
            "{field}: expected a nonempty UTF-8 path without NUL"
        )));
    }
    Ok(())
}
fn exclusions(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if pattern.is_empty()
            || pattern.contains('\0')
            || pattern.contains('\\')
            || Path::new(pattern).is_absolute()
            || pattern.split('/').any(|part| part == "..")
        {
            return Err(error(format!(
                "assets.exclude `{pattern}`: expected a nonempty root-relative glob using '/'"
            )));
        }
        builder.add(
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .backslash_escape(false)
                .build()
                .map_err(|e| error(format!("assets.exclude `{pattern}`: {e}")))?,
        );
    }
    builder
        .build()
        .map_err(|e| error(format!("assets.exclude: {e}")))
}
fn validate(settings: &Settings) -> Result<()> {
    if let Some(name) = &settings.project.name
        && (name.trim().is_empty() || name.contains('\0'))
    {
        return Err(error(
            "project.name must be nonempty and cannot contain NUL",
        ));
    }
    if let Some(target) = &settings.project.executable {
        validate_name(target, "project.executable")?;
    }
    if settings.window.title.contains('\0') {
        return Err(error("window.title cannot contain NUL"));
    }
    if settings.window.size.iter().any(|n| !(1..=8192).contains(n)) {
        return Err(error("window.size dimensions must be between 1 and 8192"));
    }
    if settings
        .render
        .reference_size
        .iter()
        .any(|n| !n.is_finite() || !(1.0..=8192.0).contains(n))
    {
        return Err(error(
            "render.reference_size dimensions must be finite and between 1 and 8192",
        ));
    }
    let mode = match settings.render.scale_mode {
        ScaleMode::Fit => crate::viewport::ScaleMode::Fit,
        ScaleMode::Expand => crate::viewport::ScaleMode::Expand,
        ScaleMode::IntegerFit => crate::viewport::ScaleMode::IntegerFit,
    };
    let view = crate::viewport::Viewport::new(
        glam::Vec2::new(
            settings.window.size[0] as f32,
            settings.window.size[1] as f32,
        ),
        glam::Vec2::from_array(settings.render.reference_size),
        mode,
    )
    .expect("validated sizes");
    crate::quality::RenderQuality {
        render_scale: settings.render.render_scale,
        anti_aliasing: settings.render.anti_aliasing,
    }
    .plan(&view, glam::Vec2::ONE, mode)
    .map_err(|e| error(format!("render: {e}")))?;
    if settings.render.target_fps > 1000 {
        return Err(error("render.target_fps must be 0..=1000"));
    }
    if !(1..=1000).contains(&settings.runtime.fixed_hz)
        || !(1..=1000).contains(&settings.runtime.max_catch_up)
    {
        return Err(error(
            "runtime.fixed_hz and runtime.max_catch_up must be between 1 and 1000",
        ));
    }
    for path in &settings.assets.roots {
        validate_path(path, "assets.roots")?;
    }
    exclusions(&settings.assets.exclude)?;
    for (name, font) in &settings.fonts {
        validate_name(name, "font")?;
        validate_path(&font.path, &format!("fonts.{name}.path"))?;
        if !(1..=512).contains(&font.raster_size) {
            return Err(error(format!(
                "fonts.{name}.raster_size must be between 1 and 512"
            )));
        }
        if let Some(glyphs) = &font.glyphs
            && (glyphs.is_empty() || glyphs.iter().any(|&value| char::from_u32(value).is_none()))
        {
            return Err(error(format!(
                "fonts.{name}.glyphs must be a nonempty array of Unicode scalar values"
            )));
        }
    }
    for name in settings.plugins.keys() {
        validate_name(name, "plugin")?;
    }
    Ok(())
}

impl ResolvedManifest {
    /// Whether a field was explicitly supplied by the base/profile. Runtime
    /// adapters apply only declared fields over caller-supplied Rust defaults.
    pub fn is_declared(&self, section: &str, field: &str) -> bool {
        self.declared
            .get(section)
            .and_then(toml::Value::as_table)
            .is_some_and(|table| table.contains_key(field))
    }
    /// Finds the first matching regular file in root order. Exclusions also
    /// apply to lookup. Absolute/traversal names and symlinks are rejected.
    pub fn asset(&self, name: impl AsRef<Path>) -> Result<PathBuf> {
        let name = name.as_ref();
        validate_path(name, "asset name")?;
        if name
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(error("asset name must be root-relative without '..'"));
        }
        // Match and open the same logical spelling. Path::components removes
        // internal '.' and repeated separators; raw spellings could otherwise
        // bypass exclusions even though the OS opens the same file.
        let normalized: PathBuf = name
            .components()
            .filter(|c| !matches!(c, Component::CurDir))
            .collect();
        validate_path(&normalized, "asset name")?;
        let name = normalized.as_path();
        let excluded = exclusions(&self.settings.assets.exclude)?;
        if is_excluded(&excluded, name) {
            return Err(error(format!("asset `{}` is excluded", name.display())));
        }
        for root in &self.settings.assets.roots {
            let candidate = root.join(name);
            let mut path = root.clone();
            let mut found = true;
            let mut components = name.components().peekable();
            while let Some(component) = components.next() {
                path.push(component);
                match fs::symlink_metadata(&path) {
                    Ok(metadata)
                        if metadata.file_type().is_symlink()
                            || (components.peek().is_some() && !metadata.is_dir())
                            || (components.peek().is_none() && !metadata.is_file()) =>
                    {
                        found = false;
                        break;
                    }
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        found = false;
                        break;
                    }
                    Err(e) => return Err(error(format!("{}: {e}", path.display()))),
                }
            }
            if found {
                return Ok(candidate);
            }
        }
        Err(error(format!(
            "asset `{}` not found in declared roots (symlinks are skipped)",
            name.display()
        )))
    }
    /// Discovers regular files in root order, sorted within each root. Missing
    /// roots are allowed; links are skipped; duplicate logical names use the
    /// first root, matching [`Self::asset`]. No native resources are loaded.
    pub fn discover_assets(&self) -> Result<Vec<PathBuf>> {
        let excluded = exclusions(&self.settings.assets.exclude)?;
        let mut selected = BTreeMap::new();
        let mut output = vec![];
        for root in &self.settings.assets.roots {
            match fs::metadata(root) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(error(format!("{}: {e}", root.display()))),
                Ok(m) if !m.is_dir() => {
                    return Err(error(format!(
                        "asset root {} is not a directory",
                        root.display()
                    )));
                }
                Ok(_) => (),
            }
            let mut files = vec![];
            collect_files(root, root, &excluded, &mut files)?;
            files.sort();
            for file in files {
                let relative = file
                    .strip_prefix(root)
                    .expect("discovered below root")
                    .to_owned();
                if selected.insert(relative, ()).is_none() {
                    output.push(file);
                }
            }
        }
        Ok(output)
    }
}
fn is_excluded(excluded: &GlobSet, path: &Path) -> bool {
    // An excluded directory excludes its entire subtree as well.
    path.ancestors()
        .filter(|p| !p.as_os_str().is_empty())
        .any(|p| excluded.is_match(p))
}
fn collect_files(
    root: &Path,
    dir: &Path,
    excluded: &GlobSet,
    output: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| error(format!("{}: {e}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| error(format!("{}: {e}", dir.display())))?;
        let path = entry.path();
        if is_excluded(excluded, path.strip_prefix(root).expect("entry below root")) {
            continue;
        }
        let kind = entry
            .file_type()
            .map_err(|e| error(format!("{}: {e}", path.display())))?;
        if kind.is_dir() {
            collect_files(root, &path, excluded, output)?;
        } else if kind.is_file() {
            output.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
