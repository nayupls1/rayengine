//! A small Rust game SDK built on raylib.
//!
//! Pure simulation primitives live in [`core`]. Raylib remains accessible for
//! features beyond the engine's prescribed lifecycle.

pub use rayengine_core as core;
pub use raylib;
