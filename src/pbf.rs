//! Read OpenStreetMap PBF files into the same intermediate shape produced by
//! the Overpass XML parser. This lets the rest of the pipeline (graph building,
//! POI extraction) work unchanged whether the data came from live Overpass or
//! a local PBF file.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader};

use crate::error::OsmGraphError;
use crate::graph::{XmlData, XmlNode, XmlNodeRef, XmlTag, XmlWay};
use crate::overpass::NetworkType;

/// Read a PBF file once and produce one `XmlData` per requested network type,
/// plus the set of node IDs that are POIs (POIs are network-type-independent).
///
/// This avoids re-reading the PBF for each network type — useful at server
/// startup when you want walk/bike/drive graphs for the same region.
pub fn read_pbf_multi(
    path: impl AsRef<Path>,
    network_types: &[NetworkType],
) -> Result<(HashMap<NetworkType, XmlData>, HashSet<i64>), OsmGraphError> {
    let mut all_nodes: HashMap<i64, RawNode> = HashMap::new();
    let mut roads_by_type: HashMap<NetworkType, Vec<RawWay>> =
        network_types.iter().map(|nt| (*nt, Vec::new())).collect();
    let mut poi_ids: HashSet<i64> = HashSet::new();

    let reader = ElementReader::from_path(path.as_ref())
        .map_err(|e| OsmGraphError::PbfError(e.to_string()))?;

    reader
        .for_each(|element| match element {
            Element::Node(node) => {
                let tags: Vec<(String, String)> = node
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let id = node.id();
                if is_poi_node(&tags) {
                    poi_ids.insert(id);
                }
                all_nodes.insert(id, RawNode { lat: node.lat(), lon: node.lon(), tags });
            }
            Element::DenseNode(node) => {
                let tags: Vec<(String, String)> = node
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let id = node.id();
                if is_poi_node(&tags) {
                    poi_ids.insert(id);
                }
                all_nodes.insert(id, RawNode { lat: node.lat(), lon: node.lon(), tags });
            }
            Element::Way(way) => {
                let tags: Vec<(String, String)> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                // Quick reject: ways without a highway tag aren't roads for any mode.
                if !tags.iter().any(|(k, _)| k == "highway") { return; }
                let refs: Vec<i64> = way.refs().collect();
                for &nt in network_types {
                    if way_passes_road_filter(&tags, nt) {
                        roads_by_type.get_mut(&nt).unwrap().push(RawWay {
                            id: way.id(),
                            refs: refs.clone(),
                            tags: tags.clone(),
                        });
                    }
                }
            }
            Element::Relation(_) => {}
        })
        .map_err(|e| OsmGraphError::PbfError(e.to_string()))?;

    // Per-network-type, emit only the nodes referenced by that type's ways
    // (plus all POI nodes — they're shared across all network types).
    let mut out: HashMap<NetworkType, XmlData> = HashMap::new();
    for (nt, roads) in roads_by_type {
        let mut needed: HashSet<i64> = poi_ids.clone();
        for w in &roads {
            for r in &w.refs {
                needed.insert(*r);
            }
        }
        let nodes: Vec<XmlNode> = all_nodes
            .iter()
            .filter(|(id, _)| needed.contains(id))
            .map(|(id, n)| XmlNode {
                id: *id,
                lat: n.lat,
                lon: n.lon,
                tags: n.tags.iter().cloned()
                    .map(|(k, v)| XmlTag { key: k, value: v })
                    .collect(),
                geohash: None,
            })
            .collect();
        let ways: Vec<XmlWay> = roads
            .into_iter()
            .map(|w| XmlWay {
                id: w.id,
                nodes: w.refs.into_iter().map(|node_id| XmlNodeRef { node_id }).collect(),
                tags: w.tags.into_iter().map(|(k, v)| XmlTag { key: k, value: v }).collect(),
                length: 0.0, speed_kph: 0.0,
                walk_travel_time: 0.0, bike_travel_time: 0.0, drive_travel_time: 0.0,
            })
            .collect();
        out.insert(nt, XmlData { nodes, ways});
    }

    Ok((out, poi_ids))
}

/// Read a PBF file and produce an `XmlData` (the canonical intermediate shape
/// our graph builder consumes) plus the set of node IDs that are POIs.
///
/// Two-pass logic implemented in a single PBF iteration:
///   1. Collect every node into a temporary map (id → lat/lon/tags).
///   2. Collect every way that passes the road-network filter for `network_type`.
///   3. Mark POI nodes (any node with our standard amenity/tourism/etc. tags).
///
/// After iteration, emit only the nodes we actually need: those referenced by
/// a kept way, or flagged as a POI. Everything else is discarded — for DC this
/// drops the ~4 million tagless nodes.
pub fn read_pbf(
    path: impl AsRef<Path>,
    network_type: NetworkType,
) -> Result<(XmlData, HashSet<i64>), OsmGraphError> {
    let mut all_nodes: HashMap<i64, RawNode> = HashMap::new();
    let mut roads: Vec<RawWay> = Vec::new();
    let mut poi_ids: HashSet<i64> = HashSet::new();

    let reader = ElementReader::from_path(path.as_ref())
        .map_err(|e| OsmGraphError::PbfError(e.to_string()))?;

    reader
        .for_each(|element| match element {
            Element::Node(node) => {
                let tags: Vec<(String, String)> = node
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let id = node.id();
                if is_poi_node(&tags) {
                    poi_ids.insert(id);
                }
                all_nodes.insert(
                    id,
                    RawNode { lat: node.lat(), lon: node.lon(), tags },
                );
            }
            Element::DenseNode(node) => {
                let tags: Vec<(String, String)> = node
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let id = node.id();
                if is_poi_node(&tags) {
                    poi_ids.insert(id);
                }
                all_nodes.insert(
                    id,
                    RawNode { lat: node.lat(), lon: node.lon(), tags },
                );
            }
            Element::Way(way) => {
                let tags: Vec<(String, String)> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                if !way_passes_road_filter(&tags, network_type) {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                roads.push(RawWay { id: way.id(), refs, tags });
            }
            Element::Relation(_) => {}
        })
        .map_err(|e| OsmGraphError::PbfError(e.to_string()))?;

    // Build the set of nodes we actually need to keep.
    let mut needed: HashSet<i64> = poi_ids.clone();
    for w in &roads {
        for r in &w.refs {
            needed.insert(*r);
        }
    }

    let nodes: Vec<XmlNode> = all_nodes
        .into_iter()
        .filter(|(id, _)| needed.contains(id))
        .map(|(id, n)| XmlNode {
            id,
            lat: n.lat,
            lon: n.lon,
            tags: n
                .tags
                .into_iter()
                .map(|(k, v)| XmlTag { key: k, value: v })
                .collect(),
            geohash: None,
        })
        .collect();

    let ways: Vec<XmlWay> = roads
        .into_iter()
        .map(|w| XmlWay {
            id: w.id,
            nodes: w
                .refs
                .into_iter()
                .map(|node_id| XmlNodeRef { node_id })
                .collect(),
            tags: w
                .tags
                .into_iter()
                .map(|(k, v)| XmlTag { key: k, value: v })
                .collect(),
            length: 0.0,
            speed_kph: 0.0,
            walk_travel_time: 0.0,
            bike_travel_time: 0.0,
            drive_travel_time: 0.0,
        })
        .collect();

    Ok((XmlData { nodes, ways}, poi_ids))
}

struct RawNode {
    lat: f64,
    lon: f64,
    tags: Vec<(String, String)>,
}

struct RawWay {
    id: i64,
    refs: Vec<i64>,
    tags: Vec<(String, String)>,
}

/// Mirror of `overpass::get_osm_filter`. If Overpass filter rules ever change,
/// these need to change in lockstep.
fn way_passes_road_filter(tags: &[(String, String)], network_type: NetworkType) -> bool {
    let get = |k: &str| {
        tags.iter()
            .find(|(tk, _)| tk == k)
            .map(|(_, v)| v.as_str())
    };

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
                "abandoned", "bridleway", "bus_guideway", "construction", "corridor",
                "cycleway", "elevator", "escalator", "footway", "no", "path", "pedestrian",
                "planned", "platform", "proposed", "raceway", "razed", "service", "steps", "track",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) { return false; }
            if get("motor_vehicle") == Some("no") { return false; }
            if get("motorcar") == Some("no") { return false; }
            const EXCLUDE_SERVICE: &[&str] = &[
                "alley", "driveway", "emergency_access", "parking", "parking_aisle", "private",
            ];
            if let Some(s) = get("service") {
                if EXCLUDE_SERVICE.contains(&s) { return false; }
            }
        }
        NetworkType::DriveService => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned", "bridleway", "bus_guideway", "construction", "corridor",
                "cycleway", "elevator", "escalator", "footway", "no", "path", "pedestrian",
                "planned", "platform", "proposed", "raceway", "razed", "steps", "track",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) { return false; }
            if get("motor_vehicle") == Some("no") { return false; }
            if get("motorcar") == Some("no") { return false; }
            const EXCLUDE_SERVICE: &[&str] = &[
                "emergency_access", "parking", "parking_aisle", "private",
            ];
            if let Some(s) = get("service") {
                if EXCLUDE_SERVICE.contains(&s) { return false; }
            }
        }
        NetworkType::Walk => {
            // "motor" is a substring pattern in Overpass — matches motor, motorway, motorroad.
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned", "bus_guideway", "construction", "corridor", "elevator", "escalator",
                "no", "planned", "platform", "proposed", "raceway", "razed",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) || highway.starts_with("motor") { return false; }
            if get("foot") == Some("no") { return false; }
            if get("service") == Some("private") { return false; }
        }
        NetworkType::Bike => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned", "bus_guideway", "construction", "corridor", "elevator", "escalator",
                "footway", "no", "planned", "platform", "proposed", "raceway", "razed", "steps",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) || highway.starts_with("motor") { return false; }
            if get("bicycle") == Some("no") { return false; }
            if get("service") == Some("private") { return false; }
        }
        NetworkType::All => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned", "construction", "no", "planned", "platform", "proposed", "raceway", "razed",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) { return false; }
            if get("service") == Some("private") { return false; }
        }
        NetworkType::AllPrivate => {
            const EXCLUDE_HIGHWAY: &[&str] = &[
                "abandoned", "construction", "no", "planned", "platform", "proposed", "raceway", "razed",
            ];
            if EXCLUDE_HIGHWAY.contains(&highway) { return false; }
        }
    }
    true
}

/// Mirror of the node selectors in `poi::create_poi_query`.
fn is_poi_node(tags: &[(String, String)]) -> bool {
    let get = |k: &str| {
        tags.iter()
            .find(|(tk, _)| tk == k)
            .map(|(_, v)| v.as_str())
    };

    if get("tourism").is_some() { return true; }
    if get("historic").is_some() { return true; }

    if let Some(v) = get("natural") {
        if matches!(v, "peak" | "waterfall" | "cave_entrance" | "beach" | "hot_spring") {
            return true;
        }
    }

    if let Some(v) = get("amenity") {
        if matches!(
            v,
            "restaurant" | "fast_food" | "cafe" | "bar" | "pub" | "biergarten" | "ice_cream"
                | "food_court" | "museum" | "theatre" | "cinema" | "arts_centre" | "library"
                | "place_of_worship" | "spa" | "swimming_pool"
        ) {
            return true;
        }
    }

    if let Some(v) = get("leisure") {
        if matches!(v, "park" | "nature_reserve" | "garden" | "sports_centre" | "fitness_centre") {
            return true;
        }
    }

    if let Some(v) = get("shop") {
        if matches!(v, "bakery" | "deli" | "chocolate" | "wine" | "cheese" | "mall" | "department_store") {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poi_detection_amenity() {
        let tags = vec![
            ("amenity".to_string(), "restaurant".to_string()),
            ("name".to_string(), "Joe's".to_string()),
        ];
        assert!(is_poi_node(&tags));
    }

    #[test]
    fn poi_detection_rejects_unrelated_amenity() {
        let tags = vec![("amenity".to_string(), "atm".to_string())];
        assert!(!is_poi_node(&tags));
    }

    #[test]
    fn poi_detection_tourism() {
        // Any tourism tag counts.
        let tags = vec![("tourism".to_string(), "hotel".to_string())];
        assert!(is_poi_node(&tags));
    }

    #[test]
    fn road_filter_walk_keeps_residential() {
        let tags = vec![("highway".to_string(), "residential".to_string())];
        assert!(way_passes_road_filter(&tags, NetworkType::Walk));
    }

    #[test]
    fn road_filter_walk_rejects_motor() {
        let tags = vec![("highway".to_string(), "motorway".to_string())];
        assert!(!way_passes_road_filter(&tags, NetworkType::Walk));
    }

    #[test]
    fn road_filter_drive_rejects_footway() {
        let tags = vec![("highway".to_string(), "footway".to_string())];
        assert!(!way_passes_road_filter(&tags, NetworkType::Drive));
    }

    #[test]
    fn road_filter_rejects_area_yes() {
        let tags = vec![
            ("highway".to_string(), "residential".to_string()),
            ("area".to_string(), "yes".to_string()),
        ];
        assert!(!way_passes_road_filter(&tags, NetworkType::Walk));
    }

    #[test]
    fn road_filter_walk_rejects_foot_no() {
        let tags = vec![
            ("highway".to_string(), "residential".to_string()),
            ("foot".to_string(), "no".to_string()),
        ];
        assert!(!way_passes_road_filter(&tags, NetworkType::Walk));
    }
}
