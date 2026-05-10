# Graphways

[![PyPI Version][pypi-version-badge]][pypi-link]
[![Crates.io Version][crates-version-badge]][crates-link]
[![Python Versions][python-versions-badge]][pypi-link]
[![CI][ci-badge]][ci-link]
[![Documentation][docs-badge]][docs-link]
[![License: MIT][license-badge]][license-link]

**Graphways** is a Rust-powered Python library for fast local reachability,
routing, and isochrone analysis on OpenStreetMap road networks. It builds a
reusable `SpatialGraph` once, then runs repeated network-aware queries directly
in process without deploying a routing server.

Graphways is designed for workflows that need the street network as a practical
analysis primitive: accessibility studies, urban analytics, site selection,
agent-based simulations, notebook exploration, and application backends that
need low-latency local graph operations.

![Graphways demo](docs/assets/graphways-demo.gif)

## Installation

Install the Python package from PyPI:

```bash
pip install graphways
```

Or add the Rust crate to `Cargo.toml`:

```toml
[dependencies]
graphways = "0.3.0"
```

To build the Python extension from source, install Rust and maturin, then run:

```bash
maturin develop
```

## Usage

```python
import graphways as gw

origin = (38.9097, -77.0432)
destination = (38.8977, -77.0365)

graph = gw.SpatialGraph.from_place(
    "Washington, DC",
    network="walk",
    max_dist=10_000,
)

isochrones = graph.isochrone(origin, minutes=[10, 20, 30])
route = graph.route(origin, destination)
reachable = graph.reachable(origin, minutes=15)
```

`SpatialGraph` is the central object. Reachability and network-time prism
queries return lightweight graph views over the parent graph, so inspection and
GeoJSON export do not copy the full road network.

```python
reachable.nodes()
reachable.edges_geojson()
reachable.route(origin, destination)

prism = graph.prism(
    origin=origin,
    destination=destination,
    max_minutes=45,
    stop_minutes=10,
    buffer_minutes=5,
)

possible_stops = prism.nodes()
slack_polygon = prism.slack_polygon(min_slack_s=5 * 60)
```

All geometry-producing methods return GeoJSON strings for easy use with common
Python mapping and data-analysis tools.

## Features

- Build reusable walking, biking, driving, and custom-access OSM road graphs.
- Load from Overpass XML, existing OSM XML strings, or local OSM PBF files.
- Query nearest nodes with an R-tree spatial index.
- Compute reachability over the road network from a single origin.
- Generate isochrones with one graph search and triangulated contour extraction.
- Route point-to-point with distance, duration, geometry, and cumulative times.
- Build network-time prisms for "what can I visit between A and B?" analysis.
- Export nodes, edges, routes, POIs, and isochrones as GeoJSON.

## Documentation

Start with the [quickstart][docs-quickstart], then see the
[Python graph API][docs-python-graph] and [Rust API notes][docs-rust-api].

The examples directory and documentation show common workflows:

- building graphs from places, XML, and PBF files
- computing walking and driving isochrones
- routing between coordinates
- querying reachable nodes and graph views
- working with POIs and GeoJSON output

## Performance

Graphways is optimized for repeated local queries over the same area. Graph
construction is paid once; subsequent reachability, routing, and isochrone
queries reuse the in-memory graph and spatial index.

The benchmark suite compares steady-state graphways queries against NetworkX /
OSMnx baselines and includes staged timings for graph construction versus
per-query work:

```bash
python benchmarks/comparison.py
```

Current benchmarks are intentionally kept in `benchmarks/` rather than treated
as a universal claim. Performance depends on graph size, network profile,
machine, cache state, and output geometry settings.

## Project Status

Graphways is pre-1.0. The core API is converging around:

```python
graph = gw.SpatialGraph.from_place("Washington, DC", network="walk")
iso = graph.isochrone(origin, minutes=[10, 20, 30])
route = graph.route(origin, destination)
reachable = graph.reachable(origin, minutes=15)
prism = graph.prism(origin, destination, max_minutes=45)
```

Known areas still under active development include turn restrictions,
profile-specific access rules, disconnected-region polygon output, richer
benchmark staging, and larger-extract graph storage.

## Relationship to Other Tools

Use **OSMnx** for mature Python-first urban network analysis and broad OSM data
workflows. Use **Valhalla**, **OSRM**, or **GraphHopper** for production routing
servers. Use **Graphways** when you want fast local OSM reachability, routing,
and isochrones from Python without standing up infrastructure.

## Citation

Graphways does not yet have an academic citation. If you use it in research,
please cite the repository for now:

```text
Loving, K. Graphways: fast local OpenStreetMap reachability, routing,
and isochrones from Python, powered by Rust.
https://github.com/kyleloving/graphways
```

## License

Graphways is open source and licensed under the MIT license. OpenStreetMap data
is licensed separately; when using OSM-derived outputs, follow the
[OpenStreetMap attribution guidelines][osm-copyright].

<!-- badges -->

[ci-badge]: https://github.com/kyleloving/graphways/actions/workflows/ci.yml/badge.svg
[ci-link]: https://github.com/kyleloving/graphways/actions/workflows/ci.yml
[crates-link]: https://crates.io/crates/graphways
[crates-version-badge]: https://img.shields.io/crates/v/graphways
[docs-badge]: https://github.com/kyleloving/graphways/actions/workflows/docs.yml/badge.svg
[docs-link]: https://kyleloving.github.io/graphways/
[license-badge]: https://img.shields.io/badge/license-MIT-green
[license-link]: https://github.com/kyleloving/graphways/blob/main/LICENSE
[pypi-link]: https://pypi.org/project/graphways/
[pypi-version-badge]: https://img.shields.io/pypi/v/graphways
[python-versions-badge]: https://img.shields.io/pypi/pyversions/graphways

<!-- links -->

[docs-python-graph]: https://kyleloving.github.io/graphways/python-graph/
[docs-quickstart]: https://kyleloving.github.io/graphways/quickstart/
[docs-rust-api]: https://kyleloving.github.io/graphways/rust-api/
[osm-copyright]: https://www.openstreetmap.org/copyright
