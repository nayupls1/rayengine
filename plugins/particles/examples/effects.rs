//! Sparks, smoke and pickup effects using only procedural art.
use rayengine::core::glam::Vec4;
use rayengine::{prelude::*, raylib::prelude::Image};
use rayengine_particles::{
    Emitter, EmitterConfig,
    render::{ParticleEffect, ParticleView},
};

const BURST: Action = Action(0);
const VIEW: Action = Action(1);
const SMOKE: Action = Action(2);
const RESET: Action = Action(3);
struct Demo {
    view: ParticleView,
    effects: Vec<ParticleEffect>,
}
fn config(position: Vec3, color: Vec4) -> EmitterConfig {
    EmitterConfig {
        position,
        start_color: color,
        end_color: Vec4::new(color.x, color.y, color.z, 0.0),
        capacity: 512,
        max_spawn: 128,
        lifetime: [0.8, 1.5],
        start_size: 0.15,
        velocity_spread: Vec3::new(1.5, 1.5, 0.3),
        seed: 42,
        ..Default::default()
    }
}
impl Game for Demo {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(BURST, KeyboardKey::KEY_SPACE)
            .bind(VIEW, KeyboardKey::KEY_TAB)
            .bind(SMOKE, KeyboardKey::KEY_S)
            .bind(RESET, KeyboardKey::KEY_R)
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let mut image = Image::gen_image_color(32, 32, Color::BLANK);
        for y in 0..32 {
            for x in 0..32 {
                let radius = Vec2::new(x as f32 - 15.5, y as f32 - 15.5).length() / 16.0;
                let alpha = ((1.0 - radius).max(0.0).powi(2) * 255.0) as u8;
                image.draw_pixel(x, y, Color::new(255, 255, 255, alpha));
            }
        }
        let texture = ctx.texture_from_image(&image)?;
        let sparks = EmitterConfig {
            rate: 24.0,
            velocity: Vec3::new(0.0, 2.0, 0.0),
            acceleration: Vec3::new(0.0, -3.0, 0.0),
            ..config(Vec3::new(-2.5, 0.0, 0.0), Vec4::new(1.0, 0.5, 0.05, 1.0))
        };
        let smoke = EmitterConfig {
            rate: 40.0,
            lifetime: [2.0, 3.0],
            velocity: Vec3::new(0.0, 0.8, 0.0),
            velocity_spread: Vec3::splat(0.15),
            start_size: 0.6,
            end_size: 1.6,
            ..config(Vec3::ZERO, Vec4::new(0.7, 0.75, 0.8, 0.6))
        };
        let pickup = EmitterConfig {
            velocity_spread: Vec3::splat(2.0),
            end_size: 0.05,
            ..config(Vec3::new(2.5, 1.0, 0.0), Vec4::new(0.2, 1.0, 0.5, 1.0))
        };
        for (config, sprite) in [
            (sparks, None),
            (smoke, Some(texture)),
            (pickup, Some(texture)),
        ] {
            let emitter = Emitter::new(config).map_err(|e| Error::Config(e.to_string()))?;
            let mut effect = ParticleEffect::new(emitter, sprite.map(Into::into))?;
            effect.init(&mut self.view, ctx)?;
            effect.burst(64);
            self.effects.push(effect);
        }
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        if ctx.input.pressed(VIEW) {
            self.view = match self.view {
                ParticleView::TwoD(_) => ParticleView::ThreeD(camera()),
                ParticleView::ThreeD(_) => ParticleView::TwoD(Camera2D {
                    view_height: 8.0,
                    ..Default::default()
                }),
            };
        }
        if ctx.input.pressed(SMOKE) {
            let smoke = &mut self.effects[1];
            if smoke.emitter().is_emitting() {
                smoke.stop();
            } else {
                smoke.start();
            }
        }
        for effect in &mut self.effects {
            if ctx.input.pressed(RESET) {
                effect.reset();
            }
            if ctx.input.pressed(BURST) {
                effect.burst(96);
            }
            effect.fixed_update(&mut self.view, ctx);
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(15, 22, 35, 255));
        // These spatially separated emitters need no global transparency sort.
        for effect in &mut self.effects {
            effect.draw(&self.view, frame);
        }
        frame.ui(|ui| {
            ui.text(
                "Sparks       Smoke       Pickup",
                Vec2::new(20.0, 20.0),
                22.0,
                Color::WHITE,
            );
            ui.text(
                "TAB 2D/3D   SPACE burst   S smoke   R reset",
                Vec2::new(20.0, 52.0),
                18.0,
                Color::LIGHTGRAY,
            );
        });
    }
}
fn camera() -> Camera3D {
    Camera3D {
        position: Vec3::new(0.0, 2.5, 9.0),
        target: Vec3::new(0.0, 1.0, 0.0),
        ..Default::default()
    }
}
fn main() -> Result<(), Error> {
    let mut config = Config::new("Particle effects");
    config.audio = false;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Demo {
            view: ParticleView::ThreeD(camera()),
            effects: Vec::new(),
        })?;
    Ok(())
}
