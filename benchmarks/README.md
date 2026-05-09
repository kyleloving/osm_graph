# Benchmarks

This directory keeps external comparison artifacts. The primary Rust benchmark
harness lives in `examples/benchmark.rs` so it can run with Cargo without adding
extra benchmark dependencies.

## Local PBF Pipeline

```powershell
cargo run --release --example benchmark -- data/district-of-columbia-latest.osm.pbf
```

Useful environment variables:

- `NETWORK=drive|walk|bike`
- `LIMITS=300,600,900`
- `LAT=38.9097`
- `LON=-77.0432`
- `ITERS=20`
- `WARMUP=5`
- `RETAIN_ALL=1`
- `PROFILE_LOOP=1`

The harness reports setup timings separately from steady-state hot-path timings
so graph loading improvements and query-time improvements are easy to compare.

## External Comparison

```powershell
python benchmarks/comparison.py
```

This compares graphways against OSMnx/NetworkX on cached graph workloads and
updates `benchmarks/performance.png` when plotting dependencies are installed.
