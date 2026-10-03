// Explicit names keep the JSON independent of raylib's integer enum layout.
use raylib::prelude::{GamepadAxis, GamepadButton, KeyboardKey, MouseButton};
use serde::{Deserialize, Deserializer, Serializer};

macro_rules! named_enum {
    ($module:ident, $ty:ident, [$($variant:ident),+ $(,)?]) => {
        pub(super) mod $module {
            use super::*;
            pub fn serialize<S: Serializer>(value: &$ty, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(match value { $($ty::$variant => stringify!($variant)),+ })
            }
            pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<$ty, D::Error> {
                let name = String::deserialize(deserializer)?;
                match name.as_str() {
                    $(stringify!($variant) => Ok($ty::$variant)),+,
                    _ => Err(serde::de::Error::custom(format!("unknown {} name: {name}", stringify!($ty)))),
                }
            }
        }
    };
}

named_enum!(
    key,
    KeyboardKey,
    [
        KEY_NULL,
        KEY_APOSTROPHE,
        KEY_COMMA,
        KEY_MINUS,
        KEY_PERIOD,
        KEY_SLASH,
        KEY_ZERO,
        KEY_ONE,
        KEY_TWO,
        KEY_THREE,
        KEY_FOUR,
        KEY_FIVE,
        KEY_SIX,
        KEY_SEVEN,
        KEY_EIGHT,
        KEY_NINE,
        KEY_SEMICOLON,
        KEY_EQUAL,
        KEY_A,
        KEY_B,
        KEY_C,
        KEY_D,
        KEY_E,
        KEY_F,
        KEY_G,
        KEY_H,
        KEY_I,
        KEY_J,
        KEY_K,
        KEY_L,
        KEY_M,
        KEY_N,
        KEY_O,
        KEY_P,
        KEY_Q,
        KEY_R,
        KEY_S,
        KEY_T,
        KEY_U,
        KEY_V,
        KEY_W,
        KEY_X,
        KEY_Y,
        KEY_Z,
        KEY_LEFT_BRACKET,
        KEY_BACKSLASH,
        KEY_RIGHT_BRACKET,
        KEY_GRAVE,
        KEY_SPACE,
        KEY_ESCAPE,
        KEY_ENTER,
        KEY_TAB,
        KEY_BACKSPACE,
        KEY_INSERT,
        KEY_DELETE,
        KEY_RIGHT,
        KEY_LEFT,
        KEY_DOWN,
        KEY_UP,
        KEY_PAGE_UP,
        KEY_PAGE_DOWN,
        KEY_HOME,
        KEY_END,
        KEY_CAPS_LOCK,
        KEY_SCROLL_LOCK,
        KEY_NUM_LOCK,
        KEY_PRINT_SCREEN,
        KEY_PAUSE,
        KEY_F1,
        KEY_F2,
        KEY_F3,
        KEY_F4,
        KEY_F5,
        KEY_F6,
        KEY_F7,
        KEY_F8,
        KEY_F9,
        KEY_F10,
        KEY_F11,
        KEY_F12,
        KEY_LEFT_SHIFT,
        KEY_LEFT_CONTROL,
        KEY_LEFT_ALT,
        KEY_LEFT_SUPER,
        KEY_RIGHT_SHIFT,
        KEY_RIGHT_CONTROL,
        KEY_RIGHT_ALT,
        KEY_RIGHT_SUPER,
        KEY_KB_MENU,
        KEY_KP_0,
        KEY_KP_1,
        KEY_KP_2,
        KEY_KP_3,
        KEY_KP_4,
        KEY_KP_5,
        KEY_KP_6,
        KEY_KP_7,
        KEY_KP_8,
        KEY_KP_9,
        KEY_KP_DECIMAL,
        KEY_KP_DIVIDE,
        KEY_KP_MULTIPLY,
        KEY_KP_SUBTRACT,
        KEY_KP_ADD,
        KEY_KP_ENTER,
        KEY_KP_EQUAL,
        KEY_BACK,
        KEY_MENU,
        KEY_VOLUME_UP,
        KEY_VOLUME_DOWN,
    ]
);

named_enum!(
    mouse,
    MouseButton,
    [
        MOUSE_BUTTON_LEFT,
        MOUSE_BUTTON_RIGHT,
        MOUSE_BUTTON_MIDDLE,
        MOUSE_BUTTON_SIDE,
        MOUSE_BUTTON_EXTRA,
        MOUSE_BUTTON_FORWARD,
        MOUSE_BUTTON_BACK,
    ]
);

named_enum!(
    button,
    GamepadButton,
    [
        GAMEPAD_BUTTON_UNKNOWN,
        GAMEPAD_BUTTON_LEFT_FACE_UP,
        GAMEPAD_BUTTON_LEFT_FACE_RIGHT,
        GAMEPAD_BUTTON_LEFT_FACE_DOWN,
        GAMEPAD_BUTTON_LEFT_FACE_LEFT,
        GAMEPAD_BUTTON_RIGHT_FACE_UP,
        GAMEPAD_BUTTON_RIGHT_FACE_RIGHT,
        GAMEPAD_BUTTON_RIGHT_FACE_DOWN,
        GAMEPAD_BUTTON_RIGHT_FACE_LEFT,
        GAMEPAD_BUTTON_LEFT_TRIGGER_1,
        GAMEPAD_BUTTON_LEFT_TRIGGER_2,
        GAMEPAD_BUTTON_RIGHT_TRIGGER_1,
        GAMEPAD_BUTTON_RIGHT_TRIGGER_2,
        GAMEPAD_BUTTON_MIDDLE_LEFT,
        GAMEPAD_BUTTON_MIDDLE,
        GAMEPAD_BUTTON_MIDDLE_RIGHT,
        GAMEPAD_BUTTON_LEFT_THUMB,
        GAMEPAD_BUTTON_RIGHT_THUMB,
    ]
);

named_enum!(
    axis,
    GamepadAxis,
    [
        GAMEPAD_AXIS_LEFT_X,
        GAMEPAD_AXIS_LEFT_Y,
        GAMEPAD_AXIS_RIGHT_X,
        GAMEPAD_AXIS_RIGHT_Y,
        GAMEPAD_AXIS_LEFT_TRIGGER,
        GAMEPAD_AXIS_RIGHT_TRIGGER,
    ]
);

// Struct variants avoid serde's missing-content handling for foreign newtypes.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub(super) enum ButtonData {
    Key {
        #[serde(with = "key")]
        key: KeyboardKey,
    },
    Mouse {
        #[serde(with = "mouse")]
        button: MouseButton,
    },
    Gamepad {
        device: i32,
        #[serde(with = "button")]
        button: GamepadButton,
    },
}
impl From<super::Button> for ButtonData {
    fn from(button: super::Button) -> Self {
        match button {
            super::Button::Key(key) => Self::Key { key },
            super::Button::Mouse(button) => Self::Mouse { button },
            super::Button::Gamepad { device, button } => Self::Gamepad { device, button },
        }
    }
}
impl From<ButtonData> for super::Button {
    fn from(button: ButtonData) -> Self {
        match button {
            ButtonData::Key { key } => Self::Key(key),
            ButtonData::Mouse { button } => Self::Mouse(button),
            ButtonData::Gamepad { device, button } => Self::Gamepad { device, button },
        }
    }
}
