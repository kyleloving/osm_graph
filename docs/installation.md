# Installation

## Python

### From PyPI

```bash
pip install pysochrone
```

Wheels are provided for Python 3.8+ on Linux, macOS, and Windows (x86-64).

### From source

Requires [Rust](https://rustup.rs/) and [maturin](https://www.maturin.rs/).

```bash
git clone https://github.com/kyleloving/osm_graph.git
cd osm_graph
pip install maturin
maturin develop --release
```

`maturin develop` compiles the Rust extension and installs it into the current Python environment in one step.  The `--release` flag enables compiler optimisations — omit it only for debug builds.

## Rust

Add to `Cargo.toml`:

```toml
[dependencies]
osm-graph = "0.2.0"
```

> **Note:** The crate is published as `osm-graph`; the library module is `pysochrone` (matching the Python package name).

## Dependencies

pysochrone has no required Python dependencies — all heavy lifting is in Rust.

Optional Python packages used in the examples:

```bash
pip install folium branca
```
