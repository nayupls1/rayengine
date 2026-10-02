//! Identical diagonal/3D/text workload for native quality comparisons.
use rayengine::prelude::*;
use rayengine::raylib::prelude::{
    Font, RaylibDraw, RaylibFont, RaylibTexture2D, TextureFilter, Vector2,
};

#[derive(Default)]
struct Comparison {
    font: Option<Font>,
}
impl Game for Comparison {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.font = Some(
            ctx.raylib
                .load_font_from_memory(
                    ctx.thread,
                    ".ttf",
                    include_bytes!("assets/iAWriterMonoS-Regular.ttf"),
                    64,
                    None,
                )
                .map_err(|e| Error::Asset(e.to_string()))?,
        );
        self.font
            .as_ref()
            .unwrap()
            .texture()
            .set_texture_filter(ctx.thread, TextureFilter::TEXTURE_FILTER_BILINEAR);
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::BLACK);
        frame.world_3d(
            Camera3D {
                position: Vec3::new(3.0, 2.0, 5.0),
                target: Vec3::ZERO,
                ..Default::default()
            },
            |c| {
                c.cube(
                    Aabb3::from_center(Vec3::ZERO, Vec3::splat(1.6)),
                    Color::WHITE,
                );
                c.sphere(Vec3::new(1.8, 0.0, 0.0), 0.5, Color::WHITE);
            },
        );
        frame.world_2d(
            Camera2D {
                view_height: 360.0,
                ..Default::default()
            },
            |c| {
                c.line(
                    Vec2::new(-280.0, -120.0),
                    Vec2::new(-100.0, 90.0),
                    1.5,
                    Color::WHITE,
                );
                c.line(
                    Vec2::new(-260.0, -120.0),
                    Vec2::new(-80.0, 90.0),
                    6.0,
                    Color::WHITE,
                );
            },
        );
        frame.ui(|ui| {
            ui.rectangle(
                Aabb2 {
                    min: Vec2::new(12.0, 12.0),
                    max: Vec2::new(600.0, 96.0),
                },
                Color::new(18, 24, 36, 210),
            );
            ui.text(
                "Native UI / diagonal edges / 3D silhouettes",
                Vec2::new(24.0, 22.0),
                18.0,
                Color::WHITE,
            );
            // Raw font bridge pending typed custom font support (#47). Coordinates
            // here are native target pixels, independently of the world render scale.
            let scale = ui.pixel_scale();
            ui.raw.draw_text_ex(
                self.font.as_ref().unwrap(),
                "Custom font: Aa Bb 0123",
                Vector2::new(24.0 * scale.x, 52.0 * scale.y),
                24.0 * scale.y,
                scale.y,
                Color::WHITE,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let profile = args.next().unwrap_or_else(|| "native".into());
    let mut config = Config::new("Rendering quality comparison");
    let manifest = rayengine::manifest::ProjectManifest::load(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/render_quality.toml"),
    )
    .map_err(|e| Error::Config(e.to_string()))?;
    let resolved = manifest
        .resolve(Some(&profile))
        .map_err(|e| Error::Config(e.to_string()))?;
    config = config.with_project(&resolved)?;
    let report = App::new(config)
        .with_options(RunOptions::parse(args)?)
        .run(Comparison::default())?;
    println!("{} frames in {:?}", report.frames, report.elapsed);
    Ok(())
}
