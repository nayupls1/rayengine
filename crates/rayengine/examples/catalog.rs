//! Object catalog and action inspector over a 3D world, with CPU input routing.
//! Run: `cargo run -p rayengine --example catalog`.
use rayengine::prelude::*;
use rayengine::raylib::prelude::{GamepadButton, MouseButton};

const PRIMARY: Action = Action(0);
const NEXT: Action = Action(1);
const PREVIOUS: Action = Action(2);
const ACTIVATE: Action = Action(3);
const TOGGLE: Action = Action(4);
const CANCEL: Action = Action(5);
const UI_ACTIONS: UiActions = UiActions {
    primary: PRIMARY,
    next: NEXT,
    previous: PREVIOUS,
    activate: ACTIVATE,
    cancel: CANCEL,
};
const WORLD_ACTIONS: &[Action] = &[PRIMARY, ACTIVATE, NEXT, PREVIOUS];
const PANEL: UiId = UiId(1);
const HEADER: UiId = UiId(2);
const CATALOG: UiId = UiId(3);
const CATALOG_THUMB: UiId = UiId(4);
const INSPECTOR: UiId = UiId(5);
const INSPECTOR_THUMB: UiId = UiId(6);
const ITEM_BASE: u64 = 100;
const ACTION_BASE: u64 = 1000;
const ACTION_LABELS: &[&str] = &[
    "Rotate",
    "Grow",
    "Shrink",
    "Paint blue",
    "Paint orange",
    "Reset",
    "Close catalog",
];
const THUMB_MIN: f32 = 24.0;

struct Catalog {
    open: bool,
    ui: UiState,
    scroll: [UiScrollState; 2],
    panel_offset: Vec2,
    labels: Vec<String>,
    selected: usize,
    angle: f32,
    cube_size: f32,
    tint: Color,
    world_actions: usize,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            open: true,
            ui: UiState::with_capacity(48),
            scroll: [UiScrollState::default(); 2],
            panel_offset: Vec2::ZERO,
            labels: (0..40).map(|i| format!("Object {:02}", i + 1)).collect(),
            selected: 0,
            angle: 0.0,
            cube_size: 1.5,
            tint: Color::SKYBLUE,
            world_actions: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct Layout {
    panel: Aabb2,
    header: Aabb2,
    views: [Aabb2; 2],
    tracks: [Aabb2; 2],
    items: [UiLayout; 2],
}

fn rect(min: Vec2, size: Vec2) -> Aabb2 {
    Aabb2 {
        min,
        max: min + size,
    }
}

impl Catalog {
    fn layout(&self, size: Vec2) -> Layout {
        let dimensions = Vec2::new(
            (size.x - 48.0).clamp(400.0, 860.0),
            (size.y - 150.0).max(180.0),
        );
        let panel = rect((size - dimensions) * 0.5 + self.panel_offset, dimensions);
        let catalog_width = (dimensions.x - 72.0) * 0.55;
        let inspector_width = dimensions.x - catalog_width - 72.0;
        let height = dimensions.y - 90.0;
        let catalog = rect(
            panel.min + Vec2::new(16.0, 66.0),
            Vec2::new(catalog_width, height),
        );
        let inspector = rect(
            panel.min + Vec2::new(catalog_width + 48.0, 66.0),
            Vec2::new(inspector_width, height),
        );
        let tracks = [catalog, inspector].map(|view| {
            rect(
                Vec2::new(view.max.x + 4.0, view.min.y),
                Vec2::new(10.0, height),
            )
        });
        Layout {
            panel,
            header: rect(panel.min, Vec2::new(dimensions.x, 36.0)),
            views: [catalog, inspector],
            tracks,
            items: [
                UiLayout::grid(
                    2,
                    Vec2::new((catalog_width - 8.0) * 0.5, 54.0),
                    Vec2::splat(8.0),
                ),
                UiLayout::list(Vec2::new(inspector_width, 44.0), 8.0),
            ],
        }
    }

    fn configure(&mut self, layout: Layout) {
        for (i, count) in [self.labels.len(), ACTION_LABELS.len()]
            .into_iter()
            .enumerate()
        {
            self.scroll[i].configure(layout.views[i].size(), layout.items[i].content_size(count));
        }
    }

    fn regions(&self, layout: Layout) -> Vec<UiRegion> {
        let mut regions = Vec::with_capacity(self.labels.len() + ACTION_LABELS.len() + 6);
        let mut panel = UiRegion::new(PANEL, layout.panel);
        panel.focusable = false;
        regions.push(panel);
        let mut header = UiRegion::new(HEADER, layout.header);
        header.focusable = false;
        header.draggable = true;
        regions.push(header);
        for (i, (surface_id, thumb_id, base, count)) in [
            (CATALOG, CATALOG_THUMB, ITEM_BASE, self.labels.len()),
            (INSPECTOR, INSPECTOR_THUMB, ACTION_BASE, ACTION_LABELS.len()),
        ]
        .into_iter()
        .enumerate()
        {
            let clip = UiClip::new(layout.views[i]);
            let mut surface = UiRegion::new(surface_id, layout.views[i]);
            surface.focusable = false;
            surface.draggable = true; // Drag the gaps between items to pan content.
            surface.scroll_target = Some(surface_id);
            regions.push(surface);
            for index in 0..count {
                let mut item = UiRegion::new(
                    UiId(base + index as u64),
                    self.scroll[i].item_bounds(layout.views[i], layout.items[i].item(index)),
                );
                item.clip = Some(clip);
                item.scroll_target = Some(surface_id);
                regions.push(item); // Retain offscreen IDs for focus/navigation.
            }
            if let Some(bounds) = self.scroll[i].vertical_thumb(layout.tracks[i], THUMB_MIN) {
                let mut thumb = UiRegion::new(thumb_id, bounds);
                thumb.focusable = false;
                thumb.draggable = true;
                thumb.scroll_target = Some(surface_id);
                regions.push(thumb);
            }
        }
        regions
    }

    fn update(&mut self, size: Vec2, pointer: Option<Vec2>, focused: bool, input: &Input) {
        let was_open = self.open;
        if input.pressed(TOGGLE) {
            self.open = !self.open;
        }
        let layout = self.layout(size);
        // Clamp a dragged panel on every tick, including resize and reopen.
        let limit = ((size - layout.panel.size()) * 0.5 - Vec2::splat(8.0)).max(Vec2::ZERO);
        self.panel_offset = self.panel_offset.clamp(-limit, limit);
        let layout = self.layout(size);
        self.configure(layout);
        let old_focus = self.ui.focused();
        let regions = if self.open {
            self.regions(layout)
        } else {
            Vec::new()
        };
        let ui_input = UiInput::from_actions(input, pointer, UI_ACTIONS, focused);
        let capture = self.ui.update(&regions, ui_input);
        if self.open {
            self.panel_offset = (self.panel_offset + self.ui.response(HEADER).unwrap().drag_delta)
                .clamp(-limit, limit);
            for (i, (surface, thumb)) in [(CATALOG, CATALOG_THUMB), (INSPECTOR, INSPECTOR_THUMB)]
                .into_iter()
                .enumerate()
            {
                let response = self.ui.response(surface).unwrap();
                self.scroll[i].scroll_by(-response.scroll * 40.0 - response.drag_delta);
                if let Some(response) = self.ui.response(thumb) {
                    self.scroll[i].drag_vertical_thumb(
                        layout.tracks[i],
                        THUMB_MIN,
                        response.drag_delta.y,
                    );
                }
            }
            // Reveal keyboard/controller focus only when navigation changes it.
            if self.ui.focused() != old_focus
                && (ui_input.next || ui_input.previous)
                && let Some(id) = self.ui.focused()
            {
                for (i, (base, count)) in [
                    (ITEM_BASE, self.labels.len()),
                    (ACTION_BASE, ACTION_LABELS.len()),
                ]
                .into_iter()
                .enumerate()
                {
                    if id.0 >= base && id.0 < base + count as u64 {
                        self.scroll[i].reveal(layout.items[i].item((id.0 - base) as usize));
                    }
                }
            }
            for i in 0..self.labels.len() {
                if self
                    .ui
                    .response(UiId(ITEM_BASE + i as u64))
                    .unwrap()
                    .activated
                {
                    self.selected = i;
                }
            }
            for i in 0..ACTION_LABELS.len() {
                if self
                    .ui
                    .response(UiId(ACTION_BASE + i as u64))
                    .unwrap()
                    .activated
                {
                    match i {
                        0 => self.angle += 0.4,
                        1 => self.cube_size = (self.cube_size + 0.25).min(3.0),
                        2 => self.cube_size = (self.cube_size - 0.25).max(0.5),
                        3 => self.tint = Color::SKYBLUE,
                        4 => self.tint = Color::ORANGE,
                        5 => {
                            self.angle = 0.0;
                            self.cube_size = 1.5;
                            self.tint = Color::SKYBLUE;
                        }
                        6 => self.open = false,
                        _ => unreachable!(),
                    }
                }
            }
        }
        // Opening/closing ticks stay masked; drags cannot become world clicks.
        let modal = was_open || self.open;
        let gameplay = input
            .routed(
                if modal { WORLD_ACTIONS } else { &[] },
                modal || capture.pointer,
            )
            .with_blocked_scroll(modal || capture.pointer);
        if gameplay.pressed(PRIMARY) || gameplay.pressed(ACTIVATE) {
            self.world_actions += 1;
            self.angle += 0.4;
        }
    }
}

impl Game for Catalog {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(PRIMARY, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(NEXT, KeyboardKey::KEY_TAB)
            .bind(NEXT, KeyboardKey::KEY_DOWN)
            .bind(PREVIOUS, KeyboardKey::KEY_UP)
            .bind(
                NEXT,
                Button::Gamepad {
                    device: 0,
                    button: GamepadButton::GAMEPAD_BUTTON_LEFT_FACE_DOWN,
                },
            )
            .bind(
                PREVIOUS,
                Button::Gamepad {
                    device: 0,
                    button: GamepadButton::GAMEPAD_BUTTON_LEFT_FACE_UP,
                },
            )
            .bind(ACTIVATE, KeyboardKey::KEY_ENTER)
            .bind(ACTIVATE, KeyboardKey::KEY_SPACE)
            .bind(
                ACTIVATE,
                Button::Gamepad {
                    device: 0,
                    button: GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_DOWN,
                },
            )
            .bind(TOGGLE, KeyboardKey::KEY_ESCAPE)
            .bind(
                TOGGLE,
                Button::Gamepad {
                    device: 0,
                    button: GamepadButton::GAMEPAD_BUTTON_MIDDLE_RIGHT,
                },
            )
            .bind(CANCEL, KeyboardKey::KEY_BACKSPACE)
            .bind(
                CANCEL,
                Button::Gamepad {
                    device: 0,
                    button: GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_RIGHT,
                },
            )
    }

    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        self.update(
            ctx.viewport.logical_size,
            ctx.pointer,
            ctx.window_focused,
            ctx.input,
        );
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(16, 24, 37, 255));
        frame.world_3d(
            Camera3D {
                position: Vec3::new(self.angle.sin() * 8.0, 4.0, self.angle.cos() * 8.0),
                target: Vec3::ZERO,
                ..Camera3D::default()
            },
            |canvas| {
                canvas.cube(
                    Aabb3::from_center(Vec3::new(0.0, -2.0, 0.0), Vec3::new(12.0, 0.2, 12.0)),
                    Color::new(42, 63, 74, 255),
                );
                canvas.cube(
                    Aabb3::from_center(Vec3::ZERO, Vec3::splat(self.cube_size)),
                    self.tint,
                );
            },
        );
        let layout = self.layout(frame.viewport.logical_size);
        self.configure(layout); // Drawing can precede the first simulation tick.
        frame.ui(|canvas| {
            canvas.text(
                "ESC / Start: catalog   Click/Space in world: rotate",
                Vec2::splat(16.0),
                18.0,
                Color::WHITE,
            );
            canvas.text(
                "Wheel: scroll   Drag header, gaps or scrollbar   Up/Down/Tab: focus",
                Vec2::new(16.0, 40.0),
                16.0,
                Color::LIGHTGRAY,
            );
            if !self.open {
                return;
            }
            canvas.rectangle(layout.panel, Color::new(19, 28, 42, 255));
            canvas.rectangle(layout.header, Color::new(34, 51, 69, 255));
            canvas.text(
                "OBJECT CATALOG / drag header",
                layout.header.min + Vec2::new(12.0, 8.0),
                18.0,
                Color::WHITE,
            );
            canvas.text(
                "Objects",
                layout.views[0].min - Vec2::new(0.0, 24.0),
                16.0,
                Color::LIGHTGRAY,
            );
            canvas.text(
                &self.labels[self.selected],
                layout.views[1].min - Vec2::new(0.0, 24.0),
                16.0,
                Color::SKYBLUE,
            );
            for (i, (base, count)) in [
                (ITEM_BASE, self.labels.len()),
                (ACTION_BASE, ACTION_LABELS.len()),
            ]
            .into_iter()
            .enumerate()
            {
                canvas.clipped(UiClip::new(layout.views[i]), |canvas| {
                    for index in 0..count {
                        let label = if i == 0 {
                            self.labels[index].as_str()
                        } else {
                            ACTION_LABELS.get(index).copied().unwrap()
                        };
                        let id = UiId(base + index as u64);
                        let bounds = self.scroll[i]
                            .item_bounds(layout.views[i], layout.items[i].item(index));
                        let style = UiButtonStyle {
                            font_size: 16.0,
                            normal: if i == 0 && index == self.selected {
                                Color::new(25, 102, 120, 255)
                            } else {
                                UiButtonStyle::default().normal
                            },
                            ..UiButtonStyle::default()
                        };
                        if let Some(response) = self.ui.response(id) {
                            canvas.button(bounds, label, response, style);
                        } else {
                            canvas.rectangle(bounds, style.normal);
                            canvas.text(
                                label,
                                bounds.min + Vec2::new(8.0, 12.0),
                                16.0,
                                Color::WHITE,
                            );
                        }
                    }
                });
                canvas.rectangle(layout.tracks[i], Color::new(34, 51, 69, 255));
                if let Some(thumb) = self.scroll[i].vertical_thumb(layout.tracks[i], THUMB_MIN) {
                    canvas.rectangle(thumb, Color::SKYBLUE);
                }
            }
            canvas.text(
                "Enter / controller A: select   D-pad: focus   B / Backspace: cancel",
                Vec2::new(16.0, canvas.logical_size.y - 28.0),
                16.0,
                Color::LIGHTGRAY,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    let mut config = Config::new("rayengine / Catalog and inspector");
    config.exit_key = None;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Catalog::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_open_close_drag_and_activation_never_reach_world() {
        let size = Vec2::new(960.0, 540.0);
        let mut game = Catalog::default();
        let mut input = Input::with_capacity(6);
        let header = game.layout(size).header.center();
        input.set(PRIMARY, true);
        game.update(size, Some(header), true, &input);
        input.consume_edges();
        game.update(size, Some(header + Vec2::splat(30.0)), true, &input);
        assert_ne!(game.panel_offset, Vec2::ZERO);
        input.consume_edges();
        input.set(TOGGLE, true);
        input.set(PRIMARY, false);
        input.set(ACTIVATE, true);
        game.update(size, Some(header), true, &input);
        assert!(!game.open);
        assert_eq!(game.world_actions, 0);
        input.consume_edges();
        input.set(TOGGLE, false);
        input.set(ACTIVATE, false);
        input.consume_edges();
        input.set(PRIMARY, true);
        game.update(size, Some(header), true, &input);
        assert_eq!(game.world_actions, 1);
        input.consume_edges();
        input.set(PRIMARY, false);
        input.set(TOGGLE, true);
        input.set(ACTIVATE, true);
        game.update(size, Some(header), true, &input);
        assert!(game.open);
        assert_eq!(game.world_actions, 1);
    }

    #[test]
    fn navigation_reveals_scrolled_items_and_inspector_can_close() {
        let size = Vec2::new(960.0, 540.0);
        let mut game = Catalog::default();
        let mut input = Input::with_capacity(6);
        for _ in 0..game.labels.len() + ACTION_LABELS.len() {
            input.set(NEXT, true);
            game.update(size, None, true, &input);
            input.consume_edges();
            input.set(NEXT, false);
            input.consume_edges();
        }
        assert_eq!(game.ui.focused(), Some(UiId(ACTION_BASE + 6)));
        assert!(game.scroll[0].offset().y > 0.0);
        assert!(game.scroll[1].offset().y > 0.0);
        input.set(ACTIVATE, true);
        game.update(size, None, true, &input);
        assert!(!game.open);
        assert_eq!(game.world_actions, 0);
    }
}
