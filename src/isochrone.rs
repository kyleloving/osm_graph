use crate::graph;
use crate::overpass;
use crate::cache;

use geo::{ConcaveHull, ConvexHull, KNearestConcaveHull, MultiPoint, Polygon};
use petgraph::algo::dijkstra;
use petgraph::prelude::*;
use std::collections::HashSet;

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
        edge_weight.travel_time
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
    hull_type: HullType,
) -> Vec<Polygon> {
    let mut handles = vec![];

    for time_limit in time_limits {
        let graph_clone = std::sync::Arc::clone(&graph);
        let handle = std::thread::spawn(move || {
            // Call dijkstra and get the shortest paths HashMap
            let shortest_paths = dijkstra(&*graph_clone, start_node, None, |e| {
                let edge_weight = graph_clone.edge_weight(e.id()).unwrap();
                edge_weight.travel_time
            });

            // Iterate over the shortest paths and collect nodes within the time limit
            let isochrone_nodes = shortest_paths
                .into_iter()
                .filter_map(|(node, weight)| {
                    if weight <= time_limit {
                        Some(node)
                    } else {
                        None
                    }
                })
                .collect::<HashSet<NodeIndex>>();

            // Convert each node index in the isochrone to latitude/longitude
            let isochrone_lat_lons = isochrone_nodes
                .into_iter()
                .map(|node_index| graph::node_to_latlon(&*graph_clone, node_index))
                .collect::<Vec<_>>();

            let points: MultiPoint<f64> = isochrone_lat_lons.into();

            match hull_type {
                HullType::FastConcave => points.concave_hull(2.0),
                HullType::Concave => points.k_nearest_concave_hull(3),
                HullType::Convex => points.convex_hull(),
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to finish and collect the results
    handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect()
}


pub async fn calculate_isochrones_from_point(
    lat: f64,
    lon: f64,
    max_dist: f64,
    time_limits: Vec<f64>,
    network_type: overpass::NetworkType,
    hull_type: HullType,
) -> Result<Vec<Polygon>, reqwest::Error> {
  
    // Step 1: Construct the query using the network type and location
    let polygon_coord_str = overpass::bbox_from_point(lat, lon, max_dist);
    let query = overpass::create_overpass_query(&polygon_coord_str, network_type);

    // Check the cache first
    if let Some(graph) = cache::check_cache(&query) {

        // Step 5: Calculate Isochrone
        let node_index = graph::latlon_to_node(&graph, lat, lon).unwrap();

        let shared_graph = std::sync::Arc::new(graph);
        let isochrones = calculate_isochrones_concurrently(
                shared_graph,
                node_index,
                time_limits,
                hull_type,
            );

        return Ok(isochrones);
    } else {
        // Step 2: Make the request and get the response
        let response =
            overpass::make_request("https://overpass-api.de/api/interpreter", &query).await?;

        // Step 3: Parse XML
        let parsed = graph::parse_xml(&response).unwrap();

        // Step 4: Create Graph
        let graph = graph::create_graph(parsed.nodes, parsed.ways, false, false);
        // Insert into cache for future use
        cache::insert_into_cache(query, graph.clone());

        // Step 5: Calculate Isochrone
        let node_index = graph::latlon_to_node(&graph, lat, lon).unwrap();

        let shared_graph = std::sync::Arc::new(graph);
        let isochrones = calculate_isochrones_concurrently(
                shared_graph,
                node_index,
                time_limits,
                hull_type,
            );

        return Ok(isochrones);
    }
}
