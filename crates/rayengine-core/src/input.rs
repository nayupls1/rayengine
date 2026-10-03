//! Action input whose edges survive render frames and are consumed per tick.

use glam::Vec2;

/// Small numeric action key, normally declared as a game-level constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Action(pub u16);

/// Small numeric analog axis key, in a separate namespace from button actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Axis(pub u16);

#[derive(Clone, Copy, Debug, Default)]
struct State {
    down: bool,
    pressed: bool,
    released: bool,
}

/// Contiguous action states; reads and sampling allocate nothing after setup.
///
/// Multiple physical bindings must be ORed before calling [`Self::set`].
/// Consume edges after each fixed update, not after every render frame.
///
/// ```
/// use rayengine_core::input::{Action, Input};
/// const JUMP: Action = Action(0);
/// let mut input = Input::with_capacity(1);
/// input.set(JUMP, true);
/// assert!(input.pressed(JUMP));
/// input.consume_edges();
/// assert!(input.down(JUMP));
/// assert!(!input.pressed(JUMP));
/// ```
#[derive(Debug, Default)]
pub struct Input {
    states: Vec<State>,
    axes: Vec<f32>,
    pointer_delta: Vec2,
    reset_pending: bool,
}

impl Input {
    /// Borrows action input with explicit per-update masks, without changing the
    /// original states or allocating. Block UI-owned actions and relative look
    /// motion before passing this view to gameplay. Held actions resume when
    /// unmasked; edges still follow the caller's normal tick consumption.
    pub fn routed<'a>(&'a self, blocked: &'a [Action], block_motion: bool) -> InputView<'a> {
        InputView {
            input: self,
            blocked,
            blocked_axes: &[],
            block_motion,
        }
    }
    /// Preallocates action slots to avoid allocation during input sampling.
    pub fn with_capacity(actions: usize) -> Self {
        Self {
            states: vec![State::default(); actions],
            axes: Vec::new(),
            pointer_delta: Vec2::ZERO,
            reset_pending: false,
        }
    }

    /// Preallocates independent button and analog slots for frame sampling.
    pub fn with_capacities(actions: usize, axes: usize) -> Self {
        Self {
            axes: vec![0.0; axes],
            ..Self::with_capacity(actions)
        }
    }

    /// Stores the latest analog value, clamped to `[-1, 1]`. Nonfinite values
    /// become neutral. Unlike pointer displacement, values persist across ticks.
    pub fn set_axis(&mut self, axis: Axis, value: f32) {
        let index = usize::from(axis.0);
        if index >= self.axes.len() {
            self.axes.resize(index + 1, 0.0);
        }
        self.axes[index] = if value.is_finite() {
            value.clamp(-1.0, 1.0)
        } else {
            0.0
        };
    }

    /// Latest sampled analog value. Unbound/unknown axes return zero.
    pub fn value(&self, axis: Axis) -> f32 {
        self.axes.get(usize::from(axis.0)).copied().unwrap_or(0.0)
    }

    /// Samples a combined action state. Automatically grows for new action IDs.
    pub fn set(&mut self, action: Action, down: bool) {
        let index = usize::from(action.0);
        if index >= self.states.len() {
            self.states.resize(index + 1, State::default());
        }
        let state = &mut self.states[index];
        state.pressed |= down && !state.down;
        state.released |= !down && state.down;
        state.down = down;
    }

    /// Whether an action is currently held.
    pub fn down(&self, action: Action) -> bool {
        self.states
            .get(usize::from(action.0))
            .is_some_and(|s| s.down)
    }

    /// Whether an action was pressed since the last consumed tick.
    pub fn pressed(&self, action: Action) -> bool {
        self.states
            .get(usize::from(action.0))
            .is_some_and(|s| s.pressed)
    }

    /// Whether an action was released since the last consumed tick.
    pub fn released(&self, action: Action) -> bool {
        self.states
            .get(usize::from(action.0))
            .is_some_and(|s| s.released)
    }

    /// Digital axis in `[-1, 1]`; opposing held actions cancel.
    pub fn axis(&self, negative: Action, positive: Action) -> f32 {
        f32::from(self.down(positive)) - f32::from(self.down(negative))
    }

    /// Accumulates relative pointer motion sampled between fixed updates.
    /// The SDK supplies logical window units, independent of viewport scaling.
    /// Panics for nonfinite motion.
    pub fn add_pointer_delta(&mut self, delta: Vec2) {
        assert!(delta.is_finite());
        self.pointer_delta += delta;
    }

    /// Relative pointer motion since the previous consumed tick.
    /// Apply sensitivity directly; this is displacement, not velocity.
    pub fn pointer_delta(&self) -> Vec2 {
        self.pointer_delta
    }

    /// Whether focus loss/reset occurred since the previous consumed tick.
    /// Retained through paused render frames so UI capture can be cancelled
    /// even if the window regains focus before simulation resumes.
    pub fn reset_pending(&self) -> bool {
        self.reset_pending
    }

    /// Clears transitions and pointer motion after one fixed update, preserving held actions.
    pub fn consume_edges(&mut self) {
        for state in &mut self.states {
            state.pressed = false;
            state.released = false;
        }
        self.pointer_delta = Vec2::ZERO;
        self.reset_pending = false;
    }

    /// Releases every held action, e.g. on focus loss. Releases are observable.
    pub fn release_all(&mut self) {
        self.reset_pending = true;
        for state in &mut self.states {
            state.released |= state.down;
            state.down = false;
        }
        self.pointer_delta = Vec2::ZERO;
        self.axes.fill(0.0);
    }
}

/// Read-only gameplay input with game-chosen action and pointer-motion masks.
#[derive(Clone, Copy, Debug)]
pub struct InputView<'a> {
    input: &'a Input,
    blocked: &'a [Action],
    blocked_axes: &'a [Axis],
    block_motion: bool,
}

impl<'a> InputView<'a> {
    /// Adds explicit analog masks to this gameplay view. Masked values are
    /// neutral; raw input stays readable by UI and held values resume unmasked.
    pub fn with_blocked_axes(mut self, blocked: &'a [Axis]) -> Self {
        self.blocked_axes = blocked;
        self
    }

    /// Latest analog value, or zero when this axis is masked.
    pub fn value(&self, axis: Axis) -> f32 {
        if self.blocked_axes.contains(&axis) {
            0.0
        } else {
            self.input.value(axis)
        }
    }
    /// Whether an unmasked action is held.
    pub fn down(&self, action: Action) -> bool {
        !self.blocked.contains(&action) && self.input.down(action)
    }

    /// Whether an unmasked action was pressed.
    pub fn pressed(&self, action: Action) -> bool {
        !self.blocked.contains(&action) && self.input.pressed(action)
    }

    /// Whether an unmasked action was released.
    pub fn released(&self, action: Action) -> bool {
        !self.blocked.contains(&action) && self.input.released(action)
    }

    /// Digital axis with masked actions contributing zero.
    pub fn axis(&self, negative: Action, positive: Action) -> f32 {
        f32::from(self.down(positive)) - f32::from(self.down(negative))
    }

    /// Relative look motion, or zero while the UI owns it.
    pub fn pointer_delta(&self) -> Vec2 {
        if self.block_motion {
            Vec2::ZERO
        } else {
            self.input.pointer_delta()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analog_values_persist_across_ticks_route_and_reset_to_neutral() {
        let movement = Axis(0);
        let look = Axis(1);
        let mut input = Input::with_capacities(1, 2);
        assert_eq!(input.value(Axis(99)), 0.0);
        input.set_axis(movement, 0.4);
        input.set_axis(look, -0.8);
        input.set(Action(0), true);
        input.add_pointer_delta(Vec2::ONE);
        let axes = [movement, look];
        let view = input.routed(&[Action(0)], true).with_blocked_axes(&axes);
        assert_eq!(view.value(movement), 0.0);
        assert_eq!(view.value(look), 0.0);
        assert!(!view.pressed(Action(0)));
        assert_eq!(input.value(movement), 0.4);
        input.consume_edges();
        for _ in 0..3 {
            assert_eq!(input.value(movement), 0.4);
            assert_eq!(input.routed(&[], false).value(look), -0.8);
            assert!(!input.pressed(Action(0)));
            assert_eq!(input.pointer_delta(), Vec2::ZERO);
            input.consume_edges();
        }
        input.set_axis(movement, -0.2); // A new render frame replaces the old value.
        assert_eq!(input.value(movement), -0.2);
        input.release_all();
        assert_eq!(input.value(movement), 0.0);
        assert_eq!(input.value(look), 0.0);
        assert!(input.released(Action(0)));
    }

    #[test]
    fn analog_samples_are_bounded_and_nonfinite_values_are_neutral() {
        let mut input = Input::default();
        for (raw, expected) in [
            (5.0, 1.0),
            (-5.0, -1.0),
            (f32::NAN, 0.0),
            (f32::INFINITY, 0.0),
            (f32::NEG_INFINITY, 0.0),
        ] {
            input.set_axis(Axis(2), raw);
            assert_eq!(input.value(Axis(2)), expected);
        }
    }

    #[test]
    fn transitions_survive_frames_but_only_fire_on_one_tick() {
        let mut input = Input::default();
        input.set(Action(2), true);
        input.set(Action(2), true); // Another render frame before a simulation tick.
        assert!(input.pressed(Action(2)));
        input.consume_edges();
        assert!(!input.pressed(Action(2)));
        assert!(input.down(Action(2)));
        input.set(Action(2), false);
        assert!(input.released(Action(2)));
    }

    #[test]
    fn quick_tap_keeps_both_edges() {
        let mut input = Input::default();
        input.set(Action(0), true);
        input.set(Action(0), false);
        assert!(input.pressed(Action(0)) && input.released(Action(0)));
        assert!(!input.down(Action(0)));
    }

    #[test]
    fn unknown_actions_and_focus_loss_are_safe() {
        let mut input = Input::default();
        assert_eq!(input.axis(Action(0), Action(1)), 0.0);
        input.set(Action(1), true);
        input.release_all();
        assert!(!input.down(Action(1)));
        assert!(input.released(Action(1)));
    }

    #[test]
    fn relative_motion_accumulates_until_one_tick_and_is_not_repeated_during_catch_up() {
        let mut input = Input::with_capacity(1);
        input.set(Action(0), true);
        input.add_pointer_delta(Vec2::new(4.0, -2.0));
        input.add_pointer_delta(Vec2::new(3.0, 1.0));
        assert_eq!(input.pointer_delta(), Vec2::new(7.0, -1.0));
        input.consume_edges();
        assert_eq!(input.pointer_delta(), Vec2::ZERO);
        assert!(input.down(Action(0)));
        input.consume_edges();
        assert_eq!(input.pointer_delta(), Vec2::ZERO);
        input.add_pointer_delta(Vec2::new(-2.0, 3.0));
        input.release_all();
        assert_eq!(input.pointer_delta(), Vec2::ZERO);
    }
}
