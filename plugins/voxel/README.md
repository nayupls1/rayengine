# rayengine-voxel

Voxel definitions, bounded **16³ chunks**, signed grid queries, and deterministic
CPU meshing, with optional textured raylib rendering. This plugin lives under
`plugins/voxel/`. Default features depend only on `rayengine-core`: no raylib,
native build toolchain, window, audio device, or graphics context. Enable `render`
for the SDK adapter. The engine core has no dependency on this package; games
own instances, scheduling, call order, and resource teardown.

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
a separately created registry happens to have equal definitions. Use
[`VoxelWorld::shared_registry`] to obtain the same allocation for generation/jobs
without retaining an additional handle in the game.

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
precision at all i32 grid positions. Uniformly tiny directions are supported;
mixed magnitudes that make a moving component underflow to zero during
normalization are rejected with [`VoxelError::InvalidRay`], preserving forward
boundary ownership. `GridRay::try_from(core::spatial::Ray3)` adapts
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
  arbitrary epsilon. Crossing times are recomputed from the original ray to avoid
  accumulated drift at repeated corners and inclusive reach limits.
- Reach is finite, nonnegative, and inclusive. The visited-cell budget is
  1..=1,048,576; defaults are 8 units and 256 cells. A zero reach still tests the
  forward starting cell.
- Missing chunks stop by default. Explicit `MissingPolicy::Skip` allows traversal
  through unavailable cells without declaring them loaded air.
- Completed misses, unloaded cells, budget exhaustion, and leaving the grid have
  distinct [`RaycastOutcome`] variants. A hit at the grid edge may have no
  representable adjacent cell. Placement rules remain game code.

## Neighbor-aware meshing

```rust
use rayengine_voxel::prelude::*;
use std::sync::Arc;
let mut definitions = BlockRegistry::new();
let stone = definitions.register(BlockDef::new("demo:stone"))?;
let mut world = VoxelWorld::new(Arc::new(definitions), 1);
world.insert_chunk(ChunkPos::default(), Chunk::filled(world.shared_registry(), stone)?)?;
let input = MeshInput::capture(&world, ChunkPos::default())?;
let mesh = input.build(MeshingOptions::default())?;
assert_eq!(mesh.stats().visible_faces, 1536);
assert_eq!(mesh.stats().quads, 6); // Six repeating 16×16 faces.
assert_eq!(mesh.stats().buffer_bytes, 936);
assert!(mesh.dependencies().is_current(&world));
for batch in mesh.batches() { batch.data().validate()?; }
world.set_block(BlockPos::new(0, 0, 0), BlockId::AIR)?;
assert!(!mesh.dependencies().is_current(&world));
# Ok::<(), Box<dyn std::error::Error>>(())
```

[`MeshInput::capture`] copies owner cells and six neighboring border slabs into
an owned 18³ padded buffer (11,664 block bytes, excluding registry/metadata).
It performs seven chunk lookups and can be moved to the existing CPU job system.
Building reads the snapshot, never live world state. Receipts retain world
identity and owner/neighbor installation/revision stamps, including absent
neighbors. [`MeshDependencies::is_current`] rejects changed owners/neighbors,
reinstallations, new neighbor arrivals, or a different world sharing the same
registry. Any neighbor edit conservatively invalidates the receipt; no border
revision optimization is assumed.

Opaque neighbors hide faces irrespective of block ID. Invisible and cutout
neighbors never occlude: retain geometry behind alpha holes, including internal
foliage faces. Opaque neighbors also hide touching cutout faces. Collision does
not determine geometry. Blended `RenderKind::Transparent` blocks in the owner
are explicitly rejected until a blended meshing/sorting policy is implemented;
they are not silently rendered opaque or omitted. Transparent neighbors do not
occlude. [`MissingFaces::Expose`] draws unavailable border faces by default;
`Hide` is an explicit alternative. Both track missing dependencies so arrivals
require a rebuild. Missing data is still distinct from loaded air in storage.

[`MeshingMode::Greedy`] scans face masks into rectangles sharing plane, tile and
alpha layer. Normals and counterclockwise winding point outward. Face UVs cover
one unit per block, including merged rectangles: a 16×16 face uses 0..16 UVs.
Side textures have V=0 at the top; opposite sides face outward without horizontal
mirroring. Top/bottom map U to +X and V to +Z. Batches sort opaque before cutout,
then tile, face and grid position, making output reproducible. Custom materials
must repeat these UVs; the supplied renderer does this inside each atlas region.
`Culled` emits individual unit faces for comparison or highly fragmented data.

[`FaceShading`] bakes directional sunlight/ambient intensities into vertex colors:
-X/+X 204, bottom 140, top 255, -Z/+Z 178. It does not propagate skylight, shadow
columns, torches or AO. Uniform per-face shading keeps merges valid. Configurable
[`MeshLimits`] bound quads and batches; indexed batches split at 65,532 vertices
or a smaller requested multiple of four. Dense cutout data can exceed a single
16-bit mesh. Invalid limits/over-budget output fail before upload. Empty geometry
is a successful result with zero batches. [`MeshStats`] reports exact logical
buffer bytes, excluding allocator/driver overhead and shared materials/textures.

Edits already return owner/touched-neighbor candidates. Invalidate those meshes
on an edit. Invalidate all six neighbors on chunk insertion/removal/replacement,
including transitions between missing and resident. Recheck receipts before
accepting work. Draws intentionally keep previously accepted geometry during
rebuilds; worker scheduling, residency, cancellation and frame upload budgets
belong to the streaming follow-up (#22).

## Optional rendering

```toml
rayengine-voxel = { path = "../rayengine/plugins/voxel", features = ["render"] }
```

This initialization fragment is checked with the rendering feature enabled:

```no_run
# #[cfg(feature = "render")]
# fn setup(ctx: &mut rayengine::prelude::InitContext<'_, '_>, world: &rayengine_voxel::VoxelWorld)
#     -> Result<(), Box<dyn std::error::Error>> {
use rayengine_voxel::prelude::*;
let texture = ctx.texture("assets/stone.png")?;
let mut materials = VoxelMaterials::create(ctx,
    &[TileTexture::whole(TileId(0), texture)], 0.5)?;
let mesh = MeshInput::capture(world, ChunkPos::default())?.build(MeshingOptions::default())?;
let mut chunk = RenderedChunk::new(ChunkPos::default())?;
chunk.upload_init(world, &mesh, &materials, ctx)?;
// Store both in the game, then draw through chunk.draw in a camera pass.
// On explicit detachment, release chunk buffers before its materials.
chunk.unload(ctx.assets);
materials.unload(ctx.assets); // Does not unload the game's texture.
# Ok(())
# }
```

`VoxelMaterials` owns one repeat shader and opaque/cutout descriptions per tile,
with a configurable alpha cutoff. `TileTexture::rect` maps normalized
(left, top, width, height) within a texture; `whole` uses the full image. The
shader applies `fract(UV)` before atlas mapping, preserving repetition on merged
faces without requiring texture-wrap state. Use nearest filtering for pixel art.
With linear filtering/mipmaps, provide tile gutters/insets and suitable mipmaps;
the adapter does not pack atlases or make neighboring pixels disappear.
`VoxelMaterials::bind` accepts borrowed custom materials for other shading and
UV policies. Their alpha mode must match the batch layer. Built-in and borrowed
resources are validated before committing geometry; the SDK's
`Assets::validate_material` exposes this read-only check to other plugins too.

`RenderedChunk::replace` runs before drawing, accepts only current receipts,
uploads **all** replacement batches, then commits and unloads old meshes. A stale
result, missing/invalid material, or any failed batch upload preserves all old
geometry; partial new uploads are released. A valid empty result removes old
geometry. This deliberately allows old/new buffers to coexist temporarily:
logical bytes are reported, but VRAM/time admission belongs to the streamer.
Mesh slots are reusable; SDK material/shader slots retain their high-water table
capacity until the run ends, so reuse a shared material table across chunks.

Draw with a `Frustum3D` captured from the same camera/viewport and render origin.
`RenderedChunk::draw` tests the full chunk bounds before submitting batches and
returns culled/submitted/unavailable counts. It allocates no CPU geometry or draw
queue. The game owns the collection of chunks (there is no automatic spatial
index); draws do not access the world or recheck stamps. Build/draw positions are
local 0..16. `meshing::chunk_translation` subtracts an i32 render origin using i64
before conversion to f32; shift the camera into that same space to keep unit
precision near large grid coordinates. This is explicit origin handling, not
automatic large-world rebasing. With the SDK's native clipping defaults use
near 0.05 / far 4000 when constructing the frustum.

Keep chunks, materials and textures in the same SDK run. External resource
unload/mutation is explicit and can invalidate subsequent draws, which report
unavailable dependencies. Call `unload` when detaching a chunk/material table.
Dropping a plugin leaves resources in `Assets` until that run ends; it cannot
implicitly perform render-thread teardown. There is no automatic `Plugin` hook
implementation because the game owns its camera, update and upload policy.
The runnable `render` example supplies original procedural tiles, opaque/cutout
geometry, per-face shading, culling, border edits and resource counts.

## Run, test, and benchmark

```sh
cargo run --locked -p rayengine-voxel --example query
cargo test --locked -p rayengine-voxel
cargo test --locked -p rayengine-voxel --features render
cargo run --locked -p rayengine-voxel --features render --example render
cargo doc --workspace --no-deps --features rayengine-voxel/render
# target/doc/rayengine_voxel/index.html
# New mesh workloads need a new baseline; retain previous voxel-v1 snapshots.
scripts/benchmark.sh save voxel-meshing-v1 voxel_
scripts/benchmark.sh compare voxel-meshing-v1 voxel_
scripts/render_benchmark.sh save voxel-render-v1 voxel_
scripts/render_benchmark.sh compare voxel-render-v1 voxel_
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
| `voxel_meshing/{solid,terrain,checkerboard,mixed_tiles,cutout}/{culled,greedy}` | Build the same 4096-cell fixtures in both modes; snapshot setup and output drop outside timing |
| `voxel_snapshot/capture_7_chunks` | Copy owner + six loaded border slabs, receipt creation; output drop outside timing |

Fixtures are constructed outside timed access/edit/query loops. Default world
hash seeding can contribute run-to-run noise; use confidence intervals and repeated
identical workloads before drawing conclusions. The existing snapshot workflow
exports named baselines, samples, revision, toolchain, and machine metadata.
These are CPU measurements, not GPU performance or whole-game FPS. Tests include
signed boundaries/extremes, admission/revision failures, all 16-bit IDs, ray faces,
missing/budget outcomes, and seeded comparison with an exhaustive box oracle.
Mesh fixtures use uniform solid; stepped terrain height `4+x/4+z/4`; alternating
solid/air checkerboard; solid tile regions alternating every two cells; and dense
cutout foliage (retaining internal faces). Each prints counts/payload outside
measurement. Snapshot/registry setup is untimed. Greedy face merging can add CPU
work on fragmented surfaces without reducing geometry; compare both modes for
your content rather than assuming a benefit from merging alone.

Native `voxel_draw/{solid,terrain,checkerboard,cutout}/{culled,greedy}` cases submit
16 identical overlapping chunks inside one camera pass, including culling and
material lookup, at the same 64×64 target. They are a repeatable submission/driver
workload, not a playable scene or FPS measurement. `voxel_upload` uses the same
fixtures/modes for atomic complete replacement (validation, new uploads and old
unload included); CPU generation is untimed. `voxel_culling/rejected_64` rejects
64 chunk bounds with no GPU submissions. Native cutout benchmark texels are white
and survive; the image probe separately verifies holes, repetition and depth.
Record GPU/driver/backend via the existing native metadata workflow. Tests include
all-face winding/tiles, all six neighbor slabs, an independent seeded unit-face
oracle, worst-case splits, receipt identity, and native failure rollback/pixels.
Terrain generation, streaming, save schemas, and survival content remain separate
roadmap issues.
