# Basic 3D lighting

Use `MaterialDesc { shading: Shading::Lit, ..MaterialDesc::default() }` to opt
into the built-in Lambert material. Generated meshes and imported models use the
same material and world-space lights. The default `Shading::Unlit`, ordinary
`mesh`/`model` calls, and custom shaders keep their existing behavior.

```no_run
use rayengine::prelude::*;

fn surface(ctx: &mut InitContext<'_, '_>) -> Result<MaterialId, Error> {
    ctx.assets.set_lighting(Lighting {
        ambient: Vec3::splat(0.15),
        directional: Some(DirectionalLight {
            // Rays travel downward and away from the camera.
            direction: Vec3::new(0.0, -1.0, -1.0),
            color: Vec3::splat(0.65),
        }),
        points: vec![PointLight {
            position: Vec3::new(0.0, 2.0, 3.0),
            color: Vec3::new(0.3, 0.5, 1.0),
            range: 6.0,
        }],
    })?;
    ctx.material(MaterialDesc {
        shading: Shading::Lit,
        tint: Color::WHITE,
        ..MaterialDesc::default()
    })
}
```

## Light convention and updates

`Assets` owns one configuration per game run. `assets.lighting()` borrows it;
`assets.set_lighting(config)` validates and replaces it atomically. Invalid
settings return `Error::Asset` naming the field or point index, preserving the
previous settings. Set lights during `init` or through `frame.assets` before
`world_3d`. Worker threads may build CPU descriptions; they do not update GPU
state. Changed values upload on the next lit draw, with cached uniform locations.
All lit materials share these settings, including across multiple camera passes.
Unlit draws ignore them. Use `Shading::Unlit` to disable lighting on a surface;
zero lights with zero ambient makes a lit surface black.

There is one optional directional light and at most `MAX_POINT_LIGHTS` (4) point
lights. Extra points are rejected rather than silently truncated. Directions and
positions are world-space: `direction` describes where rays travel, so diffuse
intensity is `max(dot(normal, -normalize(direction)), 0)`. A direction must be
finite and nonzero with a finite, nonzero representable squared length. It need
not be a unit vector. RGB irradiance must be finite and nonnegative; values above
one are allowed and may saturate the render target. Ambient defaults to RGB 0.15
with no directional or point lights.

Point lights use `max(1 - distance/range, 0)^2` attenuation, multiplied by
`max(dot(normal, towardLight), 0)`. Range must be positive and finite with a
finite reciprocal. Influence is zero at and beyond range. At the exact light
position the direction is undefined, so its diffuse contribution is zero.
Within one microunit, the direction denominator is clamped to avoid division
by zero. This is a bounded artistic falloff, suitable for a small number of
local lights. It is not inverse-square physical lighting.

The fragment RGB is texture × vertex color × material tint × draw tint × summed
irradiance. Normals are normalized in the shader. RGB values are used directly;
there is no sRGB decoding, HDR tone mapping, specular, shadows, PBR, or propagated
voxel lighting. Light settings should use reasonable scene-scale values; extreme
finite values can still overflow GPU arithmetic or saturate output.

## Geometry and transforms

Provide one finite, nonzero local-space normal per vertex. Normal squared length
must be representable and nonzero; unit normals are recommended. Supply
`MeshData::normals` for generated geometry. Export vertex normals with imported
models (for OBJ, include `vn` and face normal indices). The SDK does not generate
missing normals. CPU normal validation is cached during upload/replacement or
model import, keeping draws independent of vertex count. Missing, zero, or
invalid imported normals are permitted for unlit drawing and rejected for lit
drawing. Nonfinite generated attributes are rejected by the existing mesh upload
validator before they reach the GPU. Successful replacement refreshes cached
normal validity; a failed replacement preserves the old mesh and validity.

Use `canvas.try_mesh_material`/`try_model_material` or their `_matrix` variants
to get actionable `Error::Asset` diagnostics. They return `Ok(false)` for stale
handles. The existing boolean methods return `false` for invalid lit data.
Validation happens before geometry submission. Lit transforms must be finite,
affine, and invertible with a finite inverse. Zero scales and perspective world
matrices are rejected. Rotation, nonuniform scale, and affine shear are supported
through the inverse-transpose normal matrix. Model drawing validates and applies
`world * model.transform`. If you use direct raylib access, keep its extra model
matrix stack at identity within these SDK draws; the SDK validates the supplied
world/native matrices, while raylib also applies its internal matrix stack.

The optional albedo texture, tints, vertex colors, `Opaque`, `Cutout`, and `Blend`
policies work exactly as in the [material guide](crate::guides::materials).
Draw opaque/cutout surfaces first and blended surfaces afterward, far to near.
Lit shading cannot be combined with `MaterialDesc::shader`; custom shaders own
their lighting and should use `Shading::Unlit`.

## Ownership, demo, and checks

Materials remain CPU descriptions borrowing texture handles. Built-in lit and
unlit shaders are internal resources owned by `Assets`; both compile when the
material backend first initializes and drop before the window closes. They count
as two owned shaders in diagnostics. There are no per-light handles or GPU
resources. Removing a material never unloads its texture. Unloaded dependencies
make drawing return false, as before. Material replacement validates everything
before replacing the description. Normal validity adds a cached flag per mesh
slot/imported model; light setup/editing may allocate, but valid draws do not
allocate SDK heap memory.

Run the self-contained comparison demo:

```sh
cargo run -p rayengine --example lighting
# Reproducible hidden probe, using the standard RunOptions:
cargo run -p rayengine --example lighting -- --hidden --frames 30 --screenshot artifacts/smoke/lighting.png
```

Four columns show generated unlit/lit, then the same geometry imported as OBJ
unlit/lit, with matching textures, rotation, and nonuniform scale. Left/Right
change sun direction; A/Z change ambient; R/F change point range; D/P toggle the
sun/point light. The demo creates its tiny OBJ/texture fixtures in a temporary
directory and removes them on exit.

CPU tests cover configuration limits, finite data, and transform requirements.
The serial `native_lighting_smoke` probe checks pixels for both geometry paths,
light updates/removal, directional sign, point attenuation and the four-light
limit, inverse-transpose normals, texture/tints, alpha modes, rejected data,
unlit behavior, and dependency cleanup. It runs in `scripts/native_smoke.sh`.
The script also runs the demo with a fixed frame count and saves a screenshot.

`draw_submission/opaque_100` and `draw_submission/lit_100` compare the same
normal-bearing indexed quad, identity transform, white albedo/tint, opaque alpha,
100 draws per iteration, and 64×64 target. The lit fixture has ambient, one sun,
and four points. Setup and light updates are outside measurement. Timings include
CPU submission and driver stalls, without GPU timers; they are not FPS estimates.
Use `scripts/render_benchmark.sh save lighting-v1 'draw_submission/(opaque|lit)_100'`
to save estimates, samples, and environment metadata. See
[testing and performance](crate::guides::testing_performance) for comparison rules.
