use geohash::encode;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::{HashMap, HashSet};

use crate::graph::{XmlNode, XmlTag, XmlWay};

static ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

pub fn simplify_graph(graph: &DiGraph<XmlNode, XmlWay>) -> DiGraph<XmlNode, XmlWay> {
    // Step 1: Consolidate nearby intersections into single nodes
    let (consolidated_graph, _) = consolidate_intersections(graph, 9);

    // Step 2: Identify endpoints (intersections, dead-ends) in the consolidated graph
    let mut simplified_graph = DiGraph::new();
    let mut endpoints: HashSet<NodeIndex> = HashSet::new();
    let mut index_map: HashMap<NodeIndex, NodeIndex> = HashMap::new();

    for node in consolidated_graph.node_indices() {
        if is_endpoint(&consolidated_graph, node) {
            endpoints.insert(node);
            let new_index = simplified_graph.add_node(consolidated_graph[node].clone());
            index_map.insert(node, new_index);
        }
    }

    // Step 3: For each endpoint, walk outward along linear chains and collapse them
    // Track which (old_src, old_dst) pairs we've already added to avoid duplicates
    let mut added_edges: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();

    for &endpoint in &endpoints {
        for neighbor in consolidated_graph.neighbors_directed(endpoint, petgraph::Outgoing) {
            if added_edges.contains(&(endpoint, neighbor)) {
                continue;
            }

            let path = build_path(&consolidated_graph, endpoint, neighbor, &endpoints);
            if let Some(&last) = path.last() {
                if !endpoints.contains(&last) {
                    continue; // path didn't reach another endpoint
                }

                // Aggregate edge attributes along the path
                let mut total_length = 0.0;
                let mut total_walk = 0.0;
                let mut total_bike = 0.0;
                let mut total_drive = 0.0;
                let mut speeds = Vec::new();
                let mut all_tags: Vec<XmlTag> = Vec::new();

                for window in path.windows(2) {
                    if let [u, v] = window {
                        if let Some(edge) = consolidated_graph.find_edge(*u, *v) {
                            let way = consolidated_graph.edge_weight(edge).unwrap();
                            total_length += way.length;
                            total_walk += way.walk_travel_time;
                            total_bike += way.bike_travel_time;
                            total_drive += way.drive_travel_time;
                            speeds.push(way.speed_kph);
                            all_tags.extend(way.tags.clone());
                        }
                    }
                }

                let avg_speed = if !speeds.is_empty() {
                    speeds.iter().sum::<f64>() / speeds.len() as f64
                } else {
                    0.0
                };

                let collapsed_way = XmlWay {
                    id: get_unique_id(),
                    nodes: vec![],
                    tags: all_tags,
                    length: total_length,
                    speed_kph: avg_speed,
                    walk_travel_time: total_walk,
                    bike_travel_time: total_bike,
                    drive_travel_time: total_drive,
                };

                // Both endpoints must be in the simplified graph
                if let (Some(&new_src), Some(&new_dst)) =
                    (index_map.get(&endpoint), index_map.get(&last))
                {
                    simplified_graph.add_edge(new_src, new_dst, collapsed_way);
                    added_edges.insert((endpoint, last));
                }
            }
        }
    }

    // Step 4: Remove parallel edges, keeping only the lowest drive_travel_time per (src, dst)
    deduplicate_edges(simplified_graph)
}

/// Remove parallel edges between the same (src, dst) pair, keeping the one with
/// the lowest drive_travel_time. Returns a new graph with at most one edge per direction.
fn deduplicate_edges(graph: DiGraph<XmlNode, XmlWay>) -> DiGraph<XmlNode, XmlWay> {
    // Borrow edge weights directly from the input graph — no clones until we know the winner.
    let mut best: HashMap<(NodeIndex, NodeIndex), &XmlWay> = HashMap::new();
    for edge in graph.edge_references() {
        let key = (edge.source(), edge.target());
        let way = edge.weight();
        best.entry(key)
            .and_modify(|existing| {
                if way.drive_travel_time < existing.drive_travel_time {
                    *existing = way;
                }
            })
            .or_insert(way);
    }

    // Rebuild a clean graph, cloning only the winning edge per (src, dst) pair.
    let mut deduped = DiGraph::new();
    let mut node_map: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    for old_idx in graph.node_indices() {
        let new_idx = deduped.add_node(graph[old_idx].clone());
        node_map.insert(old_idx, new_idx);
    }
    for ((src, dst), way) in &best {
        deduped.add_edge(node_map[src], node_map[dst], (*way).clone());
    }

    deduped
}

/// Walk a linear chain starting from `start` via `first_step` until we hit an endpoint.
/// Returns the full path of NodeIndices including `start` and the terminal endpoint.
fn build_path(
    graph: &DiGraph<XmlNode, XmlWay>,
    start: NodeIndex,
    first_step: NodeIndex,
    endpoints: &HashSet<NodeIndex>,
) -> Vec<NodeIndex> {
    let mut path = vec![start, first_step];
    let mut prev = start;
    let mut current = first_step;

    while !endpoints.contains(&current) {
        let next = graph
            .neighbors_directed(current, petgraph::Outgoing)
            .find(|&n| n != prev); // avoid going back — O(1) instead of O(path len)

        match next {
            Some(n) => {
                prev = current;
                current = n;
                path.push(n);
            }
            None => break,
        }
    }

    path
}

fn is_endpoint(graph: &DiGraph<XmlNode, XmlWay>, node_index: NodeIndex) -> bool {
    // Collect all neighbors into a single Vec — road nodes have degree 2–6 so
    // sort+dedup is cheaper than HashSet allocation for this cardinality.
    let mut neighbors: Vec<NodeIndex> = graph
        .neighbors_directed(node_index, petgraph::Outgoing)
        .collect();
    let out_count = neighbors.len();
    neighbors.extend(graph.neighbors_directed(node_index, petgraph::Incoming));

    // Self-loop
    if neighbors.iter().any(|&n| n == node_index) {
        return true;
    }

    let in_count = neighbors.len() - out_count;

    // Dead-end
    if out_count == 0 || in_count == 0 {
        return true;
    }

    // A node with exactly 2 unique neighbours is a linear pass-through — not an endpoint.
    // Anything else (intersection, fork, merge) is an endpoint.
    neighbors.sort_unstable();
    neighbors.dedup();
    neighbors.len() != 2
}

/// Consolidate nearby nodes that share the same geohash cell into a single averaged node.
/// Returns the new graph and a map from old NodeIndex -> new NodeIndex.
fn consolidate_intersections(
    graph: &DiGraph<XmlNode, XmlWay>,
    precision: usize,
) -> (DiGraph<XmlNode, XmlWay>, HashMap<NodeIndex, NodeIndex>) {
    // Group old node indices by geohash
    let mut hash_to_old_indices: HashMap<String, Vec<NodeIndex>> = HashMap::new();
    for node_idx in graph.node_indices() {
        let node = &graph[node_idx];
        let hash = encode((node.lat, node.lon).into(), precision).unwrap_or_default();
        hash_to_old_indices.entry(hash).or_default().push(node_idx);
    }

    let mut new_graph = DiGraph::new();
    let mut old_to_new: HashMap<NodeIndex, NodeIndex> = HashMap::new();

    // Merge each group into one node
    for (_hash, old_indices) in &hash_to_old_indices {
        let merged = merge_nodes(graph, old_indices);
        let new_idx = new_graph.add_node(merged);
        for &old_idx in old_indices {
            old_to_new.insert(old_idx, new_idx);
        }
    }

    // Reconnect edges, skipping self-loops created by consolidation
    let mut seen_edges: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();
    for edge in graph.edge_references() {
        let new_src = old_to_new[&edge.source()];
        let new_dst = old_to_new[&edge.target()];
        if new_src == new_dst {
            continue; // consolidated into same node
        }
        if seen_edges.insert((new_src, new_dst)) {
            new_graph.add_edge(new_src, new_dst, edge.weight().clone());
        }
    }

    (new_graph, old_to_new)
}

/// Average a group of old nodes into a single new XmlNode.
fn merge_nodes(graph: &DiGraph<XmlNode, XmlWay>, indices: &[NodeIndex]) -> XmlNode {
    let nodes: Vec<&XmlNode> = indices.iter().map(|&i| &graph[i]).collect();
    let count = nodes.len() as f64;
    let avg_lat = nodes.iter().map(|n| n.lat).sum::<f64>() / count;
    let avg_lon = nodes.iter().map(|n| n.lon).sum::<f64>() / count;
    let merged_tags = nodes.iter().flat_map(|n| n.tags.clone()).collect();
    let geohash = nodes[0].geohash.clone();

    XmlNode {
        id: get_unique_id(),
        lat: avg_lat,
        lon: avg_lon,
        tags: merged_tags,
        geohash,
    }
}

fn get_unique_id() -> i64 {
    ID_COUNTER.fetch_add(1, Ordering::Relaxed) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{XmlNode, XmlWay};

    fn make_node(id: i64, lat: f64, lon: f64) -> XmlNode {
        XmlNode { id, lat, lon, tags: vec![], geohash: None }
    }

    fn make_way(id: i64, drive_travel_time: f64) -> XmlWay {
        XmlWay {
            id,
            nodes: vec![],
            tags: vec![],
            length: 100.0,
            speed_kph: 50.0,
            walk_travel_time: 72.0,
            bike_travel_time: 24.0,
            drive_travel_time,
        }
    }

    #[test]
    fn test_deduplicate_keeps_fastest_edge() {
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node(1, 0.0, 0.0));
        let b = graph.add_node(make_node(2, 0.001, 0.0));
        graph.add_edge(a, b, make_way(1, 100.0));
        graph.add_edge(a, b, make_way(2, 50.0));

        assert_eq!(graph.edge_count(), 2);
        let deduped = simplify_graph(&graph);
        assert!(deduped.edge_count() <= 1, "Expected at most 1 edge, got {}", deduped.edge_count());
    }
}
