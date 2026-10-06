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

## Catalog layout and scroll state

Run `cargo run -p rayengine --example catalog` for an object catalog and action
inspector over a 3D world. Escape/controller Start opens and closes it;
Tab/Up/Down or controller D-pad Up/Down traverses stable item IDs, and
Enter/Space/controller A activates. Wheel over either pane scrolls that pane.
Drag the header to move the window, a gap to pan content, or a scrollbar thumb
to scroll. Backspace/controller B cancels focus/capture. Object selection and
actions belong to the example game; prices, categories and inventory rules
remain game-owned.

`UiLayout::list` and `UiLayout::grid` produce uniform content-local item bounds
and a content extent. `UiScrollState` retains a nonnegative offset in UI units.
Call `configure(viewport.size(), layout.content_size(count))` on each layout or
resize; it clamps the offset when content shrinks or the viewport grows.
`item_bounds` applies the viewport origin and scroll offset once. Use those same
translated bounds for regions and drawing:

```rust
use rayengine::prelude::*;
let viewport = UiRect::top_left(Vec2::splat(16.0), Vec2::new(260.0, 180.0))
    .resolve(Vec2::new(960.0, 540.0));
let layout = UiLayout::grid(2, Vec2::new(120.0, 44.0), Vec2::splat(8.0));
let mut scroll = UiScrollState::default();
scroll.configure(viewport.size(), layout.content_size(30));
scroll.scroll_by(Vec2::new(0.0, 40.0));
let mut region = UiRegion::new(UiId(100), scroll.item_bounds(viewport, layout.item(0)));
region.clip = Some(UiClip::new(viewport));
```

Submit all enabled focusable item IDs, including fully clipped items. Scrolling
and clipping affect pointer hits, not focus eligibility. On a keyboard/controller
focus change, `reveal(layout.item(index))` brings that item into view. Call reveal
on navigation changes rather than every tick, so manual scrolling does not snap
back to a focused item. Filtering/removing an item clears its focus as usual;
reordering retains the same game-owned ID. A grid uses row-major traversal through
`UiState`'s existing next/previous actions; game bindings choose navigation keys.

Create a nonfocusable background surface for each pane and set its
`scroll_target = Some(surface.id)`. Assign the same target to its item regions
and scrollbar thumb. The **topmost hovered region** explicitly routes its wheel
displacement to that ID's `UiResponse::scroll`; an overlay with no target blocks
wheel propagation. Set targets only to submitted, enabled surfaces. Multiply
wheel input by a negative game-chosen UI-unit step, then call `scroll_by`. For
content dragging use `-response.drag_delta`. `vertical_thumb` and
`drag_vertical_thumb` share geometry for a game-owned vertical scrollbar track.
Pointer capture persists outside a clip until release/cancel.

Wheel input uses `Input::scroll_delta`: positive Y means up. The native runner
accumulates both axes across render frames; `consume_edges` clears wheel input
once per fixed tick, and focus loss/reset discards it. `UiInput::from_actions`
includes this displacement. Gameplay can independently mask it with
`InputView::with_blocked_scroll(true)`. As with clicks and navigation, raw input
remains available to the UI. Keep opening/closing ticks masked; the catalog's CPU
example tests exercise this policy, including dragging and inspector activation.

## Drawing and hit-test clipping

`UiClip` has half-open logical bounds: minimum edges are included, maximum edges
excluded. UI hit regions also use this convention so adjacent buttons do not
share an edge. Invalid/empty clips reject every hit. Intersect nested clips with
`UiClip::intersect` when assigning regions; drawing scopes automatically
intersect nested clips and restore the parent's scissor after each child:

```no_run
use rayengine::prelude::*;
fn draw_catalog(frame: &mut Frame<'_, '_>, viewport: Aabb2, bounds: Aabb2) {
    frame.ui(|canvas| {
        canvas.clipped(UiClip::new(viewport), |canvas| {
            canvas.rectangle(bounds, Color::SKYBLUE);
            canvas.text("Clipped label", bounds.min, 18.0, Color::WHITE);
        });
    });
}
```

The scope clips rectangles, labels, icons, custom fonts, render targets and raw
raylib draws. Pass the same effective clip to every corresponding region. The
canvas also intersects with the logical target bounds. Raster coverage includes
a target pixel exactly when its center is inside the logical clip; this specifies
fractional/DPI edge rounding without enlarging logical hit bounds. Scissors use
the actual UI render target's pixel scale, including native quality-mode layers.
Advanced raw drawing must preserve the scope's scissor state.
