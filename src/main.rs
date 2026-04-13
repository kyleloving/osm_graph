#![allow(dead_code)]

mod graph;
mod isochrone;
mod overpass;
mod utils;
mod cache;
mod simplify;
mod error;
mod tests;

use std::time::Instant;
use error::OsmGraphError;

#[tokio::main]
async fn main() -> Result<(), OsmGraphError> {
    let lat = 48.123456;
    let lon = 11.123456;
    let max_dist = 10_000.0;
    let time_limits = vec![300.0, 600.0, 900.0, 1_200.0, 1_500.0, 1_800.0];
    let network_type = overpass::NetworkType::Drive;
    let hull_type = isochrone::HullType::Convex;

    // --- Run WITHOUT simplification ---
    println!("Running WITHOUT simplification...");
    let start = Instant::now();
    let (_, unsimplified_sg) = isochrone::calculate_isochrones_from_point(
        lat, lon, Some(max_dist),
        time_limits.clone(),
        network_type,
        hull_type,
        true,
    )
    .await?;
    let unsimplified_duration = start.elapsed();
    println!(
        "  Nodes: {}, Edges: {}, Time: {:.2?}",
        unsimplified_sg.graph.node_count(),
        unsimplified_sg.graph.edge_count(),
        unsimplified_duration
    );

    cache::clear_cache()?;

    println!("Running WITH simplification...");
    let start = Instant::now();
    let (_, simplified_sg) = isochrone::calculate_isochrones_from_point(
        lat, lon, Some(max_dist),
        time_limits.clone(),
        network_type,
        hull_type,
        false,
    )
    .await?;
    let simplified_duration = start.elapsed();
    println!(
        "  Nodes: {}, Edges: {}, Time: {:.2?}",
        simplified_sg.graph.node_count(),
        simplified_sg.graph.edge_count(),
        simplified_duration
    );

    println!(
        "\nNode reduction:  {} -> {} ({:.1}% fewer)",
        unsimplified_sg.graph.node_count(),
        simplified_sg.graph.node_count(),
        (1.0 - simplified_sg.graph.node_count() as f64 / unsimplified_sg.graph.node_count() as f64) * 100.0
    );
    println!(
        "Edge reduction:  {} -> {} ({:.1}% fewer)",
        unsimplified_sg.graph.edge_count(),
        simplified_sg.graph.edge_count(),
        (1.0 - simplified_sg.graph.edge_count() as f64 / unsimplified_sg.graph.edge_count() as f64) * 100.0
    );
    println!(
        "Time difference: {:.2?} vs {:.2?}",
        unsimplified_duration, simplified_duration
    );

    Ok(())
}
