# graphways

**Fast isochrones, routing, and POI lookups from OpenStreetMap -- written in Rust, callable from Python.**

[![PyPI](https://img.shields.io/pypi/v/graphways)](https://pypi.org/project/graphways/)
[![Crates.io](https://img.shields.io/crates/v/graphways)](https://crates.io/crates/graphways)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

---

## What it does

graphways queries OpenStreetMap, builds a road-network graph, and gives you:

- **Isochrones** -- polygons bounding everything reachable within a time limit
- **Point-to-point routing** -- A* routes with per-waypoint cumulative travel times
- **POI fetching** -- amenities, shops, and other features within any isochrone
- **Graph introspection** -- inspect nodes, edges, and the network structure directly

Routes, isochrones, snap diagnostics, and POIs return structured Python objects.
Call `.to_geojson()` when you need serialized GeoJSON for maps or files.

---

## 30-second start

```python
import graphways as gw

# Build the graph once; reuse this object for repeated local queries.
graph = gw.SpatialGraph.from_place(
    "Marienplatz, Munich, Germany",
    network="drive",
    max_dist=10_000,
)

isos = graph.isochrone((48.137144, 11.575399), minutes=[5, 10, 15, 20])

route = graph.route((48.137144, 11.575399), (48.154560, 11.530840))
print(route.distance_m, route.duration_s)

pois = graph.fetch_pois(isos[0])
print(pois.count)
```

---

## Features at a glance

| Feature | Detail |
|---------|--------|
| Graph construction | Parses OSM XML or local OSM PBF into a reusable `SpatialGraph` |
| Simplification | Collapses linear chains, deduplicates parallel edges, and preserves edge geometry |
| Spatial index | R-tree for O(log n) nearest-node lookups |
| Isochrones | Bounded graph search plus triangulated travel-time contours |
| Routing | A* with an admissible straight-line heuristic |
| Network types | Drive, DriveService, Walk, Bike, All, AllPrivate |
| Caching | Overpass XML cache: disk XML -> in-memory XML |
| Python bindings | Structured result objects with explicit GeoJSON export |

---

## Performance

Graphways is designed for repeated local queries over a reusable `SpatialGraph`.
The benchmark suite reports graph construction separately from steady-state
route, reachability, and isochrone queries:

```bash
python benchmarks/comparison.py
python benchmarks/engines/engines.py --pbf C:\path\to\extract.osm.pbf
```

Treat benchmark numbers as workload-specific. They depend on graph size,
network profile, machine, cache state, and whether comparisons include
server-based routing engines such as OSRM or Valhalla.
