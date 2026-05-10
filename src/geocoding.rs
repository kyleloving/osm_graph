use crate::error::OsmGraphError;
use crate::overpass::{
    client, is_retryable_status, nominatim_url, retry_delay, user_agent, wait_for_nominatim_slot,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct NominatimResult {
    lat: String,
    lon: String,
}

/// Geocode a place name to (lat, lon) using the Nominatim API.
/// Nominatim usage policy requires a descriptive User-Agent and max 1 req/sec.
pub async fn geocode(place: &str) -> Result<(f64, f64), OsmGraphError> {
    let url = nominatim_url();
    let mut successful_response = None;
    let mut final_error = None;

    for attempt in 0..=2 {
        wait_for_nominatim_slot().await;
        let response = client()
            .get(&url)
            .query(&[("q", place), ("format", "json"), ("limit", "1")])
            .header("User-Agent", user_agent())
            .send()
            .await?;

        if response.status().is_success() {
            successful_response = Some(response);
            break;
        }

        let status = response.status();
        let headers = response.headers().clone();
        let error = response.error_for_status().unwrap_err();
        if attempt < 2 && is_retryable_status(status) {
            retry_delay(&headers, attempt).await;
            continue;
        }

        final_error = Some(error);
        break;
    }

    let response = match successful_response {
        Some(response) => response,
        None => {
            return Err(final_error
                .expect("geocoding retry loop ended without success or error")
                .into())
        }
    };

    let results: Vec<NominatimResult> = response.json().await?;

    let first = results
        .into_iter()
        .next()
        .ok_or_else(|| OsmGraphError::GeocodingFailed(place.to_string()))?;

    let lat = first
        .lat
        .parse::<f64>()
        .map_err(|_| OsmGraphError::GeocodingFailed(place.to_string()))?;
    let lon = first
        .lon
        .parse::<f64>()
        .map_err(|_| OsmGraphError::GeocodingFailed(place.to_string()))?;

    Ok((lat, lon))
}
