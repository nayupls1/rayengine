use super::*;

const FONT: &[u8] = include_bytes!("../../examples/fonts/LiberationSans-Regular.ttf");
fn entry(options: FontOptions) -> FontEntry {
    let options = options.normalized().unwrap();
    let outline = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default()).unwrap();
    let coverage = options
        .glyphs
        .chars()
        .filter(|c| outline.has_glyph(*c))
        .collect();
    FontEntry {
        outline,
        options,
        coverage,
        atlases: BTreeMap::new(),
    }
}
fn measure(entry: &FontEntry, text: &str, style: TextStyle) -> TextMetrics {
    style.validate(text).unwrap();
    entry.layout(text, style, |_, _| {}).unwrap()
}
#[test]
fn coverage_validation_and_adaptive_buckets() {
    let options = FontOptions {
        glyphs: "BAA".into(),
        ..Default::default()
    };
    assert_eq!(options.normalized().unwrap().glyphs, " ?AB");
    for size in [0, 7, 513] {
        assert!(
            FontOptions {
                raster_size: size,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        FontOptions {
            glyphs: "A\0".into(),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        FontOptions {
            glyphs: (0x1000..0x1500).filter_map(char::from_u32).collect(),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    let options = FontOptions::default();
    assert_eq!(options.atlas_size(24.0).unwrap(), 32);
    assert_eq!(options.atlas_size(33.0).unwrap(), 64);
    assert_eq!(options.atlas_size(129.0).unwrap(), 256);
    assert!(options.atlas_size(513.0).is_err());
    assert!(options.atlas_size(f32::NAN).is_err());
    assert_eq!(
        FontOptions {
            rasterization: FontRasterization::Fixed,
            ..options
        }
        .atlas_size(4096.0)
        .unwrap(),
        32
    );
}
#[test]
fn shared_layout_spacing_multiline_and_fallback() {
    let entry = entry(FontOptions::default());
    let style = TextStyle::new(FontId(0), 24.0);
    assert_eq!(measure(&entry, "", style), TextMetrics::default());
    let base = measure(&entry, "Hello", style);
    let spaced = measure(
        &entry,
        "Hello",
        TextStyle {
            spacing: 2.0,
            ..style
        },
    );
    assert!((spaced.size.x - base.size.x - 8.0).abs() < 0.0001);
    let multiline = measure(
        &entry,
        "Hi\nHello",
        TextStyle {
            line_spacing: 5.0,
            ..style
        },
    );
    assert_eq!(multiline.size.x, base.size.x);
    assert!(multiline.size.y > base.size.y * 2.0);
    let replacement = measure(&entry, "?", style);
    let missing = measure(&entry, "🦀", style);
    assert_eq!(missing.size, replacement.size);
    assert_eq!(missing.ink_bounds, replacement.ink_bounds);
    assert_eq!(missing.missing_glyphs, 1);
    assert_eq!(
        measure(&entry, "   ", style).ink_bounds,
        TextMetrics::default().ink_bounds
    );
    for size in [16.0, 24.0, 40.0] {
        let metrics = measure(&entry, "Ag", TextStyle::new(FontId(0), size));
        assert!(metrics.ink_bounds.min.y > 0.0);
        assert!(metrics.ink_bounds.max.y > size * 0.8);
        assert!(
            (metrics.size.x / size - measure(&entry, "Ag", style).size.x / style.size).abs()
                < 0.0001
        );
    }
    for size in [0.0, -1.0, f32::NAN, 513.0] {
        assert!(TextStyle::new(FontId(0), size).validate("A").is_err());
    }
    assert!(style.validate("NUL\0").is_err());
}
#[test]
fn manifest_schema_options_and_relative_paths() {
    let directory = std::env::current_dir().unwrap().join("example");
    let source = "schema_version = 1\n[window]\ntitle = 'ignored by font reader'\n[fonts.body]\npath = 'assets/body.ttf'\n[fonts.body.options]\nsampling = 'nearest'\nraster_size = 16\n";
    let declarations = FontDeclarations::parse(source, &directory).unwrap();
    assert_eq!(
        declarations.fonts["body"].path,
        directory.join("assets/body.ttf")
    );
    assert_eq!(
        declarations.fonts["body"].options.sampling,
        FontSampling::Nearest
    );
    for source in [
        "schema_version = 2",
        "schema_version = 1\n[fonts.body]\npath = ''",
        "schema_version = 1\n[fonts.body]\npath='a.ttf'\nunknown=1",
        "schema_version = 1\n[fonts.body]\npath='a.ttf'\n[fonts.body.options]\nsampler='nearest'",
        "schema_version = 1\n[fonts.body]\npath='a.ttf'\n[fonts.body.options]\nraster_size=0",
    ] {
        assert!(
            FontDeclarations::parse(source, &directory).is_err(),
            "{source}"
        );
    }
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fonts/rayengine.toml");
    let declarations = FontDeclarations::load(&fixture).unwrap();
    assert!(declarations.fonts["body"].path.is_absolute());
    assert!(declarations.fonts["body"].path.is_file());
}
#[test]
fn corrupt_fonts_and_stale_measurement_are_errors() {
    assert!(
        fontdue::Font::from_bytes(b"invalid font".as_slice(), fontdue::FontSettings::default())
            .is_err()
    );
    let fonts = FontAssets::new();
    assert!(
        fonts
            .measure("Hello", TextStyle::new(FontId(0), 24.0))
            .is_err()
    );
    assert_eq!(fonts.usage(), (0, 0, 0));
}
