# graphways

**Fast isochrones, routing, and POI lookups from OpenStreetMap — written in Rust, callable from Python.**

[![PyPI](https://img.shields.io/pypi/v/graphways)](https://pypi.org/project/graphways/)
[![Crates.io](https://img.shields.io/crates/v/graphways)](https://crates.io/crates/graphways)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

---

## What it does

graphways queries OpenStreetMap, builds a road-network graph, and gives you:

- **Isochrones** — polygons bounding everything reachable within a time limit
- **Point-to-point routing** — A\* routes with per-waypoint cumulative travel times
- **POI fetching** — amenities, shops, and other features within any isochrone
- **Graph introspection** — inspect nodes, edges, and the network structure directly

All GeoJSON output. All cached. Typically 5–6× faster than osmnx for repeated queries.

---

## 30-second start

```python
import graphways

# Build the graph once — subsequent calls for the same area hit the cache
graph = graphways.build_graph(48.137144, 11.575399, "Drive", max_dist=10_000)

# Isochrones from the same point
isos = graph.isochrone((48.137144, 11.575399), minutes=[5, 10, 15, 20])

# Route to somewhere else
route = graph.route((48.137144, 11.575399), (48.154560, 11.530840))

# What's reachable?
pois = graph.fetch_pois(isos[0])
```

---

## Features at a glance

| Feature | Detail |
|---------|--------|
| Graph construction | Parses OSM XML into a petgraph `DiGraph` |
| Simplification | Collapses linear chains, deduplicates parallel edges — ~89% node/edge reduction |
| Spatial index | R-tree for O(log n) nearest-node lookups |
| Isochrones | Single Dijkstra pass; hull computation parallelised across time limits |
| Routing | A\* with admissible straight-line heuristic |
| Isochrone geometry | Triangulated travel-time contours |
| Network types | Drive · DriveService · Walk · Bike · All · AllPrivate |
| Caching | 3-level: disk XML → in-memory XML → in-memory graph |
| Python bindings | Full PyO3 bindings with type stubs |

---

## Performance

Benchmarks on the Munich road network (cached, no network I/O), Intel Core i7-11370H.
Single Dijkstra pass compared against osmnx with a pre-enriched graph.

| Radius | Nodes | Edges | graphways | osmnx | Speedup |
|-------:|------:|------:|-----------:|------:|--------:|
| 5 000 m | 6 251 | 15 356 | 0.030 s | 0.190 s | **6.3×** |
| 10 000 m | 16 183 | 41 601 | 0.064 s | 0.365 s | **5.7×** |
| 20 000 m | 32 501 | 82 385 | 0.092 s | 0.455 s | **4.9×** |

The gap reflects compiled Rust and petgraph's flat adjacency list vs pure-Python NetworkX.
graphways's cache means graph construction is a one-time cost; the table shows steady-state performance for repeated queries over the same region.
