//! Game-owned JSON serialization, explicit migration, and error-aware save loading.
//! Run with an existing parent directory: `cargo run -p rayengine --example save -- PATH`.
use rayengine::save::{self, CodecError, SaveError, SaveLimits, SaveOptions, SaveStage};
use serde::{Deserialize, Serialize};
use std::{fmt, io, path::PathBuf};

const SCHEMA: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
struct Progress {
    health: u32,
    position: [f32; 3],
}
#[derive(Deserialize)]
struct OldProgress {
    health: u16,
}

#[derive(Debug)]
enum DecodeError {
    Schema(u32),
    Json(serde_json::Error),
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(version) => write!(f, "unsupported game schema {version}"),
            Self::Json(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Schema(_) => None,
        }
    }
}

fn decode_progress(version: u32, bytes: &[u8]) -> Result<Progress, DecodeError> {
    match version {
        1 => {
            let old: OldProgress = serde_json::from_slice(bytes).map_err(DecodeError::Json)?;
            Ok(Progress {
                health: u32::from(old.health),
                position: [0.0; 3],
            })
        }
        SCHEMA => serde_json::from_slice(bytes).map_err(DecodeError::Json),
        other => Err(DecodeError::Schema(other)),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::args_os().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "supply a save path whose parent already exists",
        )
    })?);
    let mut progress = match save::load_with(&path, SaveLimits::default(), decode_progress) {
        Ok(progress) => progress,
        Err(CodecError::Container(SaveError::Io {
            stage: SaveStage::Read,
            source,
        })) if source.kind() == io::ErrorKind::NotFound => Progress {
            health: 100,
            position: [0.0; 3],
        },
        Err(error) => return Err(Box::new(error)), // Corrupt/future saves are preserved.
    };
    // The example game owns this update and chooses when to commit a new snapshot.
    progress.health = progress.health.saturating_sub(1);
    save::save_with(
        &path,
        SCHEMA,
        &progress,
        SaveOptions::default(),
        serde_json::to_vec,
    )?;
    println!("{progress:?}");
    Ok(())
}
