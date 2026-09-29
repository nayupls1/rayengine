# Spatial queries

`Ray2`/`Ray3`, `SpatialIndex2D`/`SpatialIndex3D`, and `Frustum2D`/`Frustum3D`
have matching APIs and live in the display-independent `rayengine-core` crate.
Use them for selecting a collider, finding nearby objects, and culling objects
outside a camera. They do not require a window or GPU.

## Select with a ray

Create a ray from an origin and direction. The constructor normalizes the
direction, so hit distances are world units. A ray starting strictly inside a
box hits at distance zero with a zero normal. Faces, corners, and the maximum
distance are inclusive. A hit normal points outward from the box.

This 2D example converts a screen pointer through the same fitted viewport as
the camera, then selects the nearest indexed collider:

```rust
use rayengine::prelude::*;

let view = Viewport::new(
    Vec2::new(960.0, 540.0), Vec2::new(960.0, 540.0), ScaleMode::Fit,
).unwrap();
let camera = Camera2D {
    target: Vec2::ZERO,
    rotation: 0.0,
    view_height: 20.0,
};
let mut index = SpatialIndex2D::new();
index.rebuild([
    ("near", Aabb2::from_center(Vec2::new(5.0, 0.0), Vec2::splat(2.0))),
    ("far", Aabb2::from_center(Vec2::new(12.0, 0.0), Vec2::splat(2.0))),
]).unwrap();

let screen = view.ui_to_screen(camera.world_to_ui(Vec2::new(5.0, 0.0), &view));
let world = camera.screen_to_world(screen, &view).unwrap();
let ray = Ray2::new(Vec2::ZERO, world).unwrap();
let selected = index.nearest(ray, 100.0).unwrap().unwrap();
assert_eq!(selected.id, "near");
assert!((selected.hit.distance - 4.0).abs() < 0.0001);
assert_eq!(selected.hit.normal, Vec2::NEG_X);
```

For a first-person center-screen selection ray, use the camera's forward
direction. The same `nearest` API returns the caller-supplied ID:

```rust
use rayengine::prelude::*;

let camera = Camera3D {
    position: Vec3::new(0.0, 0.0, 6.0),
    target: Vec3::ZERO,
    ..Camera3D::default()
};
let mut index = SpatialIndex3D::new();
index.rebuild([(
    42_u32,
    Aabb3::from_center(Vec3::ZERO, Vec3::splat(2.0)),
)]).unwrap();
let ray = Ray3::new(camera.position, camera.target - camera.position).unwrap();
let hit = index.nearest(ray, 8.0).unwrap().unwrap();
assert_eq!((hit.id, hit.hit.distance, hit.hit.normal), (42, 5.0, Vec3::Z));
```

For a single box, `ray.cast(bounds, max_distance)` avoids building an index.
`max_distance` must be nonnegative and not NaN; use `f32::INFINITY` for an
unbounded ray. An invalid ray, distance, or box returns `SpatialError`.
Exact-distance ties in an index use the original input order.

## Nearby objects and camera visibility

Use `overlapping(area)` to collect IDs whose boxes overlap the query with
positive area/volume, matching `Aabb2::intersects`/`Aabb3::intersects`.
Touching faces alone are excluded. `visit_overlapping(area, |id| ...)` avoids
allocating an output vector. An index can hold `hecs::Entity` IDs directly:

```rust
use rayengine::prelude::*;

let mut scene = Scene::new();
let tree = scene.spawn_3d(Transform3D::at(Vec3::new(4.0, 0.0, 0.0)), ());
let mut index = SpatialIndex3D::new();
index.rebuild([(
    tree,
    Aabb3::from_center(Vec3::new(4.0, 0.0, 0.0), Vec3::splat(2.0)),
)]).unwrap();
let nearby = index.overlapping(Aabb3::from_center(
    Vec3::new(3.5, 0.0, 0.0), Vec3::splat(2.0),
)).unwrap();
assert_eq!(nearby, vec![tree]);

let viewport = Viewport::new(
    Vec2::new(960.0, 540.0), Vec2::new(960.0, 540.0), ScaleMode::Fit,
).unwrap();
let camera = Camera3D {
    position: Vec3::new(0.0, 0.0, 6.0),
    target: Vec3::ZERO,
    ..Camera3D::default()
};
let visible = Frustum3D::from_camera(&camera, &viewport, 0.1, 100.0).unwrap();
let mut count = 0;
index.visit_visible(&visible, |_| count += 1);
assert_eq!(count, 1);
```

`Frustum2D::from_camera` accounts for camera rotation and viewport aspect;
its box test is exact. `Frustum3D::from_camera` uses the camera's vertical FOV
and explicit near/far clipping distances. Its AABB test is conservative near
frustum corners: it never removes a box that intersects the view, but may keep
an off-screen box. Visibility does not check occlusion. Edge/plane contacts
count as visible. `visit_visible` avoids output allocation; `visible` collects
IDs for convenience. Query result order follows the hierarchy and is not the
input order.

## Snapshot ownership and update cost

An index **copies** supplied IDs and bounds. It does not borrow scene storage,
track entities, or update itself when transforms change. After movement and any
`Scene::propagate` call, rebuild from the current world-space bounds. A failed
rebuild leaves the previous snapshot intact; callers decide whether to keep
using it. Duplicate IDs are accepted as separate colliders. Bounds must have
finite coordinates and `min <= max` on every axis. Zero-width boxes are allowed;
they can be hit by rays but cannot have positive-volume proximity overlap.

The index is a balanced AABB hierarchy: rebuilding is O(n log n), while
read-only queries prune whole branches when possible. Rebuild reuses staging
capacity but temporarily holds both old and new snapshots. The index is a good
fit when many queries share the same collider positions. For a few boxes or
constantly changing geometry, direct `Ray::cast` or a simple loop may be cheaper.
There are no SDK allocations in visitor queries; collecting APIs allocate a
result vector. Voxel traversal, selection rules, and which objects should be
indexed remain game logic.

The stable `spatial_ray`, `spatial_nearby`, `spatial_visible`, and
`spatial_rebuild` Criterion workloads cover 128, 4,096, and 32,768 colliders
in both dimensions. Ray workloads include a direct linear-scan reference.
Use the existing CPU baseline workflow described in
[testing and performance](crate::guides::testing_performance).
