use super::FontOptions;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// One named declaration, reusable by a full project manifest reader.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontDeclaration {
    /// Font file, relative to the project manifest before resolution.
    pub path: PathBuf,
    /// The same options used by direct SDK loading.
    #[serde(default)]
    pub options: FontOptions,
}
impl FontDeclaration {
    /// Validate options and resolve a path against an absolute manifest directory.
    /// Does not load native resources or require the font file to exist yet.
    pub fn resolve(&self, directory: &Path) -> Result<Self, Error> {
        self.options.validate()?;
        if self.path.as_os_str().is_empty() || self.path.to_string_lossy().contains('\0') {
            return Err(Error::Config(
                "font declaration path must be nonempty and contain no NUL".into(),
            ));
        }
        if !directory.is_absolute() {
            return Err(Error::Config(
                "font manifest directory must be absolute".into(),
            ));
        }
        Ok(Self {
            path: directory.join(&self.path),
            options: self.options.clone(),
        })
    }
}

/// Resolved named fonts from the optional version-1 `rayengine.toml` font section.
/// Other sections are left to the full project manifest reader (issue #44).
#[derive(Clone, Debug, Default)]
pub struct FontDeclarations {
    /// Names mapped to declarations with absolute paths.
    pub fonts: BTreeMap<String, FontDeclaration>,
}
impl FontDeclarations {
    /// Reads schema_version and fonts, resolving paths relative to the manifest.
    /// Unknown font/option fields and unsupported versions are errors. Other
    /// top-level sections are ignored; this is not full project validation.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().canonicalize()?;
        let source = std::fs::read_to_string(&path)?;
        Self::parse(&source, path.parent().expect("canonical file has parent"))
    }
    /// Parses declarations against an absolute manifest directory, without I/O.
    pub fn parse(source: &str, directory: &Path) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Section {
            schema_version: u32,
            #[serde(default)]
            fonts: BTreeMap<String, FontDeclaration>,
        }
        let section: Section =
            toml::from_str(source).map_err(|e| Error::Config(format!("font manifest: {e}")))?;
        if section.schema_version != 1 {
            return Err(Error::Config(format!(
                "unsupported font manifest schema_version {} (expected 1)",
                section.schema_version
            )));
        }
        let mut fonts = BTreeMap::new();
        for (name, declaration) in section.fonts {
            if name.trim().is_empty() {
                return Err(Error::Config("font name must be nonempty".into()));
            }
            let resolved = declaration
                .resolve(directory)
                .map_err(|e| Error::Config(format!("font '{name}': {e}")))?;
            fonts.insert(name, resolved);
        }
        Ok(Self { fonts })
    }
}
