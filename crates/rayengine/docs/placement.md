# Grid placement and build mode

Run `cargo run -p rayengine --example placement`. Click to place a two-cell
object, press R to rotate, right-click an object to select it and click a new
anchor to move it. Escape cancels selection; Delete removes the selection or
hovered object. Green previews pass validation; red anchors indicate rejected
edits. Blue cells are interaction access, and the gold path shows navigation
from the grid entry to the first object's access cell.

`rayengine::placement` (also in `rayengine-core`) is an optional CPU helper:
there is no runtime registration or placement state unless the game creates it.
`Footprint::new` accepts arbitrary occupied and access offsets, including negative
offsets. `PlacementPose` rotates both sets about its anchor by `QuarterTurn`.
Rotation One maps `(x, y)` to `(-y, x)`; for an X/Z floor this turns +X toward +Z.
The anchor need not occupy a cell, and rotation does not rebase the shape.

```rust
use rayengine::prelude::*;
let chair = Footprint::new([IVec2::ZERO], [IVec2::Y])?;
let mut room = PlacementGrid::new(UVec2::new(12, 10));
let pose = PlacementPose { anchor: IVec2::new(4, 3), rotation: QuarterTurn::One };
let preview = room.validate_place(PlacementId(7), &chair, pose)?;
assert_eq!(preview.access(), &[UVec2::new(3, 3)]);
let change = room.place(PlacementId(7), &chair, pose)?;
assert_eq!(change.after.as_ref(), room.object(PlacementId(7)));
# Ok::<(), PlacementError>(())
```

Occupied cells cannot overlap another object or any other object's access cells.
Access cells must be inside the grid and unoccupied; multiple objects may share
an access cell. Moves ignore their own previous occupancy and access, while
protecting all other objects. Empty occupied footprints and occupied/access
conflicts are rejected; duplicate offsets are deduplicated. Every failed
place/move/remove leaves the entire grid unchanged. Validation is read-only,
and committing revalidates against current state, so a preview never authorizes
a stale placement. Occupancy queries return no owner outside the grid;
`is_free` also checks bounds. Storage is sparse; access validation scans objects.

Successful edits return `PlacementChange` snapshots. Remove collision belonging
to `before.id()` and build collision for `after.id()` from its occupied cells.
For a floor, map grid X/Y to world X/Z using a game-owned `GridLayout`; collider
height and detailed object geometry belong to the game. The example updates
its collision boxes only after successful edits.

For navigation, `room.navigation(&terrain)` overlays occupancy on any `NavGrid`
with matching dimensions, preserving terrain weights and walls. Access cells do
not themselves block navigation. Alternatively refresh the union of the before
and after occupied cells in a game-owned cost grid, reading **final** occupancy
and underlying terrain; do not blindly restore released cells to cost 1.
Cancel/restart pending searches, recompute distance fields, and replan followers
when the layout changes. The example recalculates its displayed path after each
successful edit.

**Free access is not proof of reachability.** A free chair-front cell may be
surrounded by furniture or separated from an entrance by a wall. The helper only
checks object occupancy; it does not enforce terrain, navigation clearance,
prices, catalogs or room rules. To enforce those, validate a preview and apply
the proposed edit to a temporary cloned placement grid, then query the resulting
navigation overlay from game-defined entrances with appropriate clearance.
Commit to the live grid only after those game checks pass. An invalid preview
must never trigger collision or navigation writes. Keeping collision/navigation
and placement synchronized across external systems is the caller's responsibility;
the atomic guarantee applies to the placement grid itself.

`Camera3D::screen_ray` accepts a pointer in logical window coordinates and the
current `Viewport`, rejecting letterbox bars. It returns a normalized `Ray3`
originating at the camera eye. `Ray3::intersect_plane` and
`Camera3D::screen_to_plane` pick an infinite plane, rejecting invalid inputs,
parallel rays and intersections behind the eye. Normals need not be normalized.
`Update::pointer` is already in UI units: convert it with
`ctx.viewport.ui_to_screen(pointer)` before picking. Then map a ground-plane
hit's X/Z coordinates with `GridLayout::cell_at`. These helpers complement
`Camera2D::screen_to_world` and use the same viewport/DPI contract.

Arbitrary-angle footprints and undo history are separate concerns. Object IDs,
assets, prices, catalogs and room rules remain game-owned.
