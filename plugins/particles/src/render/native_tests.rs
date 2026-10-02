use super::*;
use crate::EmitterConfig;
use rayengine::{diagnostics::DiagnosticsConfig, raylib::prelude::Image};
use std::path::PathBuf;

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Probe {
    effect: ParticleEffect,
    view: ParticleView,
    textured: bool,
    case: &'static str,
}
impl Game for Probe {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let original = ctx.assets.resource_counts();
        let mut image = Image::gen_image_color(8, 4, Color::MAGENTA);
        image.draw_rectangle(4, 0, 4, 4, Color::WHITE);
        let texture = ctx.texture_from_image(&image)?;
        if self.textured {
            self.effect.texture = Some(ParticleSprite {
                texture,
                region: Some(SpriteRegion::new(4, 0, 4, 4).unwrap()),
            });
            let mut invalid = ParticleEffect::new(
                Emitter::new(EmitterConfig::default()).unwrap(),
                Some(ParticleSprite {
                    texture,
                    region: Some(SpriteRegion::new(7, 0, 4, 4).unwrap()),
                }),
            )?;
            let before = ctx.assets.resource_counts();
            assert!(invalid.init(&mut self.view, ctx).is_err());
            assert_eq!(ctx.assets.resource_counts(), before);
        }
        self.effect.init(&mut self.view, ctx)?;
        let first_mesh = self.effect.mesh.unwrap();
        let first_material = self.effect.material.unwrap();
        assert!(self.effect.init(&mut self.view, ctx).is_err());
        self.effect.unload(&mut self.view, ctx.assets);
        self.effect.unload(&mut self.view, ctx.assets);
        assert!(ctx.assets.mesh(first_mesh).is_none());
        assert!(ctx.assets.material(first_material).is_none());
        assert!(ctx.assets.texture(texture).is_some());
        assert_eq!(ctx.assets.resource_counts().meshes, original.meshes);
        assert_eq!(ctx.assets.resource_counts().materials, original.materials);
        self.effect.init(&mut self.view, ctx)?;
        assert_ne!(self.effect.mesh.unwrap(), first_mesh);
        // Near older particle is yellow; far younger particle is red.
        self.effect.emitter.set_position(Vec3::Z).unwrap();
        self.effect.emitter.burst(1);
        self.effect.emitter.step(5.0).unwrap();
        self.effect.emitter.set_position(Vec3::ZERO).unwrap();
        if self.case == "sorted" {
            self.effect.emitter.burst(1);
        }
        self.effect.emitter.stop();
        if self.case == "stale" {
            ctx.assets.unload_texture(texture);
            self.effect.unload(&mut self.view, ctx.assets);
            assert!(self.effect.init(&mut self.view, ctx).is_err());
            assert_eq!(ctx.assets.resource_counts().meshes, 0);
            assert_eq!(ctx.assets.resource_counts().materials, 0);
            let replacement =
                ctx.texture_from_image(&Image::gen_image_color(4, 4, Color::WHITE))?;
            self.effect.texture = Some(replacement.into());
            self.effect.init(&mut self.view, ctx)?;
            self.effect.emitter.burst(1);
            ctx.assets.unload_texture(replacement);
        }
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {
        self.effect.emitter.step(0.0).unwrap();
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::BLUE);
        match self.view {
            ParticleView::TwoD(camera) => frame.world_2d(camera, |canvas| {
                assert_eq!(
                    self.effect.draw_2d(canvas, 1.0),
                    usize::from(self.case != "stale")
                );
            }),
            ParticleView::ThreeD(camera) => frame.world_3d(camera, |canvas| {
                if self.case == "occluded" {
                    canvas.cube(
                        Aabb3::from_center(Vec3::new(0.0, 0.0, 2.0), Vec3::splat(3.0)),
                        Color::WHITE,
                    );
                }
                assert_eq!(
                    self.effect.draw_3d(canvas, camera, 1.0),
                    if self.case == "stale" {
                        0
                    } else if self.case == "sorted" {
                        2
                    } else {
                        1
                    }
                );
                if self.case == "sorted" {
                    assert_eq!(self.effect.order, [1, 0]);
                }
                if self.case == "no_depth_write" {
                    canvas.cube(
                        Aabb3::from_center(Vec3::new(0.0, 0.0, -1.0), Vec3::splat(3.0)),
                        Color::WHITE,
                    );
                }
            }),
        }
    }
}
#[test]
fn quad_and_camera_basis_are_valid() {
    assert_eq!(quad().validate().unwrap().triangle_count, 2);
    let camera = Camera3D {
        position: Vec3::Z,
        target: Vec3::ZERO,
        ..Default::default()
    };
    let (right, up, forward) = camera_basis(camera).unwrap();
    assert_eq!((right, up, forward), (Vec3::X, Vec3::Y, Vec3::NEG_Z));
    assert!(
        camera_basis(Camera3D {
            target: Vec3::Z,
            ..camera
        })
        .is_none()
    );
    assert!(
        camera_basis(Camera3D {
            up: Vec3::Z,
            ..camera
        })
        .is_none()
    );
}
#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_particles_pixels_depth_viewports_and_lifecycle() {
    let directory =
        Directory(std::env::temp_dir().join(format!("rayengine-particles-{}", std::process::id())));
    std::fs::create_dir_all(&directory.0).unwrap();
    for size in [(960, 540), (600, 900)] {
        for (dimension, textured, case) in [
            (2, false, "single"),
            (2, true, "single"),
            (2, true, "stale"),
            (3, false, "single"),
            (3, true, "single"),
            (3, true, "stale"),
            (3, false, "sorted"),
            (3, true, "occluded"),
            (3, false, "no_depth_write"),
        ] {
            let view = if dimension == 2 {
                ParticleView::TwoD(Camera2D {
                    view_height: 8.0,
                    ..Default::default()
                })
            } else {
                ParticleView::ThreeD(Camera3D {
                    position: Vec3::new(0.0, 0.0, 10.0),
                    target: Vec3::ZERO,
                    ..Default::default()
                })
            };
            let emitter = Emitter::new(EmitterConfig {
                lifetime: [10.0, 10.0],
                start_size: 4.0,
                end_size: 4.0,
                start_color: Vec4::new(1.0, 0.0, 0.0, 0.5),
                end_color: Vec4::new(0.0, 1.0, 0.0, 0.5),
                ..Default::default()
            })
            .unwrap();
            let screenshot = directory
                .0
                .join(format!("{dimension}-{textured}-{case}-{}.png", size.0));
            let mut config = Config::new("Particle rendering probe");
            config.audio = false;
            config.vsync = false;
            config.window_size = size;
            App::new(config)
                .with_options(RunOptions {
                    frames: Some(2),
                    hidden: true,
                    screenshot: Some(screenshot.clone()),
                    diagnostics: Some(DiagnosticsConfig::new("particles/native.v1")),
                    ..Default::default()
                })
                .run(Probe {
                    effect: ParticleEffect::new(emitter, None).unwrap(),
                    view,
                    textured,
                    case,
                })
                .unwrap();
            let image = Image::load_image(screenshot.to_str().unwrap()).unwrap();
            let pixel = image.get_color(image.width / 2, image.height / 2);
            let expected: [u8; 3] = match case {
                "stale" => [0, 0, 255],
                "occluded" | "no_depth_write" => [255, 255, 255],
                "sorted" => [128, 64, 63],
                _ => [64, 64, 127],
            };
            for (actual, expected) in [pixel.r, pixel.g, pixel.b].into_iter().zip(expected) {
                assert!(
                    (i16::from(actual) - i16::from(expected)).abs() <= 4,
                    "{dimension}D textured={textured} {case} size={size:?}: {pixel:?}, expected {expected}"
                );
            }
        }
    }
}
