# Rendering quality and anti-aliasing

Fit and Expand default to the physical content-area resolution. Select higher
world quality without changing camera height/FOV, UI layout, or pointer/world
coordinates:

```rust
use rayengine::prelude::*;
let mut config = Config::new("Smooth geometry");
config.render_quality = RenderQuality {
    render_scale: 2.0,
    anti_aliasing: AntiAliasing::Fxaa,
};
config.validate()?;
# Ok::<(), Error>(())
```

`render_scale` is exactly 1 or 2, applied to each rounded
native content dimension. 2 means twice the width **and** height: four times the
world pixels. Fractional render scales are rejected so scaling preserves the
native target aspect exactly, including odd and very small dimensions. `anti_aliasing` is `None` or `Fxaa`; both can combine with any
supported scale in Fit/Expand.

| Mode | World rendering | Resolve | UI |
| --- | --- | --- | --- |
| Native (default) | physical content pixels | ordinary presentation | same native target |
| FXAA | physical content pixels | luminance-directed edge filter | separate native layer |
| 2× | twice each physical content dimension | bilinear downsample | separate native layer |
| 2× + FXAA | twice each physical content dimension | edge filter sampling internal world texels and downsample | separate native layer |
| IntegerFit | reference pixels | point sampling | reference pixels |

FXAA is a single-pass, luminance-directed approximate filter, following the
screen-space approach described in [NVIDIA's FXAA white paper](https://developer.download.nvidia.com/assets/gamedev/files/sdk/11/FXAA_WhitePaper.pdf).
It finds diagonal contrast, chooses an edge direction, samples along that edge,
and rejects samples outside the local luminance range. This is a small GLSL 330
variant, not a selectable NVIDIA preset. It operates on the actual game world
texture, writing a native offscreen output; it does not depend on window MSAA.
It can soften fine world detail and cannot eliminate temporal/subpixel shimmer.
Supersampling rasterizes additional geometry samples. At 2× the bilinear
resolve averages four internal samples per output pixel.

In quality modes, `Frame::ui` draws into a separate transparent native-resolution
layer. All its calls compose over all world/`with_raylib` passes at the end of the
frame, irrespective of their interleaving. UI call order is preserved. `clear`
clears the world and the transparent UI layer, so earlier UI also disappears.
In the default native and IntegerFit modes, the existing immediate pass order
remains unchanged. Games should draw world passes before UI in all modes for
consistent ordering. World text drawn through `with_raylib` is filtered with the
world; native text belongs in `ui`.

World and UI targets consistently store premultiplied RGBA. SDK colors and
`Frame::clear` take straight RGBA, drawing accumulates coverage alpha, and both
resolve and final window presentation use premultiplied blending so
translucent panels and glyph edges are not darkened by multiplying alpha twice.
Advanced raw shaders/blend modes must preserve that target representation; use
`Frame::clear` for straight-color clears.
`UiCanvas::pixel_scale()` exposes target pixels per logical UI unit for raw
custom-font drawing. Font atlases must still be rasterized at a suitable physical
size: supersampling does not add detail to a small atlas. The comparison fixture
loads a 64-pixel iA Writer Mono atlas through raylib; its SIL license is included.
Typed font assets and named font declarations are tracked in issue #47.
The native probe compares both default and custom-font glyph pixels exactly
across quality modes at the same DPI.

IntegerFit explicitly requires scale 1 and `None`; other combinations fail before
window creation. It retains reference-resolution world/UI targets and point
presentation, including when shrinking below the reference size. Whole-number
scales remain logical window scales; fractional desktop DPI can produce
fractional physical pixels. Texture/font sampling inside game passes remains
under game control.

## Validation, resize and resource ownership

Settings are validated before opening the window, including an initial 1× DPI
allocation plan. Every active frame recomputes targets from the logical viewport
and actual framebuffer/window ratio. Resizing or moving between DPI scales
recreates targets only when their dimensions change. Minimized frames do not
allocate. Letterbox origin and pointer mapping stay in logical window units.
Screenshots capture the final window, including native UI and letterbox bars.

Requests exceeding 8192 in either dimension, the actual device texture limit, or
512 MiB of estimated total target storage fail explicitly. Unlike the older
`Viewport::render_size` helper, the quality planner never silently reduces an
oversized request. The allocation estimate uses 4 bytes RGBA + 4 bytes depth per
pixel on every target (conservative for 24-bit depth). Device allocation or FBO
completeness failures return a backend error. Shader compilation failure returns
an error rather than accepting raylib's default-shader fallback.

Native/pixel modes use one target. Quality modes use the world target plus two
native targets (transparent UI and resolved output). At scale 2 this is six times
the default target pixels: four world + one UI + one output. The estimate excludes
font atlases, game assets, framebuffer/driver overhead and implementation padding.
Old targets are dropped **before** resize allocation so the checked steady-state
bound also bounds old/new overlap. Allocation failure ends the run cleanly.
Targets, filters, and partially allocated resources drop while the context is
alive on success and on errors. Engine clear always resets the native UI layer.

Diagnostics include `render_scale`, `anti_aliasing`, world `render_size`, native
`output_size`, `render_target_bytes`, and active GL vendor/renderer/version strings.
Render wall time includes allocation, game submission, filtering and composition;
present wall time includes the window blit, driver stalls and swap. These are
CPU wall times, not GPU timer queries.

## Comparison fixture and profiles

```sh
cargo run -p rayengine --example render_quality -- native
cargo run -p rayengine --example render_quality -- fxaa
cargo run -p rayengine --example render_quality -- 2x
cargo run -p rayengine --example render_quality -- 2x-fxaa
cargo run -p rayengine --example render_quality -- pixel
python3 scripts/quality_comparison.py
```

Every comparison submits the same static diagonal lines, cube, sphere, UI panel,
and default/custom font labels. The script builds release code, rotates profile
order, checks identical submission counts, and records repeated uncapped frame,
render and presentation wall times, screenshots, toolchain, revision/dirty files,
CPU/OS/backend, environment, and active GL driver metadata. Use `--size`,
`--frames`, `--repeats`, and `--output` to control the comparison.
`RAYENGINE_BACKEND=wayland` selects a Wayland-enabled build. Native probes run
serially through `scripts/native_smoke.sh` and require an OpenGL display.

The `rayengine.toml` profile fields are
`render_scale` and `anti_aliasing` (`"none"` or `"fxaa"`) under `render`:

```toml
schema_version = 1

[render]
render_scale = 1.0
anti_aliasing = "none"

[profiles.smooth.render]
render_scale = 1.0
anti_aliasing = "fxaa"

[profiles.ultra.render]
render_scale = 2.0
anti_aliasing = "fxaa"
```

Load these profiles with `Config::with_optional_project(path, Some("ultra"))`,
or apply a resolved project with `Config::with_project`. Explicitly declared
fields override Rust defaults individually; omitted fields retain Rust settings.
`RunOptions::size` applies last and triggers fresh allocation validation.
Unknown anti-aliasing strings, unsupported scales, incompatible pixel profiles,
and excessive initial allocations fail in both CLI and runtime loading.
The native comparison uses [checked manifest profiles](https://github.com/nayupls1/rayengine/blob/master/crates/rayengine/examples/render_quality.toml).
See the [measured comparison](https://github.com/nayupls1/rayengine/blob/master/docs/render_quality_comparison.md) for
resource and performance results and their environment limits.
