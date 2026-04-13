use petgraph::graph::DiGraph;
use petgraph::dot::{Dot, Config};
use std::fs::File;
use std::io::Write;

// Save the road network graph to the dot file format
#[allow(dead_code)]
fn save_roadnetwork_to_dot<N, E>(graph: &DiGraph<N, E>, filename: &str)
where
    N: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let dot = Dot::with_config(graph, &[Config::EdgeNoLabel]);
    let mut file = File::create(filename).unwrap();
    write!(file, "{:?}", dot).unwrap();
}
