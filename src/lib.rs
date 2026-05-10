// Public modules — available to any Rust crate that depends on this library.
// None of these import pyo3, so they compile cleanly without the extension-module feature.
pub mod error;
pub mod feasibility;
pub mod filters;
pub mod geocoding;
pub mod graph;
pub mod isochrone;
pub mod overpass;
pub mod pbf;
pub mod poi;
pub mod reachability;
pub mod routing;
pub mod utils;

// Internal implementation details; not part of the public Rust API.
mod cache;
mod simplify;

// ---------------------------------------------------------------------------
// Python extension module
//
// Everything below is compiled ONLY when the "extension-module" feature is
// active (i.e. when maturin is building the .pyd/.so for Python).
// When another Rust crate depends on this library as an rlib, none of this
// code is included and there is no pyo3 / Python linkage at all.
// ---------------------------------------------------------------------------

#[cfg(feature = "extension-module")]
use petgraph::visit::EdgeRef;
#[cfg(feature = "extension-module")]
use pyo3::prelude::*;
#[cfg(feature = "extension-module")]
use pyo3::types::{PyAny, PyDict, PyList};

#[cfg(feature = "extension-module")]
lazy_static::lazy_static! {
    static ref TOKIO_RT: tokio::runtime::Runtime =
        tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
}

// ---------------------------------------------------------------------------
// Shared helpers (Python-binding layer only)
// ---------------------------------------------------------------------------

#[cfg(feature = "extension-module")]
fn parse_network_type(s: &str) -> PyResult<overpass::NetworkType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "drive" => Ok(overpass::NetworkType::Drive),
        "driveservice" | "drive_service" | "drive-service" => {
            Ok(overpass::NetworkType::DriveService)
        }
        "walk" | "walking" => Ok(overpass::NetworkType::Walk),
        "bike" | "biking" | "bicycle" => Ok(overpass::NetworkType::Bike),
        "all" => Ok(overpass::NetworkType::All),
        "allprivate" | "all_private" | "all-private" => Ok(overpass::NetworkType::AllPrivate),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid network '{}'. Expected one of: drive, drive_service, walk, bike, all, all_private",
            s
        ))),
    }
}

#[cfg(feature = "extension-module")]
fn edge_geojson_coords(
    source: &graph::XmlNode,
    target: &graph::XmlNode,
    way: &graph::XmlWay,
) -> Vec<Vec<f64>> {
    let mut points = if way.geometry.len() >= 2 {
        way.geometry.clone()
    } else {
        vec![(source.lat, source.lon), (target.lat, target.lon)]
    };

    let first = *points.first().unwrap();
    let last = *points.last().unwrap();
    let matches_forward = utils::calculate_distance(first.0, first.1, source.lat, source.lon)
        + utils::calculate_distance(last.0, last.1, target.lat, target.lon)
        <= utils::calculate_distance(first.0, first.1, target.lat, target.lon)
            + utils::calculate_distance(last.0, last.1, source.lat, source.lon);
    if !matches_forward {
        points.reverse();
    }

    points
        .into_iter()
        .map(|(lat, lon)| vec![lon, lat])
        .collect()
}

#[cfg(feature = "extension-module")]
fn snap_json(snap: graph::SnapResult) -> geojson::JsonValue {
    let mut obj = geojson::JsonObject::new();
    obj.insert("input_lat".into(), snap.input_lat.into());
    obj.insert("input_lon".into(), snap.input_lon.into());
    obj.insert("node_id".into(), snap.node_id.into());
    obj.insert("node_lat".into(), snap.node_lat.into());
    obj.insert("node_lon".into(), snap.node_lon.into());
    obj.insert("distance_m".into(), snap.distance_m.into());
    geojson::JsonValue::Object(obj)
}

#[cfg(feature = "extension-module")]
fn route_to_geojson(r: &routing::Route) -> String {
    let coords: Vec<Vec<f64>> = r
        .coordinates
        .iter()
        .map(|(lat, lon)| vec![*lon, *lat])
        .collect();
    let geometry = geojson::Geometry::new(geojson::Value::LineString(coords));
    let mut props = geojson::JsonObject::new();
    props.insert("distance_m".into(), r.distance_m.into());
    props.insert("duration_s".into(), r.duration_s.into());
    props.insert("origin_snap".into(), snap_json(r.origin_snap));
    props.insert("destination_snap".into(), snap_json(r.destination_snap));
    props.insert(
        "cumulative_times_s".into(),
        geojson::JsonValue::Array(r.cumulative_times_s.iter().map(|&t| t.into()).collect()),
    );
    let feature = geojson::Feature {
        geometry: Some(geometry),
        properties: Some(props),
        ..Default::default()
    };
    geojson::GeoJson::Feature(feature).to_string()
}

#[cfg(feature = "extension-module")]
fn snap_to_dict<'py>(py: Python<'py>, snap: graph::SnapResult) -> PyResult<&'py PyDict> {
    let dict = PyDict::new(py);
    dict.set_item("input_lat", snap.input_lat)?;
    dict.set_item("input_lon", snap.input_lon)?;
    dict.set_item("node_id", snap.node_id)?;
    dict.set_item("node_lat", snap.node_lat)?;
    dict.set_item("node_lon", snap.node_lon)?;
    dict.set_item("distance_m", snap.distance_m)?;
    Ok(dict)
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "SnapResult")]
#[derive(Clone, Copy)]
struct PySnapResult {
    snap: graph::SnapResult,
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PySnapResult {
    #[getter]
    fn input_lat(&self) -> f64 {
        self.snap.input_lat
    }

    #[getter]
    fn input_lon(&self) -> f64 {
        self.snap.input_lon
    }

    #[getter]
    fn node_id(&self) -> i64 {
        self.snap.node_id
    }

    #[getter]
    fn node_lat(&self) -> f64 {
        self.snap.node_lat
    }

    #[getter]
    fn node_lon(&self) -> f64 {
        self.snap.node_lon
    }

    #[getter]
    fn distance_m(&self) -> f64 {
        self.snap.distance_m
    }

    fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<&'py PyDict> {
        snap_to_dict(py, self.snap)
    }

    fn __repr__(&self) -> String {
        format!(
            "SnapResult(node_id={}, distance_m={:.1})",
            self.snap.node_id, self.snap.distance_m
        )
    }
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "RouteResult")]
#[derive(Clone)]
struct PyRouteResult {
    route: routing::Route,
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyRouteResult {
    #[getter]
    fn coordinates(&self) -> Vec<(f64, f64)> {
        self.route.coordinates.clone()
    }

    #[getter]
    fn cumulative_times_s(&self) -> Vec<f64> {
        self.route.cumulative_times_s.clone()
    }

    #[getter]
    fn distance_m(&self) -> f64 {
        self.route.distance_m
    }

    #[getter]
    fn duration_s(&self) -> f64 {
        self.route.duration_s
    }

    #[getter]
    fn origin_snap(&self) -> PySnapResult {
        PySnapResult {
            snap: self.route.origin_snap,
        }
    }

    #[getter]
    fn destination_snap(&self) -> PySnapResult {
        PySnapResult {
            snap: self.route.destination_snap,
        }
    }

    fn to_geojson(&self) -> String {
        route_to_geojson(&self.route)
    }

    fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<&'py PyDict> {
        let dict = PyDict::new(py);
        dict.set_item("coordinates", self.coordinates())?;
        dict.set_item("cumulative_times_s", self.cumulative_times_s())?;
        dict.set_item("distance_m", self.route.distance_m)?;
        dict.set_item("duration_s", self.route.duration_s)?;
        dict.set_item("origin_snap", self.origin_snap().as_dict(py)?)?;
        dict.set_item("destination_snap", self.destination_snap().as_dict(py)?)?;
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "RouteResult(distance_m={:.0}, duration_s={:.0}, points={})",
            self.route.distance_m,
            self.route.duration_s,
            self.route.coordinates.len()
        )
    }
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "IsochroneResult")]
#[derive(Clone)]
struct PyIsochroneResult {
    minutes: f64,
    polygon: geo::Polygon<f64>,
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyIsochroneResult {
    #[getter]
    fn minutes(&self) -> f64 {
        self.minutes
    }

    fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<&'py PyDict> {
        let dict = PyDict::new(py);
        dict.set_item("minutes", self.minutes)?;
        dict.set_item("geojson", self.to_geojson())?;
        Ok(dict)
    }

    fn to_geojson(&self) -> String {
        utils::polygon_to_geojson_string(&self.polygon)
    }

    fn __repr__(&self) -> String {
        format!("IsochroneResult(minutes={:.1})", self.minutes)
    }
}

#[cfg(feature = "extension-module")]
fn poi_to_dict<'py>(py: Python<'py>, poi: &poi::Poi) -> PyResult<&'py PyDict> {
    let dict = PyDict::new(py);
    dict.set_item("id", poi.id)?;
    dict.set_item("lat", poi.lat)?;
    dict.set_item("lon", poi.lon)?;
    dict.set_item("tags", poi.tags.clone())?;
    Ok(dict)
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "Poi")]
#[derive(Clone)]
struct PyPoi {
    poi: poi::Poi,
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyPoi {
    #[getter]
    fn id(&self) -> i64 {
        self.poi.id
    }

    #[getter]
    fn lat(&self) -> f64 {
        self.poi.lat
    }

    #[getter]
    fn lon(&self) -> f64 {
        self.poi.lon
    }

    #[getter]
    fn tags(&self) -> std::collections::HashMap<String, String> {
        self.poi.tags.clone()
    }

    fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<&'py PyDict> {
        poi_to_dict(py, &self.poi)
    }

    fn __repr__(&self) -> String {
        let name = self
            .poi
            .tags
            .get("name")
            .map(String::as_str)
            .unwrap_or("unnamed");
        format!("Poi(id={}, name={:?})", self.poi.id, name)
    }
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "PoiCollection")]
#[derive(Clone)]
struct PyPoiCollection {
    pois: Vec<poi::Poi>,
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyPoiCollection {
    #[getter]
    fn count(&self) -> usize {
        self.pois.len()
    }

    #[getter]
    fn pois(&self) -> Vec<PyPoi> {
        self.pois.iter().cloned().map(|poi| PyPoi { poi }).collect()
    }

    fn to_geojson(&self) -> String {
        poi::pois_to_geojson(&self.pois)
    }

    fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<&'py PyDict> {
        let dict = PyDict::new(py);
        let items = PyList::empty(py);
        for poi in &self.pois {
            items.append(poi_to_dict(py, poi)?)?;
        }
        dict.set_item("pois", items)?;
        dict.set_item("count", self.pois.len())?;
        Ok(dict)
    }

    fn __len__(&self) -> usize {
        self.pois.len()
    }

    fn __repr__(&self) -> String {
        format!("PoiCollection(count={})", self.pois.len())
    }
}

// ---------------------------------------------------------------------------
// PyGraph — exposes a loaded SpatialGraph to Python
// ---------------------------------------------------------------------------

/// A road-network graph loaded from OpenStreetMap.
///
/// Construct one with `SpatialGraph.from_place(...)`, `SpatialGraph.from_pbf(...)`,
/// or `SpatialGraph.from_osm(...)`, then reuse it for queries over the same area.
#[cfg(feature = "extension-module")]
#[pyclass(name = "SpatialGraph")]
struct PyGraph {
    sg: graph::SpatialGraph,
    network_type: overpass::NetworkType,
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "ReachableGraph")]
struct PyReachableGraph {
    sg: graph::SpatialGraph,
    result: reachability::ReachabilityResult,
    network_type: overpass::NetworkType,
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "PrismGraph")]
struct PyPrismGraph {
    sg: graph::SpatialGraph,
    result: feasibility::FeasibilityResult,
    network_type: overpass::NetworkType,
    max_time_s: f64,
    stop_time_s: f64,
    buffer_s: f64,
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyGraph {
    #[staticmethod]
    #[pyo3(signature = (path, network, retain_all = false))]
    fn from_pbf(path: String, network: String, retain_all: bool) -> PyResult<Self> {
        let nt = parse_network_type(&network)?;
        let sg = graph::SpatialGraph::from_pbf(path, nt, Some(retain_all))?;
        Ok(Self {
            sg,
            network_type: nt,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (xml, network, retain_all = false))]
    fn from_osm(xml: String, network: String, retain_all: bool) -> PyResult<Self> {
        let nt = parse_network_type(&network)?;
        let sg = graph::SpatialGraph::from_osm(&xml, nt, Some(retain_all))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            sg,
            network_type: nt,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (place, network, max_dist = None, retain_all = false))]
    fn from_place(
        place: String,
        network: String,
        max_dist: Option<f64>,
        retain_all: bool,
    ) -> PyResult<Self> {
        let nt = parse_network_type(&network)?;
        let (lat, lon) = TOKIO_RT.block_on(geocoding::geocode(&place))?;
        let (_, sg) = TOKIO_RT.block_on(isochrone::calculate_isochrones_from_point(
            lat,
            lon,
            Some(max_dist.unwrap_or(5_000.0)),
            vec![],
            nt,
            retain_all,
        ))?;
        Ok(Self {
            sg,
            network_type: nt,
        })
    }

    fn node_count(&self) -> usize {
        self.sg.graph.node_count()
    }

    fn edge_count(&self) -> usize {
        self.sg.graph.edge_count()
    }

    fn nearest_node(&self, lat: f64, lon: f64) -> PyResult<Option<(i64, f64, f64)>> {
        Ok(self.sg.nearest_node(lat, lon).map(|idx| {
            let n = &self.sg.graph[idx];
            (n.id, n.lat, n.lon)
        }))
    }

    fn snap_point(&self, lat: f64, lon: f64) -> PyResult<Option<PySnapResult>> {
        let Some(snap) = self.sg.snap_point(lat, lon) else {
            return Ok(None);
        };
        Ok(Some(PySnapResult { snap }))
    }

    #[pyo3(signature = (origin, minutes, max_snap_m = Some(100.0)))]
    fn isochrone(
        &self,
        origin: (f64, f64),
        minutes: Vec<f64>,
        max_snap_m: Option<f64>,
    ) -> PyResult<Vec<PyIsochroneResult>> {
        let output_minutes = minutes.clone();
        let time_limits = minutes.into_iter().map(|m| m * 60.0).collect();
        let isos = self
            .sg
            .isochrones(
                origin.0,
                origin.1,
                time_limits,
                self.network_type,
                max_snap_m,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "No graph node found within max_snap_m of the origin coordinates",
                )
            })?;
        Ok(output_minutes
            .into_iter()
            .zip(isos)
            .map(|(minutes, polygon)| PyIsochroneResult { minutes, polygon })
            .collect())
    }

    #[pyo3(signature = (origin, destination, max_snap_m = Some(100.0)))]
    fn route(
        &self,
        origin: (f64, f64),
        destination: (f64, f64),
        max_snap_m: Option<f64>,
    ) -> PyResult<PyRouteResult> {
        let r = self.sg.route(
            origin.0,
            origin.1,
            destination.0,
            destination.1,
            self.network_type,
            max_snap_m,
        )?;
        Ok(PyRouteResult { route: r })
    }

    fn fetch_pois(&self, isochrone: &PyAny) -> PyResult<PyPoiCollection> {
        let isochrone_geojson = if let Ok(s) = isochrone.extract::<String>() {
            s
        } else if let Ok(iso) = isochrone.extract::<PyRef<PyIsochroneResult>>() {
            iso.to_geojson()
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "fetch_pois expects an IsochroneResult or GeoJSON string",
            ));
        };
        let polygon = poi::parse_isochrone(&isochrone_geojson)?;
        let pois = TOKIO_RT.block_on(poi::fetch_pois_within(&polygon))?;
        Ok(PyPoiCollection { pois })
    }

    #[pyo3(signature = (origin, minutes, max_snap_m = Some(100.0)))]
    fn reachable(
        &self,
        origin: (f64, f64),
        minutes: f64,
        max_snap_m: Option<f64>,
    ) -> PyResult<PyReachableGraph> {
        let reachable = self
            .sg
            .reachable_graph(
                origin.0,
                origin.1,
                minutes * 60.0,
                self.network_type,
                max_snap_m,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "No graph node found within max_snap_m of the origin coordinates",
                )
            })?;
        Ok(PyReachableGraph {
            sg: reachable.graph,
            result: reachable.result,
            network_type: self.network_type,
        })
    }

    #[pyo3(signature = (
        origin,
        destination,
        max_minutes,
        stop_minutes = 0.0,
        buffer_minutes = 0.0,
        max_snap_m = Some(100.0),
    ))]
    fn prism(
        &self,
        origin: (f64, f64),
        destination: (f64, f64),
        max_minutes: f64,
        stop_minutes: f64,
        buffer_minutes: f64,
        max_snap_m: Option<f64>,
    ) -> PyResult<PyPrismGraph> {
        let max_time_s = max_minutes * 60.0;
        let stop_time_s = stop_minutes * 60.0;
        let buffer_s = buffer_minutes * 60.0;
        let traversal_budget = max_time_s - stop_time_s - buffer_s;
        if !traversal_budget.is_finite() || traversal_budget < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "max_minutes must be at least stop_minutes + buffer_minutes",
            ));
        }

        let prism = self
            .sg
            .prism(
                origin.0,
                origin.1,
                destination.0,
                destination.1,
                traversal_budget,
                self.network_type,
                max_snap_m,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "No node found near the origin or destination coordinates",
                )
            })?
            .map_err(|e| match e {
                feasibility::InfeasibleReason::BudgetTooTight { .. } => {
                    pyo3::exceptions::PyValueError::new_err(e.to_string())
                }
                feasibility::InfeasibleReason::NoPathExists => {
                    pyo3::exceptions::PyLookupError::new_err(e.to_string())
                }
            })?;

        Ok(PyPrismGraph {
            sg: prism.graph,
            result: prism.result,
            network_type: self.network_type,
            max_time_s,
            stop_time_s,
            buffer_s,
        })
    }

    fn nodes_geojson(&self) -> String {
        let features: Vec<geojson::Feature> = self
            .sg
            .graph
            .node_indices()
            .map(|idx| {
                let n = &self.sg.graph[idx];
                let geom = geojson::Geometry::new(geojson::Value::Point(vec![n.lon, n.lat]));
                let mut props = geojson::JsonObject::new();
                props.insert("id".into(), n.id.into());
                props.insert("lat".into(), n.lat.into());
                props.insert("lon".into(), n.lon.into());
                geojson::Feature {
                    geometry: Some(geom),
                    properties: Some(props),
                    ..Default::default()
                }
            })
            .collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features,
            bbox: None,
            foreign_members: None,
        })
        .to_string()
    }

    fn edges_geojson(&self) -> String {
        let features: Vec<geojson::Feature> = self
            .sg
            .graph
            .edge_indices()
            .map(|eidx| {
                let (u, v) = self.sg.graph.edge_endpoints(eidx).unwrap();
                let from = &self.sg.graph[u];
                let to = &self.sg.graph[v];
                let way = self.sg.graph.edge_weight(eidx).unwrap();
                let coords = edge_geojson_coords(from, to, way);
                let geom = geojson::Geometry::new(geojson::Value::LineString(coords));
                let highway = way
                    .tags
                    .iter()
                    .find(|t| t.key == "highway")
                    .map(|t| t.value.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let mut props = geojson::JsonObject::new();
                props.insert("highway".into(), highway.into());
                props.insert("length_m".into(), way.length.into());
                props.insert("speed_kph".into(), way.speed_kph.into());
                props.insert("drive_time_s".into(), way.drive_travel_time.into());
                props.insert("walk_time_s".into(), way.walk_travel_time.into());
                props.insert("bike_time_s".into(), way.bike_travel_time.into());
                geojson::Feature {
                    geometry: Some(geom),
                    properties: Some(props),
                    ..Default::default()
                }
            })
            .collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features,
            bbox: None,
            foreign_members: None,
        })
        .to_string()
    }

    fn __repr__(&self) -> String {
        format!(
            "SpatialGraph(nodes={}, edges={}, network_type={:?})",
            self.sg.graph.node_count(),
            self.sg.graph.edge_count(),
            self.network_type,
        )
    }
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyReachableGraph {
    #[getter]
    fn max_time_s(&self) -> f64 {
        self.result.max_cost
    }

    fn node_count(&self) -> usize {
        self.sg.graph.node_count()
    }

    fn edge_count(&self) -> usize {
        self.sg.graph.edge_count()
    }

    fn contains_node(&self, node_id: i64) -> bool {
        self.result
            .distances
            .keys()
            .any(|&idx| self.sg.graph[idx].id == node_id)
    }

    fn nearest_node(&self, lat: f64, lon: f64) -> PyResult<Option<(i64, f64, f64)>> {
        Ok(self
            .result
            .distances
            .keys()
            .min_by(|&&a, &&b| {
                let a_node = &self.sg.graph[a];
                let b_node = &self.sg.graph[b];
                utils::calculate_distance(lat, lon, a_node.lat, a_node.lon)
                    .partial_cmp(&utils::calculate_distance(lat, lon, b_node.lat, b_node.lon))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|&idx| {
                let n = &self.sg.graph[idx];
                (n.id, n.lat, n.lon)
            }))
    }

    fn travel_time_to_node_id(&self, node_id: i64) -> Option<f64> {
        self.result
            .distances
            .iter()
            .find_map(|(&idx, &time)| (self.sg.graph[idx].id == node_id).then_some(time))
    }

    fn nodes<'py>(&self, py: Python<'py>) -> PyResult<&'py PyList> {
        let items = PyList::empty(py);
        for (&idx, &travel_time_s) in &self.result.distances {
            let node = &self.sg.graph[idx];
            let dict = PyDict::new(py);
            dict.set_item("node_id", node.id)?;
            dict.set_item("lat", node.lat)?;
            dict.set_item("lon", node.lon)?;
            dict.set_item("travel_time_s", travel_time_s)?;
            items.append(dict)?;
        }
        Ok(items)
    }

    fn nodes_geojson(&self) -> String {
        let features: Vec<geojson::Feature> = self
            .result
            .distances
            .iter()
            .map(|(&idx, &travel_time_s)| {
                let node = &self.sg.graph[idx];
                let geom = geojson::Geometry::new(geojson::Value::Point(vec![node.lon, node.lat]));
                let mut props = geojson::JsonObject::new();
                props.insert("node_id".into(), node.id.into());
                props.insert("lat".into(), node.lat.into());
                props.insert("lon".into(), node.lon.into());
                props.insert("travel_time_s".into(), travel_time_s.into());
                geojson::Feature {
                    geometry: Some(geom),
                    properties: Some(props),
                    ..Default::default()
                }
            })
            .collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features,
            bbox: None,
            foreign_members: None,
        })
        .to_string()
    }

    fn edges_geojson(&self) -> String {
        let features: Vec<geojson::Feature> = self
            .sg
            .graph
            .edge_references()
            .filter_map(|edge| {
                let source_time = *self.result.distances.get(&edge.source())?;
                let target_time = *self.result.distances.get(&edge.target())?;
                let source = &self.sg.graph[edge.source()];
                let target = &self.sg.graph[edge.target()];
                let way = edge.weight();
                let coords = edge_geojson_coords(source, target, way);
                let geom = geojson::Geometry::new(geojson::Value::LineString(coords));
                let highway = way
                    .tags
                    .iter()
                    .find(|tag| tag.key == "highway")
                    .map(|tag| tag.value.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let mut props = geojson::JsonObject::new();
                props.insert("source_node_id".into(), source.id.into());
                props.insert("target_node_id".into(), target.id.into());
                props.insert("source_time_s".into(), source_time.into());
                props.insert("target_time_s".into(), target_time.into());
                props.insert("highway".into(), highway.into());
                props.insert("length_m".into(), way.length.into());
                props.insert("speed_kph".into(), way.speed_kph.into());
                props.insert("drive_time_s".into(), way.drive_travel_time.into());
                props.insert("walk_time_s".into(), way.walk_travel_time.into());
                props.insert("bike_time_s".into(), way.bike_travel_time.into());
                Some(geojson::Feature {
                    geometry: Some(geom),
                    properties: Some(props),
                    ..Default::default()
                })
            })
            .collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features,
            bbox: None,
            foreign_members: None,
        })
        .to_string()
    }

    fn to_geojson(&self) -> String {
        let node_features: Vec<geojson::Feature> = self
            .result
            .distances
            .iter()
            .map(|(&idx, &travel_time_s)| {
                let node = &self.sg.graph[idx];
                let geom = geojson::Geometry::new(geojson::Value::Point(vec![node.lon, node.lat]));
                let mut props = geojson::JsonObject::new();
                props.insert("kind".into(), "node".into());
                props.insert("node_id".into(), node.id.into());
                props.insert("travel_time_s".into(), travel_time_s.into());
                geojson::Feature {
                    geometry: Some(geom),
                    properties: Some(props),
                    ..Default::default()
                }
            })
            .collect();

        let edge_features = self.sg.graph.edge_references().filter_map(|edge| {
            let source_time = *self.result.distances.get(&edge.source())?;
            let target_time = *self.result.distances.get(&edge.target())?;
            let source = &self.sg.graph[edge.source()];
            let target = &self.sg.graph[edge.target()];
            let way = edge.weight();
            let coords = edge_geojson_coords(source, target, way);
            let geom = geojson::Geometry::new(geojson::Value::LineString(coords));
            let mut props = geojson::JsonObject::new();
            props.insert("kind".into(), "edge".into());
            props.insert("source_node_id".into(), source.id.into());
            props.insert("target_node_id".into(), target.id.into());
            props.insert("source_time_s".into(), source_time.into());
            props.insert("target_time_s".into(), target_time.into());
            props.insert("length_m".into(), way.length.into());
            Some(geojson::Feature {
                geometry: Some(geom),
                properties: Some(props),
                ..Default::default()
            })
        });

        let features = node_features.into_iter().chain(edge_features).collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features,
            bbox: None,
            foreign_members: None,
        })
        .to_string()
    }

    #[pyo3(signature = (origin, minutes, max_snap_m = Some(100.0)))]
    fn isochrone(
        &self,
        origin: (f64, f64),
        minutes: Vec<f64>,
        max_snap_m: Option<f64>,
    ) -> PyResult<Vec<PyIsochroneResult>> {
        let output_minutes = minutes.clone();
        let time_limits = minutes.into_iter().map(|m| m * 60.0).collect();
        let subgraph = reachability::ReachableGraph {
            graph: self.sg.clone(),
            result: self.result.clone(),
            network_type: self.network_type,
        }
        .materialize();
        let isos = subgraph
            .isochrones(
                origin.0,
                origin.1,
                time_limits,
                self.network_type,
                max_snap_m,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "No graph node found within max_snap_m of the origin coordinates",
                )
            })?;
        Ok(output_minutes
            .into_iter()
            .zip(isos)
            .map(|(minutes, polygon)| PyIsochroneResult { minutes, polygon })
            .collect())
    }

    #[pyo3(signature = (origin, destination, max_snap_m = Some(100.0)))]
    fn route(
        &self,
        origin: (f64, f64),
        destination: (f64, f64),
        max_snap_m: Option<f64>,
    ) -> PyResult<PyRouteResult> {
        let subgraph = reachability::ReachableGraph {
            graph: self.sg.clone(),
            result: self.result.clone(),
            network_type: self.network_type,
        }
        .materialize();
        let r = subgraph.route(
            origin.0,
            origin.1,
            destination.0,
            destination.1,
            self.network_type,
            max_snap_m,
        )?;
        Ok(PyRouteResult { route: r })
    }

    fn __repr__(&self) -> String {
        format!(
            "ReachableGraph(nodes={}, edges={}, max_time_s={:.0})",
            self.node_count(),
            self.edge_count(),
            self.result.max_cost,
        )
    }
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyPrismGraph {
    #[getter]
    fn max_time_s(&self) -> f64 {
        self.max_time_s
    }

    #[getter]
    fn traversal_budget_s(&self) -> f64 {
        self.result.available_time
    }

    #[getter]
    fn stop_time_s(&self) -> f64 {
        self.stop_time_s
    }

    #[getter]
    fn buffer_s(&self) -> f64 {
        self.buffer_s
    }

    #[getter]
    fn direct_time_s(&self) -> f64 {
        self.result.direct_time
    }

    fn node_count(&self) -> usize {
        self.result.feasible.len()
    }

    fn edge_count(&self) -> usize {
        self.sg
            .graph
            .edge_references()
            .filter(|edge| {
                self.result.feasible.contains_key(&edge.source())
                    && self.result.feasible.contains_key(&edge.target())
            })
            .count()
    }

    fn contains_node(&self, node_id: i64) -> bool {
        self.result
            .feasible
            .keys()
            .any(|&idx| self.sg.graph[idx].id == node_id)
    }

    fn nearest_node(&self, lat: f64, lon: f64) -> PyResult<Option<(i64, f64, f64)>> {
        Ok(self
            .result
            .feasible
            .keys()
            .min_by(|&&a, &&b| {
                let a_node = &self.sg.graph[a];
                let b_node = &self.sg.graph[b];
                utils::calculate_distance(lat, lon, a_node.lat, a_node.lon)
                    .partial_cmp(&utils::calculate_distance(lat, lon, b_node.lat, b_node.lon))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|&idx| {
                let n = &self.sg.graph[idx];
                (n.id, n.lat, n.lon)
            }))
    }

    fn slack_at_node_id(&self, node_id: i64) -> Option<f64> {
        self.result
            .feasible
            .iter()
            .find_map(|(&idx, node)| (self.sg.graph[idx].id == node_id).then_some(node.slack))
    }

    fn nodes<'py>(&self, py: Python<'py>) -> PyResult<&'py PyList> {
        let items = PyList::empty(py);
        for (&idx, reach) in &self.result.feasible {
            let node = &self.sg.graph[idx];
            let dict = PyDict::new(py);
            dict.set_item("node_id", node.id)?;
            dict.set_item("lat", node.lat)?;
            dict.set_item("lon", node.lon)?;
            dict.set_item("inbound_time_s", reach.inbound_time)?;
            dict.set_item("outbound_time_s", reach.outbound_time)?;
            dict.set_item("slack_s", reach.slack)?;
            items.append(dict)?;
        }
        Ok(items)
    }

    fn nodes_geojson(&self) -> String {
        let features: Vec<geojson::Feature> = self
            .result
            .feasible
            .iter()
            .map(|(&idx, node_data)| {
                let node = &self.sg.graph[idx];
                let geom = geojson::Geometry::new(geojson::Value::Point(vec![node.lon, node.lat]));
                let mut props = geojson::JsonObject::new();
                props.insert("node_id".into(), node.id.into());
                props.insert("lat".into(), node.lat.into());
                props.insert("lon".into(), node.lon.into());
                props.insert("inbound_time_s".into(), node_data.inbound_time.into());
                props.insert("outbound_time_s".into(), node_data.outbound_time.into());
                props.insert("slack_s".into(), node_data.slack.into());
                geojson::Feature {
                    geometry: Some(geom),
                    properties: Some(props),
                    ..Default::default()
                }
            })
            .collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features,
            bbox: None,
            foreign_members: None,
        })
        .to_string()
    }

    fn edges_geojson(&self) -> String {
        let features: Vec<geojson::Feature> = self
            .sg
            .graph
            .edge_references()
            .filter_map(|edge| {
                let source_data = self.result.feasible.get(&edge.source())?;
                let target_data = self.result.feasible.get(&edge.target())?;
                let source = &self.sg.graph[edge.source()];
                let target = &self.sg.graph[edge.target()];
                let way = edge.weight();
                let coords = edge_geojson_coords(source, target, way);
                let geom = geojson::Geometry::new(geojson::Value::LineString(coords));
                let highway = way
                    .tags
                    .iter()
                    .find(|tag| tag.key == "highway")
                    .map(|tag| tag.value.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let mut props = geojson::JsonObject::new();
                props.insert("source_node_id".into(), source.id.into());
                props.insert("target_node_id".into(), target.id.into());
                props.insert("source_slack_s".into(), source_data.slack.into());
                props.insert("target_slack_s".into(), target_data.slack.into());
                props.insert("highway".into(), highway.into());
                props.insert("length_m".into(), way.length.into());
                props.insert("speed_kph".into(), way.speed_kph.into());
                props.insert("drive_time_s".into(), way.drive_travel_time.into());
                props.insert("walk_time_s".into(), way.walk_travel_time.into());
                props.insert("bike_time_s".into(), way.bike_travel_time.into());
                Some(geojson::Feature {
                    geometry: Some(geom),
                    properties: Some(props),
                    ..Default::default()
                })
            })
            .collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features,
            bbox: None,
            foreign_members: None,
        })
        .to_string()
    }

    fn to_geojson(&self) -> String {
        let node_features: Vec<geojson::Feature> = self
            .result
            .feasible
            .iter()
            .map(|(&idx, node_data)| {
                let node = &self.sg.graph[idx];
                let geom = geojson::Geometry::new(geojson::Value::Point(vec![node.lon, node.lat]));
                let mut props = geojson::JsonObject::new();
                props.insert("kind".into(), "node".into());
                props.insert("node_id".into(), node.id.into());
                props.insert("inbound_time_s".into(), node_data.inbound_time.into());
                props.insert("outbound_time_s".into(), node_data.outbound_time.into());
                props.insert("slack_s".into(), node_data.slack.into());
                geojson::Feature {
                    geometry: Some(geom),
                    properties: Some(props),
                    ..Default::default()
                }
            })
            .collect();

        let edge_features = self.sg.graph.edge_references().filter_map(|edge| {
            let source_data = self.result.feasible.get(&edge.source())?;
            let target_data = self.result.feasible.get(&edge.target())?;
            let source = &self.sg.graph[edge.source()];
            let target = &self.sg.graph[edge.target()];
            let way = edge.weight();
            let coords = edge_geojson_coords(source, target, way);
            let geom = geojson::Geometry::new(geojson::Value::LineString(coords));
            let mut props = geojson::JsonObject::new();
            props.insert("kind".into(), "edge".into());
            props.insert("source_node_id".into(), source.id.into());
            props.insert("target_node_id".into(), target.id.into());
            props.insert("source_slack_s".into(), source_data.slack.into());
            props.insert("target_slack_s".into(), target_data.slack.into());
            props.insert("length_m".into(), way.length.into());
            Some(geojson::Feature {
                geometry: Some(geom),
                properties: Some(props),
                ..Default::default()
            })
        });

        let features = node_features.into_iter().chain(edge_features).collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features,
            bbox: None,
            foreign_members: None,
        })
        .to_string()
    }

    #[pyo3(signature = (min_slack_s = 0.0))]
    fn slack_polygon(&self, min_slack_s: f64) -> PyResult<Option<String>> {
        Ok(
            feasibility::build_feasibility_polygon(&self.sg.graph, &self.result, min_slack_s)
                .map(|p| utils::polygon_to_geojson_string(&p)),
        )
    }

    fn slack_polygons(&self, min_slack_values: Vec<f64>) -> PyResult<Vec<Option<String>>> {
        Ok(min_slack_values
            .into_iter()
            .map(|min_slack| {
                feasibility::build_feasibility_polygon(&self.sg.graph, &self.result, min_slack)
                    .map(|p| utils::polygon_to_geojson_string(&p))
            })
            .collect())
    }

    #[pyo3(signature = (origin, minutes, max_snap_m = Some(100.0)))]
    fn isochrone(
        &self,
        origin: (f64, f64),
        minutes: Vec<f64>,
        max_snap_m: Option<f64>,
    ) -> PyResult<Vec<PyIsochroneResult>> {
        let output_minutes = minutes.clone();
        let time_limits = minutes.into_iter().map(|m| m * 60.0).collect();
        let subgraph = feasibility::PrismGraph {
            graph: self.sg.clone(),
            result: self.result.clone(),
            network_type: self.network_type,
        }
        .materialize();
        let isos = subgraph
            .isochrones(
                origin.0,
                origin.1,
                time_limits,
                self.network_type,
                max_snap_m,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "No graph node found within max_snap_m of the origin coordinates",
                )
            })?;
        Ok(output_minutes
            .into_iter()
            .zip(isos)
            .map(|(minutes, polygon)| PyIsochroneResult { minutes, polygon })
            .collect())
    }

    #[pyo3(signature = (origin, destination, max_snap_m = Some(100.0)))]
    fn route(
        &self,
        origin: (f64, f64),
        destination: (f64, f64),
        max_snap_m: Option<f64>,
    ) -> PyResult<PyRouteResult> {
        let subgraph = feasibility::PrismGraph {
            graph: self.sg.clone(),
            result: self.result.clone(),
            network_type: self.network_type,
        }
        .materialize();
        let r = subgraph.route(
            origin.0,
            origin.1,
            destination.0,
            destination.1,
            self.network_type,
            max_snap_m,
        )?;
        Ok(PyRouteResult { route: r })
    }

    fn __repr__(&self) -> String {
        format!(
            "PrismGraph(nodes={}, edges={}, direct_time_s={:.0}, max_time_s={:.0})",
            self.node_count(),
            self.edge_count(),
            self.result.direct_time,
            self.max_time_s,
        )
    }
}

// ---------------------------------------------------------------------------
// Module-level Python functions
// ---------------------------------------------------------------------------

#[cfg(feature = "extension-module")]
#[pyfunction]
fn geocode(place: String) -> PyResult<(f64, f64)> {
    Ok(TOKIO_RT.block_on(geocoding::geocode(&place))?)
}

#[cfg(feature = "extension-module")]
#[pyfunction]
fn clear_cache() -> PyResult<()> {
    cache::clear_cache()?;
    cache::clear_disk_cache()?;
    Ok(())
}

#[cfg(feature = "extension-module")]
#[pyfunction]
fn cache_dir() -> PyResult<String> {
    Ok(cache::disk_cache_dir().to_string_lossy().into_owned())
}

#[cfg(feature = "extension-module")]
#[pymodule]
fn graphways(_py: Python, m: &PyModule) -> pyo3::PyResult<()> {
    m.add_class::<PyGraph>()?;
    m.add_class::<PyReachableGraph>()?;
    m.add_class::<PyPrismGraph>()?;
    m.add_class::<PySnapResult>()?;
    m.add_class::<PyRouteResult>()?;
    m.add_class::<PyIsochroneResult>()?;
    m.add_class::<PyPoi>()?;
    m.add_class::<PyPoiCollection>()?;
    m.add_function(wrap_pyfunction!(geocode, m)?)?;
    m.add_function(wrap_pyfunction!(clear_cache, m)?)?;
    m.add_function(wrap_pyfunction!(cache_dir, m)?)?;
    Ok(())
}
