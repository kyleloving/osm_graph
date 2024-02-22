use petgraph::graph::{DiGraph, NodeIndex};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

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

// WIP
fn simplify_graph(graph: &DiGraph<XmlNode, XmlWay>) -> DiGraph<XmlNode, XmlWay> {
    let mut simplified_graph = DiGraph::new();
    let mut endpoints = HashSet::new();
    let mut index_map = HashMap::new();
    let endpoint_attrs: [String] = [];

    // Identify endpoints and add them to the simplified graph
    for node in graph.node_indices() {
        if is_endpoint(graph, node, &endpoint_attrs) {
            endpoints.insert(node);
            let new_index = simplified_graph.add_node(graph[node].clone());
            index_map.insert(node, new_index);
        }
    }

    // Build and simplify paths
    for &endpoint in &endpoints {
        for neighbor in graph.neighbors(endpoint) {
            if endpoints.contains(&neighbor) || simplified_graph.contains_edge(endpoint, neighbor) {
                continue;
            }

            let path = build_path(graph, endpoint, &endpoints);
            if let Some(&last) = path.last() {
                if endpoints.contains(&last) {
                    // Aggregate edge data along the path
                    let mut total_length = 0.0;
                    let mut total_time = 0.0;
                    let mut speeds = Vec::new();

                    for window in path.windows(2) {
                        if let [u, v] = window {
                            if let Some(edge) = graph.find_edge(*u, *v) {
                                let way = graph.edge_weight(edge).unwrap();
                                total_length += way.length;
                                total_time += way.travel_time;
                                speeds.push(way.speed_kph);
                            }
                        }
                    }

                    // Calculate average speed
                    let avg_speed = if !speeds.is_empty() {
                        speeds.iter().sum::<f64>() / speeds.len() as f64
                    } else {
                        0.0
                    };

                    // Create a new XmlWay with the aggregated data
                    let xml_way = XmlWay {
                        id: 0, // You might want to generate a unique ID or handle this differently
                        nodes: vec![],
                        tags: vec![],
                        length: total_length,
                        travel_time: total_time,
                        speed_kph: avg_speed,
                    };
                    let new_endpoint = *index_map.get(&endpoint).unwrap();
                    let new_last = *index_map.get(&last).unwrap();

                    simplified_graph.add_edge(new_endpoint, new_last, xml_way);
                }
            }
        }
    }

    simplified_graph
}

fn is_endpoint(
    graph: &DiGraph<XmlNode, XmlWay>, 
    node_index: NodeIndex,
    endpoint_attrs: &[String],
) -> bool {
    let out_neighbors: HashSet<_> = graph
        .neighbors_directed(node_index, petgraph::Outgoing)
        .collect();
    let in_neighbors: HashSet<_> = graph
        .neighbors_directed(node_index, petgraph::Incoming)
        .collect();
    let total_neighbors: HashSet<_> = out_neighbors.union(&in_neighbors).collect();

    let out_degree = out_neighbors.len();
    let in_degree = in_neighbors.len();
    let total_degree = total_neighbors.len();

    // Check if self-loop exists
    if out_neighbors.contains(&node_index) || in_neighbors.contains(&node_index) {
        return true;
    }

    // Check if no incoming or outgoing edges
    if out_degree == 0 || in_degree == 0 {
        return true;
    }

    // Check the degree condition
    if total_degree != 2 && total_degree != 4 {
        return true;
    }

    // Rule 4: Differing edge attribute values
    for attr in endpoint_attrs {
        let mut in_values = HashSet::new();
        let mut out_values = HashSet::new();

        for edge in graph.edges_directed(node_index, petgraph::Incoming) {
            if let Some(value) = edge.weight().tags.iter().find(|tag| tag.key == *attr) {
                in_values.insert(&value.value);
            }
        }

        for edge in graph.edges_directed(node_index, petgraph::Outgoing) {
            if let Some(value) = edge.weight().tags.iter().find(|tag| tag.key == *attr) {
                out_values.insert(&value.value);
            }
        }

        // Check if there's more than one unique value across in and out edges
        if in_values.union(&out_values).count() > 1 {
            return true;
        }
    }

    false
}

fn build_path(
    graph: &DiGraph<XmlNode, XmlWay>,
    start: NodeIndex,
    endpoints: &HashSet<NodeIndex>,
) -> Vec<NodeIndex> {
    let mut path = vec![start];
    let mut current = start;

    // Continue until an endpoint is reached
    while !endpoints.contains(&current) {
        // Find a successor of 'current' that is not already in 'path'
        if let Some(successor) = graph
            .neighbors_directed(current, petgraph::Outgoing)
            .find(|&n| !path.contains(&n))
        {
            path.push(successor);
            current = successor;
        } else {
            // If no successor is found, or all successors are already in 'path', break
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

// Function to calculate travel times
fn calculate_travel_time(length: f64, speed_kph: f64) -> f64 {
    let speed_m_per_s = speed_kph / 3.6;
    length / speed_m_per_s // Returns time in seconds
}

// Function to calculate distance
fn calculate_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let radius_earth = 6371000.0; // Radius of the Earth in meters

    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();

    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();

    let a = (dlat / 2.0).sin().powi(2) + (dlon / 2.0).sin().powi(2) * lat1.cos() * lat2.cos();
    let c = 2.0 * a.sqrt().asin();

    radius_earth * c // Distance in meters
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
