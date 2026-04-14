use crate::graph::{self, SpatialGraph};
use crate::overpass;
use crate::cache;
use crate::error::OsmGraphError;
use crate::overpass::NetworkType;

use geo::{ConcaveHull, ConvexHull, KNearestConcaveHull, MultiPoint, Polygon};
use petgraph::algo::dijkstra;
use petgraph::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Copy, Clone)]
pub enum HullType {
    FastConcave,
    Concave,
    Convex,
}

pub fn calculate_isochrones(
    graph: &DiGraph<graph::XmlNode, graph::XmlWay>,
    start_node: NodeIndex,
    time_limits: Vec<f64>,
    hull_type: HullType,
) -> Vec<Polygon> {
    let mut isochrones = Vec::new();

    // Compute shortest paths from start_node
    let shortest_paths = dijkstra(graph, start_node, None, |e| {
        let edge_weight = graph.edge_weight(e.id()).unwrap();
        edge_weight.drive_travel_time
    });

    // For each time limit, find unique nodes that are within that limit
    for &time_limit in &time_limits {
        let isochrone_nodes = shortest_paths
            .iter()
            .filter_map(|(&node, &time)| if time <= time_limit { Some(node) } else { None })
            .collect::<HashSet<_>>();

        // Convert each node index in the isochrone to latitude/longitude
        let isochrone_lat_lons = isochrone_nodes
            .into_iter()
            .map(|node_index| graph::node_to_latlon(graph, node_index))
            .collect::<Vec<_>>();

        let points: MultiPoint<f64> = isochrone_lat_lons.into();

        let hull = match hull_type {
            HullType::FastConcave => points.concave_hull(2.0),
            HullType::Concave => points.k_nearest_concave_hull(3),
            HullType::Convex => points.convex_hull(),
        };

        isochrones.push(hull);
    }

    isochrones
}

fn calculate_isochrones_concurrently(
    graph: std::sync::Arc<DiGraph<graph::XmlNode, graph::XmlWay>>,
    start_node: NodeIndex,
    time_limits: Vec<f64>,
    network_type: overpass::NetworkType,
    hull_type: HullType,
) -> Vec<Polygon> {
    // Run Dijkstra once — results cover all time limits
    let shortest_paths = dijkstra(&*graph, start_node, None, |e| {
        let way = graph.edge_weight(e.id()).unwrap();
        match network_type {
            NetworkType::Walk => way.walk_travel_time,
            NetworkType::Bike => way.bike_travel_time,
            _ => way.drive_travel_time,
        }
    });

    // Collect all (node, time) pairs once, then filter per time limit in parallel
    let node_times: Vec<(NodeIndex, f64)> = shortest_paths.into_iter().collect();
    let node_times = std::sync::Arc::new(node_times);

    let mut handles = vec![];
    for time_limit in time_limits {
        let graph_clone = std::sync::Arc::clone(&graph);
        let node_times_clone = std::sync::Arc::clone(&node_times);
        let handle = std::thread::spawn(move || {
            let points: MultiPoint<f64> = node_times_clone
                .iter()
                .filter(|(_, t)| *t <= time_limit)
                .map(|(node, _)| graph::node_to_latlon(&*graph_clone, *node))
                .collect::<Vec<_>>()
                .into();

            match hull_type {
                HullType::FastConcave => points.concave_hull(2.0),
                HullType::Concave => points.k_nearest_concave_hull(3),
                HullType::Convex => points.convex_hull(),
            }
        });
        handles.push(handle);
    }

    handles.into_iter().map(|h| h.join().unwrap()).collect()
}


pub async fn calculate_isochrones_from_point(
    lat: f64,
    lon: f64,
    max_dist: Option<f64>,
    time_limits: Vec<f64>,
    network_type: overpass::NetworkType,
    hull_type: HullType,
    retain_all: bool,
) -> Result<(Vec<Polygon>, SpatialGraph), OsmGraphError> {

    // Auto-size bounding box if not provided.
    // Use max time limit * a generous speed + 20% buffer to ensure the
    // isochrone never saturates into a square at the bbox boundary.
    let max_speed_m_per_s = match network_type {
        NetworkType::Walk => 5.0 / 3.6,
        NetworkType::Bike => 25.0 / 3.6,
        _ => 120.0 / 3.6,
    };
    let max_time = time_limits.iter().cloned().fold(0.0_f64, f64::max);
    let computed_dist = max_dist.unwrap_or_else(|| max_time * max_speed_m_per_s * 1.2);

    let polygon_coord_str = overpass::bbox_from_point(lat, lon, computed_dist);
    let query = overpass::create_overpass_query(&polygon_coord_str, network_type);
    let graph_key = format!("{}:{}", query, retain_all);

    let sg = if let Some(cached) = cache::check_cache(&graph_key)? {
        cached
    } else {
        let xml = if let Some(cached_xml) = cache::check_xml_cache(&query)? {
            cached_xml                                      // in-memory hit
        } else if let Some(disk_xml) = cache::check_disk_xml_cache(&query) {
            cache::insert_into_xml_cache(query.clone(), disk_xml.clone())?; // promote to memory
            disk_xml                                        // disk hit
        } else {
            let fetched = overpass::make_request("https://overpass-api.de/api/interpreter", &query).await?;
            cache::write_disk_xml_cache(&query, &fetched); // persist to disk (best-effort)
            cache::insert_into_xml_cache(query.clone(), fetched.clone())?;
            fetched                                         // network fetch
        };

        let parsed = graph::parse_xml(&xml)?;
        if parsed.nodes.is_empty() {
            return Err(OsmGraphError::EmptyGraph);
        }
        let g = graph::create_graph(parsed.nodes, parsed.ways, retain_all, false);
        let sg = SpatialGraph::new(g);
        cache::insert_into_cache(graph_key, sg.clone())?;
        sg
    };

    let node_index = sg.nearest_node(lat, lon).ok_or(OsmGraphError::NodeNotFound)?;
    let shared_graph = Arc::clone(&sg.graph); // O(1) refcount bump — no graph copy
    let isochrones = calculate_isochrones_concurrently(
        shared_graph,
        node_index,
        time_limits,
        network_type,
        hull_type,
    );

    Ok((isochrones, sg))
}
