# Quickstart

## The core pattern

Graphways is centered on a reusable `SpatialGraph`. Build the graph once for an area, then run reachability, isochrone, route, and POI queries against that graph.

```python
import graphways as gw

origin = (48.137144, 11.575399)
destination = (48.154560, 11.530840)

graph = gw.SpatialGraph.from_place(
    "Marienplatz, Munich, Germany",
    network="drive",
    max_dist=10_000,
)
print(graph)

isos = graph.isochrone(origin, minutes=[5, 10, 15, 20])
route = graph.route(origin, destination)
pois = graph.fetch_pois(isos[-1])
```

Use `SpatialGraph.from_pbf(path, network="walk")` for local offline OSM PBF workflows, or `SpatialGraph.from_osm(xml, network="walk")` when you already have OSM XML.

---
## Working with GeoJSON output

All results come back as GeoJSON strings. Parse them with the standard library:

```python
import json

# Isochrone geometry
iso = json.loads(isos[0])
print(iso["type"])        # "Polygon"
print(iso["coordinates"]) # [[lon, lat], ...]

# Route feature
route_data = json.loads(route)
props = route_data["properties"]
print(f"Distance: {props['distance_m']:.0f} m")
print(f"Duration: {props['duration_s']:.0f} s")
print(f"Waypoints: {len(props['cumulative_times_s'])}")

# POI FeatureCollection
pois_data = json.loads(pois)
for feature in pois_data["features"]:
    tags = feature["properties"]
    name = tags.get("name", "unnamed")
    kind = tags.get("amenity") or tags.get("shop") or "?"
    print(f"{name} ({kind})")
```

---

## Choosing a network type

| Value | Includes |
|-------|----------|
| `"drive"` | Public roads accessible to private cars; excludes service roads, driveways |
| `"drive_service"` | Like `Drive` but includes service roads |
| `"walk"` | Footways, pedestrian paths, and shared roads where walking is permitted |
| `"bike"` | Cycleways and roads open to bicycles |
| `"all"` | All highway types except private access |
| `"all_private"` | All highway types including private access |

---

## Choosing a hull type

| Value | Shape | Speed | Best for |
|-------|-------|-------|----------|
Isochrone polygons use triangulated travel-time contour extraction by default.

---

## Caching

graphways uses a three-level cache so repeated queries skip the network entirely:

```
Overpass API (network) → disk XML → in-memory XML → in-memory graph
```

Each level persists across process restarts (disk) or within a session (memory).

```python
print(gw.cache_dir())  # shows the disk cache location
gw.clear_cache()       # wipe all three levels
```

Set `OSM_GRAPH_CACHE_DIR` to override the default cache location.
