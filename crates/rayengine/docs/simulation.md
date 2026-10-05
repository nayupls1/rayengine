# Scaled simulation timelines

`SimulationClock` is an optional, game-owned CPU timeline. The host runner still
samples input, executes `Game::fixed_update`, and renders at its ordinary cadence.
Pause or accelerate the timeline inside that callback, keeping menu interaction
outside the simulation loop. You can also advance it from an independent CPU
loop with unscaled wall time. Calendar, day length, and scheduled activities stay
game-owned.

```rust
use rayengine::prelude::*;
use std::time::Duration;

let mut simulation = SimulationClock::new(100, 8);
let mut production = Timer::repeating(Duration::from_secs(1));
simulation.set_speed(4.0)?;
let plan = simulation.advance(Duration::from_millis(10));
let mut products = 0;
for tick in plan.ticks(simulation.step()) {
    assert_eq!(tick.dt, 0.01); // speed changes tick count, never physics dt
    products += production.advance(simulation.step());
}
assert_eq!(plan.steps, 4);
simulation.set_paused(true);
assert_eq!(simulation.advance(Duration::from_secs(10)).steps, 0);
# Ok::<(), InvalidSimulationSpeed>(())
```

Use the timeline's exact `step()` for duration-based systems and its ticks' `dt`
for floating-point movement. When nesting inside `Game::fixed_update`, pass the
**host** clock's unscaled step once per callback; do not multiply it by speed.
The example configures the host at 100 Hz and passes an exact 10 ms duration.
For other rates, derive that duration once with
`FixedClock::new(config.fixed_hz, config.max_catch_up).step()`, rather than
round-tripping `context.tick.dt` through `Duration::from_secs_f32` each update.
The runner's existing wall-time catch-up limit still applies before a nested
simulation timeline sees the host updates.

Speed accepts finite values in `0..=1000`, rounded to the nearest millionth.
`set_speed` rejects invalid values without changing state. `speed()` reports the
rounded value. Zero speed stops progress; explicit pause preserves the configured
speed. `set_paused` freezes fractional simulation progress, and elapsed calls
while paused do not accumulate debt. Resume and speed changes retain both partial
ticks and fractions of nanoseconds. Splitting elapsed time across calls preserves
progress when neither call hits its work limit.

Every `advance` returns at most the constructor's `max_steps`. Excess **whole
simulation ticks are discarded** and reported by `plan.dropped`; only the
fractional tick is carried forward. There is no whole-tick backlog to drain.
`ticks()` and `elapsed()` count requested work, excluding discarded and fractional
time. Execute every requested tick. A 100 Hz timeline with a four-step limit,
speed 4, and 26.25 ms elapsed requests four ticks, discards 60 ms of simulation
time, and retains a half tick. Extremely large scaled durations saturate at
`Duration::MAX`, so `dropped` can report only representable discarded time.
Work is bounded per call; choose limits across all timeline calls in a host
render frame when budgeting a game.

Route systems according to the time they own:

- Step physics worlds once per simulation tick with the simulation `Tick`. Never
  increase physics `dt` to fast-forward. Store previous/current transforms on
  each executed step, and render with `simulation.alpha()` or `plan.alpha`.
- Advance gameplay `Timer`s, `AnimationPlayer`s and `Tween`s with
  `simulation.step()` inside the simulation loop. They stop and accelerate with
  the world. Keep UI tweens and presentation-only animations on `frame.delta`.
- Send and drain simulation-owned `Events<T>` at simulation tick boundaries.
  A paused world produces no new timer, animation, or physics events. Copy any
  notifications needed by the UI into game-owned presentation state.
- Handle input, pause/speed buttons, menus, and other presentation controls once
  per host update, outside the simulation loop. Drawing and streamed audio
  servicing continue on their existing render cadence. Audio pause/pitch policy
  remains an explicit game decision.

The simulation alpha belongs to the scaled timeline; `frame.alpha` belongs to
the host timeline. They cannot be substituted or multiplied. The example samples
the simulation alpha from the last host update, so interpolation updates at the
host's fixed cadence. Pausing freezes the rendered fraction between the last two
world states. Initial state and teleports should set previous equal to current.

An input edge must become a game-owned command **before** the runner consumes it.
Never read `context.input.pressed(action)` repeatedly inside an accelerated
simulation loop. Slow updates can run zero simulation ticks, so retain pending
commands until a step consumes them. For example, this game coalesces multiple
reverse presses into one pending reverse:

```rust
use rayengine::prelude::*;
use std::time::Duration;
let mut simulation = SimulationClock::new(100, 8);
simulation.set_speed(8.0)?;
let mut input = Input::default();
const REVERSE: Action = Action(0);
input.set(REVERSE, true);
let mut pending_reverse = false; // store this in game state across host updates
pending_reverse |= input.pressed(REVERSE);
let plan = simulation.advance(Duration::from_millis(10));
let mut reversals = 0;
for _tick in plan.ticks(simulation.step()) {
    if std::mem::take(&mut pending_reverse) {
        reversals += 1;
    }
    // Held button/axis values may be read on every tick; edges may not.
}
assert_eq!(reversals, 1);
# Ok::<(), InvalidSimulationSpeed>(())
```

Queue semantics belong to the game: use bounded command storage if every press
must be retained, and define whether commands during pause are rejected or queued.
Clear stale commands on focus loss (`input.reset_pending()`), modal transitions,
or scene resets as appropriate. Input edges consumed by the host do not disappear
from commands already copied into your game state. Pointer displacement is also
consumed once; held buttons and analog axes remain valid for every simulated tick.

Run `cargo run -p rayengine --example simulation`. P pauses/resumes; 1 selects
0.25x, 2 selects 1x, and 3 selects 8x (speed selection also resumes). Click the
buttons, or use Tab/Up/Down and Enter. The world moves and produces one item per
simulated second; the green UI pulse continues while paused. R reverses once per
command; holding B boosts motion on all steps. Commands are rejected while paused,
pending commands clear on pause or focus reset, and repeated pending reverse
presses coalesce. The example needs no external assets.
