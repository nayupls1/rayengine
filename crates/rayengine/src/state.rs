//! Optional game-owned state composition. See [`crate::guides::states`].

use crate::{
    assets::{Assets, MaterialId, MeshId, ModelId, ShaderId, SoundId, TextureId},
    input::Bindings,
    render::Frame,
    runtime::{CursorMode, Error, Game, InitContext, Update},
};
use rayengine_core::input::Input;

/// Independent propagation choices for a state and those underneath it.
/// The default is an opaque, modal state. Input only propagates to states that
/// also receive updates; blocked input includes held actions, edges, analog axes and pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatePolicy {
    /// Continue fixed updates down the stack (visited top to bottom).
    pub update_below: bool,
    /// Draw underlying states first, followed by this state.
    pub draw_below: bool,
    /// Allow lower updated states to see the original input and UI pointer.
    pub input_below: bool,
}

/// A scene, menu, or overlay using the SDK's existing contexts.
/// Covered states remain entered. Only removal calls `exit`; revealing a state
/// does not enter it again. Rust fields drop after exit and resource release.
pub trait State {
    /// Routing policy, sampled once at the start of each traversal.
    fn policy(&self) -> StatePolicy {
        StatePolicy::default()
    }
    /// Cursor policy of the top state.
    fn cursor_mode(&self) -> CursorMode {
        CursorMode::Free
    }
    /// Initialize before insertion. Register exclusively owned handles as soon
    /// as they are created. On error, `exit` and resource cleanup still run.
    fn enter(
        &mut self,
        _context: &mut InitContext<'_, '_>,
        _resources: &mut StateResources,
    ) -> Result<(), Error> {
        Ok(())
    }
    /// Teardown on removal, failed entry, or normal/error runner shutdown.
    /// Must tolerate partially completed entry. Called before registered handles
    /// unload; do not unload these handles yourself or unload shared assets.
    fn exit(&mut self, _context: &mut InitContext<'_, '_>) {}
    /// Advance simulation or UI; requesting a transition stops lower updates
    /// on this tick. The transition runs after the whole callback returns.
    fn fixed_update(&mut self, _context: &mut Update<'_, '_>, _commands: &mut StateCommands) {}
    /// Draw without changing simulation. Requests run after all selected draws,
    /// outside render passes. A later request is rejected while one is pending.
    fn draw(&mut self, frame: &mut Frame<'_, '_>, commands: &mut StateCommands);
}

/// One explicit change. New states enter before existing states exit, so failed
/// initialization preserves the old stack. `Reset` is useful for return-to-title.
pub enum Transition {
    /// Enter and cover the existing top state.
    Push(Box<dyn State>),
    /// Exit the top state. An empty stack is a valid no-op.
    Pop,
    /// Enter a new top, then exit the old top (pushes if empty).
    Replace(Box<dyn State>),
    /// Enter a new root, then exit all old states from top to bottom.
    Reset(Box<dyn State>),
    /// Exit all states from top to bottom, leaving an empty usable stack.
    Clear,
}

/// A single pending transition. No callback can mutate the live stack.
#[derive(Default)]
pub struct StateCommands {
    pending: Option<Transition>,
}

impl StateCommands {
    /// Queue a transition, or return it unchanged if one is already pending.
    /// The first accepted request wins at each lifecycle boundary.
    pub fn request(&mut self, transition: Transition) -> Result<(), Transition> {
        if self.pending.is_some() {
            Err(transition)
        } else {
            self.pending = Some(transition);
            Ok(())
        }
    }

    /// Whether a transition is waiting for a safe lifecycle boundary.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// Explicitly transferred, exclusively state-owned asset handles.
/// Registration does not clone assets or establish reference counts. Never
/// register borrowed/shared handles or path-cached assets used by other states.
/// Shared assets belong to the game run and are untouched by this collection.
#[derive(Default)]
pub struct StateResources {
    materials: Vec<MaterialId>,
    meshes: Vec<MeshId>,
    models: Vec<ModelId>,
    textures: Vec<TextureId>,
    sounds: Vec<SoundId>,
    shaders: Vec<ShaderId>,
}

macro_rules! own_asset {
    ($method:ident, $field:ident, $ty:ty, $doc:literal) => {
        #[doc = $doc]
        pub fn $method(&mut self, id: $ty) -> $ty {
            if !self.$field.contains(&id) {
                self.$field.push(id);
            }
            id
        }
    };
}

impl StateResources {
    own_asset!(
        own_material,
        materials,
        MaterialId,
        "Transfer an exclusive material handle; returns it for storage in the state."
    );
    own_asset!(
        own_mesh,
        meshes,
        MeshId,
        "Transfer an exclusive generated mesh handle; returns it for storage in the state."
    );
    own_asset!(
        own_model,
        models,
        ModelId,
        "Transfer an exclusive model handle; returns it for storage in the state."
    );
    own_asset!(
        own_texture,
        textures,
        TextureId,
        "Transfer an exclusive texture handle; returns it for storage in the state."
    );
    own_asset!(
        own_sound,
        sounds,
        SoundId,
        "Transfer an exclusive sound handle; returns it for storage in the state."
    );
    own_asset!(
        own_shader,
        shaders,
        ShaderId,
        "Transfer an exclusive shader handle; returns it for storage in the state."
    );

    fn release(&mut self, assets: &mut Assets<'_>) {
        for id in self.materials.drain(..).rev() {
            assets.unload_material(id);
        }
        for id in self.meshes.drain(..).rev() {
            assets.unload_mesh(id);
        }
        for id in self.models.drain(..).rev() {
            assets.unload_model(id);
        }
        for id in self.textures.drain(..).rev() {
            assets.unload_texture(id);
        }
        for id in self.sounds.drain(..).rev() {
            assets.unload_sound(id);
        }
        for id in self.shaders.drain(..).rev() {
            assets.unload_shader(id);
        }
    }
}

struct Entry {
    state: Box<dyn State>,
    resources: StateResources,
}

impl Entry {
    fn enter(mut state: Box<dyn State>, context: &mut InitContext<'_, '_>) -> Result<Self, Error> {
        let mut resources = StateResources::default();
        if let Err(error) = state.enter(context, &mut resources) {
            state.exit(context);
            resources.release(context.assets);
            return Err(error);
        }
        Ok(Self { state, resources })
    }

    fn exit(mut self, context: &mut InitContext<'_, '_>) {
        self.state.exit(context);
        self.resources.release(context.assets);
    }
}

/// Optional [`Game`] adapter owning a stack, bindings, and deferred transitions.
/// Can also be composed inside an existing game by forwarding the lifecycle
/// methods, including `boundary` and `shutdown`. See [`crate::guides::states`].
#[derive(Default)]
pub struct StateStack {
    entries: Vec<Entry>,
    bindings: Bindings,
    commands: StateCommands,
    last_error: Option<Error>,
    blocked_input: Input,
    suppress_input: bool,
}

impl StateStack {
    /// Queue an initial root for `Game::init`; no backend work occurs here.
    pub fn new(initial: impl State + 'static, bindings: Bindings) -> Self {
        Self {
            bindings,
            commands: StateCommands {
                pending: Some(Transition::Push(Box::new(initial))),
            },
            ..Self::default()
        }
    }

    /// Number of successfully entered states.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no entered states. Empty stacks neither update nor draw.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Request a change from the game owner, preserving an earlier request.
    pub fn request(&mut self, transition: Transition) -> Result<(), Transition> {
        self.commands.request(transition)
    }

    /// Take the latest recoverable entry error recorded by `Game::boundary`.
    /// Initial entry errors instead propagate from `Game::init` to `App::run`.
    pub fn take_error(&mut self) -> Option<Error> {
        self.last_error.take()
    }

    /// Apply one queued change outside all update/draw callbacks and passes.
    /// Failure consumes the request, cleans the attempted state, and preserves
    /// all previous states. Calling manually lets a composing game handle errors.
    pub fn apply(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let Some(transition) = self.commands.pending.take() else {
            return Ok(());
        };
        self.suppress_input = true;
        match transition {
            Transition::Push(state) => self.entries.push(Entry::enter(state, context)?),
            Transition::Replace(state) => {
                let entry = Entry::enter(state, context)?;
                if let Some(old) = self.entries.pop() {
                    old.exit(context);
                }
                self.entries.push(entry);
            }
            Transition::Reset(state) => {
                let entry = Entry::enter(state, context)?;
                self.clear(context);
                self.entries.push(entry);
            }
            Transition::Pop => {
                if let Some(old) = self.entries.pop() {
                    old.exit(context);
                }
            }
            Transition::Clear => self.clear(context),
        }
        Ok(())
    }

    fn clear(&mut self, context: &mut InitContext<'_, '_>) {
        while let Some(entry) = self.entries.pop() {
            entry.exit(context);
        }
    }

    fn policies(&self) -> Vec<StatePolicy> {
        self.entries.iter().map(|e| e.state.policy()).collect()
    }
}

// Snapshot both reachability and routing before calling any state code.
fn update_route(policies: &[StatePolicy]) -> Vec<(usize, bool)> {
    let mut route = Vec::new();
    let mut input = true;
    for (index, policy) in policies.iter().enumerate().rev() {
        route.push((index, input));
        if !policy.update_below {
            break;
        }
        input &= policy.input_below;
    }
    route
}

fn draw_start(policies: &[StatePolicy]) -> usize {
    policies.iter().rposition(|p| !p.draw_below).unwrap_or(0)
}

impl Game for StateStack {
    fn bindings(&self) -> Bindings {
        self.bindings.clone()
    }

    fn cursor_mode(&self) -> CursorMode {
        self.entries
            .last()
            .map_or(CursorMode::Free, |e| e.state.cursor_mode())
    }

    fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.apply(context)
    }

    fn boundary(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        if let Err(error) = self.apply(context) {
            self.last_error = Some(error);
        }
        Ok(())
    }

    fn shutdown(&mut self, context: &mut InitContext<'_, '_>) {
        self.commands.pending = None;
        self.clear(context);
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        let suppress_input = std::mem::take(&mut self.suppress_input);
        for (index, input) in update_route(&self.policies()) {
            let input = input && !suppress_input;
            if self.commands.is_pending() {
                break;
            }
            // A blocked state still simulates, but sees neutral buttons/axes and no pointer.
            // Preserve focus/reset signals so UI can cancel old captures.
            self.blocked_input.consume_edges();
            if context.input.reset_pending() {
                self.blocked_input.release_all();
            }
            let mut routed = Update {
                input: if input {
                    context.input
                } else {
                    &self.blocked_input
                },
                bindings: &mut *context.bindings,
                pointer: if input { context.pointer } else { None },
                tick: context.tick,
                viewport: context.viewport,
                window_focused: context.window_focused,
                assets: context.assets,
                quit: &mut *context.quit,
            };
            self.entries[index]
                .state
                .fixed_update(&mut routed, &mut self.commands);
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let start = draw_start(&self.policies());
        for entry in &mut self.entries[start..] {
            entry.state.draw(frame, &mut self.commands);
        }
    }
}

#[cfg(test)]
mod tests;
