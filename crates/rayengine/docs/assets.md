# Assets and ownership

Load assets in `Game::init`. The runtime caches them by canonical path and returns
typed `TextureId`, `ModelId`, `SoundId`, `MusicId`, `ShaderId`, and `FontId` handles. Gameplay stores these handles;
the runtime owns and drops the native resources.

```no_run
use rayengine::prelude::*;

struct Artwork { portrait: Option<TextureId> }
impl Game for Artwork {
    fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.portrait = Some(context.texture("assets/portrait.png")?);
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::BLACK);
        frame.world_2d(Camera2D::default(), |canvas| {
            if let Some(texture) = self.portrait {
                canvas.texture(texture, Aabb2::from_center(Vec2::ZERO, Vec2::splat(100.0)), Color::WHITE);
            }
        });
    }
}
```

Paths are relative to the game process's working directory. The CLI runs a game
from its manifest directory, so `assets/...` works consistently there. If using
Cargo directly from a different directory, choose explicit paths or run from
the game directory. The backend expects UTF-8 paths without NUL characters;
the SDK validates them before passing strings to raylib.

Loading the same canonical path returns the same live handle. Explicit unload
invalidates that handle, and future loads get a new one: indices are never
reused during a run. `assets.texture/model/sound` return `None` after unload;
drawing an unloaded handle returns `false`. Handles belong to one run and should
not be retained for another `App::run`.

Generated geometry uses `MeshData` and a versioned `MeshId`. Upload with
`InitContext::mesh` or `Frame::mesh`, draw with `Canvas3D::mesh`, and replace or
unload explicitly on the render thread. Mesh slots can be reused without reviving
old handles. See [generated meshes](crate::guides::generated_meshes) for the
complete lifecycle, validation rules, and a compiled example.

`MaterialDesc` describes a reusable textured/shaded surface. Create it with
`context.material`, then draw generated meshes or imported models using
`Canvas3D::mesh_material` or `model_material`. Shader programs have independent
ownership; unloading a material never unloads its shared texture or shader.
See [materials and shaders](crate::guides::materials) for alpha policies,
typed uniforms, custom GLSL, replacement, and dependency lifetime rules.

Sprite sheets draw selected `SpriteRegion`s from the same cached texture handles.
See [sprite sheets and animation](crate::guides::sprites) for explicit pivots,
rotation, flips, tint, CPU playback and region validation.

Audio is opt-in:

```no_run
use rayengine::prelude::*;
let mut config = Config::new("Game with sound");
config.audio = true;
// During init: let click = context.sound("assets/click.wav")?;
// During an update: context.assets.play(click);
```

Audio initialization fails explicitly when a requested device is unavailable.
Geometric examples do not initialize audio, so they run on machines without an
audio device. Streamed music, named buses, fades, overlapping one-shots and
persistable settings are covered in the [audio guide](crate::guides::audio).

Textures and models drop before the graphics window closes. Sounds, pooled voices, cached waveforms and music streams drop before
the audio device closes. The game itself is also dropped before these handles,
so game-owned native resources can be cleaned up while the context is alive,
including when `init` returns an error.

GPU work and raylib resource creation stay on the owning thread. Use worker
threads for independent CPU work and pass results back explicitly. Do not send
the raylib thread token or GPU resources to workers.

For fonts or other native resources, use `InitContext::raylib` and
`InitContext::thread`. For specialized drawing, use a canvas's `raw` guard or
`Frame::with_raylib`. These APIs keep the basic SDK small while preserving
raylib access. Bounded CPU jobs and staged mesh uploads are covered in
[background work](crate::guides::background_work); automatic file asset pipelines
and hot reload remain outside 0.0.3.

Generated or imported CPU images can upload directly during initialization:

```rust,no_run
use rayengine::{prelude::*, raylib::prelude::Image};
fn upload(ctx: &mut InitContext<'_, '_>) -> Result<TextureId, Error> {
    let image = Image::gen_image_color(16, 16, Color::GREEN);
    ctx.texture_from_image(&image)
}
```

`texture_from_image` creates a new run-owned handle on every call. It does not
cache or write a file. The CPU image may be dropped after uploading; unload the
texture through `Assets::unload_texture` when it is no longer used. Materials
borrow texture handles, so unload dependent meshes/materials first.

Custom fonts use canonical paths plus rasterization/sampling/coverage options as
their cache key. `Assets::unload_font` frees all size variants. Measurement and
custom drawing return errors for stale handles. See [fonts](crate::guides::fonts)
for shared layout, DPI-aware atlases, pixel text, and named manifest declarations.
