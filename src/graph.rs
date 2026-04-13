use petgraph::graph::{DiGraph, NodeIndex};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
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
    pub fn filter_useful_tags(&self) -> Self {
        static USEFUL_TAGS: &[&str] = &[
            "bridge", "tunnel", "oneway", "lanes", "ref", "name",
            "highway", "maxspeed", "service", "access", "area",
            "landuse", "width", "est_width", "junction",
        ];
        let useful_tags_set: HashSet<&str> = USEFUL_TAGS.iter().copied().collect();

        let filtered_tags: Vec<XmlTag> = self.tags.iter()
            .filter(|tag| useful_tags_set.contains(tag.key.as_str()))
            .cloned()
            .collect();

        XmlWay {
            id: self.id,
            nodes: self.nodes.clone(),
            tags: filtered_tags,
            length: self.length,
            speed_kph: self.speed_kph,
            walk_travel_time: self.walk_travel_time,
            bike_travel_time: self.bike_travel_time,
            drive_travel_time: self.drive_travel_time,
        }
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
        let node_index = graph.add_node(node.clone());
        node_index_map.insert(node.id, node_index);
    }

    // Add edges to the graph
    for way in ways {
        let filtered_way = way.filter_useful_tags();
        let path_direction = assess_path_directionality(&filtered_way);

        for window in filtered_way.nodes.windows(2) {
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

    // Add travel time as edge weights
    // Assign default highway speeds
    let hwy_speeds = HashMap::from([
        ("school zone".to_string(), 30.0),
        ("urban".to_string(), 30.0),
        ("residential".to_string(), 50.0),
        ("rural".to_string(), 88.5),
        ("motorway".to_string(), 88.5),
        ("highway".to_string(), 88.5), // ... other highway types and their typical speeds
    ]);
    let fallback_speed = 50.0; // Fallback speed in kph

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

fn build_path(
    graph: &DiGraph<XmlNode, XmlWay>,
    start: NodeIndex,
    endpoints: &HashSet<NodeIndex>,
) -> Vec<NodeIndex> {
    let mut path = vec![start];
    let mut current = start;

    while !endpoints.contains(&current) {
        if let Some(successor) = graph
            .neighbors_directed(current, petgraph::Outgoing)
            .find(|&n| !path.contains(&n))
        {
            path.push(successor);
            current = successor;
        } else {
            break;
        }
    }

    path
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
#[derive(Clone)]
pub struct SpatialGraph {
    pub graph: DiGraph<XmlNode, XmlWay>,
    tree: RTree<NodeEntry>,
}

impl SpatialGraph {
    pub fn new(graph: DiGraph<XmlNode, XmlWay>) -> Self {
        let entries = graph.node_indices()
            .map(|i| NodeEntry { point: [graph[i].lat, graph[i].lon], index: i })
            .collect();
        let tree = RTree::bulk_load(entries);
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
