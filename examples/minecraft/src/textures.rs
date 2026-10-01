//! CPU-only loading of a small extracted Java PNG subset. No archive parsing,
//! installation discovery, or runtime filesystem writes.
use rayengine_voxel::{TileId, glam::Vec4};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

/// Render-only tile keys. Block IDs and version-one terrain cells stay unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Tile {
    /// Underground unbreakable rock.
    Bedrock = 0,
    /// Plain stone.
    Stone = 1,
    /// Soil and grass underside.
    Dirt = 2,
    /// Tinted grass upper face.
    GrassTop = 3,
    /// Coal-bearing stone.
    Coal = 4,
    /// Iron-bearing stone.
    Iron = 5,
    /// Oak bark on vertical log faces.
    LogSide = 6,
    /// Tinted alpha-cutout oak foliage.
    Leaves = 7,
    /// Grass side, composited with a tinted overlay.
    GrassSide = 8,
    /// Oak rings on both log ends.
    LogTop = 9,
}
impl Tile {
    /// Atlas order, matching stable tile keys.
    pub const ALL: [Self; 10] = [
        Self::Bedrock,
        Self::Stone,
        Self::Dirt,
        Self::GrassTop,
        Self::Coal,
        Self::Iron,
        Self::LogSide,
        Self::Leaves,
        Self::GrassSide,
        Self::LogTop,
    ];
    /// Key consumed by voxel meshing/material lookup.
    pub const fn id(self) -> TileId {
        TileId(self as u16)
    }
}
const FILES: [&str; 11] = [
    "bedrock",
    "stone",
    "dirt",
    "grass_block_top",
    "coal_ore",
    "iron_ore",
    "oak_log",
    "oak_leaves",
    "grass_block_side",
    "oak_log_top",
    "grass_block_side_overlay",
];
/// Constant game palette for imported grayscale grass/foliage, not biome lookup.
pub const GRASS_TINT: [u8; 3] = [105, 175, 65];
/// Constant imported oak foliage palette.
pub const LEAF_TINT: [u8; 3] = [72, 142, 47];
const MAX_FILE: u64 = 4 * 1024 * 1024;
/// Maximum imported square tile side. Supported sizes are powers of two, 16..=256.
pub const MAX_TILE_SIZE: u32 = 256;
const COLUMNS: u32 = 5;
/// Bad paths, incomplete packs, unsupported image layouts, or decoding failures.
#[derive(Debug)]
pub struct TextureError(pub String);
impl std::fmt::Display for TextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for TextureError {}
fn error(label: &str, why: impl std::fmt::Display) -> TextureError {
    TextureError(format!("{label}: {why}"))
}
/// Validated square image in top-to-bottom RGBA8 order.
#[derive(Clone, Debug)]
pub struct TileImage {
    size: u32,
    rgba: Vec<u8>,
}
impl TileImage {
    /// Rejects unsupported sizes or an inconsistent payload before import/packing.
    pub fn new(size: u32, rgba: Vec<u8>) -> Result<Self, TextureError> {
        if !(16..=MAX_TILE_SIZE).contains(&size)
            || !size.is_power_of_two()
            || rgba.len() != (size * size * 4) as usize
        {
            return Err(error(
                "tile",
                "expected a power-of-two square RGBA8 image, 16..=256 pixels",
            ));
        }
        Ok(Self { size, rgba })
    }
    /// Width/height in pixels.
    pub fn size(&self) -> u32 {
        self.size
    }
    /// Top-to-bottom rows, four bytes per pixel.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}
/// Decodes one supported static PNG. Palette, gray, RGB and RGBA inputs normalize
/// to RGBA8; transparent cutout pixels retain alpha. Animated/strip images fail.
pub fn decode_tile(bytes: &[u8], label: &str) -> Result<TileImage, TextureError> {
    if bytes.len() as u64 > MAX_FILE {
        return Err(error(label, "PNG exceeds 4 MiB input limit"));
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 4 * 1024 * 1024,
    });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| error(label, e))?;
    let info = reader.info();
    if info.animation_control.is_some()
        || info.width != info.height
        || !(16..=MAX_TILE_SIZE).contains(&info.width)
        || !info.width.is_power_of_two()
    {
        return Err(error(
            label,
            "unsupported PNG; expected a static power-of-two square, 16..=256 pixels (animated strips/APNG are unsupported)",
        ));
    }
    let size = info.width;
    let mut bytes = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| error(label, "PNG output too large"))?
    ];
    let output = reader.next_frame(&mut bytes).map_err(|e| error(label, e))?;
    let channels = match output.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        _ => return Err(error(label, "unsupported PNG color encoding")),
    };
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for pixel in bytes[..output.buffer_size()].chunks_exact(channels) {
        match channels {
            4 => rgba.extend_from_slice(pixel),
            3 => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
            2 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]),
            _ => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255]),
        }
    }
    TileImage::new(size, rgba).map_err(|e| error(label, e))
}
/// A validated ten-tile set before atlas packing.
pub struct TextureSet {
    tiles: [TileImage; 10],
    source: String,
}
impl TextureSet {
    /// Validated tiles in Tile::ALL order. This constructor applies no Minecraft tint.
    pub fn new(tiles: [TileImage; 10], source: impl Into<String>) -> Self {
        Self {
            tiles,
            source: source.into(),
        }
    }
    /// Original repository-owned procedural pixel art, usable without external files.
    pub fn fallback() -> Self {
        Self::new(
            std::array::from_fn(|i| fallback(Tile::ALL[i])),
            "built-in fallback",
        )
    }
    /// Loads a direct PNG directory or extracted modern Java pack root.
    /// Requires all eleven documented PNGs. The game never opens an archive
    /// or writes an asset file; extract installation textures once beforehand.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, TextureError> {
        let path = path.as_ref();
        let mut source = Source::open(path)?;
        let mut images = Vec::with_capacity(FILES.len());
        for name in FILES {
            let bytes = source.read(name)?;
            images.push(decode_tile(
                &bytes,
                &format!("{} ({name}.png)", path.display()),
            )?);
        }
        tint(&mut images[3], GRASS_TINT);
        tint(&mut images[7], LEAF_TINT);
        tint(&mut images[10], GRASS_TINT);
        let overlay = images.pop().unwrap();
        images[8] = composite(&images[8], &overlay);
        // Opaque surfaces are opaque even when an input pack carries alpha.
        for (i, image) in images.iter_mut().enumerate() {
            if i != Tile::Leaves as usize {
                for pixel in image.rgba.as_chunks_mut::<4>().0 {
                    pixel[3] = 255;
                }
            }
        }
        Ok(Self::new(
            images.try_into().expect("exactly ten mapped tiles"),
            path.display().to_string(),
        ))
    }
    /// Decoded input tile for tests, tools or alternate renderers.
    pub fn tile(&self, tile: Tile) -> &TileImage {
        &self.tiles[tile as usize]
    }
    /// Pack into one nearest-filtered atlas, with duplicated one-pixel edge gutters.
    /// Mixed supported resolutions scale to the largest tile by nearest sampling.
    /// The shader wraps local UVs inside each tile, keeping greedy faces tiled.
    pub fn pack(&self) -> Atlas {
        let size = self.tiles.iter().map(|tile| tile.size).max().unwrap();
        let stride = size + 2;
        let width = COLUMNS * stride;
        let height = 2 * stride;
        let mut rgba = vec![0; (width * height * 4) as usize];
        let rects = std::array::from_fn(|i| {
            let x = (i as u32 % COLUMNS) * stride;
            let y = (i as u32 / COLUMNS) * stride;
            let tile = &self.tiles[i];
            for py in 0..stride {
                for px in 0..stride {
                    let tx = px.saturating_sub(1).min(size - 1) * tile.size / size;
                    let ty = py.saturating_sub(1).min(size - 1) * tile.size / size;
                    let src = ((ty * tile.size + tx) * 4) as usize;
                    let dest = (((y + py) * width + x + px) * 4) as usize;
                    rgba[dest..dest + 4].copy_from_slice(&tile.rgba[src..src + 4]);
                }
            }
            Vec4::new(
                (x + 1) as f32 / width as f32,
                (y + 1) as f32 / height as f32,
                size as f32 / width as f32,
                size as f32 / height as f32,
            )
        });
        Atlas {
            width,
            height,
            rgba,
            rects,
            source: self.source.clone(),
        }
    }
}
fn tint(image: &mut TileImage, color: [u8; 3]) {
    for p in image.rgba.as_chunks_mut::<4>().0 {
        for i in 0..3 {
            p[i] = (u16::from(p[i]) * u16::from(color[i]) / 255) as u8;
        }
    }
}
fn composite(base: &TileImage, overlay: &TileImage) -> TileImage {
    let size = base.size.max(overlay.size);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let b = ((y * base.size / size * base.size + x * base.size / size) * 4) as usize;
            let o =
                ((y * overlay.size / size * overlay.size + x * overlay.size / size) * 4) as usize;
            let alpha = u32::from(overlay.rgba[o + 3]);
            for c in 0..3 {
                rgba.push(
                    ((u32::from(overlay.rgba[o + c]) * alpha
                        + u32::from(base.rgba[b + c]) * (255 - alpha)
                        + 127)
                        / 255) as u8,
                );
            }
            rgba.push(255);
        }
    }
    TileImage { size, rgba }
}
/// One packed CPU image with normalized regions for all ten tiles.
pub struct Atlas {
    /// Atlas width in pixels, including gutters.
    pub width: u32,
    /// Atlas height in pixels, including gutters.
    pub height: u32,
    /// Top-to-bottom RGBA8 pixels.
    pub rgba: Vec<u8>,
    /// Whole-tile UV regions in Tile::ALL order; gutters excluded.
    pub rects: [Vec4; 10],
    /// Explicit source path or built-in fallback label.
    pub source: String,
}
impl Atlas {
    /// Encode a temporary in-memory image for raylib or an explicitly chosen
    /// output file. Never writes a file automatically.
    pub fn png(&self) -> Result<Vec<u8>, TextureError> {
        let mut result = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut result, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|e| error("atlas", e))?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|e| error("atlas", e))?;
        }
        Ok(result)
    }
}
struct Source(PathBuf);
impl Source {
    fn open(path: &Path) -> Result<Self, TextureError> {
        if !path.is_dir() {
            return Err(error(
                &path.display().to_string(),
                "expected a PNG directory; extract installation textures first with scripts/import_minecraft_textures.py",
            ));
        }
        let nested = path.join("assets/minecraft/textures/block");
        if nested.is_dir() {
            return Ok(Self(nested));
        }
        if FILES
            .iter()
            .any(|name| path.join(format!("{name}.png")).is_file())
        {
            return Ok(Self(path.to_path_buf()));
        }
        Err(error(
            &path.display().to_string(),
            "no modern PNG block texture layout; choose an extracted pack root or block directory (legacy terrain.png/Bedrock packs are unsupported)",
        ))
    }
    fn read(&mut self, name: &str) -> Result<Vec<u8>, TextureError> {
        let file = self.0.join(format!("{name}.png"));
        let label = file.display().to_string();
        let reader = File::open(file).map_err(|e| {
            error(
                &label,
                format!("required demo texture missing/unreadable: {e}"),
            )
        })?;
        let mut bytes = Vec::new();
        reader
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| error(&label, e))?;
        if bytes.len() as u64 > MAX_FILE {
            return Err(error(&label, "PNG exceeds 4 MiB limit"));
        }
        Ok(bytes)
    }
}
mod fallback;
use fallback::fallback;
#[cfg(all(test, feature = "render"))]
mod native_tests;
#[cfg(test)]
mod tests;
