#![doc = include_str!("../README.md")]

pub use rayengine_core as core;
pub use raylib;

pub mod assets;
pub mod input;
pub mod material;
pub mod render;
pub mod runtime;

pub use runtime::{
    App, Config, CursorMode, Error, Game, InitContext, RunOptions, RunReport, Update,
};

/// Guides authored in Markdown and exported to HTML by rustdoc.
pub mod guides {
    #[doc = include_str!("../docs/quickstart.md")]
    pub mod quickstart {}
    #[doc = include_str!("../docs/game_structure.md")]
    pub mod game_structure {}
    #[doc = include_str!("../docs/responsive.md")]
    pub mod responsive {}
    #[doc = include_str!("../docs/timing_input.md")]
    pub mod timing_input {}
    #[doc = include_str!("../docs/assets.md")]
    pub mod assets {}
    #[doc = include_str!("../docs/generated_meshes.md")]
    pub mod generated_meshes {}
    #[doc = include_str!("../docs/materials.md")]
    pub mod materials {}
    #[doc = include_str!("../docs/testing_performance.md")]
    pub mod testing_performance {}
    #[doc = include_str!("../docs/agent_workflow.md")]
    pub mod agent_workflow {}
}

/// Common imports for a game using the prescribed lifecycle.
pub mod prelude {
    pub use crate::assets::{MaterialId, MeshId, ModelId, ShaderId, SoundId, TextureId};
    pub use crate::input::{Bindings, Button};
    pub use crate::material::{AlphaMode, MaterialDesc, MaterialParam, UniformId, UniformValue};
    pub use crate::render::Frame;
    pub use crate::{App, Config, CursorMode, Error, Game, InitContext, RunOptions, Update};
    pub use rayengine_core::prelude::*;
    pub use raylib::prelude::{Color, KeyboardKey};
}
