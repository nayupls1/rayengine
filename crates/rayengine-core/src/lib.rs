//! Display-independent primitives shared by 2D and 3D rayengine games.
//!
//! This crate deliberately has no raylib dependency. Simulations and tests can
//! run without a C compiler, window, display server, or graphics context.

pub use glam;
pub use hecs::{Entity, World};
