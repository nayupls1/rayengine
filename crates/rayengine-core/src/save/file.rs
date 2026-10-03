use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Reads only the named regular file. Limits and actual file length are checked
/// before payload allocation; checksum validation precedes game decoding.
/// Leftover temporary files are never discovered or promoted automatically.
pub fn load(path: impl AsRef<Path>, limits: SaveLimits) -> Result<SaveData, SaveError> {
    let mut file = File::open(path).map_err(|source| io_error(SaveStage::Read, source))?;
    let metadata = file
        .metadata()
        .map_err(|source| io_error(SaveStage::Read, source))?;
    if !metadata.is_file() {
        return Err(SaveError::NotRegularFile);
    }
    let mut header = [0; HEADER_LEN];
    let count = read_complete(&mut file, &mut header)?;
    if count != HEADER_LEN {
        return Err(SaveError::TruncatedHeader { bytes: count });
    }
    let (schema_version, size) = parse_header(&header, limits)?;
    let total = (size as u64) + HEADER_LEN as u64;
    if metadata.len() != total {
        return Err(SaveError::LengthMismatch {
            declared: size as u64,
            actual: metadata.len().saturating_sub(HEADER_LEN as u64),
        });
    }
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(size)
        .map_err(SaveError::Allocation)?;
    payload.resize(size, 0);
    let count = read_complete(&mut file, &mut payload)?;
    if count != size {
        return Err(SaveError::LengthMismatch {
            declared: size as u64,
            actual: count as u64,
        });
    }
    if read_complete(&mut file, &mut [0])? != 0 {
        return Err(SaveError::LengthMismatch {
            declared: size as u64,
            actual: size as u64 + 1,
        });
    }
    verify_checksum(&header, &payload)?;
    Ok(SaveData {
        schema_version,
        payload,
    })
}

/// Reads/validates, then calls the game decoder once with its schema and bytes.
/// This is the migration hook: reject unsupported schemas or convert older
/// payloads explicitly. Decoder errors never trigger a write or fallback.
pub fn load_with<T, E>(
    path: impl AsRef<Path>,
    limits: SaveLimits,
    decode: impl FnOnce(u32, &[u8]) -> Result<T, E>,
) -> Result<T, CodecError<E>> {
    let data = load(path, limits).map_err(CodecError::Container)?;
    decode(data.schema_version, &data.payload).map_err(CodecError::Payload)
}

/// Encodes the game's payload before touching the filesystem, then writes it.
/// The byte limit applies after serialization; the game's encoder owns its
/// allocation policy. Serialization errors preserve the existing destination.
pub fn save_with<T, E>(
    path: impl AsRef<Path>,
    schema_version: u32,
    value: &T,
    options: SaveOptions,
    encode: impl FnOnce(&T) -> Result<Vec<u8>, E>,
) -> Result<(), CodecError<E>> {
    let payload = encode(value).map_err(CodecError::Payload)?;
    save(path, schema_version, &payload, options).map_err(CodecError::Container)
}

/// Writes a validated container to a unique sibling, then replaces the name.
/// The destination is never truncated. Parent directories must already exist.
/// Linux durable writes sync the file before rename and the parent after it.
/// Failures before rename preserve the destination; rename errors are reported
/// as ambiguous, and a directory-sync error reports an already-installed save.
/// Check [`SaveError::commit_state`] before deciding recovery. Concurrent saves
/// use unique temporaries; last replacement wins, without game-state locking.
/// Temporary cleanup is best effort on error/unwind; process crashes can leave
/// uncommitted siblings. Unix files use mode 0600; old metadata is not copied.
pub fn save(
    path: impl AsRef<Path>,
    schema_version: u32,
    payload: &[u8],
    options: SaveOptions,
) -> Result<(), SaveError> {
    save_using(
        path.as_ref(),
        schema_version,
        payload,
        options,
        &mut NativeOps,
    )
}

fn save_using(
    path: &Path,
    schema: u32,
    payload: &[u8],
    options: SaveOptions,
    ops: &mut impl FileOps,
) -> Result<(), SaveError> {
    let header = make_header(schema, payload, options.limits)?;
    if path.file_name().is_none() {
        return Err(SaveError::InvalidPath);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = open_directory(parent, options.durability)?;
    let mut temp = Temporary::create(parent)?;
    let file = temp.file.as_mut().expect("temporary open");
    ops.write(file, &header, payload)
        .map_err(|source| io_error(SaveStage::WriteTemporary, source))?;
    if options.durability == Durability::Durable {
        ops.sync_file(file)
            .map_err(|source| io_error(SaveStage::SyncTemporary, source))?;
    }
    // Close before rename for platforms whose open handles prevent replacement.
    drop(temp.file.take());
    ops.rename(&temp.path, path)
        .map_err(|source| io_error(SaveStage::Rename, source))?;
    temp.committed = true;
    if let Some(directory) = directory {
        ops.sync_directory(&directory)
            .map_err(|source| io_error(SaveStage::SyncDirectory, source))?;
    }
    Ok(())
}

fn open_directory(parent: &Path, durability: Durability) -> Result<Option<File>, SaveError> {
    if durability == Durability::Atomic {
        return Ok(None);
    }
    #[cfg(target_os = "linux")]
    {
        File::open(parent)
            .map(Some)
            .map_err(|source| io_error(SaveStage::OpenDirectory, source))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = parent;
        Err(SaveError::UnsupportedDurability)
    }
}

fn io_error(stage: SaveStage, source: io::Error) -> SaveError {
    SaveError::Io { stage, source }
}

fn read_complete(file: &mut File, output: &mut [u8]) -> Result<usize, SaveError> {
    let mut count = 0;
    while count < output.len() {
        match file.read(&mut output[count..]) {
            Ok(0) => break,
            Ok(bytes) => count += bytes,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(source) => return Err(io_error(SaveStage::Read, source)),
        }
    }
    Ok(count)
}

struct Temporary {
    path: PathBuf,
    file: Option<File>,
    committed: bool,
}
impl Temporary {
    fn create(parent: &Path) -> Result<Self, SaveError> {
        for _ in 0..128 {
            // `try_update` is newer than our Rust 1.89 minimum.
            #[allow(deprecated)]
            let id = NEXT_TEMP
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .map_err(|_| {
                    io_error(
                        SaveStage::CreateTemporary,
                        io::Error::other("temporary name space exhausted"),
                    )
                })?;
            // Independent of destination length/encoding, including Unix byte paths.
            let path = parent.join(format!(
                ".rayengine-save-{}-{id:016x}.tmp",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                        committed: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(io_error(SaveStage::CreateTemporary, source)),
            }
        }
        Err(io_error(
            SaveStage::CreateTemporary,
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "temporary file collision limit exceeded",
            ),
        ))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

// Monomorphized production operations, with a private test seam for real-file
// partial write/fsync/rename errors. No public test hooks or virtual dispatch.
trait FileOps {
    fn write(&mut self, file: &mut File, header: &[u8], payload: &[u8]) -> io::Result<()>;
    fn sync_file(&mut self, file: &File) -> io::Result<()>;
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()>;
    fn sync_directory(&mut self, directory: &File) -> io::Result<()>;
}
struct NativeOps;
impl FileOps for NativeOps {
    fn write(&mut self, file: &mut File, header: &[u8], payload: &[u8]) -> io::Result<()> {
        file.write_all(header)?;
        file.write_all(payload)
    }
    fn sync_file(&mut self, file: &File) -> io::Result<()> {
        file.sync_all()
    }
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
    fn sync_directory(&mut self, directory: &File) -> io::Result<()> {
        directory.sync_all()
    }
}

#[cfg(test)]
mod tests;
