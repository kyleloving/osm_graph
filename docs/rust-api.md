# Rust API

The Rust API exposes the same core functionality without Python overhead.
All async functions require a Tokio runtime.

Full generated API documentation is published alongside this site:  
**[Rust API docs →](https://docs.rs/osm_graph/)**

---

## Quick example

```rust
use osm_graph::isochrone::{calculate_isochrones_from_point, HullType};
use osm_graph::overpass::NetworkType;

#[tokio::main]
async fn main() {
    let (isochrones, graph) = calculate_isochrones_from_point(
        48.137144,
        11.575399,
        Some(10_000.0),                                        // max_dist in metres; None = auto
        vec![300.0, 600.0, 900.0, 1_200.0, 1_500.0, 1_800.0],
        NetworkType::Drive,
        HullType::Concave,
        false,                                                 // false = simplified (faster)
    )
    .await
    .unwrap();

    println!("{} isochrones, {} nodes", isochrones.len(), graph.graph.node_count());
}
```

---

## Key types

### `NetworkType`

```rust
pub enum NetworkType {
    Drive,
    DriveService,
    Walk,
    Bike,
    All,
    AllPrivate,
}
```

Controls which OSM highway tags are included in the graph.  See the [Quickstart](quickstart.md#choosing-a-network-type) for details.

---

### `HullType`

```rust
pub enum HullType {
    Convex,
    FastConcave,
    Concave,
}
```

Controls how the isochrone boundary is computed from the set of reachable nodes.

---

### `SpatialGraph`

```rust
pub struct SpatialGraph {
    pub graph: Arc<DiGraph<XmlNode, XmlWay>>,
    // internal R-tree omitted
}
```

A petgraph `DiGraph` bundled with an R-tree spatial index.  Both inner fields are
`Arc`-wrapped, so cloning a `SpatialGraph` is O(1).

```rust
// Nearest-node lookup — O(log n)
let node_idx = sg.nearest_node(lat, lon)?;

// Direct petgraph access
let node_count = sg.graph.node_count();
let edge_count = sg.graph.edge_count();
```

---

### `XmlNode`

```rust
pub struct XmlNode {
    pub id: i64,
    pub lat: f64,
    pub lon: f64,
    pub tags: Vec<XmlTag>,
    pub geohash: Option<String>,
}
```

---

### `XmlWay`

```rust
pub struct XmlWay {
    pub id: i64,
    pub tags: Vec<XmlTag>,
    pub length: f64,           // metres
    pub speed_kph: f64,
    pub walk_travel_time: f64, // seconds
    pub bike_travel_time: f64, // seconds
    pub drive_travel_time: f64,// seconds
}
```

---

## Core functions

### `calculate_isochrones_from_point`

```rust
pub async fn calculate_isochrones_from_point(
    lat: f64,
    lon: f64,
    max_dist: Option<f64>,
    time_limits: Vec<f64>,
    network_type: NetworkType,
    hull_type: HullType,
    retain_all: bool,
) -> Result<(Vec<Polygon>, SpatialGraph), OsmGraphError>
```

Fetch (or cache-hit) the road network, run a single Dijkstra pass, and compute
hull polygons for each time limit in parallel threads.

Pass `max_dist = None` to auto-size the bounding box from the largest time limit.
Pass `time_limits = vec![]` to skip isochrone computation and only obtain the `SpatialGraph`.

---

### `routing::route`

```rust
pub fn route(
    sg: &SpatialGraph,
    origin_lat: f64,
    origin_lon: f64,
    dest_lat: f64,
    dest_lon: f64,
    network_type: NetworkType,
) -> Result<Route, OsmGraphError>
```

A\* point-to-point routing.  Returns a `Route`:

```rust
pub struct Route {
    pub coordinates: Vec<(f64, f64)>,   // (lat, lon) per waypoint
    pub cumulative_times_s: Vec<f64>,   // parallel to coordinates
    pub distance_m: f64,
    pub duration_s: f64,
}
```

---

### `overpass::bbox_from_point`

```rust
pub fn bbox_from_point(lat: f64, lon: f64, dist: f64) -> String
```

Construct a `south,west,north,east` bounding-box string for an Overpass API query.

---

### `overpass::make_request`

```rust
pub async fn make_request(url: &str, query: &str) -> Result<String, reqwest::Error>
```

POST a query to an Overpass API endpoint and return the raw XML response.

---

## Error type

```rust
pub enum OsmGraphError {
    XmlParseError(quick_xml::DeError),
    RequestError(reqwest::Error),
    EmptyGraph,
    NodeNotFound,
    InvalidInput(String),
    CacheError(String),
}
```
