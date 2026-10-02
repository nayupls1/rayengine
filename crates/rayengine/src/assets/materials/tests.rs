//! Image and lifetime checks against real raylib/OpenGL, serial native execution.
use super::*;
use crate::{
    prelude::*,
    raylib::prelude::{Image, RaylibTexture2D, TextureFilter},
};
use std::path::PathBuf;

fn quad() -> MeshData {
    MeshData {
        positions: vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
        ],
        texcoords: Some(vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]),
        indices: Some(vec![0, 1, 2, 0, 2, 3]),
        ..MeshData::default()
    }
}
fn camera() -> rayengine_core::camera::Camera3D {
    rayengine_core::camera::Camera3D {
        position: Vec3::new(0.0, 0.0, 6.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        vertical_fov: 60.0,
    }
}
fn pixel(frame: &Frame<'_, '_>, world_x: f32) -> Color {
    let mut image = frame.target.texture().load_image().unwrap();
    image.flip_vertical();
    let scale = image.height as f32 / (2.0 * 30_f32.to_radians().tan() * 6.0);
    image.get_color(
        (image.width as f32 * 0.5 + world_x * scale) as i32,
        image.height / 2,
    )
}

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires native display/OpenGL; scripts/native_smoke.sh runs serially"]
fn native_material_smoke() {
    let directory = Directory(
        std::env::temp_dir().join(format!("rayengine-material-smoke-{}", std::process::id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    let mut image = Image::gen_image_color(16, 8, Color::new(0, 255, 0, 0));
    image.draw_rectangle(8, 0, 8, 8, Color::new(0, 255, 0, 255));
    std::fs::write(
        directory.0.join("leaf.png"),
        &*image.export_image_to_memory(".png").unwrap(),
    )
    .unwrap();
    std::fs::write(directory.0.join("quad.obj"),"v -1 -1 0\nv 1 -1 0\nv 1 1 0\nv -1 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap();
    let custom = include_str!("default.fs")
        .replace(
            "out vec4 finalColor;",
            "uniform vec3 gain;\nout vec4 finalColor;",
        )
        .replace(
            "finalColor = color;",
            "finalColor = vec4(color.rgb * gain, color.a);",
        );
    std::fs::write(directory.0.join("gain.fs"), &custom).unwrap();
    struct Probe {
        path: PathBuf,
        mesh: Option<MeshId>,
        model: Option<ModelId>,
        texture: Option<TextureId>,
        shader: Option<ShaderId>,
        uniform: Option<UniformId>,
        opaque: Option<MaterialId>,
        cutout_zero: Option<MaterialId>,
        cutout: Option<MaterialId>,
        blend: Option<MaterialId>,
        red: Option<MaterialId>,
        green: Option<MaterialId>,
        blue: Option<MaterialId>,
        default: Option<MaterialId>,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.mesh = Some(ctx.mesh(&quad())?);
            self.model = Some(ctx.model(self.path.join("quad.obj"))?);
            let texture = ctx.texture(self.path.join("leaf.png"))?;
            self.texture = Some(texture);
            ctx.assets
                .texture(texture)
                .unwrap()
                .set_texture_filter(ctx.thread, TextureFilter::TEXTURE_FILTER_POINT);
            assert!(ctx.shader_from_source(None, "not GLSL").is_err());
            assert!(ctx.shader_from_source(None, "\0").is_err());
            let shader = ctx.shader(None, self.path.join("gain.fs"))?;
            assert_eq!(shader, ctx.shader(None, self.path.join("gain.fs"))?);
            self.shader = Some(shader);
            assert!(
                ctx.uniform(shader, "missing", UniformValue::Float(1.0))
                    .is_err()
            );
            assert!(
                ctx.uniform(shader, "gain", UniformValue::Float(1.0))
                    .is_err()
            );
            assert!(
                ctx.uniform(
                    shader,
                    "mvp",
                    UniformValue::Mat4(rayengine_core::glam::Mat4::IDENTITY)
                )
                .is_err()
            );
            let gain = ctx.uniform(shader, "gain", UniformValue::Vec3(Vec3::Z))?;
            assert_eq!(
                gain,
                ctx.uniform(shader, "gain", UniformValue::Vec3(Vec3::Z))?
            );
            self.uniform = Some(gain);
            assert!(
                ctx.material(MaterialDesc {
                    alpha: AlphaMode::Cutout(f32::NAN),
                    ..MaterialDesc::default()
                })
                .is_err()
            );
            self.opaque = Some(ctx.material(MaterialDesc {
                texture: Some(texture),
                ..MaterialDesc::default()
            })?);
            self.cutout = Some(ctx.material(MaterialDesc {
                texture: Some(texture),
                alpha: AlphaMode::Cutout(0.5),
                ..MaterialDesc::default()
            })?);
            self.cutout_zero = Some(ctx.material(MaterialDesc {
                texture: Some(texture),
                alpha: AlphaMode::Cutout(0.0),
                ..MaterialDesc::default()
            })?);
            self.blend = Some(ctx.material(MaterialDesc {
                tint: Color::new(255, 0, 0, 128),
                alpha: AlphaMode::Blend,
                ..MaterialDesc::default()
            })?);
            self.red = Some(ctx.material(MaterialDesc {
                shader: Some(shader),
                parameters: vec![
                    MaterialParam {
                        uniform: gain,
                        value: UniformValue::Vec3(Vec3::Y),
                    },
                    MaterialParam {
                        uniform: gain,
                        value: UniformValue::Vec3(Vec3::X),
                    },
                ],
                ..MaterialDesc::default()
            })?);
            self.green = Some(ctx.material(MaterialDesc {
                shader: Some(shader),
                parameters: vec![MaterialParam {
                    uniform: gain,
                    value: UniformValue::Vec3(Vec3::Y),
                }],
                ..MaterialDesc::default()
            })?);
            self.blue = Some(ctx.material(MaterialDesc {
                shader: Some(shader),
                ..MaterialDesc::default()
            })?);
            self.default = Some(ctx.material(MaterialDesc::default())?);
            let no_cutout = ctx.shader_from_source(None,"#version 330\nin vec4 fragColor; out vec4 finalColor; void main(){finalColor=fragColor;}")?;
            assert!(
                ctx.material(MaterialDesc {
                    shader: Some(no_cutout),
                    alpha: AlphaMode::Cutout(0.5),
                    ..MaterialDesc::default()
                })
                .is_err()
            );
            assert!(
                ctx.material(MaterialDesc {
                    shader: Some(no_cutout),
                    parameters: vec![MaterialParam {
                        uniform: gain,
                        value: UniformValue::Vec3(Vec3::ONE)
                    }],
                    ..MaterialDesc::default()
                })
                .is_err()
            );
            assert!(ctx.assets.unload_shader(no_cutout));
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            let mesh = self.mesh.unwrap();
            let default = self.default.unwrap();
            let before = gpu::snapshot(frame.thread);
            frame.clear(Color::BLACK);
            match frame.index {
                0 => {
                    // Opaque ignores texture alpha, cutout leaves holes and depth untouched.
                    frame.world_3d(camera(), |canvas| {
                        assert!(canvas.mesh_material(
                            mesh,
                            self.opaque.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ))
                    });
                    assert_eq!(pixel(frame, -0.5), Color::new(0, 255, 0, 255));
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |canvas| {
                        assert!(canvas.mesh_material(
                            mesh,
                            self.cutout_zero.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ));
                    });
                    assert_eq!(pixel(frame, -0.5), Color::new(0, 255, 0, 255));
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |canvas| {
                        // Same alpha mode, different cutoff: cached uniforms must update.
                        assert!(canvas.mesh_material(
                            mesh,
                            self.cutout.unwrap(),
                            Transform3D::at(Vec3::new(0.0, 0.0, 0.5)),
                            Color::WHITE
                        ));
                        assert!(canvas.mesh_material(
                            mesh,
                            default,
                            Transform3D::default(),
                            Color::BLUE
                        ));
                    });
                    assert_eq!(pixel(frame, -0.5), Color::BLUE);
                    assert_eq!(pixel(frame, 0.5), Color::new(0, 255, 0, 255));
                    // Imported model uses the same override path without editing its original material.
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |canvas| {
                        assert!(canvas.model_material(
                            self.model.unwrap(),
                            self.opaque.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ))
                    });
                    assert_eq!(pixel(frame, 0.5), Color::new(0, 255, 0, 255));
                }
                1 => {
                    frame.world_3d(camera(), |canvas| {
                        assert!(canvas.mesh_material(
                            mesh,
                            default,
                            Transform3D::default(),
                            Color::BLUE
                        ));
                        assert!(canvas.mesh_material(
                            mesh,
                            self.blend.unwrap(),
                            Transform3D::at(Vec3::new(0.0, 0.0, 0.5)),
                            Color::WHITE
                        ));
                    });
                    let color = pixel(frame, 0.0);
                    assert!(
                        color.r.abs_diff(128) < 2 && color.b.abs_diff(127) < 2 && color.g == 0,
                        "alpha blend {color:?}"
                    );
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |canvas| {
                        // Batched primitives must be submitted before a material
                        // changes depth/blend state and draws an immediate mesh.
                        canvas.cube(
                            Aabb3::from_center(Vec3::ZERO, Vec3::new(2.0, 2.0, 0.1)),
                            Color::BLUE,
                        );
                        assert!(canvas.mesh_material(
                            mesh,
                            self.blend.unwrap(),
                            Transform3D::at(Vec3::new(0.0, 0.0, 0.5)),
                            Color::WHITE
                        ));
                    });
                    let color = pixel(frame, 0.0);
                    assert!(
                        color.r.abs_diff(128) < 2 && color.b.abs_diff(127) < 2 && color.g == 0,
                        "blend after batched primitive {color:?}"
                    );
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |canvas| {
                        // A later far opaque surface remains visible: blend never writes depth.
                        assert!(canvas.mesh_material(
                            mesh,
                            self.blend.unwrap(),
                            Transform3D::at(Vec3::new(0.0, 0.0, 0.5)),
                            Color::WHITE
                        ));
                        assert!(canvas.mesh_material(
                            mesh,
                            default,
                            Transform3D::default(),
                            Color::BLUE
                        ));
                    });
                    assert_eq!(pixel(frame, 0.0), Color::BLUE);
                }
                2 => {
                    for (id, expected) in [
                        (self.red.unwrap(), Color::RED),
                        (self.green.unwrap(), Color::new(0, 255, 0, 255)),
                        (self.blue.unwrap(), Color::BLUE),
                        (self.red.unwrap(), Color::RED),
                    ] {
                        frame.clear(Color::BLACK);
                        frame.world_3d(camera(), |canvas| {
                            assert!(canvas.mesh_material_matrix(
                                mesh,
                                id,
                                Transform3D::default().matrix(),
                                Color::WHITE
                            ))
                        });
                        assert_eq!(pixel(frame, 0.0), expected);
                    }
                    assert!(
                        frame
                            .assets
                            .set_uniform(self.uniform.unwrap(), UniformValue::Float(1.0))
                            .is_err()
                    );
                    assert!(
                        frame
                            .assets
                            .set_uniform(
                                self.uniform.unwrap(),
                                UniformValue::Vec3(Vec3::splat(f32::NAN))
                            )
                            .is_err()
                    );
                    frame
                        .assets
                        .set_uniform(self.uniform.unwrap(), UniformValue::Vec3(Vec3::Y))
                        .unwrap();
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |canvas| {
                        assert!(canvas.mesh_material(
                            mesh,
                            self.blue.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ))
                    });
                    assert_eq!(pixel(frame, 0.0), Color::new(0, 255, 0, 255));
                    assert!(
                        frame
                            .assets
                            .replace_material(
                                self.red.unwrap(),
                                MaterialDesc {
                                    alpha: AlphaMode::Cutout(-1.0),
                                    ..MaterialDesc::default()
                                }
                            )
                            .is_err()
                    );
                    frame
                        .assets
                        .replace_material(
                            self.red.unwrap(),
                            MaterialDesc {
                                tint: Color::BLUE,
                                ..MaterialDesc::default()
                            },
                        )
                        .unwrap();
                    frame.clear(Color::BLACK);
                    frame.world_3d(camera(), |canvas| {
                        assert!(canvas.mesh_material(
                            mesh,
                            self.red.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ))
                    });
                    assert_eq!(pixel(frame, 0.0), Color::BLUE);
                }
                3 => {
                    // Unloading a material must not unload its shared shader or texture.
                    assert!(frame.assets.unload_material(self.opaque.unwrap()));
                    assert!(!frame.assets.unload_material(self.opaque.unwrap()));
                    assert!(
                        frame
                            .assets
                            .texture(self.texture.unwrap())
                            .unwrap()
                            .is_texture_valid()
                    );
                    frame
                        .assets
                        .set_uniform(self.uniform.unwrap(), UniformValue::Vec3(Vec3::Z))
                        .unwrap();
                    frame.world_3d(camera(), |canvas| {
                        assert!(canvas.mesh_material(
                            mesh,
                            self.cutout.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ))
                    });
                    assert_eq!(pixel(frame, 0.5), Color::new(0, 255, 0, 255));
                    frame.assets.unload_texture(self.texture.unwrap());
                    frame.world_3d(camera(), |canvas| {
                        assert!(!canvas.mesh_material(
                            mesh,
                            self.cutout.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ))
                    });
                    let fresh = frame
                        .assets
                        .load_texture(frame.raylib, frame.thread, &self.path.join("leaf.png"))
                        .unwrap();
                    assert_ne!(fresh, self.texture.unwrap());
                    frame.world_3d(camera(), |canvas| {
                        assert!(!canvas.mesh_material(
                            mesh,
                            self.cutout.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ))
                    });
                    assert!(frame.assets.unload_shader(self.shader.unwrap()));
                    assert!(!frame.assets.unload_shader(self.shader.unwrap()));
                    assert!(
                        frame
                            .assets
                            .set_uniform(self.uniform.unwrap(), UniformValue::Vec3(Vec3::ONE))
                            .is_err()
                    );
                    frame.world_3d(camera(), |canvas| {
                        assert!(!canvas.mesh_material(
                            mesh,
                            self.green.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ))
                    });
                    let fresh_shader = frame
                        .shader_from_source(None, include_str!("default.fs"))
                        .unwrap();
                    assert_ne!(Some(fresh_shader), self.shader);
                    let fresh_material = frame.material(MaterialDesc::default()).unwrap();
                    assert_ne!(Some(fresh_material), self.opaque);
                    frame.world_3d(camera(), |canvas| {
                        assert!(!canvas.mesh_material(
                            mesh,
                            self.opaque.unwrap(),
                            Transform3D::default(),
                            Color::WHITE
                        ));
                        assert!(canvas.mesh_material(
                            mesh,
                            fresh_material,
                            Transform3D::default(),
                            Color::RED
                        ));
                        // Legacy drawing after a material also restores blend/depth state.
                        assert!(canvas.mesh(
                            mesh,
                            Transform3D::at(Vec3::new(0.0, 0.0, 0.5)),
                            Color::BLUE
                        ));
                    });
                    assert_eq!(pixel(frame, 0.0), Color::BLUE);
                }
                _ => unreachable!(),
            }
            assert_eq!(
                gpu::snapshot(frame.thread),
                before,
                "material pass changed caller render state"
            );
            frame.ui(|ui| {
                ui.rectangle(
                    Aabb2::from_center(Vec2::splat(30.0), Vec2::splat(20.0)),
                    Color::RED,
                )
            });
        }
    }
    let mut config = Config::new("material shader native probe");
    config.window_size = (960, 540);
    config.vsync = false;
    App::new(config)
        .with_options(RunOptions {
            frames: Some(4),
            hidden: true,
            uncapped: true,
            ..RunOptions::default()
        })
        .run(Probe {
            path: directory.0.clone(),
            mesh: None,
            model: None,
            texture: None,
            shader: None,
            uniform: None,
            opaque: None,
            cutout_zero: None,
            cutout: None,
            blend: None,
            red: None,
            green: None,
            blue: None,
            default: None,
        })
        .unwrap();
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_material_2d_scoped_alpha_without_materials_and_unwind() {
    struct Probe;
    impl Game for Probe {
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            frame.clear(Color::new(0, 0, 255, 255));
            let thread = frame.thread;
            let before = gpu::snapshot(thread);
            frame.world_2d(
                rayengine_core::camera::Camera2D {
                    view_height: 8.0,
                    ..Default::default()
                },
                |canvas| {
                    canvas
                        .with_alpha_blend(|canvas| {
                            canvas.rectangle(
                                Aabb2::from_center(Vec2::ZERO, Vec2::splat(2.0)),
                                Color::new(255, 0, 0, 128),
                            );
                        })
                        .unwrap();
                    assert_eq!(gpu::snapshot(thread), before);
                    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        canvas
                            .with_alpha_blend(|canvas| {
                                canvas.rectangle(
                                    Aabb2::from_center(Vec2::new(3.0, 0.0), Vec2::ONE),
                                    Color::new(255, 0, 0, 128),
                                );
                                panic!("intentional alpha-scope unwind");
                            })
                            .unwrap();
                    }));
                    assert!(unwound.is_err());
                    assert_eq!(gpu::snapshot(thread), before);
                },
            );
            assert_eq!(gpu::snapshot(thread), before);
            let image = frame.target.texture().load_image().unwrap();
            let center = image.get_color(image.width / 2, image.height / 2);
            assert_eq!(center, Color::new(128, 0, 127, 255));
            assert_eq!(frame.assets.resource_counts().materials, 0);
            assert_eq!(frame.assets.resource_counts().shaders, 0);
        }
    }
    let mut config = Config::new("Scoped alpha probe");
    config.audio = false;
    config.vsync = false;
    App::new(config)
        .with_options(RunOptions {
            hidden: true,
            frames: Some(1),
            uncapped: true,
            ..Default::default()
        })
        .run(Probe)
        .unwrap();
}
