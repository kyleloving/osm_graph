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

    let result = astar(&sg.graph, origin, |n| n == dest, edge_cost, heuristic)
        .ok_or(OsmGraphError::NodeNotFound)?; // no path found

    let (_, path) = result;

    let coordinates: Vec<(f64, f64)> = path
        .iter()
        .map(|&idx| {
            let n = &sg.graph[idx];
            (n.lat, n.lon)
        })
        .collect();

    // Aggregate distance and duration along the path
    let mut distance_m = 0.0;
    let mut duration_s = 0.0;
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
            }
        }
    }

    Ok(Route { coordinates, distance_m, duration_s })
}
