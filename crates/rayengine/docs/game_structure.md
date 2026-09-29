# Game structure

A game implements [`crate::Game`]. The engine calls `bindings` once, `init` once,
then alternates fixed updates and drawing. State belongs to your game; the engine
does not require a global singleton or a particular game-state enum.

Start with `main.rs`; extract modules as the game grows:

```text
src/
  main.rs       window configuration and App::run
  game.rs       Game implementation and screen/state transitions
  components.rs plain Rust gameplay components
  systems.rs    movement, combat, pickups and other simulation
  presentation.rs interpolated world passes and UI
assets/         game-owned source assets
```

Use [`crate::core::scene::Scene`] when entity composition or parenting helps.
It wraps `hecs` dense component storage; your components are ordinary structs.
Small games can also use directly owned structs. No mandatory plugin scheduler
or dynamic reflection is involved.

```rust
use rayengine::prelude::*;

struct Health(u32);
struct Velocity(Vec3);
let mut scene = Scene::new();
let player = scene.spawn_3d(Transform3D::at(Vec3::new(0.0, 1.0, 0.0)),
    (Health(100), Velocity(Vec3::X)));

for (transform, velocity) in scene.world.query_mut::<(&mut Transform3D, &Velocity)>() {
    transform.position += velocity.0 / 120.0;
}
scene.propagate().unwrap();
assert_eq!(scene.world.get::<&Health>(player).unwrap().0, 100);
```

In hecs 0.11, entity IDs are explicit query items. Use
`query::<(Entity, &Health)>()` when you need IDs, and `query::<&Health>()` when
you only need components. Prefer `query_mut` when the whole world is exclusively
borrowed; it avoids dynamic borrow checking.

2D and 3D use the same lifecycle and entity IDs, but keep their spatial types
distinct:

| Primitive | 2D | 3D |
| --- | --- | --- |
| Position | `Vec2` | `Vec3` |
| Local transform | `Transform2D` | `Transform3D` |
| Global transform | `GlobalTransform2D` | `GlobalTransform3D` |
| Camera | `Camera2D`, visible height | `Camera3D`, vertical FOV |
| Bounds | `Aabb2` | `Aabb3` |
| Character movement | `Body2D` | `Body3D` |
| Ray and index | `Ray2`, `SpatialIndex2D` | `Ray3`, `SpatialIndex3D` |
| Camera visibility | `Frustum2D` | `Frustum3D` |
| Drawing pass | `frame.world_2d` | `frame.world_3d` |
| Filled primitive | `canvas.rectangle` | `canvas.cube` |

Both share action input, fixed ticks, assets, hierarchy operations and `frame.ui`.
`Timer` and typed `Events<T>` queues are game-owned and work in either dimension.
Advance timers and clear/drain events at explicit simulation phase boundaries.
2D positive Y points **down**. 3D positive Y points **up**, with right-handed
coordinates. Rotations in transforms and the 2D camera use radians; the 3D FOV
uses degrees, as raylib does.

Parent entities must have the same transform dimension as their children.
`scene.set_parent` rejects cycles and invalid IDs before changing anything.
Call `scene.propagate` after local transforms change. `scene.despawn` detaches
direct children while preserving their local transforms. Entity generations
prevent a recycled slot from becoming an old entity's new parent.

If you directly spawn into `scene.world`, insert the local/global transform
pair yourself when you want propagation. Direct `Parent` insertion is an escape
hatch; valid transformed parent chains are checked during propagation.

Drawing never changes simulation state. Store previous/current positions and
interpolate in `draw`; see [timing](crate::guides::timing_input). For a menu or
level change, use a game-owned enum and let each state perform its updates and
passes. Asset IDs and explicit state transitions keep ownership visible.
