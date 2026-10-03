# Tweens, easing and screen shake

`Tween`, `Ease`, `Sequence`, `Parallel` and `Shake` live in the display-independent
`rayengine_core::tween` module and are re-exported by the SDK prelude. They
replace hand-written UI slides, door motion, damage flashes and camera shake with
game-owned values that you advance explicitly, exactly like `Timer` and
`AnimationPlayer`. Nothing is registered or advanced behind your back.

```rust
use rayengine::prelude::*;
use std::time::Duration;

let mut door = Tween::new(0.0_f32, 1.0, Duration::from_millis(600))
    .with_ease(Ease::CubicInOut)
    .with_delay(Duration::from_millis(100))
    .with_id(TweenId(1));
let mut events = Events::with_capacity(4);
for _ in 0..120 {
    if let Some(done) = door.advance(Duration::from_secs_f32(1.0 / 120.0)) {
        events.send(done); // reported once
    }
}
assert_eq!(door.value(), 1.0);
assert_eq!(events.read(), &[TweenCompleted { id: TweenId(1) }]);
```

## Easing

`rayengine_core::tween::ease` provides linear, quad, cubic, sine, expo, back,
elastic and bounce curves with in, out and in-out variants as pure
`fn(f32) -> f32` functions. `Ease` selects one of them as a copyable value. Every
curve clamps progress to `[0, 1]` (NaN counts as zero) and returns exactly `0.0`
and `1.0` at the ends. Back and elastic curves overshoot in between, so values may
briefly pass their targets.

## Tweens

`Tween<T>` interpolates any `Tweenable` value: `f32`, `Vec2`, `Vec3`, `Vec4`
(including linear RGBA colors), `Quat` (spherical, for hinges) and 8-bit RGBA
`[u8; 4]` colors (rounded and clamped per channel). Convert the latter for drawing
with `Color::new(r, g, b, a)`. Implement `Tweenable` for your own types.

- `with_delay` holds the start value once before the first cycle.
- `with_mode(TweenMode::Loop)` restarts each cycle; `TweenMode::PingPong` plays
  the next cycle backward. Repeating tweens need a nonzero duration and run
  forever unless `with_cycles(n)` is set. A ping-pong cycle is one leg, so `2`
  goes there and back.
- `with_id` sets the `TweenId` carried by `TweenCompleted`.
- `value()` reads the current value without advancing; the endpoints are exact.
- `retarget(to)` restarts from the current value toward a new one, so an
  interrupted door or menu never jumps.

Controls match the timer and sprite API. `pause` discards time until `resume`.
`reset` rewinds and rearms completion while preserving pause. `cancel` freezes the
current value without completing. `finish` jumps to the final value and returns
the completion immediately, even while paused. `advance` returns
`Some(TweenCompleted)` on the one call that reaches the end, and never again until
`reset`. Zero-duration one-shots complete on their first advance, even a
zero-length one.

Timing uses integer nanoseconds. Looping tweens never accumulate rounding drift,
splitting time into many small steps gives the same state as one large step, and
even a `Duration::MAX` step runs in constant time. A `Tween` is `Copy` whenever
its value is and never allocates.

## Sequences and parallel groups

`Sequence` runs members one after another and `Parallel` runs them together,
completing when every member is done. Members are stored in the array, `Vec` or
tuple (up to eight different types) you pass in. That storage is reused for the
group's whole life. Groups have the same controls and completion events as
tweens and implement `Animate`, so they nest.

```rust
use rayengine::prelude::*;
use std::time::Duration;
let ms = Duration::from_millis;
let mut pickup = Parallel::new((
    Tween::new(Vec2::ZERO, Vec2::new(0.0, -24.0), ms(300)).with_ease(Ease::QuadOut),
    Tween::new([255, 220, 80, 255], [255, 220, 80, 0], ms(400)).with_delay(ms(100)),
));
pickup.advance(ms(200));
let (rise, fade) = pickup.tracks();
assert!(rise.value().y < 0.0 && fade.value()[3] < 255);
```

When a sequence member completes, the time left over carries into the next member,
so tick or frame partitioning never shifts later members. `Sequence::value` reads
the running member of a same-typed sequence and `current` reports its index.
Cancelled members are skipped and paused members hold their group. An endlessly
repeating member never yields to later sequence members. Control members through
their group.

## Fixed ticks or render frames

Advance a tween where its value is used:

- **Gameplay-relevant motion** (doors that block movement, moving platforms,
  anything saved or replayed) belongs in `fixed_update` with
  `Duration::from_secs_f32(context.tick.dt)`. Keep the previous value and blend
  it in `draw` with `T::interpolate(previous, current, frame.alpha)` for smooth
  motion at any refresh rate.
- **Presentation-only motion** (menus, toasts, HUD flashes) can advance once per
  rendered frame in `draw` with `frame.delta`, the wall time since the previous
  rendered frame. It keeps running in frames without a simulation tick and is
  not limited by catch-up, so do not use it for simulation state.

Do not advance the same tween in both places.

## Screen shake

`Shake` applies trauma-based shake. Call `add_trauma(0.0..=1.0)` on impacts.
Shake strength is trauma squared, trauma decays linearly by `ShakeConfig::decay`
per second, and smooth seeded noise moves at `frequency` samples per second.
`apply_2d` and `apply_3d` return a shaken copy of a `Camera2D` or `Camera3D`;
keep your unshaken camera as game state. 2D offsets follow the camera's rotated
screen axes and roll adds to its rotation. 3D offsets move the eye and target
along the view's right/up axes, and `up` rolls around the view direction. Scale
`max_offset` to your camera's world units. With zero trauma, cameras come back
unchanged. `Shake::new` rejects nonfinite or negative limits and a nonpositive
frequency.

Run the playable example with `cargo run -p rayengine --example tweens`. Space
opens or closes a sliding door. Interrupting it continues from its current
position, and each completion slides in a UI toast. H flashes the training dummy
and shakes the camera. P pauses the world tweens, while the crate bobs forever
with a ping-pong tween whenever the world is running. The complete example below
is checked by rustdoc.
