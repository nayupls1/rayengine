# Materials and shaders

Materials describe a mesh surface: one optional albedo texture, an optional
custom shader, tint, alpha policy, and typed parameter overrides. Both generated
meshes and imported models use the same description. Create resources during
`Game::init`, or before a drawing pass through `Frame`.

This complete example uploads a textured quad with transparent holes:

```no_run
use rayengine::prelude::*;

#[derive(Default)]
struct Leaves {
    mesh: Option<MeshId>,
    material: Option<MaterialId>,
}

impl Game for Leaves {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.mesh = Some(ctx.mesh(&MeshData {
            positions: vec![
                Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0), Vec3::new(-1.0, 1.0, 0.0),
            ],
            texcoords: Some(vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]),
            indices: Some(vec![0, 1, 2, 0, 2, 3]),
            ..MeshData::default()
        })?);
        let texture = ctx.texture("assets/leaves.png")?;
        self.material = Some(ctx.material(MaterialDesc {
            texture: Some(texture),
            alpha: AlphaMode::Cutout(0.5),
            ..MaterialDesc::default()
        })?);
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::SKYBLUE);
        let camera = Camera3D {
            position: Vec3::new(0.0, 0.0, 6.0),
            target: Vec3::ZERO,
            ..Camera3D::default()
        };
        frame.world_3d(camera, |canvas| {
            canvas.mesh_material(
                self.mesh.unwrap(), self.material.unwrap(),
                Transform3D::default(), Color::WHITE,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    App::new(Config::new("Leaves")).run(Leaves::default())?;
    Ok(())
}
```

`Canvas3D::mesh_material` accepts translation, quaternion rotation and nonuniform
scale. `mesh_material_matrix` accepts a scene's `GlobalTransform3D.0` or another
affine `Mat4`. `model_material` and `model_material_matrix` override every mesh
of an imported model without editing its native materials; they also apply the
model's native transform. These methods return `false` if any required handle
is unloaded. Ordinary `mesh` and `model` drawing remain available.

## Alpha and ordering

| Mode | Blending | Depth writes | Built-in fragment behavior |
| --- | --- | --- | --- |
| `Opaque` (default) | Disabled | Enabled | Ignore alpha; output alpha one |
| `Cutout(threshold)` | Disabled | Enabled | Discard below threshold; otherwise output alpha one |
| `Blend` | Straight alpha | Disabled | Preserve alpha |

Depth testing stays enabled by the 3D camera pass. Cutoff values must be finite
and in `0..=1`; a value equal to the threshold survives. The built-in unlit
shader multiplies texture pixels, vertex colors, material tint and draw tint.
An omitted texture uses white, and an omitted shader uses this built-in shader.

Draw opaque and cutout geometry first. Draw blended surfaces afterward, sorted
from far to near by the game. There is no automatic transparent sorting or draw
queue. Large intersecting transparent surfaces may require game-specific
splitting or another rendering technique.

The SDK flushes preceding batched raylib primitives before entering a material
state, and restores the camera pass's original blend/depth-write state before
legacy SDK drawing and on pass exit, including unwind. Consecutive materials
with the same blend policy reuse state. For direct raylib state changes or custom
shader passes, use a separate `Frame::with_raylib` pass; mixing manual state
changes into the SDK's cached material state requires managing those changes.

## Custom shaders and parameters

Load shader files with `ctx.shader(Some(vertex_path), fragment_path)` or choose
raylib's default vertex shader with `ctx.shader(None, fragment_path)`. Files are
cached by canonical vertex/fragment path pair. `shader_from_source` compiles
provided GLSL strings without file caching. Compilation/link errors return
`Error::Asset`; raylib's silent fallback to its default shader is rejected.

Mesh shaders must have `vertexPosition` at attribute location 0 and an active
`mvp` uniform. Active standard attributes use `vertexTexCoord = 1`,
`vertexNormal = 2`, and `vertexColor = 3`. Use raylib's standard uniform names for
camera/model matrices, `texture0` for albedo, and `colDiffuse` for tint. A custom
shader implements its own fragment behavior, including alpha discard. This
minimal GLSL 330 fragment shader works with raylib's default vertex shader and
the three SDK alpha modes:

```glsl
#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
uniform sampler2D texture0;
uniform vec4 colDiffuse;
uniform int rayengineAlphaMode;      // 0 = opaque, 1 = cutout, 2 = blend
uniform float rayengineAlphaCutoff;
uniform vec3 gain;
out vec4 finalColor;
void main() {
    vec4 color = texture(texture0, fragTexCoord) * colDiffuse * fragColor;
    if (rayengineAlphaMode == 1 && color.a < rayengineAlphaCutoff) discard;
    if (rayengineAlphaMode != 2) color.a = 1.0;
    finalColor = vec4(color.rgb * gain, color.a);
}
```

The SDK sets the two alpha uniforms when present. `Cutout` requires both to be
active with the types shown; the custom shader must actually discard fragments.
`Blend` uses straight-alpha output, so do not premultiply RGB.

Register a parameter once, then use its cached binding:

```no_run
use rayengine::prelude::*;

fn surface(ctx: &mut InitContext<'_, '_>) -> Result<(MaterialId, UniformId), Error> {
    let shader = ctx.shader(None, "assets/surface.fs")?;
    let gain = ctx.uniform(shader, "gain", UniformValue::Vec3(Vec3::ONE))?;
    let material = ctx.material(MaterialDesc {
        shader: Some(shader),
        parameters: vec![MaterialParam {
            uniform: gain,
            value: UniformValue::Vec3(Vec3::new(0.6, 1.0, 0.6)),
        }],
        ..MaterialDesc::default()
    })?;
    // Changes the shared default; this material's override still takes priority.
    ctx.assets.set_uniform(gain, UniformValue::Vec3(Vec3::splat(0.8)))?;
    Ok((material, gain))
}
```

Supported values are `Float`, `Int`, `Bool`, `Vec2`, `Vec3`, `Vec4`, and `Mat4`.
GLSL reflection checks the exact type during registration. Missing or optimized
out uniforms, arrays, nonfinite float components, and renderer-reserved names
are errors. Each binding belongs to one shader; using it in another shader's
material is an error. Registering the same name/type returns the same binding
and updates its default. Renderer-owned names include `mvp`, `matModel`,
`matView`, `matProjection`, `matNormal`, `colDiffuse`, `texture0`, and the two
alpha uniforms.

Before each material draw, all registered shader defaults are submitted, then
that material's overrides. Shared shaders therefore cannot leak an override into
the next material. `assets.set_uniform` edits a CPU default; the next draw sends
it to the GPU. Uniform locations and types are resolved during setup, with no
per-draw string lookup. Additional samplers and uniform arrays are outside this
initial material API; raylib access remains available for specialized pipelines.

## Ownership and cost

`MaterialId` identifies a CPU description. `ShaderId` identifies an owned GPU
program. Materials reference shaders/textures through handles; they never own or
unload those resources. `assets.unload_material` removes only its description.
Unloading a texture/shader makes dependent material draws return `false`, and
unloading a shader invalidates its `UniformId` bindings. Reloading allocates a
new handle and does not revive old dependencies. Handles belong to one run;
material and shader indices are not reused, so unload does not reclaim slot-table
capacity during that run. All GPU resources drop before the window closes.

`assets.replace_material(id, desc)` validates live dependencies, alpha policy,
and parameter types before replacing anything. Failure preserves the old
description. Replace a stale dependency with a newly loaded handle explicitly.
`assets.material(id)` borrows the current description.

Drawing creates only a fixed-size, temporary native material view on the stack,
borrowing live shader and texture resources for the synchronous draw. The view
never escapes, and never unloads shared resources. There is no SDK heap
allocation or CPU vertex conversion on this path. GPU state is captured once
per 3D camera pass after material support is initialized. Shader compilation,
material creation/editing, file caches, and uniform registration allocate;
perform those operations outside hot drawing loops. The default is unlit;
lighting, PBR and automatic world streaming remain separate work.

The serial native material probe in `scripts/native_smoke.sh` checks texture/model
pixels, alpha cutout/depth behavior, blending, batched primitive ordering, uniform
isolation, compile/type errors, stale dependencies and state restoration. See
[testing and performance](crate::guides::testing_performance) for native drawing
baselines alongside the existing CPU workloads.
