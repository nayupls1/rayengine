# Arcade physics

Own a `PhysicsWorld2D` or `PhysicsWorld3D` in game state. Call `step(ctx.tick,
&mut events)` from `Game::fixed_update`. CPU simulations can use `FixedClock`
and `Tick` directly. Gravity is opt-in per body: 2D uses positive Y down, 3D uses
positive Y up. Shapes are centered boxes, circles and spheres.

```rust
use rayengine_core::prelude::*;
let mut world = PhysicsWorld2D::new(4.0);
let mut floor = PhysicsBody2D::new(Vec2::new(0.0, 10.0),
    Shape2D::box_shape(Vec2::new(100.0, 1.0)));
floor.kind = BodyKind::Static;
world.insert(floor);
let mut ball = PhysicsBody2D::new(Vec2::ZERO, Shape2D::round(0.5));
ball.gravity = Some(Vec2::new(0.0, 20.0));
ball.restitution = 0.8;
let ball = world.insert(ball);
let mut events = Events::<TriggerEvent>::default();
for index in 0..120 {
    world.step(Tick { index, dt: 1.0 / 120.0 }, &mut events);
    events.clear(); // explicit game-owned event lifetime
}
assert!(world.body(ball).unwrap().position.y < 9.0);
```

Use `BodyKind::Kinematic` and velocity for a moving platform. Riders touching
its upward face inherit its translation while their intrinsic velocity stays
independent. Walking off transfers current world velocity into ordinary airborne motion;
landing on another solid keeps the resolved world velocity. Jumping away detaches. A restitution rebound detaches immediately and keeps
its current world velocity, including inherited platform motion. Teleporting a platform by editing position
is a teleport, not carry. Kinematics follow their prescribed velocity through
solids; games must reverse them at route endpoints and avoid crushing riders.
Static colliders ignore velocity. Dynamic pairs separate and exchange normal
impulses using inverse mass. Restitution uses the maximum of the pair; friction
uses the maximum coefficient and limits tangential impulse by normal impulse.
Drag damps intrinsic velocity; `max_speed` caps it after forces. Collision impulses can exceed the cap until the next
tick so an unstoppable kinematic cannot leave a slower body tunneling.

`CollisionFilter` uses two bitsets. Each mask must accept the other's layers.
The same filter applies to static solids, kinematics and triggers. Set
`is_trigger` on any collider to disable solid response and report
`TriggerEvent { trigger, other, phase }`. Enter/stay/exit compare consecutive
step boundaries. A full swept passage also emits enter and exit in that tick.
Events append in stable pair order; removing a collider or changing its filter
produces exits on the next step. Trigger-trigger overlap reports both directions.

The solver depenetrates initial overlaps, applies forces, and processes earliest
continuous contacts for relative translation. Circle/sphere contacts use exact
rounded box faces, edges and corners. A reusable `UniformGrid2D`/`UniformGrid3D`
provides sorted broadphase candidates for physics or tilemap callers. Large
colliders and long queries fall back to a bounds scan to bound memory.

Identical insertion order, inputs and fixed tick durations produce deterministic
results on the same build/platform. Floating-point results are not promised to
be bit-identical across architectures. No rotation, joints, stacking stability
or arbitrary polygons are provided. Depenetration is bounded to 16 passes at spawn and the final boundary;
resting normal constraints use up to 32 passes for touching chains;
`StepReport::unresolved_overlaps` flags impossible or crowded configurations.
Continuous contact processing is bounded to 256 contacts per tick; exhaustion
freezes the remaining motion and reports `dropped_time`. Use ordinary fixed
rates and avoid extreme stacks. Touching shapes are not positive overlaps.

The existing `Body2D`/`Body3D::move_and_slide` APIs remain independent. Arena and
Meadow keep their existing movement and game-owned gravity. The standalone
swept bodies also depenetrate initial overlaps with static AABBs.

Run the interactive example with `cargo run -p rayengine --example physics`.
It has pushable boxes, bouncing balls, a moving platform and a trigger zone.
For broadphase scaling: `cargo bench -p rayengine-core --bench physics`.

## Filtered collision queries

`Shape2D`/`Shape3D::cast` query any supported shape pair without a world;
`shape.raycast(center, ray, max_distance)` queries an exact box/circle/sphere.
`Ray2` and `Ray3` normalize finite nonzero directions. The existing spatial
`Ray::cast(Aabb, ...)` has inclusive boundary rules; physics shape raycasts use
entering-contact rules consistent with the continuous physics solver.

World `raycast`, `cast_shape` and `visit_overlaps` take a `QueryFilter`. Both
collision masks must accept each other's layers. Triggers are excluded by
default; opt in with `include_triggers: true`. Exclude the casting body's ID
and any other identities through `excluded`. All body kinds are eligible.
The first cast hit is the smallest travel fraction, with the lowest `BodyId`
breaking equal unrounded-fraction ties. Overlap visits occur in ascending identity order,
with exact narrowphase tests rather than bounding-box overlap.

```rust
use rayengine_core::prelude::*;
let mut world = PhysicsWorld2D::new(4.0);
// Insert target before wall: earliest geometry still wins.
let target = world.insert(PhysicsBody2D::new(Vec2::new(10.0, 0.0), Shape2D::round(1.0)));
let wall = world.insert(PhysicsBody2D::new(Vec2::new(5.0, 0.0),
    Shape2D::box_shape(Vec2::new(0.1, 10.0))));
let hit = world.cast_shape(Shape2D::round(0.25), Vec2::ZERO,
    Vec2::new(100.0, 0.0), QueryFilter::default())?.unwrap();
assert_eq!(hit.body, wall);
assert!((hit.hit.distance - 4.7).abs() < 0.0001);
world.remove(wall);
let ray = Ray2::new(Vec2::ZERO, Vec2::X)?;
assert_eq!(world.raycast(ray, 100.0, QueryFilter::default())?.unwrap().body, target);
let mut overlaps = Vec::new();
world.visit_overlaps(Shape2D::round(2.0), Vec2::new(10.0, 0.0),
    QueryFilter::default(), |hit| overlaps.push(hit.body))?;
assert_eq!(overlaps, vec![target]);
# Ok::<(), Box<dyn std::error::Error>>(())
```

A cast reports `fraction` in `[0, 1]`, `distance` in world units, `position`
and `normal`. **Position is the cast's reference center at impact** (the ray
point for a ray), not a shape surface contact point. The normal points from
target toward caster. Relative coordinates, hit selection and contact construction
use f64 internally; public distance/fraction fields round to f32 independently
of position. On very long casts, distinct hits may share the same rounded
fraction/distance, so reconstructing position from those fields can lose
precision. World queries compare the unrounded fractions. Initial positive overlaps report fraction/distance zero,
the original center and a zero normal; use `overlap` for a separating normal.
Touching at the start hits only with inward motion. Outward motion, parallel
surface grazing and pure tangency miss. Entering contact at the end of travel
is included. Zero travel reports only positive initial overlap. Overlap
visitors require positive penetration, so touching is excluded.

Queries return `QueryError` for invalid geometry or motion, including geometry
edited through public fields. Centers and dimensions must be finite; shape
sizes/radii must be positive. Ray limits must be finite and nonnegative;
translations, their length and end positions must be finite. Overflowing
bounds, relative coordinates or combined extents are invalid geometry.
Excluded or filtered bodies are not validated. Eligible invalid bodies return
an error even if an earlier valid hit was found. Overlap visitors validate all
eligible geometry before calling the visitor, preventing partial results.

World queries are **snapshot casts**: target positions stay fixed and velocities
are ignored. They scan the current bodies directly in O(body count) time and
see inserts, removals, position/shape/filter/trigger edits and completed steps
immediately, without rebuilding an index. No query acceleration structure is
introduced. For a relative-motion query against one moving target, call
`caster_shape.cast(start, caster_delta - target_delta, target_shape, target_start)`.
The returned fraction is the time fraction of the two motions. Its distance
and position describe the relative path; reconstruct the caster's actual world
center as `start + caster_delta * fraction`. The solver performs relative motion
internally, but one snapshot world query does not predict moving-target impacts.

Run the focused headless example with
`cargo run -p rayengine-core --example collision_queries`. Projectile lifetime,
damage, faction rules and decisions about trigger eligibility remain game-owned.
