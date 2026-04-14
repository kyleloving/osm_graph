use crate::cache;
use crate::error::OsmGraphError;
use crate::graph::XmlData;
use crate::overpass;
use geo::{Contains, Coord, LineString, Point, Polygon};
use std::collections::HashMap;

pub struct Poi {
    pub id: i64,
    pub lat: f64,
    pub lon: f64,
    pub tags: HashMap<String, String>,
}

fn create_poi_query(bbox: &str) -> String {
    format!(
        "[out:xml];(\
         node[\"tourism\"]({bbox});\
         node[\"historic\"]({bbox});\
         node[\"natural\"~\"peak|waterfall|cave_entrance|beach|hot_spring\"]({bbox});\
         node[\"amenity\"~\"museum|theatre|cinema|arts_centre|library\"]({bbox});\
         node[\"leisure\"~\"park|nature_reserve|garden\"]({bbox});\
         );out;"
    )
}

/// Extract a `south,west,north,east` Overpass bbox from an isochrone polygon.
/// Internal coordinate convention: x = lat, y = lon.
fn bbox_from_polygon(polygon: &Polygon<f64>) -> String {
    let mut min_lat = f64::MAX;
    let mut max_lat = f64::MIN;
    let mut min_lon = f64::MAX;
    let mut max_lon = f64::MIN;
    for coord in polygon.exterior().coords() {
        min_lat = min_lat.min(coord.x);
        max_lat = max_lat.max(coord.x);
        min_lon = min_lon.min(coord.y);
        max_lon = max_lon.max(coord.y);
    }
    format!("{},{},{},{}", min_lat, min_lon, max_lat, max_lon)
}

/// Parse a GeoJSON geometry string (as produced by `polygon_to_geojson_string`)
/// back into a `geo::Polygon<f64>` using the library's internal x=lat, y=lon convention.
pub fn parse_isochrone(geojson_str: &str) -> Result<Polygon<f64>, OsmGraphError> {
    let gj: geojson::GeoJson = geojson_str
        .parse()
        .map_err(|_| OsmGraphError::InvalidInput("invalid GeoJSON".into()))?;
    let rings = match gj {
        geojson::GeoJson::Geometry(geom) => match geom.value {
            geojson::Value::Polygon(rings) => rings,
            _ => return Err(OsmGraphError::InvalidInput("expected Polygon geometry".into())),
        },
        _ => return Err(OsmGraphError::InvalidInput("expected a GeoJSON Geometry, not a Feature or FeatureCollection".into())),
    };
    // GeoJSON coords are [lon, lat]; internal convention is x=lat, y=lon.
    let exterior: Vec<Coord<f64>> = rings[0]
        .iter()
        .map(|c| Coord { x: c[1], y: c[0] })
        .collect();
    Ok(Polygon::new(LineString::from(exterior), vec![]))
}

pub async fn fetch_pois_within(polygon: &Polygon<f64>) -> Result<Vec<Poi>, OsmGraphError> {
    let bbox = bbox_from_polygon(polygon);
    let query = create_poi_query(&bbox);

    let xml = if let Some(cached) = cache::check_xml_cache(&query)? {
        cached
    } else if let Some(disk) = cache::check_disk_xml_cache(&query) {
        cache::insert_into_xml_cache(query.clone(), disk.clone())?;
        disk
    } else {
        let fetched = overpass::make_request("https://overpass-api.de/api/interpreter", &query).await?;
        cache::write_disk_xml_cache(&query, &fetched);
        cache::insert_into_xml_cache(query.clone(), fetched.clone())?;
        fetched
    };

    let data: XmlData = quick_xml::de::from_str(&xml)?;

    let pois = data
        .nodes
        .into_iter()
        .filter(|n| polygon.contains(&Point::new(n.lat, n.lon)))
        .map(|n| Poi {
            id: n.id,
            lat: n.lat,
            lon: n.lon,
            tags: n.tags.into_iter().map(|t| (t.key, t.value)).collect(),
        })
        .collect();

    Ok(pois)
}

pub fn pois_to_geojson(pois: &[Poi]) -> String {
    let features: Vec<geojson::Feature> = pois
        .iter()
        .map(|poi| {
            // GeoJSON spec: [longitude, latitude]
            let geometry = geojson::Geometry::new(geojson::Value::Point(vec![poi.lon, poi.lat]));
            let props: geojson::JsonObject = poi
                .tags
                .iter()
                .map(|(k, v)| (k.clone(), geojson::JsonValue::String(v.clone())))
                .collect();
            geojson::Feature {
                geometry: Some(geometry),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_isochrone_valid() {
        // GeoJSON uses [lon, lat]; internal convention is x=lat, y=lon
        let geojson = r#"{"type":"Polygon","coordinates":[[[11.0,48.0],[11.1,48.0],[11.05,48.1],[11.0,48.0]]]}"#;
        let polygon = parse_isochrone(geojson).unwrap();
        let first = polygon.exterior().coords().next().unwrap();
        // [11.0, 48.0] → x=48.0 (lat), y=11.0 (lon)
        assert!((first.x - 48.0).abs() < 1e-9, "x should be lat (48.0), got {}", first.x);
        assert!((first.y - 11.0).abs() < 1e-9, "y should be lon (11.0), got {}", first.y);
    }

    #[test]
    fn test_parse_isochrone_invalid_json() {
        let result = parse_isochrone("not valid json");
        assert!(matches!(result, Err(crate::error::OsmGraphError::InvalidInput(_))));
    }

    #[test]
    fn test_parse_isochrone_wrong_geometry_type() {
        let geojson = r#"{"type":"Point","coordinates":[11.0,48.0]}"#;
        let result = parse_isochrone(geojson);
        assert!(matches!(result, Err(crate::error::OsmGraphError::InvalidInput(_))));
    }

    #[test]
    fn test_pois_to_geojson_empty() {
        let json = pois_to_geojson(&[]);
        let gj: geojson::GeoJson = json.parse().unwrap();
        if let geojson::GeoJson::FeatureCollection(fc) = gj {
            assert_eq!(fc.features.len(), 0);
        } else {
            panic!("expected FeatureCollection");
        }
    }

    #[test]
    fn test_pois_to_geojson_coordinate_order() {
        // GeoJSON spec: coordinates must be [longitude, latitude]
        let poi = Poi { id: 1, lat: 48.0, lon: 11.0, tags: HashMap::new() };
        let json = pois_to_geojson(&[poi]);
        let gj: geojson::GeoJson = json.parse().unwrap();
        if let geojson::GeoJson::FeatureCollection(fc) = gj {
            let geom = fc.features[0].geometry.as_ref().unwrap();
            if let geojson::Value::Point(coords) = &geom.value {
                assert!((coords[0] - 11.0).abs() < 1e-9, "first coord should be lon");
                assert!((coords[1] - 48.0).abs() < 1e-9, "second coord should be lat");
            } else {
                panic!("expected Point geometry");
            }
        } else {
            panic!("expected FeatureCollection");
        }
    }

    #[test]
    fn test_pois_to_geojson_tags_as_properties() {
        let mut tags = HashMap::new();
        tags.insert("tourism".to_string(), "museum".to_string());
        let poi = Poi { id: 1, lat: 48.0, lon: 11.0, tags };
        let json = pois_to_geojson(&[poi]);
        let gj: geojson::GeoJson = json.parse().unwrap();
        if let geojson::GeoJson::FeatureCollection(fc) = gj {
            let props = fc.features[0].properties.as_ref().unwrap();
            assert_eq!(props["tourism"], geojson::JsonValue::String("museum".to_string()));
        } else {
            panic!("expected FeatureCollection");
        }
    }
}
