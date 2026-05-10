use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::error::OsmGraphError;
use crate::graph::{SnapResult, SpatialGraph};
use crate::overpass::NetworkType;
use crate::utils::calculate_distance;

#[derive(Debug, Clone)]
pub struct Route {
    /// Ordered list of (lat, lon) coordinates along the route
    pub coordinates: Vec<(f64, f64)>,
    /// Cumulative travel time in seconds at each coordinate (parallel to `coordinates`)
    pub cumulative_times_s: Vec<f64>,
    /// Total route distance in meters
    pub distance_m: f64,
    /// Total travel time in seconds for the given network type
    pub duration_s: f64,
    /// Snap diagnostics for the requested origin coordinate.
    pub origin_snap: SnapResult,
    /// Snap diagnostics for the requested destination coordinate.
    pub destination_snap: SnapResult,
}

#[derive(Clone, Copy, Debug)]
struct SearchState {
    estimated_total: f64,
    cost: f64,
    node: NodeIndex,
}

impl PartialEq for SearchState {
    fn eq(&self, other: &Self) -> bool {
        self.estimated_total == other.estimated_total && self.node == other.node
    }
}

impl Eq for SearchState {}

impl PartialOrd for SearchState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SearchState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimated_total
            .partial_cmp(&self.estimated_total)
            .unwrap_or(Ordering::Equal)
    }
}

fn shortest_path_edges(
    sg: &SpatialGraph,
    origin: NodeIndex,
    dest: NodeIndex,
    network_type: NetworkType,
) -> Option<(f64, Vec<NodeIndex>, Vec<EdgeIndex>)> {
    let heuristic = |node: NodeIndex| -> f64 {
        let n = &sg.graph[node];
        let d = &sg.graph[dest];
        let dist = calculate_distance(n.lat, n.lon, d.lat, d.lon);
        let max_speed_m_per_s = 200.0 / 3.6;
        dist / max_speed_m_per_s
    };

    let mut heap = BinaryHeap::new();
    let mut best: HashMap<NodeIndex, f64> = HashMap::new();
    let mut predecessor: HashMap<NodeIndex, (NodeIndex, EdgeIndex)> = HashMap::new();

    best.insert(origin, 0.0);
    heap.push(SearchState {
        estimated_total: heuristic(origin),
        cost: 0.0,
        node: origin,
    });

    while let Some(SearchState { cost, node, .. }) = heap.pop() {
        if cost > *best.get(&node).unwrap_or(&f64::INFINITY) {
            continue;
        }
        if node == dest {
            let mut nodes = vec![dest];
            let mut edges = Vec::new();
            let mut current = dest;
            while current != origin {
                let (prev, edge) = predecessor[&current];
                edges.push(edge);
                nodes.push(prev);
                current = prev;
            }
            nodes.reverse();
            edges.reverse();
            return Some((cost, nodes, edges));
        }

        for edge in sg.graph.edges(node) {
            let next = edge.target();
            let edge_cost = edge.weight().travel_time(network_type);
            if !edge_cost.is_finite() || edge_cost < 0.0 {
                continue;
            }
            let next_cost = cost + edge_cost;
            if next_cost < *best.get(&next).unwrap_or(&f64::INFINITY) {
                best.insert(next, next_cost);
                predecessor.insert(next, (node, edge.id()));
                heap.push(SearchState {
                    estimated_total: next_cost + heuristic(next),
                    cost: next_cost,
                    node: next,
                });
            }
        }
    }

    None
}

fn directed_edge_geometry(sg: &SpatialGraph, edge: EdgeIndex) -> Vec<(f64, f64)> {
    let (source, target) = sg.graph.edge_endpoints(edge).unwrap();
    let way = sg.graph.edge_weight(edge).unwrap();
    let mut points = if way.geometry.len() >= 2 {
        way.geometry.clone()
    } else {
        vec![
            (sg.graph[source].lat, sg.graph[source].lon),
            (sg.graph[target].lat, sg.graph[target].lon),
        ]
    };

    let source_point = (sg.graph[source].lat, sg.graph[source].lon);
    let target_point = (sg.graph[target].lat, sg.graph[target].lon);
    let first = *points.first().unwrap();
    let last = *points.last().unwrap();
    let matches_forward = calculate_distance(first.0, first.1, source_point.0, source_point.1)
        + calculate_distance(last.0, last.1, target_point.0, target_point.1)
        <= calculate_distance(first.0, first.1, target_point.0, target_point.1)
            + calculate_distance(last.0, last.1, source_point.0, source_point.1);
    if !matches_forward {
        points.reverse();
    }
    points
}

fn route_geometry_and_times(
    sg: &SpatialGraph,
    nodes: &[NodeIndex],
    edges: &[EdgeIndex],
    network_type: NetworkType,
) -> (Vec<(f64, f64)>, Vec<f64>, f64, f64) {
    if edges.is_empty() {
        let node = &sg.graph[nodes[0]];
        return (vec![(node.lat, node.lon)], vec![0.0], 0.0, 0.0);
    }

    let mut coordinates = Vec::new();
    let mut cumulative_times_s = Vec::new();
    let mut distance_m = 0.0;
    let mut duration_s = 0.0;

    for &edge in edges {
        let way = sg.graph.edge_weight(edge).unwrap();
        let points = directed_edge_geometry(sg, edge);
        let edge_time = way.travel_time(network_type);
        let segment_lengths: Vec<f64> = points
            .windows(2)
            .map(|pair| calculate_distance(pair[0].0, pair[0].1, pair[1].0, pair[1].1))
            .collect();
        let geometry_length: f64 = segment_lengths.iter().sum();
        let edge_start_time = duration_s;

        if coordinates.is_empty() {
            coordinates.push(points[0]);
            cumulative_times_s.push(duration_s);
        }

        let mut elapsed_on_edge = 0.0;
        for (i, point) in points.iter().enumerate().skip(1) {
            let segment_len = segment_lengths.get(i - 1).copied().unwrap_or(0.0);
            let segment_time = if geometry_length > 0.0 {
                edge_time * (segment_len / geometry_length)
            } else {
                edge_time / (points.len().saturating_sub(1).max(1) as f64)
            };
            elapsed_on_edge += segment_time;
            coordinates.push(*point);
            cumulative_times_s.push(edge_start_time + elapsed_on_edge);
        }

        distance_m += way.length;
        duration_s += edge_time;
        if let Some(last) = cumulative_times_s.last_mut() {
            *last = duration_s;
        }
    }

    (coordinates, cumulative_times_s, distance_m, duration_s)
}

pub fn route(
    sg: &SpatialGraph,
    origin_lat: f64,
    origin_lon: f64,
    dest_lat: f64,
    dest_lon: f64,
    network_type: NetworkType,
    max_snap_m: Option<f64>,
) -> Result<Route, OsmGraphError> {
    let origin_snap = sg
        .snap_point(origin_lat, origin_lon)
        .ok_or(OsmGraphError::OriginNodeNotFound)?;
    let destination_snap = sg
        .snap_point(dest_lat, dest_lon)
        .ok_or(OsmGraphError::DestinationNodeNotFound)?;
    if let Some(max_distance_m) = max_snap_m {
        if origin_snap.distance_m > max_distance_m {
            return Err(OsmGraphError::SnapDistanceExceeded {
                role: "origin",
                distance_m: origin_snap.distance_m,
                max_distance_m,
            });
        }
        if destination_snap.distance_m > max_distance_m {
            return Err(OsmGraphError::SnapDistanceExceeded {
                role: "destination",
                distance_m: destination_snap.distance_m,
                max_distance_m,
            });
        }
    }

    let result = shortest_path_edges(
        sg,
        origin_snap.node_index,
        destination_snap.node_index,
        network_type,
    )
    .ok_or(OsmGraphError::PathNotFound)?;

    let (_, path, edge_path) = result;
    let (coordinates, cumulative_times_s, distance_m, duration_s) =
        route_geometry_and_times(sg, &path, &edge_path, network_type);

    Ok(Route {
        coordinates,
        cumulative_times_s,
        distance_m,
        duration_s,
        origin_snap,
        destination_snap,
    })
}

impl SpatialGraph {
    /// Find the shortest route between two lat/lon points.
    ///
    /// Snaps both points to the nearest graph nodes, then runs A* to find the
    /// optimal path. Returns [`OsmGraphError::OriginNodeNotFound`] or
    /// [`OsmGraphError::DestinationNodeNotFound`] if snapping fails, and
    /// [`OsmGraphError::PathNotFound`] if the snapped nodes are disconnected.
    pub fn route(
        &self,
        origin_lat: f64,
        origin_lon: f64,
        dest_lat: f64,
        dest_lon: f64,
        network_type: NetworkType,
        max_snap_m: Option<f64>,
    ) -> Result<Route, OsmGraphError> {
        route(
            self,
            origin_lat,
            origin_lon,
            dest_lat,
            dest_lon,
            network_type,
            max_snap_m,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{SpatialGraph, XmlNode, XmlTag, XmlWay};
    use crate::overpass::NetworkType;
    use petgraph::graph::DiGraph;

    fn make_node(id: i64, lat: f64, lon: f64) -> XmlNode {
        XmlNode {
            id,
            lat,
            lon,
            tags: vec![],
        }
    }

    fn make_way(drive_travel_time: f64, length: f64) -> XmlWay {
        XmlWay {
            id: 1,
            nodes: vec![],
            tags: vec![XmlTag {
                key: "highway".into(),
                value: "residential".into(),
            }],
            length,
            speed_kph: 50.0,
            walk_travel_time: length / (5.0 / 3.6),
            bike_travel_time: length / (15.0 / 3.6),
            drive_travel_time,
            geometry: Vec::new(),
        }
    }

    fn make_profile_way(drive_travel_time: f64, walk_travel_time: f64, length: f64) -> XmlWay {
        XmlWay {
            id: 1,
            nodes: vec![],
            tags: vec![XmlTag {
                key: "highway".into(),
                value: "residential".into(),
            }],
            length,
            speed_kph: 50.0,
            walk_travel_time,
            bike_travel_time: walk_travel_time,
            drive_travel_time,
            geometry: Vec::new(),
        }
    }

    fn make_way_with_geometry(
        drive_travel_time: f64,
        length: f64,
        geometry: Vec<(f64, f64)>,
    ) -> XmlWay {
        XmlWay {
            geometry,
            ..make_way(drive_travel_time, length)
        }
    }

    fn linear_graph() -> SpatialGraph {
        // A → B → C along a straight line
        let mut g = DiGraph::new();
        let a = g.add_node(make_node(1, 0.0, 0.0));
        let b = g.add_node(make_node(2, 0.001, 0.0));
        let c = g.add_node(make_node(3, 0.002, 0.0));
        g.add_edge(a, b, make_way(10.0, 111.0));
        g.add_edge(b, c, make_way(10.0, 111.0));
        SpatialGraph::new(g)
    }

    #[test]
    fn test_cumulative_times_starts_at_zero() {
        let sg = linear_graph();
        let r = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, None).unwrap();
        assert_eq!(r.cumulative_times_s[0], 0.0);
    }

    #[test]
    fn test_cumulative_times_parallel_to_coordinates() {
        let sg = linear_graph();
        let r = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, None).unwrap();
        assert_eq!(r.cumulative_times_s.len(), r.coordinates.len());
    }

    #[test]
    fn test_cumulative_times_monotonic() {
        let sg = linear_graph();
        let r = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, None).unwrap();
        for w in r.cumulative_times_s.windows(2) {
            assert!(w[1] >= w[0], "times decreased: {:?}", r.cumulative_times_s);
        }
    }

    #[test]
    fn test_cumulative_times_last_equals_duration() {
        let sg = linear_graph();
        let r = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, None).unwrap();
        let last = *r.cumulative_times_s.last().unwrap();
        assert!(
            (last - r.duration_s).abs() < 1e-6,
            "last cumulative time {last:.6} != duration {:.6}",
            r.duration_s
        );
    }

    #[test]
    fn test_route_chooses_faster_path_not_fewer_edges() {
        let mut g = DiGraph::new();
        let a = g.add_node(make_node(1, 0.0, 0.0));
        let b = g.add_node(make_node(2, 0.001, 0.0));
        let c = g.add_node(make_node(3, 0.002, 0.0));
        g.add_edge(a, c, make_way(100.0, 100.0));
        g.add_edge(a, b, make_way(10.0, 50.0));
        g.add_edge(b, c, make_way(10.0, 50.0));
        let sg = SpatialGraph::new(g);

        let route = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, None).unwrap();

        assert_eq!(route.coordinates.len(), 3);
        assert_eq!(route.duration_s, 20.0);
        assert_eq!(route.distance_m, 100.0);
    }

    #[test]
    fn test_route_totals_use_selected_parallel_edge() {
        let mut g = DiGraph::new();
        let a = g.add_node(make_node(1, 0.0, 0.0));
        let b = g.add_node(make_node(2, 0.001, 0.0));
        let c = g.add_node(make_node(3, 0.002, 0.0));
        g.add_edge(a, b, make_way(100.0, 1_000.0));
        g.add_edge(a, b, make_way(10.0, 50.0));
        g.add_edge(b, c, make_way(10.0, 50.0));
        let sg = SpatialGraph::new(g);

        let route = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, None).unwrap();

        assert_eq!(route.duration_s, 20.0);
        assert_eq!(route.distance_m, 100.0);
    }

    #[test]
    fn test_route_uses_edge_geometry_between_nodes() {
        let mut g = DiGraph::new();
        let a = g.add_node(make_node(1, 0.0, 0.0));
        let b = g.add_node(make_node(2, 0.001, 0.0));
        g.add_edge(
            a,
            b,
            make_way_with_geometry(
                10.0,
                100.0,
                vec![(0.0, 0.0), (0.0005, 0.0002), (0.001, 0.0)],
            ),
        );
        let sg = SpatialGraph::new(g);

        let route = route(&sg, 0.0, 0.0, 0.001, 0.0, NetworkType::Drive, None).unwrap();

        assert_eq!(
            route.coordinates,
            vec![(0.0, 0.0), (0.0005, 0.0002), (0.001, 0.0)]
        );
        assert_eq!(route.cumulative_times_s.len(), route.coordinates.len());
        assert_eq!(*route.cumulative_times_s.last().unwrap(), route.duration_s);
    }

    #[test]
    fn test_route_oneway_succeeds_forward_and_fails_reverse() {
        let sg = linear_graph();

        let forward = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, None);
        let reverse = route(&sg, 0.002, 0.0, 0.0, 0.0, NetworkType::Drive, None);

        assert!(forward.is_ok());
        assert!(matches!(reverse, Err(OsmGraphError::PathNotFound)));
    }

    #[test]
    fn test_route_origin_equals_destination_is_zero_cost() {
        let sg = linear_graph();

        let route = route(&sg, 0.0, 0.0, 0.0, 0.0, NetworkType::Drive, None).unwrap();

        assert_eq!(route.coordinates.len(), 1);
        assert_eq!(route.duration_s, 0.0);
        assert_eq!(route.distance_m, 0.0);
        assert_eq!(route.cumulative_times_s, vec![0.0]);
    }

    #[test]
    fn test_route_uses_network_specific_costs() {
        let mut g = DiGraph::new();
        let a = g.add_node(make_node(1, 0.0, 0.0));
        let b = g.add_node(make_node(2, 0.001, 0.0));
        let c = g.add_node(make_node(3, 0.002, 0.0));
        g.add_edge(a, c, make_profile_way(10.0, 100.0, 100.0));
        g.add_edge(a, b, make_profile_way(30.0, 5.0, 50.0));
        g.add_edge(b, c, make_profile_way(30.0, 5.0, 50.0));
        let sg = SpatialGraph::new(g);

        let drive = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, None).unwrap();
        let walk = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Walk, None).unwrap();

        assert_eq!(drive.coordinates.len(), 2);
        assert_eq!(drive.duration_s, 10.0);
        assert_eq!(walk.coordinates.len(), 3);
        assert_eq!(walk.duration_s, 10.0);
    }

    #[test]
    fn test_route_disconnected_components_return_path_not_found() {
        let mut g = DiGraph::new();
        let a = g.add_node(make_node(1, 0.0, 0.0));
        let b = g.add_node(make_node(2, 0.001, 0.0));
        let c = g.add_node(make_node(3, 1.0, 1.0));
        let d = g.add_node(make_node(4, 1.001, 1.0));
        g.add_edge(a, b, make_way(10.0, 100.0));
        g.add_edge(c, d, make_way(10.0, 100.0));
        let sg = SpatialGraph::new(g);

        let result = route(&sg, 0.0, 0.0, 1.001, 1.0, NetworkType::Drive, None);

        assert!(matches!(result, Err(OsmGraphError::PathNotFound)));
    }

    #[test]
    fn test_route_respects_max_snap_distance() {
        let sg = linear_graph();

        let close = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive, Some(1.0));
        assert!(close.is_ok());

        let far = route(&sg, 0.0, 0.0005, 0.002, 0.0, NetworkType::Drive, Some(1.0));
        assert!(matches!(
            far,
            Err(OsmGraphError::SnapDistanceExceeded { role: "origin", .. })
        ));
    }
}
