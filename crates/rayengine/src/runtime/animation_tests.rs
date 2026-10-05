use super::*;
use crate::assets::{ModelAnimationsId, ModelClipId, ModelPose};
use crate::material::MaterialDesc;
use rayengine_core::{
    camera::Camera3D as EngineCamera3D,
    glam::{Mat4, Quat, Vec3, Vec4},
    skeletal::{KeyframeRate, PlaybackMode},
    transform::Transform3D,
};

const CHARACTER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/character.glb");
const PENDULUM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pendulum.glb");
const CLEAR: Color = Color::new(10, 20, 30, 255);
const SKIN: Color = Color::new(232, 186, 140, 255);

fn camera() -> EngineCamera3D {
    EngineCamera3D {
        position: Vec3::new(0.0, 1.3, 4.5),
        target: Vec3::new(0.0, 1.3, 0.0),
        ..EngineCamera3D::default()
    }
}

/// Target pixel containing a world point.
fn pixel(frame: &Frame<'_, '_>, image: &Image, world: Vec3) -> (i32, i32) {
    let camera = camera();
    let clip = camera.projection(&frame.viewport, 0.01, 1000.0)
        * camera.view_matrix()
        * Vec4::new(world.x, world.y, world.z, 1.0);
    let ndc = clip.truncate() / clip.w;
    (
        ((ndc.x + 1.0) * 0.5 * image.width as f32) as i32,
        ((1.0 - ndc.y) * 0.5 * image.height as f32) as i32,
    )
}

fn near(actual: Color, expected: Color) -> bool {
    [
        (actual.r, expected.r),
        (actual.g, expected.g),
        (actual.b, expected.b),
    ]
    .iter()
    .all(|&(a, e)| a.abs_diff(e) <= 6)
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_animation_poses_shared_models_and_cleans_up() {
    #[derive(Default)]
    struct Probe {
        character: Option<ModelId>,
        set: Option<ModelAnimationsId>,
        stale: Option<ModelClipId>,
        wave: Option<ModelClipId>,
        walk: Option<ModelClipId>,
        swing: Option<ModelClipId>,
        material: Option<MaterialId>,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            let character = ctx.model(CHARACTER)?;
            let set = ctx.model_animations(CHARACTER, KeyframeRate::GLTF)?;
            assert_eq!(set, ctx.model_animations(CHARACTER, KeyframeRate::GLTF)?);
            // The cache key includes the rate, which determines clip timing.
            let other_rate = ctx.model_animations(CHARACTER, KeyframeRate::per_second(30))?;
            assert_ne!(other_rate, set);
            assert_eq!(ctx.assets.model_clip_count(set), Some(3));
            let names: Vec<_> = (0..3)
                .map(|i| {
                    let clip = ctx.assets.model_clip(set, i).unwrap();
                    assert_eq!((clip.set(), clip.index()), (set, i));
                    ctx.assets.model_clip_info(clip).unwrap().name().to_owned()
                })
                .collect();
            assert_eq!(names, ["idle", "walk", "wave"]);
            assert!(ctx.assets.model_clip(set, 3).is_none());
            assert!(ctx.assets.find_model_clip(set, "run").is_none());
            let walk = ctx.assets.find_model_clip(set, "walk").unwrap();
            let wave = ctx.assets.find_model_clip(set, "wave").unwrap();
            let info = ctx.assets.model_clip_info(walk).unwrap();
            assert_eq!(info.bones(), 7);
            // A one-second glTF loop is resampled at 60 Hz, including both ends.
            assert_eq!(info.timing().keyframes(), 61);
            assert_eq!(info.timing().duration(), std::time::Duration::from_secs(1));
            let slow = ctx.assets.find_model_clip(other_rate, "walk").unwrap();
            assert_eq!(
                ctx.assets
                    .model_clip_info(slow)
                    .unwrap()
                    .timing()
                    .duration(),
                std::time::Duration::from_secs(2)
            );
            ctx.assets.check_model_clip(character, walk)?;
            let animator = ctx
                .assets
                .model_animator(character, wave, PlaybackMode::Once)?;
            assert_eq!(animator.clip(), wave);
            assert_eq!(animator.timing().keyframes(), 97);

            // Different skeletons are rejected in both directions, before posing.
            let pendulum = ctx.model(PENDULUM)?;
            let pendulum_set = ctx.model_animations(PENDULUM, KeyframeRate::GLTF)?;
            let swing = ctx.assets.find_model_clip(pendulum_set, "swing").unwrap();
            ctx.assets.check_model_clip(pendulum, swing)?;
            let error = ctx
                .assets
                .check_model_clip(character, swing)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("2 bones but the model skeleton has 7"),
                "{error}"
            );
            assert!(ctx.assets.check_model_clip(pendulum, walk).is_err());
            assert!(
                ctx.assets
                    .model_animator(character, swing, PlaybackMode::Loop)
                    .is_err()
            );

            // Static models have no skeleton; non-animation files have no clips.
            let directory = std::env::temp_dir()
                .join(format!("rayengine-animation-probe-{}", std::process::id()));
            std::fs::create_dir_all(&directory)?;
            let obj = directory.join("triangle.obj");
            std::fs::write(&obj, "v -1 0 0\nv 1 0 0\nv 0 2 0\nf 1 2 3\n")?;
            let triangle = ctx.model(&obj)?;
            let error = ctx
                .assets
                .check_model_clip(triangle, walk)
                .unwrap_err()
                .to_string();
            assert!(error.contains("no skeleton"), "{error}");
            assert!(ctx.model_animations(&obj, KeyframeRate::GLTF).is_err());
            // A scene-root joint would crash the native glTF animation loader.
            let root_joint = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/root_joint.glb");
            let error = ctx
                .model_animations(root_joint, KeyframeRate::GLTF)
                .unwrap_err()
                .to_string();
            assert!(error.contains("parent node"), "{error}");
            assert!(
                ctx.model_animations(directory.join("missing.glb"), KeyframeRate::GLTF)
                    .is_err()
            );
            std::fs::remove_dir_all(&directory)?;

            // Unloading a set invalidates its clips forever; reloading is a new set.
            let before = ctx.assets.resource_counts();
            assert_eq!(before.model_animations, 3);
            assert_eq!(before.model_clips, 7);
            assert!(before.model_animation_bytes > 0);
            assert!(ctx.assets.unload_model_animations(other_rate));
            assert!(!ctx.assets.unload_model_animations(other_rate));
            assert!(ctx.assets.model_clip_info(slow).is_none());
            assert!(ctx.assets.model_clip_count(other_rate).is_none());
            let error = ctx
                .assets
                .check_model_clip(character, slow)
                .unwrap_err()
                .to_string();
            assert!(error.contains("unloaded"), "{error}");
            let after = ctx.assets.resource_counts();
            assert_eq!((after.model_animations, after.model_clips), (2, 4));
            assert!(after.model_animation_bytes < before.model_animation_bytes);
            let reloaded = ctx.model_animations(CHARACTER, KeyframeRate::per_second(30))?;
            assert_ne!(reloaded, other_rate);
            assert!(ctx.assets.unload_model_animations(reloaded));

            self.material = Some(ctx.material(MaterialDesc {
                tint: Color::new(40, 220, 90, 255),
                ..MaterialDesc::default()
            })?);
            ctx.assets.unload_model(pendulum);
            *self = Self {
                character: Some(character),
                set: Some(set),
                stale: Some(slow),
                wave: Some(wave),
                walk: Some(walk),
                swing: Some(swing),
                ..*self
            };
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            let character = self.character.unwrap();
            let (wave, walk) = (self.wave.unwrap(), self.walk.unwrap());
            let at = |x: f32| Transform3D {
                position: Vec3::new(x, 0.0, 0.0),
                ..Transform3D::default()
            };
            let at_z = |z: f32| Transform3D {
                position: Vec3::new(0.0, 0.0, z),
                ..Transform3D::default()
            };
            // 0.35 s into the wave the right arm is raised 150 degrees.
            let raised = ModelPose {
                clip: wave,
                keyframe: 21.0,
            };
            let rest = ModelPose {
                clip: wave,
                keyframe: 0.0,
            };
            frame.set_draw_counters_enabled(true);
            frame.clear(CLEAR);
            let material = self.material.unwrap();
            let swing = self.swing.unwrap();
            frame.world_3d(camera(), |canvas| {
                // One shared model, three poses in one pass: each draw re-poses it.
                assert!(
                    canvas
                        .try_animated_model(character, rest, at(-1.2), Color::WHITE)
                        .unwrap()
                );
                assert!(
                    canvas
                        .try_animated_model(character, raised, at(1.2), Color::WHITE)
                        .unwrap()
                );
                assert!(canvas.animated_model_material(
                    character,
                    material,
                    raised,
                    at_z(-3.0),
                    Color::WHITE
                ));
                // Invalid keyframes, transforms and skeletons are errors without a draw.
                for keyframe in [-0.5, 96.01, f32::NAN, f32::INFINITY] {
                    let pose = ModelPose {
                        clip: wave,
                        keyframe,
                    };
                    assert!(
                        canvas
                            .try_animated_model(character, pose, at(0.0), Color::RED)
                            .is_err()
                    );
                    assert!(!canvas.animated_model(character, pose, at(0.0), Color::RED));
                }
                let last = ModelPose {
                    clip: wave,
                    keyframe: 96.0,
                };
                let hidden = Transform3D {
                    position: Vec3::new(0.0, -50.0, 0.0),
                    ..Transform3D::default()
                };
                assert!(canvas.animated_model(character, last, hidden, Color::RED));
                for invalid in [
                    Transform3D {
                        rotation: Quat::from_xyzw(0.0, 0.0, 0.0, 0.0),
                        ..at(0.0)
                    },
                    Transform3D {
                        scale: Vec3::splat(f32::NAN),
                        ..at(0.0)
                    },
                ] {
                    assert!(
                        canvas
                            .try_animated_model(character, rest, invalid, Color::RED)
                            .is_err()
                    );
                }
                let mismatch = ModelPose {
                    clip: swing,
                    keyframe: 0.0,
                };
                assert!(
                    canvas
                        .try_animated_model(character, mismatch, at(0.0), Color::RED)
                        .is_err()
                );
                // Stale clips and models are skipped like other stale handles.
                let stale = ModelPose {
                    clip: self.stale.unwrap(),
                    keyframe: 0.0,
                };
                assert!(
                    !canvas
                        .try_animated_model(character, stale, at(0.0), Color::RED)
                        .unwrap()
                );
                assert!(
                    !canvas
                        .try_animated_model_material_matrix(
                            character,
                            material,
                            stale,
                            Mat4::IDENTITY,
                            Color::RED
                        )
                        .unwrap()
                );
            });
            let counters = frame.draw_counters().unwrap();
            assert_eq!(counters.model_poses, 4);
            assert_eq!(counters.models, 4);
            let mut image = frame.target.texture().load_image().unwrap();
            image.flip_vertical();
            let color = |world: Vec3| {
                let (x, y) = pixel(frame, &image, world);
                image.get_color(x, y)
            };
            // Raised hand: shoulder (-0.31, 1.58) plus the bind hand offset
            // (-0.07, -0.5) rotated by -150 degrees about Z.
            let hand = Vec3::new(-0.5, 2.05, 0.07);
            let bind_hand = Vec3::new(-0.38, 1.08, 0.07);
            let left = Vec3::new(-1.2, 0.0, 0.0);
            let right = Vec3::new(1.2, 0.0, 0.0);
            assert!(
                near(color(left + bind_hand), SKIN),
                "{:?}",
                color(left + bind_hand)
            );
            assert_eq!(color(left + hand), CLEAR);
            assert!(near(color(right + hand), SKIN), "{:?}", color(right + hand));
            assert_eq!(color(right + bind_hand), CLEAR);
            // The material override follows the same pose behind them.
            let back = Vec3::new(0.0, 0.0, -3.0);
            assert_ne!(color(back + hand), CLEAR);
            assert_eq!(color(back + bind_hand), CLEAR);

            // Unloading the active set or model makes later draws stale.
            assert!(frame.assets.unload_model_animations(self.set.unwrap()));
            assert!(frame.assets.check_model_clip(character, walk).is_err());
            frame.world_3d(camera(), |canvas| {
                assert!(
                    !canvas
                        .try_animated_model(character, raised, at(0.0), Color::RED)
                        .unwrap()
                );
            });
            frame.assets.unload_model(character);
            assert!(frame.assets.model(character).is_none());
        }
    }
    let mut config = Config::new("native animation probe");
    config.window_size = (640, 360);
    config.exit_key = None;
    App::new(config)
        .with_options(RunOptions {
            frames: Some(1),
            hidden: true,
            uncapped: true,
            ..RunOptions::default()
        })
        .run(Probe::default())
        .unwrap();
}
