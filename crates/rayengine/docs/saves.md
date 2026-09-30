# Versioned saves and file replacement

`rayengine::save` (also `rayengine_core::save`) stores opaque game-defined bytes
inside a bounded, versioned container. Games choose serialization, schema
versions, migration, save locations and recovery policy. It works without a
graphics context. JSON in the example is a game choice; binary codecs work too.

## Bytes and game codecs

```rust
use rayengine::save::{self, SaveLimits};
let limits = SaveLimits { max_payload_bytes: 1024 };
let bytes = save::encode(3, b"game-owned payload", limits)?;
let view = save::decode(&bytes, limits)?;
view.require_schema(3)?;
assert_eq!(view.payload, b"game-owned payload");
# Ok::<(), save::SaveError>(())
```

`decode` borrows its payload without allocating. `encode_into` reuses a vector;
`encode` creates a new one. `load` admits the header and actual regular-file
length before allocating one owned payload. All readers reject unsupported
engine formats, malformed lengths, trailing bytes, limit violations and CRC
failures before the game decoder runs. The default payload limit is 64 MiB;
zero permits empty payloads. Game codecs own any additional allocations.

`save_with` runs a game encoder before filesystem work. `load_with` validates
the container, then calls a decoder once with the stored game schema and bytes.
`CodecError::Container` distinguishes engine failures from
`CodecError::Payload`, which contains the game's serialization/migration error.
Use `require_schema` for a strict single-version game, or dispatch explicitly
in the decoder for migrations. Unknown game schemas must be handled by the game.
Migration does not rewrite files automatically.

The complete example below uses game-owned serde types, converts schema 1 to
schema 2, and starts a new game only for a missing save. Corrupt, undecodable
and future-version saves return errors without being overwritten. Its game
code deliberately commits a new schema-2 snapshot after successful loading.
Games using serde should declare their own `serde`/`serde_json` dependencies.

## Persistence and recovery

Parents must already exist. The writer creates a unique sibling with
`create_new`, writes the complete header/payload, and renames it over the
destination. It never truncates the old file. Temporary basenames do not depend
on destination length or UTF-8 encoding. Unix files use mode 0600; replacement
creates new metadata and replaces the destination name rather than following
an existing destination symlink.

`SaveOptions::default()` selects `Durability::Durable`: on Linux, sync the
complete temporary file, replace it, then sync its parent directory. A file
flush alone does not persist the directory entry, which requires a separate
directory flush. These guarantees depend on the filesystem/device honoring
synchronization. They describe local Linux filesystems, not arbitrary network
storage or storage that ignores flushes. See the [Linux fsync documentation](https://man7.org/linux/man-pages/man2/fsync.2.html).

`Durability::Atomic` skips both flushes. Readers of same-filesystem replacements
see a complete old or new file during normal operation; this mode makes no
power-loss durability promise. Rust exposes replacement through
[`std::fs::rename`](https://doc.rust-lang.org/std/fs/fn.rename.html).
Durable writes are explicitly rejected on non-Linux targets before temporary
creation; callers can opt into Atomic there, subject to platform rename/sharing
restrictions. Windows/macOS are optional and not tested here.

Use `SaveError::commit_state()` to choose recovery:

| State | Meaning | Game policy |
| --- | --- | --- |
| `NotCommitted` | Validation, creation, write or file-flush failed before replacement | Keep the last valid save; report or retry explicitly |
| `Unknown` | Rename returned an error | Reload/inspect before deciding retry; do not assume rollback |
| `Committed` | Rename succeeded; parent flush failed | New save is installed, but durability is unconfirmed; report that state |

Rename errors are conservatively ambiguous because a network server can finish
replacement before returning an error. [Linux documents this NFS behavior](https://man7.org/linux/man-pages/man2/rename.2.html).
The engine never rolls back a committed or ambiguous replacement. Another
writer may already have installed a newer snapshot.

Normal errors/unwinding clean up uncommitted siblings on a best-effort basis.
Process crashes or cleanup failures can leave temporary files. `load` reads
only the requested file; it does not scan siblings, promote interrupted saves,
or choose backups. Backup slots, corruption quarantine, and user recovery are
explicit game policies. Preserve unsupported saves rather than treating every
load error as a new game.

Games must keep parent paths stable during writes and avoid concurrent in-place
changes to a loaded file. Concurrent calls through this API have unique
temporaries and complete replacements; the last replacement wins, without
business-state locking or merging. When creating new parent directories, the
caller also owns durable directory creation/ancestor synchronization. The
engine deliberately does not create that directory tree.

Saving/checksumming and fsync can block. Take an owned snapshot at a game-defined
point and use an optional [CPU job](crate::guides::background_work) when needed;
GPU resources remain on their owning thread. Keep snapshots/payload sizes
bounded and decide how obsolete snapshots are handled before committing them.

## Container format 1

Integers are little-endian. CRC32 covers bytes 0–23 followed by the payload,
including the schema/declared length. It detects accidental corruption and does
not provide authentication. There is no compression or prescribed entity/world
schema.

| Byte offset | Size | Field |
| --- | ---: | --- |
| 0 | 8 | `RAYSAVE\0` magic |
| 8 | 4 | Engine format version, currently 1 |
| 12 | 4 | Game schema version (`u32`, game-defined) |
| 16 | 8 | Payload byte length |
| 24 | 4 | IEEE CRC32 of prefix + payload |
| 28 | Declared length | Opaque payload |

A fixed independently generated fixture tests byte compatibility. Limits also
reject sizes that overflow platform arithmetic before allocation.

## Runnable JSON/migration example

```sh
mkdir -p artifacts/save-example
cargo run -p rayengine --example save -- artifacts/save-example/slot.save
# Run again to load and update the same snapshot.
```

The source is `crates/rayengine/examples/save.rs`. Rustdoc includes and checks
that same source below. The example needs no window or graphics context.
