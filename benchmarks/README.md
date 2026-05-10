# Benchmarks

This directory keeps benchmark and external comparison artifacts.

## Local PBF Pipeline

`benchmark.rs` is a Rust harness for profiling the local PBF pipeline. It
measures one-shot setup costs separately from steady-state hot-path timings.
It is kept in `benchmarks/` as a repo-local profiling tool rather than as part
of the published Rust crate.

Useful environment variables:

- `NETWORK=drive|walk|bike`
- `LIMITS=300,600,900`
- `LAT=38.9097`
- `LON=-77.0432`
- `ITERS=20`
- `WARMUP=5`
- `RETAIN_ALL=1`
- `PROFILE_LOOP=1`

## External Comparison

```powershell
python benchmarks/comparison.py
python benchmarks/comparison.py --skip-r5py
python benchmarks/comparison.py --pbf C:\path\to\extract.osm.pbf
```

This runs three sections:

- a steady-state graphways vs NetworkX comparison on pre-warmed OSM graphs
- a graphways-only split between cached XML graph construction and repeated query cost
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
