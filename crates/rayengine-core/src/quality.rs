//! CPU-only render quality validation and allocation planning.
use crate::viewport::{ScaleMode, Viewport};
use glam::Vec2;
use std::fmt;

/// Anti-aliasing applied to the game's offscreen world before UI composition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AntiAliasing {
    /// No edge filter (supersampling can still be selected independently).
    #[default]
    None,
    /// Fast approximate anti-aliasing, a luminance-directed offscreen edge filter.
    Fxaa,
}

/// Rendering controls, independent of camera and UI coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderQuality {
    /// Internal world pixels per native content pixel in each dimension: exactly 1 or 2.
    /// 2 means four times the world pixel count, resolved with bilinear filtering.
    pub render_scale: f32,
    /// World edge filter. UI is composed afterwards at native resolution.
    pub anti_aliasing: AntiAliasing,
}
impl Default for RenderQuality {
    fn default() -> Self {
        Self {
            render_scale: 1.0,
            anti_aliasing: AntiAliasing::None,
        }
    }
}

/// Invalid settings or a render allocation exceeding the SDK's bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityError(pub String);
impl fmt::Display for QualityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for QualityError {}

/// Checked target dimensions and steady-state color/depth allocation estimate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderPlan {
    /// World target dimensions (reference-sized in IntegerFit).
    pub world: (u32, u32),
    /// Native composition target dimensions, or world dimensions for pixel art.
    pub output: (u32, u32),
    /// Whether separate native UI and resolved output targets are required.
    pub separate_ui: bool,
    /// Conservative allocation estimate: 4-byte RGBA plus 4-byte depth per target pixel.
    pub target_bytes: u64,
}
impl RenderQuality {
    /// Maximum width or height of any target; the device limit may be lower.
    pub const MAX_DIMENSION: u32 = 8192;
    /// Maximum estimated total allocation, 512 MiB; resize drops old targets first.
    pub const MAX_TARGET_BYTES: u64 = 512 * 1024 * 1024;

    /// Checks supported quality/pixel-art combinations without opening a window.
    pub fn validate(self, mode: ScaleMode) -> Result<(), QualityError> {
        if !self.render_scale.is_finite() || ![1.0, 2.0].contains(&self.render_scale) {
            return Err(QualityError(
                "render_scale must be 1 (native) or 2 (supersampling)".into(),
            ));
        }
        if mode == ScaleMode::IntegerFit && self != Self::default() {
            return Err(QualityError(
                "IntegerFit requires render_scale = 1 and anti_aliasing = none".into(),
            ));
        }
        Ok(())
    }

    /// Plans targets from current logical viewport and physical framebuffer DPI.
    /// Oversized requests fail explicitly rather than silently lowering quality.
    pub fn plan(
        self,
        view: &Viewport,
        dpi: Vec2,
        mode: ScaleMode,
    ) -> Result<RenderPlan, QualityError> {
        self.validate(mode)?;
        if !dpi.is_finite() || dpi.min_element() <= 0.0 {
            return Err(QualityError(
                "framebuffer DPI must be finite and positive".into(),
            ));
        }
        let physical = if mode == ScaleMode::IntegerFit {
            view.logical_size
        } else {
            view.size * dpi
        };
        let dimensions = |size: Vec2| -> Result<(u32, u32), QualityError> {
            if !size.is_finite()
                || size.min_element() <= 0.0
                || size.round().max_element() > Self::MAX_DIMENSION as f32
            {
                return Err(QualityError(
                    "render target dimensions exceed 8192 or are invalid".into(),
                ));
            }
            Ok((
                size.x.round().max(1.0) as u32,
                size.y.round().max(1.0) as u32,
            ))
        };
        let output = dimensions(physical)?;
        // Scale the rounded native dimensions so 2x always means exactly twice each dimension.
        let world = dimensions(Vec2::new(output.0 as f32, output.1 as f32) * self.render_scale)?;
        let separate_ui = self != Self::default();
        let pixels = |size: (u32, u32)| u64::from(size.0) * u64::from(size.1);
        let target_bytes = 8 * (pixels(world) + if separate_ui { 2 * pixels(output) } else { 0 });
        if target_bytes > Self::MAX_TARGET_BYTES {
            return Err(QualityError(format!(
                "render targets need {target_bytes} bytes; limit is {}",
                Self::MAX_TARGET_BYTES
            )));
        }
        Ok(RenderPlan {
            world,
            output,
            separate_ui,
            target_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera2D;
    #[test]
    fn invalid_and_pixel_art_combinations_fail() {
        for scale in [0.0, 0.5, 1.5, 2.1, f32::NAN, f32::INFINITY] {
            assert!(
                RenderQuality {
                    render_scale: scale,
                    ..Default::default()
                }
                .validate(ScaleMode::Fit)
                .is_err()
            );
        }
        for quality in [
            RenderQuality {
                render_scale: 2.0,
                ..Default::default()
            },
            RenderQuality {
                anti_aliasing: AntiAliasing::Fxaa,
                ..Default::default()
            },
        ] {
            assert!(quality.validate(ScaleMode::IntegerFit).is_err());
        }
    }
    #[test]
    fn dpi_resize_quality_preserve_layout_camera_and_picking() {
        for mode in [ScaleMode::Fit, ScaleMode::Expand, ScaleMode::IntegerFit] {
            for window in [
                Vec2::new(1280.0, 720.0),
                Vec2::new(800.0, 1000.0),
                Vec2::new(160.0, 90.0),
            ] {
                let view = Viewport::new(window, Vec2::new(960.0, 540.0), mode).unwrap();
                let ui = view.logical_size * 0.4;
                let screen = view.ui_to_screen(ui);
                let camera = Camera2D::default();
                let before = camera.screen_to_world(screen, &view).unwrap();
                for dpi in [Vec2::ONE, Vec2::splat(2.0), Vec2::splat(1.25)] {
                    let native = RenderQuality::default().plan(&view, dpi, mode).unwrap();
                    if mode == ScaleMode::IntegerFit {
                        assert_eq!(native.world, (960, 540));
                        continue;
                    }
                    for aa in [AntiAliasing::None, AntiAliasing::Fxaa] {
                        let plan = RenderQuality {
                            render_scale: 2.0,
                            anti_aliasing: aa,
                        }
                        .plan(&view, dpi, mode)
                        .unwrap();
                        assert_eq!(plan.output, native.world);
                        assert_eq!(plan.world, (native.world.0 * 2, native.world.1 * 2));
                        assert_eq!(plan.target_bytes, native.target_bytes * 6);
                        assert!(view.screen_to_ui(screen).unwrap().abs_diff_eq(ui, 0.001));
                        assert_eq!(camera.screen_to_world(screen, &view).unwrap(), before);
                    }
                }
            }
        }
    }
    #[test]
    fn excessive_allocations_and_bad_dpi_fail_without_clamping() {
        let large = Viewport::new(Vec2::new(8192.0, 8192.0), Vec2::ONE, ScaleMode::Fit).unwrap();
        assert_eq!(
            RenderQuality::default()
                .plan(&large, Vec2::ONE, ScaleMode::Fit)
                .unwrap()
                .target_bytes,
            RenderQuality::MAX_TARGET_BYTES
        );
        let high = RenderQuality {
            render_scale: 2.0,
            ..Default::default()
        };
        assert!(high.plan(&large, Vec2::ONE, ScaleMode::Fit).is_err());
        let budget = Viewport::new(Vec2::splat(4096.0), Vec2::ONE, ScaleMode::Fit).unwrap();
        assert!(
            high.plan(&budget, Vec2::ONE, ScaleMode::Fit)
                .unwrap_err()
                .0
                .contains("bytes")
        );
        for dpi in [Vec2::ZERO, Vec2::splat(f32::NAN), Vec2::splat(3.0)] {
            assert!(
                RenderQuality::default()
                    .plan(&large, dpi, ScaleMode::Fit)
                    .is_err()
            );
        }
    }
}
