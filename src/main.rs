#![allow(dead_code)]

mod graph;
mod isochrone;
mod overpass;
mod utils;
mod cache;

#[tokio::main]
async fn main() {
    for i in 1..4 {
        println!("Run #{}", i);
        println!("------------------------");
        let _isochrone = isochrone::calculate_isochrones_from_point(
            48.123456,
            11.123456,
            10000.0,
            vec![300.0, 600.0, 900.0, 1200.0, 1500.0, 1800.0],
            overpass::NetworkType::Drive,
            isochrone::HullType::Convex,
        )
        .await
        .unwrap();
        // println!("{:?}", isochrone);
        println!("------------------------");
    }
}
