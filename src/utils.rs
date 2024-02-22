use geo::Polygon;
use geojson::{GeoJson, Geometry, Value};

pub fn calculate_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let radius_earth = 6371000.0; // Radius of the Earth in meters

    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();

    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();

    let a = (dlat / 2.0).sin().powi(2) + (dlon / 2.0).sin().powi(2) * lat1.cos() * lat2.cos();
    let c = 2.0 * a.sqrt().asin();

    radius_earth * c // Distance in meters
}

pub fn calculate_travel_time(length: f64, speed_kph: f64) -> f64 {
    let speed_m_per_s = speed_kph / 3.6;
    length / speed_m_per_s // Returns time in seconds
}

pub fn polygon_to_geojson(polygon: &Polygon<f64>) -> GeoJson {
    let exterior_coords = polygon
        .exterior()
        .0
        .iter()
        .map(|coord| vec![coord.y, coord.x])
        .collect::<Vec<_>>();

    let geojson_polygon = Geometry::new(Value::Polygon(vec![exterior_coords]));

    GeoJson::Geometry(geojson_polygon)
}

// Convert polygon to GeoJSON string
pub fn polygon_to_geojson_string(polygon: &Polygon<f64>) -> String {
    let geojson = polygon_to_geojson(polygon);
    geojson.to_string()
}
