//! SDK adapter using cached texture handles and the existing sprite draw path.
use crate::{SubmissionStats, Tilemap};
use rayengine::render::Canvas2D;
use rayengine::{prelude::*, raylib::prelude::RaylibDraw};
use rayengine_core::{spatial::SpatialError, sprite::SpriteTransform};

/// A caller-owned cached atlas handle. The map owns no GPU resources.
#[derive(Clone, Copy, Debug)]
pub struct TileAtlas {
    /// Stable SDK texture handle; unloading it makes later submissions fail safely.
    pub texture: TextureId,
    /// Multiplied RGBA tint.
    pub tint: Color,
}
/// Culled work and successful backend submissions for a draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawStats {
    /// CPU culling work.
    pub visible: SubmissionStats,
    /// Successful sprite submissions; stale handles/out-of-texture regions skip draws.
    pub drawn: usize,
}
impl TileAtlas {
    /// Creates an untinted atlas adapter. Source bounds are checked on each draw.
    pub fn new(texture: TextureId) -> Self {
        Self {
            texture,
            tint: Color::WHITE,
        }
    }
    /// Draws layer-ordered tiles inside a world pass using the same camera and
    /// viewport as the pass. No GPU resource or command-buffer allocation occurs.
    pub fn draw<D: RaylibDraw>(
        &self,
        map: &Tilemap,
        canvas: &mut Canvas2D<'_, D>,
        camera: &Camera2D,
        viewport: &Viewport,
    ) -> Result<DrawStats, SpatialError> {
        let mut drawn = 0;
        let visible = map.visit_visible(camera, viewport, |tile| {
            let transform = SpriteTransform {
                position: tile.bounds.min,
                size: tile.bounds.size(),
                ..Default::default()
            };
            drawn += usize::from(canvas.sprite(
                self.texture,
                map.palette()[tile.tile.0 as usize].region,
                transform,
                self.tint,
            ));
        })?;
        Ok(DrawStats { visible, drawn })
    }
    /// Opens a world pass and uses its current viewport for culling and drawing.
    pub fn draw_frame(
        &self,
        map: &Tilemap,
        frame: &mut Frame<'_, '_>,
        camera: Camera2D,
    ) -> Result<DrawStats, SpatialError> {
        let viewport = frame.viewport;
        let mut result = Ok(DrawStats::default());
        frame.world_2d(camera, |canvas| {
            result = self.draw(map, canvas, &camera, &viewport);
        });
        result
    }
}

#[cfg(test)]
mod native_tests;
