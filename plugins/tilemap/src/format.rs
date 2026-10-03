use crate::*;
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Level {
    version: u32,
    atlas: String,
    tile_size: [f32; 2],
    #[serde(default)]
    origin: [f32; 2],
    tiles: BTreeMap<String, Definition>,
    layers: Vec<Grid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    region: [u32; 4],
    #[serde(default)]
    solid: bool,
    #[serde(default)]
    one_way: bool,
    #[serde(default)]
    trigger: u32,
    #[serde(default)]
    custom: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grid {
    name: String,
    rows: Vec<String>,
}

impl Tilemap {
    /// Parses version 1 TOML: one Unicode character per cell, `.` for empty,
    /// a character-keyed palette, and equal-sized row arrays for each layer.
    /// Unknown fields, undefined symbols, invalid regions, ragged layers and
    /// unsupported versions are errors. Atlas names must be root-relative
    /// asset names without parent traversal (resolved by the game manifest).
    pub fn from_toml(text: &str) -> Result<Self, TilemapError> {
        let level: Level = toml::from_str(text).map_err(|e| error(e.to_string()))?;
        if level.version != 1 {
            return Err(error(format!(
                "unsupported level version {}",
                level.version
            )));
        }
        let path = Path::new(&level.atlas);
        if level.atlas.trim().is_empty()
            || level.atlas.contains('\\')
            || path.is_absolute()
            || path
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(error("atlas must be a nonempty root-relative asset name"));
        }
        let mut palette = Vec::new();
        let mut legend = BTreeMap::new();
        for (symbol, definition) in level.tiles {
            let mut chars = symbol.chars();
            let Some(character) = chars.next() else {
                return Err(error("empty legend symbol"));
            };
            if chars.next().is_some() || character == '.' || character.is_control() {
                return Err(error(
                    "legend keys must be single non-control characters other than '.'",
                ));
            }
            let [x, y, width, height] = definition.region;
            let region =
                SpriteRegion::new(x, y, width, height).map_err(|e| error(e.to_string()))?;
            legend.insert(character, TileId(palette.len() as u32));
            palette.push(TileDefinition {
                region,
                collision: CollisionFlags {
                    solid: definition.solid,
                    one_way: definition.one_way,
                    trigger: definition.trigger,
                    custom: definition.custom,
                },
            });
        }
        let first = level
            .layers
            .first()
            .ok_or_else(|| error("level has no layers"))?;
        let width = first
            .rows
            .first()
            .ok_or_else(|| error("layer has no rows"))?
            .chars()
            .count();
        let height = first.rows.len();
        let mut map = Self::new(
            u32::try_from(width).map_err(|_| error("too many columns"))?,
            u32::try_from(height).map_err(|_| error("too many rows"))?,
            Vec2::from_array(level.origin),
            Vec2::from_array(level.tile_size),
            palette,
            level.layers.iter().map(|l| l.name.clone()).collect(),
        )?;
        for (layer, grid) in level.layers.iter().enumerate() {
            if grid.rows.len() != height {
                return Err(error("layers must have equal height"));
            }
            for (y, row) in grid.rows.iter().enumerate() {
                if row.chars().count() != width {
                    return Err(error("rows must have equal width"));
                }
                for (x, symbol) in row.chars().enumerate() {
                    if symbol == '.' {
                        continue;
                    }
                    let tile = legend.get(&symbol).ok_or_else(|| {
                        error(format!(
                            "undefined tile symbol {symbol:?} at layer {layer}, {x},{y}"
                        ))
                    })?;
                    map.set_tile(layer, x as u32, y as u32, Some(*tile))?;
                }
            }
        }
        map.atlas = Some(level.atlas);
        Ok(map)
    }
    /// Loads a UTF-8 TOML level from disk. Use `ResolvedManifest::asset` to
    /// resolve this path through declared asset roots before loading.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, TilemapError> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).map_err(|e| error(format!("{}: {e}", path.display())))?;
        Self::from_toml(&text).map_err(|e| error(format!("{}: {e}", path.display())))
    }
}
