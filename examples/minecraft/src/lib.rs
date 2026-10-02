#![doc = include_str!("../README.md")]
/// Release and bundle guides exported with the offline reference.
pub mod guides {
    #[doc = include_str!("../../../docs/minecraft_release.md")]
    pub mod release {}
    #[doc = include_str!("../RELEASE.md")]
    pub mod controls {}
}
pub mod breaking;
pub mod gameplay;
pub mod hud;
pub mod persistence;
#[cfg(feature = "render")]
pub mod preview;
pub mod survival;
pub mod terrain;
pub mod textures;
