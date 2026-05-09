//! Shared OSM tag-filtering logic used by both the Overpass XML pipeline and
//! the PBF pipeline. A single source of truth means road-filter rule changes
//! and POI category additions only need to happen in one place.

use crate::overpass::NetworkType;

// ---------------------------------------------------------------------------
// Road / way filter
// ---------------------------------------------------------------------------

/// Return `true` if a way with the given tags should be included in the road
/// network for `network_type`.
///
/// The rules mirror `overpass::get_osm_filter` exactly — if you change one,
/// change the other. The Overpass filter is a string passed to the API; this
/// function is the equivalent predicate applied to already-fetched data (PBF
/// or cached XML).
pub fn way_passes_road_filter(tags: &[(String, String)], network_type: NetworkType) -> bool {
    let get = |k: &str| tags.iter().find(|(tk, _)| tk == k).map(|(_, v)| v.as_str());

    let highway = match get("highway") {
        Some(v) => v,
        None => return false,
    };
    if get("area") == Some("yes") {
        return false;
    }

    match network_type {
        NetworkType::Drive => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned",
                "bridleway",
                "bus_guideway",
                "construction",
                "corridor",
                "cycleway",
                "elevator",
                "escalator",
                "footway",
                "no",
                "path",
                "pedestrian",
                "planned",
                "platform",
                "proposed",
                "raceway",
                "razed",
                "service",
                "steps",
                "track",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) {
                return false;
            }
            if get("motor_vehicle") == Some("no") {
                return false;
            }
            if get("motorcar") == Some("no") {
                return false;
            }
            const EXCLUDE_SERVICE: &[&str] = &[
                "alley",
                "driveway",
                "emergency_access",
                "parking",
                "parking_aisle",
                "private",
            ];
            if let Some(s) = get("service") {
                if EXCLUDE_SERVICE.contains(&s) {
                    return false;
                }
            }
        }
        NetworkType::DriveService => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned",
                "bridleway",
                "bus_guideway",
                "construction",
                "corridor",
                "cycleway",
                "elevator",
                "escalator",
                "footway",
                "no",
                "path",
                "pedestrian",
                "planned",
                "platform",
                "proposed",
                "raceway",
                "razed",
                "steps",
                "track",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) {
                return false;
            }
            if get("motor_vehicle") == Some("no") {
                return false;
            }
            if get("motorcar") == Some("no") {
                return false;
            }
            const EXCLUDE_SERVICE: &[&str] =
                &["emergency_access", "parking", "parking_aisle", "private"];
            if let Some(s) = get("service") {
                if EXCLUDE_SERVICE.contains(&s) {
                    return false;
                }
            }
        }
        NetworkType::Walk => {
            // "motor" prefix matches motorway, motorroad, etc. — same as the
            // `highway!~"motor"` regex in the Overpass filter.
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned",
                "bus_guideway",
                "construction",
                "corridor",
                "elevator",
                "escalator",
                "no",
                "planned",
                "platform",
                "proposed",
                "raceway",
                "razed",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) || highway.starts_with("motor") {
                return false;
            }
            if get("foot") == Some("no") {
                return false;
            }
            if get("service") == Some("private") {
                return false;
            }
        }
        NetworkType::Bike => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned",
                "bus_guideway",
                "construction",
                "corridor",
                "elevator",
                "escalator",
                "footway",
                "no",
                "planned",
                "platform",
                "proposed",
                "raceway",
                "razed",
                "steps",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) || highway.starts_with("motor") {
                return false;
            }
            if get("bicycle") == Some("no") {
                return false;
            }
            if get("service") == Some("private") {
                return false;
            }
        }
        NetworkType::All => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned",
                "construction",
                "no",
                "planned",
                "platform",
                "proposed",
                "raceway",
                "razed",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) {
                return false;
            }
            if get("service") == Some("private") {
                return false;
            }
        }
        NetworkType::AllPrivate => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned",
                "construction",
                "no",
                "planned",
                "platform",
                "proposed",
                "raceway",
                "razed",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) {
                return false;
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// POI filter
// ---------------------------------------------------------------------------

/// Return `true` if a node with the given tags is a point of interest.
///
/// The categories here must stay in sync with the selectors in
/// `poi::create_poi_query`. If you add a category to the Overpass query,
/// add the matching arm here so PBF parsing picks it up too.
pub fn is_poi_node(tags: &[(String, String)]) -> bool {
    let get = |k: &str| tags.iter().find(|(tk, _)| tk == k).map(|(_, v)| v.as_str());

    if get("tourism").is_some() {
        return true;
    }
    if get("historic").is_some() {
        return true;
    }

    if let Some(v) = get("natural") {
        if matches!(
            v,
            "peak" | "waterfall" | "cave_entrance" | "beach" | "hot_spring"
        ) {
            return true;
        }
    }

    if let Some(v) = get("amenity") {
        if matches!(
            v,
            "restaurant"
                | "fast_food"
                | "cafe"
                | "bar"
                | "pub"
                | "biergarten"
                | "ice_cream"
                | "food_court"
                | "museum"
                | "theatre"
                | "cinema"
                | "arts_centre"
                | "library"
                | "place_of_worship"
                | "spa"
                | "swimming_pool"
        ) {
            return true;
        }
    }

    if let Some(v) = get("leisure") {
        if matches!(
            v,
            "park" | "nature_reserve" | "garden" | "sports_centre" | "fitness_centre"
        ) {
            return true;
        }
    }

    if let Some(v) = get("shop") {
        if matches!(
            v,
            "bakery" | "deli" | "chocolate" | "wine" | "cheese" | "mall" | "department_store"
        ) {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- POI ---

    #[test]
    fn poi_amenity_restaurant() {
        let tags = vec![("amenity".to_string(), "restaurant".to_string())];
        assert!(is_poi_node(&tags));
    }

    #[test]
    fn poi_rejects_unrelated_amenity() {
        let tags = vec![("amenity".to_string(), "atm".to_string())];
        assert!(!is_poi_node(&tags));
    }

    #[test]
    fn poi_any_tourism_tag() {
        let tags = vec![("tourism".to_string(), "hotel".to_string())];
        assert!(is_poi_node(&tags));
    }

    #[test]
    fn poi_no_tags() {
        assert!(!is_poi_node(&[]));
    }

    // --- Road filter ---

    #[test]
    fn road_walk_keeps_residential() {
        let tags = vec![("highway".to_string(), "residential".to_string())];
        assert!(way_passes_road_filter(&tags, NetworkType::Walk));
    }

    #[test]
    fn road_walk_rejects_motorway() {
        let tags = vec![("highway".to_string(), "motorway".to_string())];
        assert!(!way_passes_road_filter(&tags, NetworkType::Walk));
    }

    #[test]
    fn road_drive_rejects_footway() {
        let tags = vec![("highway".to_string(), "footway".to_string())];
        assert!(!way_passes_road_filter(&tags, NetworkType::Drive));
    }

    #[test]
    fn road_rejects_area_yes() {
        let tags = vec![
            ("highway".to_string(), "residential".to_string()),
            ("area".to_string(), "yes".to_string()),
        ];
        assert!(!way_passes_road_filter(&tags, NetworkType::Walk));
    }

    #[test]
    fn road_walk_rejects_foot_no() {
        let tags = vec![
            ("highway".to_string(), "residential".to_string()),
            ("foot".to_string(), "no".to_string()),
        ];
        assert!(!way_passes_road_filter(&tags, NetworkType::Walk));
    }

    #[test]
    fn road_no_highway_tag_rejected() {
        let tags = vec![("name".to_string(), "Some Street".to_string())];
        assert!(!way_passes_road_filter(&tags, NetworkType::Drive));
    }
}
