#![allow(dead_code)]

// Public modules — available to any Rust crate that depends on this library.
// None of these import pyo3, so they compile cleanly without the extension-module feature.
pub mod error;
pub mod geocoding;
pub mod graph;
pub mod isochrone;
pub mod overpass;
pub mod pbf;
pub mod poi;
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
use std::sync::Arc;

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
        "Drive"        => Ok(overpass::NetworkType::Drive),
        "DriveService" => Ok(overpass::NetworkType::DriveService),
        "Walk"         => Ok(overpass::NetworkType::Walk),
        "Bike"         => Ok(overpass::NetworkType::Bike),
        "All"          => Ok(overpass::NetworkType::All),
        "AllPrivate"   => Ok(overpass::NetworkType::AllPrivate),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid network type '{}'. Expected: Drive, DriveService, Walk, Bike, All, AllPrivate", s
        ))),
    }
}

#[cfg(feature = "extension-module")]
fn parse_hull_type(s: &str) -> PyResult<isochrone::HullType> {
    match s {
        "Convex"      => Ok(isochrone::HullType::Convex),
        "FastConcave" => Ok(isochrone::HullType::FastConcave),
        "Concave"     => Ok(isochrone::HullType::Concave),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid hull type '{}'. Expected: Convex, FastConcave, Concave", s
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
#[pyclass(name = "Graph")]
struct PyGraph {
    sg: graph::SpatialGraph,
    network_type: overpass::NetworkType,
}

#[cfg(feature = "extension-module")]
#[pymethods]
impl PyGraph {
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

    #[pyo3(signature = (lat, lon, time_limits, hull_type = "Concave"))]
    fn isochrones(
        &self,
        lat: f64,
        lon: f64,
        time_limits: Vec<f64>,
        hull_type: &str,
    ) -> PyResult<Vec<String>> {
        let hull = parse_hull_type(hull_type)?;
        let node = self.sg.nearest_node(lat, lon)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(
                "No node found near the given coordinates"
            ))?;

        let shared = Arc::clone(&self.sg.graph);
        let isos = isochrone::calculate_isochrones_concurrently(
            shared, node, time_limits, self.network_type, hull,
        );
        Ok(isos.iter().map(|p| utils::polygon_to_geojson_string(p)).collect())
    }

    fn route(
        &self,
        origin_lat: f64,
        origin_lon: f64,
        dest_lat: f64,
        dest_lon: f64,
    ) -> PyResult<String> {
        let r = routing::route(&self.sg, origin_lat, origin_lon, dest_lat, dest_lon, self.network_type)?;

        let coords: Vec<Vec<f64>> = r.coordinates.iter()
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
        let feature = geojson::Feature { geometry: Some(geometry), properties: Some(props), ..Default::default() };
        Ok(geojson::GeoJson::Feature(feature).to_string())
    }

    fn fetch_pois(&self, isochrone_geojson: String) -> PyResult<String> {
        let polygon = poi::parse_isochrone(&isochrone_geojson)?;
        let pois = TOKIO_RT.block_on(poi::fetch_pois_within(&polygon))?;
        Ok(poi::pois_to_geojson(&pois))
    }

    fn nodes_geojson(&self) -> String {
        let features: Vec<geojson::Feature> = self.sg.graph.node_indices().map(|idx| {
            let n = &self.sg.graph[idx];
            let geom = geojson::Geometry::new(geojson::Value::Point(vec![n.lon, n.lat]));
            let mut props = geojson::JsonObject::new();
            props.insert("id".into(), n.id.into());
            props.insert("lat".into(), n.lat.into());
            props.insert("lon".into(), n.lon.into());
            geojson::Feature { geometry: Some(geom), properties: Some(props), ..Default::default() }
        }).collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features, bbox: None, foreign_members: None,
        }).to_string()
    }

    fn edges_geojson(&self) -> String {
        let features: Vec<geojson::Feature> = self.sg.graph.edge_indices().map(|eidx| {
            let (u, v) = self.sg.graph.edge_endpoints(eidx).unwrap();
            let from = &self.sg.graph[u];
            let to   = &self.sg.graph[v];
            let way  = self.sg.graph.edge_weight(eidx).unwrap();
            let coords = vec![vec![from.lon, from.lat], vec![to.lon, to.lat]];
            let geom = geojson::Geometry::new(geojson::Value::LineString(coords));
            let highway = way.tags.iter()
                .find(|t| t.key == "highway")
                .map(|t| t.value.as_str()).unwrap_or("unknown").to_string();
            let mut props = geojson::JsonObject::new();
            props.insert("highway".into(),      highway.into());
            props.insert("length_m".into(),     way.length.into());
            props.insert("speed_kph".into(),    way.speed_kph.into());
            props.insert("drive_time_s".into(), way.drive_travel_time.into());
            props.insert("walk_time_s".into(),  way.walk_travel_time.into());
            props.insert("bike_time_s".into(),  way.bike_travel_time.into());
            geojson::Feature { geometry: Some(geom), properties: Some(props), ..Default::default() }
        }).collect();
        geojson::GeoJson::FeatureCollection(geojson::FeatureCollection {
            features, bbox: None, foreign_members: None,
        }).to_string()
    }

    fn __repr__(&self) -> String {
        format!(
            "Graph(nodes={}, edges={}, network_type={:?})",
            self.sg.graph.node_count(),
            self.sg.graph.edge_count(),
            self.network_type,
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
    lat: f64, lon: f64, network_type: String,
    max_dist: Option<f64>, retain_all: bool,
) -> PyResult<PyGraph> {
    let nt = parse_network_type(&network_type)?;
    let dist = max_dist.unwrap_or(5_000.0);
    let (_, sg) = TOKIO_RT.block_on(isochrone::calculate_isochrones_from_point(
        lat, lon, Some(dist), vec![], nt, isochrone::HullType::Convex, retain_all,
    ))?;
    Ok(PyGraph { sg, network_type: nt })
}

#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (lat, lon, time_limits, network_type, hull_type, max_dist=None, retain_all=false))]
fn calc_isochrones(
    lat: f64, lon: f64, time_limits: Vec<f64>,
    network_type: String, hull_type: String,
    max_dist: Option<f64>, retain_all: bool,
) -> PyResult<Vec<String>> {
    let nt = parse_network_type(&network_type)?;
    let hull = parse_hull_type(&hull_type)?;
    let (isochrones, _) = TOKIO_RT.block_on(isochrone::calculate_isochrones_from_point(
        lat, lon, max_dist, time_limits, nt, hull, retain_all,
    ))?;
    Ok(isochrones.iter().map(|iso| utils::polygon_to_geojson_string(iso)).collect())
}

#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (origin_lat, origin_lon, dest_lat, dest_lon, network_type, max_dist=None, retain_all=false))]
fn calc_route(
    origin_lat: f64, origin_lon: f64, dest_lat: f64, dest_lon: f64,
    network_type: String, max_dist: Option<f64>, retain_all: bool,
) -> PyResult<String> {
    let nt = parse_network_type(&network_type)?;
    let mid_lat = (origin_lat + dest_lat) / 2.0;
    let mid_lon = (origin_lon + dest_lon) / 2.0;
    let straight_line = utils::calculate_distance(origin_lat, origin_lon, dest_lat, dest_lon);
    let computed_dist = max_dist.unwrap_or_else(|| (straight_line * 1.5).max(5_000.0));
    let (_, sg) = TOKIO_RT.block_on(isochrone::calculate_isochrones_from_point(
        mid_lat, mid_lon, Some(computed_dist), vec![], nt, isochrone::HullType::Convex, retain_all,
    ))?;
    let r = routing::route(&sg, origin_lat, origin_lon, dest_lat, dest_lon, nt)?;
    let coords: Vec<Vec<f64>> = r.coordinates.iter().map(|(lat, lon)| vec![*lon, *lat]).collect();
    let geometry = geojson::Geometry::new(geojson::Value::LineString(coords));
    let mut props = geojson::JsonObject::new();
    props.insert("distance_m".to_string(), r.distance_m.into());
    props.insert("duration_s".to_string(), r.duration_s.into());
    props.insert(
        "cumulative_times_s".to_string(),
        geojson::JsonValue::Array(r.cumulative_times_s.iter().map(|&t| geojson::JsonValue::from(t)).collect()),
    );
    let feature = geojson::Feature { geometry: Some(geometry), properties: Some(props), ..Default::default() };
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
fn pysochrone(_py: Python, m: &PyModule) -> pyo3::PyResult<()> {
    m.add_class::<PyGraph>()?;
    m.add_function(wrap_pyfunction!(build_graph, m)?)?;
    m.add_function(wrap_pyfunction!(calc_isochrones, m)?)?;
    m.add_function(wrap_pyfunction!(calc_route, m)?)?;
    m.add_function(wrap_pyfunction!(geocode, m)?)?;
    m.add_function(wrap_pyfunction!(fetch_pois, m)?)?;
    m.add_function(wrap_pyfunction!(clear_cache, m)?)?;
    m.add_function(wrap_pyfunction!(cache_dir, m)?)?;
    Ok(())
}
