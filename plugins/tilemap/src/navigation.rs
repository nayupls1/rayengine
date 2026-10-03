use crate::*;
use rayengine_core::{
    glam::UVec2,
    pathfinding::{GridLayout, NavGrid},
};

/// Top-down navigation over the current cells: a cell is blocked when any
/// layer holds a solid tile, and costs `1.0` otherwise. One-way, trigger and
/// custom tiles are walkable. For other rules, such as costs per tile, wrap
/// the map in a [`GridFn`](rayengine_core::pathfinding::GridFn).
impl NavGrid for Tilemap {
    fn size(&self) -> UVec2 {
        UVec2::new(self.width, self.height)
    }

    fn cost(&self, cell: UVec2) -> Option<f32> {
        (!self.is_solid(cell.x, cell.y)).then_some(1.0)
    }
}

impl Tilemap {
    /// Whether any layer holds a solid tile at a cell. Cells outside the map
    /// are not solid.
    pub fn is_solid(&self, x: u32, y: u32) -> bool {
        (0..self.layers.len()).any(|layer| {
            self.tile(layer, x, y)
                .is_some_and(|id| self.palette[id.0 as usize].collision.solid)
        })
    }

    /// The map's cell geometry for pathfinding, converting path cells to
    /// world positions and back.
    pub fn grid_layout(&self) -> GridLayout {
        GridLayout::new(self.origin, self.tile_size)
    }
}
