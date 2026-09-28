//! One viewport contract for cameras, layout, letterboxing and pointer mapping.

use glam::Vec2;

/// Policy for adapting a reference view to a resizable window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScaleMode {
    /// Preserve the reference aspect ratio and visible area, adding bars.
    #[default]
    Fit,
    /// Fit at whole-number scales for pixel art; fractionally shrink if necessary.
    IntegerFit,
    /// Fill the window while preserving reference height; width changes with aspect.
    Expand,
}

/// A fitted content rectangle measured in logical window coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    /// Top-left corner of the content, excluding letterbox bars.
    pub origin: Vec2,
    /// Content dimensions in logical window coordinates.
    pub size: Vec2,
    /// Content dimensions in game/UI units.
    pub logical_size: Vec2,
    /// Window logical pixels per UI unit.
    pub scale: f32,
}

impl Viewport {
    /// Computes a viewport. Returns `None` for minimized or invalid dimensions.
    pub fn new(window: Vec2, reference: Vec2, mode: ScaleMode) -> Option<Self> {
        if !window.is_finite()
            || !reference.is_finite()
            || window.min_element() <= 0.0
            || reference.min_element() <= 0.0
        {
            return None;
        }
        let fit = (window / reference).min_element();
        let scale = match mode {
            ScaleMode::Fit => fit,
            ScaleMode::IntegerFit if fit >= 1.0 => fit.floor(),
            ScaleMode::IntegerFit => fit,
            ScaleMode::Expand => window.y / reference.y,
        };
        let logical_size = if mode == ScaleMode::Expand {
            window / scale
        } else {
            reference
        };
        let size = logical_size * scale;
        Some(Self {
            origin: (window - size) * 0.5,
            size,
            logical_size,
            scale,
        })
    }

    /// Aspect ratio seen by the camera and renderer.
    pub fn aspect(&self) -> f32 {
        self.logical_size.x / self.logical_size.y
    }

    /// Converts a pointer to UI units, rejecting points inside the bars.
    pub fn screen_to_ui(&self, screen: Vec2) -> Option<Vec2> {
        let local = screen - self.origin;
        if !local.is_finite()
            || local.min_element() < 0.0
            || local.x >= self.size.x
            || local.y >= self.size.y
        {
            None
        } else {
            Some(local / self.scale)
        }
    }

    /// Converts UI units to logical window coordinates.
    pub fn ui_to_screen(&self, ui: Vec2) -> Vec2 {
        self.origin + ui * self.scale
    }

    /// Target pixel dimensions at the given framebuffer scale, rounded and bounded.
    pub fn render_size(&self, dpi: Vec2) -> (u32, u32) {
        let physical = self.size * dpi.max(Vec2::ONE);
        // Preserve aspect when limiting very large/high-DPI targets.
        let physical = physical * (8192.0 / physical.max_element()).min(1.0);
        (
            physical.x.round().clamp(1.0, 8192.0) as u32,
            physical.y.round().clamp(1.0, 8192.0) as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ultrawide_fit_keeps_view_and_pointer_coordinates() {
        let view = Viewport::new(
            Vec2::new(2400.0, 900.0),
            Vec2::new(960.0, 540.0),
            ScaleMode::Fit,
        )
        .unwrap();
        assert_eq!(view.origin, Vec2::new(400.0, 0.0));
        assert_eq!(view.logical_size, Vec2::new(960.0, 540.0));
        assert!(view.screen_to_ui(Vec2::new(100.0, 100.0)).is_none());
        let ui = Vec2::new(300.0, 120.0);
        assert!(
            view.screen_to_ui(view.ui_to_screen(ui))
                .unwrap()
                .abs_diff_eq(ui, 0.001)
        );
        assert_eq!(view.render_size(Vec2::splat(2.0)), (3200, 1800));
    }

    #[test]
    fn integer_scaling_fits_even_a_small_window() {
        let reference = Vec2::new(320.0, 180.0);
        let large =
            Viewport::new(Vec2::new(1000.0, 700.0), reference, ScaleMode::IntegerFit).unwrap();
        assert_eq!(large.scale, 3.0);
        let small =
            Viewport::new(Vec2::new(160.0, 90.0), reference, ScaleMode::IntegerFit).unwrap();
        assert_eq!(small.scale, 0.5);
    }

    #[test]
    fn expand_changes_only_visible_width() {
        let view = Viewport::new(
            Vec2::new(2000.0, 1000.0),
            Vec2::new(960.0, 540.0),
            ScaleMode::Expand,
        )
        .unwrap();
        assert_eq!(view.logical_size, Vec2::new(1080.0, 540.0));
        assert_eq!(view.origin, Vec2::ZERO);
    }

    #[test]
    fn minimized_and_nonfinite_sizes_are_rejected() {
        assert!(Viewport::new(Vec2::ZERO, Vec2::ONE, ScaleMode::Fit).is_none());
        assert!(Viewport::new(Vec2::splat(f32::NAN), Vec2::ONE, ScaleMode::Fit).is_none());
    }

    #[test]
    fn target_limit_preserves_camera_aspect_ratio() {
        let view = Viewport::new(
            Vec2::new(16000.0, 9000.0),
            Vec2::new(960.0, 540.0),
            ScaleMode::Fit,
        )
        .unwrap();
        assert_eq!(view.render_size(Vec2::ONE), (8192, 4608));
    }
}
