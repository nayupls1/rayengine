//! Optional, statically typed extensions composed by a game.
//!
//! A plugin is an ordinary Cargo dependency. The game owns its instances and
//! chooses when to call each hook, passing the same public contexts it receives
//! from [`crate::Game`]. There is no registry, scheduler, implicit dispatch, or
//! dependency from the engine to plugin crates. See [`crate::guides::plugins`].

use crate::{Error, InitContext, Update, assets::Assets, render::Frame};

/// Opt-in lifecycle hooks over explicitly shared, game-owned state.
///
/// `State` can be a scene, a game-defined struct, a borrowed view, or `()` for an
/// independent plugin. Hooks have defaults so a plugin implements only what it
/// needs. Games may also call plugin-specific methods directly, for example to
/// draw several plugins inside one world pass or supply routed input.
///
/// The engine does **not** invoke these hooks. Call them from your [`crate::Game`]
/// implementation in the desired order. Propagate initialization errors with
/// `?`; the engine then drops the game before assets and the graphics context.
/// Partial initialization is not automatically rolled back when a game catches
/// the error and continues. Each plugin must document that recovery policy.
///
/// CPU data and jobs can live in plugin fields. Keep SDK contexts borrowed for
/// the duration of the call; native GPU work belongs to init/draw/unload on the
/// owning thread. Asset IDs borrow ownership from the run's asset collection:
/// dropping an ID does not unload its resource. Do not unload shared resources
/// without an explicit ownership agreement.
pub trait Plugin<State: ?Sized = ()> {
    /// Initializes this instance before its first update/draw.
    ///
    /// The game chooses configuration, bindings, dependencies, and init order.
    /// Whether repeated initialization is allowed is plugin-specific.
    fn init(
        &mut self,
        _state: &mut State,
        _context: &mut InitContext<'_, '_>,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Advances simulation once with the game's fixed-update context.
    ///
    /// Input edges/motion are consumed by the runner after the entire game
    /// callback, so multiple plugins see the same input unless the game supplies
    /// an explicitly routed view through their own APIs. The game allocates
    /// action IDs and controls pause/input policies. Do not block on jobs here.
    fn fixed_update(&mut self, _state: &mut State, _context: &mut Update<'_, '_>) {}

    /// Draws using immutable shared simulation state and `frame.alpha`.
    ///
    /// Mutable plugin fields may hold presentation caches or pending uploads.
    /// The game controls pass order and background clearing. Report recoverable
    /// update/draw failures through plugin-specific results, events, or status;
    /// these hooks match the engine's infallible update/draw callbacks.
    fn draw(&mut self, _state: &State, _frame: &mut Frame<'_, '_>) {}

    /// Explicitly removes an instance's state and exclusively owned assets.
    ///
    /// Optional: the game calls this when detaching a plugin on the owning
    /// thread, using assets available during init or drawing. The engine does
    /// not call it at exit or after failed initialization. Normal exit/error
    /// drops plugin fields with the game, then drops all remaining SDK assets.
    /// Use `Drop` for CPU cleanup and owned job-pool shutdown; use this hook for
    /// early asset removal. Plugins must document reuse and failure semantics.
    fn unload(&mut self, _state: &mut State, _assets: &mut Assets<'_>) {}
}
