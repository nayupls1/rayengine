# Tilemap plugin

`rayengine-tilemap` is an optional CPU-only library for finite, layered 2D grids.
The default feature set has no raylib or native dependencies. Enable `render`
for `render::TileAtlas`, an adapter over cached SDK textures and validated sprite
regions. Games own the map, atlas handle, and character; engine crates never
depend on this plugin.

```rust
use rayengine_tilemap::{Tilemap, TileId};
use rayengine_core::{collision::Body2D, glam::Vec2, spatial::Ray2};
let mut map = Tilemap::from_toml(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/level.toml")))?;
let mut body = Body2D::new(Vec2::new(48.0, 48.0), Vec2::new(20.0, 28.0));
body.velocity.y = 100.0;
map.move_body(&mut body, 1.0 / 60.0, false);
let hit = map.raycast(Ray2::new(body.position, Vec2::Y)?, 1000.0)?;
map.set_tile(0, 0, 0, None)?;
assert_eq!(map.tile(0, 0, 0), None);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Version 1 levels are UTF-8 TOML, with one Unicode character per cell (`.` is
empty), a legend of source pixel regions and collision flags, and equally sized
named layer grids. Unknown fields, ragged grids, unknown symbols, zero/overflowing
sprite regions, unsupported versions and invalid geometry are rejected.
`atlas` is a root-relative asset name. For example:

```toml
version = 1
atlas = "tiles.png"
tile_size = [32.0, 32.0]
origin = [-32.0, 0.0] # optional, defaults to zero

[tiles."#"]
region = [0, 0, 16, 16]
solid = true

[tiles."="]
region = [16, 0, 16, 16]
one_way = true
trigger = 1 # optional game-owned bitmask
custom = 2  # optional game-owned bitmask

[[layers]]
name = "ground"
rows = ["....", ".==.", "####"]
```

Declare an asset root in `rayengine.toml` (`[assets] roots = ["assets"]`),
resolve `project.asset("level.toml")`, then call `Tilemap::load(path)`. Resolve
`map.atlas_asset()` through the same project and load with `ctx.texture(path)`.
Source rectangles are checked against the actual cached texture on each draw;
unloaded handles and invalid source rectangles skip submissions safely.
See the runnable [level example](crate::guides::level) for disk loading, player
collision, manifest asset lookup and runtime tile edits:

```sh
cargo run -p rayengine-tilemap --features render --example level
```

A/D or arrows move, Space jumps, S drops through one-way platforms, E toggles a
bridge tile. The example uses an original tiny atlas included with the level.

Maps have 16×16 chunks, at most 16,777,216 allocated cells including padding
across all layers, immutable palettes and named layers in draw order. Cells are
axis-aligned with positive Y down. World↔tile conversion uses floor semantics;
the map's bottom/right boundary is outside. `visit_region` includes touching
cells and returns solid/one-way/trigger/custom metadata. `solid_geometry`
produces ordinary `Aabb2` values for `Body2D::move_and_slide` or a physics
broadphase. `rebuild_solid_index` builds the existing `SpatialIndex2D` with stable
(layer, x, y) IDs; rebuild caller-owned snapshots after edits. The planned physics
plugin can consume these IDs, flags and coordinate queries.

`Tilemap` implements the core `pathfinding::NavGrid` for top-down navigation:
a cell is blocked when any layer holds a solid tile (`is_solid`), and one-way,
trigger and custom tiles are walkable. `grid_layout()` converts path cells to
world positions, including rectangular tiles. Searches read current cells, so
replan after edits.

`move_body` queries the swept region, resolves X then Y, and handles downward
one-way top contacts. Solid takes precedence over one-way; trigger/custom bits
do not block movement by themselves. Spawn outside solids (initial overlaps
are not depenetrated). `drop_through` disables one-way collision for that step.
`raycast` hits solids only, tests chunk bounds before cells, returns exact slab
contacts and accepts infinite distance. Its chunk traversal is linear in chunk
count; equal-distance contacts prefer layer, row, then column.

Every query reads current cells: set/clear needs no dirty-cache synchronization.
`visit_visible` conservatively selects chunks with a camera AABB (including a
rounding margin), tests cells against the rotated camera rectangle, and preserves
layer order. It uses
the current `Viewport`, so Fit/Expand/IntegerFit, resizing, rotation and DPI share
the SDK camera contract. `TileAtlas::draw_frame` pairs culling and rendering with
the same camera automatically.

Validation: `cargo test -p rayengine-tilemap` (including this checked example),
`cargo test -p rayengine-tilemap --features render native_tilemap -- --ignored --test-threads=1`
(requires a display), and `cargo bench -p rayengine-tilemap --bench tilemap`.
The [performance guide](crate::guides::performance) describes the 512×512 comparison.
