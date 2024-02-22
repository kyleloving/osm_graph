#![allow(dead_code)]

mod graph;
mod isochrone;
mod overpass;
mod utils;
mod cache;

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
) -> PyResult<Vec<String>> {
    // Convert network_type string to NetworkType enum
    let network_type_enum = match network_type.as_str() {
        "Drive" => overpass::NetworkType::Drive,
        "DriveService" => overpass::NetworkType::DriveService,
        "Walk" => overpass::NetworkType::Walk,
        "Bike" => overpass::NetworkType::Bike,
        "All" => overpass::NetworkType::All,
        "AllPrivate" => overpass::NetworkType::AllPrivate,
        _ => {
            return Err(pyo3::PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid network type",
            ))
        }
    };

    // Convert hull_type string to HullType enum
    let hull_type_enum = match hull_type.as_str() {
        "Convex" => isochrone::HullType::Convex,
        "FastConcave" => isochrone::HullType::FastConcave,
        "Concave" => isochrone::HullType::Concave,
        _ => {
            return Err(pyo3::PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid hull type",
            ))
        }
    };

    // Create a new Tokio runtime
    let rt = tokio::runtime::Runtime::new().unwrap();
    let isochrones = rt
        .block_on(isochrone::calculate_isochrones_from_point(
            lat,
            lon,
            max_dist,
            time_limits,
            network_type_enum,
            hull_type_enum,
        ))
        .unwrap();

    // Convert from Geo::Polygon to GeoJSON string
    let mut isochrones_converted = Vec::new();
    for isochrone in isochrones {
        let converted = utils::polygon_to_geojson_string(&isochrone);
        isochrones_converted.push(converted);
    }

    Ok(isochrones_converted)
}

/// Python module for quickly creating isochrones
#[pymodule]
fn pysochrone(_py: Python, m: &PyModule) -> pyo3::PyResult<()> {
    m.add_function(wrap_pyfunction!(calc_isochrones, m)?)?;
    Ok(())
}

