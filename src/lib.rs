#![allow(dead_code)]

mod graph;
mod isochrone;
mod overpass;
mod utils;
mod cache;
mod simplify;
mod error;
mod tests;

use pyo3::prelude::*;

/// Calculates isochrones from a point
#[pyfunction]
fn calc_isochrones(
    lat: f64,
    lon: f64,
    max_dist: f64,
    time_limits: Vec<f64>,
    network_type: String,
    hull_type: String,
    retain_all: Option<bool>,
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
            retain_all.unwrap_or(false),
        ))?;

    Ok(isochrones
        .iter()
        .map(|iso| utils::polygon_to_geojson_string(iso))
        .collect())
}

/// Python module for quickly creating isochrones
#[pymodule]
fn pysochrone(_py: Python, m: &PyModule) -> pyo3::PyResult<()> {
    m.add_function(wrap_pyfunction!(calc_isochrones, m)?)?;
    Ok(())
}
