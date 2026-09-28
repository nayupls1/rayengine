//! A small Rust game SDK built on raylib.
//!
//! Pure simulation primitives live in [`core`]. Raylib remains accessible for
//! features beyond the engine's prescribed lifecycle.

pub use rayengine_core as core;
pub use raylib;

pub mod assets;
pub mod input;
pub mod render;
pub mod runtime;

pub use runtime::{App, Config, Error, Game, InitContext, RunOptions, RunReport, Update};

/// Common imports for a game using the prescribed lifecycle.
pub mod prelude {
    pub use crate::assets::{ModelId, SoundId, TextureId};
    pub use crate::input::{Bindings, Button};
    pub use crate::render::Frame;
    pub use crate::{App, Config, Error, Game, InitContext, RunOptions, Update};
    pub use rayengine_core::prelude::*;
    pub use raylib::prelude::{Color, KeyboardKey};
}
