# Python API - Module Functions

These small helpers are available directly on the `graphways` module.  
The primary API is [`SpatialGraph`](python-graph.md). Build one graph, then run operations on it:

```python
import graphways as gw

graph = gw.SpatialGraph.from_place("Washington, DC", network="walk")
isos = graph.isochrone((38.9097, -77.0432), minutes=[10, 20, 30])
route = graph.route((38.9097, -77.0432), (38.8977, -77.0365))
reachable = graph.reachable((38.9097, -77.0432), minutes=15)
prism = graph.prism(
    (38.9097, -77.0432),
    (38.8977, -77.0365),
    max_minutes=45,
)
```

Use `SpatialGraph.from_pbf(path, network="walk")` for local PBF files and `SpatialGraph.from_osm(xml, network="walk")` for OSM XML strings.
## `geocode`

```python
gw.geocode(place: str) -> tuple[float, float]
```

Convert a place name to `(lat, lon)` coordinates via the Nominatim API.

**Parameters**

| Parameter | Type | Description |
|-----------|------|-------------|
| `place` | `str` | Any Nominatim-supported query string |

**Returns** `(lat, lon)` as a tuple of floats

**Example**

```python
lat, lon = gw.geocode("Marienplatz, Munich, Germany")
print(lat, lon)  # 48.137... 11.575...
```

---

## `fetch_pois`

```python
graph.fetch_pois(isochrone_geojson: str) -> str
```

Fetch OpenStreetMap points of interest that fall within a given isochrone polygon.

Makes a fresh Overpass API request for the bounding box of the polygon, then
filters the returned nodes to those geometrically inside the polygon.

**Parameters**

| Parameter | Type | Description |
|-----------|------|-------------|
| `isochrone_geojson` | `str` | A GeoJSON geometry string as returned by `graph.isochrone(...)` |

**Returns** `str` — GeoJSON `FeatureCollection` where each feature is a POI `Point`
with all raw OSM tags as properties.

!!! note
    To avoid filtering POIs yourself, prefer `graph.fetch_pois(iso)` when you already
    have a `Graph` object — it uses the same implementation.

**Example**

```python
import json
import graphways as gw

graph = gw.SpatialGraph.from_place("Marienplatz, Munich, Germany", network="walk")
isos = graph.isochrone((48.137144, 11.575399), minutes=[10])
pois_str = graph.fetch_pois(isos[0])
pois = json.loads(pois_str)

for feature in pois["features"]:
    tags = feature["properties"]
    if tags.get("amenity") == "restaurant":
        print(tags.get("name", "unnamed restaurant"))
```

---

## `cache_dir`

```python
gw.cache_dir() -> str
```

Return the path to the on-disk XML cache directory.

Override the default location by setting the `OSM_GRAPH_CACHE_DIR` environment variable.

---

## `clear_cache`

```python
gw.clear_cache() -> None
```

Clear both the in-memory (graph and XML) caches and the on-disk XML cache.

Useful when you want to force a fresh fetch from Overpass, e.g. after OSM data has been updated.
