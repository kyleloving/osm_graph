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
    // node_to_latlon returns (lat, lon) tuples which geo stores as (x=lat, y=lon).
    // GeoJSON spec requires [longitude, latitude], so coord.y = lon, coord.x = lat.
    let exterior_coords = polygon
        .exterior()
        .0
        .iter()
        .map(|coord| vec![coord.y, coord.x]) // [lon, lat] per GeoJSON spec
        .collect::<Vec<_>>();

    let geojson_polygon = Geometry::new(Value::Polygon(vec![exterior_coords]));

    GeoJson::Geometry(geojson_polygon)
}

// Convert polygon to GeoJSON string
pub fn polygon_to_geojson_string(polygon: &Polygon<f64>) -> String {
    let geojson = polygon_to_geojson(polygon);
    geojson.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_same_point() {
        let d = calculate_distance(48.0, 11.0, 48.0, 11.0);
        assert_eq!(d, 0.0);
    }

    #[test]
    fn test_distance_known_value() {
        // Munich to roughly 1km north — should be close to 1000m
        let d = calculate_distance(48.0, 11.0, 48.009, 11.0);
        assert!((d - 1000.0).abs() < 10.0, "Expected ~1000m, got {}", d);
    }

    #[test]
    fn test_distance_is_symmetric() {
        let d1 = calculate_distance(48.0, 11.0, 52.0, 13.0);
        let d2 = calculate_distance(52.0, 13.0, 48.0, 11.0);
        assert!((d1 - d2).abs() < 1e-6);
    }

    #[test]
    fn test_travel_time_basic() {
        // 1000m at 36 kph = 100 seconds
        let t = calculate_travel_time(1000.0, 36.0);
        assert!((t - 100.0).abs() < 1e-6, "Expected 100s, got {}", t);
    }

    #[test]
    fn test_travel_time_walking() {
        // 500m at 5 kph = 360 seconds
        let t = calculate_travel_time(500.0, 5.0);
        assert!((t - 360.0).abs() < 1e-6);
    }
}