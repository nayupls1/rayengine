# Scene switching and state stacks

[`crate::state::StateStack`] is an optional game-owned [`crate::Game`] adapter.
Existing games can keep their enum and the original lifecycle. This stack is
separate from [`crate::core::scene::Scene`], which stores gameplay entities.
A state can own a Scene, cameras, timers, UI, and any other Rust data.

Run the complete example below with `cargo run -p rayengine --example states`.
Enter starts gameplay, Escape pauses, Enter/Escape resumes, and T from pause
returns to title. The simulation timer stops while paused; the world draws
beneath the menu. Returning to title releases the gameplay mesh, and starting
again creates a new session.

## Routing and transitions

Implement [`crate::state::State`] and choose a [`crate::state::StatePolicy`]:

| Choice | Default | Behavior |
| --- | --- | --- |
| `update_below` | false | Visit lower states after this state's update |
| `draw_below` | false | Include lower states before drawing this state |
| `input_below` | false | Give lower updated states actions and pointer input |

Updates run **top to bottom**. Drawing runs **bottom to top**, starting at the
highest state that blocks drawing below it. Policies are sampled before each
traversal. A pause overlay enables only `draw_below`; a HUD that allows simulation
but consumes input enables `draw_below` and `update_below`. Input blocking is
cumulative, so a lower overlay cannot re-enable input blocked by a higher one.
Blocked states receive an empty `Input`, no UI pointer, and the original timing,
assets, focus and reset signals. Their simulation can continue independently.
The top state's cursor policy applies, even when it passes input downwards.

Callbacks request a [`crate::state::Transition`] through
[`crate::state::StateCommands::request`]. `Push` covers a state, `Pop` removes the
top, `Replace` changes just the top, `Reset` replaces the entire stack with a new
root, and `Clear` empties it. There is one pending request: a second request
returns `Err(transition)`, allowing the caller to retain or discard it explicitly.
An accepted update request immediately stops lower callbacks on that tick, even
for an overlay that normally passes updates/input through. Drawing still finishes
all selected states, with later requests rejected while the first is pending.

[`crate::Game::boundary`] runs after **each fixed update** (including individual
catch-up ticks) and after **each draw**, once all passes have ended. The stack
applies its pending transition there. No insertion/removal occurs during callback
iteration. Entered states can draw immediately but never update on the requesting
tick. After any attempted transition, input is suppressed for the next update
(actions, edges, relative motion and UI pointer); this also prevents unconsumed
draw-time input from activating a newly revealed state. Held input resumes on
subsequent ticks. Input edges are consumed by the runner after each update.

## Entry, failure, and ownership

`enter` runs once before insertion; covering/revealing a state does not run
`exit`/`enter`. `exit` runs once on removal, followed by registered asset release
and then Rust field destruction. Clearing/shutdown removes states top to bottom.
A replacement/reset **enters the new state before exiting old states**; it may
briefly require resources for both. Therefore entry must not unload or mutate
resources shared with the existing stack.

If entry returns an error, the attempted state's `exit` still runs, registered
resources unload, and its fields drop. Existing states remain entered and the
request is consumed. `exit` must tolerate partial initialization. Initial entry
errors propagate from `App::run`; later errors are recoverable and can be read
with [`crate::state::StateStack::take_error`] (the latest error is retained until
taken or superseded). The standalone adapter continues running the previous
stack. A composing game can call [`crate::state::StateStack::apply`] at its boundary
to handle the returned error directly and decide whether to retry, show an error,
or stop. A pop on an empty stack is a no-op; replace/reset can repopulate it.
An empty stack remains usable and draws only the runner's cleared background;
it does not implicitly quit.

[`crate::state::StateResources`] records **explicit exclusive ownership**, not
reference counting. Register newly created unique meshes, textures or materials
immediately with `own_mesh`, `own_texture`, etc.; each returns the handle for
storage. Cleanup releases materials, meshes, models, textures, sounds, then
shaders, reversing registration order within each kind. Dependencies must either
be shared run-owned assets or remain alive until their registered consumers exit.
`exit` can access resources before they unload; do not unload registered handles
there. For dynamic resources, keep ownership in your state and release them in
`exit`, or register them during entry. Raw raylib RAII fields drop while the
backend is still alive.

Path-loaded assets are cached and may return the same handle to multiple states.
Keep these **shared handles out of StateResources** unless the game can guarantee
exclusive ownership for their entire lifetime. Unregistered assets stay owned by
[`crate::assets::Assets`] until the run ends. The stack never scans the cache,
unloads borrowed dependencies, or unloads an asset just because a state stores
its ID. Native resources created before an entry error must be registered or
otherwise cleaned up by the partially initialized state's `exit`/Rust fields.

## Composing with a game

Pass `StateStack::new(initial, bindings)` straight to `App::run`, or keep a stack
inside your own game and forward `init`, `fixed_update`, `draw`, `cursor_mode`,
`boundary` and `shutdown` via the `Game` trait. Declare the game's bindings once;
all states use the same action IDs. `StateStack::default()` starts empty, so an
owner can `request` an initial push and then call `init`.

The runner calls `shutdown` exactly once after initialization is attempted, on
normal exit and returned errors, before assets/audio/window teardown. Forward it
to the stack so entered states receive `exit` and registered resources unload.
When manually driving a stack, apply pending work at the documented boundaries
and explicitly call `shutdown` while the backend is alive. Merely dropping a
stack drops its Rust fields; it cannot invoke context-dependent exit/unload hooks.
Panics are ordinary Rust unwinding, not recoverable entry errors; shutdown hooks
are guaranteed for returned `Result` errors, not panics or process termination.
