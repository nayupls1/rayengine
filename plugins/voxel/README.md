# rayengine-voxel

CPU-only voxel definitions, bounded **16³ chunks**, signed grid coordinates,
and budgeted ray traversal. This optional plugin lives under `plugins/voxel/`
and depends only on `rayengine-core`: no raylib, native build toolchain, window,
audio device, or graphics context. It supplies ordinary typed methods; SDK
lifecycle/rendering adapters can be added when geometry and streaming land.
The engine core has no dependency on this package.

Add it to a game with a path dependency from this checkout:

```toml
[dependencies]
rayengine-voxel = { path = "../rayengine/plugins/voxel" }
```

## Definitions and a loaded world

```rust
use rayengine_voxel::prelude::*;
use std::sync::Arc;

let mut definitions = BlockRegistry::new();
let mut stone = BlockDef::new("demo:stone");
stone.hardness = Some(2.0);
stone.textures = [TileId(3); 6];
let stone = definitions.register(stone)?;
let definitions = Arc::new(definitions);
let mut world = VoxelWorld::new(definitions.clone(), 8);
world.insert_chunk(ChunkPos::new(-1, 0, 0),
    Chunk::filled(definitions.clone(), BlockId::AIR)?)?;
let edit = world.set_block(BlockPos::new(-1, 2, 3), stone)?.unwrap();
assert_eq!(world.block(edit.position), Some(stone));
assert_eq!(edit.affected_chunks.as_slice(),
    &[ChunkPos::new(-1, 0, 0), ChunkPos::new(0, 0, 0)]);
assert_eq!(world.block(BlockPos::new(0, 2, 3)), None); // Unloaded, not air.
# Ok::<(), Box<dyn std::error::Error>>(())
```

[`BlockRegistry`] reserves [`BlockId::AIR`] at zero. Other IDs are 16-bit indices
in registration order, with 65,536 total definitions including air. Names are
unique ASCII keys of 1..=128 bytes. Hardness must be finite/nonnegative or `None`
(unbreakable). Collision and rendering are independent: a cutout plant can be
noncolliding, and an invisible barrier can collide. [`BlockDef::textures`] holds
six game-defined [`TileId`] values in [`Face::ALL`] order: -X,+X,-Y,+Y,-Z,+Z.
Tile keys are not GPU handles; the game/renderer supplies their atlas mapping.
Properties remain immutable after registration. Shared `Arc` registries allow
CPU generation/query jobs without copying definitions.

The type is named `VoxelWorld` to coexist with the engine ECS `World` in game imports.

Game-specific drops, recipes, tools, growth, and health belong in separate tables
keyed by block IDs. For serialization, preserve the registry order or implement
name/version mapping in the game; this crate does not silently remap numeric IDs.
All resident chunks must share the **same Arc allocation** as the world, even if
a separately created registry happens to have equal definitions.

## Coordinates, layout, and bounds

Positive Y points up. Blocks occupy half-open unit cells. [`BlockPos::split`]
uses Euclidean division: world `(-1,-16,-17)` becomes chunk `(-1,-1,-2)` and local
`(15,0,15)`. Local positions are validated and cannot be constructed out of range.
[`ChunkPos::origin`] / [`ChunkPos::block`] reject overflow. The entire i32 cell
grid is representable, with chunks -134217728 through 134217727 on each axis.

Dense ordering is **X fastest, then Z, then Y**: `x + 16*z + 256*y`.
[`LocalPos::from_index`] is its inverse. [`Chunk::blocks`] exposes read-only data
for meshing/snapshots. Imported buffers must contain exactly 4096 registered
IDs; unused Vec capacity is discarded by boxing. Cell payload is 8192 bytes per
chunk, excluding allocator, metadata, map, registry, snapshots, and GPU overhead.
Chunk dimensions/layout are public conventions to account for in future saves.

[`VoxelWorld`] admits at most the configured number of resident chunks. Zero capacity
is supported. Reads, edits, and traversal do not allocate or implicitly create
chunks. Constructors/insertions allocate explicitly and report admission errors.
A rejected [`VoxelWorld::insert_chunk`] returns [`ChunkInsertError`] containing the
incoming data; existing residents are preserved. A successful replacement returns
the old chunk, including dirty state. Removal also returns its data. The caller
owns save/eviction policy; there is no implicit discard, I/O, or streaming.
The map may retain allocation from its bounded high-water mark after removal.

## Edits, invalidation, and asynchronous snapshots

Actual content changes increment a chunk's u64 revision. No-op edits leave
revision/dirty state unchanged; unknown IDs or counter exhaustion reject the edit
before changing anything. New/imported chunks start dirty. [`VoxelWorld::set_block`]
returns the previous/current IDs, new [`ChunkStamp`], and a fixed-size list of
owner plus touched face-neighbor chunks. Neighbors may be unloaded; their cell
content/save dirtiness does not change. This is a **face dependency** notification,
not full lighting/AO propagation: future systems may need diagonal/distant updates.

A stamp combines content revision with a monotonic installation generation.
Remove/reinsert and replacement get fresh generations, preventing stale work
from matching a different resident with the same revision. Stamps are local to
one `VoxelWorld`, not universal IDs or a persistence schema. Mesh jobs depending on
neighbors must validate those neighbors' stamps as well.

[`VoxelWorld::mark_saved`] clears dirty state only when the complete supplied stamp
still matches. Older edits and older resident generations cannot clear new work.
Call it only after a corresponding snapshot actually persisted. Offline chunks
provide revision-only acknowledgement through [`Chunk::mark_saved`]. A newly
imported, known-persisted chunk can be acknowledged at revision zero explicitly.
Changing a cell back to its old value still advances its revision and stays dirty.

## Grid queries and selection policy

```rust
use rayengine_voxel::{glam::DVec3, prelude::*};
use std::sync::Arc;
let mut definitions = BlockRegistry::new();
let stone = definitions.register(BlockDef::new("demo:stone"))?;
let definitions = Arc::new(definitions);
let mut world = VoxelWorld::new(definitions.clone(), 1);
world.insert_chunk(ChunkPos::default(), Chunk::filled(definitions, BlockId::AIR)?)?;
world.set_block(BlockPos::new(3, 1, 1), stone)?;
let ray = GridRay::new(DVec3::new(0.5, 1.5, 1.5), DVec3::X)?;
let result = world.raycast(ray, RaycastOptions::default(),
    |_, block| block.collision == CollisionKind::Solid)?;
let RaycastOutcome::Hit(hit) = result.outcome else { panic!("expected stone"); };
assert_eq!(hit.position, BlockPos::new(3, 1, 1));
assert_eq!(hit.distance, 2.5);
assert_eq!(hit.face, Some(Face::NegX));
assert_eq!(hit.adjacent, Some(BlockPos::new(2, 1, 1)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

[`GridRay`] normalizes finite nonzero directions using f64 math. f64 retains cell
precision at all i32 grid positions. `GridRay::try_from(core::spatial::Ray3)` adapts
the engine's camera ray; conversion cannot recover precision already lost in
f32 positions. This is query precision, not a large-world rendering solution.
Ray origins must have a representable forward starting cell.

[`VoxelWorld::raycast`] accepts a predicate over registered block IDs/properties, so
selection, collision, and interaction can use different rules. For procedural or
external storage, [`GridRay::cast`] takes a cell callback returning [`RayCell`].
Each visited cell invokes the source exactly once. Resident queries cache the
current chunk lookup while traversing its cells, including unavailable chunks. No allocation occurs inside
traversal; a caller-supplied callback/predicate can still allocate or block.

Rules:

- Integer-boundary starts choose the cell immediately forward along moving axes;
  stationary axes use floor/half-open ownership. A boundary-facing start reports
  the entry face at distance zero. Interior starts have no entry face or adjacent
  placement cell. A stationary boundary alone does not invent an entry face.
- Exact floating-point edge/corner ties advance all tied axes simultaneously,
  skipping side cells with zero-length contact. X, then Y, then Z selects the
  reported face. Nearby, distinct floating-point crossings are not merged by an
  arbitrary epsilon.
- Reach is finite, nonnegative, and inclusive. The visited-cell budget is
  1..=1,048,576; defaults are 8 units and 256 cells. A zero reach still tests the
  forward starting cell.
- Missing chunks stop by default. Explicit `MissingPolicy::Skip` allows traversal
  through unavailable cells without declaring them loaded air.
- Completed misses, unloaded cells, budget exhaustion, and leaving the grid have
  distinct [`RaycastOutcome`] variants. A hit at the grid edge may have no
  representable adjacent cell. Placement rules remain game code.

## Run, test, and benchmark

```sh
cargo run --locked -p rayengine-voxel --example query
cargo test --locked -p rayengine-voxel
cargo doc --workspace --no-deps
# target/doc/rayengine_voxel/index.html
scripts/benchmark.sh save voxel-v1 voxel_
scripts/benchmark.sh compare voxel-v1 voxel_
```

Stable Criterion IDs and fixtures:

| Workload | Fixture/measurement |
| --- | --- |
| `voxel_storage/allocate_filled_4096` | Registered uniform stone chunk, allocation/fill/drop, with one fill-ID validation |
| `voxel_storage/validate_import_4096` | Validate/adopt 4096 stone IDs; input clone and output drop outside timing |
| `voxel_access/local` | One dense read at local (7,8,9) |
| `voxel_access/world_reads_1024/{1,64}` | Fixed 1024-position sequence across 1/64 resident stone chunks |
| `voxel_edits/{interior_pair,border_pair}` | Stone→air edit pair at interior / three-face corner; revisions and notifications included |
| `voxel_raycast/empty_axis/{16,256}` | Traverse exactly 16/256 unselected cells via callback |
| `voxel_raycast/resident_hit_256` | Lookup/predicate across 16 resident air chunks, selected stone at x=255 |
| `voxel_raycast/empty_corner_256` | 256 exact diagonal cells, simultaneous corner crossings |

Fixtures are constructed outside timed access/edit/query loops. Default world
hash seeding can contribute run-to-run noise; use confidence intervals and repeated
identical workloads before drawing conclusions. The existing snapshot workflow
exports named baselines, samples, revision, toolchain, and machine metadata.
These are CPU measurements, not GPU performance or whole-game FPS. Tests include
signed boundaries/extremes, admission/revision failures, all 16-bit IDs, ray faces,
missing/budget outcomes, and seeded comparison with an exhaustive box oracle.
Meshing, terrain generation, streaming, save schemas, and survival content are
separate roadmap issues.
