# rayengine-actions

An optional CPU-only library for agent action queues and shared interaction slots.
No renderer, ECS, native toolchain or engine dependency is required. It is
repository-only; use a Cargo path dependency on `plugins/actions`.

The API starts with [a complete small game-owned example](examples/interaction.rs):
two workers reserve one workstation, walk to it, perform work, and release it.
Run both completion and player cancellation paths:

```sh
cargo run --locked -p rayengine-actions --example interaction
cargo run --locked -p rayengine-actions --example interaction -- --cancel
cargo run --locked -p rayengine-actions --example interaction -- --interactive
cargo test --locked -p rayengine-actions
```

The `--cancel` flag simulates the player's cancel input on tick 3. For live player
input, `--interactive` advances one tick per Enter; type `c` to cancel worker 0 or
`q` to quit with cleanup. In a graphical game, call the same `cancel` method from
the input handler. Movement, positions,
products and cleanup effects in the example belong to the game.

## Lifecycle and ordering

Games own an `ActionSystem<World>`, register agents and targets, implement
`Action<World>` hooks, and call `step` from their simulation loop. Each agent has
a FIFO queue; each step visits agents in registration order and invokes at most
one start/tick hook per agent. An action is `Pending` until it reaches the head.
The first step sets it `Running` and calls `start` exactly once. Subsequent steps
call `tick`. A `Continue` result keeps it running; `Succeeded` or `Failed(message)`
finishes it immediately. Starting the next action always waits for a later step.

Every enqueue requires a nonzero tick budget. Start and tick each consume one
unit, including busy-wait retries. A `Continue` on the final unit fails with
`TickBudgetExceeded`; success/failure on that final call takes precedence. Pending
time does not consume budget. A queued job waits behind finitely budgeted jobs,
but only advances if the game steps the system. Repeated game interruptions can
delay queued jobs indefinitely. Budgets measure simulation calls, not wall time;
game hooks must return promptly. No hook runs asynchronously.

Every terminal transition calls `finish(world, context, terminal_state)` exactly
once, including cancellation of a never-started pending job. Track any transient
game state within the action so cleanup is safe for both pending and running
jobs. Existing reservations remain valid during `finish`, which may release them
but cannot acquire claims. After the hook, the scheduler releases **all** of the
action's remaining claims, removes it from the queue, drops the action, and retains
an `ActionOutcome`. `state` exposes live and retained terminal state. Regularly
consume `drain_finished` to bound history memory; draining forgets terminal IDs.
Queue size is controlled by the game when enqueueing.

`cancel` handles one job; `interrupt` cancels the current head and puts a new job
before the remaining queue. `remove_agent` cancels its entire queue.
`remove_target` fails each action holding that target and releases all its other
claims too; non-owning waiters see `UnknownTarget` on their next attempt and the
game decides how to fail or retarget. Call removal **before** deleting the
corresponding world objects so hooks can still clean up game data. `shutdown`
cancels everything and clears agents/targets; it is idempotent. Call it while the
world still exists. Dropping the system frees storage but cannot run world cleanup
hooks. Dropping alone is suitable only when the game world is being discarded.

With unwinding panics, the scheduler catches start/tick/finish panics, records
`Failed(HookPanicked)`, releases claims and resumes the panic. Bulk removal and
shutdown finish all affected actions before resuming the first panic. A finish
panic never causes a second finish call. This cannot roll back partially applied
game effects or handle aborts, process termination, or panicking destructors.

## Reservations and ownership

A target has a nonzero slot capacity; one slot makes it exclusive. Only action
contexts can acquire/release claims, so each claim has a scheduler-assigned
agent/action owner. `reserve` is immediate and nonblocking: a full target returns
`Busy` without allocating a waiter. Retry from later ticks within the required
budget or fail immediately. An action may claim multiple targets but at most one
slot per target. Repeated reservation of the same target returns the same token.
There is no cross-agent fairness guarantee: earlier agents get the first attempt
at a newly free slot. Avoid hold-and-wait cycles by acquiring multiple resources
in a game-defined order or releasing partial acquisitions on `Busy`. The tick
budget bounds failures even if the game's acquisition policy deadlocks.

`Reservation` tokens are copyable proofs, not RAII guards. Copies do not duplicate
ownership. Only the owning action can release them. Cancellation, failure,
success, agent removal, target removal and shutdown invalidate claims reliably.
Releasing and reacquiring gives a new token. Stale tokens and tokens from another
system never match future claims; opaque identities are never reused within the
process. Removing a target and registering a replacement also gives a new ID.
The game maps its own stable entity identities to runtime `AgentId`/`TargetId`.

## Save/load contract

Runtime IDs, tokens, queues, trait objects and outcomes are intentionally not
serialized. Save game-owned agent/target identifiers, capacities, activity
parameters, queue order and persistent progress. Restore into a **fresh** system:
register surviving agents/targets, rebuild the ID mapping and enqueue rebuilt
actions in saved order. Restored actions start pending and must reacquire claims
through their new contexts before resuming movement or applying effects. Do not
copy a saved `Running` state or assume a serialized token owns a live slot.

If preserving the exact saved ownership matters, let restored owner actions
reserve first during reconstruction steps and enqueue other contenders afterward,
or gate their acquisition with game-owned restoration state. Resolve missing
objects, capacity changes and busy claims in game code (retry within budget,
retarget, or fail). Avoid replaying persistent effects: save the game's progress
and make reconstructed hooks resume from it. For an in-place load, shut down the
old scheduler with the old world **before** replacing either. Old tokens cannot
release new claims.

Need scoring, autonomy selection, social rules, navigation and object-specific
activity effects stay game-owned. The engine does not dispatch this library;
plugins are selected through ordinary Cargo dependencies.
