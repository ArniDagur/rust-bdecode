#[macro_use]
extern crate criterion;
use criterion::{Benchmark, Criterion, Throughput};

macro_rules! bench {
    ($name:ident, $path:expr) => {
        fn $name(c: &mut Criterion) {
            let bytes = include_bytes!($path);

            c.bench(
                stringify!($name),
                Benchmark::new("bdecode", move |b| b.iter(|| ::bdecode::bdecode(bytes)))
                    .throughput(Throughput::Bytes(bytes.len() as u64)),
            );
        }
    };
}

bench!(
    k_on_complete_saga,
    "../props/[ToishY] K-ON - THE COMPLETE SAGA (BD 1920x1080 x.264 FLAC).torrent"
);
bench!(
    hibike_complete_saga,
    "../props/[ToishY] Hibike! Euphonium - THE COMPLETE SAGA (BD 1920x1080 x264 FLAC).torrent"
);
bench!(
    touhou_lossless_collection_19,
    "../props/Touhou lossless music collection.torrent"
);

criterion_group!(
    benches,
    k_on_complete_saga,
    hibike_complete_saga,
    touhou_lossless_collection_19
);
criterion_main!(benches);
