# Responsive windows, cameras and UI

The window is resizable and requests high-DPI support automatically. The default
reference view is 960×540 logical UI units. Drawing happens in a render texture
whose dimensions match the fitted physical content area; it is then presented
inside the logical window rectangle. This keeps the 3D aspect ratio independent
of the outside window, and renders ordinary UI at framebuffer resolution.

[`crate::core::viewport::ScaleMode`] supplies three explicit policies:

- **Fit**, the default: preserve reference aspect and visible area, adding bars.
- **IntegerFit**: use whole logical scale factors when enlarging, nearest filtering,
  and a reference-resolution target. Smaller windows shrink fractionally.
- **Expand**: preserve reference height and fill the window; wider windows reveal
  more world and give UI additional horizontal space.

```rust
use rayengine::prelude::*;
let reference = Vec2::new(960.0, 540.0);
let wide = Viewport::new(Vec2::new(2400.0, 900.0), reference, ScaleMode::Fit).unwrap();
assert_eq!(wide.origin.x, 400.0);
assert_eq!(wide.logical_size, reference);
assert!(wide.screen_to_ui(Vec2::new(10.0, 10.0)).is_none());
```

A `Camera2D` specifies its center and visible world height, not a pixel zoom.
A `Camera3D` specifies a vertical FOV. With Fit, both the reference aspect and
their visible area remain stable when the window changes. With Expand, vertical
coverage stays stable and horizontal coverage changes. Resize causes render
target recreation only when pixel dimensions actually change.

UI coordinates are independent of world cameras. Use `UiRect` anchors and pivots
against `ui.logical_size` to maintain margins:

```rust
use rayengine::prelude::*;
let health_panel = UiRect::bottom_right(Vec2::splat(-20.0), Vec2::new(200.0, 48.0));
let bounds = health_panel.resolve(Vec2::new(960.0, 540.0));
assert_eq!(bounds.max, Vec2::new(940.0, 520.0));
```

`Update::pointer` is already in UI units and returns `None` for bars or an
unfocused window or a captured cursor. Interactive regions use the same current
reference-unit layout; see [interactive UI](crate::guides::interactive_ui).
For world picking, use `Camera2D::screen_to_world` with a
logical screen pointer and the same viewport. Camera rotation is included in
both conversions. Raw raylib mouse positions are logical window coordinates;
do not multiply them by DPI again.

IntegerFit's whole factors are logical-window factors. A desktop using fractional
physical DPI can still produce fractional physical pixel sizes. For pixel-art
games, check the intended desktop scaling and choose the reference resolution
accordingly. Normal Fit renders at physical resolution, rather than stretching
a low-resolution UI buffer.

Minimized windows pause simulation, release held actions, keep polling backend
events, and sleep briefly. Restoring the window does not queue the entire paused
interval as simulation work. Reference/window configuration is validated before
opening the backend, and target allocation is bounded to 8192 pixels per axis.
