use super::*;
use crate::render::UiButtonStyle;
use rayengine_core::ui::{UiButton, UiId, UiInput, UiRect, UiRegion, UiState};
use std::cell::Cell;

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_ui_cursor_and_scaled_icons_smoke() {
    struct Probe {
        path: PathBuf,
        texture: Option<TextureId>,
        stale: Option<TextureId>,
        ui: UiState,
        policy_calls: Cell<usize>,
        previous_calls: usize,
        menu: bool,
    }
    impl Game for Probe {
        fn cursor_mode(&self) -> CursorMode {
            self.policy_calls.set(self.policy_calls.get() + 1);
            if self.menu {
                CursorMode::Free
            } else {
                CursorMode::Captured
            }
        }
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            let stale = ctx.texture(&self.path)?;
            ctx.assets.unload_texture(stale);
            self.stale = Some(stale);
            self.texture = Some(ctx.texture(&self.path)?);

            // Hidden test windows may lack focus. Exercise the runner's actual
            // native policy application with explicit focus transitions too.
            let mut state = CursorState::default();
            sync_cursor(ctx.raylib, &mut state, CursorMode::Captured, true);
            assert!(ctx.raylib.is_cursor_hidden());
            assert!(!state.take_motion());
            sync_cursor(ctx.raylib, &mut state, CursorMode::Free, true);
            assert!(!ctx.raylib.is_cursor_hidden());
            sync_cursor(ctx.raylib, &mut state, CursorMode::Captured, true);
            assert!(ctx.raylib.is_cursor_hidden());
            sync_cursor(ctx.raylib, &mut state, CursorMode::Captured, false);
            assert!(!ctx.raylib.is_cursor_hidden());
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            assert!(
                self.policy_calls.get() > self.previous_calls,
                "runner must re-read cursor policy"
            );
            self.previous_calls = self.policy_calls.get();
            self.menu = !self.menu;
            let bounds = UiRect::bottom_right(Vec2::splat(-20.0), Vec2::new(140.0, 50.0))
                .resolve(frame.viewport.logical_size);
            let mut region = UiRegion::new(UiId(1), bounds);
            region.enabled = frame.index != 3;
            let primary = match frame.index {
                1 => UiButton {
                    down: true,
                    pressed: true,
                    released: false,
                },
                2 => UiButton {
                    released: true,
                    ..UiButton::default()
                },
                _ => UiButton::default(),
            };
            self.ui.update(
                &[region],
                UiInput {
                    pointer: Some(bounds.center()),
                    primary,
                    window_focused: true,
                    ..UiInput::default()
                },
            );
            let style = UiButtonStyle::default();
            let icon = UiRect::top_left(Vec2::splat(30.0), Vec2::splat(40.0))
                .resolve(frame.viewport.logical_size);
            frame.clear(Color::BLACK);
            frame.ui(|canvas| {
                canvas.button(bounds, "", self.ui.response(UiId(1)).unwrap(), style);
                assert!(canvas.icon(self.texture.unwrap(), icon, Color::WHITE));
                assert!(!canvas.icon(self.stale.unwrap(), icon, Color::RED));
            });
            let mut image = frame.target.texture().load_image().unwrap();
            image.flip_vertical();
            let scale =
                Vec2::new(image.width as f32, image.height as f32) / frame.viewport.logical_size;
            let sample = |position: Vec2| {
                image.get_color((position.x * scale.x) as i32, (position.y * scale.y) as i32)
            };
            assert_eq!(sample(icon.center()), Color::GREEN);
            let expected = match frame.index {
                1 => style.pressed,
                3 => style.disabled,
                _ => style.hovered,
            };
            assert_eq!(sample(bounds.min + Vec2::splat(8.0)), expected);
            if frame.index == 2 {
                assert!(self.ui.response(UiId(1)).unwrap().activated);
            }
        }
    }
    let path = std::env::temp_dir().join(format!("rayengine-ui-icon-{}.png", std::process::id()));
    let png = Image::gen_image_color(8, 8, Color::GREEN)
        .export_image_to_memory(".png")
        .unwrap();
    std::fs::write(&path, &*png).unwrap();
    for (size, mode) in [
        ((640, 360), ScaleMode::Fit),
        ((640, 800), ScaleMode::Fit),
        ((1000, 400), ScaleMode::Expand),
    ] {
        let mut config = Config::new("native UI probe");
        config.window_size = size;
        config.scale_mode = mode;
        config.exit_key = None;
        App::new(config)
            .with_options(RunOptions {
                frames: Some(4),
                hidden: true,
                uncapped: true,
                ..RunOptions::default()
            })
            .run(Probe {
                path: path.clone(),
                texture: None,
                stale: None,
                ui: UiState::with_capacity(1),
                policy_calls: Cell::new(0),
                previous_calls: 0,
                menu: true,
            })
            .unwrap();
    }
    std::fs::remove_file(path).unwrap();
}
