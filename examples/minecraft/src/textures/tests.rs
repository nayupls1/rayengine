use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rayengine-textures-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn encode(size: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, size, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixel.repeat((size * height) as usize))
            .unwrap();
    }
    data
}
fn fixture() -> Directory {
    let dir = Directory::new();
    for (i, name) in FILES.into_iter().enumerate() {
        std::fs::write(
            dir.0.join(format!("{name}.png")),
            encode(
                16,
                16,
                if i == 10 {
                    [255, 255, 255, 128]
                } else {
                    [255, 255, 255, 255]
                },
            ),
        )
        .unwrap();
    }
    dir
}
#[test]
fn directory_and_extracted_pack_load_the_same_subset() {
    let source = fixture();
    let direct = TextureSet::load(&source.0).unwrap();
    let root = Directory::new();
    let blocks = root.0.join("assets/minecraft/textures/block");
    std::fs::create_dir_all(&blocks).unwrap();
    for name in FILES {
        let file = format!("{name}.png");
        std::fs::copy(source.0.join(&file), blocks.join(&file)).unwrap();
    }
    let loaded = TextureSet::load(&root.0).unwrap();
    for tile in Tile::ALL {
        assert_eq!(loaded.tile(tile).rgba(), direct.tile(tile).rgba());
    }
    assert_eq!(&direct.tile(Tile::GrassTop).rgba()[..3], &GRASS_TINT);
    assert_eq!(&direct.tile(Tile::Leaves).rgba()[..3], &LEAF_TINT);
    // Half-alpha overlay blends the tinted grass with the white side base.
    assert_eq!(
        &direct.tile(Tile::GrassSide).rgba()[..4],
        &[180, 215, 160, 255]
    );
    // Loading leaves the extracted source files untouched.
    assert_eq!(std::fs::read_dir(&source.0).unwrap().count(), 11);
}
#[test]
fn missing_invalid_unsupported_and_oversized_sources_have_named_errors() {
    let dir = fixture();
    std::fs::remove_file(dir.0.join("oak_log_top.png")).unwrap();
    assert!(
        TextureSet::load(&dir.0)
            .err()
            .unwrap()
            .to_string()
            .contains("oak_log_top.png")
    );
    std::fs::write(dir.0.join("oak_log_top.png"), b"not a PNG").unwrap();
    assert!(
        TextureSet::load(&dir.0)
            .err()
            .unwrap()
            .to_string()
            .contains("oak_log_top.png")
    );
    for (w, h) in [(16, 32), (15, 15), (512, 512)] {
        assert!(
            decode_tile(&encode(w, h, [255; 4]), "bad.png")
                .unwrap_err()
                .to_string()
                .contains("unsupported PNG")
        );
    }
    assert!(
        decode_tile(&vec![0; MAX_FILE as usize + 1], "huge.png")
            .unwrap_err()
            .to_string()
            .contains("4 MiB")
    );
    let empty = Directory::new();
    std::fs::write(empty.0.join("terrain.png"), encode(16, 16, [255; 4])).unwrap();
    assert!(
        TextureSet::load(&empty.0)
            .err()
            .unwrap()
            .to_string()
            .contains("legacy terrain.png")
    );
    for file in ["absent", "client.jar"] {
        if file.ends_with("jar") {
            std::fs::write(empty.0.join(file), b"archive").unwrap();
        }
        assert!(
            TextureSet::load(empty.0.join(file))
                .err()
                .unwrap()
                .to_string()
                .contains("extract installation textures first")
        );
    }
    let incomplete = fixture();
    std::fs::remove_file(incomplete.0.join("stone.png")).unwrap();
    assert!(
        TextureSet::load(&incomplete.0)
            .err()
            .unwrap()
            .to_string()
            .contains("stone.png")
    );
}
#[test]
fn packing_keeps_upright_rows_duplicates_gutters_and_resamples_nearest() {
    let pixels = (0..32)
        .flat_map(|y| (0..32).flat_map(move |x| [x as u8, y as u8, 10, 255]))
        .collect();
    let large = TileImage::new(32, pixels).unwrap();
    let small = TileImage::new(16, [10, 20, 30, 255].repeat(256)).unwrap();
    let set = TextureSet::new(
        std::array::from_fn(|i| if i == 1 { large.clone() } else { small.clone() }),
        "fixture",
    );
    let atlas = set.pack();
    assert_eq!((atlas.width, atlas.height), (170, 68));
    let p = |x: u32, y: u32| -> &[u8] {
        let i = ((y * atlas.width + x) * 4) as usize;
        &atlas.rgba[i..i + 4]
    };
    assert_eq!(p(35, 1), &[0, 0, 10, 255]);
    assert_eq!(p(66, 32), &[31, 31, 10, 255]);
    assert_eq!(p(34, 0), p(35, 1));
    assert_eq!(p(67, 33), p(66, 32));
    assert_eq!(p(1, 1), &[10, 20, 30, 255]);
    assert_eq!(p(32, 32), &[10, 20, 30, 255]);
    for rect in atlas.rects {
        assert!(rect.x >= 0.0 && rect.y >= 0.0 && rect.x + rect.z <= 1.0 && rect.y + rect.w <= 1.0);
    }
    let png = atlas.png().unwrap();
    let mut reader = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    assert_eq!(pixels, atlas.rgba);
}
#[test]
fn fallback_is_original_deterministic_and_has_cutout_foliage() {
    let first = TextureSet::fallback();
    let second = TextureSet::fallback();
    for tile in Tile::ALL {
        assert_eq!(first.tile(tile).rgba(), second.tile(tile).rgba());
        let alpha: Vec<_> = first
            .tile(tile)
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        if tile == Tile::Leaves {
            assert!(alpha.contains(&0) && alpha.contains(&255));
        } else {
            assert!(alpha.iter().all(|&v| v == 255));
        }
    }
    assert_ne!(
        first.tile(Tile::GrassTop).rgba(),
        first.tile(Tile::GrassSide).rgba()
    );
    assert_ne!(
        first.tile(Tile::LogTop).rgba(),
        first.tile(Tile::LogSide).rgba()
    );
}
#[test]
fn palette_gray_rgb_and_alpha_pngs_normalize_to_rgba() {
    for color in [
        png::ColorType::Grayscale,
        png::ColorType::GrayscaleAlpha,
        png::ColorType::Rgb,
    ] {
        let pixel = match color {
            png::ColorType::Grayscale => vec![100],
            png::ColorType::GrayscaleAlpha => vec![100, 51],
            _ => vec![100, 101, 102],
        };
        let mut data = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut data, 16, 16);
            encoder.set_color(color);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&pixel.repeat(256))
                .unwrap();
        }
        let result = decode_tile(&data, "color.png").unwrap();
        let expected = match color {
            png::ColorType::Grayscale => [100, 100, 100, 255],
            png::ColorType::GrayscaleAlpha => [100, 100, 100, 51],
            _ => [100, 101, 102, 255],
        };
        assert_eq!(&result.rgba()[..4], &expected);
    }
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, 16, 16);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::One);
        encoder.set_palette(vec![100, 101, 102, 50, 51, 52]);
        encoder.set_trns(vec![0, 255]);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[0; 32])
            .unwrap();
    }
    assert_eq!(
        &decode_tile(&data, "indexed.png").unwrap().rgba()[..4],
        &[100, 101, 102, 0]
    );
}

#[test]
fn terrain_maps_grass_sides_bottom_and_log_end_faces() {
    use crate::terrain::{GENERATOR_VERSION, Terrain, TerrainSettings};
    use rayengine_voxel::Face;
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let registry = terrain.registry();
    let grass = registry.get(terrain.blocks().grass).unwrap();
    let wood = registry.get(terrain.blocks().wood).unwrap();
    for face in Face::ALL {
        assert_eq!(
            grass.texture(face),
            match face {
                Face::PosY => Tile::GrassTop.id(),
                Face::NegY => Tile::Dirt.id(),
                _ => Tile::GrassSide.id(),
            }
        );
        assert_eq!(
            wood.texture(face),
            match face {
                Face::PosY | Face::NegY => Tile::LogTop.id(),
                _ => Tile::LogSide.id(),
            }
        );
    }
    assert_eq!(GENERATOR_VERSION, 1); // Render keys do not change generated block IDs.
}
