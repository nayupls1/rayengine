//! Physical bindings sampled once per render frame into shared action states.

use rayengine_core::input::{Action, Input};
use raylib::prelude::{GamepadButton, KeyboardKey, MouseButton, RaylibHandle};

/// Physical button source. Several buttons can drive one action.
#[derive(Clone, Copy, Debug)]
pub enum Button {
    /// Keyboard key.
    Key(KeyboardKey),
    /// Mouse button.
    Mouse(MouseButton),
    /// Gamepad button and zero-based device index.
    Gamepad {
        /// Device index.
        device: i32,
        /// Physical button.
        button: GamepadButton,
    },
}

impl From<KeyboardKey> for Button {
    fn from(key: KeyboardKey) -> Self {
        Self::Key(key)
    }
}

/// Game-owned action bindings. Sampling ORs all physical buttons for an action.
#[derive(Debug, Default)]
pub struct Bindings {
    actions: Vec<(Action, Vec<Button>)>,
}

impl Bindings {
    /// Empty binding set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a physical binding and returns the set for fluent setup.
    pub fn bind(mut self, action: Action, button: impl Into<Button>) -> Self {
        if let Some((_, buttons)) = self.actions.iter_mut().find(|(a, _)| *a == action) {
            buttons.push(button.into());
        } else {
            self.actions.push((action, vec![button.into()]));
        }
        self
    }

    pub(crate) fn capacity(&self) -> usize {
        self.actions
            .iter()
            .map(|(a, _)| usize::from(a.0) + 1)
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn sample(&self, raylib: &RaylibHandle, input: &mut Input) {
        if !raylib.is_window_focused() {
            input.release_all();
            return;
        }
        for (action, buttons) in &self.actions {
            let mut down = false;
            let mut pressed = false;
            for &button in buttons {
                let state = match button {
                    Button::Key(key) => (raylib.is_key_down(key), raylib.is_key_pressed(key)),
                    Button::Mouse(button) => (
                        raylib.is_mouse_button_down(button),
                        raylib.is_mouse_button_pressed(button),
                    ),
                    Button::Gamepad { device, button } if raylib.is_gamepad_available(device) => (
                        raylib.is_gamepad_button_down(device, button),
                        raylib.is_gamepad_button_pressed(device, button),
                    ),
                    Button::Gamepad { .. } => (false, false),
                };
                down |= state.0;
                pressed |= state.1;
            }
            if pressed && !input.down(*action) {
                input.set(*action, true);
            }
            input.set(*action, down);
        }
    }
}
