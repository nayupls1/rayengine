# Generated meshes

Use `MeshData` for CPU geometry and `MeshId` for its uploaded GPU resource.
`MeshData` lives in `rayengine-core`: it can be generated, validated, and sent
between workers without initializing raylib. Upload in `Game::init` using
`InitContext::mesh`, or before a drawing pass using `Frame::mesh`.

This complete game uploads an indexed quad once. It also shows how to accept a
pending replacement without losing the previous mesh if upload fails:

```no_run
use rayengine::prelude::*;

#[derive(Default)]
struct Procedural {
    mesh: Option<MeshId>,
    pending: Option<MeshData>,
    upload_error: Option<String>,
}

fn quad() -> MeshData {
    MeshData {
        positions: vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new( 1.0, -1.0, 0.0),
            Vec3::new( 1.0,  1.0, 0.0),
            Vec3::new(-1.0,  1.0, 0.0),
        ],
        normals: Some(vec![Vec3::Z; 4]),
        texcoords: Some(vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]),
        colors: Some(vec![[80, 200, 120, 255]; 4]),
        indices: Some(vec![0, 1, 2, 0, 2, 3]),
    }
}

impl Game for Procedural {
    fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.mesh = Some(context.mesh(&quad())?);
        Ok(())
    }

    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {
        // When gameplay changes geometry, place CPU data in self.pending.
        // A worker can produce the data too; consume its result here.
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let mesh = self.mesh.expect("init uploaded geometry");
        if let Some(data) = self.pending.take() {
            self.upload_error = frame.replace_mesh(mesh, &data)
                .err().map(|error| error.to_string());
        }
        frame.clear(Color::BLACK);
        let camera = Camera3D {
            position: Vec3::new(0.0, 0.0, 6.0),
            target: Vec3::ZERO,
            ..Camera3D::default()
        };
        frame.world_3d(camera, |canvas| {
            canvas.mesh(mesh, Transform3D::default(), Color::WHITE);
        });
        if let Some(error) = &self.upload_error {
            frame.ui(|ui| ui.text(error, Vec2::splat(20.0), 20.0, Color::RED));
        }
    }
}

fn main() -> Result<(), Error> {
    App::new(Config::new("Generated quad")).run(Procedural::default())?;
    Ok(())
}
```

`Canvas3D::mesh` supports translation, quaternion rotation, nonuniform scale,
and tint. `Canvas3D::mesh_matrix` accepts a `glam::Mat4`; pass a scene entity's
`GlobalTransform3D.0` to include its ancestors. Drawing borrows the uploaded
resource and performs no SDK heap allocation or CPU vertex conversion. The
default material is unlit, uses a white texture, and multiplies vertex colors
by tint. UVs and normals are uploaded for material/shader use; this default path
does not introduce textures or lighting. Use `mesh_material` to apply a reusable
textured surface or custom shader; see [materials](crate::guides::materials).

## Data rules

- Triangles use counterclockwise winding when viewed from the front. Back faces
  are culled by raylib's default rendering state.
- Unindexed meshes use each consecutive three positions as a triangle. To build
  one, use `MeshData::new(positions)` or leave `indices` as `None`.
- Indexed meshes use `Some(Vec<u16>)`, with three indices per triangle and at
  most **65,535 vertices**. This follows the pinned raylib-rs builder's limit.
  Split larger geometry into multiple meshes, or use an unindexed stream.
- Positions and the selected triangle stream must be nonempty. An optional
  attribute supplied as `Some` must match the position count. `Some(vec![])`
  does not mean an omitted attribute.
- Positions, normals, and UVs must be finite. UVs outside 0..1, non-unit normals,
  unused vertices, and degenerate triangles are permitted. Normals are not
  calculated automatically.
- Missing UVs become zero; missing colors become white. RGBA colors use bytes.
  Buffer sizes must fit raylib's signed byte counts; validation checks the
  largest supported position/index buffers before conversion or GPU calls.

`MeshData::validate` returns counts or a structured `MeshError`. It allocates
nothing and can run in CPU tests. SDK upload calls it automatically and returns
an `Error::Asset` with the validation or upload failure message.

## Replacement and lifetime

`InitContext::replace_mesh` and `Frame::replace_mesh` preserve a live handle.
They upload a complete replacement before dropping the old resource, so a
validation/upload error leaves the old geometry usable. This temporarily needs
space for both GPU meshes. Before accepting an upload, the SDK checks the actual
OpenGL storage size of the position, UV, and any supplied normal/color/index
buffers. A nonzero VAO or buffer name alone is insufficient. Missing or undersized
storage returns `Error::Asset` and frees the partial mesh; queries restore the
array-buffer binding and leave VAO element bindings untouched.

Uploads also allocate typed conversion buffers and
raylib-owned CPU copies; schedule them when geometry changes, never on every
draw. This first API uses static uploads and complete replacement, with no
partial buffer updates, upload budgets, or asynchronous scheduling.

`context.assets.unload_mesh(id)` or `frame.assets.unload_mesh(id)` frees the
resource and returns whether it was live. After unloading, `assets.mesh(id)`
returns `None`, drawing returns `false`, and replacement returns an error.
Mesh slots are reused with incremented generations: old handles remain invalid,
and streaming does not grow the slot table on every unload/load cycle. The table
retains its peak slot capacity during the run. Generation overflow retires a
slot. Handles belong to one `App::run` and must not be carried to another run.

All generated meshes share one engine-owned default material. It is created
lazily and released when the last mesh unloads. Remaining meshes and the material
drop before the graphics context closes, including on initialization failure.
Raylib-rs returns a non-owning default material wrapper; a private, documented
ownership conversion installs its RAII owner. OpenGL allocation checks also use a
private render-thread FFI bridge with documented pointer/procedure invariants.
The SDK denies unsafe code outside these bridges; the CPU core continues to forbid it.

Render-thread contexts expose upload and replacement. `Update` only offers
shared asset access; GPU mutation happens during initialization or rendering.
Mutable frame access also prevents unloading/replacing a resource while its
camera pass borrows it. Game code can keep `MeshData` only as long as it needs
it: uploads copy the data and do not retain a borrow.

Use [background work](crate::guides::background_work) for bounded CPU jobs and
explicit render-thread upload budgets. Voxel/chunk storage,
face selection, meshing algorithms, and world streaming policy remain game code.

## Verification

Run CPU validation tests with `cargo test -p rayengine-core mesh`. The native
mesh probe in `scripts/native_smoke.sh` checks actual pixels, indexed/unindexed
replacement, transforms, tint, failed replacement, stale handles, repeated slot
reuse, portrait resizing, and normal/error teardown. On Linux a second native
probe rejects each `glBufferData` upload individually while allowing object
creation and the other allocations. It verifies failed creation/replacement,
partial resource cleanup, unchanged handles/geometry, rendered old pixels, and
recovery on a later successful upload. The test-only GLAD hook is scoped and
restores the original dispatch on exit or unwind; native probes run serially.
`mesh_validation` Criterion
workloads measure positions-only and indexed geometry with all attributes at
1K/10K triangles. They measure CPU validation, not GPU upload or rendering.
