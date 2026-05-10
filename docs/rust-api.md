# Rust API

The Rust API exposes the same core graph operations without Python overhead.

Full generated API documentation is published alongside this site:
**[Rust API docs ->](https://docs.rs/graphways/)**

---

## Quick example

```rust
use graphways::graph::SpatialGraph;
use graphways::overpass::NetworkType;

fn main() -> Result<(), graphways::error::OsmGraphError> {
    let graph = SpatialGraph::from_pbf(
        "data/district-of-columbia-latest.osm.pbf",
        NetworkType::Walk,
        None,
    )?;

    let reachable = graph.reachable_graph(
        38.9097,
        -77.0432,
        15.0 * 60.0,
        NetworkType::Walk,
        Some(100.0),
    );

    println!(
        "{} nodes, reachable graph exists: {}",
        graph.graph.node_count(),
        reachable.is_some()
    );
    Ok(())
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
// Nearest-node lookup -- O(log n)
let node_idx = sg.nearest_node(lat, lon)?;

// Direct petgraph access
let node_count = sg.graph.node_count();
let edge_count = sg.graph.edge_count();
```

For local PBF workflows, construct the reusable graph directly:

```rust
use graphways::graph::SpatialGraph;
use graphways::overpass::NetworkType;

let graph = SpatialGraph::from_pbf(
    "data/district-of-columbia-latest.osm.pbf",
    NetworkType::Walk,
    None,
)?;
```

For OSM XML, use the sibling constructor:

```rust
let graph = SpatialGraph::from_osm(xml, NetworkType::Walk, None)?;
```

---

### `XmlNode`

```rust
pub struct XmlNode {
    pub id: i64,
    pub lat: f64,
    pub lon: f64,
    pub tags: Vec<XmlTag>,
}
```

---

### `XmlWay`

```rust
pub struct XmlWay {
    pub id: i64,
    pub tags: Vec<XmlTag>,
    pub geometry: Vec<(f64, f64)>, // (lat, lon), including simplified edge shape
    pub length: f64,           // meters
    pub speed_kph: f64,
    pub walk_travel_time: f64, // seconds
    pub bike_travel_time: f64, // seconds
    pub drive_travel_time: f64,// seconds
}
```

---

## Core functions

### `routing::route`

```rust
pub fn route(
    sg: &SpatialGraph,
    origin_lat: f64,
    origin_lon: f64,
    dest_lat: f64,
    dest_lon: f64,
    network_type: NetworkType,
    max_snap_m: Option<f64>,
) -> Result<Route, OsmGraphError>
```

A\* point-to-point routing. Pass `max_snap_m` to reject endpoints that snap too
far from the graph. Returns a `Route`:

```rust
pub struct Route {
    pub coordinates: Vec<(f64, f64)>,   // (lat, lon) per waypoint
    pub cumulative_times_s: Vec<f64>,   // parallel to coordinates
    pub distance_m: f64,
    pub duration_s: f64,
    pub origin_snap: SnapResult,
    pub destination_snap: SnapResult,
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
Transient `429` / `5xx` responses are retried. The default endpoint helpers
respect `GRAPHWAYS_OVERPASS_URL`, `GRAPHWAYS_NOMINATIM_URL`, and
`GRAPHWAYS_USER_AGENT`.

---

## Error type

```rust
pub enum OsmGraphError {
    Network(reqwest::Error),
    XmlParse(quick_xml::DeError),
    EmptyGraph,
    NodeNotFound,
    OriginNodeNotFound,
    DestinationNodeNotFound,
    SnapDistanceExceeded { role: &'static str, distance_m: f64, max_distance_m: f64 },
    PathNotFound,
    LockPoisoned,
    GeocodingFailed(String),
    InvalidInput(String),
    Io(std::io::Error),
    PbfError(String),
}
```
