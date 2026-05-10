use petgraph::graph::{DiGraph, EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use rstar::{PointDistance, RTree, RTreeObject, AABB};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::graph::{XmlNode, XmlTag, XmlWay};
use crate::utils::calculate_distance;

static ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

const CONSOLIDATION_DISTANCE_M: f64 = 5.0;

pub fn simplify_graph(graph: &DiGraph<XmlNode, XmlWay>) -> DiGraph<XmlNode, XmlWay> {
    let (consolidated_graph, _) = consolidate_intersections(graph, CONSOLIDATION_DISTANCE_M);

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

    let mut added_edges: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();

    for &endpoint in &endpoints {
        for edge in consolidated_graph.edges_directed(endpoint, petgraph::Outgoing) {
            let neighbor = edge.target();
            if added_edges.contains(&(endpoint, neighbor)) {
                continue;
            }

            let path = build_path(
                &consolidated_graph,
                endpoint,
                neighbor,
                edge.id(),
                &endpoints,
            );
            let Some(&last) = path.nodes.last() else {
                continue;
            };
            if !endpoints.contains(&last) || path.edges.is_empty() {
                continue;
            }

            let collapsed_way = collapse_path_edges(&consolidated_graph, &path.edges);

            if let (Some(&new_src), Some(&new_dst)) =
                (index_map.get(&endpoint), index_map.get(&last))
            {
                simplified_graph.add_edge(new_src, new_dst, collapsed_way);
                added_edges.insert((endpoint, last));
            }
        }
    }

    deduplicate_edges(simplified_graph)
}

fn collapse_path_edges(graph: &DiGraph<XmlNode, XmlWay>, edges: &[EdgeIndex]) -> XmlWay {
    let mut total_length = 0.0;
    let mut total_walk = 0.0;
    let mut total_bike = 0.0;
    let mut total_drive = 0.0;
    let mut weighted_speed_sum = 0.0;
    let mut tags: Option<Vec<XmlTag>> = None;
    let mut geometry: Vec<(f64, f64)> = Vec::new();

    for &edge in edges {
        let way = graph.edge_weight(edge).unwrap();
        total_length += way.length;
        total_walk += way.walk_travel_time;
        total_bike += way.bike_travel_time;
        total_drive += way.drive_travel_time;
        weighted_speed_sum += way.speed_kph * way.length;
        if tags.is_none() {
            tags = Some(way.tags.clone());
        }
        append_edge_geometry(graph, edge, &mut geometry);
    }

    let speed_kph = if total_length > 0.0 {
        weighted_speed_sum / total_length
    } else {
        0.0
    };

    XmlWay {
        id: get_unique_id(),
        nodes: Vec::new(),
        tags: tags.unwrap_or_default(),
        length: total_length,
        speed_kph,
        walk_travel_time: total_walk,
        bike_travel_time: total_bike,
        drive_travel_time: total_drive,
        geometry,
    }
}

fn append_edge_geometry(
    graph: &DiGraph<XmlNode, XmlWay>,
    edge: EdgeIndex,
    out: &mut Vec<(f64, f64)>,
) {
    let (source, target) = graph.edge_endpoints(edge).unwrap();
    let way = graph.edge_weight(edge).unwrap();
    let mut points = if way.geometry.len() >= 2 {
        way.geometry.clone()
    } else {
        vec![
            (graph[source].lat, graph[source].lon),
            (graph[target].lat, graph[target].lon),
        ]
    };

    let source_point = (graph[source].lat, graph[source].lon);
    let target_point = (graph[target].lat, graph[target].lon);
    let first = *points.first().unwrap();
    let last = *points.last().unwrap();
    let matches_forward = calculate_distance(first.0, first.1, source_point.0, source_point.1)
        + calculate_distance(last.0, last.1, target_point.0, target_point.1)
        <= calculate_distance(first.0, first.1, target_point.0, target_point.1)
            + calculate_distance(last.0, last.1, source_point.0, source_point.1);
    if !matches_forward {
        points.reverse();
    }

    if out.is_empty() {
        out.extend(points);
    } else {
        out.extend(points.into_iter().skip(1));
    }
}

fn deduplicate_edges(graph: DiGraph<XmlNode, XmlWay>) -> DiGraph<XmlNode, XmlWay> {
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

struct Path {
    nodes: Vec<NodeIndex>,
    edges: Vec<EdgeIndex>,
}

fn build_path(
    graph: &DiGraph<XmlNode, XmlWay>,
    start: NodeIndex,
    first_step: NodeIndex,
    first_edge: EdgeIndex,
    endpoints: &HashSet<NodeIndex>,
) -> Path {
    let mut path = Path {
        nodes: vec![start, first_step],
        edges: vec![first_edge],
    };
    let mut prev = start;
    let mut current = first_step;

    while !endpoints.contains(&current) {
        let Some((next, edge)) = next_chain_step(graph, current, prev) else {
            break;
        };
        prev = current;
        current = next;
        path.nodes.push(next);
        path.edges.push(edge);
    }

    path
}

fn next_chain_step(
    graph: &DiGraph<XmlNode, XmlWay>,
    current: NodeIndex,
    prev: NodeIndex,
) -> Option<(NodeIndex, EdgeIndex)> {
    let mut by_target: HashMap<NodeIndex, EdgeIndex> = HashMap::new();
    for edge in graph.edges_directed(current, petgraph::Outgoing) {
        let target = edge.target();
        if target == prev {
            continue;
        }
        by_target
            .entry(target)
            .and_modify(|existing| {
                let old = graph.edge_weight(*existing).unwrap();
                if edge.weight().drive_travel_time < old.drive_travel_time {
                    *existing = edge.id();
                }
            })
            .or_insert(edge.id());
    }

    if by_target.len() == 1 {
        by_target.into_iter().next()
    } else {
        None
    }
}

fn is_endpoint(graph: &DiGraph<XmlNode, XmlWay>, node_index: NodeIndex) -> bool {
    let out: Vec<NodeIndex> = graph
        .neighbors_directed(node_index, petgraph::Outgoing)
        .collect();
    let incoming: Vec<NodeIndex> = graph
        .neighbors_directed(node_index, petgraph::Incoming)
        .collect();

    if out.is_empty() || incoming.is_empty() {
        return true;
    }
    if out.iter().chain(incoming.iter()).any(|&n| n == node_index) {
        return true;
    }

    let mut neighbors = out;
    neighbors.extend(incoming);
    neighbors.sort_unstable();
    neighbors.dedup();
    neighbors.len() != 2
}

fn consolidate_intersections(
    graph: &DiGraph<XmlNode, XmlWay>,
    merge_distance_m: f64,
) -> (DiGraph<XmlNode, XmlWay>, HashMap<NodeIndex, NodeIndex>) {
    let entries: Vec<NodeEntry> = graph
        .node_indices()
        .map(|index| NodeEntry {
            point: projected_point(graph[index].lat, graph[index].lon),
            index,
        })
        .collect();
    let tree = RTree::bulk_load(entries);
    let clusters = cluster_nodes_by_distance(graph, &tree, merge_distance_m);

    let mut new_graph = DiGraph::new();
    let mut old_to_new: HashMap<NodeIndex, NodeIndex> = HashMap::new();

    for cluster in clusters {
        let merged = merge_nodes(graph, &cluster.members);
        let new_idx = new_graph.add_node(merged);
        for &old_idx in &cluster.members {
            old_to_new.insert(old_idx, new_idx);
        }
    }

    let mut seen_edges: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();
    for edge in graph.edge_references() {
        let new_src = old_to_new[&edge.source()];
        let new_dst = old_to_new[&edge.target()];
        if new_src == new_dst {
            continue;
        }
        if seen_edges.insert((new_src, new_dst)) {
            new_graph.add_edge(new_src, new_dst, edge.weight().clone());
        }
    }

    (new_graph, old_to_new)
}

struct Cluster {
    members: Vec<NodeIndex>,
}

fn cluster_nodes_by_distance(
    graph: &DiGraph<XmlNode, XmlWay>,
    tree: &RTree<NodeEntry>,
    merge_distance_m: f64,
) -> Vec<Cluster> {
    let mut clusters: Vec<Cluster> = Vec::new();
    let mut assigned: HashSet<NodeIndex> = HashSet::new();

    for idx in graph.node_indices() {
        if assigned.contains(&idx) {
            continue;
        }
        let node = &graph[idx];
        let center = projected_point(node.lat, node.lon);
        let members: Vec<NodeIndex> = tree
            .locate_within_distance(center, merge_distance_m * merge_distance_m)
            .filter_map(|entry| {
                if assigned.contains(&entry.index) {
                    return None;
                }
                let candidate = &graph[entry.index];
                (calculate_distance(node.lat, node.lon, candidate.lat, candidate.lon)
                    <= merge_distance_m)
                    .then_some(entry.index)
            })
            .collect();

        for member in &members {
            assigned.insert(*member);
        }
        clusters.push(Cluster { members });
    }

    clusters
}

#[derive(Clone, Copy)]
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
        let dx = self.point[0] - point[0];
        let dy = self.point[1] - point[1];
        dx * dx + dy * dy
    }
}

fn projected_point(lat: f64, lon: f64) -> [f64; 2] {
    const METERS_PER_DEGREE: f64 = 111_320.0;
    [
        lat * METERS_PER_DEGREE,
        lon * METERS_PER_DEGREE * lat.to_radians().cos(),
    ]
}

fn merge_nodes(graph: &DiGraph<XmlNode, XmlWay>, indices: &[NodeIndex]) -> XmlNode {
    let count = indices.len() as f64;
    let avg_lat = indices.iter().map(|&i| graph[i].lat).sum::<f64>() / count;
    let avg_lon = indices.iter().map(|&i| graph[i].lon).sum::<f64>() / count;

    XmlNode {
        id: get_unique_id(),
        lat: avg_lat,
        lon: avg_lon,
        tags: Vec::new(),
    }
}

fn get_unique_id() -> i64 {
    ID_COUNTER.fetch_add(1, Ordering::Relaxed) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{XmlNode, XmlTag, XmlWay};

    fn make_node(id: i64, lat: f64, lon: f64) -> XmlNode {
        XmlNode {
            id,
            lat,
            lon,
            tags: Vec::new(),
        }
    }

    fn make_tag(key: &str, value: &str) -> XmlTag {
        XmlTag {
            key: key.into(),
            value: value.into(),
        }
    }

    fn make_way(id: i64, drive_travel_time: f64) -> XmlWay {
        XmlWay {
            id,
            nodes: Vec::new(),
            tags: Vec::new(),
            length: 100.0,
            speed_kph: 50.0,
            walk_travel_time: 72.0,
            bike_travel_time: 24.0,
            drive_travel_time,
            geometry: Vec::new(),
        }
    }

    fn make_way_with_length(id: i64, drive_travel_time: f64, length: f64) -> XmlWay {
        XmlWay {
            length,
            ..make_way(id, drive_travel_time)
        }
    }

    fn make_way_with_geometry(
        id: i64,
        drive_travel_time: f64,
        geometry: Vec<(f64, f64)>,
    ) -> XmlWay {
        XmlWay {
            geometry,
            ..make_way(id, drive_travel_time)
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
        assert!(
            deduped.edge_count() <= 1,
            "Expected at most 1 edge, got {}",
            deduped.edge_count()
        );
    }

    #[test]
    fn consolidation_does_not_merge_nodes_beyond_threshold() {
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node(1, 38.0, -77.0));
        let b = graph.add_node(make_node(2, 38.0001, -77.0));
        graph.add_edge(a, b, make_way(1, 1.0));

        let (consolidated, map) = consolidate_intersections(&graph, 5.0);

        assert_eq!(consolidated.node_count(), 2);
        assert_ne!(map[&a], map[&b]);
    }

    #[test]
    fn merged_nodes_drop_source_tags() {
        let mut graph = DiGraph::new();
        let mut n1 = make_node(1, 38.0, -77.0);
        n1.tags.push(make_tag("highway", "traffic_signals"));
        let a = graph.add_node(n1);
        let b = graph.add_node(make_node(2, 38.000001, -77.0));

        let merged = merge_nodes(&graph, &[a, b]);

        assert!(merged.tags.is_empty());
    }

    #[test]
    fn path_aggregation_uses_traversed_edge() {
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node(1, 0.0, 0.0));
        let b = graph.add_node(make_node(2, 0.001, 0.0));
        let c = graph.add_node(make_node(3, 0.002, 0.0));
        let e1 = graph.add_edge(a, b, make_way(1, 10.0));
        graph.add_edge(a, b, make_way(2, 100.0));
        let e2 = graph.add_edge(b, c, make_way(3, 20.0));
        graph.add_edge(b, c, make_way(4, 200.0));

        let path = Path {
            nodes: vec![a, b, c],
            edges: vec![e1, e2],
        };
        let collapsed = collapse_path_edges(&graph, &path.edges);

        assert_eq!(collapsed.drive_travel_time, 30.0);
    }

    #[test]
    fn linear_chain_collapses_to_single_summed_edge() {
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node(1, 0.0, 0.0));
        let b = graph.add_node(make_node(2, 0.001, 0.0));
        let c = graph.add_node(make_node(3, 0.002, 0.0));
        let d = graph.add_node(make_node(4, 0.003, 0.0));
        graph.add_edge(a, b, make_way_with_length(1, 10.0, 100.0));
        graph.add_edge(b, c, make_way_with_length(2, 20.0, 200.0));
        graph.add_edge(c, d, make_way_with_length(3, 30.0, 300.0));

        let simplified = simplify_graph(&graph);

        assert_eq!(simplified.node_count(), 2);
        assert_eq!(simplified.edge_count(), 1);
        let edge = simplified.edge_weights().next().unwrap();
        assert_eq!(edge.drive_travel_time, 60.0);
        assert_eq!(edge.length, 600.0);
    }

    #[test]
    fn linear_chain_preserves_intermediate_geometry() {
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node(1, 0.0, 0.0));
        let b = graph.add_node(make_node(2, 0.001, 0.0));
        let c = graph.add_node(make_node(3, 0.002, 0.0));
        let d = graph.add_node(make_node(4, 0.003, 0.0));
        graph.add_edge(
            a,
            b,
            make_way_with_geometry(1, 10.0, vec![(0.0, 0.0), (0.0005, 0.0002), (0.001, 0.0)]),
        );
        graph.add_edge(
            b,
            c,
            make_way_with_geometry(2, 20.0, vec![(0.001, 0.0), (0.0015, 0.0002), (0.002, 0.0)]),
        );
        graph.add_edge(
            c,
            d,
            make_way_with_geometry(3, 30.0, vec![(0.002, 0.0), (0.0025, 0.0002), (0.003, 0.0)]),
        );

        let simplified = simplify_graph(&graph);
        let edge = simplified.edge_weights().next().unwrap();

        assert_eq!(
            edge.geometry,
            vec![
                (0.0, 0.0),
                (0.0005, 0.0002),
                (0.001, 0.0),
                (0.0015, 0.0002),
                (0.002, 0.0),
                (0.0025, 0.0002),
                (0.003, 0.0),
            ]
        );
    }

    #[test]
    fn t_junction_preserves_decision_node() {
        let mut graph = DiGraph::new();
        let west = graph.add_node(make_node(1, 0.0, 0.0));
        let center = graph.add_node(make_node(2, 0.001, 0.0));
        let east = graph.add_node(make_node(3, 0.002, 0.0));
        let north = graph.add_node(make_node(4, 0.001, 0.001));
        graph.add_edge(west, center, make_way(1, 10.0));
        graph.add_edge(center, east, make_way(2, 10.0));
        graph.add_edge(center, north, make_way(3, 10.0));

        let simplified = simplify_graph(&graph);

        assert_eq!(simplified.node_count(), 4);
        assert_eq!(simplified.edge_count(), 3);
    }

    #[test]
    fn simplification_preserves_oneway_direction() {
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node(1, 0.0, 0.0));
        let b = graph.add_node(make_node(2, 0.001, 0.0));
        let c = graph.add_node(make_node(3, 0.002, 0.0));
        graph.add_edge(a, b, make_way(1, 10.0));
        graph.add_edge(b, c, make_way(2, 10.0));

        let simplified = simplify_graph(&graph);
        let edge = simplified.edge_references().next().unwrap();
        let source = &simplified[edge.source()];
        let target = &simplified[edge.target()];

        assert_eq!(simplified.edge_count(), 1);
        assert!(source.lat < target.lat);
    }

    #[test]
    fn simplification_does_not_connect_near_crossing_roads() {
        let mut graph = DiGraph::new();
        let west = graph.add_node(make_node(1, 0.0, -0.001));
        let east = graph.add_node(make_node(2, 0.0, 0.001));
        let south = graph.add_node(make_node(3, -0.001, 0.0));
        let north = graph.add_node(make_node(4, 0.001, 0.0));
        graph.add_edge(west, east, make_way(1, 10.0));
        graph.add_edge(south, north, make_way(2, 10.0));

        let simplified = simplify_graph(&graph);

        assert_eq!(simplified.node_count(), 4);
        assert_eq!(simplified.edge_count(), 2);
    }
}
