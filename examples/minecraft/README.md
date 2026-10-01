# Minecraft demo terrain

The game-owned version-one recipe lives here; the reusable generation contract,
chunk storage, meshing and streaming live in `plugins/voxel`. Terrain tools/tests
use CPU-only dependencies by default. Native rendering is an optional feature.

```rust
use rayengine_minecraft::terrain::{Terrain, TerrainSettings, chunk_fingerprint};
use rayengine_voxel::prelude::*;
let terrain = Terrain::new(42, TerrainSettings::default())?;
let chunk = terrain.chunk(ChunkPos::new(-1, 2, 0))?;
assert!(!chunk.is_dirty()); // Untouched terrain is reproducible.
assert_eq!(chunk_fingerprint(&chunk), chunk_fingerprint(&terrain.chunk(ChunkPos::new(-1, 2, 0))?));
let spawn = terrain.find_spawn(0, 0, 16, 1089)?;
assert_eq!(terrain.block_at(spawn.support), terrain.blocks().grass);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Run the CPU tool or native preview:

```sh
cargo run -p rayengine-minecraft --bin terrain -- --seed 42 --chunk=-1,2,0
cargo run --release -p rayengine-minecraft --features render --bin minecraft -- --seed 42
```

The CPU tool prints one JSON object with generator identity, settings, signed
chunk coordinates, raw-ID fingerprint, block counts, generation wall time, and a
safe support/feet position near that chunk's X/Z origin. Negative coordinates use
Euclidean chunk/lattice division. It needs no display, raylib, or C toolchain.

The native preview starts at a safe spawn and streams a 5×5×5 focus region.
Arrows move the focus one chunk; Escape exits. It renders original flat block
colors, so a clean checkout needs no Minecraft installation. Standard SDK
`--hidden`, `--frames`, `--size`, `--screenshot` and diagnostics flags work after
`--seed`. Camera and local mesh translations share a chunk-relative origin;
resizing uses the normal fitted viewport and stable vertical field of view.
Movement, mining/placement, texture import, survival and saving are subsequent
roadmap issues; this entry point currently previews generated terrain.

## Recipe and compatibility

`rayengine-minecraft:alpha-terrain`, **generator version 1**, uses the full u64
seed and validated `TerrainSettings`. The finite vertical domain is Y=0..127:
bedrock at zero, stone/coal/iron below three dirt cells and a grass surface, with
air below zero and at/above 128. Default surface heights lie in 24..72. Settings
keep surfaces in 8..112 and all tree cells below 128.

Two signed integer value-noise fields make broad/detail hills at 96/24 block
scales. Coordinate hashing uses fixed wrapping-u64 constants; corner values lie
in -32768..32767. Q16 cubic smoothstep and signed arithmetic shifts interpolate
corners. Heights use signed truncating division. Cached chunk-local lattice nodes
produce exactly the same samples as direct world-coordinate evaluation. No float,
random stream, neighbor reads or request-history state determines generated cells.

Two 3D fields intersect to carve caves above Y=3 and below a five-block surface
roof. Two more fields form coal/iron clusters in stone, with coal below 64 and
iron below 40. Trees use a world-coordinate 12×12 anchor grid: a seeded subset
receives four-to-six-block trunks and compact canopies. Generation enumerates
anchors whose canopies overlap each chunk, including roots in other chunks.
Logs take precedence over leaves; decorations replace only air. These rules apply
at both horizontal and vertical seams and negative/far coordinates.

The fixed block order after air is bedrock, stone, dirt, grass, coal ore, iron
ore, wood, leaves. Leaves are solid cutout cells. Block identities, hardness and
tiles are game-owned. A recipe version change is required for cell/noise,
feature-precedence, or block-mapping changes. Future saves must record generator
key/version, **seed and validated settings**, plus the registry mapping and edits.
A version alone does not identify worlds with different settings.

Untouched generated chunks are explicitly marked saved because the recipe can
regenerate them. An edit through `VoxelWorld::set_block` marks its chunk dirty;
streaming retains it until the game acknowledges an exact saved stamp. The
preview's eviction policy keeps dirty chunks; it performs no file I/O.

## Spawn and limits

`find_spawn` checks grass support and two empty cells above it for a centered
player no wider than one block and no taller than two blocks. A deterministic
square-ring search skips trunks/canopies and clips candidates at signed grid
edges. Radius is at most 32, and at most 4,225 valid columns can be examined;
invalid limits or exhaustion return explicit errors. Feet are computed in f64
so distant cell centers retain precision. Later gameplay checks movement against
the resident/edited world; the sampler describes untouched generation only.

A chunk output is 8,192 block bytes. Bulk generation caches 256 surface heights,
four fixed arrays of at most 216 lattice nodes each, and at most nine tree
candidates; it polls cancellation at Y slices and tree passes. It validates the
receiving registry allocation and never schedules work itself.

The preview uses four outstanding jobs/mesh slots, two workers, at most 160
resident chunks, and 160 installed render chunks. GPU limits include temporary
old/new geometry: 4,096 mesh handles and 64 MiB logical buffers. Staging is bounded
by 256 requests/4 MiB per transaction; each presented frame attempts at most
eight uploads/2 MiB, checking a 3 ms threshold between calls. Native allocation
and driver overhead are additional. Errors remain visible in the HUD.

## Validation and stable benchmarks

Golden CPU fingerprints freeze seed 42/version 1 at surface, underground,
bedrock/cave/ore, empty and signed-grid-edge fixtures. Tests compare all cells
across reversed/concurrent requests, a direct world sampler, all six neighbor
faces, and trees crossing horizontal/vertical negative-coordinate seams. Spawn
fixtures verify actual generated grass, headroom, bounded search, and extreme
coordinates. Cancellation, invalid settings, registry mismatch and domain limits
have focused tests. The native probe settles streaming around spawn, checks
CPU/GPU bounds and actual resident spawn clearance, then unloads all meshes.

```sh
scripts/check.sh
cargo test -p rayengine-minecraft
scripts/native_smoke.sh
scripts/benchmark.sh save minecraft-terrain-v1 'minecraft_(generation|spawn)_v1'
scripts/benchmark.sh compare minecraft-terrain-v1 'minecraft_(generation|spawn)_v1'
```

Seven `minecraft_generation_v1` cases generate 4,096 cells: `sky`,
`bedrock_caves_ores`, `underground`, `surface`, `canopy`, `negative_surface`, and
`world_edge`. Setup, checksum/count validation, and output destruction are outside
timing; generation, allocation, ID validation and clean acknowledgement are timed.
The fixed seed/settings/positions and expected fingerprints live in the benchmark,
which refuses a silently changed recipe. Two `minecraft_spawn_v1` cases measure
origin and negative-coordinate spawn selection. Workload IDs carry their recipe
version; preserve existing IDs/fixtures when optimizing an unchanged recipe.
The existing benchmark scripts export compiler/commit/machine metadata, samples
and estimates so identical workloads can be compared later.
