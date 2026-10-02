use super::*;
use rayengine_core::{
    camera::Camera2D as EngineCamera2D,
    glam::Mat2,
    sprite::{SpriteRegion, SpriteTransform},
};

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_sprite_regions_transforms_and_viewports() {
    struct Probe {
        texture: Option<TextureId>,
        stale: Option<TextureId>,
        dpi: f32,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            // An asymmetric central region surrounded by magenta. Each quadrant
            // identifies orientation independently, and magenta detects bleed.
            let mut image = Image::gen_image_color(16, 16, Color::MAGENTA);
            for (x, y, color) in [
                (4, 4, Color::RED),
                (8, 4, Color::GREEN),
                (4, 8, Color::BLUE),
                (8, 8, Color::WHITE),
            ] {
                image.draw_rectangle(x, y, 4, 4, color);
            }
            let stale = ctx.texture_from_image(&image)?;
            ctx.assets.unload_texture(stale);
            self.stale = Some(stale);
            self.texture = Some(ctx.texture_from_image(&image)?);
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            // Simulate a 2x physical-DPI target through the viewport's actual
            // render-size contract without depending on desktop DPI settings.
            // IntegerFit intentionally keeps its reference-resolution target.
            if self.dpi > 1.0 {
                let (w, h) = frame.viewport.render_size(Vec2::splat(self.dpi));
                *frame.target = frame
                    .raylib
                    .load_render_texture(frame.thread, w, h)
                    .unwrap();
            }
            frame.set_draw_counters_enabled(true);
            frame.clear(Color::BLACK);
            let camera = EngineCamera2D {
                target: Vec2::new(7.0, -5.0),
                rotation: 0.2,
                view_height: 180.0,
            };
            let region = SpriteRegion::new(4, 4, 8, 8).unwrap();
            let cases = [
                (
                    Vec2::new(-90.0, -40.0),
                    false,
                    false,
                    0.0,
                    Color::WHITE,
                    region,
                ),
                (
                    Vec2::new(-30.0, -40.0),
                    true,
                    false,
                    0.0,
                    Color::WHITE,
                    region,
                ),
                (
                    Vec2::new(30.0, -40.0),
                    false,
                    true,
                    0.0,
                    Color::WHITE,
                    region,
                ),
                (
                    Vec2::new(90.0, -40.0),
                    true,
                    true,
                    0.0,
                    Color::WHITE,
                    region,
                ),
                (
                    Vec2::new(-90.0, 35.0),
                    false,
                    false,
                    std::f32::consts::FRAC_PI_2,
                    Color::WHITE,
                    region,
                ),
                (
                    Vec2::new(-30.0, 35.0),
                    true,
                    true,
                    -0.6,
                    Color::new(128, 200, 64, 255),
                    region,
                ),
                (
                    Vec2::new(30.0, 35.0),
                    false,
                    false,
                    0.3,
                    Color::new(255, 255, 255, 128),
                    SpriteRegion::new(8, 8, 4, 4).unwrap(),
                ),
                (
                    Vec2::new(90.0, 35.0),
                    false,
                    false,
                    0.0,
                    Color::WHITE,
                    SpriteRegion::new(12, 12, 4, 4).unwrap(),
                ),
            ];
            let transform = |position, flip_x, flip_y, rotation| SpriteTransform {
                position,
                size: Vec2::new(32.0, 24.0),
                origin: Vec2::new(8.0, 18.0),
                flip_x,
                flip_y,
                rotation,
            };
            frame.world_2d(camera, |canvas| {
                for &(pos, fx, fy, rotation, tint, source) in &cases {
                    assert!(canvas.sprite(
                        self.texture.unwrap(),
                        source,
                        transform(pos, fx, fy, rotation),
                        tint
                    ));
                }
                let t = transform(cases[0].0, false, false, 0.0);
                assert!(!canvas.sprite(self.stale.unwrap(), region, t, Color::MAGENTA));
                assert!(!canvas.sprite(
                    self.texture.unwrap(),
                    SpriteRegion::new(13, 4, 4, 4).unwrap(),
                    t,
                    Color::MAGENTA
                ));
                assert!(!canvas.sprite(
                    self.texture.unwrap(),
                    SpriteRegion::new(4, 13, 4, 4).unwrap(),
                    t,
                    Color::MAGENTA
                ));
                for invalid in [
                    SpriteTransform {
                        size: Vec2::ZERO,
                        ..t
                    },
                    SpriteTransform {
                        origin: Vec2::splat(f32::NAN),
                        ..t
                    },
                    SpriteTransform {
                        rotation: f32::MAX,
                        ..t
                    },
                ] {
                    assert!(!canvas.sprite(self.texture.unwrap(), region, invalid, Color::MAGENTA));
                }
            });
            assert_eq!(frame.draw_counters().unwrap().textures, cases.len() as u64);
            let mut image = frame.target.texture().load_image().unwrap();
            image.flip_vertical();
            let scale =
                Vec2::new(image.width as f32, image.height as f32) / frame.viewport.logical_size;
            for (case, &(pos, fx, fy, rotation, tint, _)) in cases.iter().enumerate() {
                let t = transform(pos, fx, fy, rotation);
                for (x, y) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let local = Vec2::new(x, y) * t.size - t.origin;
                    let world = t.position + Mat2::from_angle(t.rotation) * local;
                    let pixel = camera.world_to_ui(world, &frame.viewport) * scale;
                    let actual = image.get_color(pixel.x.floor() as i32, pixel.y.floor() as i32);
                    let right = (x > 0.5) != fx;
                    let bottom = (y > 0.5) != fy;
                    let source = if case == 6 {
                        Color::WHITE
                    } else if case == 7 {
                        Color::MAGENTA
                    } else {
                        match (right, bottom) {
                            (false, false) => Color::RED,
                            (true, false) => Color::GREEN,
                            (false, true) => Color::BLUE,
                            (true, true) => Color::WHITE,
                        }
                    };
                    for (a, s, t) in [
                        (actual.r, source.r, tint.r),
                        (actual.g, source.g, tint.g),
                        (actual.b, source.b, tint.b),
                    ] {
                        let expected = s as f32 * t as f32 / 255.0 * tint.a as f32 / 255.0;
                        assert!(
                            (a as f32 - expected).abs() <= 2.0,
                            "case {case} local {x},{y}: {actual:?}, source {source:?}, tint {tint:?}"
                        );
                    }
                }
            }
            // Outside all quads, the clear survives (pivot/size do not stretch
            // the entire sheet, and rejected draws never reach the renderer).
            assert_eq!(image.get_color(2, 2), Color::BLACK);
        }
    }
    for (size, mode, dpi) in [
        ((640, 360), ScaleMode::Fit, 1.0),
        ((640, 800), ScaleMode::Fit, 1.0),
        ((800, 360), ScaleMode::Expand, 1.0),
        ((1000, 700), ScaleMode::IntegerFit, 1.0),
        ((160, 90), ScaleMode::IntegerFit, 1.0),
        ((640, 360), ScaleMode::Fit, 2.0),
    ] {
        let mut config = Config::new("native sprite probe");
        config.window_size = size;
        config.reference_size = Vec2::new(320.0, 180.0);
        config.scale_mode = mode;
        config.exit_key = None;
        App::new(config)
            .with_options(RunOptions {
                frames: Some(2),
                hidden: true,
                uncapped: true,
                ..RunOptions::default()
            })
            .run(Probe {
                texture: None,
                stale: None,
                dpi,
            })
            .unwrap();
    }
}
