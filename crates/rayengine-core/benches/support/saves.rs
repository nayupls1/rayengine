//! Stable payload workloads; filesystem benchmarks require explicit opt-in.

use criterion::{BenchmarkId, Criterion, Throughput};
use rayengine_core::save::{self, Durability, SaveLimits, SaveOptions};
use std::{
    fs,
    hint::black_box,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn containers(c: &mut Criterion) {
    let limits = SaveLimits::default();
    let mut group = c.benchmark_group("save_container");
    for size in [1_024, 65_536, 1_048_576] {
        let payload = vec![0x5a; size];
        let mut output = save::encode(7, &payload, limits).unwrap();
        let encoded = output.clone();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(BenchmarkId::new("encode_new", size), |b| {
            b.iter(|| black_box(save::encode(7, black_box(&payload), limits).unwrap()));
        });
        group.bench_function(BenchmarkId::new("encode_reused", size), |b| {
            b.iter(|| {
                save::encode_into(&mut output, 7, black_box(&payload), limits).unwrap();
                black_box(&output);
            });
        });
        group.bench_function(BenchmarkId::new("decode_borrowed", size), |b| {
            b.iter(|| black_box(save::decode(black_box(&encoded), limits).unwrap()));
        });
    }
    group.finish();
}

pub fn files(c: &mut Criterion) {
    if std::env::var("RAYENGINE_SAVE_IO_BENCH").as_deref() != Ok("1") {
        return;
    }
    let directory = Fixture::new();
    let mut group = c.benchmark_group("save_file");
    group
        .sample_size(10)
        .warm_up_time(Duration::from_millis(250))
        .measurement_time(Duration::from_secs(1));
    for size in [65_536, 1_048_576] {
        let payload = vec![0x5a; size];
        let path = directory.0.join(format!("{size}.save"));
        let mut modes = vec![("replace_atomic", Durability::Atomic)];
        if cfg!(target_os = "linux") {
            modes.push(("replace_durable", Durability::Durable));
        }
        group.throughput(Throughput::Bytes(size as u64));
        for (name, durability) in modes {
            let options = SaveOptions {
                durability,
                ..SaveOptions::default()
            };
            save::save(&path, 7, &payload, options).unwrap();
            group.bench_function(BenchmarkId::new(name, size), |b| {
                b.iter(|| {
                    save::save(black_box(&path), 7, black_box(&payload), options).unwrap();
                });
            });
        }
        group.bench_function(BenchmarkId::new("load_cached", size), |b| {
            b.iter(|| black_box(save::load(black_box(&path), SaveLimits::default()).unwrap()));
        });
    }
    group.finish();
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let base = std::env::var_os("RAYENGINE_SAVE_BENCH_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("artifacts/benchmarks/save-io-fixtures"));
        fs::create_dir_all(&base).expect("create benchmark base directory");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = base.join(format!("run-{}-{timestamp}", std::process::id()));
        // Own only a newly created directory; never remove someone else's files.
        fs::create_dir(&path).expect("create unique benchmark directory");
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
