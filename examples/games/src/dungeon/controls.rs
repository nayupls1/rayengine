use super::profile::Profile;
use rayengine::{
    prelude::*,
    raylib::prelude::{GamepadAxis as PadAxis, GamepadButton as Pad, MouseButton},
};
pub const ATTACK: Action = Action(0);
pub const DASH: Action = Action(1);
pub const INTERACT: Action = Action(2);
pub const PAUSE: Action = Action(3);
pub const INVENTORY: Action = Action(4);
pub const RESET: Action = Action(5);
pub const PRIMARY: Action = Action(6);
pub const NEXT: Action = Action(7);
pub const PREVIOUS: Action = Action(8);
pub const ACCEPT: Action = Action(9);
pub const LESS: Action = Action(10);
pub const MORE: Action = Action(11);
pub const MOVE_X: Axis = Axis(0);
pub const MOVE_Y: Axis = Axis(1);
pub const AIM_X: Axis = Axis(2);
pub const AIM_Y: Axis = Axis(3);
pub const UI: UiActions = UiActions {
    primary: PRIMARY,
    next: NEXT,
    previous: PREVIOUS,
    activate: ACCEPT,
    cancel: PAUSE,
};
fn pad(button: Pad) -> Button {
    Button::Gamepad { device: 0, button }
}
fn key_axis(a: KeyboardKey, b: KeyboardKey) -> AxisBinding {
    AxisBinding::new(AxisSource::Buttons {
        negative: a.into(),
        positive: b.into(),
    })
}
fn axis(axis: PadAxis) -> AxisBinding {
    AxisBinding::new(AxisSource::Gamepad { device: 0, axis })
}
pub fn bindings(settings: &Profile) -> Bindings {
    use KeyboardKey::*;
    use Pad::*;
    let (left, right, up, down) = if settings.arrows {
        (KEY_LEFT, KEY_RIGHT, KEY_UP, KEY_DOWN)
    } else {
        (KEY_A, KEY_D, KEY_W, KEY_S)
    };
    Bindings::new()
        .bind(
            ATTACK,
            if settings.alternate_attack {
                KEY_K
            } else {
                KEY_J
            },
        )
        .bind(ATTACK, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
        .bind(ATTACK, pad(GAMEPAD_BUTTON_RIGHT_FACE_LEFT))
        .bind(DASH, KEY_SPACE)
        .bind(DASH, pad(GAMEPAD_BUTTON_RIGHT_FACE_DOWN))
        .bind(INTERACT, KEY_E)
        .bind(INTERACT, pad(GAMEPAD_BUTTON_RIGHT_FACE_UP))
        .bind(PAUSE, KEY_ESCAPE)
        .bind(PAUSE, pad(GAMEPAD_BUTTON_MIDDLE_RIGHT))
        .bind(PAUSE, pad(GAMEPAD_BUTTON_RIGHT_FACE_RIGHT))
        .bind(INVENTORY, KEY_TAB)
        .bind(INVENTORY, pad(GAMEPAD_BUTTON_MIDDLE_LEFT))
        .bind(RESET, KEY_R)
        .bind(RESET, pad(GAMEPAD_BUTTON_LEFT_TRIGGER_1))
        .bind(PRIMARY, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
        .bind(NEXT, KEY_DOWN)
        .bind(NEXT, KEY_TAB)
        .bind(NEXT, pad(GAMEPAD_BUTTON_LEFT_FACE_DOWN))
        .bind(PREVIOUS, KEY_UP)
        .bind(PREVIOUS, pad(GAMEPAD_BUTTON_LEFT_FACE_UP))
        .bind(ACCEPT, KEY_ENTER)
        .bind(ACCEPT, pad(GAMEPAD_BUTTON_RIGHT_FACE_DOWN))
        .bind(LESS, KEY_LEFT)
        .bind(LESS, pad(GAMEPAD_BUTTON_LEFT_FACE_LEFT))
        .bind(MORE, KEY_RIGHT)
        .bind(MORE, pad(GAMEPAD_BUTTON_LEFT_FACE_RIGHT))
        .bind_axis(MOVE_X, key_axis(left, right))
        .unwrap()
        .bind_axis(MOVE_X, axis(PadAxis::GAMEPAD_AXIS_LEFT_X))
        .unwrap()
        .bind_axis(MOVE_Y, key_axis(up, down))
        .unwrap()
        .bind_axis(MOVE_Y, axis(PadAxis::GAMEPAD_AXIS_LEFT_Y))
        .unwrap()
        .bind_axis(AIM_X, axis(PadAxis::GAMEPAD_AXIS_RIGHT_X))
        .unwrap()
        .bind_axis(AIM_Y, axis(PadAxis::GAMEPAD_AXIS_RIGHT_Y))
        .unwrap()
}
