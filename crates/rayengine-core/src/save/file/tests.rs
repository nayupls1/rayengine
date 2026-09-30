use super::*;
use std::{
    cell::Cell,
    sync::{Arc, Barrier},
    thread,
};

static NEXT_TEST: AtomicU64 = AtomicU64::new(0);
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        for _ in 0..128 {
            let id = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("rayengine-save-test-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("test directory: {e}"),
            }
        }
        panic!("test directory collision limit");
    }
    fn slot(&self) -> PathBuf {
        self.0.join("slot.save")
    }
    fn entries(&self) -> usize {
        fs::read_dir(&self.0).unwrap().count()
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn atomic() -> SaveOptions {
    SaveOptions {
        durability: Durability::Atomic,
        ..SaveOptions::default()
    }
}

#[test]
fn disk_round_trips_replace_and_load_only_the_named_file() {
    let dir = TestDir::new();
    let path = dir.slot();
    save(&path, 1, b"original", atomic()).unwrap();
    save(&path, 2, b"replacement", atomic()).unwrap();
    let data = load(&path, SaveLimits::default()).unwrap();
    assert_eq!(
        data,
        SaveData {
            schema_version: 2,
            payload: b"replacement".to_vec()
        }
    );
    data.require_schema(2).unwrap();
    assert!(data.require_schema(1).is_err());
    assert_eq!(dir.entries(), 1);
    fs::write(
        dir.0.join(".rayengine-save-leftover.tmp"),
        b"partial crash data",
    )
    .unwrap();
    assert_eq!(load(&path, SaveLimits::default()).unwrap(), data);
    assert!(
        matches!(load(dir.0.join("missing.save"), SaveLimits::default()), Err(SaveError::Io { stage: SaveStage::Read, source }) if source.kind() == io::ErrorKind::NotFound)
    );
    assert_eq!(dir.entries(), 2); // No implicit recovery/deletion of leftovers.
}

#[test]
fn corrupt_and_oversized_files_fail_before_the_game_decoder_runs() {
    let dir = TestDir::new();
    let called = Cell::new(false);
    let mut bytes = encode(4, b"data", SaveLimits::default()).unwrap();
    bytes[HEADER_LEN] ^= 1;
    fs::write(dir.slot(), &bytes).unwrap();
    let error = load_with(
        dir.slot(),
        SaveLimits::default(),
        |_, _| -> Result<(), io::Error> {
            called.set(true);
            Ok(())
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CodecError::Container(SaveError::ChecksumMismatch { .. })
    ));
    assert!(!called.get());
    bytes = encode(4, b"data", SaveLimits::default()).unwrap();
    bytes[16..24].copy_from_slice(&(64_u64 * 1024 * 1024).to_le_bytes());
    bytes.truncate(HEADER_LEN);
    fs::write(dir.slot(), &bytes).unwrap();
    assert!(matches!(
        load(dir.slot(), SaveLimits::default()),
        Err(SaveError::LengthMismatch {
            declared: 67_108_864,
            actual: 0
        })
    ));
    bytes[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    fs::write(dir.slot(), &bytes).unwrap();
    assert!(matches!(
        load(dir.slot(), SaveLimits::default()),
        Err(SaveError::TooLarge { .. })
    ));
    bytes.truncate(3);
    fs::write(dir.slot(), &bytes).unwrap();
    assert!(matches!(
        load(dir.slot(), SaveLimits::default()),
        Err(SaveError::TruncatedHeader { bytes: 3 })
    ));
}

#[test]
fn codec_errors_preserve_existing_data_and_migrations_are_explicit() {
    let dir = TestDir::new();
    save_with(
        dir.slot(),
        1,
        &42_u16,
        atomic(),
        |value| -> Result<_, io::Error> { Ok(value.to_le_bytes().to_vec()) },
    )
    .unwrap();
    let original = fs::read(dir.slot()).unwrap();
    let failure = save_with(
        dir.slot(),
        2,
        &99_u32,
        atomic(),
        |_| -> Result<Vec<u8>, io::Error> { Err(io::Error::other("game encoder failed")) },
    )
    .unwrap_err();
    assert!(matches!(failure, CodecError::Payload(_)));
    assert_eq!(fs::read(dir.slot()).unwrap(), original);
    assert_eq!(dir.entries(), 1);
    let health: u32 = load_with(
        dir.slot(),
        SaveLimits::default(),
        |schema, bytes| -> Result<_, SaveError> {
            match schema {
                1 => Ok(u32::from(u16::from_le_bytes(bytes.try_into().unwrap()))),
                2 => Ok(u32::from_le_bytes(bytes.try_into().unwrap())),
                found => Err(SaveError::UnsupportedSchema { expected: 2, found }),
            }
        },
    )
    .unwrap();
    assert_eq!(health, 42);
    assert_eq!(fs::read(dir.slot()).unwrap(), original); // Migration does not auto-write.
    save_with(
        dir.slot(),
        2,
        &health,
        atomic(),
        |value| -> Result<_, io::Error> { Ok(value.to_le_bytes().to_vec()) },
    )
    .unwrap();
    let unsupported = load_with(
        dir.slot(),
        SaveLimits::default(),
        |found, _| -> Result<(), SaveError> {
            Err(SaveError::UnsupportedSchema { expected: 9, found })
        },
    )
    .unwrap_err();
    assert!(matches!(
        unsupported,
        CodecError::Payload(SaveError::UnsupportedSchema { found: 2, .. })
    ));
    let before = fs::read(dir.slot()).unwrap();
    let failure = save(
        dir.slot(),
        9,
        b"too big",
        SaveOptions {
            limits: SaveLimits {
                max_payload_bytes: 1,
            },
            ..atomic()
        },
    )
    .unwrap_err();
    assert_eq!(failure.commit_state(), CommitState::NotCommitted);
    assert_eq!(fs::read(dir.slot()).unwrap(), before);
}

enum Fault {
    Before(SaveStage),
    RenameAfter,
    None,
}
struct FaultOps {
    fault: Fault,
    events: Vec<SaveStage>,
}
impl FaultOps {
    fn record(&mut self, stage: SaveStage) -> io::Result<()> {
        self.events.push(stage);
        if matches!(self.fault, Fault::Before(fail) if fail == stage) {
            Err(io::Error::other("injected filesystem failure"))
        } else {
            Ok(())
        }
    }
}
impl FileOps for FaultOps {
    fn write(&mut self, file: &mut File, header: &[u8], payload: &[u8]) -> io::Result<()> {
        if matches!(self.fault, Fault::Before(SaveStage::WriteTemporary)) {
            file.write_all(header)?;
            file.write_all(&payload[..payload.len().min(3)])?;
        }
        self.record(SaveStage::WriteTemporary)?;
        NativeOps.write(file, header, payload)
    }
    fn sync_file(&mut self, file: &File) -> io::Result<()> {
        self.record(SaveStage::SyncTemporary)?;
        NativeOps.sync_file(file)
    }
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        self.record(SaveStage::Rename)?;
        NativeOps.rename(from, to)?;
        if matches!(self.fault, Fault::RenameAfter) {
            Err(io::Error::other("rename completed before server error"))
        } else {
            Ok(())
        }
    }
    fn sync_directory(&mut self, directory: &File) -> io::Result<()> {
        self.record(SaveStage::SyncDirectory)?;
        NativeOps.sync_directory(directory)
    }
}

#[test]
fn partial_write_and_rename_errors_preserve_the_last_valid_file_and_clean_siblings() {
    for stage in [SaveStage::WriteTemporary, SaveStage::Rename] {
        let dir = TestDir::new();
        save(dir.slot(), 1, b"last valid", atomic()).unwrap();
        let original = fs::read(dir.slot()).unwrap();
        let mut ops = FaultOps {
            fault: Fault::Before(stage),
            events: Vec::new(),
        };
        let error = save_using(&dir.slot(), 2, b"new payload", atomic(), &mut ops).unwrap_err();
        assert!(matches!(&error, SaveError::Io { stage: failed, .. } if *failed == stage));
        assert_eq!(
            error.commit_state(),
            if stage == SaveStage::Rename {
                CommitState::Unknown
            } else {
                CommitState::NotCommitted
            }
        );
        assert_eq!(fs::read(dir.slot()).unwrap(), original);
        assert_eq!(
            load(dir.slot(), SaveLimits::default()).unwrap().payload,
            b"last valid"
        );
        assert_eq!(dir.entries(), 1);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn durable_flush_failures_distinguish_preserved_from_committed_saves() {
    for stage in [SaveStage::SyncTemporary, SaveStage::SyncDirectory] {
        let dir = TestDir::new();
        save(dir.slot(), 1, b"old", SaveOptions::default()).unwrap();
        let mut ops = FaultOps {
            fault: Fault::Before(stage),
            events: Vec::new(),
        };
        let error =
            save_using(&dir.slot(), 2, b"new", SaveOptions::default(), &mut ops).unwrap_err();
        let data = load(dir.slot(), SaveLimits::default()).unwrap();
        if stage == SaveStage::SyncTemporary {
            assert_eq!(error.commit_state(), CommitState::NotCommitted);
            assert_eq!(data.payload, b"old");
            assert_eq!(
                ops.events,
                [SaveStage::WriteTemporary, SaveStage::SyncTemporary]
            );
        } else {
            assert_eq!(error.commit_state(), CommitState::Committed);
            assert_eq!(data.payload, b"new");
            assert_eq!(
                ops.events,
                [
                    SaveStage::WriteTemporary,
                    SaveStage::SyncTemporary,
                    SaveStage::Rename,
                    SaveStage::SyncDirectory
                ]
            );
        }
        assert_eq!(dir.entries(), 1);
    }
    let dir = TestDir::new();
    let mut ops = FaultOps {
        fault: Fault::None,
        events: Vec::new(),
    };
    save_using(
        &dir.slot(),
        1,
        b"complete",
        SaveOptions::default(),
        &mut ops,
    )
    .unwrap();
    assert_eq!(
        ops.events,
        [
            SaveStage::WriteTemporary,
            SaveStage::SyncTemporary,
            SaveStage::Rename,
            SaveStage::SyncDirectory
        ]
    );
}

#[test]
fn a_rename_error_can_report_an_already_replaced_file_without_rollback() {
    let dir = TestDir::new();
    save(dir.slot(), 1, b"old", atomic()).unwrap();
    let mut ops = FaultOps {
        fault: Fault::RenameAfter,
        events: Vec::new(),
    };
    let error = save_using(&dir.slot(), 2, b"new", atomic(), &mut ops).unwrap_err();
    assert_eq!(error.commit_state(), CommitState::Unknown);
    assert_eq!(
        load(dir.slot(), SaveLimits::default()).unwrap().payload,
        b"new"
    );
    assert_eq!(dir.entries(), 1);
}

#[test]
fn concurrent_replacements_never_expose_torn_payloads() {
    let dir = TestDir::new();
    let path = dir.slot();
    save(&path, 0, &[0; 1024], atomic()).unwrap();
    let barrier = Arc::new(Barrier::new(5));
    thread::scope(|scope| {
        for value in 1..=4_u8 {
            let barrier = Arc::clone(&barrier);
            let path = &path;
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..16 {
                    save(path, u32::from(value), &[value; 1024], atomic()).unwrap();
                }
            });
        }
        barrier.wait();
        for _ in 0..256 {
            let data = load(&path, SaveLimits::default()).unwrap();
            assert_eq!(data.payload.len(), 1024);
            assert!(
                data.payload
                    .iter()
                    .all(|&v| u32::from(v) == data.schema_version)
            );
            assert!(data.schema_version <= 4);
        }
    });
    assert_eq!(dir.entries(), 1);
}

#[test]
fn path_and_read_errors_are_explicit_and_parents_are_not_created() {
    let dir = TestDir::new();
    assert!(matches!(
        save(Path::new("."), 1, b"x", atomic()),
        Err(SaveError::InvalidPath)
    ));
    assert!(matches!(
        load(&dir.0, SaveLimits::default()),
        Err(SaveError::NotRegularFile)
    ));
    assert!(
        matches!(save(dir.0.join("missing/slot.save"), 1, b"x", atomic()), Err(SaveError::Io { stage: SaveStage::CreateTemporary, source }) if source.kind() == io::ErrorKind::NotFound)
    );
    assert_eq!(dir.entries(), 0);
}

#[cfg(unix)]
#[test]
fn unix_replacements_use_private_permissions_and_replace_the_name_not_a_symlink_target() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = TestDir::new();
    let original = dir.0.join("original.save");
    save(&original, 1, b"untouched", atomic()).unwrap();
    symlink(&original, dir.slot()).unwrap();
    save(dir.slot(), 2, b"replacement", atomic()).unwrap();
    assert_eq!(
        load(&original, SaveLimits::default()).unwrap().payload,
        b"untouched"
    );
    assert!(
        !fs::symlink_metadata(dir.slot())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::metadata(dir.slot()).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(target_os = "linux")]
#[test]
fn long_non_utf8_destination_names_do_not_expand_the_temporary_basename() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let dir = TestDir::new();
    let path = dir.0.join(OsString::from_vec(vec![0xff; 255]));
    save(&path, 1, b"bytes", SaveOptions::default()).unwrap();
    assert_eq!(load(path, SaveLimits::default()).unwrap().payload, b"bytes");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn unsupported_durability_rejects_without_touching_the_target() {
    let dir = TestDir::new();
    save(dir.slot(), 1, b"original", atomic()).unwrap();
    let failure = save(dir.slot(), 2, b"new", SaveOptions::default()).unwrap_err();
    assert!(matches!(failure, SaveError::UnsupportedDurability));
    assert_eq!(
        load(dir.slot(), SaveLimits::default()).unwrap().payload,
        b"original"
    );
    assert_eq!(dir.entries(), 1);
}
