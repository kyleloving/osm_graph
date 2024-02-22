use h3o::{LatLng, Resolution, Cell};
use petgraph::graph::NodeIndex;
use std::collections::{HashMap, HashSet};

fn index_nodes_with_h3(graph: &DiGraph<XmlNode, XmlWay>) -> HashMap<Cell, Vec<NodeIndex>> {
    let mut cell_to_nodes: HashMap<Cell, Vec<NodeIndex>> = HashMap::new();
    for node_index in graph.node_indices() {
        let node = &graph[node_index];
        let coord = LatLng::new(node.lat, node.lon).expect("valid coordinates");
        let cell = coord.to_cell(Resolution::Eleven).expect("valid cell");

        cell_to_nodes.entry(cell).or_insert_with(Vec::new).push(node_index);
    }
    cell_to_nodes
}