use petgraph::graph::{DiGraph, NodeIndex};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use crate::utils::{calculate_distance, calculate_travel_time};

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
    pub travel_time: f64,
}

impl XmlWay {
    pub fn filter_useful_tags(&self) -> XmlWay {
        let useful_tags_way: HashSet<&str> = [
            "bridge",
            "tunnel",
            "oneway",
            "lanes",
            "ref",
            "name",
            "highway",
            "maxspeed",
            "service",
            "access",
            "area",
            "landuse",
            "width",
            "est_width",
            "junction",
        ]
        .iter()
        .copied()
        .collect();

        let filtered_tags: Vec<XmlTag> = self.tags.iter()
            .filter(|tag| useful_tags_way.contains(tag.key.as_str()))
            .cloned()
            .collect();

        XmlWay {
            tags: filtered_tags,
            ..self.clone()
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

// Function to parse the XML response
pub fn parse_xml(xml_data: &str) -> Result<XmlData, quick_xml::DeError> {
    let root: XmlData = quick_xml::de::from_str(xml_data)?;
    Ok(root)
}

// Function checks whether a path is one way
fn is_path_one_way(path: &XmlWay, bidirectional: bool) -> bool {
    let oneway_values = ["yes", "true", "1", "-1", "reverse", "T", "F"];

    // Rule 1: Bi-directional network type
    if bidirectional {
        return false;
    }

    // Rule 2: Check 'oneway' tag
    if let Some(oneway_tag) = path.tags.iter().find(|tag| tag.key == "oneway") {
        return oneway_values.contains(&oneway_tag.value.as_str());
    }

    // Rule 3: Check 'junction' tag for roundabouts
    if let Some(junction_tag) = path.tags.iter().find(|tag| tag.key == "junction") {
        return junction_tag.value == "roundabout";
    }

    false
}

fn is_path_reversed(path: &XmlWay) -> bool {
    let reversed_values = ["-1", "reverse", "T"];
    if let Some(oneway_tag) = path.tags.iter().find(|tag| tag.key == "oneway") {
        return reversed_values.contains(&oneway_tag.value.as_str());
    }

    false
}

fn find_tag<'a>(tags: &'a [XmlTag], key: &str) -> Option<&'a XmlTag> {
    tags.iter().find(|tag| tag.key == key)
}

// Function to create the network graph
pub fn create_graph(
    nodes: Vec<XmlNode>,
    ways: Vec<XmlWay>,
    _retain_all: bool,
    bidirectional: bool,
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
        let is_one_way = is_path_one_way(&filtered_way, bidirectional);
        let is_reversed = is_path_reversed(&filtered_way);

        for window in filtered_way.nodes.windows(2) {
            if let [start_ref, end_ref] = window {
                let (start_index, end_index) = if is_reversed {
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
                if !is_one_way {
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

    // // Simplify graph topology for faster downstream calculations
    // // Consolidates distance and speed from
    // if !retain_all {
    //     graph = simplify_graph(&graph)
    // }

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
                    hwy_speeds
                        .get(&way.tags[0].value)
                        .copied()
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
        let travel_time = calculate_travel_time(way.length, way.speed_kph);
        way.travel_time = travel_time;
    }
}

pub fn node_to_latlon(graph: &DiGraph<XmlNode, XmlWay>, node_index: NodeIndex) -> (f64, f64) {
    let node = &graph[node_index];
    (node.lat, node.lon)
}

pub fn latlon_to_node(graph: &DiGraph<XmlNode, XmlWay>, lat: f64, lon: f64) -> Option<NodeIndex> {
    graph.node_indices().min_by(|&a, &b| {
        let node_a = &graph[a];
        let node_b = &graph[b];
        let dist_a = calculate_distance(node_a.lat, node_a.lon, lat, lon);
        let dist_b = calculate_distance(node_b.lat, node_b.lon, lat, lon);
        dist_a
            .partial_cmp(&dist_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}
