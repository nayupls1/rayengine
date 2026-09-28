//! Action input whose edges survive render frames and are consumed per tick.

/// Small numeric action key, normally declared as a game-level constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Action(pub u16);

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
}

impl Input {
    /// Preallocates action slots to avoid allocation during input sampling.
    pub fn with_capacity(actions: usize) -> Self {
        Self {
            states: vec![State::default(); actions],
        }
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

    /// Clears transitions after one fixed update, preserving held actions.
    pub fn consume_edges(&mut self) {
        for state in &mut self.states {
            state.pressed = false;
            state.released = false;
        }
    }

    /// Releases every held action, e.g. on focus loss. Releases are observable.
    pub fn release_all(&mut self) {
        for state in &mut self.states {
            state.released |= state.down;
            state.down = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
