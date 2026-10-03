use super::*;
use crate::{CollisionFlags, TileDefinition, TileId};
use rayengine::{core::sprite::SpriteRegion, raylib::prelude::Image};
struct Probe {
    map: Tilemap,
    atlas: Option<TileAtlas>,
    stale: bool,
    invalid: bool,
}
impl Game for Probe {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let mut image = Image::gen_image_color(16, 8, Color::RED);
        image.draw_rectangle(8, 0, 8, 8, Color::GREEN);
        let texture = ctx.texture_from_image(&image)?;
        self.atlas = Some(TileAtlas::new(texture));
        if self.stale {
            ctx.assets.unload_texture(texture);
        }
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::BLUE);
        let camera = Camera2D {
            view_height: 8.0,
            rotation: 0.3,
            ..Default::default()
        };
        // Edits both remove the red overlay and restore it as green, without rebuild.
        self.map.set_tile(1, 0, 0, None).unwrap();
        self.map
            .set_tile(1, 0, 0, Some(TileId(if self.invalid { 2 } else { 1 })))
            .unwrap();
        let stats = self
            .atlas
            .unwrap()
            .draw_frame(&self.map, frame, camera)
            .unwrap();
        assert_eq!(stats.visible.visible_chunks, 2);
        assert_eq!(stats.visible.tiles, 2);
        assert_eq!(
            stats.drawn,
            if self.stale {
                0
            } else if self.invalid {
                1
            } else {
                2
            }
        );
    }
}
#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_tilemap_layer_order_edits_atlas_and_viewport_policies() {
    let directory = std::env::temp_dir().join(format!("rayengine-tilemap-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for mode in [ScaleMode::Fit, ScaleMode::Expand, ScaleMode::IntegerFit] {
        for size in [(960, 540), (600, 900)] {
            for (stale, invalid) in [(false, false), (true, false), (false, true)] {
                let mut map = Tilemap::new(
                    1,
                    1,
                    Vec2::splat(-2.0),
                    Vec2::splat(4.0),
                    [
                        SpriteRegion::new(0, 0, 8, 8).unwrap(),
                        SpriteRegion::new(8, 0, 8, 8).unwrap(),
                        SpriteRegion::new(15, 0, 8, 8).unwrap(),
                    ]
                    .into_iter()
                    .map(|region| TileDefinition {
                        region,
                        collision: CollisionFlags::default(),
                    })
                    .collect(),
                    vec!["ground".into(), "overlay".into()],
                )
                .unwrap();
                map.set_tile(0, 0, 0, Some(TileId(0))).unwrap();
                map.set_tile(1, 0, 0, Some(TileId(0))).unwrap();
                let screenshot =
                    directory.join(format!("{mode:?}-{}-{stale}-{invalid}.png", size.0));
                let mut config = Config::new("Tilemap native probe");
                config.audio = false;
                config.vsync = false;
                config.window_size = size;
                config.scale_mode = mode;
                App::new(config)
                    .with_options(RunOptions {
                        hidden: true,
                        frames: Some(2),
                        screenshot: Some(screenshot.clone()),
                        ..Default::default()
                    })
                    .run(Probe {
                        map,
                        atlas: None,
                        stale,
                        invalid,
                    })
                    .unwrap();
                let image = Image::load_image(screenshot.to_str().unwrap()).unwrap();
                let pixel = image.get_color(image.width / 2, image.height / 2);
                let expected = if stale {
                    Color::BLUE
                } else if invalid {
                    Color::RED
                } else {
                    Color::GREEN
                };
                assert_eq!(
                    pixel, expected,
                    "{mode:?} {size:?} stale={stale} invalid={invalid}"
                );
            }
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
