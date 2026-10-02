#![doc = include_str!("../README.md")]

pub use rayengine_core as core;
pub use rayengine_core::first_person;
pub use rayengine_core::manifest;
pub use rayengine_core::save;
pub use rayengine_core::sprite;
pub use raylib;

pub mod assets;
pub mod diagnostics;
pub mod input;
pub mod lighting;
pub mod material;
pub mod plugin;
pub mod render;
pub mod runtime;
pub mod state;
pub mod upload;

pub use plugin::Plugin;
pub use runtime::{
    App, Config, CursorMode, Error, Game, InitContext, RunOptions, RunReport, Update,
};

/// Guides authored in Markdown and exported to HTML by rustdoc.
pub mod guides {
    #[doc = include_str!("../docs/quickstart.md")]
    pub mod quickstart {}
    #[doc = include_str!("../docs/game_structure.md")]
    pub mod game_structure {}
    #[doc = include_str!("../docs/states.md")]
    #[doc = "\n\n```no_run"]
    #[doc = include_str!("../examples/states.rs")]
    #[doc = "```"]
    pub mod states {}
    #[doc = include_str!("../docs/plugins.md")]
    pub mod plugins {}
    #[doc = include_str!("../docs/responsive.md")]
    pub mod responsive {}
    #[doc = include_str!("../docs/timing_input.md")]
    pub mod timing_input {}
    #[doc = include_str!("../docs/first_person.md")]
    #[doc = "\n\n```no_run"]
    #[doc = include_str!("../examples/first_person.rs")]
    #[doc = "```"]
    pub mod first_person {}
    #[doc = include_str!("../docs/assets.md")]
    pub mod assets {}
    #[doc = include_str!("../docs/project_manifest.md")]
    pub mod project_manifest {}
    #[doc = include_str!("../docs/sprites.md")]
    #[doc = "\n\n```no_run"]
    #[doc = include_str!("../examples/sprites.rs")]
    #[doc = "```"]
    pub mod sprites {}
    #[doc = include_str!("../docs/generated_meshes.md")]
    pub mod generated_meshes {}
    #[doc = include_str!("../docs/materials.md")]
    pub mod materials {}
    #[doc = include_str!("../docs/lighting.md")]
    #[doc = "\n\n```no_run"]
    #[doc = include_str!("../examples/lighting.rs")]
    #[doc = "```"]
    pub mod lighting {}
    #[doc = include_str!("../docs/spatial_queries.md")]
    pub mod spatial_queries {}
    #[doc = include_str!("../docs/background_work.md")]
    pub mod background_work {}
    #[doc = include_str!("../docs/interactive_ui.md")]
    #[doc = "\n\n```no_run"]
    #[doc = include_str!("../examples/menu.rs")]
    #[doc = "```"]
    pub mod interactive_ui {}
    #[doc = include_str!("../docs/saves.md")]
    #[doc = "\n\n```no_run"]
    #[doc = include_str!("../examples/save.rs")]
    #[doc = "```"]
    pub mod saves {}
    #[doc = include_str!("../docs/diagnostics.md")]
    pub mod diagnostics {}
    #[doc = include_str!("../docs/testing_performance.md")]
    pub mod testing_performance {}
    #[doc = include_str!("../docs/agent_workflow.md")]
    pub mod agent_workflow {}
}

/// Common imports for a game using the prescribed lifecycle.
pub mod prelude {
    pub use crate::assets::{MaterialId, MeshId, ModelId, ShaderId, SoundId, TextureId};
    pub use crate::input::{Bindings, Button};
    pub use crate::lighting::{DirectionalLight, Lighting, MAX_POINT_LIGHTS, PointLight};
    pub use crate::material::{
        AlphaMode, MaterialDesc, MaterialParam, Shading, UniformId, UniformValue,
    };
    pub use crate::render::{Frame, UiButtonStyle};
    pub use crate::state::{
        State, StateCommands, StatePolicy, StateResources, StateStack, Transition,
    };
    pub use crate::upload::{
        MeshUpload, MeshUploadOutcome, MeshUploadQueue, MeshUploadResult, MeshUploadTarget,
        UploadBudget, UploadReport,
    };
    pub use crate::{
        App, Config, CursorMode, Error, Game, InitContext, Plugin, RunOptions, Update,
    };
    pub use rayengine_core::prelude::*;
    pub use raylib::prelude::{Color, KeyboardKey};
}
