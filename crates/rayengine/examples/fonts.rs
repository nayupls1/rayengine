//! Two distributable fonts at multiple sizes; resize to compare display scales.
use rayengine::prelude::*;

#[derive(Default)]
struct Fonts {
    body: Option<FontId>,
    pixel: Option<FontId>,
}
impl Game for Fonts {
    fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fonts/rayengine.toml");
        let declarations = FontDeclarations::load(manifest)?;
        let fonts = context.fonts(&declarations)?;
        self.body = Some(fonts["body"]);
        self.pixel = Some(fonts["pixel"]);
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(15, 23, 36, 255));
        frame.ui(|ui| {
            for (column, (font, name)) in [
                (self.body.unwrap(), "Smooth outlines"),
                (self.pixel.unwrap(), "Nearest pixels"),
            ]
            .into_iter()
            .enumerate()
            {
                let x = 30.0 + column as f32 * 470.0;
                ui.text_with(
                    name,
                    Vec2::new(x, 20.0),
                    TextStyle::new(font, 20.0),
                    Color::SKYBLUE,
                )
                .unwrap();
                for (row, size) in [16.0, 24.0, 40.0].into_iter().enumerate() {
                    let position = Vec2::new(x, 80.0 + row as f32 * 140.0);
                    let style = TextStyle {
                        spacing: 1.0,
                        ..TextStyle::new(font, size)
                    };
                    let metrics = ui.measure_text("Hello, Ray!", style).unwrap();
                    ui.rectangle(
                        Aabb2 {
                            min: position + metrics.ink_bounds.min,
                            max: position + metrics.ink_bounds.max,
                        },
                        Color::new(40, 57, 78, 255),
                    );
                    ui.text_with("Hello, Ray!", position, style, Color::WHITE)
                        .unwrap();
                    let bounds = Aabb2 {
                        min: position + Vec2::new(0.0, 60.0),
                        max: position + Vec2::new(420.0, 110.0),
                    };
                    let mut state = UiState::with_capacity(1);
                    state.update(&[UiRegion::new(UiId(1), bounds)], UiInput::default());
                    ui.try_button(
                        bounds,
                        "Play",
                        state.response(UiId(1)).unwrap(),
                        UiButtonStyle {
                            font: Some(font),
                            font_size: size,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                }
            }
        });
    }
}
fn main() -> Result<(), Error> {
    App::new(Config::new("Custom fonts — resize to compare scales"))
        .with_options(RunOptions::from_env()?)
        .run(Fonts::default())?;
    Ok(())
}
