use super::*;

const FONT: &[u8] = include_bytes!("../../examples/fonts/LiberationSans-Regular.ttf");
fn entry(options: FontOptions) -> FontEntry {
    let options = options.normalized().unwrap();
    let outline = parse_outline(FONT).unwrap();
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
    for size in [0, 513] {
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
fn resolved_project_font_profiles_match_sdk_options() {
    use rayengine_core::manifest::{FontFilter, ProjectManifest};
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fonts/rayengine.toml");
    let manifest = ProjectManifest::load(&fixture).unwrap();
    let base = manifest.resolve(None).unwrap();
    let pixel = manifest.resolve(Some("pixel")).unwrap();
    assert!(base.settings.fonts["body"].path.is_absolute());
    assert!(base.settings.fonts["body"].path.is_file());
    let body = FontOptions::try_from(&base.settings.fonts["body"]).unwrap();
    assert_eq!(body.sampling, FontSampling::Smooth);
    assert_eq!(body.rasterization, FontRasterization::Adaptive);
    let body = FontOptions::try_from(&pixel.settings.fonts["body"]).unwrap();
    assert_eq!(body.sampling, FontSampling::Nearest);
    assert_eq!(body.rasterization, FontRasterization::Fixed);
    assert_eq!(body.raster_size, 16);
    assert_eq!(pixel.settings.fonts["body"].filter, FontFilter::Nearest);
    let mut declared = base.settings.fonts["body"].clone();
    declared.glyphs = Some(vec![65, 65, 233]);
    let options = FontOptions::try_from(&declared).unwrap();
    assert_eq!(options.glyphs, " ?Aé");
    let font = entry(options);
    assert_eq!(
        measure(&font, "é", TextStyle::new(FontId(0), 24.0)).missing_glyphs,
        0
    );
    declared.glyphs = Some(vec![0]);
    assert!(FontOptions::try_from(&declared).is_err());
    declared.glyphs = Some(vec![]);
    assert!(FontOptions::try_from(&declared).is_err());
    declared.glyphs = Some(vec![0xD800]);
    assert!(FontOptions::try_from(&declared).is_err());
}

#[test]
fn large_curved_glyphs_match_high_resolution_outline_reference() {
    // A separately configured high-resolution font is the reference. The old
    // 40px geometry approximation keeps identical metrics but makes these
    // 512px contours visibly polygonal, so bounds-only tests cannot catch it.
    let actual = parse_outline(FONT).unwrap();
    let reference = fontdue::Font::from_bytes(
        FONT,
        fontdue::FontSettings {
            scale: 512.0,
            load_substitutions: false,
            ..Default::default()
        },
    )
    .unwrap();
    for c in "OGSQDBPaeos&@089".chars() {
        for raster in [256.0, 512.0] {
            let (_, actual) = actual.rasterize(c, raster);
            let (_, expected) = reference.rasterize(c, raster);
            assert_eq!(actual.len(), expected.len());
            let differing_edges = actual
                .iter()
                .zip(&expected)
                .filter(|(a, b)| a.abs_diff(**b) > 20)
                .count();
            assert_eq!(
                differing_edges, 0,
                "{c} at {raster}px must retain smooth outline curves"
            );
        }
    }
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
