use petgraph::graph::{DiGraph, NodeIndex};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use crate::utils::{calculate_distance, calculate_travel_time};
use crate::simplify::simplify_graph;
use rstar::{RTree, RTreeObject, AABB, PointDistance};

#[derive(Debug, Deserialize)]
pub struct XmlData {
    #[serde(rename = "node", default)]
    pub nodes: Vec<XmlNode>,
    #[serde(rename = "way", default)]
    pub ways: Vec<XmlWay>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct XmlNode {
    #[serde(rename = "@id")]
    pub id: i64,
    #[serde(rename = "@lat")]
    pub lat: f64,
    #[serde(rename = "@lon")]
    pub lon: f64,
    #[serde(rename = "tag", default)]
    pub tags: Vec<XmlTag>,
    pub geohash: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct XmlWay {
    #[serde(rename = "@id")]
    pub id: i64,
    #[serde(rename = "nd", default)]
    pub nodes: Vec<XmlNodeRef>,
    #[serde(rename = "tag", default)]
    pub tags: Vec<XmlTag>,
    #[serde(default)]
    pub length: f64,
    #[serde(default)]
    pub speed_kph: f64,
    #[serde(default)]
    pub walk_travel_time: f64,
    #[serde(default)]
    pub bike_travel_time: f64,
    #[serde(default)]
    pub drive_travel_time: f64,
}

impl XmlWay {
    pub fn filter_useful_tags(self) -> Self {
        const USEFUL_TAGS: &[&str] = &[
            "bridge", "tunnel", "oneway", "lanes", "ref", "name",
            "highway", "maxspeed", "service", "access", "area",
            "landuse", "width", "est_width", "junction",
        ];
        // Linear search on 15-element static slice — no HashSet allocation needed.
        let tags = self.tags.into_iter()
            .filter(|tag| USEFUL_TAGS.iter().any(|&k| k == tag.key.as_str()))
            .collect();
        XmlWay { tags, ..self }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct XmlNodeRef {
    #[serde(rename = "@ref")]
    pub node_id: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct XmlTag {
    #[serde(rename = "@k")]
    pub key: String,
    #[serde(rename = "@v")]
    pub value: String,
}

struct PathDirectionality {
    is_one_way: bool,
    is_reversed: bool,
}

// Function to parse the XML response
pub fn parse_xml(xml_data: &str) -> Result<XmlData, quick_xml::DeError> {
    let root: XmlData = quick_xml::de::from_str(xml_data)?;
    Ok(root)
}

fn find_tag<'a>(tags: &'a [XmlTag], key: &str) -> Option<&'a XmlTag> {
    tags.iter().find(|tag| tag.key == key)
}

fn assess_path_directionality(path: &XmlWay) -> PathDirectionality {
    let oneway_tag = find_tag(&path.tags, "oneway");
    let junction_tag = find_tag(&path.tags, "junction");

    let is_one_way = match oneway_tag {
        Some(tag) => {
            // The oneway tag can have several values indicating true: "yes", "true", "1"
            // or indicating reversed: "-1", "reverse"
            // Any other value (including absence of the tag) defaults to false
            matches!(tag.value.as_str(), "yes" | "true" | "1" | "-1" | "reverse")
        },
        None => false,
    };

    let is_reversed = oneway_tag.map_or(false, |tag| {
        matches!(tag.value.as_str(), "-1" | "reverse")
    });

    // Roundabouts are considered one-way implicitly
    let is_roundabout = junction_tag.map_or(false, |tag| tag.value == "roundabout");

    PathDirectionality {
        is_one_way: is_one_way || is_roundabout,
        is_reversed,
    }
}

// Function to create the network graph
pub fn create_graph(
    nodes: Vec<XmlNode>,
    ways: Vec<XmlWay>,
    retain_all: bool,
    _bidirectional: bool,
) -> DiGraph<XmlNode, XmlWay> {
    let mut graph = DiGraph::<XmlNode, XmlWay>::new();
    let mut node_index_map = HashMap::new();

    // Add nodes to the graph and keep track of their indices
    for node in nodes {
        let id = node.id;
        let node_index = graph.add_node(node); // move — no clone needed, nodes is already owned
        node_index_map.insert(id, node_index);
    }

    // Add edges to the graph
    for mut way in ways {
        // Extract node refs before consuming `way` so that edge weights are stored
        // without the construction-only node list (saves memory for every edge in the graph).
        let node_refs = std::mem::take(&mut way.nodes);
        let filtered_way = way.filter_useful_tags();
        let path_direction = assess_path_directionality(&filtered_way);

        for window in node_refs.windows(2) {
            if let [start_ref, end_ref] = window {
                let (start_index, end_index) = if path_direction.is_reversed {
                    (
                        node_index_map[&end_ref.node_id],
                        node_index_map[&start_ref.node_id],
                    )
                } else {
                    (
                        node_index_map[&start_ref.node_id],
                        node_index_map[&end_ref.node_id],
                    )
                };

                graph.add_edge(start_index, end_index, filtered_way.clone());
                if !path_direction.is_one_way {
                    graph.add_edge(end_index, start_index, filtered_way.clone());
                }
            }
        }
    }

    // Add distance as edge weights
    add_edge_lengths(&mut graph);

    // Standard OSM highway type speeds (kph), based on typical urban defaults.
    // These apply when no maxspeed tag is present.
    let hwy_speeds = HashMap::from([
        ("motorway".to_string(),       110.0),
        ("motorway_link".to_string(),   60.0),
        ("trunk".to_string(),           90.0),
        ("trunk_link".to_string(),      45.0),
        ("primary".to_string(),         65.0),
        ("primary_link".to_string(),    45.0),
        ("secondary".to_string(),       55.0),
        ("secondary_link".to_string(),  40.0),
        ("tertiary".to_string(),        45.0),
        ("tertiary_link".to_string(),   35.0),
        ("unclassified".to_string(),    45.0),
        ("residential".to_string(),     30.0),
        ("living_street".to_string(),   10.0),
        ("service".to_string(),         20.0),
        ("track".to_string(),           20.0),
        ("road".to_string(),            50.0),
    ]);
    let fallback_speed = 50.0;

    add_edge_speeds(&mut graph, &hwy_speeds, fallback_speed);
    add_edge_travel_times(&mut graph);

    // Simplify graph topology for faster downstream calculations
    // Consolidates distance and speed from
    if !retain_all {
        graph = simplify_graph(&graph)
    }
    // ... other future logic

    graph
}

fn add_edge_lengths(graph: &mut DiGraph<XmlNode, XmlWay>) {
    for edge in graph.edge_indices() {
        let (start_index, end_index) = graph.edge_endpoints(edge).unwrap();
        let start_node = &graph[start_index];
        let end_node = &graph[end_index];

        let distance =
            calculate_distance(start_node.lat, start_node.lon, end_node.lat, end_node.lon);

        let way = graph.edge_weight_mut(edge).unwrap();
        way.length = distance;
    }
}

fn add_edge_speeds(
    graph: &mut DiGraph<XmlNode, XmlWay>,
    hwy_speeds: &HashMap<String, f64>,
    fallback: f64,
) {
    for edge in graph.edge_indices() {
        let way = graph.edge_weight_mut(edge).unwrap();
        let speed = way
            .tags
            .iter()
            .find(|tag| tag.key == "maxspeed")
            .map_or_else(
                || {
                    way.tags
                        .iter()
                        .find(|tag| tag.key == "highway")
                        .and_then(|tag| hwy_speeds.get(&tag.value).copied())
                        .unwrap_or(fallback)
                },
                |tag| clean_maxspeed(&tag.value),
            );
        way.speed_kph = speed;
    }
}

fn clean_maxspeed(maxspeed: &str) -> f64 {
    let mph_to_kph = 1.60934;
    let speed = maxspeed.parse::<f64>().unwrap_or(0.0);
    if maxspeed.to_lowercase().contains("mph") {
        speed * mph_to_kph
    } else {
        speed
    }
}

// Function to add travel times as an edge weight
fn add_edge_travel_times(graph: &mut DiGraph<XmlNode, XmlWay>) {
    for edge in graph.edge_indices() {
        let way = graph.edge_weight_mut(edge).unwrap();
        let walk_travel_time = calculate_travel_time(way.length, 5.0);
        let bike_travel_time = calculate_travel_time(way.length, 15.0);
        let drive_travel_time = calculate_travel_time(way.length, way.speed_kph);

        way.walk_travel_time = walk_travel_time;
        way.bike_travel_time = bike_travel_time;
        way.drive_travel_time = drive_travel_time;
    }
}

pub fn node_to_latlon(graph: &DiGraph<XmlNode, XmlWay>, node_index: NodeIndex) -> (f64, f64) {
    let node = &graph[node_index];
    (node.lat, node.lon)
}

/// R-tree entry pairing a node's coordinates with its NodeIndex.
#[derive(Clone)]
struct NodeEntry {
    point: [f64; 2],
    index: NodeIndex,
}

impl RTreeObject for NodeEntry {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}

impl PointDistance for NodeEntry {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let dlat = self.point[0] - point[0];
        let dlon = self.point[1] - point[1];
        dlat * dlat + dlon * dlon
    }
}

/// A graph bundled with a spatial index for O(log n) nearest-node queries.
/// Build once via `SpatialGraph::new`, reuse for all lookups and routing.
/// Both inner fields are reference-counted, so cloning a `SpatialGraph` is O(1).
#[derive(Clone)]
pub struct SpatialGraph {
    pub graph: Arc<DiGraph<XmlNode, XmlWay>>,
    tree: Arc<RTree<NodeEntry>>,
}

impl SpatialGraph {
    pub fn new(graph: DiGraph<XmlNode, XmlWay>) -> Self {
        let entries = graph.node_indices()
            .map(|i| NodeEntry { point: [graph[i].lat, graph[i].lon], index: i })
            .collect();
        let tree = Arc::new(RTree::bulk_load(entries));
        let graph = Arc::new(graph);
        Self { graph, tree }
    }

    pub fn nearest_node(&self, lat: f64, lon: f64) -> Option<NodeIndex> {
        self.tree.nearest_neighbor(&[lat, lon]).map(|e| e.index)
    }
}

// Keep the free function for backwards compatibility but delegate to SpatialGraph
pub fn latlon_to_node(graph: &DiGraph<XmlNode, XmlWay>, lat: f64, lon: f64) -> Option<NodeIndex> {
    SpatialGraph::new(graph.clone()).nearest_node(lat, lon)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: i64, lat: f64, lon: f64) -> XmlNode {
        XmlNode { id, lat, lon, tags: vec![], geohash: None }
    }

    fn make_way_raw(node_ids: Vec<i64>, tags: Vec<(&str, &str)>) -> XmlWay {
        XmlWay {
            id: 1,
            nodes: node_ids.into_iter().map(|id| XmlNodeRef { node_id: id }).collect(),
            tags: tags.into_iter().map(|(k, v)| XmlTag { key: k.into(), value: v.into() }).collect(),
            length: 0.0, speed_kph: 0.0,
            walk_travel_time: 0.0, bike_travel_time: 0.0, drive_travel_time: 0.0,
        }
    }

    #[test]
    fn test_graph_respects_maxspeed_tag() {
        let nodes = vec![make_node(1, 0.0, 0.0), make_node(2, 0.001, 0.0)];
        let way = make_way_raw(vec![1, 2], vec![("highway", "residential"), ("maxspeed", "30")]);
        let graph = create_graph(vec![nodes[0].clone(), nodes[1].clone()], vec![way], true, false);
        assert_eq!(graph.edge_weights().next().unwrap().speed_kph, 30.0);
    }

    #[test]
    fn test_oneway_produces_single_edge() {
        let nodes = vec![make_node(1, 0.0, 0.0), make_node(2, 0.001, 0.0)];
        let way = make_way_raw(vec![1, 2], vec![("highway", "residential"), ("oneway", "yes")]);
        let graph = create_graph(vec![nodes[0].clone(), nodes[1].clone()], vec![way], true, false);
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_bidirectional_produces_two_edges() {
        let nodes = vec![make_node(1, 0.0, 0.0), make_node(2, 0.001, 0.0)];
        let way = make_way_raw(vec![1, 2], vec![("highway", "residential")]);
        let graph = create_graph(vec![nodes[0].clone(), nodes[1].clone()], vec![way], true, false);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_nearest_node_finds_closest() {
        let mut graph = DiGraph::new();
        graph.add_node(make_node(1, 48.0, 11.0));
        graph.add_node(make_node(2, 52.0, 13.0));
        let sg = SpatialGraph::new(graph);
        let idx = sg.nearest_node(48.001, 11.001).unwrap();
        assert_eq!(sg.graph[idx].id, 1);
    }
}
