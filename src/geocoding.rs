use crate::error::OsmGraphError;
use crate::overpass::CLIENT;
use serde::Deserialize;

#[derive(Deserialize)]
struct NominatimResult {
    lat: String,
    lon: String,
}

/// Geocode a place name to (lat, lon) using the Nominatim API.
/// Nominatim usage policy requires a descriptive User-Agent and max 1 req/sec.
pub async fn geocode(place: &str) -> Result<(f64, f64), OsmGraphError> {
    let response = CLIENT
        .get("https://nominatim.openstreetmap.org/search")
        .query(&[("q", place), ("format", "json"), ("limit", "1")])
        .header("User-Agent", "osm_graph/0.1 (https://github.com/kyleloving/osm_graph)")
        .send()
        .await?;

    let results: Vec<NominatimResult> = response.json().await?;

    let first = results.into_iter().next()
        .ok_or_else(|| OsmGraphError::GeocodingFailed(place.to_string()))?;

    let lat = first.lat.parse::<f64>()
        .map_err(|_| OsmGraphError::GeocodingFailed(place.to_string()))?;
    let lon = first.lon.parse::<f64>()
        .map_err(|_| OsmGraphError::GeocodingFailed(place.to_string()))?;

    Ok((lat, lon))
}
