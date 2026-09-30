//! Versioned, checksummed containers for game-defined payloads.
//!
//! The engine validates its container; the game owns serialization, schema
//! versions, migration, and recovery policy. Filesystem writes are explicit and
//! optional. See [`save`] for replacement and durability behavior.

use std::{collections::TryReserveError, fmt, io};

mod file;
pub use file::{load, load_with, save, save_with};

/// Current engine container format, independent of a game's schema version.
pub const FORMAT_VERSION: u32 = 1;
/// Fixed header size: magic (8), format/schema (4 each), length (8), CRC32 (4).
pub const HEADER_LEN: usize = 28;
const MAGIC: &[u8; 8] = b"RAYSAVE\0";

/// Bounds container payload allocation and admission; game codecs can allocate
/// additional memory. The default limit is 64 MiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveLimits {
    /// Maximum admitted payload bytes, excluding the fixed header.
    pub max_payload_bytes: usize,
}
impl Default for SaveLimits {
    fn default() -> Self {
        Self {
            max_payload_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Explicit storage policy. Neither mode creates parent directories.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Durability {
    /// Same-directory replacement without fsync. No power-loss durability promise.
    Atomic,
    /// Sync the complete temporary file, replace, then sync its parent directory.
    /// Supported on Linux; other targets return an error before creating a file.
    #[default]
    Durable,
}

/// File-write limits and persistence policy. Defaults to Linux durable writes.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveOptions {
    /// Maximum serialized payload size.
    pub limits: SaveLimits,
    /// Whether to flush storage as part of replacement.
    pub durability: Durability,
}

/// Filesystem operation that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStage {
    /// Opening, inspecting, or reading a container.
    Read,
    /// Opening the existing parent directory before a durable write.
    OpenDirectory,
    /// Creating a unique sibling temporary file.
    CreateTemporary,
    /// Writing the header or payload to the temporary file.
    WriteTemporary,
    /// Flushing the temporary file before replacement.
    SyncTemporary,
    /// Replacing the destination name.
    Rename,
    /// Flushing the directory after successful replacement.
    SyncDirectory,
}

/// File replacement disposition when handling a save error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitState {
    /// Replacement was not attempted; the destination was not changed.
    NotCommitted,
    /// Replacement returned an error. Reload to resolve ambiguous filesystem outcomes.
    Unknown,
    /// Replacement succeeded, but directory durability confirmation failed.
    Committed,
}

/// Container validation or filesystem failure. No recovery is performed implicitly.
#[derive(Debug)]
pub enum SaveError {
    /// A header ended before all fixed fields were available.
    TruncatedHeader {
        /// Available header bytes.
        bytes: usize,
    },
    /// The file is not a rayengine save container.
    BadMagic,
    /// The engine cannot decode this container format.
    UnsupportedFormat {
        /// Recorded container version.
        found: u32,
    },
    /// A strict game schema check failed; migration is game-owned.
    UnsupportedSchema {
        /// Required game schema.
        expected: u32,
        /// Recorded game schema.
        found: u32,
    },
    /// Declared payload size exceeds the configured bound.
    TooLarge {
        /// Declared payload bytes.
        bytes: u64,
        /// Configured byte limit.
        limit: usize,
    },
    /// Header/payload size cannot be represented on this platform.
    SizeOverflow,
    /// Truncation or trailing data disagrees with the declared payload size.
    LengthMismatch {
        /// Declared payload bytes.
        declared: u64,
        /// Available payload bytes.
        actual: u64,
    },
    /// CRC32 of the header prefix and payload did not match.
    ChecksumMismatch {
        /// Stored checksum.
        expected: u32,
        /// Calculated checksum.
        actual: u32,
    },
    /// Destination must name a file, with an existing stable parent directory.
    InvalidPath,
    /// Loading requires a regular file.
    NotRegularFile,
    /// Linux durable semantics are unavailable on this target.
    UnsupportedDurability,
    /// Bounded container allocation failed.
    Allocation(TryReserveError),
    /// An operating-system operation failed.
    Io {
        /// Operation that failed.
        stage: SaveStage,
        /// Original operating-system error.
        source: io::Error,
    },
}

impl SaveError {
    /// Disposition for recovery after a save operation. Rename errors are
    /// conservatively ambiguous (notably on network filesystems). A directory
    /// sync error means the new container is already installed; never roll it
    /// back implicitly. Load/validation errors do not modify files.
    pub fn commit_state(&self) -> CommitState {
        match self {
            Self::Io {
                stage: SaveStage::Rename,
                ..
            } => CommitState::Unknown,
            Self::Io {
                stage: SaveStage::SyncDirectory,
                ..
            } => CommitState::Committed,
            _ => CommitState::NotCommitted,
        }
    }
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader { bytes } => {
                write!(f, "save header has {bytes} bytes; needs {HEADER_LEN}")
            }
            Self::BadMagic => f.write_str("invalid save magic"),
            Self::UnsupportedFormat { found } => write!(f, "unsupported save format {found}"),
            Self::UnsupportedSchema { expected, found } => {
                write!(f, "expected game schema {expected}, found {found}")
            }
            Self::TooLarge { bytes, limit } => {
                write!(f, "save payload {bytes} exceeds limit {limit}")
            }
            Self::SizeOverflow => f.write_str("save size exceeds platform limits"),
            Self::LengthMismatch { declared, actual } => {
                write!(f, "save declares {declared} payload bytes, found {actual}")
            }
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "save checksum mismatch: expected {expected:08x}, found {actual:08x}"
            ),
            Self::InvalidPath => f.write_str("save path must name a file"),
            Self::NotRegularFile => f.write_str("save source must be a regular file"),
            Self::UnsupportedDurability => f.write_str(
                "durable saves require Linux; choose Atomic explicitly on other targets",
            ),
            Self::Allocation(source) => write!(f, "save allocation: {source}"),
            Self::Io { stage, source } => write!(f, "save {stage:?}: {source}"),
        }
    }
}
impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Keeps game serialization/migration failures separate from engine failures.
#[derive(Debug)]
pub enum CodecError<E> {
    /// Container or filesystem failure, including replacement disposition.
    Container(SaveError),
    /// Game-defined encoder, decoder, or migration failure.
    Payload(E),
}
impl<E: fmt::Display> fmt::Display for CodecError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Container(e) => e.fmt(f),
            Self::Payload(e) => write!(f, "save payload codec: {e}"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for CodecError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Container(e) => Some(e),
            Self::Payload(e) => Some(e),
        }
    }
}

/// Validated borrowed payload. The engine does not interpret the game schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveRef<'a> {
    /// Game-owned payload schema version.
    pub schema_version: u32,
    /// Validated opaque bytes, borrowing the container.
    pub payload: &'a [u8],
}
impl SaveRef<'_> {
    /// Checks one supported schema. Games with migrations can dispatch on the
    /// recorded version instead, after container validation.
    pub fn require_schema(&self, expected: u32) -> Result<(), SaveError> {
        if self.schema_version == expected {
            Ok(())
        } else {
            Err(SaveError::UnsupportedSchema {
                expected,
                found: self.schema_version,
            })
        }
    }
}

/// Owned disk payload, allocated once after header/length admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveData {
    /// Game-owned payload schema version.
    pub schema_version: u32,
    /// Validated opaque payload bytes.
    pub payload: Vec<u8>,
}
impl SaveData {
    /// Borrows metadata/payload without copying.
    pub fn as_ref(&self) -> SaveRef<'_> {
        SaveRef {
            schema_version: self.schema_version,
            payload: &self.payload,
        }
    }
    /// Strict schema check; migration-capable games can inspect the version.
    pub fn require_schema(&self, expected: u32) -> Result<(), SaveError> {
        self.as_ref().require_schema(expected)
    }
}

/// Encodes a container into a new vector. Use [`encode_into`] to reuse storage.
pub fn encode(
    schema_version: u32,
    payload: &[u8],
    limits: SaveLimits,
) -> Result<Vec<u8>, SaveError> {
    let mut output = Vec::new();
    encode_into(&mut output, schema_version, payload, limits)?;
    Ok(output)
}

/// Encodes using reusable output storage. Admission/allocation failures preserve
/// its existing bytes. CRC covers the fixed prefix (including game schema and
/// payload length) followed by the payload; integers are little-endian.
pub fn encode_into(
    output: &mut Vec<u8>,
    schema_version: u32,
    payload: &[u8],
    limits: SaveLimits,
) -> Result<(), SaveError> {
    let header = make_header(schema_version, payload, limits)?;
    let total = HEADER_LEN
        .checked_add(payload.len())
        .ok_or(SaveError::SizeOverflow)?;
    output
        .try_reserve(total.saturating_sub(output.len()))
        .map_err(SaveError::Allocation)?;
    output.clear();
    output.extend_from_slice(&header);
    output.extend_from_slice(payload);
    Ok(())
}

/// Validates without allocation and borrows the opaque payload. Unknown engine
/// formats, truncation, trailing data, oversized payloads and bad checksums fail
/// before any game decoder runs.
pub fn decode(bytes: &[u8], limits: SaveLimits) -> Result<SaveRef<'_>, SaveError> {
    let header = bytes
        .get(..HEADER_LEN)
        .ok_or(SaveError::TruncatedHeader { bytes: bytes.len() })?;
    let (schema_version, size) = parse_header(header, limits)?;
    let payload = &bytes[HEADER_LEN..];
    if payload.len() != size {
        return Err(SaveError::LengthMismatch {
            declared: size as u64,
            actual: payload.len() as u64,
        });
    }
    verify_checksum(header, payload)?;
    Ok(SaveRef {
        schema_version,
        payload,
    })
}

fn make_header(
    schema: u32,
    payload: &[u8],
    limits: SaveLimits,
) -> Result<[u8; HEADER_LEN], SaveError> {
    let size = u64::try_from(payload.len()).map_err(|_| SaveError::SizeOverflow)?;
    if payload.len() > limits.max_payload_bytes {
        return Err(SaveError::TooLarge {
            bytes: size,
            limit: limits.max_payload_bytes,
        });
    }
    HEADER_LEN
        .checked_add(payload.len())
        .ok_or(SaveError::SizeOverflow)?;
    let mut header = [0; HEADER_LEN];
    header[..8].copy_from_slice(MAGIC);
    header[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    header[12..16].copy_from_slice(&schema.to_le_bytes());
    header[16..24].copy_from_slice(&size.to_le_bytes());
    let crc = checksum(&header[..24], payload);
    header[24..].copy_from_slice(&crc.to_le_bytes());
    Ok(header)
}

fn parse_header(header: &[u8], limits: SaveLimits) -> Result<(u32, usize), SaveError> {
    if &header[..8] != MAGIC {
        return Err(SaveError::BadMagic);
    }
    let format = u32::from_le_bytes(header[8..12].try_into().expect("fixed header"));
    if format != FORMAT_VERSION {
        return Err(SaveError::UnsupportedFormat { found: format });
    }
    let schema = u32::from_le_bytes(header[12..16].try_into().expect("fixed header"));
    let length = u64::from_le_bytes(header[16..24].try_into().expect("fixed header"));
    let size = usize::try_from(length).map_err(|_| SaveError::TooLarge {
        bytes: length,
        limit: limits.max_payload_bytes,
    })?;
    if size > limits.max_payload_bytes {
        return Err(SaveError::TooLarge {
            bytes: length,
            limit: limits.max_payload_bytes,
        });
    }
    HEADER_LEN
        .checked_add(size)
        .ok_or(SaveError::SizeOverflow)?;
    Ok((schema, size))
}

fn checksum(prefix: &[u8], payload: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(prefix);
    hasher.update(payload);
    hasher.finalize()
}
fn verify_checksum(header: &[u8], payload: &[u8]) -> Result<(), SaveError> {
    let expected = u32::from_le_bytes(header[24..28].try_into().expect("fixed header"));
    let actual = checksum(&header[..24], payload);
    if actual == expected {
        Ok(())
    } else {
        Err(SaveError::ChecksumMismatch { expected, actual })
    }
}

#[cfg(test)]
mod tests;
