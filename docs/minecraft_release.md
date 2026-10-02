# Minecraft demo release 0.0.1

The release combines the optional `plugins/voxel` library with the game-owned
recipe, survival rules and persistence in `examples/minecraft`. The core and
SDK retain their existing dependency boundaries. Arena and Meadow remain
standalone 2D/3D demos; a game using `rayengine` does not acquire voxel content.

## Start from a clean checkout

Install the Rust 1.89+ and native prerequisites in
[quickstart](../crates/rayengine/docs/quickstart.md). Use Linux with an OpenGL 3.3
context and X11/XWayland for the default backend:

```sh
cargo run --locked --release -p rayengine-minecraft --features render --bin minecraft -- --seed 42 --save examples/minecraft/local-saves/world.save
# Reopen using the saved seed/settings:
cargo run --locked --release -p rayengine-minecraft --features render --bin minecraft -- --save examples/minecraft/local-saves/world.save
cargo run --locked -p rayengine-minecraft --features render --bin minecraft -- --help
```

The clean checkout starts with original fallback textures and an empty inventory.
Gather two logs, craft planks/sticks and a wooden pickaxe, then harvest stone for
stone tools. Hold LMB to mine, click RMB to place a held block, use 1–9 for the
hotbar and E/Escape for inventory. WASD/mouse move/look, Space jumps and Shift
sprints. F5 saves/retries and F10 or Quit exits. Falling causes damage and the
death screen respawns while retaining inventory. See the
[full terrain/survival/save guide](../examples/minecraft/README.md) and
[bundle controls](../examples/minecraft/RELEASE.md).

Native Wayland is an explicit `rayengine/wayland` feature and needs the development
libraries listed in quickstart. CPU terrain, gameplay, voxel and save tests use
default features without raylib/display/CMake. Optional imported Minecraft PNGs
are selected explicitly with `--textures`; they stay local and are excluded from
release artifacts. The original fallback art is sufficient for every release check.

## Build a Linux bundle

The packaging tools need Python 3.12+, Git and `ldd` in addition to the build
prerequisites. Commit source changes first so the manifest identifies exactly the
built revision:

```sh
python3 scripts/package_minecraft.py
# Optional native Wayland bundle, using a distinct output directory:
python3 scripts/package_minecraft.py --backend wayland --output artifacts/releases-wayland
# Validate the default bundle from a fresh temporary working directory:
python3 scripts/test_minecraft_package.py artifacts/releases
# In headless Linux CI: xvfb-run -a python3 scripts/test_minecraft_package.py artifacts/releases
```

The archive contains `minecraft`, its standalone README/license, offline checked
rustdoc in `reference/`, a source/compiler/target/libc/backend manifest, linked
library/version information, dependency license notices and per-file checksums. An adjacent checksum covers
the archive. Only these build products are packaged; local saves/assets and the
checkout are excluded. Source changes during the build reject publication. Existing archives are preserved; choose another `--output`
directory to build a new candidate. The packager owns a separate cache under
`target/minecraft-release/` and clears its generated doc tree before export.
Archive order/ownership/timestamps are normalized to the source commit; native
binaries are not promised to be bit-reproducible across build machines.

Use the bundle on a Linux desktop compatible with its recorded host architecture,
libc and shared-library versions. It requires a graphics context even with
`--hidden`. It can run outside the checkout and does not need Cargo or an installed
Minecraft copy. CI uploads the candidate archive, checksums and native smoke
screenshots/reports for review.

## Release contract and budgets

Generation identity is `rayengine-minecraft:alpha-terrain`, version1/seed/settings;
game saves use schema1 in the existing core format-one checksummed container.
The integer recipe is independent of request order. Registry order/settings are
validated on load. Rejected saves are preserved, and no automatic migration is
performed. Textures are selected independently on each launch.

| Work | Bound / policy |
| --- | --- |
| Terrain | 16³ cells/chunk; Y=0..127; fixed seed42/default settings in golden fixtures |
| Resident/render chunks | 160 each; streaming radius2 horizontally/vertically |
| CPU generation/meshing | 2 workers, 4 outstanding jobs/mesh slots |
| Terrain GPU | 4,096 mesh handles / 64 MiB logical buffers, including replacement coexistence |
| Upload staging | 256 requests / 4 MiB per transaction |
| Per presented frame | 8 uploads / 2 MiB; 3 ms threshold checked between calls |
| Mining cracks | 5 cached meshes / 79,488 logical buffer bytes outside terrain quota |
| Saves | 1 worker / 1 outstanding snapshot; 160 copied chunks / 1,024 edited histories / 12 MiB JSON |
| Survival | 9 hotbar + 27 reserve slots; 128 stationary pickups; 20 health units |

Dirty chunks remain resident until an exact saved revision/installation is
acknowledged. Concurrent edits stay dirty, failed saves retain chunks, and retries
are explicit. At the history cap, new chunk edits are denied before any block/item
mutation. Reproducible untouched terrain regenerates; saved edits reload after
eviction. Save capture copies only dirty cells and shares immutable history;
encoding/flush/replacement run off the render thread. Final shutdown can wait on
filesystem I/O. These bounds exclude allocator, snapshots, shared texture/material,
controller and graphics-driver overhead; they are not a total memory/FPS promise.

Deferred: mobs, liquids, redstone, multiplayer, extensive crafting, hunger,
propagated torch/skylight lighting and interactive JSON game automation. Directional
face shading and alpha-cutout foliage are supported. Linux is validated;
Windows/macOS remain optional, and mobile/browser are outside the release scope.

## Validation and benchmark evidence

```sh
scripts/check.sh
cargo test --locked -p rayengine-core -p rayengine-voxel -p rayengine-minecraft
cargo +1.89.0 check --locked --workspace --all-targets --features rayengine-voxel/render,rayengine-minecraft/render
python3 scripts/template_smoke.py
cargo check --locked -p rayengine-minecraft --features render,rayengine/wayland
scripts/native_smoke.sh
cargo bench --locked --workspace --features rayengine-voxel/render,rayengine-minecraft/render --no-run
```

The release probe `native_minecraft_release_play_travel_save_and_reload` runs real
survival input/UI, draws and save workers in one scenario. It gathers/crafts/equips
a tool, mines/places, takes real fall damage, dies and respawns, saves, relocates to
a distant generated safe spawn, moves/jumps/looks and verifies saved chunks evict.
A fresh scene reopens the saved player/inventory/fall state, returns to the edited
region and validates cells/render receipts, then restores the saved pose. The
second scene leaves the container byte-identical. Assertions check resource bounds
and complete scene-owned GPU teardown; the SDK retains its built-in material
shader until run shutdown. Interaction cells and the distant relocation are
controlled fixtures; game input, loaders, rendering and persistence use production
paths. Screenshots and a structured proof report are exported under
`artifacts/smoke/minecraft-release-*` and `minecraft-release.json`.

Existing probes cover texture orientation/cutout, growing cracks, wide/portrait UI,
corrupt/future/incompatible saves, interrupted writes, concurrent edits, exact
acknowledgements, save failures and streamed resource bounds. The separate bundle
probe verifies checksums/offline docs, help/version without a display, fallback
launch from an empty working directory, the shipped CLI's durable/atomic saves,
recorded-seed reopen and preservation on seed conflict.

For performance comparisons, record a baseline on the previous clean revision and
compare the same fixed workloads after the change, on the same machine/backend:

```sh
scripts/benchmark.sh save release-cpu 'minecraft_(generation|spawn|survival|persistence)_v1|voxel_meshing|voxel_stream_v1'
scripts/benchmark.sh compare release-cpu 'minecraft_(generation|spawn|survival|persistence)_v1|voxel_meshing|voxel_stream_v1'
scripts/render_benchmark.sh save release-render 'voxel_(upload|draw|culling|stream_upload_v1)|minecraft_.*render_v1'
scripts/render_benchmark.sh compare release-render 'voxel_(upload|draw|culling|stream_upload_v1)|minecraft_.*render_v1'
```

Golden IDs/cells/mesh statistics/payload hashes keep fixtures stable. The existing
scripts export samples, estimates and commit/compiler/machine/backend provenance;
[compare_benchmarks.py](../scripts/compare_benchmarks.py) reports matching workloads
and differing metadata. CPU and native submissions/driver stalls are separate
measurements; neither establishes GPU time or game FPS. Comparisons are kept in
ignored `artifacts/benchmarks/`; benchmark compilation remains part of shared CI.
