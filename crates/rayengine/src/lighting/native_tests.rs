//! Real GPU pixel checks for generated/imported surfaces and light updates.
use crate::{
    prelude::*,
    raylib::prelude::{Image, RaylibRenderTexture2D, RaylibTexture2D},
};
use std::path::PathBuf;

fn quad(normal: Option<Vec3>) -> MeshData {
    MeshData {
        positions: vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
        ],
        normals: normal.map(|n| vec![n; 4]),
        indices: Some(vec![0, 1, 2, 0, 2, 3]),
        ..MeshData::default()
    }
}
fn camera() -> Camera3D {
    Camera3D {
        position: Vec3::new(0.0, 0.0, 6.0),
        target: Vec3::ZERO,
        ..Camera3D::default()
    }
}
fn center(frame: &Frame<'_, '_>) -> Color {
    let image = frame.target.texture().load_image().unwrap();
    image.get_color(image.width / 2, image.height / 2)
}
fn near(actual: Color, expected: Color) {
    for (a, b) in [
        (actual.r, expected.r),
        (actual.g, expected.g),
        (actual.b, expected.b),
    ] {
        assert!(a.abs_diff(b) <= 3, "expected {expected:?}, got {actual:?}");
    }
}
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires native display/OpenGL; scripts/native_smoke.sh runs serially"]
fn native_lighting_smoke() {
    let directory =
        Directory(std::env::temp_dir().join(format!("rayengine-lighting-{}", std::process::id())));
    std::fs::create_dir(&directory.0).unwrap();
    for (name, normal) in [("quad", "0 0 1"), ("slant", "1 0 1"), ("invalid", "0 0 0")] {
        std::fs::write(directory.0.join(format!("{name}.obj")), format!("v -1 -1 0\nv 1 -1 0\nv 1 1 0\nv -1 1 0\nvn {normal}\nf 1//1 2//1 3//1\nf 1//1 3//1 4//1\n")).unwrap();
    }
    for (name, color) in [
        ("albedo", Color::new(200, 100, 50, 255)),
        ("alpha", Color::new(200, 100, 50, 128)),
    ] {
        let image = Image::gen_image_color(2, 2, color);
        std::fs::write(
            directory.0.join(format!("{name}.png")),
            &*image.export_image_to_memory(".png").unwrap(),
        )
        .unwrap();
    }
    struct Probe {
        path: PathBuf,
        mesh: Option<MeshId>,
        model: Option<ModelId>,
        slant: Option<ModelId>,
        invalid_model: Option<ModelId>,
        material: Option<MaterialId>,
        texture: Option<TextureId>,
        alpha: Option<TextureId>,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.mesh = Some(ctx.mesh(&quad(Some(Vec3::Z)))?);
            self.model = Some(ctx.model(self.path.join("quad.obj"))?);
            self.slant = Some(ctx.model(self.path.join("slant.obj"))?);
            self.invalid_model = Some(ctx.model(self.path.join("invalid.obj"))?);
            self.texture = Some(ctx.texture(self.path.join("albedo.png"))?);
            self.alpha = Some(ctx.texture(self.path.join("alpha.png"))?);
            self.material = Some(ctx.material(MaterialDesc {
                shading: Shading::Lit,
                ..MaterialDesc::default()
            })?);
            let previous = ctx.assets.lighting().clone();
            assert!(
                ctx.assets
                    .set_lighting(Lighting {
                        ambient: Vec3::splat(f32::NAN),
                        ..Lighting::default()
                    })
                    .is_err()
            );
            assert_eq!(ctx.assets.lighting(), &previous);
            let shader =
                ctx.shader_from_source(None, include_str!("../assets/materials/default.fs"))?;
            assert!(
                ctx.material(MaterialDesc {
                    shading: Shading::Lit,
                    shader: Some(shader),
                    ..MaterialDesc::default()
                })
                .is_err()
            );
            assert!(ctx.assets.unload_shader(shader));
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            let mesh = self.mesh.unwrap();
            let model = self.model.unwrap();
            let material = self.material.unwrap();
            // Both geometry paths see identical world-space light direction and live updates.
            for (lights, expected) in [
                (
                    Lighting {
                        ambient: Vec3::splat(0.25),
                        ..Lighting::default()
                    },
                    Color::new(64, 64, 64, 255),
                ),
                (
                    Lighting {
                        ambient: Vec3::ZERO,
                        directional: Some(DirectionalLight {
                            direction: -Vec3::Z * 2.0,
                            color: Vec3::new(0.5, 0.25, 0.0),
                        }),
                        points: vec![],
                    },
                    Color::new(128, 64, 0, 255),
                ),
                (
                    Lighting {
                        ambient: Vec3::ZERO,
                        directional: Some(DirectionalLight {
                            direction: Vec3::Z,
                            color: Vec3::ONE,
                        }),
                        points: vec![],
                    },
                    Color::BLACK,
                ),
                (
                    Lighting {
                        ambient: Vec3::ZERO,
                        directional: None,
                        points: vec![PointLight {
                            position: Vec3::Z * 2.0,
                            color: Vec3::ONE,
                            range: 4.0,
                        }],
                    },
                    Color::new(64, 64, 64, 255),
                ),
                (
                    Lighting {
                        ambient: Vec3::ZERO,
                        directional: None,
                        points: vec![PointLight {
                            position: Vec3::Z * 4.0,
                            color: Vec3::ONE,
                            range: 4.0,
                        }],
                    },
                    Color::BLACK,
                ),
                (
                    Lighting {
                        ambient: Vec3::ZERO,
                        directional: None,
                        points: vec![
                            PointLight {
                                position: Vec3::Z * 2.0,
                                color: Vec3::splat(0.25),
                                range: 4.0
                            };
                            MAX_POINT_LIGHTS
                        ],
                    },
                    Color::new(64, 64, 64, 255),
                ),
                (
                    Lighting {
                        ambient: Vec3::ZERO,
                        directional: None,
                        points: vec![],
                    },
                    Color::BLACK,
                ),
            ] {
                frame.assets.set_lighting(lights).unwrap();
                for imported in [false, true] {
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |c| {
                        assert!(if imported {
                            c.try_model_material(
                                model,
                                material,
                                Transform3D::default(),
                                Color::WHITE,
                            )
                            .unwrap()
                        } else {
                            c.try_mesh_material(
                                mesh,
                                material,
                                Transform3D::default(),
                                Color::WHITE,
                            )
                            .unwrap()
                        });
                    });
                    near(center(frame), expected);
                }
            }
            // Inverse-transpose normals under rotation + nonuniform scale, both paths.
            frame
                .replace_mesh(mesh, &quad(Some(Vec3::new(1.0, 0.0, 1.0))))
                .unwrap();
            let transform = Transform3D {
                rotation: Quat::from_rotation_y(0.5),
                scale: Vec3::new(2.0, 0.7, 0.5),
                ..Transform3D::default()
            };
            let normal = transform
                .matrix()
                .inverse()
                .transpose()
                .transform_vector3(Vec3::new(1.0, 0.0, 1.0))
                .normalize();
            let brightness = (normal.z * 255.0).round() as u8;
            frame
                .assets
                .set_lighting(Lighting {
                    ambient: Vec3::ZERO,
                    directional: Some(DirectionalLight {
                        direction: -Vec3::Z,
                        color: Vec3::ONE,
                    }),
                    points: vec![],
                })
                .unwrap();
            for imported in [false, true] {
                frame.clear(Color::BLACK);
                frame.world_3d(camera(), |c| {
                    assert!(if imported {
                        c.try_model_material(self.slant.unwrap(), material, transform, Color::WHITE)
                            .unwrap()
                    } else {
                        c.try_mesh_material(mesh, material, transform, Color::WHITE)
                            .unwrap()
                    });
                });
                near(
                    center(frame),
                    Color::new(brightness, brightness, brightness, 255),
                );
            }
            // Albedo, material tint, draw tint, cutout and blending on both paths.
            frame.replace_mesh(mesh, &quad(Some(Vec3::Z))).unwrap();
            frame
                .assets
                .set_lighting(Lighting {
                    ambient: Vec3::splat(0.5),
                    ..Lighting::default()
                })
                .unwrap();
            for (alpha, texture, expected) in [
                (AlphaMode::Opaque, self.texture, Color::new(50, 25, 25, 255)),
                (AlphaMode::Cutout(0.6), self.alpha, Color::BLUE),
                (
                    AlphaMode::Cutout(0.4),
                    self.alpha,
                    Color::new(50, 25, 25, 255),
                ),
                (AlphaMode::Blend, self.alpha, Color::new(25, 13, 140, 255)),
            ] {
                frame
                    .assets
                    .replace_material(
                        material,
                        MaterialDesc {
                            shading: Shading::Lit,
                            texture,
                            tint: Color::new(128, 255, 255, 255),
                            alpha,
                            ..MaterialDesc::default()
                        },
                    )
                    .unwrap();
                for imported in [false, true] {
                    frame.clear(Color::BLUE);
                    frame.world_3d(camera(), |c| {
                        assert!(if imported {
                            c.model_material(
                                model,
                                material,
                                Transform3D::default(),
                                Color::new(255, 128, 255, 255),
                            )
                        } else {
                            c.mesh_material(
                                mesh,
                                material,
                                Transform3D::default(),
                                Color::new(255, 128, 255, 255),
                            )
                        });
                    });
                    near(center(frame), expected);
                }
            }
            frame
                .assets
                .replace_material(
                    material,
                    MaterialDesc {
                        shading: Shading::Lit,
                        ..MaterialDesc::default()
                    },
                )
                .unwrap();
            for normal in [None, Some(Vec3::ZERO)] {
                frame.replace_mesh(mesh, &quad(normal)).unwrap();
                frame.world_3d(camera(), |c| {
                    assert!(
                        c.try_mesh_material(mesh, material, Transform3D::default(), Color::WHITE)
                            .unwrap_err()
                            .to_string()
                            .contains("normal")
                    );
                    assert!(!c.mesh_material(mesh, material, Transform3D::default(), Color::WHITE));
                    assert!(
                        c.try_model_material(
                            self.invalid_model.unwrap(),
                            material,
                            Transform3D::default(),
                            Color::WHITE
                        )
                        .is_err()
                    );
                });
            }
            frame.replace_mesh(mesh, &quad(Some(Vec3::Z))).unwrap();
            frame.world_3d(camera(), |c| {
                let transform = Transform3D {
                    scale: Vec3::ZERO,
                    ..Transform3D::default()
                };
                assert!(
                    c.try_mesh_material(mesh, material, transform, Color::WHITE)
                        .is_err()
                );
                assert!(
                    c.try_model_material(model, material, transform, Color::WHITE)
                        .is_err()
                );
            });
            // Switching to unlit preserves normal-optional geometry and ignores lights.
            frame.replace_mesh(mesh, &quad(None)).unwrap();
            frame
                .assets
                .replace_material(material, MaterialDesc::default())
                .unwrap();
            frame.clear(Color::BLACK);
            frame.world_3d(camera(), |c| {
                assert!(c.mesh_material(mesh, material, Transform3D::default(), Color::WHITE));
            });
            near(center(frame), Color::WHITE);
            // Description teardown never owns textures; unloading dependencies invalidates draws.
            frame
                .assets
                .replace_material(
                    material,
                    MaterialDesc {
                        texture: self.texture,
                        shading: Shading::Lit,
                        ..MaterialDesc::default()
                    },
                )
                .unwrap();
            frame.replace_mesh(mesh, &quad(Some(Vec3::Z))).unwrap();
            frame.assets.unload_texture(self.texture.unwrap());
            frame.world_3d(camera(), |c| {
                assert!(!c.mesh_material(mesh, material, Transform3D::default(), Color::WHITE));
            });
            assert!(frame.assets.unload_material(material));
            assert!(frame.assets.texture(self.alpha.unwrap()).is_some());
        }
    }
    let mut config = Config::new("basic lighting probe");
    config.vsync = false;
    App::new(config)
        .with_options(RunOptions {
            frames: Some(1),
            hidden: true,
            uncapped: true,
            ..RunOptions::default()
        })
        .run(Probe {
            path: directory.0.clone(),
            mesh: None,
            model: None,
            slant: None,
            invalid_model: None,
            material: None,
            texture: None,
            alpha: None,
        })
        .unwrap();
}
