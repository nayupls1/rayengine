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

The native game starts at a safe spawn with an **empty inventory** and streams
terrain around the player. WASD moves, mouse looks, Space jumps and Shift sprints.
Hold left mouse to mine; right mouse places one held block. Keys 1–9 select the
hotbar. E or Escape opens/closes inventory; F10, the Quit button or native close
exits. Start by gathering two logs, crafting planks and sticks, then a wooden
pickaxe to harvest stone. Bedrock is unbreakable.

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

## Survival, inventory and crafting

Survival rules live in `survival.rs`; CPU interaction/layout live in `hud.rs`.
The engine core and voxel plugin do not depend on items, recipes or health.
Inventory contains nine hotbar slots and 27 reserve slots. Blocks/ingredients
stack to 64; each tool occupies a separate slot. Compatible stacks fill before
empty slots. Click two inventory slots to exchange their entire stacks. Closing
or losing focus cancels the selection without removing any items. Tab/Up/Down
moves focus, Enter/Space activates. Available recipes are enabled automatically;
crafting searches the entire inventory and commits costs/results atomically.
A full inventory or missing ingredients changes nothing.

| Recipe result | Ingredients |
| --- | --- |
| 4 planks | 1 log |
| 4 sticks | 2 planks |
| Wooden pickaxe / wooden axe | 3 planks + 2 sticks |
| Stone pickaxe / stone axe | 3 stone + 2 sticks |

Hand mining takes block hardness seconds (stone/ores: two; other blocks: one).
Matching wooden tools mine three times faster; stone tools mine six times faster.
Pickaxes match stone and ores; axes match logs. Changing tools cancels progress.
Five original crack meshes progressively cover the target as mining advances;
they are uploaded once, reused across targets, and disappear on cancellation or
successful breaking. This adds five mesh handles and 79,488 logical buffer bytes
outside the terrain renderer's resource quota (including the SDK's default UV
buffers); no per-tick GPU uploads occur.

| Broken block | Pickup requirement / result |
| --- | --- |
| Dirt or grass | Any held item / 1 dirt |
| Log | Any held item / 1 log |
| Leaves | Any held item / 1 leaves |
| Stone | Either pickaxe / 1 stone |
| Coal ore | Either pickaxe / 1 coal |
| Iron ore | Stone pickaxe / 1 iron ore |

Wrong tools can destroy rock but yield no resource. Dirt, stone, logs and leaves
can be placed. Planks, sticks, coal, iron and tools are inventory items only;
smelting and additional registered block types are deferred. Placement consumes
one item only after a successful world mutation; blocked/body-overlapping or
unloaded placement consumes nothing.

Mined items appear as small colored world cubes. Move within two blocks of a
pickup's center to collect it automatically. Full inventories retain unaccepted
items. Pickups are stationary and never silently despawn; checkpoints retain uncollected items. At 128
live pickups, mining pauses before changing terrain until room is freed by
collection. Draw distance is bounded to 64 blocks; there is no dropped-item
physics, tool durability, hunger, passive regeneration or death inventory loss
in this minimal demo.

Health starts at 20 half-heart units. Landing after falling more than three
blocks deals `ceil(distance - 3)` units, with a small collision-rounding tolerance.
Fall tracking follows global height through origin rebasing and pauses while
terrain is unavailable or inventory is open. Falling below Y=-16 is lethal.
Death releases the cursor and pauses gameplay. Choose Respawn to recover full
health and retain inventory. Respawn searches edited, loaded terrain within eight
columns of the original spawn, checking support and headroom. If unavailable,
streaming refocuses on spawn and retries at most four times a second; it never
spawns inside the original support blindly.

Inventory/death screens pause movement, look, mining, placement, pickups and fall
tracking while streaming/rendering continue. Opening and closing ticks stay
masked. Mining, placement and jump held across a modal tick remain suppressed
until physical release, preventing an inventory click/Space activation from
becoming an accidental attack or jump. The responsive panel and hotbar share
resolved bounds between hit testing and drawing; the usual fitted viewport
keeps them usable in wide and portrait windows.

```rust
use rayengine_minecraft::survival::{Inventory, Item, Recipe, Health};
let mut inventory = Inventory::default();
assert_eq!(inventory.insert(Item::Log, 2), 0);
inventory.craft(Recipe::Planks)?;
inventory.craft(Recipe::Planks)?;
inventory.craft(Recipe::Sticks)?;
inventory.craft(Recipe::WoodenPickaxe)?;
assert_eq!(inventory.count(Item::WoodenPickaxe), 1);
assert_eq!(inventory.count(Item::Planks), 3);
let mut health = Health::default();
health.movement(10.0, false);
assert_eq!(health.movement(4.0, true), 3);
assert_eq!(health.value(), 17);
health.respawn();
assert_eq!(health.value(), 20);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## World saves

Choose a slot with `--save PATH`. Without it, progress stays in memory for the
current session. The ignored `local-saves/` directory is suitable for local worlds:

```sh
cargo run --release -p rayengine-minecraft --features render --bin minecraft -- --seed 42 --save examples/minecraft/local-saves/world.save
# Reopen using the saved seed/settings; --textures can be chosen independently.
cargo run --release -p rayengine-minecraft --features render --bin minecraft -- --save examples/minecraft/local-saves/world.save
```

A new slot starts with seed42 unless `--seed` is supplied. An existing slot
restores the recorded seed; a conflicting explicit seed fails before native init.
Only a missing file creates a new world. Corrupt containers, unsupported container
or game schemas, generator/version/registry mismatches, malformed settings and
invalid gameplay state return errors and preserve the slot. There is no automatic
migration or fallback to a fresh world. Back up a rejected slot and choose a new
path to start over. Interrupted sibling temporary files are ignored; the named
slot is the only authoritative checkpoint.

The first frame queues a checkpoint; autosave requests occur every ten seconds
of fixed simulation time, including inventory/death screens. F5 saves or retries
a failed write. Dirty out-of-range chunks also request a checkpoint. Requests
coalesce while the single worker is busy. Quit/F10 freezes gameplay, releases the
cursor and waits for the newest requested state before exit; failure resumes the
scene with an error and leaves dirty chunks loaded. Native window close/`--frames`
also captures the latest state during scene destruction; the CLI reports final
save errors with a failing exit status. Final shutdown waits for disk I/O, which
can block if the filesystem is stalled. Forced termination can lose progress
since the last successful checkpoint.

Game schema **1** uses the core's format-one `RAYSAVE` container and CRC32. Its
bounded JSON records generator name/version/seed/settings, ordered block names,
original respawn support, chunk-relative simulation position/velocity/look and
exact collision body dimensions,
health/airborne fall peak, selected slot/all 36 inventory slots, all uncollected
pickups, and dense cells for every historically modified chunk. Untouched terrain
regenerates from the original recipe. Strict loading checks coordinates, raw IDs,
counts, finite poses, body clearance and spawn identity before installing state.
Transient mining progress, jump grace/buffers, UI focus, renderer resources and
queued generation work restart fresh. Textures are selected separately on launch.

Snapshots capture blocks and survival state together at one simulation point;
encoding, checksums, temporary writes, flushes and replacement run on one worker,
with one outstanding job/result. Only dirty resident cell buffers are copied;
unchanged saved history shares immutable buffers. After a successful write, the
loader installs that checkpoint **before** acknowledging each exact chunk
installation/revision. Edits made while saving stay dirty. A failed write never
marks chunks clean; even a successful rename followed by a failed directory flush
keeps them pinned until an explicit retry succeeds. This retains core errors and
their `NotCommitted`/`Unknown`/`Committed` recovery information.
After a worker failure, retries write a fresh checkpoint even if gameplay has
reverted to the previous state, because the failed replacement may already be on disk.

Limits are 160 resident/copied chunks per snapshot, 1,024 historical modified
chunks (8 MiB dense u16 cells), 128 pickups, and 12 MiB encoded payload. The
resident dirty cell copy is at most 1.25 MiB; snapshots, maps, JSON/container
buffers and allocator overhead add to process memory. Saved history remains in
memory, including evicted chunks. At the history cap, mining/placement may modify
existing edited chunks, but new chunk histories are denied **before** changing a
block, dropping or consuming an item. Restoring original cells does not reclaim a
history slot in this version. Failed writes can pin all 160 resident chunks and
pause movement/new loads; repair the filesystem and press F5 to resume. Chunk
serialization never runs inside the eviction callback.

Linux defaults to durable core replacement: sync the temporary file, replace the
slot, sync its parent. Newly created save directories/entries are synced too.
Use `--atomic-save` with `--save` to opt into replacement without power-loss
flush guarantees; this is also the option for other desktops. See
[engine save guarantees](../../crates/rayengine/docs/saves.md) for filesystem and
commit-state limits. A persistent `PATH.lock` sidecar holds an exclusive advisory
[OS file lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
throughout the session and all writes. A second cooperating game instance fails
instead of sharing a slot. The OS releases the lock on process exit/crash; the
empty sidecar may remain. Choose a regular file path; save/lock symlinks and names
ending in `.lock` are rejected.

The CPU snapshot API needs no display or raylib:

```rust
use std::sync::Arc;
use rayengine_minecraft::{gameplay::Player, persistence::{Snapshot, SCHEMA_VERSION, LIMITS},
    survival::{Survival, Item}, terrain::{Terrain, TerrainSettings}};
use rayengine_core::save;
use rayengine_voxel::prelude::*;
let terrain = Arc::new(Terrain::new(42, TerrainSettings::default())?);
let spawn = terrain.find_spawn(0, 0, 16, 1089)?;
let player = Player::new(spawn.feet())?;
let mut survival = Survival::default();
let checkpoint = Snapshot::new(terrain.clone(), spawn.support, &player, &survival)?;
let mut world = VoxelWorld::new(terrain.registry(), 160);
let position = spawn.support.split().0;
world.insert_chunk(position, terrain.chunk(position)?)?;
world.set_block(spawn.support, BlockId::AIR)?;
survival.inventory.insert(Item::Dirt, 1);
let (snapshot, stamps) = checkpoint.capture(&world, &player, &survival)?;
let container = save::encode(SCHEMA_VERSION, &snapshot.encode()?, LIMITS)?;
let data = save::decode(&container, LIMITS)?;
data.require_schema(SCHEMA_VERSION)?;
let restored = Snapshot::decode(&data.payload)?;
assert_eq!(restored.survival()?.inventory.count(Item::Dirt), 1);
assert_eq!(restored.chunk(position)?.get(spawn.support.split().1), BlockId::AIR);
// Encoding alone leaves chunks dirty. Saving acknowledges these exact stamps
// only after the worker successfully replaces the file.
assert_eq!(stamps.len(), 1);
assert!(world.chunk(position).unwrap().is_dirty());
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
feature-precedence, or block-mapping changes. Checkpoints record generator
key/version, **seed and validated settings**, plus the registry mapping and edits.
A version alone does not identify worlds with different settings.

Untouched generated chunks are explicitly marked saved because the recipe can
regenerate them. An edit through `VoxelWorld::set_block` marks its chunk dirty;
streaming retains it until the game acknowledges an exact saved stamp. The
game's eviction callback requests a background checkpoint and keeps dirty chunks;
it performs no file I/O. Successful writes make exact saved revisions evictable;
the loader restores committed edits on return. Without `--save`, dirty chunks
remain pinned for this session. Pinned dirty chunks consume the same 160-chunk
resident budget and can block new loads until a successful save.

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
resident chunks, and 160 installed render chunks. Terrain GPU limits include temporary
old/new geometry: 4,096 mesh handles and 64 MiB logical buffers. The five cached
crack meshes add 79,488 bytes outside that quota. Staging is bounded
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

## Survival validation and benchmark fixtures

CPU tests cover stack limits, full/partial insertions, atomic crafting failures,
all six recipes, harvest tiers/speeds, tool changes, placement count conservation,
pickup backpressure, health/respawn and modal input boundaries. Layout checks
cover a 320×480 minimum logical viewport, narrow expanded viewports, and the
standard fitted wide/portrait layout. The native survival probe runs the actual
game update/draw paths: gather two logs, craft/equip a wooden pickaxe using UI
clicks, mine/place stone, pause a fall in inventory, take damage, die and respawn.
It checks resource teardown, exports inventory/death/crack screenshots, and
compares early/late crack pixels to verify visible animation growth.

```sh
scripts/benchmark.sh save minecraft-survival-v1 minecraft_survival_v1
scripts/benchmark.sh compare minecraft-survival-v1 minecraft_survival_v1
scripts/render_benchmark.sh save minecraft-survival-render-v1 minecraft_survival_render_v1
scripts/render_benchmark.sh compare minecraft-survival-render-v1 minecraft_survival_render_v1
```

Six fixed CPU `minecraft_survival_v1` cases measure: insert into 36 full dirt
stacks (2,304 items); craft a wooden pickaxe from two 64-item ingredient stacks;
reject the same recipe with all 36 slots occupied; collect 128 nearby one-log
pickups into an empty inventory; hover a warmed 44-region inventory; and build
the fifth crack stage (1,248 vertices). Fixture construction, clones, validation
and output drops are untimed. UI response storage/pickup collection reuse
capacity; timing includes real slot/recipe scans, admission and mesh construction.

Two opt-in native `minecraft_survival_render_v1` cases draw the fifth cached
crack mesh 16 times, and submit a panel with 36 populated slot buttons/count
labels, six recipe buttons and Resume/Quit buttons. All inventory slots contain
64 dirt items; recipes are disabled. Labels, layout and assets are prepared
outside timing. The native harness uses a 960×960 logical view in a 64×64 target,
vsync disabled; measurements include CPU submissions/driver stalls, not GPU
elapsed time. No terrain streaming or texture uploads enter these workloads.
Benchmark IDs/fixtures stay fixed across future implementations; scripts export
commit/compiler/machine/renderer provenance and matching-workload comparisons.

## Persistence validation and benchmark fixtures

CPU tests cover strict schema/registry/generator admission, invalid player/item/
chunk coordinates/counts, interrupted siblings, corrupt CRC, bounded encoding,
single-slot exclusion, concurrent edits/reinstallation during blocked writes,
failures before replacement and after directory flush, exact dirty-chunk eviction/
reload, modified-history backpressure, retained pickups and airborne fall damage.
The native persistence probe mines a log through gameplay, saves inside inventory,
waits for F10's latest checkpoint, checks final native-close saving and reopens the
world to verify terrain, inventory, health and look. A native final-write failure
probe verifies the error reaches the caller and the old checkpoint survives.
Screenshots are exported to
`artifacts/smoke/minecraft-save-{write,reload}.png`.

```sh
scripts/benchmark.sh save minecraft-persistence-v1 minecraft_persistence_v1
scripts/benchmark.sh compare minecraft-persistence-v1 minecraft_persistence_v1
# Optional existing core file benchmarks record filesystem/durability metadata:
RAYENGINE_SAVE_IO_BENCH=1 scripts/benchmark.sh save save-io-v1 save_file
RAYENGINE_SAVE_IO_BENCH=1 scripts/benchmark.sh compare save-io-v1 save_file
```

Nine `minecraft_persistence_v1` CPU cases measure capture of 1/160 dirty chunks,
one dirty chunk with 1,024 prior histories, and encode/decode of 1/128/1,024 edited
chunks. Fixtures use schema1/generator1/seed42/default settings and the original
safe spawn, empty survival state, chunks `(i-512,10,-8)` and cell IDs `index % 9`.
The raw-ID fingerprint is `2731d4deb7933cdd`. Canonical payloads contain
8,929 / 1,053,885 / 8,425,443 bytes; benchmark assertions freeze their hashes.
Setup, fixture validation and output
drops are untimed; capture includes buffer/map copying and gameplay validation;
encoding is bounded canonical JSON; decoding includes field/budget/compatibility
checks and gameplay reconstruction. No disk/window/GPU work is timed. Preserve
IDs and fixtures when optimizing; the standard scripts export clean-commit,
compiler/machine provenance, estimates and matching-workload comparisons. Core
save/container/file workloads cover the unchanged CRC/replacement/flush path.
