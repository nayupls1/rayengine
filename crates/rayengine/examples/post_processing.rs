//! Run `cargo run -p rayengine --example post_processing -- effects`.
use rayengine::{diagnostics::DiagnosticsConfig, prelude::*};

struct Demo {
    mode: String,
    effects: Vec<MaterialId>,
    preview: Option<RenderTargetId>,
    enabled: bool,
    ui: UiPlacement,
}
impl Game for Demo {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(Action(0), KeyboardKey::KEY_T)
            .bind(Action(1), KeyboardKey::KEY_U)
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        if self.mode == "effects" {
            self.preview = Some(ctx.assets.create_render_target(RenderTargetDesc {
                size: TargetSize::Reference,
                filter: TargetFilter::Bilinear,
            })?);
            for (effect, values) in [
                (
                    BuiltinEffect::ColorGrade,
                    vec![
                        ("gain", UniformValue::Vec3(Vec3::new(1.1, 0.9, 0.8))),
                        ("lift", UniformValue::Vec3(Vec3::new(0.02, 0.0, 0.01))),
                    ],
                ),
                (
                    BuiltinEffect::Vignette,
                    vec![("strength", UniformValue::Float(0.65))],
                ),
                (
                    BuiltinEffect::Scanlines,
                    vec![
                        ("strength", UniformValue::Float(0.2)),
                        ("lines", UniformValue::Float(270.0)),
                    ],
                ),
            ] {
                let shader = ctx.shader_from_source(None, effect.fragment_source())?;
                for (name, value) in values {
                    ctx.uniform(shader, name, value)?;
                }
                self.effects.push(ctx.material(effect.material(shader))?);
            }
        }
        self.enabled = self.mode == "effects";
        if self.mode != "direct" {
            self.configure(ctx.assets)?;
        }
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        if ctx.input.pressed(Action(0)) {
            self.enabled = !self.enabled;
        }
        if ctx.input.pressed(Action(1)) {
            self.ui = match self.ui {
                UiPlacement::AfterEffects => UiPlacement::BeforeEffects,
                UiPlacement::BeforeEffects => UiPlacement::AfterEffects,
            };
        }
    }
    fn boundary(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        if self.mode == "effects" {
            self.configure(ctx.assets)?;
        }
        Ok(())
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(26, 42, 68, 255));
        let camera = Camera3D {
            position: Vec3::new(4.0, 3.0, 6.0),
            target: Vec3::ZERO,
            ..Default::default()
        };
        if let Some(id) = self.preview {
            frame
                .with_target(id, |target| {
                    target.clear(Color::new(12, 20, 32, 255));
                    target.world_3d(camera, |world| {
                        world.cube(
                            Aabb3::from_center(Vec3::ZERO, Vec3::splat(2.0)),
                            Color::ORANGE,
                        )
                    });
                })
                .unwrap();
        }
        frame.world_3d(camera, |world| {
            world.cube(
                Aabb3::from_center(Vec3::ZERO, Vec3::splat(2.0)),
                Color::SKYBLUE,
            );
            world.sphere(Vec3::new(2.0, 0.0, 0.0), 0.75, Color::ORANGE);
        });
        frame.world_2d(Camera2D::default(), |world| {
            world.line(
                Vec2::new(-4.0, -2.0),
                Vec2::new(-1.0, 2.0),
                0.05,
                Color::GREEN,
            )
        });
        frame.ui(|ui| {
            ui.text(
                "T: toggle effects   U: UI before/after effects",
                Vec2::new(20.0, 20.0),
                20.0,
                Color::WHITE,
            );
            if let Some(id) = self.preview {
                ui.render_target(
                    id,
                    Aabb2 {
                        min: Vec2::new(20.0, 80.0),
                        max: Vec2::new(260.0, 215.0),
                    },
                    Color::WHITE,
                );
            }
        });
    }
}
impl Demo {
    fn configure(&self, assets: &mut rayengine::assets::Assets<'_>) -> Result<(), Error> {
        assets.set_post_processing(PostProcessing {
            materials: if self.enabled {
                self.effects.clone()
            } else {
                Vec::new()
            },
            ui: self.ui,
        })
    }
}
fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "effects".into());
    if !["direct", "empty", "effects"].contains(&mode.as_str()) {
        return Err(Error::Config(
            "mode must be direct, empty or effects".into(),
        ));
    }
    let mut options = RunOptions::parse(args)?;
    options
        .diagnostics
        .get_or_insert_with(|| DiagnosticsConfig::new("post-processing/mixed.v1"));
    let report = App::new(Config::new("Render targets and post-processing"))
        .with_options(options)
        .run(Demo {
            mode,
            effects: Vec::new(),
            preview: None,
            enabled: false,
            ui: UiPlacement::AfterEffects,
        })?;
    println!("{} frames in {:?}", report.frames, report.elapsed);
    Ok(())
}
