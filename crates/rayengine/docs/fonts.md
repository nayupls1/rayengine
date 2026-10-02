# Custom fonts and sharp UI text

Games can load multiple TrueType/OpenType outline fonts into typed `FontId`
handles. No font downloads are needed for `cargo run -p rayengine --example fonts`:
the example ships unmodified Liberation Sans and Press Start 2P with their SIL
Open Font License notices. It shows labels, measured ink backgrounds, and
centered button labels at 16, 24, and 40 UI units. Resize the window or use
`--size 960x540`, `--size 1920x1080`, and `--size 800x1000` to compare scales.

```no_run
use rayengine::prelude::*;
# fn load(ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
let body = ctx.font("assets/body.ttf", FontOptions::default())?;
let pixel = ctx.font("assets/pixel.ttf", FontOptions {
    raster_size: 16,
    sampling: FontSampling::Nearest,
    rasterization: FontRasterization::Fixed,
    ..Default::default()
})?;
let style = TextStyle { spacing: 1.5, ..TextStyle::new(body, 24.0) };
let metrics = ctx.assets.measure_text("Hello", style)?;
assert!(metrics.size.x > 0.0);
# let _ = pixel;
# Ok(()) }
# fn draw(frame: &mut Frame<'_, '_>, body: FontId) {
frame.ui(|ui| {
    let style = TextStyle::new(body, 24.0);
    let measured = ui.measure_text("Hello", style).unwrap();
    let drawn = ui.text_with("Hello", Vec2::new(20.0, 20.0), style, Color::WHITE).unwrap();
    assert_eq!(measured, drawn);
});
# }
```

`TextStyle::size` is an em size in logical UI units. `spacing` adds a gap between
characters; `line_spacing` adds to the font's intrinsic line height. `size` in
`TextMetrics` gives line advances and line-box height. `ink_bounds` gives the
outline bounds relative to the top-left line box, including negative bearings
and descenders. Antialias coverage can occupy an extra target pixel around an
outline. Empty text has zero metrics, and whitespace has zero ink bounds.
Tabs advance one space. Newlines start another line. The same layout function
places glyphs and measures them, so measurements stay stable across DPI and
quality changes. This initial text API is left-to-right, without kerning,
shaping, bidi, wrapping, or automatic script fallback.

Set `UiButtonStyle::font = Some(id)` and `spacing` to select a font per button.
Custom button labels center their ink bounds. `try_button` reports label
errors; the existing `button` helper omits a failed custom label while drawing
its body. `None` retains the built-in font. Existing `ui.text` also retains the
built-in font; use `text_with` for custom fonts and configurable spacing.

## Rasterization and sampling

The engine uses fontdue to parse and rasterize outline coverage and owns RGBA
atlas textures. `Smooth` uses bilinear filtering; `Nearest` uses point filtering.
`Adaptive` starts at `raster_size` and adds power-of-two atlas sizes when text's
em size in actual target pixels increases. The target-to-UI ratio incorporates
UI resizing and physical framebuffer DPI, and also any higher internal render
resolution. It uses the larger axis if target rounding differs by a pixel.
Returning to a previous size reuses its atlas. Logical layout does not change.
No low-resolution atlas is enlarged under the adaptive policy.

`Fixed` intentionally keeps the declared raster size when enlarged. Pair it
with `Nearest` for pixel fonts and whole-number display scales. `IntegerFit`
also presents a reference-resolution target with point filtering: the font
atlas follows that target, preserving its explicit pixel-art behavior. Nearest
atlas filtering alone cannot override bilinear final presentation in Fit/Expand.
The font API requires no window-MSAA assumption. A 2x supersampled target uses
a correspondingly larger adaptive atlas before final downsampling; rendering
quality selection itself belongs to issue #48.

Fonts are cached by canonical path plus normalized options (coverage is sorted
and deduplicated). Changing the file on disk does not replace a live handle;
unload and reload to read new data. Raster size is 8..=512; adaptive requests
above 512 target pixels fail explicitly. Coverage is limited to 1024 unique
printable Unicode characters. Font files are limited to 16 MiB, each atlas to
4 million pixels (16 MiB RGBA), and all atlases of one handle to 64 MiB. Packing
and glyph sizes are checked before raster bitmap allocation. Large glyph sets
at large sizes can exceed these limits and return an error. Atlas variants are
retained until unloading, avoiding eviction of textures referenced by queued
drawing. First use at a new size may allocate on the render thread; avoid
changing text sizes continuously in latency-sensitive UI.

## Fallback, errors, and ownership

The default coverage is printable ASCII. Add supported Unicode to `glyphs`.
Space and `?` are always included; a font lacking either fails to load.
A character missing from the requested coverage or source font becomes `?` in
both measurement and drawing; `missing_glyphs` reports the substitutions.
Missing files, corrupt/unsupported outline fonts, stale handles, NUL text, and
invalid or excessive sizes return `Error`. Failed loads are never cached and
never silently select raylib's default font. Handles belong to one run and
must not be reused across runs.

`InitContext::font` and `Frame::font` load on the owning render thread.
`Assets::measure_text` needs no GPU operation. Call `Assets::unload_font` between
passes; it frees every atlas and removes the cache entry. Reloading creates a
new handle, and the old one stays invalid. The runner drops game state, assets
(including fonts), audio, and finally the window, including on early errors.
Diagnostics expose `fonts`, `font_atlases`, and `font_bytes`; CPU outline storage
and driver overhead are excluded from these GPU payload counts.

## Named declarations in rayengine.toml

The reusable `FontDeclaration` type and `FontDeclarations` reader establish the
font section for the version-1 project manifest work in issue #44:

```toml
schema_version = 1
[fonts.body]
path = "assets/body.ttf"
[fonts.body.options]
raster_size = 32
sampling = "smooth"
rasterization = "adaptive"
glyphs = " ?ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789éö"
```

```no_run
use rayengine::prelude::*;
# fn load(ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
let declarations = FontDeclarations::load("rayengine.toml")?;
let named = ctx.fonts(&declarations)?;
let body = named["body"];
# let _ = body;
# Ok(()) }
```

The reader resolves paths relative to the manifest's canonical parent, even
when invoked from another directory. It validates `schema_version`, names,
font fields and options without loading native resources. Other top-level
sections are left to the full project manifest reader; this font-specific
reader does not perform its project/profile validation. Native loading remains
explicit through the SDK. If a later named load fails, earlier successful loads
remain cached, and the error names the failing declaration and file.

## Validation

CPU tests validate coverage, options, file errors, multiline metrics, spacing,
fallback, and manifest paths/schema. `scripts/native_smoke.sh` includes
`native_font` probes with identical workloads at several sizes, fractional/UI
scales, simulated 2x physical DPI, and a 2x internal target. They compare rendered
ink with measured bounds, exercise smooth/nearest filtering, atlas caching and
stale handles, and export native comparison PNGs and environment metadata under
`artifacts/smoke/fonts/` when `RAYENGINE_FONT_ARTIFACTS` is set.
Columns compare adaptive smooth outlines, deliberately low-resolution fixed
smooth outlines, fixed nearest outlines, and a real pixel font. `*-presented.png`
shows final presentation after any downsampling. The metadata records target
sizes, atlas additions/reuse, byte counts, and GL vendor/renderer/version. For
the default ASCII coverage, the 2x DPI plus 2x render-scale probe uses a 256px
atlas; its cached 32/64/128/256 variants total about 21 MiB. Higher-resolution
atlases trade memory and first-use rasterization time for sharper text.
