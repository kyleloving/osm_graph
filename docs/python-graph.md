# Python API - `SpatialGraph`

The `SpatialGraph` object wraps a loaded road-network graph and its spatial index.  
Construct one with `gw.SpatialGraph.from_place(...)`, `from_pbf(...)`, or `from_osm(...)`.

Reusing a `SpatialGraph` across multiple queries avoids redundant cache lookups and
lets you run isochrones from many different origin points over the same network
without re-fetching data.

```python
graph = gw.SpatialGraph.from_place("Marienplatz, Munich, Germany", network="drive", max_dist=10_000)
print(graph)  # SpatialGraph(nodes=6251, edges=15356, network_type=Drive)
```

---

## Inspection

### `node_count`

```python
graph.node_count() -> int
```

Number of nodes in the graph after simplification (unless `retain_all=True` was passed to the constructor).

---

### `edge_count`

```python
graph.edge_count() -> int
```

Number of directed edges in the graph.

---

### `nearest_node`

```python
graph.nearest_node(lat: float, lon: float) -> tuple[int, float, float] | None
```

Return `(osm_id, lat, lon)` for the graph node nearest to `(lat, lon)`.

Uses the internal R-tree spatial index — O(log n) lookup regardless of graph size.

Returns `None` if the graph is empty.

**Example**

```python
osm_id, node_lat, node_lon = graph.nearest_node(48.137144, 11.575399)
print(f"Snapped to OSM node {osm_id} at ({node_lat:.6f}, {node_lon:.6f})")
```

---

## Isochrones

### `isochrone`

```python
graph.isochrone(
    origin: tuple[float, float],`r`n    minutes: list[float],
) -> list[str]
```

Compute isochrones from an origin using the travel times of this graph's network type.

One Dijkstra pass is run from the nearest graph node; one triangulated contour
polygon is computed per time limit.

**Parameters**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `origin` | `tuple[float, float]` | - | `(lat, lon)` origin |`r`n| `minutes` | `list[float]` | - | Travel-time thresholds in minutes |

**Returns** `list[str]` — one GeoJSON geometry string per time limit, in the same order as `minutes`

**Example**

```python
isos = graph.isochrone((48.137144, 11.575399), minutes=[5, 10, 15, 20])
```

---

## Routing

### `route`

```python
graph.route(
    origin: tuple[float, float],`r`n    destination: tuple[float, float],
) -> str
```

Find the fastest route between two coordinates using A\*.

The network type (drive/walk/bike) is inherited from the `Graph` object.  
If either origin or destination falls outside the graph's bounding box the
nearest in-graph node is used as a proxy.

**Parameters**

| Parameter | Type | Description |
|-----------|------|-------------|
| `origin` | `tuple[float, float]` | `(lat, lon)` origin |`r`n| `destination` | `tuple[float, float]` | `(lat, lon)` destination |

**Returns** `str` — GeoJSON `Feature` (LineString) with properties:

| Property | Type | Description |
|----------|------|-------------|
| `distance_m` | `float` | Total route distance in metres |
| `duration_s` | `float` | Total travel time in seconds |
| `cumulative_times_s` | `list[float]` | Elapsed travel time at each waypoint |

**Example**

```python
import json

route_str = graph.route((48.137144, 11.575399), (48.154560, 11.530840))
route = json.loads(route_str)
props = route["properties"]
coords = route["geometry"]["coordinates"]  # [[lon, lat], ...]

print(f"Distance: {props['distance_m']:.0f} m")
print(f"Duration: {props['duration_s'] / 60:.1f} min")
print(f"Waypoints: {len(coords)}")
```

---

## Reachability

### `reachable`

```python
graph.reachable(
    origin: tuple[float, float],
    minutes: float,
) -> ReachableGraph
```

Return a travel-time-labeled view of the graph reachable from an origin.
Inspection and GeoJSON export use the parent graph plus reachable-node labels,
so they do not copy the road network. Constrained routing and isochrones
materialize a bounded subgraph internally only when those methods are called.

**Example**

```python
import json

reachable = graph.reachable((48.137144, 11.575399), minutes=15)

nodes = reachable.nodes()
node_layer = json.loads(reachable.nodes_geojson())
edge_layer = json.loads(reachable.edges_geojson())
network_layer = json.loads(reachable.to_geojson())
route = reachable.route((48.137144, 11.575399), (48.142, 11.58))
isos = reachable.isochrone((48.137144, 11.575399), minutes=[5, 10, 15])
```

`ReachableGraph` methods:

| Method | Returns | Description |
|--------|---------|-------------|
| `node_count()` | `int` | Number of reachable nodes |
| `edge_count()` | `int` | Number of reachable directed edges |
| `nearest_node(lat, lon)` | `tuple | None` | Nearest node inside the reachable subgraph |
| `contains_node(node_id)` | `bool` | Whether an OSM node id is reachable |
| `travel_time_to_node_id(node_id)` | `float | None` | Travel time to an OSM node id |
| `nodes()` | `list[dict]` | Reachable nodes with `node_id`, `lat`, `lon`, `travel_time_s` |
| `nodes_geojson()` | `str` | Reachable nodes as GeoJSON points |
| `edges_geojson()` | `str` | Edges whose source and target are both reachable |
| `to_geojson()` | `str` | Reachable nodes and edges in one FeatureCollection |
| `route(origin, destination)` | `str` | Route constrained to the reachable subgraph |
| `isochrone(origin, minutes)` | `list[str]` | Isochrones constrained to the reachable subgraph |

---

## Network-Time Prisms

### `prism`

```python
graph.prism(
    origin: tuple[float, float],
    destination: tuple[float, float],
    max_minutes: float,
    stop_minutes: float = 0.0,
    buffer_minutes: float = 0.0,
) -> PrismGraph
```

Return a graph view of possible stops between an origin and a destination within
a fixed time window. A node is inside the prism when:

```text
origin -> node -> destination + stop_minutes + buffer_minutes <= max_minutes
```

This is useful for "what can I do on the way?" analysis without turning the
library into a trip-planning or stop-order optimizer.

Like `ReachableGraph`, `PrismGraph` is a lightweight view. It stores the parent
graph plus inbound, outbound, and slack labels; constrained routes or isochrones
materialize a bounded subgraph only when needed.

**Example**

```python
prism = graph.prism(
    origin=(48.137144, 11.575399),
    destination=(48.154560, 11.530840),
    max_minutes=45,
    stop_minutes=10,
    buffer_minutes=5,
)

nodes = prism.nodes()
network_layer = prism.to_geojson()
slack = prism.slack_polygon(min_slack_s=5 * 60)
route = prism.route((48.137144, 11.575399), (48.142, 11.56))
```

`PrismGraph` methods:

| Method | Returns | Description |
|--------|---------|-------------|
| `node_count()` | `int` | Number of nodes inside the prism |
| `edge_count()` | `int` | Number of directed edges inside the prism |
| `nearest_node(lat, lon)` | `tuple | None` | Nearest node inside the prism graph |
| `contains_node(node_id)` | `bool` | Whether an OSM node id is inside the prism |
| `slack_at_node_id(node_id)` | `float | None` | Remaining slack for an OSM node id |
| `nodes()` | `list[dict]` | Nodes with `node_id`, `lat`, `lon`, `inbound_time_s`, `outbound_time_s`, `slack_s` |
| `nodes_geojson()` | `str` | Prism nodes as GeoJSON points |
| `edges_geojson()` | `str` | Edges whose source and target are inside the prism |
| `to_geojson()` | `str` | Prism nodes and edges in one FeatureCollection |
| `slack_polygon(min_slack_s)` | `str | None` | Polygon enclosing nodes with at least the requested slack |
| `route(origin, destination)` | `str` | Route constrained to the prism graph |
| `isochrone(origin, minutes)` | `list[str]` | Isochrones constrained to the prism graph |

---

## Points of interest

### `fetch_pois`

```python
graph.fetch_pois(isochrone_geojson: str) -> str
```

Fetch OSM points of interest that fall within a given isochrone polygon.

Makes a fresh Overpass API request for the polygon's bounding box, then filters
returned nodes to those geometrically inside the polygon.

**Parameters**

| Parameter | Type | Description |
|-----------|------|-------------|
| `isochrone_geojson` | `str` | A GeoJSON geometry string (typically from `graph.isochrone(...)`) |

**Returns** `str` — GeoJSON `FeatureCollection` where each `Feature` is a POI `Point`
with raw OSM tags as properties.

**Example**

```python
import json

isos = graph.isochrone((48.137144, 11.575399), minutes=[10])
pois_str = graph.fetch_pois(isos[0])
pois = json.loads(pois_str)

restaurants = [
    f["properties"].get("name", "?")
    for f in pois["features"]
    if f["properties"].get("amenity") == "restaurant"
]
print(f"Found {len(restaurants)} restaurants within 10 minutes")
```

---

## Visualisation

### `nodes_geojson`

```python
graph.nodes_geojson() -> str
```

Return all graph nodes as a GeoJSON `FeatureCollection` of `Point` features.

Each feature has properties: `id` (OSM node id), `lat`, `lon`.

Useful for visualising the network in mapping tools.

**Example**

```python
import folium, json

nodes = json.loads(graph.nodes_geojson())
m = folium.Map(location=[48.137, 11.575], zoom_start=13)
for feature in nodes["features"]:
    lat = feature["properties"]["lat"]
    lon = feature["properties"]["lon"]
    folium.CircleMarker([lat, lon], radius=2).add_to(m)
```

---

### `edges_geojson`

```python
graph.edges_geojson() -> str
```

Return all graph edges as a GeoJSON `FeatureCollection` of `LineString` features.

Each feature has properties:

| Property | Type | Description |
|----------|------|-------------|
| `highway` | `str` | OSM highway tag value (e.g. `"residential"`) |
| `length_m` | `float` | Edge length in metres |
| `speed_kph` | `float` | Assigned speed in km/h |
| `drive_time_s` | `float` | Drive travel time in seconds |
| `walk_time_s` | `float` | Walk travel time in seconds |
| `bike_time_s` | `float` | Bike travel time in seconds |

**Example**

```python
import folium, json

edges = json.loads(graph.edges_geojson())
m = folium.Map(location=[48.137, 11.575], zoom_start=13)
folium.GeoJson(edges, style_function=lambda _: {"color": "#3388ff", "weight": 1}).add_to(m)
```
