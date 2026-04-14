// Define an enum for network types
#[derive(Debug, Clone, Copy)]
pub enum NetworkType {
    Drive,
    DriveService,
    Walk,
    Bike,
    All,
    AllPrivate,
}

// Custom error type for better error messages
#[derive(Debug)]
pub enum OverpassError {
    RequestError(reqwest::Error),
    InvalidNetworkType,
}

impl std::fmt::Display for OverpassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverpassError::RequestError(err) => write!(f, "Request Error: {}", err),
            OverpassError::InvalidNetworkType => write!(f, "Invalid Network Type"),
        }
    }
}

// Function to get OSM filter
pub fn get_osm_filter(network_type: NetworkType) -> Result<&'static str, OverpassError> {
    match network_type {
        NetworkType::Drive => Ok(
            "[\"highway\"][\"area\"!~\"yes\"][\"highway\"!~\"abandoned|bridleway|bus_guideway|construction|corridor|cycleway|elevator|escalator|footway|no|path|pedestrian|planned|platform|proposed|raceway|razed|service|steps|track\"][\"motor_vehicle\"!~\"no\"][\"motorcar\"!~\"no\"][\"service\"!~\"alley|driveway|emergency_access|parking|parking_aisle|private\"]"
        ),
        NetworkType::DriveService => Ok(
            "[\"highway\"][\"area\"!~\"yes\"][\"highway\"!~\"abandoned|bridleway|bus_guideway|construction|corridor|cycleway|elevator|escalator|footway|no|path|pedestrian|planned|platform|proposed|raceway|razed|steps|track\"][\"motor_vehicle\"!~\"no\"][\"motorcar\"!~\"no\"][\"service\"!~\"emergency_access|parking|parking_aisle|private\"]"
        ),
        NetworkType::Walk => Ok(
            "[\"highway\"][\"area\"!~\"yes\"][\"highway\"!~\"abandoned|bus_guideway|construction|corridor|elevator|escalator|motor|no|planned|platform|proposed|raceway|razed\"][\"foot\"!~\"no\"][\"service\"!~\"private\"]"
        ),
        NetworkType::Bike => Ok(
            "[\"highway\"][\"area\"!~\"yes\"][\"highway\"!~\"abandoned|bus_guideway|construction|corridor|elevator|escalator|footway|motor|no|planned|platform|proposed|raceway|razed|steps\"][\"bicycle\"!~\"no\"][\"service\"!~\"private\"]"
        ),
        NetworkType::All => Ok(
            "[\"highway\"][\"area\"!~\"yes\"][\"highway\"!~\"abandoned|construction|no|planned|platform|proposed|raceway|razed\"][\"service\"!~\"private\"]"
        ),
        NetworkType::AllPrivate => Ok(
            "[\"highway\"][\"area\"!~\"yes\"][\"highway\"!~\"abandoned|construction|no|planned|platform|proposed|raceway|razed\"]"
        ),
    }
}

// Function to create the Overpass query string
pub fn create_overpass_query(polygon_coord_str: &str, network_type: NetworkType) -> String {
    let filter = get_osm_filter(network_type).unwrap_or("");
    format!("[out:xml];(way{}({});>;);out;", filter, polygon_coord_str)
}

// Reuse a single reqwest::Client across all HTTP calls in the library
lazy_static::lazy_static! {
    pub(crate) static ref CLIENT: reqwest::Client = reqwest::Client::new();
}

// Function to make request to Overpass API
pub async fn make_request(url: &str, query: &str) -> Result<String, reqwest::Error> {
    let response = CLIENT
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(query.to_string())
        .send()
        .await?;

    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(response.error_for_status().unwrap_err())
    }
}

/// Construct a `south,west,north,east` bounding box string from a point and radius.
pub fn bbox_from_point(lat: f64, lon: f64, dist: f64) -> String {
    const EARTH_RADIUS_M: f64 = 6_371_009.0;

    // Calculate deltas
    let delta_lat = (dist / EARTH_RADIUS_M) * (180.0 / std::f64::consts::PI);
    let delta_lon = (dist / EARTH_RADIUS_M) * (180.0 / std::f64::consts::PI)
        / (lat * std::f64::consts::PI / 180.0).cos();

    // Calculate bounding box
    let north = lat + delta_lat;
    let south = lat - delta_lat;
    let east = lon + delta_lon;
    let west = lon - delta_lon;

    // Construct polygon_coord_str for Overpass API query
    format!("{},{},{},{}", south, west, north, east)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bbox_is_symmetric() {
        let bbox = bbox_from_point(48.0, 11.0, 1000.0);
        let parts: Vec<f64> = bbox.split(',').map(|s| s.parse().unwrap()).collect();
        let (south, west, north, east) = (parts[0], parts[1], parts[2], parts[3]);
        assert!((48.0 - south - (north - 48.0)).abs() < 1e-6);
        assert!((11.0 - west - (east - 11.0)).abs() < 1e-6);
    }

    #[test]
    fn test_bbox_larger_dist_gives_larger_box() {
        let small = bbox_from_point(48.0, 11.0, 1_000.0);
        let large = bbox_from_point(48.0, 11.0, 10_000.0);
        let small_parts: Vec<f64> = small.split(',').map(|s| s.parse().unwrap()).collect();
        let large_parts: Vec<f64> = large.split(',').map(|s| s.parse().unwrap()).collect();
        assert!(large_parts[2] > small_parts[2]);
    }
}
