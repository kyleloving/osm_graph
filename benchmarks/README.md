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
python benchmarks/comparison.py --skip-r5py
python benchmarks/comparison.py --pbf C:\path\to\extract.osm.pbf
```

This runs three sections:

- a steady-state graphways vs NetworkX comparison on pre-warmed OSM graphs
- a graphways-only split between cached graph lookup and repeated query cost
- an optional r5py comparison when `--pbf` is supplied and r5py is installed

The chart uses the headline comparison and updates `benchmarks/performance.png`
when plotting dependencies are installed.

The r5py section also requires a compatible Java JDK. If r5py imports but the
JVM fails to start, check `java -version` and `JAVA_HOME`; on Windows, a
conda/mamba environment from `conda-forge` is usually the least fussy setup.

## Routing Engine Comparison

```powershell
python benchmarks/engines/engines.py --pbf C:\path\to\munich.osm.pbf
```

This optional harness compares steady-state route latency against OSRM and
Valhalla, plus isochrone latency against Valhalla. Engine setup is documented in
`benchmarks/engines/README.md`; both engines require preprocessing before they
can serve requests.
