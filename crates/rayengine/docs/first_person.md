# First-person controller

`FirstPersonController` is an optional, allocation-free CPU helper over
`Body3D`. It combines mouse look, yaw-relative movement, sprinting, gravity,
jumping and an interpolated eye camera. It is available in both preludes and
as `rayengine::first_person` / `rayengine_core::first_person`. It needs no ECS
or graphics context; Meadow stores it as its Explorer component.

```rust
use rayengine::prelude::*;
let mut player = FirstPersonController::new(
    Vec3::new(0.0, 0.9, 6.0), Vec3::new(0.8, 1.8, 0.8),
    FirstPersonConfig { walk_speed: 5.0, ..Default::default() },
)?;
let ground = Aabb3::from_center(Vec3::new(0.0, -0.5, 0.0), Vec3::new(40.0, 1.0, 40.0));
player.step(FirstPersonInput {
    movement: Vec2::new(0.0, -1.0), look_delta: Vec2::new(10.0, -2.0),
    ..Default::default()
}, 1.0 / 120.0, &[ground]);
assert!(player.body.grounded);
let camera = player.camera(0.5);
assert_eq!(camera.vertical_fov, 75.0);
# Ok::<(), FirstPersonError>(())
```

## Timing and input

Call `step` once per fixed update with the fixed `tick.dt`. Movement and
keyboard turning use seconds; mouse motion is accumulated logical-window
displacement and is multiplied only by sensitivity, never dt or viewport scale.
The runner consumes input edges and relative motion after each tick, so catch-up
ticks do not replay a click, jump press or mouse displacement. CPU callers must
call `Input::consume_edges` themselves. A zero dt changes only look angles.

Movement input is right/back positive, with forward `(0, -1)`. Opposite digital
actions cancel; diagonals normalize to avoid faster motion. Yaw zero faces -Z;
positive yaw turns toward +X. Movement remains horizontal at every pitch.
Use `FirstPersonInput::from_actions` with game-defined `FirstPersonActions` IDs;
physical keys belong to SDK `Bindings`. Sprint and keyboard look IDs can be None.
For menus, use `from_view(input.routed(blocked_actions, true), actions)` to mask
gameplay and mouse motion. The game controls `CursorMode::Captured` / `Free`.
Focus transitions and capture changes are handled by the runner.

Render with `player.camera(frame.alpha)`. Position interpolates previous/current
fixed state; view angles use the latest input without interpolation delay.
Eye offset is relative to the body's center in world axes; default Y is 0.7 for
the default demo's 1.8-high body. Choose it for your own collision dimensions.
Vertical FOV stays with the camera as the viewport resizes. Do not step physics
in `draw`, scale mouse displacement by dt, or replay displacement in catch-up.

## Configuration and ownership

`FirstPersonConfig` chooses walk/sprint speeds, exponential horizontal response,
jump speed, positive gravity, fall-speed limit, mouse sensitivity, keyboard
turning speed, pitch limits, eye offset, FOV, coyote and jump-buffer durations.
Defaults match Meadow. Response zero selects immediate horizontal velocity;
positive response smoothly approaches the requested velocity. Grace durations
zero disable grace/buffering while still permitting a grounded jump edge.
Holding jump does not auto-repeat. A buffered jump fires on the tick after landing.

Construction and `set_config` validate finite rates, positive gravity/fall limit,
and pitch strictly inside the vertical poles. Failed reconfiguration preserves
the old state. Successful reconfiguration clamps pitch and clears pending grace
and jump state. `set_look` accepts finite angles, wraps yaw and clamps pitch.
Tick inputs require finite axes in [-1, 1], finite pointer motion and finite
nonnegative dt; programmer violations panic like the core collision API.

Spawn outside solid boxes. This uses the existing X/Z/Y swept box solver, with
no depenetration, slopes, step-up, rigid-body forces or moving-platform support.
The game supplies the collision snapshot each tick. Public `body` supports
inspection/game edits; retain its numeric/dimension invariants. Horizontal
velocity is controlled by the helper, so external horizontal impulses require
game policy. Gravity uses fixed-step integration, not an assertion of identical
trajectories at different simulation frequencies.

`teleport` sets current/previous position together and clears velocity, contact
and pending jumps, while keeping view angles. Games decide respawn conditions,
checkpoint destinations and look resets. Pickups, streaming, voxel editing,
health, survival rules and save data remain game-owned. Meadow delegates its
controller/camera and retains reset/checkpoint/orb logic.

## Minimal runnable game

```sh
cargo run -p rayengine --example first_person
cargo run -p rayengine --example first_person -- \
  --hidden --frames 30 --size 800x1000 --screenshot artifacts/first-person.png
```

The complete source below is included directly from
`crates/rayengine/examples/first_person.rs` and checked by rustdoc. WASD moves,
mouse looks, Space jumps, Shift sprints, R resets, and Escape exits. It uses
static boxes with no external assets. [Timing/input](crate::guides::timing_input)
and [responsive cameras](crate::guides::responsive) cover the shared lifecycle.
