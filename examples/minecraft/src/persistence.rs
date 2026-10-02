//! Game-owned snapshots in the engine's versioned save container, without a GPU.
use rayengine_core::{
    jobs::JobPoolError,
    save::{self, SaveError, SaveLimits, SaveOptions},
};
use rayengine_voxel::prelude::*;
use std::{
    fmt,
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
pub(crate) mod bounded;
mod schema;
mod session;
pub use schema::{PlayerState, Snapshot};
pub use session::{EditAdmission, SaveStatus, SavedTerrain, Saving};
/// Version of the game payload inside the independent rayengine container.
pub const SCHEMA_VERSION: u32 = 1;
/// Historical modified-chunk budget (8 MiB dense u16 cell payload).
pub const MAX_EDITED_CHUNKS: usize = 1024;
/// Maximum resident chunks copied at one snapshot point (1.25 MiB cells).
pub const MAX_SNAPSHOT_CHUNKS: usize = 160;
/// JSON payload budget; allocation stops at this bound during encoding/decoding.
pub const MAX_PAYLOAD_BYTES: usize = 12 * 1024 * 1024;
/// Identical admission for in-memory containers and filesystem reads/writes.
pub const LIMITS: SaveLimits = SaveLimits {
    max_payload_bytes: MAX_PAYLOAD_BYTES,
};
/// Container, schema, compatibility, admission and worker errors remain explicit.
#[derive(Debug)]
pub enum Error {
    /// Engine container/I/O error; retain its commit-state recovery information.
    Container(SaveError),
    /// Game JSON cannot be decoded or encoded.
    Json(serde_json::Error),
    /// Invalid game state or incompatible generator/registry.
    Invalid(String),
    /// Parent setup/sidecar lock error; no game slot was replaced.
    Io(io::Error),
    /// Another instance owns this save's advisory lock.
    Locked,
    /// Background worker admission/creation failure.
    Jobs(JobPoolError),
    /// Worker panic or cancellation; chunks remain dirty.
    Worker,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Container(e) => e.fmt(f),
            Self::Json(e) => write!(f, "Minecraft save JSON: {e}"),
            Self::Invalid(e) => write!(f, "Minecraft save: {e}"),
            Self::Io(e) => write!(f, "Minecraft save setup: {e}"),
            Self::Locked => f.write_str("Minecraft save is already open in another instance"),
            Self::Jobs(e) => e.fmt(f),
            Self::Worker => {
                f.write_str("Minecraft save worker failed; unsaved chunks remain loaded")
            }
        }
    }
}
impl std::error::Error for Error {}
impl From<SaveError> for Error {
    fn from(e: SaveError) -> Self {
        Self::Container(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<VoxelError> for Error {
    fn from(e: VoxelError) -> Self {
        Self::Invalid(e.to_string())
    }
}
/// One explicitly selected slot. A stable sidecar lock excludes concurrent game
/// instances, while save::save still replaces only the requested container.
/// The lock file remains on disk; its OS lock releases automatically on crash.
#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    options: SaveOptions,
    _lock: Arc<File>,
}
impl Store {
    /// Prepare/canonicalize the parent, lock the slot, and bound container I/O.
    /// Corrupt/future saves are never treated as missing by [`Self::load`].
    pub fn open(path: impl AsRef<Path>, mut options: SaveOptions) -> Result<Self, Error> {
        let path = path.as_ref();
        let name = path
            .file_name()
            .ok_or_else(|| Error::Invalid("save path must name a file".into()))?;
        if name.to_string_lossy().ends_with(".lock") {
            return Err(Error::Invalid("save filenames cannot end in .lock".into()));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        prepare_parent(parent, options.durability)?;
        let parent = fs::canonicalize(parent)?;
        let path = parent.join(name);
        reject_special(&path)?;
        let mut lock_name = name.to_os_string();
        lock_name.push(".lock");
        let lock_path = parent.join(lock_name);
        reject_special(&lock_path)?;
        let mut open = OpenOptions::new();
        open.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open.mode(0o600);
        }
        let lock = open.open(lock_path)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(Error::Locked),
            Err(TryLockError::Error(e)) => return Err(Error::Io(e)),
        }
        options.limits.max_payload_bytes = options.limits.max_payload_bytes.min(MAX_PAYLOAD_BYTES);
        Ok(Self {
            path,
            options,
            _lock: Arc::new(lock),
        })
    }
    /// Canonical parent plus the requested destination name.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Load only this named container; None means its file genuinely does not exist.
    pub fn load(&self) -> Result<Option<Snapshot>, Error> {
        match save::load(&self.path, self.options.limits) {
            Ok(data) => {
                data.require_schema(SCHEMA_VERSION)?;
                Ok(Some(Snapshot::decode(&data.payload)?))
            }
            Err(SaveError::Io {
                stage: save::SaveStage::Read,
                source,
            }) if source.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::Container(error)),
        }
    }
    /// Encode validated owned state and use the engine's same-directory replacement.
    /// Called by the background saver or once during final native-close shutdown.
    pub fn write(&self, snapshot: &Snapshot) -> Result<(), Error> {
        let payload = snapshot.encode_with_limit(self.options.limits.max_payload_bytes)?;
        save::save(&self.path, SCHEMA_VERSION, &payload, self.options)?;
        Ok(())
    }
}
fn reject_special(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => Err(Error::Invalid(
            "save and lock paths must be regular files, not symlinks".into(),
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn prepare_parent(parent: &Path, durability: save::Durability) -> Result<(), Error> {
    if durability == save::Durability::Durable && !cfg!(target_os = "linux") {
        return Err(Error::Container(SaveError::UnsupportedDurability));
    }
    // Record absent ancestors before creation; synchronize each new entry's parent
    // so a durable first save does not rely on unflushed mkdir directory entries.
    let absolute = if parent.is_absolute() {
        parent.to_owned()
    } else {
        std::env::current_dir()?.join(parent)
    };
    let mut missing = Vec::new();
    let mut p = absolute.as_path();
    while !p.exists() {
        missing.push(p.to_owned());
        p = p
            .parent()
            .ok_or_else(|| Error::Invalid("save parent has no existing ancestor".into()))?;
    }
    fs::create_dir_all(&absolute)?;
    if durability == save::Durability::Durable {
        for directory in missing.iter().rev() {
            File::open(directory)?.sync_all()?;
            File::open(directory.parent().unwrap())?.sync_all()?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
