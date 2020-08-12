#[macro_use]
extern crate criterion;

use criterion::{BenchmarkId, Criterion, Throughput};
use criterion_cycles_per_byte::CyclesPerByte;

use std::time::Duration;

const K_ON: &[u8] =
    include_bytes!("../props/[ToishY] K-ON - THE COMPLETE SAGA (BD 1920x1080 x.264 FLAC).torrent");
const HIBIKE: &[u8] = include_bytes!(
    "../props/[ToishY] Hibike! Euphonium - THE COMPLETE SAGA (BD 1920x1080 x264 FLAC).torrent"
);
const TOUHOU: &[u8] = include_bytes!("../props/Touhou lossless music collection.torrent");

const TEN_SECONDS: Duration = Duration::from_secs(10);

fn bench(c: &mut Criterion<CyclesPerByte>) {
    let mut group = c.benchmark_group("bdecode");

    for &(name, bytes) in &[("K-ON", K_ON), ("HIBIKE", HIBIKE), ("TOUHOU", TOUHOU)] {
        group.measurement_time(TEN_SECONDS);
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function(BenchmarkId::new("parse", name), |b| {
            b.iter(|| ::bdecode::bdecode(bytes));
        });
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default().with_measurement(CyclesPerByte);
    targets = bench
);
criterion_main!(benches);
