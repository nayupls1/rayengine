//! Original procedural pixel art, licensed under the repository MIT license.
//! These patterns are game-owned; no Minecraft image data is embedded here.
use super::{Tile, TileImage};
fn noise(x: u32, y: u32, salt: u32) -> i16 {
    let mut n =
        x.wrapping_mul(0x45d9f3b) ^ y.wrapping_mul(0x27d4eb2d) ^ salt.wrapping_mul(0x9e3779b9);
    n = (n ^ (n >> 16)).wrapping_mul(0x45d9f3b);
    ((n >> 24) % 25) as i16 - 12
}
pub(super) fn fallback(tile: Tile) -> TileImage {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let mut alpha = 255;
            let color = match tile {
                Tile::Bedrock => {
                    if noise(x, y, 14) > 1 {
                        [54, 56, 62]
                    } else {
                        [88, 91, 99]
                    }
                }
                Tile::Stone => [129, 134, 139],
                Tile::Dirt => {
                    if (x + 3 * y) % 11 == 0 {
                        [112, 79, 46]
                    } else {
                        [144, 102, 62]
                    }
                }
                Tile::GrassTop => [91, 157, 55],
                Tile::GrassSide => {
                    if y < 3 + ((x / 3) % 3) {
                        [91, 157, 55]
                    } else {
                        [144, 102, 62]
                    }
                }
                Tile::Coal | Tile::Iron => {
                    if noise(x / 2, y / 2, 3) > 2 {
                        if tile == Tile::Coal {
                            [45, 47, 51]
                        } else {
                            [182, 135, 104]
                        }
                    } else {
                        [129, 134, 139]
                    }
                }
                Tile::LogSide => {
                    if (x + x / 4) % 4 == 0 {
                        [91, 61, 35]
                    } else {
                        [129, 90, 51]
                    }
                }
                Tile::LogTop => {
                    let d = (x as i32 - 7).abs().max((y as i32 - 7).abs());
                    if d % 3 == 0 {
                        [126, 85, 47]
                    } else {
                        [177, 139, 84]
                    }
                }
                Tile::Leaves => {
                    if noise(x, y, 7) > 1 && (x + y) % 3 != 0 {
                        alpha = 0;
                    }
                    [58, 130, 48]
                }
            };
            let shade = noise(x, y, tile as u32);
            rgba.extend(color.map(|v| (v as i16 + shade).clamp(0, 255) as u8));
            rgba.push(alpha);
        }
    }
    TileImage::new(16, rgba).expect("fixed fallback dimensions")
}
