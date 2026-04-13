#![allow(dead_code)]

mod graph;
mod isochrone;
mod overpass;
mod utils;
mod cache;
mod simplify;
mod error;
mod tests;
mod routing;
mod geocoding;

use pyo3::prelude::*;

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

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let (isochrones, _) = rt
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

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    // Use the midpoint as the query origin, with max_dist covering both points
    let mid_lat = (origin_lat + dest_lat) / 2.0;
    let mid_lon = (origin_lon + dest_lon) / 2.0;

    // Auto-size bounding box: straight-line distance between points * 1.5 buffer, min 5km
    let straight_line = utils::calculate_distance(origin_lat, origin_lon, dest_lat, dest_lon);
    let computed_dist = max_dist.unwrap_or_else(|| (straight_line * 1.5).max(5_000.0));

    let (_, sg) = rt
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
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(rt.block_on(geocoding::geocode(&place))?)
}

/// Python module for quickly creating isochrones
#[pymodule]
fn pysochrone(_py: Python, m: &PyModule) -> pyo3::PyResult<()> {
    m.add_function(wrap_pyfunction!(calc_isochrones, m)?)?;
    m.add_function(wrap_pyfunction!(calc_route, m)?)?;
    m.add_function(wrap_pyfunction!(geocode, m)?)?;
    Ok(())
}
