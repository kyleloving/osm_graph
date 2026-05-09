# Graphways

Fast OpenStreetMap reachability, routing, and isochrones from Python, powered by Rust - no routing server required.

Graphways was formerly `osm_graph` / `pysochrone`. The project was renamed to reflect its focus on spatial graph routing and reachability.

Graphways builds reusable local road-network graphs from OpenStreetMap data, then runs reachability, isochrone, routing, and POI workflows directly in process.

![Graphways demo](docs/assets/graphways-demo.gif)

## Features
- **Graph Construction:** Parses OpenStreetMap data to construct a directed graph representing the road network.
- **Graph Simplification:** Topologically simplifies the graph by collapsing linear chains and deduplicating parallel edges, reducing node/edge count by ~89% for faster downstream computation.
- **Spatial Indexing:** R-tree spatial index for O(log n) nearest-node lookups, built once and reused for all queries.
- **Isochrone Calculation:** Generates isochrones using a single Dijkstra traversal and triangulated contour extraction.
- **Routing:** A* point-to-point routing returning a GeoJSON LineString with distance, total duration, and cumulative travel times at each waypoint.
- **Geocoding:** Place-name to coordinate lookup via Nominatim.
- **Caching:** Three-level cache (disk XML → in-memory XML → in-memory graph) so repeated queries for the same area skip the network entirely, persisting across process restarts.
- **Python Integration:** Python bindings for all core functionality — isochrones, routing, geocoding, and cache management.
- **GeoJSON Output:** All results returned as GeoJSON strings for easy integration with mapping tools and data science workflows.

## Installation
To use the library in Rust, add it to your Cargo.toml:

```toml
[dependencies]
graphways = "0.2.0"
```

For Python:

```bash
pip install graphways
```

Or build from source with Rust and maturin installed:

```bash
maturin develop
```

## Usage

### Python

**Isochrones**
```python
import graphways as gw

graph = gw.SpatialGraph.from_place("Washington, DC", "Walk")
isochrones = graph.isochrone((38.9097, -77.0432), minutes=[10, 20, 30])
# Returns a list of GeoJSON geometry strings, one per time limit
```

**Routing**
```python
route = graphways.calc_route(
    48.137144, 11.575399,   # origin lat, lon
    48.154560, 11.530840,   # destination lat, lon
    "Drive",
)
# Returns a GeoJSON Feature (LineString) with properties:
#   distance_m          – total distance in metres
#   duration_s          – total travel time in seconds
#   cumulative_times_s  – travel time at each waypoint (parallel to coordinates)
```

**Geocoding**
```python
lat, lon = graphways.geocode("Marienplatz, Munich, Germany")
```

**Points of interest**
```python
# Pass any isochrone string from calc_isochrones
pois = graphways.fetch_pois(isochrones[0])
# Returns a GeoJSON FeatureCollection; each feature carries raw OSM tags as properties
```

**Cache management**
```python
graphways.cache_dir()    # path to the on-disk XML cache
graphways.clear_cache()  # clear both in-memory and disk caches
```

### Rust

```rust
use graphways::isochrone::calculate_isochrones_from_point;
use graphways::overpass::NetworkType;

#[tokio::main]
async fn main() {
    let (isochrones, _graph) = calculate_isochrones_from_point(
        48.137144,
        11.575399,
        Some(10_000.0),                                        // max_dist in metres; None = auto
        vec![300.0, 600.0, 900.0, 1_200.0, 1_500.0, 1_800.0],
        NetworkType::Drive,
        false,                                                 // false = simplified (faster)
    )
    .await
    .unwrap();
}
```

## Performance

Benchmarks run on Munich road network, cached data only (no network I/O), Intel Core i7-11370H. Compared against OSMnx using a pre-enriched graph and a single NetworkX Dijkstra pass.

The comparison measures steady-state isochrone computation after graph construction. Graphways computes network reachability, extracts triangulated contour polygons, and serializes them to GeoJSON. The OSMnx baseline computes NetworkX travel times and wraps reachable nodes in a convex hull, which is simpler geometry and should be read as a conservative baseline rather than an identical output-quality comparison.

Local Rust pipeline benchmark:

```bash
cargo run --release --example benchmark -- data/district-of-columbia-latest.osm.pbf
```

External OSMnx comparison:

```bash
python benchmarks/comparison.py
```

| Radius  |  Nodes |  Edges | graphways |     osmnx | Speedup |
|--------:|-------:|-------:|-----------:|----------:|--------:|
|  5,000m |  6,251 | 15,356 |     0.016s |    0.511s |   32.5x |
| 10,000m | 16,183 | 41,601 |     0.053s |    0.983s |   18.6x |
| 20,000m | 32,501 | 82,385 |     0.064s |    0.944s |   14.7x |

The speedup reflects compiled Rust graph traversal, reusable in-memory graph state, and triangulated contour extraction. OSMnx is a mature, full-featured urban network analysis library; Graphways is optimized for fast local reachability, routing, and isochrone queries from Python without a routing server.

Graphways' caching model means graph construction and edge enrichment are one-time costs paid on the first query for an area. Subsequent queries reuse the in-memory graph directly, so the numbers above represent steady-state performance for repeated queries over the same region.

![Performance comparison](benchmarks/performance.png)

## Roadmap

Done:

- [x] Walking, biking, and driving network profiles.
- [x] Reusable `SpatialGraph` API for repeated local queries.
- [x] Point-to-point routing with distance, duration, and route geometry.
- [x] Reachability queries over the road network.
- [x] Isochrones from one Dijkstra traversal plus triangulated contour extraction.
- [x] Topological graph simplification for smaller routing graphs.
- [x] PBF loading for offline workflows.
- [x] Benchmark harnesses for internal timings and OSMnx comparison.
- [x] Basic CI for formatting, Clippy, tests, Python import smoke checks, and wheel builds.

Near-term:

- [ ] Broaden correctness tests for one-way streets, access rules, snapping, disconnected graphs, and profile-specific behavior.
- [ ] Improve turn restriction support.
- [ ] Add user-configurable speed/profile overrides.
- [ ] Return richer polygon output, including MultiPolygon support for disconnected reachable regions.
- [ ] Expose lower-level benchmark stages for search, contour extraction, serialization, and payload size.
- [ ] Polish Python ergonomics around lowercase network names and `import graphways as gw` examples.

Later:

- [ ] Compact graph storage for larger regional extracts.
- [ ] More advanced caching controls for dynamic query parameters.
- [ ] Additional network analytics built on `SpatialGraph`.
- [ ] Interactive visualization helpers for notebooks and web maps.
- [ ] Optional integrations with external geocoding, OSM, or map-provider APIs.

## Contributing
Contributions are welcome! Please submit pull requests, open issues for discussion, and suggest new features or improvements.

## License
This library is licensed under MIT License.
