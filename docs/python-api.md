# Python API — Module functions

These top-level functions are available directly on the `pysochrone` module.  
For repeated queries over the same area, prefer [`build_graph`](#build_graph) and the [`Graph`](python-graph.md) object.

---

## `build_graph`

```python
pysochrone.build_graph(
    lat: float,
    lon: float,
    network_type: str,
    *,
    max_dist: float | None = None,
    retain_all: bool = False,
) -> Graph
```

Build and return a road-network [`Graph`](python-graph.md) for the area around `(lat, lon)`.

The graph is internally cached — repeated calls for the same area and network type
return the in-memory graph with no network I/O.

**Parameters**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lat` | `float` | — | Latitude of the centre point |
| `lon` | `float` | — | Longitude of the centre point |
| `network_type` | `str` | — | See [network types](quickstart.md#choosing-a-network-type) |
| `max_dist` | `float \| None` | `5000` | Bounding-box radius in metres |
| `retain_all` | `bool` | `False` | Skip graph simplification (preserves all OSM nodes and edges) |

**Returns** [`Graph`](python-graph.md)

**Example**

```python
graph = pysochrone.build_graph(48.137144, 11.575399, "Drive", max_dist=10_000)
print(graph)  # Graph(nodes=6251, edges=15356, network_type=Drive)
```

---

## `calc_isochrones`

```python
pysochrone.calc_isochrones(
    lat: float,
    lon: float,
    time_limits: list[float],
    network_type: str,
    hull_type: str,
    *,
    max_dist: float | None = None,
    retain_all: bool = False,
) -> list[str]
```

Compute isochrones from a single origin point.

Internally fetches and caches the graph for the area, then runs a single
Dijkstra pass and computes one hull polygon per time limit in parallel threads.

**Parameters**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lat` | `float` | — | Origin latitude |
| `lon` | `float` | — | Origin longitude |
| `time_limits` | `list[float]` | — | Travel-time thresholds in **seconds** |
| `network_type` | `str` | — | See [network types](quickstart.md#choosing-a-network-type) |
| `hull_type` | `str` | — | `"Convex"` \| `"FastConcave"` \| `"Concave"` |
| `max_dist` | `float \| None` | auto | Bounding-box radius in metres.  When `None`, derived from the largest time limit and a conservative speed estimate. |
| `retain_all` | `bool` | `False` | Skip graph simplification |

**Returns** `list[str]` — one GeoJSON geometry string per time limit, in the same order as `time_limits`

!!! tip
    Pass `time_limits` in ascending order.  The returned list preserves that order, making it easy to render isochrones largest-first (so smaller ones render on top).

**Example**

```python
isos = pysochrone.calc_isochrones(
    48.137144, 11.575399,
    [300, 600, 900, 1200, 1500, 1800],
    "Walk", "Concave",
)
# isos[0] is the 5-minute isochrone, isos[-1] the 30-minute isochrone
```

---

## `calc_route`

```python
pysochrone.calc_route(
    origin_lat: float,
    origin_lon: float,
    dest_lat: float,
    dest_lon: float,
    network_type: str,
    *,
    max_dist: float | None = None,
    retain_all: bool = False,
) -> str
```

Find the fastest route between two coordinates using A\*.

**Parameters**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `origin_lat` | `float` | — | Origin latitude |
| `origin_lon` | `float` | — | Origin longitude |
| `dest_lat` | `float` | — | Destination latitude |
| `dest_lon` | `float` | — | Destination longitude |
| `network_type` | `str` | — | See [network types](quickstart.md#choosing-a-network-type) |
| `max_dist` | `float \| None` | auto | Bounding-box radius.  When `None`, uses `max(5000, 1.5 × straight-line distance)`. |
| `retain_all` | `bool` | `False` | Skip graph simplification |

**Returns** `str` — GeoJSON `Feature` (LineString) with properties:

| Property | Type | Description |
|----------|------|-------------|
| `distance_m` | `float` | Total route distance in metres |
| `duration_s` | `float` | Total travel time in seconds |
| `cumulative_times_s` | `list[float]` | Elapsed travel time at each waypoint, starting at `0.0` and ending at `duration_s` |

**Example**

```python
import json

route_str = pysochrone.calc_route(
    48.137144, 11.575399,
    48.154560, 11.530840,
    "Drive",
)
route = json.loads(route_str)
props = route["properties"]
coords = route["geometry"]["coordinates"]  # [[lon, lat], ...]
print(f"{props['distance_m']:.0f} m in {props['duration_s']:.0f} s")
```

---

## `geocode`

```python
pysochrone.geocode(place: str) -> tuple[float, float]
```

Convert a place name to `(lat, lon)` coordinates via the Nominatim API.

**Parameters**

| Parameter | Type | Description |
|-----------|------|-------------|
| `place` | `str` | Any Nominatim-supported query string |

**Returns** `(lat, lon)` as a tuple of floats

**Example**

```python
lat, lon = pysochrone.geocode("Marienplatz, Munich, Germany")
print(lat, lon)  # 48.137... 11.575...
```

---

## `fetch_pois`

```python
pysochrone.fetch_pois(isochrone_geojson: str) -> str
```

Fetch OpenStreetMap points of interest that fall within a given isochrone polygon.

Makes a fresh Overpass API request for the bounding box of the polygon, then
filters the returned nodes to those geometrically inside the polygon.

**Parameters**

| Parameter | Type | Description |
|-----------|------|-------------|
| `isochrone_geojson` | `str` | A GeoJSON geometry string as returned by `calc_isochrones` |

**Returns** `str` — GeoJSON `FeatureCollection` where each feature is a POI `Point`
with all raw OSM tags as properties.

!!! note
    To avoid filtering POIs yourself, prefer `graph.fetch_pois(iso)` when you already
    have a `Graph` object — it uses the same implementation.

**Example**

```python
import json

isos = pysochrone.calc_isochrones(48.137144, 11.575399, [600], "Walk", "Concave")
pois_str = pysochrone.fetch_pois(isos[0])
pois = json.loads(pois_str)

for feature in pois["features"]:
    tags = feature["properties"]
    if tags.get("amenity") == "restaurant":
        print(tags.get("name", "unnamed restaurant"))
```

---

## `cache_dir`

```python
pysochrone.cache_dir() -> str
```

Return the path to the on-disk XML cache directory.

Override the default location by setting the `OSM_GRAPH_CACHE_DIR` environment variable.

---

## `clear_cache`

```python
pysochrone.clear_cache() -> None
```

Clear both the in-memory (graph and XML) caches and the on-disk XML cache.

Useful when you want to force a fresh fetch from Overpass, e.g. after OSM data has been updated.
