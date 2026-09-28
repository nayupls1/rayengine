//! Render-thread generated mesh ownership. CPU data lives in rayengine-core.

use super::MeshId;
use crate::{
    Error,
    render::{v2, v3},
};
use rayengine_core::mesh::MeshData;
use raylib::prelude::*;

mod gpu;

pub(super) struct MeshAssets {
    slots: Vec<Slot>,
    free: Vec<usize>,
    live: usize,
    material: Option<DefaultMaterial>,
}

struct Slot {
    generation: u64,
    mesh: Option<Mesh>,
}

// Only the owning Material drops GPU/CPU data. The WeakMaterial is a private
// draw view of that same allocation; it is never exposed to game code.
struct DefaultMaterial {
    owner: Material,
    view: WeakMaterial,
}

impl DefaultMaterial {
    fn new(raylib: &RaylibHandle, thread: &RaylibThread) -> Result<Self, Error> {
        let view = raylib.load_material_default(thread);
        // SAFETY: LoadMaterialDefault returns a fresh maps allocation, but the
        // raylib-rs API wraps it in a non-owning WeakMaterial. This is its sole
        // owner. `view` never escapes this private store, never unloads anything,
        // and is only used while `owner` and the render context are alive. The
        // default shader/texture are retained by raylib's UnloadMaterial.
        #[allow(unsafe_code)]
        let mut owner = unsafe { Material::from_raw(*view.as_ref()) };
        if !owner.is_material_valid() {
            return Err(Error::Asset(
                "raylib could not create the mesh default material".into(),
            ));
        }
        Ok(Self { owner, view })
    }
}

impl MeshAssets {
    pub(super) fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
            material: None,
        }
    }

    pub(super) fn get(&self, id: MeshId) -> Option<&Mesh> {
        self.slots
            .get(id.slot)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.mesh.as_ref())
    }

    pub(super) fn upload(
        &mut self,
        raylib: &RaylibHandle,
        thread: &RaylibThread,
        data: &MeshData,
    ) -> Result<MeshId, Error> {
        let mesh = upload(thread, data)?;
        if self.material.is_none() {
            self.material = Some(DefaultMaterial::new(raylib, thread)?);
        }
        self.live += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index];
            slot.mesh = Some(mesh);
            Ok(MeshId {
                slot: index,
                generation: slot.generation,
            })
        } else {
            let id = MeshId {
                slot: self.slots.len(),
                generation: 0,
            };
            self.slots.push(Slot {
                generation: 0,
                mesh: Some(mesh),
            });
            Ok(id)
        }
    }

    pub(super) fn replace(
        &mut self,
        thread: &RaylibThread,
        id: MeshId,
        data: &MeshData,
    ) -> Result<(), Error> {
        if self.get(id).is_none() {
            return Err(Error::Asset(
                "cannot replace an unloaded mesh handle".into(),
            ));
        }
        // Allocate the complete replacement before dropping the old resource.
        let mesh = upload(thread, data)?;
        self.slots[id.slot].mesh = Some(mesh);
        Ok(())
    }

    pub(super) fn unload(&mut self, id: MeshId) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        let slot = &mut self.slots[id.slot];
        slot.mesh = None;
        self.live -= 1;
        // Retire a slot on overflow rather than revive an ancient handle.
        if let Some(generation) = slot.generation.checked_add(1) {
            slot.generation = generation;
            self.free.push(id.slot);
        }
        if self.live == 0 {
            self.material = None;
        }
        true
    }

    pub(super) fn for_draw(&mut self, id: MeshId, tint: Color) -> Option<(&Mesh, WeakMaterial)> {
        let slot = self.slots.get(id.slot)?;
        if slot.generation != id.generation {
            return None;
        }
        let mesh = slot.mesh.as_ref()?;
        let material = self.material.as_mut()?;
        material
            .owner
            .set_map_color(raylib::consts::MaterialMapIndex::MATERIAL_MAP_ALBEDO, tint);
        Some((mesh, material.view.clone()))
    }
}

fn upload(thread: &RaylibThread, data: &MeshData) -> Result<Mesh, Error> {
    data.validate()
        .map_err(|error| Error::Asset(format!("mesh: {error}")))?;
    // Typed conversion instead of relying on glam/raylib memory layout.
    let positions: Vec<_> = data.positions.iter().copied().map(v3).collect();
    let texcoords: Vec<_> = match &data.texcoords {
        Some(uvs) => uvs.iter().copied().map(v2).collect(),
        None => vec![Vector2::zero(); positions.len()],
    };
    let normals: Option<Vec<_>> = data
        .normals
        .as_ref()
        .map(|normals| normals.iter().copied().map(v3).collect());
    let colors: Option<Vec<_>> = data.colors.as_ref().map(|colors| {
        colors
            .iter()
            .map(|&[r, g, b, a]| Color::new(r, g, b, a))
            .collect()
    });
    let mut builder = Mesh::gen_mesh(&positions, &texcoords);
    if let Some(normals) = &normals {
        builder.normals(normals);
    }
    if let Some(colors) = &colors {
        builder.colors(colors);
    }
    if let Some(indices) = &data.indices {
        builder.indices(indices);
    }
    let mesh = builder
        .build(thread)
        .map_err(|error| Error::Asset(format!("mesh upload: {error}")))?;
    // Object creation can succeed even when glBufferData fails. Accept the
    // replacement only after every required buffer has its full storage. On
    // failure this temporary Mesh drops, leaving the store's old mesh intact.
    gpu::verify(thread, &mesh, data)?;
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{App, Config, Game, InitContext, RunOptions, Update, render::Frame};
    use rayengine_core::{
        camera::Camera3D,
        glam::Quat,
        prelude::{Transform3D, Vec2, Vec3},
    };

    fn quad(color: [u8; 4]) -> MeshData {
        MeshData {
            positions: vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
            ],
            normals: Some(vec![Vec3::Z; 4]),
            texcoords: Some(vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]),
            colors: Some(vec![color; 4]),
            indices: Some(vec![0, 1, 2, 0, 2, 3]),
        }
    }

    fn camera() -> Camera3D {
        Camera3D {
            position: Vec3::new(0.0, 0.0, 6.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            vertical_fov: 60.0,
        }
    }

    fn assert_pixel(frame: &Frame<'_, '_>, world: Vec3, expected: Color) {
        let mut image = frame.target.texture().load_image().unwrap();
        image.flip_vertical();
        let height = image.height as f32;
        let scale = height / (2.0 * (30.0_f32.to_radians()).tan() * 6.0);
        let x = (image.width as f32 * 0.5 + world.x * scale) as i32;
        let y = (height * 0.5 - world.y * scale) as i32;
        assert_eq!(
            image.get_color(x, y),
            expected,
            "mesh pixel at {world:?}, frame {}",
            frame.index
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires native OpenGL; run serially via scripts/native_smoke.sh"]
    fn native_mesh_gpu_failure_smoke() {
        use gpu::fault::RejectedUpload;

        struct Probe {
            mesh: Option<MeshId>,
        }
        impl Game for Probe {
            fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
                for reject in 1..=5 {
                    let fault = RejectedUpload::new(context.thread, reject);
                    assert!(
                        matches!(context.mesh(&quad([255; 4])), Err(Error::Asset(_))),
                        "creation accepted failed GPU buffer {reject}"
                    );
                    fault.assert_partial_upload_released();
                    assert_eq!(context.assets.meshes.live, 0);
                    assert!(context.assets.meshes.slots.is_empty());
                    assert!(context.assets.meshes.material.is_none());
                }
                self.mesh = Some(context.mesh(&quad([0, 128, 0, 255]))?);
                Ok(())
            }

            fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}

            fn draw(&mut self, frame: &mut Frame<'_, '_>) {
                let id = self.mesh.unwrap();
                let vao = frame.assets.mesh(id).unwrap().vaoId;
                for reject in 1..=5 {
                    let fault = RejectedUpload::new(frame.thread, reject);
                    assert!(
                        matches!(
                            frame.replace_mesh(id, &quad([255, 0, 0, 255])),
                            Err(Error::Asset(_))
                        ),
                        "replacement accepted failed GPU buffer {reject}"
                    );
                    fault.assert_partial_upload_released();
                    assert_eq!(frame.assets.mesh(id).unwrap().vaoId, vao);
                    assert_eq!(frame.assets.meshes.live, 1);
                    assert_eq!(frame.assets.meshes.slots.len(), 1);
                    // Positions remain live, and buffer inspection restores GL state.
                    fault.assert_query_binding_restored(frame.assets.mesh(id).unwrap());
                    drop(fault);
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |canvas| {
                        assert!(canvas.mesh(id, Transform3D::default(), Color::WHITE))
                    });
                    assert_pixel(frame, Vec3::ZERO, Color::GREEN);
                }
                frame.replace_mesh(id, &quad([0, 0, 255, 255])).unwrap();
                frame.clear(Color::BLACK);
                frame.world_3d(camera(), |canvas| {
                    assert!(canvas.mesh(id, Transform3D::default(), Color::WHITE))
                });
                assert_pixel(frame, Vec3::ZERO, Color::BLUE);
            }
        }
        let mut config = Config::new("GPU mesh allocation failure probe");
        config.window_size = (960, 540);
        config.vsync = false;
        App::new(config)
            .with_options(RunOptions {
                frames: Some(1),
                hidden: true,
                uncapped: true,
                ..RunOptions::default()
            })
            .run(Probe { mesh: None })
            .unwrap();
    }

    #[test]
    #[ignore = "requires a native display and OpenGL context; scripts/native_smoke.sh"]
    fn native_mesh_smoke() {
        struct Probe {
            mesh: Option<MeshId>,
        }
        impl Game for Probe {
            fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
                assert!(context.mesh(&MeshData::default()).is_err());
                assert_eq!(context.assets.meshes.live, 0);
                let id = context.mesh(&quad([255, 0, 0, 255]))?;
                context.replace_mesh(id, &quad([0, 128, 0, 255]))?;
                let mesh = context.assets.mesh(id).unwrap();
                assert_eq!(mesh.vertices().len(), 4);
                assert_eq!(mesh.normals()[0], Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(mesh.texcoords()[2], Vector2::new(1.0, 1.0));
                assert_eq!(mesh.indices(), &[0, 1, 2, 0, 2, 3]);
                assert_eq!(mesh.colors()[0], Color::GREEN);
                self.mesh = Some(id);
                Ok(())
            }

            fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}

            fn draw(&mut self, frame: &mut Frame<'_, '_>) {
                frame.clear(Color::BLACK);
                let id = self.mesh.unwrap();
                match frame.index {
                    0 => {
                        let transform = Transform3D {
                            position: Vec3::new(0.5, 0.0, 0.0),
                            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                            scale: Vec3::new(0.5, 0.75, 1.0),
                        };
                        frame.world_3d(camera(), |canvas| {
                            assert!(canvas.mesh(id, transform, Color::WHITE))
                        });
                        assert_pixel(frame, transform.position, Color::GREEN);
                        assert_pixel(frame, Vec3::new(1.15, 0.0, 0.0), Color::GREEN);
                        assert_pixel(frame, Vec3::new(0.5, 0.7, 0.0), Color::BLACK);
                    }
                    1 => {
                        let triangle = MeshData::new(vec![
                            Vec3::new(-1.0, -1.0, 0.0),
                            Vec3::new(1.0, -1.0, 0.0),
                            Vec3::new(0.0, 1.0, 0.0),
                        ]);
                        frame.replace_mesh(id, &triangle).unwrap();
                        assert_eq!(frame.assets.mesh(id).unwrap().vertices().len(), 3);
                        let vao = frame.assets.mesh(id).unwrap().vaoId;
                        assert!(frame.replace_mesh(id, &MeshData::default()).is_err());
                        let mut invalid = quad([255; 4]);
                        invalid.indices = Some(vec![0, 1, 4]);
                        assert!(frame.replace_mesh(id, &invalid).is_err());
                        assert_eq!(frame.assets.mesh(id).unwrap().vaoId, vao);
                        frame.world_3d(camera(), |canvas| {
                            assert!(canvas.mesh(id, Transform3D::default(), Color::BLUE))
                        });
                        assert_pixel(frame, Vec3::ZERO, Color::BLUE);
                    }
                    2 => {
                        frame.replace_mesh(id, &quad([0, 128, 0, 255])).unwrap();
                        let other = frame.mesh(&quad([255; 4])).unwrap();
                        frame.world_3d(camera(), |canvas| {
                            assert!(canvas.mesh(id, Transform3D::default(), Color::WHITE));
                            assert!(canvas.mesh_matrix(
                                other,
                                Transform3D::at(Vec3::new(2.5, 0.0, 0.0)).matrix(),
                                Color::RED
                            ));
                        });
                        assert_pixel(frame, Vec3::ZERO, Color::GREEN);
                        assert_pixel(frame, Vec3::new(2.5, 0.0, 0.0), Color::RED);
                        assert!(frame.assets.unload_mesh(other));
                        let fresh = frame.mesh(&quad([255; 4])).unwrap();
                        assert_ne!(fresh, other);
                        assert_eq!(fresh.slot, other.slot);
                        assert!(frame.assets.mesh(other).is_none());
                        assert!(!frame.assets.unload_mesh(other));
                        assert!(frame.replace_mesh(other, &quad([255; 4])).is_err());
                        frame.world_3d(camera(), |canvas| {
                            assert!(!canvas.mesh(other, Transform3D::default(), Color::WHITE))
                        });
                        assert!(frame.assets.mesh(fresh).is_some());
                        assert!(frame.assets.unload_mesh(fresh));
                        frame.raylib.set_window_size(800, 1000);
                    }
                    3 => {
                        // Replacing repeatedly does not allocate new handle slots.
                        for _ in 0..100 {
                            frame.replace_mesh(id, &quad([255; 4])).unwrap();
                        }
                        assert_eq!(frame.assets.meshes.slots.len(), 2);
                        assert!(frame.assets.unload_mesh(id));
                        assert!(frame.assets.meshes.material.is_none());
                        for _ in 0..100 {
                            let streamed = frame.mesh(&quad([255; 4])).unwrap();
                            assert!(frame.assets.mesh(id).is_none());
                            assert!(frame.assets.unload_mesh(streamed));
                        }
                        assert_eq!(frame.assets.meshes.slots.len(), 2);
                        assert_eq!(frame.assets.meshes.live, 0);
                        assert!(frame.assets.meshes.material.is_none());
                        let fresh = frame.mesh(&quad([255; 4])).unwrap();
                        assert_eq!(frame.raylib.get_screen_width(), 800);
                        assert!(frame.viewport.origin.y > 200.0);
                        frame.world_3d(camera(), |canvas| {
                            assert!(!canvas.mesh(id, Transform3D::default(), Color::WHITE));
                            assert!(canvas.mesh(fresh, Transform3D::default(), Color::RED));
                        });
                        assert_pixel(frame, Vec3::ZERO, Color::RED);
                        // Leave a live mesh for automatic teardown before GL closes.
                    }
                    _ => unreachable!(),
                }
            }
        }
        let mut config = Config::new("generated mesh native probe");
        config.window_size = (960, 540);
        config.vsync = false;
        let report = App::new(config.clone())
            .with_options(RunOptions {
                frames: Some(4),
                hidden: true,
                uncapped: true,
                ..RunOptions::default()
            })
            .run(Probe { mesh: None })
            .unwrap();
        assert_eq!(report.frames, 4);

        struct FailedInit;
        impl Game for FailedInit {
            fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
                context.mesh(&quad([255; 4]))?;
                Err(Error::Asset("intentional init failure after upload".into()))
            }
            fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
            fn draw(&mut self, _: &mut Frame<'_, '_>) {
                unreachable!();
            }
        }
        assert!(
            matches!(App::new(config).with_options(RunOptions { hidden: true, ..RunOptions::default() }).run(FailedInit), Err(Error::Asset(message)) if message == "intentional init failure after upload")
        );
    }
}
