# Minecraft voxel demo

The game-owned version-one recipe lives here; the reusable generation contract,
chunk storage, local collision queries, meshing and streaming live in `plugins/voxel`. Terrain tools/tests
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

Run the CPU tool or native game:

```sh
cargo run -p rayengine-minecraft --bin terrain -- --seed 42 --chunk=-1,2,0
cargo run --release -p rayengine-minecraft --features render --bin minecraft -- --seed 42
```

The CPU tool prints one JSON object with generator identity, settings, signed
chunk coordinates, raw-ID fingerprint, block counts, generation wall time, and a
safe support/feet position near that chunk's X/Z origin. Negative coordinates use
Euclidean chunk/lattice division. It needs no display, raylib, or C toolchain.

The native game starts at a safe spawn and streams a 5×5×5 region around the
player. WASD moves, mouse looks, Space jumps, Shift sprints, and Escape exits.
Hold left mouse to mine; right mouse places the selected block. Keys 1/2/3 select
dirt/stone/wood. Hand mining takes hardness seconds (stone/ores: two seconds);
bedrock is unbreakable. Blocks are unlimited for this interaction demo; tools,
drops, inventory, crafting, health and saves are later roadmap issues.

The crosshair selects visible cells within five blocks of the current simulation
eye. A black outline and HUD identify the target and mining progress. Releasing
mining, changing targets or changing the target chunk's revision resets progress.
Placement requires loaded air and cannot overlap the player's 0.6×1.8×0.6 body;
touching its feet is allowed. Both queries stop at unloaded terrain. Movement
pauses if its conservative local swept region is incomplete, while look remains
responsive. Edits affect collision immediately; chunk rendering updates through
bounded asynchronous remeshing and retains old geometry until replacement succeeds.

Original built-in pixel textures mean a clean checkout needs no Minecraft installation.
Standard SDK `--hidden`, `--frames`, `--size`, `--screenshot` and diagnostics flags
work after `--seed`. Camera and physics share a chunk-relative integer origin,
rebased during movement without resetting velocity, look, or interpolation;
resizing retains the normal fitted viewport and stable vertical field of view.
Use `--textures PATH` for the supported explicit local import described below.

```rust
use rayengine_minecraft::{gameplay::{Player, Interaction}, terrain::{Terrain, TerrainSettings}};
use rayengine_voxel::prelude::*;
let terrain = Terrain::new(42, TerrainSettings::default())?;
let spawn = terrain.find_spawn(0, 0, 16, 1089)?;
let player = Player::new(spawn.feet())?;
let mut world = VoxelWorld::new(terrain.registry(), 1);
world.insert_chunk(spawn.support.split().0, terrain.chunk(spawn.support.split().0)?)?;
// An incomplete selection is explicit; a missing cell never becomes a target.
let _selection = player.selection(&world)?;
let mut interaction = Interaction::default();
let report = interaction.step(&mut world, &player, false, false, 1.0 / 60.0, terrain.blocks().dirt)?;
assert!(report.edit.is_none());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Textures and local Minecraft assets

With no asset flag, the game uses original, MIT-licensed procedural 16×16 pixel
art from `src/textures/fallback.rs`. It needs no installation, download or runtime
file write. Grass top/side/bottom and log bark/end faces have separate mappings;
foliage uses alpha cutout. Generated cell fingerprints and block IDs remain
version one: these tile keys only affect rendering.

The game loads an explicit **PNG directory** with `--textures PATH`: either the
direct block directory or an extracted pack root containing
`assets/minecraft/textures/block/`. The eleven files below are required;
missing/invalid files name the source and texture. Partial sets do not silently
mix with fallback. The runtime has no JAR/ZIP reader or archive dependency.
Installation/profile directories, Bedrock packs and legacy Alpha/Beta
`terrain.png` sheets are unsupported.

Extract the supported subset **once** from an explicitly selected Java client
archive with `scripts/import_minecraft_textures.py`. This separate Python standard
library utility reads only the eleven named PNGs and writes ordinary files to a
new directory; it refuses to overwrite an existing directory and validates all
required entries before writing. Its default output is the ignored
`examples/minecraft/local-assets/minecraft/` folder. It does not inspect other
installation data. The Rust game only reads the resulting PNGs.

| Demo face | Tile key | Required PNG in `assets/minecraft/textures/block/` |
| --- | --- | --- |
| Bedrock | 0 | `bedrock.png` |
| Stone | 1 | `stone.png` |
| Dirt / grass bottom | 2 | `dirt.png` |
| Grass top | 3 | `grass_block_top.png` |
| Coal ore | 4 | `coal_ore.png` |
| Iron ore | 5 | `iron_ore.png` |
| Log sides | 6 | `oak_log.png` |
| Leaves | 7 | `oak_leaves.png` |
| Grass sides | 8 | `grass_block_side.png` + `grass_block_side_overlay.png` |
| Log top/bottom | 9 | `oak_log_top.png` |

Static power-of-two square PNGs from 16×16 through 256×256 are supported, including
indexed, grayscale, RGB and RGBA encodings. APNG and vertical animation strips
are rejected. Mixed resolutions scale to the largest input by nearest sampling.
Grass and leaves use a fixed palette (`GRASS_TINT`/`LEAF_TINT`); the grass overlay
is tinted and composited onto its side texture. Opaque blocks force alpha 255;
leaves preserve alpha and use cutoff 0.5. Custom models, biome color maps, PBR,
pack layering and animation metadata are outside this subset.

The ten final tiles pack into one five-column/two-row atlas with one-pixel edge
gutters, nearest filtering and per-block shader repetition on greedy faces.
At 16 pixels this is 90×36 (12,960 RGBA bytes); at 256 it is 1,290×516 (2,662,560
bytes). There are no generated mipmaps. The CPU atlas, encoded PNG and decoded
raylib image are dropped after initialization; drawing retains one texture,
one repeat shader and twenty opaque/cutout material descriptions. Loaded PNGs
are limited to 4 MiB each and decoder allocation to 4 MiB per image. The separate
extraction utility limits source archives to 512 MiB / 100,000 entries. These are admission limits, not a total
process-memory or driver-overhead guarantee.

```sh
# Inspect fallback assets as JSON using CPU-only dependencies:
cargo run -p rayengine-minecraft --bin textures
# Extract once, then inspect/play using ordinary PNG files:
python3 scripts/import_minecraft_textures.py /path/to/client.jar
cargo run -p rayengine-minecraft --bin textures -- --source examples/minecraft/local-assets/minecraft
cargo run --release -p rayengine-minecraft --features render --bin minecraft -- --seed 42 --textures examples/minecraft/local-assets/minecraft
```

For Modrinth on Linux, client archives can live under
`~/.local/share/ModrinthApp/meta/versions/<version>/<version>.jar`; some modded
profiles keep resources in a separate `meta/libraries/net/minecraft/client/.../*-extra.jar`.
Choose a file containing the documented entries, rather than a mod-loader JAR.
The local 26.3 and 1.21.1 texture subsets have been checked against this
mapping. No installation path is automatically searched or embedded in the game.

Extracted Minecraft assets stay local and must not be committed or included in
releases. The game writes no asset files. If manually staging a source in this
checkout, `examples/minecraft/local-assets/` is ignored by Git; generated screenshots
and benchmark samples belong in ignored `artifacts/`. Only the original fallback
implementation and original test fixtures are distributed.

```rust
use rayengine_minecraft::textures::{TextureSet, Tile};
let textures = TextureSet::fallback();
let atlas = textures.pack();
assert_eq!((atlas.width, atlas.height), (90, 36));
assert_eq!(textures.tile(Tile::Leaves).size(), 16);
assert_eq!(atlas.rgba.len(), 12_960);
let encoded = atlas.png()?;
assert!(encoded.starts_with(b"\x89PNG\r\n\x1a\n"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

```rust,no_run
use rayengine_minecraft::textures::TextureSet;
let textures = TextureSet::load("/path/to/block-pngs")?;
let atlas = textures.pack();
# Ok::<(), Box<dyn std::error::Error>>(())
```

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
game's eviction policy keeps dirty chunks; it performs no file I/O. Edits survive
travel within this session but not restart. Pinned dirty chunks consume the same
160-chunk resident budget: enough distant edited chunks can block new loads, at
which point movement pauses. A later save-system issue will resolve that limit.

## Spawn and limits

`find_spawn` checks grass support and two empty cells above it for a centered
player no wider than one block and no taller than two blocks. A deterministic
square-ring search skips trunks/canopies and clips candidates at signed grid
edges. Radius is at most 32, and at most 4,225 valid columns can be examined;
invalid limits or exhaustion return explicit errors. Feet are computed in f64
so distant cell centers retain precision. Gameplay checks movement against
the resident/edited world; the sampler describes untouched generation only.

A chunk output is 8,192 block bytes. Bulk generation caches 256 surface heights,
four fixed arrays of at most 216 lattice nodes each, and at most nine tree
candidates; it polls cancellation at Y slices and tree passes. It validates the
receiving registry allocation and never schedules work itself.

The scene uses four outstanding jobs/mesh slots, two workers, at most 160
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
CPU/GPU bounds and actual resident spawn clearance, mines the support block,
checks falling and GPU receipt invalidation/replacement, places it back, verifies
collision and landing, and unloads all meshes. CPU gameplay tests cover jump and
wall contact, signed chunk-boundary movement, rebasing, reach/unloaded queries,
mining resets, unbreakable cells, and placement inside the player.

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

Local collision and interaction query benchmarks live in the voxel plugin as
`voxel_colliders_v1` and `voxel_interaction_v1`. The standing/swept fixtures run
with both 8 and 512 resident chunks and identical local work. They reuse warmed
output capacity; query time includes grid/chunk lookups and policy checks.

```sh
scripts/benchmark.sh save voxel-interaction-v1 'voxel_(colliders|interaction)_v1'
scripts/benchmark.sh compare voxel-interaction-v1 'voxel_(colliders|interaction)_v1'
```


`minecraft_textures_v1` CPU workloads cover RGBA16/256 and indexed16 decoding,
fallback16 and maximum256 packing, and fallback atlas PNG encoding. Setup,
fixture validation and output destruction are untimed. `minecraft_texture_render_v1`
measures replacement and sixteen draw submissions for seed42/chunk(0,3,0) with
its six resident neighbors, ten fallback tiles and one shared atlas. The fixture
asserts 485 visible faces, 183 quads, 732 vertices, 366 triangles, five batches
and 28,548 logical mesh bytes; its atlas has 12,960 RGBA bytes. Native
window/texture/material creation and CPU terrain/meshing are untimed; native
measurements include driver work/stalls rather than GPU timers. Preserve recipe,
texture fixtures, mesh counts, renderer/backend and sample settings when comparing.

```sh
scripts/benchmark.sh save minecraft-textures-v1 minecraft_textures_v1
scripts/benchmark.sh compare minecraft-textures-v1 minecraft_textures_v1
scripts/render_benchmark.sh save minecraft-textures-render-v1 minecraft_texture_render_v1
scripts/render_benchmark.sh compare minecraft-textures-render-v1 minecraft_texture_render_v1
# Optional local import smoke alongside the standard fallback/native checks:
RAYENGINE_MINECRAFT_TEXTURES=/path/to/block-pngs scripts/native_smoke.sh
```

Texture tests cover direct/extracted-directory mapping, incomplete and unsupported
inputs, palette/grayscale/RGB decoding, alpha, tint compositing, mixed-resolution
packing, gutters, upright rows and original fallback consistency. Native image
probes check all four side orientations, grass top/bottom, log bark/ends and leaf
holes exposing terrain behind them. The headless `textures` binary prints schema1
source-directory/tile dimensions, normalized regions and atlas byte counts; it writes no PNGs.
