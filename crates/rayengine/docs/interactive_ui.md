# Interactive UI and input routing

`UiState` handles buttons, focus and drags on the CPU. Submit game-owned
`UiRegion` IDs and current bounds once per fixed tick, before gameplay. Draw
the resulting `UiResponse`s with `UiCanvas::button`, or use the ordinary drawing
primitives for your own appearance. The same interaction/layout works in 2D
and 3D games.

Run the complete menu example below with:

```sh
cargo run -p rayengine --example menu
# Native Wayland:
cargo run -p rayengine --features wayland --example menu
```

Escape opens/closes the menu, Tab/Down/Up changes focus, and Enter/Space selects.
The header is draggable. With the menu closed, A/D moves the cube, mouse motion
orbits the camera, and click/Space changes its color. Menu clicks never change
the cube's color, including the tick that closes the menu.

## Regions and interaction

Use stable, unique IDs, independent of list order. Resolve `UiRect` with
`context.viewport.logical_size` each update, and draw against the same current
logical size. `Update::pointer` is already mapped to UI units. Bars, captured
cursors and unfocused windows yield `None`; do not apply DPI a second time.

Last-listed regions win overlapping pointer hits. Disabled regions block
click-through, but cannot capture, activate, or focus. Keyboard navigation
wraps through enabled, focusable regions in submitted order. Clicking empty
space or cancelling clears focus. Simultaneous forward/backward navigation
presses do not move focus.

A button activates on release inside the region where the press began. Moving
over a different button while held does not transfer capture. Quick taps retain
both edges and activate once. Focused keyboard activation uses a press edge.
Events last one update; process them during that update, rather than in drawing.
Catch-up ticks consume the edges once. If a frame precedes the first update,
`response` can return `None`; draw a resting appearance until state exists.

Draggable regions start immediately on pointer press and retain capture outside
their bounds. `drag_delta` and `drag_total` use UI units. Unavailable positions,
including bars, produce zero delta until a mapped position returns. Release
ends the drag; game code decides drop targets and accepted operations. Pointer
drag release does not activate a button. Keyboard activation remains available
for a focusable drag region if the game wants an alternative action.

Focus loss, input reset, removal, disabling, cancellation, or a missing held
button ends capture with `cancelled = true`. A removed captured region's
cancellation response remains available for one update. The SDK retains input
resets across minimized/paused frames, preventing a pre-pause press from
activating after focus returns. Submit an empty region list when closing a menu
to release old capture and focus.

## Explicit gameplay routing

`UiState::update` returns `UiCapture` ownership signals. It does not mutate the
raw input. Games choose the shared actions to mask with `Input::routed` and
whether relative look motion should be blocked. UI can read the original input
before gameplay receives the `InputView`:

```rust
use rayengine::prelude::*;
const ATTACK: Action = Action(0);
let mut raw = Input::with_capacity(1);
raw.set(ATTACK, true);
let gameplay = raw.routed(&[ATTACK], true);
assert!(raw.pressed(ATTACK));
assert!(!gameplay.pressed(ATTACK));
assert_eq!(gameplay.pointer_delta(), Vec2::ZERO);
```

Modal menus usually mask all gameplay actions and look motion. Overlays can
mask only shared clicks or navigation actions while leaving movement available.
Keep the closing/opening tick masked too. Unmasked held actions resume normally;
the view does not synthesize releases or replay consumed edges. If a game wants
to suppress a held key until its physical release, that additional policy
belongs to the game. Systems receiving raw `Input` intentionally bypass masks.

## Cursor policy and icons

Return `CursorMode::Free` while the menu is open and `Captured` during mouse-look
play. The runner re-reads `Game::cursor_mode` before sampling and after fixed
updates, applies capture/release immediately, and discards the next motion
sample after transitions. Losing focus suspends capture. Set
`Config::exit_key = None` when Escape belongs to the menu; native close and
`Update::quit` still exit. The default exit key remains Escape.

`UiCanvas::icon` draws a loaded texture into reference-unit bounds and returns
false for an unloaded/stale handle. A region can share those bounds without
imposing inventory or crafting rules:

```no_run
use rayengine::prelude::*;
fn load_icon(ctx: &mut InitContext<'_, '_>) -> Result<TextureId, Error> {
    ctx.texture("assets/icon.png")
}
fn draw_icon(frame: &mut Frame<'_, '_>, icon: TextureId) {
    frame.ui(|ui| {
        let bounds = UiRect::top_left(Vec2::splat(20.0), Vec2::splat(32.0)).resolve(ui.logical_size);
        ui.icon(icon, bounds, Color::WHITE);
    });
}
```

`UiState::with_capacity` reserves reusable response storage, including one
removed-capture event. Regions can be arrays or reused vectors. After the
high-water mark, interaction and input views allocate nothing; hit testing and
navigation scan regions, and action masks/response lookup scan their slices.
Use small shared-action masks. There is no UI interaction overhead unless the
game calls it. Label measurement/drawing is separate from interaction.

Stable CPU `ui_interaction` cases measure hover, paired click updates, keyboard
navigation, and three-update drags with 1 and 32 regions. `ui_routing` compares
eight masked/unmasked actions. Native `ui_draw` measures 32 buttons with labels
and 32 texture icons. See [testing and performance](crate::guides::testing_performance)
for the existing baseline/export workflow.

## Complete menu example

The source is `crates/rayengine/examples/menu.rs`. Rustdoc includes and checks
that same source below, so the runnable game and HTML documentation stay in sync.
