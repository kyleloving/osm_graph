#![allow(dead_code)]

mod graph;
mod isochrone;
mod overpass;
mod utils;
mod cache;
mod simplify;
mod error;
mod routing;
mod geocoding;
mod poi;

use pyo3::prelude::*;

lazy_static::lazy_static! {
    static ref TOKIO_RT: tokio::runtime::Runtime =
        tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
}

/// Calculates isochrones from a point
#[pyfunction]
#[pyo3(signature = (lat, lon, time_limits, network_type, hull_type, max_dist=None, retain_all=false))]
fn calc_isochrones(
    lat: f64,
    lon: f64,
    time_limits: Vec<f64>,
    network_type: String,
    hull_type: String,
    max_dist: Option<f64>,
    retain_all: bool,
) -> PyResult<Vec<String>> {
    let network_type_enum = match network_type.as_str() {
        "Drive" => overpass::NetworkType::Drive,
        "DriveService" => overpass::NetworkType::DriveService,
        "Walk" => overpass::NetworkType::Walk,
        "Bike" => overpass::NetworkType::Bike,
        "All" => overpass::NetworkType::All,
        "AllPrivate" => overpass::NetworkType::AllPrivate,
        _ => return Err(pyo3::exceptions::PyValueError::new_err(
            format!("Invalid network type '{}'. Expected one of: Drive, DriveService, Walk, Bike, All, AllPrivate", network_type)
        )),
    };

    let hull_type_enum = match hull_type.as_str() {
        "Convex" => isochrone::HullType::Convex,
        "FastConcave" => isochrone::HullType::FastConcave,
        "Concave" => isochrone::HullType::Concave,
        _ => return Err(pyo3::exceptions::PyValueError::new_err(
            format!("Invalid hull type '{}'. Expected one of: Convex, FastConcave, Concave", hull_type)
        )),
    };

    let (isochrones, _) = TOKIO_RT
        .block_on(isochrone::calculate_isochrones_from_point(
            lat,
            lon,
            max_dist,
            time_limits,
            network_type_enum,
            hull_type_enum,
            retain_all,
        ))?;

    Ok(isochrones
        .iter()
        .map(|iso| utils::polygon_to_geojson_string(iso))
        .collect())
}

/// Routes between two points, returning a GeoJSON LineString string
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
    let network_type_enum = match network_type.as_str() {
        "Drive" => overpass::NetworkType::Drive,
        "DriveService" => overpass::NetworkType::DriveService,
        "Walk" => overpass::NetworkType::Walk,
        "Bike" => overpass::NetworkType::Bike,
        "All" => overpass::NetworkType::All,
        "AllPrivate" => overpass::NetworkType::AllPrivate,
        _ => return Err(pyo3::exceptions::PyValueError::new_err(
            format!("Invalid network type '{}'. Expected one of: Drive, DriveService, Walk, Bike, All, AllPrivate", network_type)
        )),
    };

    // Use the midpoint as the query origin, with max_dist covering both points
    let mid_lat = (origin_lat + dest_lat) / 2.0;
    let mid_lon = (origin_lon + dest_lon) / 2.0;

    // Auto-size bounding box: straight-line distance between points * 1.5 buffer, min 5km
    let straight_line = utils::calculate_distance(origin_lat, origin_lon, dest_lat, dest_lon);
    let computed_dist = max_dist.unwrap_or_else(|| (straight_line * 1.5).max(5_000.0));

    let (_, sg) = TOKIO_RT
        .block_on(isochrone::calculate_isochrones_from_point(
            mid_lat, mid_lon, Some(computed_dist),
            vec![],
            network_type_enum,
            isochrone::HullType::Convex,
            retain_all,
        ))?;

    let route = routing::route(&sg, origin_lat, origin_lon, dest_lat, dest_lon, network_type_enum)?;

    // Convert to GeoJSON LineString — coordinates are [lon, lat] per spec
    let coords: Vec<Vec<f64>> = route.coordinates.iter()
        .map(|(lat, lon)| vec![*lon, *lat])
        .collect();

    let geometry = geojson::Geometry::new(geojson::Value::LineString(coords));
    let mut props = geojson::JsonObject::new();
    props.insert("distance_m".to_string(), route.distance_m.into());
    props.insert("duration_s".to_string(), route.duration_s.into());
    props.insert(
        "cumulative_times_s".to_string(),
        geojson::JsonValue::Array(
            route.cumulative_times_s.iter().map(|&t| geojson::JsonValue::from(t)).collect(),
        ),
    );
    let feature = geojson::Feature {
        geometry: Some(geometry),
        properties: Some(props),
        ..Default::default()
    };

    Ok(geojson::GeoJson::Feature(feature).to_string())
}

/// Geocode a place name to (lat, lon)
#[pyfunction]
fn geocode(place: String) -> PyResult<(f64, f64)> {
    Ok(TOKIO_RT.block_on(geocoding::geocode(&place))?)
}

/// Fetch POIs from OpenStreetMap that fall within a given isochrone polygon.
/// `isochrone_geojson` should be one of the strings returned by `calc_isochrones`.
/// Returns a GeoJSON FeatureCollection string — each feature is a POI with its
/// raw OSM tags as properties.
#[pyfunction]
fn fetch_pois(isochrone_geojson: String) -> PyResult<String> {
    let polygon = poi::parse_isochrone(&isochrone_geojson)?;
    let pois = TOKIO_RT.block_on(poi::fetch_pois_within(&polygon))?;
    Ok(poi::pois_to_geojson(&pois))
}

/// Clear both the in-memory graph/XML caches and the on-disk XML cache.
#[pyfunction]
fn clear_cache() -> PyResult<()> {
    cache::clear_cache()?;
    cache::clear_disk_cache()?;
    Ok(())
}

/// Return the path to the on-disk XML cache directory.
/// Set the OSM_GRAPH_CACHE_DIR environment variable to override the default location.
#[pyfunction]
fn cache_dir() -> PyResult<String> {
    Ok(cache::disk_cache_dir()
        .to_string_lossy()
        .into_owned())
}

/// Python module for quickly creating isochrones
#[pymodule]
fn pysochrone(_py: Python, m: &PyModule) -> pyo3::PyResult<()> {
    m.add_function(wrap_pyfunction!(calc_isochrones, m)?)?;
    m.add_function(wrap_pyfunction!(calc_route, m)?)?;
    m.add_function(wrap_pyfunction!(geocode, m)?)?;
    m.add_function(wrap_pyfunction!(fetch_pois, m)?)?;
    m.add_function(wrap_pyfunction!(clear_cache, m)?)?;
    m.add_function(wrap_pyfunction!(cache_dir, m)?)?;
    Ok(())
}
