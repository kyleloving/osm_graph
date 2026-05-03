//! Quick smoke test for the PBF reader.
//!
//! Usage:
//!     cargo run --release --example parse_pbf -- data/district-of-columbia-latest.osm.pbf [walk|bike|drive]
//!
//! Prints node/way/POI counts and the bbox of the parsed data.
//! `--release` is recommended — debug mode can take several minutes on a city PBF.

use std::env;
use std::time::Instant;

use pysochrone::overpass::NetworkType;
use pysochrone::pbf::read_pbf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: parse_pbf <path-to.osm.pbf> [walk|bike|drive]");
        std::process::exit(1);
    }
    let path = &args[1];
    let net = match args.get(2).map(|s| s.as_str()).unwrap_or("walk") {
        "walk" => NetworkType::Walk,
        "bike" => NetworkType::Bike,
        "drive" => NetworkType::Drive,
        other => {
            eprintln!("unknown network type '{}', expected walk|bike|drive", other);
            std::process::exit(1);
        }
    };

    println!("reading {} ({:?}) …", path, net);
    let start = Instant::now();
    let (data, poi_ids) = read_pbf(path, net)?;
    let elapsed = start.elapsed();

    let (mut min_lat, mut max_lat) = (f64::MAX, f64::MIN);
    let (mut min_lon, mut max_lon) = (f64::MAX, f64::MIN);
    for n in &data.nodes {
        min_lat = min_lat.min(n.lat); max_lat = max_lat.max(n.lat);
        min_lon = min_lon.min(n.lon); max_lon = max_lon.max(n.lon);
    }

    println!();
    println!("parsed in       {:.2}s", elapsed.as_secs_f64());
    println!("nodes (kept)    {}", data.nodes.len());
    println!("ways            {}", data.ways.len());
    println!("POI nodes       {}", poi_ids.len());
    println!("bbox (s,w,n,e)  {:.5}, {:.5}, {:.5}, {:.5}", min_lat, min_lon, max_lat, max_lon);

    Ok(())
}
