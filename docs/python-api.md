# Python API - Module Functions

These small helpers are available directly on the `graphways` module.  
The primary API is [`SpatialGraph`](python-graph.md). Build one graph, then run operations on it:

```python
import graphways as gw

graph = gw.SpatialGraph.from_place("Washington, DC", network="walk")
isos = graph.isochrone((38.9097, -77.0432), minutes=[10, 20, 30])
route = graph.route((38.9097, -77.0432), (38.8977, -77.0365))
print(route.duration_s, route.distance_m)
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
Graphways rate-limits Nominatim requests to one request per second and retries
transient `429` / `5xx` responses.

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
graph.fetch_pois(isochrone: IsochroneResult | str) -> PoiCollection
```

Fetch OpenStreetMap points of interest that fall within a given isochrone polygon.

Queries Overpass for the bounding box of the polygon, using the XML cache when
available, then filters the returned nodes to those geometrically inside the
polygon.

**Parameters**

| Parameter | Type | Description |
|-----------|------|-------------|
| `isochrone` | `IsochroneResult` or `str` | An isochrone result or a GeoJSON geometry string |

**Returns** `PoiCollection` with structured POIs. Call `.to_geojson()` for a
GeoJSON `FeatureCollection`.

!!! note
    To avoid filtering POIs yourself, prefer `graph.fetch_pois(iso)` when you already
    have a `SpatialGraph` object.

**Example**

```python
import graphways as gw

graph = gw.SpatialGraph.from_place("Marienplatz, Munich, Germany", network="walk")
isos = graph.isochrone((48.137144, 11.575399), minutes=[10])
pois = graph.fetch_pois(isos[0])

for poi in pois.pois:
    tags = poi.tags
    if tags.get("amenity") == "restaurant":
        print(tags.get("name", "unnamed restaurant"))
```

---

## `cache_dir`

```python
gw.cache_dir() -> str
```

Return the path to the on-disk XML cache directory.

Override the default location by setting the `GRAPHWAYS_CACHE_DIR` environment variable.

## Network service configuration

Graphways uses public OpenStreetMap services for `gw.geocode(...)`,
`SpatialGraph.from_place(...)`, and POI fetching. Override the defaults with:

| Variable | Purpose |
|----------|---------|
| `GRAPHWAYS_OVERPASS_URL` | Overpass interpreter endpoint |
| `GRAPHWAYS_NOMINATIM_URL` | Nominatim search endpoint |
| `GRAPHWAYS_USER_AGENT` | User-Agent sent with HTTP requests |

For production or high-volume workflows, prefer a local Overpass/Nominatim
instance or `SpatialGraph.from_pbf(...)`.

---

## `clear_cache`

```python
gw.clear_cache() -> None
```

Clear both the in-memory and on-disk XML caches.

Useful when you want to force a fresh fetch from Overpass, e.g. after OSM data has been updated.
