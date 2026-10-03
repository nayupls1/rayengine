#![doc = include_str!("../README.md")]

mod format;
mod queries;
#[cfg(feature = "render")]
pub mod render;

/// Checked guides exported with the plugin's offline Rust documentation.
pub mod guides {
    #[doc = include_str!("../docs/performance.md")]
    pub mod performance {}
    /// Disk-loaded example: manifest assets, drawing, movement and tile edits.
    /// Enable the `render` feature to include its checked source below.
    #[cfg_attr(feature = "render", doc = "\n\n```no_run")]
    #[cfg_attr(feature = "render", doc = include_str!("../examples/level.rs"))]
    #[cfg_attr(feature = "render", doc = "```")]
    pub mod level {}
}

use rayengine_core::{collision::Aabb2, glam::Vec2, sprite::SpriteRegion};
use std::fmt;

pub use queries::{CollisionTile, SubmissionStats, TileHit};

/// Fixed chunk edge in tiles. Edge chunks may be partially occupied.
pub const CHUNK_SIZE: u32 = 16;
/// Maximum allocated cells across all layers (including chunk padding).
pub const MAX_CELLS: usize = 16 * 1024 * 1024;

/// Tile index into the immutable map palette; zero is an ordinary tile ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileId(pub u32);

/// Collision metadata shared by placements of a tile definition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollisionFlags {
    /// Full box collision.
    pub solid: bool,
    /// Top-face collision when moving downward from above (positive Y is down).
    pub one_way: bool,
    /// Game-owned trigger bits; these never block movement on their own.
    pub trigger: u32,
    /// Game-owned custom bits; these never block movement on their own.
    pub custom: u32,
}

/// A sprite-sheet tile and its collision metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileDefinition {
    /// Region in the single map atlas. Validated against texture size by the adapter.
    pub region: SpriteRegion,
    /// Collision metadata; solid takes precedence over one-way.
    pub collision: CollisionFlags,
}

/// Invalid tilemap geometry, coordinates, or level data.
#[derive(Debug)]
pub struct TilemapError(pub(crate) String);
impl fmt::Display for TilemapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for TilemapError {}
fn error(message: impl Into<String>) -> TilemapError {
    TilemapError(message.into())
}

struct Chunk {
    tiles: Vec<Option<TileId>>,
    occupied: usize,
}
struct Layer {
    name: String,
    chunks: Vec<Chunk>,
}

/// Finite layered grid. Layers draw in insertion order; positive Y points down.
/// Empty cells are `None`. Edits immediately affect every query and submission;
/// no collision or render snapshot is cached internally.
pub struct Tilemap {
    width: u32,
    height: u32,
    origin: Vec2,
    tile_size: Vec2,
    chunks_x: u32,
    chunks_y: u32,
    palette: Vec<TileDefinition>,
    layers: Vec<Layer>,
    atlas: Option<String>,
}

impl Tilemap {
    /// Creates empty layers, rejecting zero dimensions, invalid geometry,
    /// empty/duplicate layer names, an empty palette, or more than [`MAX_CELLS`].
    /// World coordinates must retain enough f32 precision to distinguish cells.
    pub fn new(
        width: u32,
        height: u32,
        origin: Vec2,
        tile_size: Vec2,
        palette: Vec<TileDefinition>,
        layer_names: Vec<String>,
    ) -> Result<Self, TilemapError> {
        let end = origin + Vec2::new(width as f32, height as f32) * tile_size;
        if width == 0
            || height == 0
            || width > MAX_CELLS as u32
            || height > MAX_CELLS as u32
            || !origin.is_finite()
            || !tile_size.is_finite()
            || tile_size.min_element() <= 0.0
            || !end.is_finite()
            || !(origin + tile_size).cmpgt(origin).all()
            || !(end - tile_size).cmplt(end).all()
        {
            return Err(error("invalid dimensions, origin or tile size"));
        }
        // Requiring at least two ulps avoids collapsed intermediate cell edges.
        for axis in 0..2 {
            let magnitude = origin[axis].abs().max(end[axis].abs());
            if tile_size[axis] < (magnitude.next_up() - magnitude) * 2.0 {
                return Err(error(
                    "tile size is too small for world coordinate precision",
                ));
            }
        }
        if palette.is_empty() || palette.len() > u32::MAX as usize || layer_names.is_empty() {
            return Err(error("palette and layers must be nonempty"));
        }
        let mut names = std::collections::BTreeSet::new();
        for name in &layer_names {
            if name.trim().is_empty() || !names.insert(name) {
                return Err(error("layer names must be nonblank and unique"));
            }
        }
        let chunks_x = width.div_ceil(CHUNK_SIZE);
        let chunks_y = height.div_ceil(CHUNK_SIZE);
        let count = u64::from(chunks_x) * u64::from(chunks_y);
        let cells = count
            .checked_mul(u64::from(CHUNK_SIZE * CHUNK_SIZE))
            .and_then(|n| n.checked_mul(layer_names.len() as u64));
        if cells.is_none_or(|n| n > MAX_CELLS as u64) {
            return Err(error("map exceeds MAX_CELLS including chunk padding"));
        }
        let layers = layer_names
            .into_iter()
            .map(|name| Layer {
                name,
                chunks: (0..count)
                    .map(|_| Chunk {
                        tiles: vec![None; (CHUNK_SIZE * CHUNK_SIZE) as usize],
                        occupied: 0,
                    })
                    .collect(),
            })
            .collect();
        Ok(Self {
            width,
            height,
            origin,
            tile_size,
            chunks_x,
            chunks_y,
            palette,
            layers,
            atlas: None,
        })
    }
    /// Dimensions in cells.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    /// World-space top-left.
    pub fn origin(&self) -> Vec2 {
        self.origin
    }
    /// Cell dimensions in world units.
    pub fn tile_size(&self) -> Vec2 {
        self.tile_size
    }
    /// Immutable definitions referenced by tile IDs.
    pub fn palette(&self) -> &[TileDefinition] {
        &self.palette
    }
    /// Number of layers.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
    /// Layer name, if present.
    pub fn layer_name(&self, layer: usize) -> Option<&str> {
        self.layers.get(layer).map(|l| l.name.as_str())
    }
    /// Optional atlas asset name supplied by the level format.
    pub fn atlas_asset(&self) -> Option<&str> {
        self.atlas.as_deref()
    }
    /// World bounds of the finite map.
    pub fn bounds(&self) -> Aabb2 {
        Aabb2 {
            min: self.origin,
            max: self.origin + Vec2::new(self.width as f32, self.height as f32) * self.tile_size,
        }
    }
    fn address(&self, layer: usize, x: u32, y: u32) -> Option<(usize, usize)> {
        if layer >= self.layers.len() || x >= self.width || y >= self.height {
            return None;
        }
        Some((
            (y / CHUNK_SIZE * self.chunks_x + x / CHUNK_SIZE) as usize,
            (y % CHUNK_SIZE * CHUNK_SIZE + x % CHUNK_SIZE) as usize,
        ))
    }
    /// Looks up a tile; empty cells and out-of-map coordinates return `None`.
    pub fn tile(&self, layer: usize, x: u32, y: u32) -> Option<TileId> {
        let (chunk, cell) = self.address(layer, x, y)?;
        self.layers[layer].chunks[chunk].tiles[cell]
    }
    /// Sets or clears a cell. Invalid coordinates/IDs leave the map unchanged.
    pub fn set_tile(
        &mut self,
        layer: usize,
        x: u32,
        y: u32,
        tile: Option<TileId>,
    ) -> Result<(), TilemapError> {
        let (chunk, cell) = self
            .address(layer, x, y)
            .ok_or_else(|| error("tile coordinate or layer outside map"))?;
        if tile.is_some_and(|id| id.0 as usize >= self.palette.len()) {
            return Err(error("unknown tile ID"));
        }
        let chunk = &mut self.layers[layer].chunks[chunk];
        chunk.occupied -= usize::from(chunk.tiles[cell].is_some());
        chunk.tiles[cell] = tile;
        chunk.occupied += usize::from(tile.is_some());
        Ok(())
    }
    /// Top-left world position of a cell, or `None` outside the map.
    pub fn tile_to_world(&self, x: u32, y: u32) -> Option<Vec2> {
        (x < self.width && y < self.height)
            .then(|| self.origin + Vec2::new(x as f32, y as f32) * self.tile_size)
    }
    /// Cell containing a world point; right/bottom edges are outside the map.
    pub fn world_to_tile(&self, world: Vec2) -> Option<(u32, u32)> {
        if !world.is_finite()
            || !world.cmpge(self.origin).all()
            || !world.cmplt(self.bounds().max).all()
        {
            return None;
        }
        let cell = ((world - self.origin) / self.tile_size).floor();
        let mut x = (cell.x as u32).min(self.width - 1);
        let mut y = (cell.y as u32).min(self.height - 1);
        // Correct division rounding at the shared f32 cell edges.
        while x > 0 && world.x < self.tile_to_world(x, y)?.x {
            x -= 1;
        }
        while x + 1 < self.width && world.x >= self.tile_to_world(x + 1, y)?.x {
            x += 1;
        }
        while y > 0 && world.y < self.tile_to_world(x, y)?.y {
            y -= 1;
        }
        while y + 1 < self.height && world.y >= self.tile_to_world(x, y + 1)?.y {
            y += 1;
        }
        Some((x, y))
    }
    /// World box of a cell, or `None` outside the map.
    pub fn tile_bounds(&self, x: u32, y: u32) -> Option<Aabb2> {
        let min = self.tile_to_world(x, y)?;
        // Use shared edges rather than min + size to avoid seams from rounding.
        let max = self.origin + Vec2::new((x + 1) as f32, (y + 1) as f32) * self.tile_size;
        Some(Aabb2 { min, max })
    }
    fn chunk_bounds(&self, x: u32, y: u32) -> Aabb2 {
        let min = self.tile_to_world(x * CHUNK_SIZE, y * CHUNK_SIZE).unwrap();
        let max = self.origin
            + Vec2::new(
                ((x + 1) * CHUNK_SIZE).min(self.width) as f32,
                ((y + 1) * CHUNK_SIZE).min(self.height) as f32,
            ) * self.tile_size;
        Aabb2 { min, max }
    }
}

#[cfg(test)]
mod tests;
