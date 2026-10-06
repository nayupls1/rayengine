//! Renderer-independent catalog layout, clipping and retained scroll offsets.
use crate::collision::Aabb2;
use glam::Vec2;

/// Half-open clipping rectangle: minimum edges are included, maximum edges excluded.
/// Empty or invalid bounds clip everything. Intersect nested clips before assigning
/// the same value to drawing and [`super::UiRegion::clip`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiClip {
    bounds: Aabb2,
}

impl UiClip {
    /// Creates a clip; nonfinite or inverted bounds become empty.
    pub fn new(bounds: Aabb2) -> Self {
        let bounds = if bounds.min.is_finite()
            && bounds.max.is_finite()
            && bounds.max.cmpgt(bounds.min).all()
        {
            bounds
        } else {
            Aabb2 {
                min: Vec2::ZERO,
                max: Vec2::ZERO,
            }
        };
        Self { bounds }
    }

    /// Effective bounds, including a zero-sized rectangle for an empty clip.
    pub fn bounds(self) -> Aabb2 {
        self.bounds
    }

    /// Intersects with an enclosing clip. Disjoint clips are empty.
    pub fn intersect(self, other: Self) -> Self {
        Self::new(Aabb2 {
            min: self.bounds.min.max(other.bounds.min),
            max: self.bounds.max.min(other.bounds.max),
        })
    }

    /// Tests the same half-open logical bounds used for drawing.
    pub fn contains(self, point: Vec2) -> bool {
        point.is_finite()
            && point.cmpge(self.bounds.min).all()
            && point.cmplt(self.bounds.max).all()
    }

    /// Converts to a target-pixel scissor `(x, y, width, height)`. A pixel is
    /// included exactly when its center is inside the logical clip. The canvas
    /// intersects with its target bounds first. Scale must be positive and finite.
    pub fn scissor(self, scale: Vec2) -> (i32, i32, i32, i32) {
        assert!(scale.is_finite() && scale.min_element() > 0.0);
        let min = (self.bounds.min * scale - Vec2::splat(0.5)).ceil();
        let max = (self.bounds.max * scale - Vec2::splat(0.5)).ceil();
        (
            min.x as i32,
            min.y as i32,
            (max.x - min.x).max(0.0) as i32,
            (max.y - min.y).max(0.0) as i32,
        )
    }
}

/// Uniform row-major grid, or a vertical list using one column. Item indices
/// describe layout only; retain game-owned stable IDs when filtering/reordering.
#[derive(Clone, Copy, Debug)]
pub struct UiLayout {
    columns: usize,
    item_size: Vec2,
    gap: Vec2,
}

impl UiLayout {
    /// Grid with positive column count/item dimensions and nonnegative gaps.
    /// Panics for invalid arguments.
    pub fn grid(columns: usize, item_size: Vec2, gap: Vec2) -> Self {
        assert!(columns > 0);
        assert!(item_size.is_finite() && item_size.min_element() > 0.0);
        assert!(gap.is_finite() && gap.min_element() >= 0.0);
        Self {
            columns,
            item_size,
            gap,
        }
    }

    /// Vertical list with a fixed item size and vertical gap.
    pub fn list(item_size: Vec2, gap: f32) -> Self {
        Self::grid(1, item_size, Vec2::new(0.0, gap))
    }

    /// Unscrolled local item bounds relative to the content origin.
    pub fn item(self, index: usize) -> Aabb2 {
        let cell = Vec2::new((index % self.columns) as f32, (index / self.columns) as f32);
        let min = cell * (self.item_size + self.gap);
        Aabb2 {
            min,
            max: min + self.item_size,
        }
    }

    /// Total content extent; an empty catalog has zero extent.
    pub fn content_size(self, count: usize) -> Vec2 {
        if count == 0 {
            return Vec2::ZERO;
        }
        let cells = Vec2::new(
            count.min(self.columns) as f32,
            count.div_ceil(self.columns) as f32,
        );
        cells * self.item_size + (cells - Vec2::ONE) * self.gap
    }
}

/// Retained content offset in UI units. Reconfigure on resize/content changes,
/// submit all focusable IDs (even clipped items), and reveal newly navigated focus.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiScrollState {
    offset: Vec2,
    viewport_size: Vec2,
    content_size: Vec2,
}

impl UiScrollState {
    /// Current nonnegative content offset.
    pub fn offset(&self) -> Vec2 {
        self.offset
    }

    /// Maximum offset after the last configuration.
    pub fn max_offset(&self) -> Vec2 {
        (self.content_size - self.viewport_size).max(Vec2::ZERO)
    }

    /// Updates dimensions and clamps the offset. Panics for invalid dimensions.
    pub fn configure(&mut self, viewport_size: Vec2, content_size: Vec2) {
        assert!(viewport_size.is_finite() && viewport_size.min_element() >= 0.0);
        assert!(content_size.is_finite() && content_size.min_element() >= 0.0);
        self.viewport_size = viewport_size;
        self.content_size = content_size;
        self.set_offset(self.offset);
    }

    /// Sets/clamps an offset. Nonfinite requests are ignored.
    pub fn set_offset(&mut self, offset: Vec2) {
        if offset.is_finite() {
            self.offset = offset.clamp(Vec2::ZERO, self.max_offset());
        }
    }

    /// Scrolls by UI-unit displacement. For wheel input, multiply the routed
    /// wheel delta by a game-chosen negative step. For content dragging, negate delta.
    pub fn scroll_by(&mut self, delta: Vec2) {
        self.set_offset(self.offset + delta);
    }

    /// Moves a content-local item into view, minimally. Oversized items align
    /// their minimum edge. Call on focus changes, so manual scrolling stays put.
    pub fn reveal(&mut self, item: Aabb2) {
        if !item.min.is_finite() || !item.max.is_finite() || item.max.cmplt(item.min).any() {
            return;
        }
        let end = item.max - self.viewport_size;
        self.set_offset(self.offset.max(end).min(item.min));
    }

    /// Translates content-local bounds into viewport coordinates for drawing
    /// and interaction. Clip these translated bounds with the viewport clip.
    pub fn item_bounds(&self, viewport: Aabb2, item: Aabb2) -> Aabb2 {
        let translation = viewport.min - self.offset;
        Aabb2 {
            min: item.min + translation,
            max: item.max + translation,
        }
    }

    /// Vertical scrollbar thumb within a game-owned track; `None` when scrolling
    /// is unnecessary or the track is invalid. No inventory semantics are imposed.
    pub fn vertical_thumb(&self, track: Aabb2, minimum_height: f32) -> Option<Aabb2> {
        let size = track.max - track.min;
        if !track.min.is_finite()
            || !size.is_finite()
            || size.min_element() <= 0.0
            || !minimum_height.is_finite()
            || minimum_height < 0.0
            || self.max_offset().y <= 0.0
        {
            return None;
        }
        let height = (size.y * self.viewport_size.y / self.content_size.y)
            .max(minimum_height)
            .min(size.y);
        let y = (size.y - height) * self.offset.y / self.max_offset().y;
        let min = track.min + Vec2::new(0.0, y);
        Some(Aabb2 {
            min,
            max: min + Vec2::new(size.x, height),
        })
    }

    /// Applies a captured scrollbar thumb's UI-unit drag delta. Returns false
    /// when there is no movable thumb. Reuses the exact drawing geometry.
    pub fn drag_vertical_thumb(&mut self, track: Aabb2, minimum_height: f32, delta: f32) -> bool {
        let Some(thumb) = self.vertical_thumb(track, minimum_height) else {
            return false;
        };
        let travel = (track.max.y - track.min.y) - (thumb.max.y - thumb.min.y);
        if travel <= 0.0 || !delta.is_finite() {
            return false;
        }
        self.scroll_by(Vec2::new(0.0, delta * self.max_offset().y / travel));
        true
    }
}

#[cfg(test)]
mod tests;
