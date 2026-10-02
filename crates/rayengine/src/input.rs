//! Configurable physical sources sampled once per render frame.
//!
//! Buttons are ORed. Analog sources use the greatest absolute value, with the
//! first binding winning ties. See [`crate::guides::timing_input`] for timing,
//! routing, device selection and persistence policies.

mod config;
mod serde_enums;
#[cfg(test)]
mod tests;

use crate::Error;
pub use config::{ActionBinding, AxisConfig, BindingConfig};
use rayengine_core::input::{Action, Axis, Input};
use raylib::prelude::{GamepadAxis, GamepadButton, KeyboardKey, MouseButton, RaylibHandle};
use serde::{Deserialize, Serialize};

/// Physical button source. Several buttons can drive one action.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "serde_enums::ButtonData", into = "serde_enums::ButtonData")]
pub enum Button {
    /// Keyboard key, serialized using its raylib name.
    Key(KeyboardKey),
    /// Mouse button, serialized using its raylib name.
    Mouse(MouseButton),
    /// Gamepad button and explicit zero-based backend device slot.
    Gamepad {
        /// Device slot, from zero through three. No automatic device switching.
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

impl Button {
    fn validate(self) -> Result<(), Error> {
        match self {
            Self::Key(KeyboardKey::KEY_NULL) => Err(invalid("KEY_NULL is not a binding")),
            Self::Gamepad { device, button } => {
                validate_device(device)?;
                if button == GamepadButton::GAMEPAD_BUTTON_UNKNOWN {
                    return Err(invalid("GAMEPAD_BUTTON_UNKNOWN is not a binding"));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// Physical analog source, including digital equivalents for keyboard movement.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", deny_unknown_fields)]
pub enum AxisSource {
    /// Held buttons contribute -1 and +1; opposing buttons cancel.
    Buttons {
        /// Negative direction.
        negative: Button,
        /// Positive direction.
        positive: Button,
    },
    /// Stick or trigger on an explicit backend device slot.
    Gamepad {
        /// Device slot, from zero through three.
        device: i32,
        /// Stick axes use [-1, 1]; triggers are normalized to [0, 1].
        #[serde(with = "serde_enums::axis")]
        axis: GamepadAxis,
    },
}

/// Per-source analog processing. Values are clamped after sensitivity is applied.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisBinding {
    /// Physical source.
    pub source: AxisSource,
    /// Scalar dead zone in [0, 1). Remaining travel is rescaled to full range.
    pub dead_zone: f32,
    /// Negates the processed value (including triggers).
    pub inverted: bool,
    /// Finite multiplier in [0, 100]. Zero disables this source.
    pub sensitivity: f32,
}

impl AxisBinding {
    /// Defaults to a 0.15 dead zone, no inversion, and unit sensitivity.
    pub fn new(source: AxisSource) -> Self {
        Self {
            source,
            dead_zone: 0.15,
            inverted: false,
            sensitivity: 1.0,
        }
    }

    /// Checks source/device selection and finite processing parameters.
    pub fn validate(&self) -> Result<(), Error> {
        match self.source {
            AxisSource::Buttons { negative, positive } => {
                negative.validate()?;
                positive.validate()?;
            }
            AxisSource::Gamepad { device, .. } => validate_device(device)?,
        }
        if !self.dead_zone.is_finite() || !(0.0..1.0).contains(&self.dead_zone) {
            return Err(invalid("dead_zone must be finite and in [0, 1)"));
        }
        if !self.sensitivity.is_finite() || !(0.0..=100.0).contains(&self.sensitivity) {
            return Err(invalid("sensitivity must be finite and in [0, 100]"));
        }
        Ok(())
    }

    /// Processes a backend sample. Stick/button raw values are [-1, 1]; raylib
    /// trigger raw values are -1 at rest and +1 when fully pressed. `None`
    /// (unavailable device/axis) and nonfinite samples always produce zero.
    pub fn process(&self, raw: Option<f32>) -> f32 {
        let Some(mut value) = raw.filter(|v| v.is_finite()) else {
            return 0.0;
        };
        value = value.clamp(-1.0, 1.0);
        if matches!(
            self.source,
            AxisSource::Gamepad {
                axis: GamepadAxis::GAMEPAD_AXIS_LEFT_TRIGGER
                    | GamepadAxis::GAMEPAD_AXIS_RIGHT_TRIGGER,
                ..
            }
        ) {
            value = (value + 1.0) * 0.5;
        }
        let magnitude = ((value.abs() - self.dead_zone) / (1.0 - self.dead_zone)).max(0.0);
        let value = (value.signum() * magnitude * self.sensitivity).clamp(-1.0, 1.0);
        if self.inverted { -value } else { value }
    }
}

/// Game-owned mappings. Sampling does not allocate after slots are preallocated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bindings {
    actions: Vec<(Action, Vec<Button>)>,
    axes: Vec<(Axis, Vec<AxisBinding>)>,
}

impl Bindings {
    /// Empty binding set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a button for fluent setup. The runner validates initial bindings.
    /// Use [`Self::add`] for fallible runtime changes.
    pub fn bind(mut self, action: Action, button: impl Into<Button>) -> Self {
        self.push_button(action, button.into());
        self
    }

    /// Adds a validated analog source for fluent setup.
    pub fn bind_axis(mut self, axis: Axis, binding: AxisBinding) -> Result<Self, Error> {
        self.add_axis(axis, binding)?;
        Ok(self)
    }

    fn push_button(&mut self, action: Action, button: Button) {
        if let Some((_, buttons)) = self.actions.iter_mut().find(|(a, _)| *a == action) {
            buttons.push(button);
        } else {
            self.actions.push((action, vec![button]));
        }
    }

    /// Appends a physical button without modifying the set on validation failure.
    pub fn add(&mut self, action: Action, button: impl Into<Button>) -> Result<(), Error> {
        let button = button.into();
        button.validate()?;
        self.push_button(action, button);
        Ok(())
    }

    /// Replaces all sources for an action. An empty list removes the action.
    pub fn rebind(&mut self, action: Action, buttons: Vec<Button>) -> Result<(), Error> {
        for button in &buttons {
            button.validate()?;
        }
        self.remove(action);
        if !buttons.is_empty() {
            self.actions.push((action, buttons));
        }
        Ok(())
    }

    /// Removes one physical source. Returns whether it was present.
    pub fn remove_button(&mut self, action: Action, button: Button) -> bool {
        let Some((_, buttons)) = self.actions.iter_mut().find(|(a, _)| *a == action) else {
            return false;
        };
        let before = buttons.len();
        buttons.retain(|b| *b != button);
        let removed = before != buttons.len();
        if buttons.is_empty() {
            self.remove(action);
        }
        removed
    }

    /// Removes all sources for a button action. Returns whether it was present.
    pub fn remove(&mut self, action: Action) -> bool {
        let before = self.actions.len();
        self.actions.retain(|(a, _)| *a != action);
        before != self.actions.len()
    }

    /// Appends a validated analog source.
    pub fn add_axis(&mut self, axis: Axis, binding: AxisBinding) -> Result<(), Error> {
        binding.validate()?;
        if let Some((_, sources)) = self.axes.iter_mut().find(|(a, _)| *a == axis) {
            sources.push(binding);
        } else {
            self.axes.push((axis, vec![binding]));
        }
        Ok(())
    }

    /// Replaces all sources for an analog axis, leaving old sources on error.
    /// An empty list removes the axis.
    pub fn rebind_axis(&mut self, axis: Axis, sources: Vec<AxisBinding>) -> Result<(), Error> {
        for source in &sources {
            source.validate()?;
        }
        self.remove_axis(axis);
        if !sources.is_empty() {
            self.axes.push((axis, sources));
        }
        Ok(())
    }

    /// Removes all sources for an analog axis. Returns whether it was present.
    pub fn remove_axis(&mut self, axis: Axis) -> bool {
        let before = self.axes.len();
        self.axes.retain(|(a, _)| *a != axis);
        before != self.axes.len()
    }

    /// Current physical sources for an action; unknown actions have none.
    pub fn buttons(&self, action: Action) -> &[Button] {
        self.actions
            .iter()
            .find(|(a, _)| *a == action)
            .map_or(&[], |(_, b)| b)
    }

    /// Current analog sources; unknown axes have none.
    pub fn axis_bindings(&self, axis: Axis) -> &[AxisBinding] {
        self.axes
            .iter()
            .find(|(a, _)| *a == axis)
            .map_or(&[], |(_, b)| b)
    }

    /// Validates all bindings. Also called before window creation by the runner.
    pub fn validate(&self) -> Result<(), Error> {
        for (_, buttons) in &self.actions {
            for button in buttons {
                button.validate()?;
            }
        }
        for (_, bindings) in &self.axes {
            for binding in bindings {
                binding.validate()?;
            }
        }
        Ok(())
    }

    pub(crate) fn capacities(&self) -> (usize, usize) {
        (
            self.actions
                .iter()
                .map(|(a, _)| usize::from(a.0) + 1)
                .max()
                .unwrap_or(0),
            self.axes
                .iter()
                .map(|(a, _)| usize::from(a.0) + 1)
                .max()
                .unwrap_or(0),
        )
    }

    // Reconcile after each fixed update too, so removed/rebound sources cannot
    // keep moving during catch-up ticks. Only changes clone/allocate a snapshot.
    pub(crate) fn reconcile(&self, previous: &mut Self, input: &mut Input) -> Result<(), Error> {
        if self == previous {
            return Ok(());
        }
        // Whole-set assignment and fluent `bind` can bypass fallible mutators.
        // Reject those changes before they can reach any native sample.
        self.validate()?;
        for (action, buttons) in &previous.actions {
            if self.buttons(*action) != buttons {
                input.set(*action, false);
            }
        }
        for (axis, sources) in &previous.axes {
            if self.axis_bindings(*axis) != sources {
                input.set_axis(*axis, 0.0);
            }
        }
        previous.clone_from(self);
        Ok(())
    }

    pub(crate) fn sample(
        &self,
        raylib: &RaylibHandle,
        input: &mut Input,
        sampling: &mut SamplingState,
    ) {
        self.sample_from(&NativeInput(raylib), input, sampling);
    }

    fn sample_from(
        &self,
        backend: &impl PhysicalInput,
        input: &mut Input,
        sampling: &mut SamplingState,
    ) {
        if !backend.focused() {
            sampling.trigger_armed.fill(false);
            input.release_all();
            return;
        }
        // Poll device presence even without active trigger bindings, so an old
        // armed state never survives a sampled disconnect and reconnect.
        for device in 0..4 {
            if !backend.available(device) {
                sampling.trigger_armed[device as usize * 2..device as usize * 2 + 2].fill(false);
            }
        }
        for (action, buttons) in &self.actions {
            let mut down = false;
            let mut pressed = false;
            for &button in buttons {
                let state = backend.button(button);
                down |= state.0;
                pressed |= state.1;
            }
            if pressed && !input.down(*action) {
                input.set(*action, true);
            }
            input.set(*action, down);
        }
        for (axis, sources) in &self.axes {
            let mut value: f32 = 0.0;
            for source in sources {
                let raw = match source.source {
                    AxisSource::Buttons { negative, positive } => Some(
                        f32::from(backend.button(positive).0)
                            - f32::from(backend.button(negative).0),
                    ),
                    AxisSource::Gamepad { device, axis } => {
                        sampling.filter(device, axis, backend.axis(device, axis))
                    }
                };
                let candidate = source.process(raw);
                if candidate.abs() > value.abs() {
                    value = candidate;
                }
            }
            input.set_axis(*axis, value);
        }
    }
}

// GLFW reports six logical axes even for mappings with missing triggers. Those
// missing triggers read zero, indistinguishable from a real half press. Requiring
// an observed release before accepting pressure keeps such mappings neutral.
#[derive(Debug, Default)]
pub(crate) struct SamplingState {
    trigger_armed: [bool; 8],
}

impl SamplingState {
    fn filter(&mut self, device: i32, axis: GamepadAxis, raw: Option<f32>) -> Option<f32> {
        let trigger = match axis {
            GamepadAxis::GAMEPAD_AXIS_LEFT_TRIGGER => 0,
            GamepadAxis::GAMEPAD_AXIS_RIGHT_TRIGGER => 1,
            _ => return raw,
        };
        if !(0..4).contains(&device) {
            return None;
        }
        let armed = &mut self.trigger_armed[device as usize * 2 + trigger];
        let Some(raw) = raw.filter(|value| value.is_finite()) else {
            *armed = false;
            return None;
        };
        // Allow small drift near raylib's released trigger endpoint (-1).
        if raw <= -0.95 {
            *armed = true;
        }
        armed.then_some(raw)
    }
}

trait PhysicalInput {
    fn focused(&self) -> bool;
    fn available(&self, device: i32) -> bool;
    fn button(&self, button: Button) -> (bool, bool);
    fn axis(&self, device: i32, axis: GamepadAxis) -> Option<f32>;
}

struct NativeInput<'a>(&'a RaylibHandle);
impl PhysicalInput for NativeInput<'_> {
    fn focused(&self) -> bool {
        self.0.is_window_focused()
    }
    fn available(&self, device: i32) -> bool {
        (0..4).contains(&device) && self.0.is_gamepad_available(device)
    }
    fn button(&self, button: Button) -> (bool, bool) {
        match button {
            Button::Key(key) => (self.0.is_key_down(key), self.0.is_key_pressed(key)),
            Button::Mouse(button) => (
                self.0.is_mouse_button_down(button),
                self.0.is_mouse_button_pressed(button),
            ),
            Button::Gamepad { device, button } if self.available(device) => (
                self.0.is_gamepad_button_down(device, button),
                self.0.is_gamepad_button_pressed(device, button),
            ),
            Button::Gamepad { .. } => (false, false),
        }
    }
    fn axis(&self, device: i32, axis: GamepadAxis) -> Option<f32> {
        (self.available(device) && self.0.get_gamepad_axis_count(device) > axis as i32)
            .then(|| self.0.get_gamepad_axis_movement(device, axis))
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Config(message.into())
}
fn validate_device(device: i32) -> Result<(), Error> {
    if !(0..4).contains(&device) {
        return Err(invalid("gamepad device must be a slot in 0..=3"));
    }
    Ok(())
}
