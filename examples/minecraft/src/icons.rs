//! CPU-built inventory and HUD icons. Block items reuse the packed terrain atlas
//! (including imported packs); tools, sticks and torches are original pixel art.
use crate::survival::Item;
use crate::textures::{Atlas, TextureError, Tile};

/// Square straight-RGBA icon image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    /// Width and height in pixels.
    pub size: u32,
    /// Row-major RGBA8 pixels.
    pub rgba: Vec<u8>,
}
/// Heart states drawn by the health bar, in [`Icon`] order of [`hearts`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heart {
    /// Two health units.
    Full = 0,
    /// One health unit.
    Half = 1,
    /// Missing health.
    Empty = 2,
}
impl Icon {
    fn blank(size: u32) -> Self {
        Self {
            size,
            rgba: vec![0; (size * size * 4) as usize],
        }
    }
    fn set(&mut self, x: i32, y: i32, [r, g, b]: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.size as i32 || y >= self.size as i32 {
            return;
        }
        let i = ((y as u32 * self.size + x as u32) * 4) as usize;
        self.rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
    }
    /// Encode for raylib's in-memory image loader.
    pub fn png(&self) -> Result<Vec<u8>, TextureError> {
        let mut result = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut result, self.size, self.size);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| TextureError(format!("icon: {e}")))?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|e| TextureError(format!("icon: {e}")))?;
        }
        Ok(result)
    }
}
/// Copy one tile, without gutters, out of the packed atlas.
fn tile(atlas: &Atlas, tile: Tile) -> Icon {
    let rect = atlas.rects[tile as usize];
    let x0 = (rect.x * atlas.width as f32).round() as u32;
    let y0 = (rect.y * atlas.height as f32).round() as u32;
    let size = (rect.z * atlas.width as f32).round() as u32;
    let mut icon = Icon::blank(size);
    for y in 0..size {
        let from = (((y0 + y) * atlas.width + x0) * 4) as usize;
        let to = (y * size * 4) as usize;
        icon.rgba[to..to + size as usize * 4]
            .copy_from_slice(&atlas.rgba[from..from + size as usize * 4]);
    }
    icon
}
const OUTLINE: [u8; 3] = [40, 30, 18];
const HANDLE: [u8; 3] = [137, 103, 52];
const HANDLE_DARK: [u8; 3] = [96, 70, 34];
const WOOD_HEAD: [u8; 3] = [176, 136, 78];
const STONE_HEAD: [u8; 3] = [138, 140, 146];
/// Diagonal handle from the lower-left corner, shared by sticks and tools.
fn handle(icon: &mut Icon, length: i32) {
    for t in 0..length {
        let (x, y) = (2 + t, 13 - t);
        icon.set(x, y, HANDLE);
        icon.set(x + 1, y, HANDLE_DARK);
        icon.set(x - 1, y, OUTLINE);
        icon.set(x, y + 1, OUTLINE);
    }
}
fn pickaxe(head: [u8; 3]) -> Icon {
    let mut icon = Icon::blank(16);
    handle(&mut icon, 10);
    let shade = head.map(|c| (c as f32 * 0.7) as u8);
    // A curved head over the top-right end of the handle.
    for (x, y) in [
        (4, 2),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (9, 2),
        (10, 2),
        (11, 3),
        (12, 4),
        (13, 5),
        (13, 6),
        (14, 7),
        (14, 8),
        (14, 9),
        (14, 10),
    ] {
        icon.set(x, y, OUTLINE);
        icon.set(x, y + 1, head);
        icon.set(x - 1, y + 1, head);
        icon.set(x - 1, y + 2, shade);
    }
    icon
}
fn axe(head: [u8; 3]) -> Icon {
    let mut icon = Icon::blank(16);
    handle(&mut icon, 10);
    let shade = head.map(|c| (c as f32 * 0.7) as u8);
    for y in 1..8 {
        for x in 8..14 {
            if (x - 8) + (7 - y) < 11 && !(x == 8 && y > 4) {
                icon.set(x, y, if x > 11 || y > 5 { shade } else { head });
            }
        }
    }
    for y in 1..8 {
        icon.set(14, y, OUTLINE);
    }
    icon
}
fn stick() -> Icon {
    let mut icon = Icon::blank(16);
    handle(&mut icon, 12);
    icon
}
fn torch() -> Icon {
    let mut icon = Icon::blank(16);
    for y in 6..15 {
        icon.set(7, y, HANDLE);
        icon.set(8, y, HANDLE_DARK);
    }
    for (x, y, color) in [
        (7, 2, [255, 236, 140]),
        (8, 2, [255, 214, 92]),
        (6, 3, [255, 170, 60]),
        (7, 3, [255, 248, 200]),
        (8, 3, [255, 236, 140]),
        (9, 3, [255, 170, 60]),
        (6, 4, [234, 120, 40]),
        (7, 4, [255, 214, 92]),
        (8, 4, [255, 196, 80]),
        (9, 4, [234, 120, 40]),
        (7, 5, [180, 80, 30]),
        (8, 5, [180, 80, 30]),
        (8, 1, [255, 200, 90]),
    ] {
        icon.set(x, y, color);
    }
    icon
}
fn planks() -> Icon {
    let mut icon = Icon::blank(16);
    for y in 0..16 {
        for x in 0..16 {
            let seam = y % 4 == 3 || (x == (y / 4 * 5 + 3) % 16);
            let grain = (x * 7 + y * 3) % 11 == 0;
            icon.set(
                x,
                y,
                if seam {
                    [112, 82, 44]
                } else if grain {
                    [170, 128, 72]
                } else {
                    [188, 145, 86]
                },
            );
        }
    }
    icon
}
fn coal() -> Icon {
    let mut icon = Icon::blank(16);
    for y in 3..14 {
        for x in 3..14 {
            let (dx, dy) = (x as f32 - 8.0, y as f32 - 8.5);
            if dx * dx + dy * dy * 1.3 < 26.0 {
                let shine = dx + dy < -4.0;
                icon.set(
                    x,
                    y,
                    if shine {
                        [92, 96, 104]
                    } else if (x + y) % 3 == 0 {
                        [30, 31, 35]
                    } else {
                        [50, 52, 58]
                    },
                );
            }
        }
    }
    icon
}
/// Icons in [`Item::ALL`] order.
pub fn items(atlas: &Atlas) -> Vec<Icon> {
    Item::ALL
        .iter()
        .map(|&item| match item {
            Item::Dirt => tile(atlas, Tile::Dirt),
            Item::Stone => tile(atlas, Tile::Stone),
            Item::Log => tile(atlas, Tile::LogSide),
            Item::Leaves => tile(atlas, Tile::Leaves),
            Item::IronOre => tile(atlas, Tile::Iron),
            Item::Coal => coal(),
            Item::Planks => planks(),
            Item::Stick => stick(),
            Item::WoodenPickaxe => pickaxe(WOOD_HEAD),
            Item::StonePickaxe => pickaxe(STONE_HEAD),
            Item::WoodenAxe => axe(WOOD_HEAD),
            Item::StoneAxe => axe(STONE_HEAD),
            Item::Torch => torch(),
        })
        .collect()
}
/// Full, half and empty 9×9 hearts, in [`Heart`] order.
pub fn hearts() -> [Icon; 3] {
    const SHAPE: [&str; 8] = [
        ".##...##.",
        "####.####",
        "#########",
        "#########",
        ".#######.",
        "..#####..",
        "...###...",
        "....#....",
    ];
    std::array::from_fn(|kind| {
        let mut icon = Icon::blank(9);
        for (y, row) in SHAPE.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell != b'#' {
                    continue;
                }
                let filled =
                    kind == Heart::Full as usize || (kind == Heart::Half as usize && x < 5);
                let color = if !filled {
                    [58, 22, 26]
                } else if x <= 2 && y <= 2 {
                    [255, 140, 140]
                } else {
                    [214, 38, 44]
                };
                icon.set(x as i32, y as i32 + 1, color);
            }
        }
        icon
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textures::TextureSet;

    #[test]
    fn every_item_has_a_visible_square_icon_matching_atlas_tiles() {
        let atlas = TextureSet::fallback().pack();
        let icons = items(&atlas);
        assert_eq!(icons.len(), Item::ALL.len());
        for (item, icon) in Item::ALL.iter().zip(&icons) {
            assert_eq!(
                item.index(),
                Item::ALL.iter().position(|i| i == item).unwrap()
            );
            assert!(icon.size >= 16);
            assert_eq!(icon.rgba.len(), (icon.size * icon.size * 4) as usize);
            let opaque = icon.rgba.chunks(4).filter(|p| p[3] == 255).count();
            assert!(opaque >= 12, "{item:?} icon is nearly empty");
            assert!(!icon.png().unwrap().is_empty());
        }
        let stone = &icons[Item::Stone.index()];
        let source = TextureSet::fallback();
        assert_eq!(stone.rgba, source.tile(Tile::Stone).rgba());
        let [full, half, empty] = hearts();
        let red = |i: &Icon| {
            i.rgba
                .chunks(4)
                .filter(|p| p[0] > 200 && p[3] == 255)
                .count()
        };
        assert!(red(&full) > red(&half) && red(&half) > red(&empty));
    }
}
