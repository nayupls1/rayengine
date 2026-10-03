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
independent. Jumping away detaches. A restitution rebound detaches immediately and keeps
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
