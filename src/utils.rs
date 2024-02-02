use geo::Polygon;
use geojson::{GeoJson, Geometry, Value};

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
