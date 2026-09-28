//! Frozen gameplay fixtures rendered through the real SDK and raylib backend.

use rayengine::{prelude::*, raylib::prelude::Image};

struct Frozen<G>(G);

impl<G: Game> Game for Frozen<G> {
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        self.0.draw(frame);
    }
}

pub(crate) fn screenshot(game: impl Game, filename: &str) -> Image {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/regressions")
        .join(filename);
    let mut config = Config::new("rayengine gameplay regression");
    config.window_size = (960, 540);
    config.vsync = false;
    let report = App::new(config)
        .with_options(RunOptions {
            frames: Some(3),
            screenshot: Some(path.clone()),
            hidden: true,
            uncapped: true,
            ..RunOptions::default()
        })
        .run(Frozen(game))
        .unwrap();
    assert_eq!(report.frames, 3);
    let image = Image::load_image(path.to_str().unwrap()).unwrap();
    assert_eq!((image.width, image.height), (960, 540));
    image
}
