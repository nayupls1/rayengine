# Timing and input

The default simulation rate is **120 Hz**, regardless of the render cap or VSync.
Each `fixed_update` receives the same `context.tick.dt`. A frame can have zero,
one or several updates. Catch-up is bounded (eight updates by default); excess
whole simulation time is discarded and reported in `RunReport::dropped_time`.

Move with units per second, never units per rendered frame:

```rust
use rayengine::prelude::*;
let mut body = Body2D::new(Vec2::ZERO, Vec2::new(24.0, 40.0));
let floor = Aabb2::from_center(Vec2::new(0.0, 100.0), Vec2::new(500.0, 20.0));
let dt = 1.0 / 120.0;
body.velocity.x = 200.0;
body.velocity.y += 1400.0 * dt;
body.move_and_slide(dt, &[floor]);
assert!(body.position.x > 0.0);
```

The collision helpers sweep each axis against static boxes and zero blocked
velocity components. This prevents tunneling along the swept axis, but does not
provide general continuous collision, rotated colliders, dynamic rigid bodies or
automatic depenetration. Spawn characters outside solids. Scanning colliders is
linear; introduce a measured broad phase when a game's world needs one.

2D grounds characters on downward positive-Y motion. 3D grounds them on downward
negative-Y motion. Arena implements one-way upper platforms as a game-specific
filter, keeping the core collision primitives small.

Declare actions once and bind physical sources:

```rust
use rayengine::prelude::*;
const JUMP: Action = Action(0);
let bindings = Bindings::new()
    .bind(JUMP, KeyboardKey::KEY_SPACE)
    .bind(JUMP, KeyboardKey::KEY_W);
```

Multiple bindings for one action are ORed. Releasing one while another remains
held does not release the action. `down` means held; `pressed` and `released`
are transitions since the last fixed update. A transition survives a render
frame with no update and is consumed only after the first following fixed tick.
Quick press/release transitions can both be present in that tick.

Keyboard, mouse buttons and digital gamepad buttons are supported. Analog axes,
rebinding persistence and more elaborate input schemes can be added when needed.
Focus loss releases held actions. Escape uses raylib's normal window-exit behavior;
games can request their own exit through `context.quit()`.

For first-person mouse look, return `CursorMode::Captured` from
`Game::cursor_mode`. The runner hides and captures the focused window's cursor,
releases it on focus loss, and ignores motion during focus/capture transitions.
Free cursors remain the default, including for Arena.

`context.input.pointer_delta()` supplies relative motion in logical window units.
Motion accumulates across render frames with no update, is consumed on the first
fixed tick, and is zero on subsequent catch-up ticks. Apply mouse sensitivity
directly to this displacement; multiplying by `dt` makes sensitivity depend on
frame timing. Absolute `context.pointer` coordinates remain available for UI
and aimed clicks.

For smooth presentation, record the position before each tick:

```rust
use rayengine::prelude::*;
let previous = Vec2::new(10.0, 0.0);
let current = Vec2::new(12.0, 0.0);
let alpha = 0.5; // frame.alpha in Game::draw
let rendered = previous.lerp(current, alpha);
assert_eq!(rendered.x, 11.0);
```

Interpolation presents between previous/current state and adds up to one fixed
tick of visual delay. At 120 Hz that is at most roughly 8.3 ms. A game may choose
the current state for parts of its presentation when lower visual delay matters.
On teleport or respawn, set previous and current to the new position together.

Fixed updates make repeatable tests practical within one build. They do not
guarantee bit-identical floating-point results across different hardware or
compilers. A game's randomness, ordering and asynchronous work must also be
controlled before promising deterministic replays.

Timers and typed events are explicit, shared CPU primitives:

```rust
use std::time::Duration;
use rayengine::prelude::*;
let mut timer = Timer::repeating(Duration::from_millis(100));
let mut pulses = Events::with_capacity(8);
for _ in 0..timer.advance(Duration::from_millis(250)) { pulses.send("pulse"); }
assert_eq!(pulses.read().len(), 2);
assert_eq!(timer.fraction(), 0.5);
pulses.clear(); // all readers have finished this tick; capacity is retained
```

`Timer::once` completes once until reset. Paused timers discard elapsed input;
repeating timers report every crossed period. Events stay in production order
and remain readable by several systems until explicitly cleared or drained.
No subscription dispatcher or hidden scheduling is introduced.
