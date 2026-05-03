use petgraph::algo::astar;
use petgraph::graph::NodeIndex;

use crate::error::OsmGraphError;
use crate::graph::{SpatialGraph, XmlWay};
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
}

pub fn route(
    sg: &SpatialGraph,
    origin_lat: f64,
    origin_lon: f64,
    dest_lat: f64,
    dest_lon: f64,
    network_type: NetworkType,
) -> Result<Route, OsmGraphError> {
    let origin = sg.nearest_node(origin_lat, origin_lon).ok_or(OsmGraphError::NodeNotFound)?;
    let dest = sg.nearest_node(dest_lat, dest_lon).ok_or(OsmGraphError::NodeNotFound)?;

    let edge_cost = |e: petgraph::graph::EdgeReference<XmlWay>| -> f64 {
        let way = e.weight();
        match network_type {
            NetworkType::Walk => way.walk_travel_time,
            NetworkType::Bike => way.bike_travel_time,
            _ => way.drive_travel_time,
        }
    };

    // Heuristic: straight-line travel time from node to destination
    let heuristic = |node: NodeIndex| -> f64 {
        let n = &sg.graph[node];
        let d = &sg.graph[dest];
        let dist = calculate_distance(n.lat, n.lon, d.lat, d.lon);
        // Use a generous speed so the heuristic is admissible (never overestimates)
        let max_speed_m_per_s = 200.0 / 3.6; // 200 kph
        dist / max_speed_m_per_s
    };

    let result = astar(&*sg.graph, origin, |n| n == dest, edge_cost, heuristic)
        .ok_or(OsmGraphError::NodeNotFound)?; // no path found

    let (_, path) = result;

    let coordinates: Vec<(f64, f64)> = path
        .iter()
        .map(|&idx| {
            let n = &sg.graph[idx];
            (n.lat, n.lon)
        })
        .collect();

    // Aggregate distance, duration, and cumulative times along the path
    let mut distance_m = 0.0;
    let mut duration_s = 0.0;
    let mut cumulative_times_s = vec![0.0_f64]; // origin starts at t=0
    for window in path.windows(2) {
        if let [u, v] = window {
            if let Some(edge) = sg.graph.find_edge(*u, *v) {
                let way = sg.graph.edge_weight(edge).unwrap();
                distance_m += way.length;
                duration_s += match network_type {
                    NetworkType::Walk => way.walk_travel_time,
                    NetworkType::Bike => way.bike_travel_time,
                    _ => way.drive_travel_time,
                };
                cumulative_times_s.push(duration_s);
            }
        }
    }

    Ok(Route { coordinates, cumulative_times_s, distance_m, duration_s })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{SpatialGraph, XmlNode, XmlTag, XmlWay};
    use crate::overpass::NetworkType;
    use petgraph::graph::DiGraph;

    fn make_node(id: i64, lat: f64, lon: f64) -> XmlNode {
        XmlNode { id, lat, lon, tags: vec![], geohash: None }
    }

    fn make_way(drive_travel_time: f64, length: f64) -> XmlWay {
        XmlWay {
            id: 1,
            nodes: vec![],
            tags: vec![XmlTag { key: "highway".into(), value: "residential".into() }],
            length,
            speed_kph: 50.0,
            walk_travel_time: length / (5.0 / 3.6),
            bike_travel_time: length / (15.0 / 3.6),
            drive_travel_time,
        }
    }

    fn linear_graph() -> SpatialGraph {
        // A → B → C along a straight line
        let mut g = DiGraph::new();
        let a = g.add_node(make_node(1, 0.0,   0.0));
        let b = g.add_node(make_node(2, 0.001, 0.0));
        let c = g.add_node(make_node(3, 0.002, 0.0));
        g.add_edge(a, b, make_way(10.0, 111.0));
        g.add_edge(b, c, make_way(10.0, 111.0));
        SpatialGraph::new(g)
    }

    #[test]
    fn test_cumulative_times_starts_at_zero() {
        let sg = linear_graph();
        let r = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive).unwrap();
        assert_eq!(r.cumulative_times_s[0], 0.0);
    }

    #[test]
    fn test_cumulative_times_parallel_to_coordinates() {
        let sg = linear_graph();
        let r = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive).unwrap();
        assert_eq!(r.cumulative_times_s.len(), r.coordinates.len());
    }

    #[test]
    fn test_cumulative_times_monotonic() {
        let sg = linear_graph();
        let r = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive).unwrap();
        for w in r.cumulative_times_s.windows(2) {
            assert!(w[1] >= w[0], "times decreased: {:?}", r.cumulative_times_s);
        }
    }

    #[test]
    fn test_cumulative_times_last_equals_duration() {
        let sg = linear_graph();
        let r = route(&sg, 0.0, 0.0, 0.002, 0.0, NetworkType::Drive).unwrap();
        let last = *r.cumulative_times_s.last().unwrap();
        assert!(
            (last - r.duration_s).abs() < 1e-6,
            "last cumulative time {last:.6} != duration {:.6}", r.duration_s
        );
    }
}
