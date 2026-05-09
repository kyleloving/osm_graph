# Quickstart

## The two usage patterns

### Stateless functions (simple, one-off queries)

Call `calc_isochrones` or `calc_route` directly. Each call internally fetches
and caches the graph, so the first call for an area is slow (network I/O) and
subsequent calls are fast (cache hit).

```python
import graphways

# Geocode a place name
lat, lon = graphways.geocode("Marienplatz, Munich, Germany")

# Isochrones: 5, 10, 15, 20 minutes driving
isos = graphways.calc_isochrones(lat, lon, [300, 600, 900, 1200], "Drive")

# Route between two points
route = graphways.calc_route(lat, lon, 48.154560, 11.530840, "Drive")
```

### Stateful Graph object (multiple queries over the same area)

`build_graph` returns a `Graph` that you reuse for isochrones, routing, and POI
lookups without re-loading data from the cache each time.

```python
import graphways

graph = graphways.build_graph(48.137144, 11.575399, "Drive", max_dist=10_000)
print(graph)  # Graph(nodes=6251, edges=15356, network_type=Drive)

# Compute isochrones for multiple origin points using the same graph
origins = [(48.137144, 11.575399), (48.154560, 11.530840)]
for lat, lon in origins:
    isos = graph.isochrone((lat, lon), minutes=[5, 10, 15])
    pois = graph.fetch_pois(isos[-1])  # POIs within the largest isochrone
```

Prefer the `Graph` object whenever you make more than one query for the same area.

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
| `"Drive"` | Public roads accessible to private cars; excludes service roads, driveways |
| `"DriveService"` | Like `Drive` but includes service roads |
| `"Walk"` | Footways, pedestrian paths, and shared roads where walking is permitted |
| `"Bike"` | Cycleways and roads open to bicycles |
| `"All"` | All highway types except private access |
| `"AllPrivate"` | All highway types including private access |

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
print(graphways.cache_dir())  # shows the disk cache location
graphways.clear_cache()       # wipe all three levels
```

Set `graphways_CACHE_DIR` to override the default cache location.
