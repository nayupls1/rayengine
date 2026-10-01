//! Stable CPU atlas/decode workloads; fixture creation and output drops are untimed.
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayengine_minecraft::textures::{TextureSet, Tile, TileImage, decode_tile};
use std::{hint::black_box, time::Duration};
fn encode(size: u32, indexed: bool) -> Vec<u8> {
    let tile = TextureSet::fallback();
    let source = tile.tile(Tile::GrassTop);
    let channels = if indexed { 1 } else { 4 };
    let mut pixels = Vec::new();
    for y in 0..size {
        for x in 0..size {
            let i = (((y % 16) * 16 + x % 16) * 4) as usize;
            pixels.extend_from_slice(&source.rgba()[i..i + channels]);
        }
    }
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, size, size);
        encoder.set_depth(png::BitDepth::Eight);
        if indexed {
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_palette((0..=255).flat_map(|v| [v, v, v]).collect::<Vec<u8>>());
        } else {
            encoder.set_color(png::ColorType::Rgba);
        }
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    data
}
fn workloads(c: &mut Criterion) {
    let mut group = c.benchmark_group("minecraft_textures_v1");
    for (name, size, indexed) in [
        ("decode_rgba", 16, false),
        ("decode_rgba", 256, false),
        ("decode_indexed", 16, true),
    ] {
        let png = encode(size, indexed);
        let decoded = decode_tile(&png, "fixture").unwrap();
        assert_eq!(decoded.rgba().len(), (size * size * 4) as usize);
        group.throughput(Throughput::Bytes(u64::from(size) * u64::from(size) * 4));
        group.bench_function(BenchmarkId::new(name, size), |b| {
            b.iter_batched(
                || (),
                |()| black_box(decode_tile(black_box(&png), "fixture").unwrap()),
                BatchSize::SmallInput,
            )
        });
    }
    let fallback = TextureSet::fallback();
    let atlas = fallback.pack();
    assert_eq!((atlas.width, atlas.height), (90, 36));
    group.throughput(Throughput::Bytes(atlas.rgba.len() as u64));
    group.bench_function("pack_fallback_16", |b| {
        b.iter_batched(
            || (),
            |()| black_box(black_box(&fallback).pack()),
            BatchSize::SmallInput,
        )
    });
    group.bench_function("encode_fallback_90x36", |b| {
        b.iter_batched(
            || (),
            |()| black_box(black_box(&atlas).png().unwrap()),
            BatchSize::SmallInput,
        )
    });
    let tile = TileImage::new(256, [100, 160, 50, 255].repeat(256 * 256)).unwrap();
    let maximum = TextureSet::new(std::array::from_fn(|_| tile.clone()), "maximum fixture");
    let atlas = maximum.pack();
    assert_eq!((atlas.width, atlas.height), (1290, 516));
    group.throughput(Throughput::Bytes(atlas.rgba.len() as u64));
    group.bench_function("pack_maximum_256", |b| {
        b.iter_batched(
            || (),
            |()| black_box(black_box(&maximum).pack()),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}
criterion_group! {name=benches;config=Criterion::default().warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));targets=workloads}
criterion_main!(benches);
