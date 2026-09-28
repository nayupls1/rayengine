//! Display-independent primitives shared by 2D and 3D rayengine games.
//!
//! This crate deliberately has no raylib dependency. Simulations and tests can
//! run without a C compiler, window, display server, or graphics context.

pub use glam;
pub use hecs::{Bundle, Entity, World};

pub mod camera;
pub mod collision;
pub mod input;
pub mod scene;
pub mod time;
pub mod transform;
pub mod ui;
pub mod viewport;

/// Common imports for display-independent game code.
pub mod prelude {
    pub use crate::camera::{Camera2D, Camera3D};
    pub use crate::collision::{Aabb2, Aabb3, Body2D, Body3D};
    pub use crate::input::{Action, Input};
    pub use crate::scene::Scene;
    pub use crate::time::{FixedClock, Tick};
    pub use crate::transform::{
        GlobalTransform2D, GlobalTransform3D, Parent, Transform2D, Transform3D,
    };
    pub use crate::ui::UiRect;
    pub use crate::viewport::{ScaleMode, Viewport};
    pub use glam::{Quat, Vec2, Vec3};
    pub use hecs::{Entity, World};
}
