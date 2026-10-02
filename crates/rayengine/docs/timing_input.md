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

Keyboard, mouse buttons and gamepad buttons are supported alongside analog stick
and trigger axes. `Action` identifies buttons and `Axis` identifies independent
analog slots; they can use the same numeric ID. Focus loss releases held actions
and zeros analog values. `Input::reset_pending` remembers a reset until
the next consumed tick, including pauses. Escape defaults to window exit; set
`Config::exit_key = None` to bind it to UI instead. Games can exit with
`context.quit()` or the window close button.

Bind keyboard and controller sources to the same movement intent:

```rust
use rayengine::prelude::*;
use rayengine::raylib::prelude::GamepadAxis;
const MOVE_X: Axis = Axis(0);
let bindings = Bindings::new()
    .bind_axis(MOVE_X, AxisBinding::new(AxisSource::Buttons {
        negative: KeyboardKey::KEY_A.into(),
        positive: KeyboardKey::KEY_D.into(),
    }))?
    .bind_axis(MOVE_X, AxisBinding {
        source: AxisSource::Gamepad {
            device: 0,
            axis: GamepadAxis::GAMEPAD_AXIS_LEFT_X,
        },
        dead_zone: 0.2,
        inverted: false,
        sensitivity: 1.0,
    })?;
let mut input = Input::default();
input.set_axis(MOVE_X, 0.5); // The runner supplies this in a real game.
let speed = 200.0 * input.value(MOVE_X);
assert_eq!(speed, 100.0);
# Ok::<(), rayengine::Error>(())
```

Sources are sampled once per active render frame. Stick values are clamped to
[-1, 1]; raylib trigger samples (-1 at rest, +1 fully pressed) normalize to [0, 1].
A scalar dead zone `d` maps magnitude at or below `d` to zero and rescales the
remaining travel as `(abs(value) - d) / (1 - d)`. Sensitivity then multiplies this
value, the result is clamped to [-1, 1], and inversion negates it. Dead zones are
per axis, not radial. Button pairs use held state; opposites cancel. `None` or
nonfinite backend samples always produce zero, including triggers. Native
sampling checks both device availability and axis count.

Each analog target selects the processed source with greatest absolute value;
the first configured source wins ties. Values are not summed, so duplicate
sources cannot amplify movement. A physical source may drive several targets;
this is allowed and has no exclusive ownership. Games can normalize a two-axis
movement vector with `clamp_length_max(1.0)` to avoid diagonal speed boosts.

Analog `input.value(axis)` is the latest value, not an edge or displacement. It
survives `consume_edges`, remains constant on subsequent catch-up fixed ticks,
and is replaced on the next rendered frame. Use `value * speed * tick.dt` for
movement or controller look. A render frame without a fixed update overwrites
analog values with its latest sample while preserving pending button edges.

Device selection is an explicit raylib slot 0..=3 for each source. There is no
implicit fallback to another controller and no persistent hardware identity.
Missing devices/axes and disconnected controller sources are neutral on the
next sample; other keyboard/controller bindings still contribute. A reused slot
can drive the mapping when a controller reconnects. Focus loss neutralizes all
held buttons, analog values and pointer motion, sets `reset_pending`, and retains
observable releases. Minimization does the same and pauses simulation. Held
physical controls resume after a focused sample. Masking is explicit per axis:

```rust
use rayengine::prelude::*;
let mut input = Input::default();
input.set_axis(Axis(0), 0.75);
let axes = [Axis(0)];
let gameplay = input.routed(&[], true).with_blocked_axes(&axes);
assert_eq!(gameplay.value(Axis(0)), 0.0);
assert_eq!(input.value(Axis(0)), 0.75); // UI can still read the original.
```

Button and pointer masks do not automatically mask analog axes. Mask all movement
and controller look axes while a menu owns them. Unmasking resumes the latest
held value; it does not synthesize button edges.

Live controls are available as `context.bindings` inside `fixed_update`. Use
`add`, `rebind`, `remove_button`, `remove`, `add_axis`, `rebind_axis`, `remove_axis`,
or `replace`. Fallible operations validate first and leave old mappings intact
on failure. Setup `bind` is fluent and validated by the runner before creating a
window; `bind_axis` validates immediately. Use `buttons`, `axis_bindings` or
`config` to inspect settings in a controls menu.

A changed or removed target is neutralized **after the current fixed callback**,
including during catch-up ticks. A held changed action emits a release; unchanged
targets retain their held values and pending transitions. New mappings begin
sampling on the next render frame, generating ordinary button transitions. This
also applies when assigning an entirely new `Bindings` to `*context.bindings`.
An invalid direct assignment (including an invalid fluent `bind`) makes `App::run`
return a configuration error after that callback, before any native sampling of
the new set. Use the fallible mutators to report errors while keeping the game
running and its previous settings intact.
Changes made and undone within a callback have no effect on the final mapping.

`BindingConfig` is serde-serializable schema 1 with numeric game-owned target IDs
and named raylib enums. `Bindings::from_config`, `replace`, and `from_json`
validate the version, duplicate target IDs, device slots, reserved button names,
finite dead zones in [0, 1), and finite sensitivity in [0, 100]. Unknown fields,
unknown enum names and out-of-range IDs fail with clear errors. Empty source
lists are accepted and canonicalize to removed targets. Repeated physical
sources are allowed under the OR/strongest-source policy above. Raw serde
deserialization alone does not validate numeric settings; call `validate` or
construct `Bindings::from_config` before use.

Persistence is explicit and game-owned. No filesystem access happens during
sampling or rebinding. `load(path)` reads and validates JSON; `save(path)` validates
before overwriting JSON. Parents must already exist; errors propagate, and games
choose whether a missing file should keep defaults. Saving is not a crash-safe
commit. Games needing atomic persistence can feed `to_json()` into their own
storage, or keep `config()` in an existing game save. Saving from a fixed callback
can stall a frame; larger games can schedule persistence outside gameplay.

```no_run
use rayengine::prelude::*;
let mut bindings = Bindings::new().bind(Action(0), KeyboardKey::KEY_SPACE);
// Paths belong to this game; the engine never selects a user directory.
let path = std::path::Path::new("my-game-controls.json");
bindings.save(path)?;
bindings.replace(Bindings::load(path)?.config())?;
# Ok::<(), rayengine::Error>(())
```

Run `cargo run -p rayengine --example controls` for keyboard/controller movement,
controller look and trigger boost with a small controls menu. It can switch WASD
and arrow keys, invert look, change sensitivity, and explicitly save/load while
running. The example chooses `controls.json` in its working directory; set
`RAYENGINE_CONTROLS_PATH` to choose another game-owned location. It starts with
defaults and only loads when requested. Escape opens/closes the menu,
Tab/Up/Down select controls, Enter activates, and the mouse can click. The menu
masks all gameplay analog axes, including the opening/closing tick.

For first-person mouse look, return `CursorMode::Captured` from
`Game::cursor_mode`. The runner hides and captures the focused window's cursor,
releases it on focus loss, and ignores motion during focus/capture transitions.
The policy is re-read before sampling and after fixed updates: return `Free`
while menus are open, and `Captured` during play. Free cursors remain the default,
including for Arena. Use [interactive UI](crate::guides::interactive_ui) for
focus, drag capture and explicit action/look-motion masks with `Input::routed`.

`context.input.pointer_delta()` supplies relative motion in logical window units.
Motion accumulates across render frames with no update, is consumed on the first
fixed tick, and is zero on subsequent catch-up ticks. Apply mouse sensitivity
directly to this displacement; multiplying by `dt` makes sensitivity depend on
frame timing. Absolute `context.pointer` coordinates remain available for UI
and aimed clicks while the cursor is free; captured cursors yield `None`.

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
