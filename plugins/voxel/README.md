# rayengine-voxel

Optional CPU-only block definitions and bounded 16³ chunk storage under
`plugins/voxel/`. Depends only on rayengine-core; no native toolchain or display
is needed. Game content and survival rules stay outside this crate.

Block IDs occupy 16 bits, with air reserved at zero. Build a BlockRegistry,
share it through Arc, and create chunks/worlds using the same allocation.
Each chunk validates its 4096 IDs, tracks content revisions and save dirtiness,
and exposes read-only X/Z/Y ordered cells. VoxelWorld enforces a resident count
limit, reports face-border edit dependencies, and uses installation generations
to reject stale acknowledgements after replacement. Rejected insertions retain
the incoming chunk; removal/replacement return old data for caller-owned policy.

```sh
cargo test -p rayengine-voxel
cargo doc -p rayengine-voxel --no-deps
```
