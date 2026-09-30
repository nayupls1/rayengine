//! Optional rayengine plugin. The consuming game owns and invokes this instance.
use rayengine::prelude::*;

/// Explicitly shared state; replace this with your own types or a borrowed view.
#[derive(Default)]
pub struct PluginState {
    /// Number of fixed updates requested by the game.
    pub ticks: u64,
}

/// A minimal plugin. Add typed configuration and owned resources as needed.
#[derive(Default)]
pub struct MyPlugin;

impl Plugin<PluginState> for MyPlugin {
    fn fixed_update(&mut self, state: &mut PluginState, _context: &mut Update<'_, '_>) {
        state.ticks = state.ticks.saturating_add(1);
    }
}
