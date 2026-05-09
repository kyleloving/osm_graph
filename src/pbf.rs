//! Read OpenStreetMap PBF files into the same intermediate shape produced by
//! the Overpass XML parser. This lets the rest of the pipeline (graph building,
//! POI extraction) work unchanged whether the data came from live Overpass or
//! a local PBF file.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader};

use crate::error::OsmGraphError;
use crate::filters::{is_poi_node, way_passes_road_filter};
use crate::graph::{SpatialGraph, XmlData, XmlNode, XmlNodeRef, XmlTag, XmlWay};
use crate::overpass::NetworkType;
use crate::poi::Poi;

impl SpatialGraph {
    /// Build a routable [`SpatialGraph`] directly from a local OSM PBF file.
    ///
    /// POIs are parsed separately from road-network nodes and pre-snapped onto
    /// the graph. Use [`read_pbf`] when you need access to the intermediate
    /// [`XmlData`] or raw [`Poi`] list.
    pub fn from_pbf(
        path: impl AsRef<Path>,
        network_type: NetworkType,
        retain_all: Option<bool>,
    ) -> Result<Self, OsmGraphError> {
        let (data, pois) = read_pbf(path, network_type)?;
        let mut spatial_graph =
            SpatialGraph::from_parsed_osm(data, network_type, retain_all.unwrap_or(false));
        spatial_graph.snap_pois(&pois);
        Ok(spatial_graph)
    }
}

/// Read a PBF file once and produce one `XmlData` per requested network type,
/// plus the POIs found in the extract (POIs are network-type-independent).
///
/// This avoids re-reading the PBF for each network type — useful at server
/// startup when you want walk/bike/drive graphs for the same region.
pub fn read_pbf_multi(
    path: impl AsRef<Path>,
    network_types: &[NetworkType],
) -> Result<(HashMap<NetworkType, XmlData>, Vec<Poi>), OsmGraphError> {
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
                all_nodes.insert(
                    id,
                    RawNode {
                        lat: node.lat(),
                        lon: node.lon(),
                        tags,
                    },
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
                    RawNode {
                        lat: node.lat(),
                        lon: node.lon(),
                        tags,
                    },
                );
            }
            Element::Way(way) => {
                let tags: Vec<(String, String)> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                // Quick reject: ways without a highway tag aren't roads for any mode.
                if !tags.iter().any(|(k, _)| k == "highway") {
                    return;
                }
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

    let pois = pois_from_nodes(&all_nodes, &poi_ids);

    // Per-network-type, emit only the road nodes referenced by that type's ways.
    // POIs are returned separately so POI-only nodes do not enter the routable graph.
    let mut out: HashMap<NetworkType, XmlData> = HashMap::new();
    for (nt, roads) in roads_by_type {
        let mut needed: HashSet<i64> = HashSet::new();
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
                tags: n
                    .tags
                    .iter()
                    .cloned()
                    .map(|(k, v)| XmlTag { key: k, value: v })
                    .collect(),
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
        out.insert(nt, XmlData { nodes, ways });
    }

    Ok((out, pois))
}

/// Read a PBF file and produce an `XmlData` (the canonical intermediate shape
/// our graph builder consumes) plus the POIs found in the extract.
///
/// Two-pass logic implemented in a single PBF iteration:
///   1. Collect every node into a temporary map (id → lat/lon/tags).
///   2. Collect every way that passes the road-network filter for `network_type`.
///   3. Collect POI nodes separately (any node with our standard amenity/tourism/etc. tags).
///
/// After iteration, emit only road-network nodes in `XmlData`: nodes referenced
/// by a kept way. POIs are returned as [`Poi`] values and can be snapped onto a
/// [`crate::graph::SpatialGraph`] afterward.
pub fn read_pbf(
    path: impl AsRef<Path>,
    network_type: NetworkType,
) -> Result<(XmlData, Vec<Poi>), OsmGraphError> {
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
                    RawNode {
                        lat: node.lat(),
                        lon: node.lon(),
                        tags,
                    },
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
                    RawNode {
                        lat: node.lat(),
                        lon: node.lon(),
                        tags,
                    },
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
                roads.push(RawWay {
                    id: way.id(),
                    refs,
                    tags,
                });
            }
            Element::Relation(_) => {}
        })
        .map_err(|e| OsmGraphError::PbfError(e.to_string()))?;

    let pois = pois_from_nodes(&all_nodes, &poi_ids);

    // Build the set of road nodes we actually need to keep.
    let mut needed: HashSet<i64> = HashSet::new();
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

    Ok((XmlData { nodes, ways }, pois))
}

fn pois_from_nodes(all_nodes: &HashMap<i64, RawNode>, poi_ids: &HashSet<i64>) -> Vec<Poi> {
    poi_ids
        .iter()
        .filter_map(|id| {
            let node = all_nodes.get(id)?;
            Some(Poi {
                id: *id,
                lat: node.lat,
                lon: node.lon,
                tags: node.tags.iter().cloned().collect(),
            })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pois_from_nodes_preserves_poi_data() {
        let mut nodes = HashMap::new();
        nodes.insert(
            10,
            RawNode {
                lat: 38.9,
                lon: -77.0,
                tags: vec![("amenity".into(), "restaurant".into())],
            },
        );
        nodes.insert(
            20,
            RawNode {
                lat: 39.0,
                lon: -77.1,
                tags: vec![("highway".into(), "traffic_signals".into())],
            },
        );
        let poi_ids = HashSet::from([10]);

        let pois = pois_from_nodes(&nodes, &poi_ids);

        assert_eq!(pois.len(), 1);
        assert_eq!(pois[0].id, 10);
        assert_eq!(pois[0].lat, 38.9);
        assert_eq!(pois[0].lon, -77.0);
        assert_eq!(pois[0].tags["amenity"], "restaurant");
    }
}
