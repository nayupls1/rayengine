//! Small anchored UI primitives shared by both dimensions.

use crate::collision::Aabb2;
use glam::Vec2;

mod interaction;
pub use interaction::{
    UiActions, UiButton, UiCapture, UiId, UiInput, UiRegion, UiResponse, UiState,
};

/// A sized UI rectangle with normalized anchor and pivot coordinates.
#[derive(Clone, Copy, Debug)]
pub struct UiRect {
    /// Point on the viewport: `(0,0)` is top left, `(1,1)` bottom right.
    pub anchor: Vec2,
    /// Point on this rectangle that sits at the anchor.
    pub pivot: Vec2,
    /// Offset from the anchor in UI units.
    pub offset: Vec2,
    /// Rectangle dimensions in UI units.
    pub size: Vec2,
}

impl UiRect {
    /// Top-left anchored rectangle.
    pub fn top_left(offset: Vec2, size: Vec2) -> Self {
        Self {
            anchor: Vec2::ZERO,
            pivot: Vec2::ZERO,
            offset,
            size,
        }
    }

    /// Bottom-right anchored rectangle with matching pivot.
    pub fn bottom_right(offset: Vec2, size: Vec2) -> Self {
        Self {
            anchor: Vec2::ONE,
            pivot: Vec2::ONE,
            offset,
            size,
        }
    }

    /// Resolves the rectangle against the viewport's logical size.
    pub fn resolve(&self, logical_size: Vec2) -> Aabb2 {
        let min = logical_size * self.anchor + self.offset - self.size * self.pivot;
        Aabb2 {
            min,
            max: min + self.size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchored_ui_preserves_margin_as_view_expands() {
        let rect = UiRect::bottom_right(Vec2::splat(-16.0), Vec2::new(200.0, 40.0));
        for size in [Vec2::new(960.0, 540.0), Vec2::new(1400.0, 540.0)] {
            assert_eq!(rect.resolve(size).max, size - Vec2::splat(16.0));
        }
    }
}
