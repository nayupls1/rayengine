# Grid pathfinding

`rayengine::pathfinding` (from the display-independent `rayengine-core`
crate) finds paths on 2D grids: A* with four- or eight-way movement, per-cell
costs and corner-cutting rules, line-of-sight path smoothing, a path follower
for character bodies, and distance fields for many agents chasing one target.
It needs no window, GPU or native libraries.

## Describe the grid

Searches read cells through the `NavGrid` trait, so a tilemap, a voxel layer
or a plain array is searched in place instead of being copied:

```rust
use rayengine::prelude::*;

struct Tiles {
    width: u32,
    solid: Vec<bool>,
    mud: Vec<bool>,
}

impl NavGrid for Tiles {
    fn size(&self) -> UVec2 {
        UVec2::new(self.width, self.solid.len() as u32 / self.width)
    }

    fn cost(&self, cell: UVec2) -> Option<f32> {
        let i = (cell.y * self.width + cell.x) as usize;
        (!self.solid[i]).then_some(if self.mud[i] { 3.0 } else { 1.0 })
    }
}
# let _ = Tiles { width: 1, solid: vec![false], mud: vec![false] };
```

`cost` returns `None` for a blocked cell, or the cost of entering it. Moving
costs the entered cell's cost times the step length (`1` orthogonally, `√2`
diagonally); the start cell is never charged or checked, so an agent standing
in a blocked cell can still leave it. For a quick closure, use
`GridFn::new(size, |cell| ...)`. `CostGrid` is a small owned grid with
`set(cell, cost)` and `block_rect(min, max)`. A voxel world can expose one
walkable layer, for example "air above solid ground", the same way.

## Find a path

`PathFinder` keeps its search buffers between queries. Write paths into a
reused vector so steady-state queries do not allocate:

```rust
use rayengine::prelude::*;

let mut grid = CostGrid::new(UVec2::new(8, 5), 1.0);
grid.block_rect(UVec2::new(3, 0), UVec2::new(3, 3));
grid.set(UVec2::new(5, 4), Some(4.0));

let mut finder = PathFinder::new();
let mut path = Vec::new();
let options = PathOptions {
    neighborhood: Neighborhood::Eight(DiagonalRule::IfBothOpen),
    ..PathOptions::default()
};
let status = finder
    .find_path(&grid, UVec2::new(0, 0), UVec2::new(7, 0), &options, &mut path)
    .unwrap();
let PathStatus::Found { cost } = status else { panic!("walled off") };
assert_eq!(path.first(), Some(&UVec2::new(0, 0)));
assert_eq!(path.last(), Some(&UVec2::new(7, 0)));
assert!(path.contains(&UVec2::new(3, 4)));
assert!(cost > 7.0);
```

The path holds every cell from start to goal inclusive. `Unreachable` leaves
it empty; a blocked goal is unreachable without searching, unless it is also
the start, which is always found with cost zero. Exact ties are
broken deterministically, so the same inputs give the same path.

`Neighborhood::Four` allows orthogonal moves only. `Neighborhood::Eight` adds
diagonals limited by a `DiagonalRule` on the two cells beside the move:

| Rule | Diagonal allowed when |
| --- | --- |
| `Always` | always, even squeezing between two blocked cells |
| `IfEitherOpen` | at least one side cell is walkable |
| `IfBothOpen` (default) | both side cells are walkable: no corner cutting |

`PathOptions::min_cost` (default `1.0`) is the lowest cost any cell may have.
It scales the distance heuristic so paths stay optimal; set it lower to allow
cheaper roads. Costs that are nonfinite or below it return
`PathError::InvalidCost` and cancel the search.

### Spread long searches over frames

Set `PathOptions::budget` to limit expanded cells per call. A search that runs
out returns `PathStatus::Pending`; call `resume` on later ticks with the same,
unchanged grid:

```rust
use rayengine::prelude::*;

let grid = CostGrid::new(UVec2::new(64, 64), 1.0);
let mut finder = PathFinder::new();
let mut path = Vec::new();
let options = PathOptions { budget: Some(32), ..PathOptions::default() };
let mut status = finder
    .find_path(&grid, UVec2::ZERO, UVec2::new(63, 63), &options, &mut path)
    .unwrap();
while status == PathStatus::Pending {
    // In a game, resume on the next fixed tick instead.
    status = finder.resume(&grid, Some(32), &mut path).unwrap();
}
assert_eq!(path.len(), 64);
```

Starting a new `find_path` cancels a pending search; `cancel` drops it
explicitly. `resume` rejects a grid whose size changed. A grid whose cells
change mid-search yields a path for a mix of old and new cells, so restart
after edits.

## Smooth and follow the path

Grid paths turn at cell centers. `smooth_path` removes waypoints a straight
segment can skip, keeping only corners. A shortcut must cross walkable cells
only, respect the corner rule where it passes exactly through a cell corner,
and cost no more than the cells it replaces, so cutting across mud must save
more than the mud costs.

`PathFollower` turns cells into world positions through a `GridLayout` and
sets a body's velocity; collision stays with `move_and_slide`:

```rust
use rayengine::pathfinding::smooth_path;
use rayengine::prelude::*;

let mut grid = CostGrid::new(UVec2::new(6, 4), 1.0);
grid.block_rect(UVec2::new(2, 0), UVec2::new(2, 2));
let layout = GridLayout::new(Vec2::ZERO, 1.0);
let walls: Vec<Aabb2> = (0..3).map(|y| layout.cell_bounds(UVec2::new(2, y))).collect();

let mut finder = PathFinder::new();
let mut path = Vec::new();
let options = PathOptions::default();
finder.find_path(&grid, UVec2::ZERO, UVec2::new(5, 0), &options, &mut path).unwrap();
smooth_path(&grid, &mut path, options.neighborhood).unwrap();

let mut follower = PathFollower::new(0.05);
follower.set_cells(&layout, &path);
let mut body = Body2D::new(layout.cell_center(UVec2::ZERO), Vec2::splat(0.6));
let dt = 1.0 / 60.0;
for _ in 0..300 {
    follower.steer_body_2d(&mut body, 4.0, dt);
    body.move_and_slide(dt, &walls);
}
assert!(follower.is_finished());
assert!(body.position.distance(layout.cell_center(UVec2::new(5, 0))) < 0.06);
```

An intermediate waypoint counts as reached within the arrival radius or one
step of movement, whichever is larger, or once the body has moved beyond it
along the segment leading to it, within that tolerance of the segment's line,
so a fast body never turns back. The final waypoint must be reached within the
arrival radius, and speed is limited so the body stops on it.
`steer_body_3d` reads waypoints as world `(x, z)` and keeps vertical velocity
for gravity. `GridLayout::cell_at` converts a world position back to a cell.

Segments are checked as lines between cell centers. A body wider than a point
can brush a wall corner and slide along it; keep bodies smaller than a cell
and prefer `DiagonalRule::IfBothOpen` for tight maps.

## Many agents, one target

`DistanceField` stores the cost from every cell to the nearest of one or more
goals. Compute it once when the target moves; each agent then calls
`next_step` with its own cell, at constant cost per agent:

```rust
use rayengine::prelude::*;

let mut grid = CostGrid::new(UVec2::new(9, 3), 1.0);
grid.block_rect(UVec2::new(4, 0), UVec2::new(4, 1));
let mut field = DistanceField::new();
field.compute(&grid, [UVec2::new(8, 0)], Neighborhood::default()).unwrap();

let mut agents = [UVec2::new(0, 0), UVec2::new(2, 2), UVec2::new(6, 1)];
for _ in 0..20 {
    for agent in &mut agents {
        if let Some(next) = field.next_step(&grid, *agent) {
            *agent = next;
        }
    }
}
assert!(agents.iter().all(|&agent| agent == UVec2::new(8, 0)));
```

Distances use the same costs and corner rules as `PathFinder`, so following
`next_step` traces an optimal path. Blocked or unreachable cells have no
distance, so unlike `PathFinder`, an agent standing in a blocked cell gets no
next step. Recompute after the grid changes.

## Cost and limits

A* runs in O(n log n) for n expanded cells and stops at the goal; open maps
expand few cells, while mazes may expand most of the grid. A distance field
always visits every reachable cell. Buffers grow to the largest grid seen
and are reused: one 16-byte node per cell for A*, one `f32` per cell for a
field, plus the open queue. Grids must have fewer than `u32::MAX` cells.
Searches do not consider other agents, dynamic obstacles or body clearance;
avoidance and replanning frequency remain game logic.

The `pathfinding_astar`, `pathfinding_smooth` and `pathfinding_field`
Criterion workloads cover 64², 256² and 512² maps. See
[testing and performance](crate::guides::testing_performance).

## Example

Click to move the gold target. The red pack steers with one shared distance
field; the blue scout follows a smoothed A* path planned a few cells per tick.
Run it with `cargo run -p rayengine --example pathfinding`:
