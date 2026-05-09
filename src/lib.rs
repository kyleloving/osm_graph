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
use pyo3::prelude::*;
#[cfg(feature = "extension-module")]
use pyo3::types::{PyDict, PyList};

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
    match s {
        "Drive" => Ok(overpass::NetworkType::Drive),
        "DriveService" => Ok(overpass::NetworkType::DriveService),
        "Walk" => Ok(overpass::NetworkType::Walk),
        "Bike" => Ok(overpass::NetworkType::Bike),
        "All" => Ok(overpass::NetworkType::All),
        "AllPrivate" => Ok(overpass::NetworkType::AllPrivate),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid network type '{}'. Expected: Drive, DriveService, Walk, Bike, All, AllPrivate",
            s
        ))),
    }
}

// ---------------------------------------------------------------------------
// PyGraph — exposes a loaded SpatialGraph to Python
// ---------------------------------------------------------------------------

/// A road-network graph loaded from OpenStreetMap.
///
/// Obtain one via `build_graph(...)` and reuse it for multiple queries over
/// the same area — isochrones, routing, and POI lookups all share the same
/// in-memory graph with no redundant fetches.
#[cfg(feature = "extension-module")]
#[pyclass(name = "SpatialGraph")]
struct PyGraph {
    sg: graph::SpatialGraph,
    network_type: overpass::NetworkType,
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "Reachability")]
struct PyReachability {
    sg: graph::SpatialGraph,
    result: reachability::ReachabilityResult,
}

#[cfg(feature = "extension-module")]
#[pyclass(name = "BetweenReachability")]
struct PyBetweenReachability {
    sg: graph::SpatialGraph,
    result: feasibility::FeasibilityResult,
    max_time_s: f64,
    stop_time_s: f64,
    buffer_s: f64,
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyGraph {
    #[staticmethod]
    #[pyo3(signature = (path, network_type, retain_all = false))]
    fn from_pbf(path: String, network_type: String, retain_all: bool) -> PyResult<Self> {
        let nt = parse_network_type(&network_type)?;
        let sg = graph::SpatialGraph::from_pbf(path, nt, Some(retain_all))?;
        Ok(Self {
            sg,
            network_type: nt,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (xml, network_type, retain_all = false))]
    fn from_osm(xml: String, network_type: String, retain_all: bool) -> PyResult<Self> {
        let nt = parse_network_type(&network_type)?;
        let sg = graph::SpatialGraph::from_osm(&xml, nt, Some(retain_all))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            sg,
            network_type: nt,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (place, network_type, max_dist = None, retain_all = false))]
    fn from_place(
        place: String,
        network_type: String,
        max_dist: Option<f64>,
        retain_all: bool,
    ) -> PyResult<Self> {
        let nt = parse_network_type(&network_type)?;
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

    fn snap_point<'py>(
        &self,
        py: Python<'py>,
        lat: f64,
        lon: f64,
    ) -> PyResult<Option<&'py PyDict>> {
        let Some(snap) = self.sg.snap_point(lat, lon) else {
            return Ok(None);
        };
        let dict = PyDict::new(py);
        dict.set_item("input_lat", snap.input_lat)?;
        dict.set_item("input_lon", snap.input_lon)?;
        dict.set_item("node_id", snap.node_id)?;
        dict.set_item("node_lat", snap.node_lat)?;
        dict.set_item("node_lon", snap.node_lon)?;
        dict.set_item("distance_m", snap.distance_m)?;
        Ok(Some(dict))
    }

    fn isochrone(&self, origin: (f64, f64), minutes: Vec<f64>) -> PyResult<Vec<String>> {
        let time_limits = minutes.into_iter().map(|m| m * 60.0).collect();
        let isos = self
            .sg
            .isochrones(origin.0, origin.1, time_limits, self.network_type)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("No node found near the given coordinates")
            })?;
        Ok(isos
            .iter()
            .map(|p| utils::polygon_to_geojson_string(p))
            .collect())
    }

    fn route(&self, origin: (f64, f64), destination: (f64, f64)) -> PyResult<String> {
        let r = self.sg.route(
            origin.0,
            origin.1,
            destination.0,
            destination.1,
            self.network_type,
        )?;

        let coords: Vec<Vec<f64>> = r
            .coordinates
            .iter()
            .map(|(lat, lon)| vec![*lon, *lat])
            .collect();
        let geometry = geojson::Geometry::new(geojson::Value::LineString(coords));
        let mut props = geojson::JsonObject::new();
        props.insert("distance_m".into(), r.distance_m.into());
        props.insert("duration_s".into(), r.duration_s.into());
        props.insert(
            "cumulative_times_s".into(),
            geojson::JsonValue::Array(r.cumulative_times_s.iter().map(|&t| t.into()).collect()),
        );
        let feature = geojson::Feature {
            geometry: Some(geometry),
            properties: Some(props),
            ..Default::default()
        };
        Ok(geojson::GeoJson::Feature(feature).to_string())
    }

    fn fetch_pois(&self, isochrone_geojson: String) -> PyResult<String> {
        let polygon = poi::parse_isochrone(&isochrone_geojson)?;
        let pois = TOKIO_RT.block_on(poi::fetch_pois_within(&polygon))?;
        Ok(poi::pois_to_geojson(&pois))
    }

    fn reachable(&self, origin: (f64, f64), minutes: f64) -> PyResult<PyReachability> {
        let result = self
            .sg
            .reachable_from(origin.0, origin.1, minutes * 60.0, self.network_type)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("No node found near the given coordinates")
            })?;
        Ok(PyReachability {
            sg: self.sg.clone(),
            result,
        })
    }

    #[pyo3(signature = (
        origin,
        destination,
        max_time_s,
        stop_time_s = 0.0,
        buffer_s = 0.0,
    ))]
    fn reachable_between(
        &self,
        origin: (f64, f64),
        destination: (f64, f64),
        max_time_s: f64,
        stop_time_s: f64,
        buffer_s: f64,
    ) -> PyResult<PyBetweenReachability> {
        let traversal_budget = max_time_s - stop_time_s - buffer_s;
        if !traversal_budget.is_finite() || traversal_budget < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "max_time_s must be at least stop_time_s + buffer_s",
            ));
        }

        let result = self
            .sg
            .reachable_between(
                origin.0,
                origin.1,
                destination.0,
                destination.1,
                traversal_budget,
                self.network_type,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "No node found near the origin or destination coordinates",
                )
            })?
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        Ok(PyBetweenReachability {
            sg: self.sg.clone(),
            result,
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
                let coords = vec![vec![from.lon, from.lat], vec![to.lon, to.lat]];
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
impl PyReachability {
    #[getter]
    fn max_time_s(&self) -> f64 {
        self.result.max_cost
    }

    fn node_count(&self) -> usize {
        self.result.distances.len()
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

    fn isochrones(&self, time_limits: Vec<f64>) -> PyResult<Vec<String>> {
        let polygons =
            isochrone::build_isochrone_polygons(&self.sg.graph, &self.result, &time_limits);
        Ok(polygons
            .iter()
            .map(|p| utils::polygon_to_geojson_string(p))
            .collect())
    }

    fn __repr__(&self) -> String {
        format!(
            "Reachability(nodes={}, max_time_s={:.0})",
            self.result.distances.len(),
            self.result.max_cost,
        )
    }
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyBetweenReachability {
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

    fn __repr__(&self) -> String {
        format!(
            "BetweenReachability(nodes={}, direct_time_s={:.0}, max_time_s={:.0})",
            self.result.feasible.len(),
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
#[pyo3(signature = (lat, lon, network_type, max_dist = None, retain_all = false))]
fn build_graph(
    lat: f64,
    lon: f64,
    network_type: String,
    max_dist: Option<f64>,
    retain_all: bool,
) -> PyResult<PyGraph> {
    let nt = parse_network_type(&network_type)?;
    let dist = max_dist.unwrap_or(5_000.0);
    let (_, sg) = TOKIO_RT.block_on(isochrone::calculate_isochrones_from_point(
        lat,
        lon,
        Some(dist),
        vec![],
        nt,
        retain_all,
    ))?;
    Ok(PyGraph {
        sg,
        network_type: nt,
    })
}

#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (lat, lon, time_limits, network_type, max_dist=None, retain_all=false))]
fn calc_isochrones(
    lat: f64,
    lon: f64,
    time_limits: Vec<f64>,
    network_type: String,
    max_dist: Option<f64>,
    retain_all: bool,
) -> PyResult<Vec<String>> {
    let nt = parse_network_type(&network_type)?;
    let (isos, _) = TOKIO_RT.block_on(isochrone::calculate_isochrones_from_point(
        lat,
        lon,
        max_dist,
        time_limits.clone(),
        nt,
        retain_all,
    ))?;
    Ok(isos
        .iter()
        .map(|iso| utils::polygon_to_geojson_string(iso))
        .collect())
}

#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (origin_lat, origin_lon, dest_lat, dest_lon, network_type, max_dist=None, retain_all=false))]
fn calc_route(
    origin_lat: f64,
    origin_lon: f64,
    dest_lat: f64,
    dest_lon: f64,
    network_type: String,
    max_dist: Option<f64>,
    retain_all: bool,
) -> PyResult<String> {
    let nt = parse_network_type(&network_type)?;
    let mid_lat = (origin_lat + dest_lat) / 2.0;
    let mid_lon = (origin_lon + dest_lon) / 2.0;
    let straight_line = utils::calculate_distance(origin_lat, origin_lon, dest_lat, dest_lon);
    let computed_dist = max_dist.unwrap_or_else(|| (straight_line * 1.5).max(5_000.0));
    let (_, sg) = TOKIO_RT.block_on(isochrone::calculate_isochrones_from_point(
        mid_lat,
        mid_lon,
        Some(computed_dist),
        vec![],
        nt,
        retain_all,
    ))?;
    let r = sg.route(origin_lat, origin_lon, dest_lat, dest_lon, nt)?;
    let coords: Vec<Vec<f64>> = r
        .coordinates
        .iter()
        .map(|(lat, lon)| vec![*lon, *lat])
        .collect();
    let geometry = geojson::Geometry::new(geojson::Value::LineString(coords));
    let mut props = geojson::JsonObject::new();
    props.insert("distance_m".to_string(), r.distance_m.into());
    props.insert("duration_s".to_string(), r.duration_s.into());
    props.insert(
        "cumulative_times_s".to_string(),
        geojson::JsonValue::Array(
            r.cumulative_times_s
                .iter()
                .map(|&t| geojson::JsonValue::from(t))
                .collect(),
        ),
    );
    let feature = geojson::Feature {
        geometry: Some(geometry),
        properties: Some(props),
        ..Default::default()
    };
    Ok(geojson::GeoJson::Feature(feature).to_string())
}

#[cfg(feature = "extension-module")]
#[pyfunction]
fn geocode(place: String) -> PyResult<(f64, f64)> {
    Ok(TOKIO_RT.block_on(geocoding::geocode(&place))?)
}

#[cfg(feature = "extension-module")]
#[pyfunction]
fn fetch_pois(isochrone_geojson: String) -> PyResult<String> {
    let polygon = poi::parse_isochrone(&isochrone_geojson)?;
    let pois = TOKIO_RT.block_on(poi::fetch_pois_within(&polygon))?;
    Ok(poi::pois_to_geojson(&pois))
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
    m.add_class::<PyReachability>()?;
    m.add_class::<PyBetweenReachability>()?;
    m.add("Graph", m.getattr("SpatialGraph")?)?;
    m.add_function(wrap_pyfunction!(build_graph, m)?)?;
    m.add_function(wrap_pyfunction!(calc_isochrones, m)?)?;
    m.add_function(wrap_pyfunction!(calc_route, m)?)?;
    m.add_function(wrap_pyfunction!(geocode, m)?)?;
    m.add_function(wrap_pyfunction!(fetch_pois, m)?)?;
    m.add_function(wrap_pyfunction!(clear_cache, m)?)?;
    m.add_function(wrap_pyfunction!(cache_dir, m)?)?;
    Ok(())
}
