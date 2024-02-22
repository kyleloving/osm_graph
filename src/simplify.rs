// WIP
fn simplify_graph(graph: &DiGraph<XmlNode, XmlWay>) -> DiGraph<XmlNode, XmlWay> {
    let mut simplified_graph = DiGraph::new();
    let mut endpoints = HashSet::new();
    let mut index_map = HashMap::new();

    // Identify endpoints and add them to the simplified graph
    for node in graph.node_indices() {
        if is_endpoint(graph, node) {
            endpoints.insert(node);
            let new_index = simplified_graph.add_node(graph[node].clone());
            index_map.insert(node, new_index);
        }
    }

    // Consolidate intersections


    // Build and simplify paths
    for &endpoint in &endpoints {
        for neighbor in graph.neighbors(endpoint) {
            if endpoints.contains(&neighbor) || simplified_graph.contains_edge(endpoint, neighbor) {
                continue;
            }

            let path = build_path(graph, endpoint, &endpoints);
            if let Some(&last) = path.last() {
                if endpoints.contains(&last) {
                    // Aggregate edge data along the path
                    let mut total_length = 0.0;
                    let mut total_time = 0.0;
                    let mut speeds = Vec::new();

                    for window in path.windows(2) {
                        if let [u, v] = window {
                            if let Some(edge) = graph.find_edge(*u, *v) {
                                let way = graph.edge_weight(edge).unwrap();
                                total_length += way.length;
                                total_time += way.travel_time;
                                speeds.push(way.speed_kph);
                            }
                        }
                    }

                    // Calculate average speed
                    let avg_speed = if !speeds.is_empty() {
                        speeds.iter().sum::<f64>() / speeds.len() as f64
                    } else {
                        0.0
                    };

                    // Create a new XmlWay with the aggregated data
                    let xml_way = XmlWay {
                        id: 0, // You might want to generate a unique ID or handle this differently
                        nodes: vec![],
                        tags: vec![],
                        length: total_length,
                        travel_time: total_time,
                        speed_kph: avg_speed,
                    };
                    let new_endpoint = *index_map.get(&endpoint).unwrap();
                    let new_last = *index_map.get(&last).unwrap();

                    simplified_graph.add_edge(new_endpoint, new_last, xml_way);
                }
            }
        }
    }

    simplified_graph
}

fn is_endpoint(
    graph: &DiGraph<XmlNode, XmlWay>, 
    node_index: NodeIndex
) -> bool {
    let out_neighbors: HashSet<_> = graph
        .neighbors_directed(node_index, petgraph::Outgoing)
        .collect();
    let in_neighbors: HashSet<_> = graph
        .neighbors_directed(node_index, petgraph::Incoming)
        .collect();
    let total_neighbors: HashSet<_> = out_neighbors.union(&in_neighbors).collect();

    let out_degree = out_neighbors.len();
    let in_degree = in_neighbors.len();
    let total_degree = total_neighbors.len();

    // Check if self-loop exists
    if out_neighbors.contains(&node_index) || in_neighbors.contains(&node_index) {
        return true;
    }

    // Check if no incoming or outgoing edges
    if out_degree == 0 || in_degree == 0 {
        return true;
    }

    // Check the degree condition
    if total_degree != 2 && total_degree != 4 {
        return true;
    }

    // // Rule 4: Differing edge attribute values
    // for attr in endpoint_attrs {
    //     let mut in_values = HashSet::new();
    //     let mut out_values = HashSet::new();

    //     for edge in graph.edges_directed(node_index, petgraph::Incoming) {
    //         if let Some(value) = edge.weight().tags.iter().find(|tag| tag.key == *attr) {
    //             in_values.insert(&value.value);
    //         }
    //     }

    //     for edge in graph.edges_directed(node_index, petgraph::Outgoing) {
    //         if let Some(value) = edge.weight().tags.iter().find(|tag| tag.key == *attr) {
    //             out_values.insert(&value.value);
    //         }
    //     }

    //     // Check if there's more than one unique value across in and out edges
    //     if in_values.union(&out_values).count() > 1 {
    //         return true;
    //     }
    // }

    false
}

fn build_path(
    graph: &DiGraph<XmlNode, XmlWay>,
    start: NodeIndex,
    endpoints: &HashSet<NodeIndex>,
) -> Vec<NodeIndex> {
    let mut path = vec![start];
    let mut current = start;

    while !endpoints.contains(&current) {
        if let Some(successor) = graph
            .neighbors_directed(current, petgraph::Outgoing)
            .find(|&n| !path.contains(&n))
        {
            path.push(successor);
            current = successor;
        } else {
            break;
        }
    }

    path
}

fn consolidate_intersections(
    graph: &DiGraph<XmlNode, XmlWay>,
    distance_threshold: f64,
) -> DiGraph<XmlNode, XmlWay> {
    let mut new_graph = DiGraph::new();
    let mut node_group_map = HashMap::new(); // Maps original nodes to their new consolidated node
    let mut groups = Vec::new(); // Vec of node groups, each represented by a Vec<NodeIndex>

    // Step 1: Identify nodes to consolidate
    // This is a placeholder for your logic to group nodes
    // For example, you could implement spatial clustering based on node coordinates
    for node_index in graph.node_indices() {
        // Your logic here to determine which group this node belongs to
        // For simplicity, let's assume you have a function `find_group_for_node`
        // that determines the appropriate group for a node based on your criteria
        let group_id = find_group_for_node(&graph, node_index, distance_threshold);
        if groups.len() <= group_id {
            groups.resize(group_id + 1, Vec::new());
        }
        groups[group_id].push(node_index);
    }

    // Step 2: Merge nodes and add to new graph
    for group in groups.iter() {
        let new_node = merge_nodes(&graph, group); // Implement `merge_nodes` to create a new node based on the group
        let new_index = new_graph.add_node(new_node);
        for &old_index in group {
            node_group_map.insert(old_index, new_index);
        }
    }

    // Step 3: Reconnect edges
    for (_, &new_index) in node_group_map.iter() {
        // For each edge in the original graph, find its new start and end points in the new graph
        for edge in graph.edge_references() {
            let (source, target) = (edge.source(), edge.target());
            let new_source = *node_group_map.get(&source).unwrap();
            let new_target = *node_group_map.get(&target).unwrap();
            if new_source != new_index || new_target != new_index { // Avoid self-loops
                let new_edge = merge_edges(&graph, source, target); // Implement `merge_edges` based on your criteria
                new_graph.add_edge(new_source, new_target, new_edge);
            }
        }
    }

    new_graph
}

// Placeholder for your logic to find a group for a node based on distance or other criteria
fn find_group_for_node(
    graph: &DiGraph<XmlNode, XmlWay>,
    node_index: NodeIndex,
    distance_threshold: f64,
) -> usize {
    // Implement your logic here
    todo!()
}

// Placeholder for merging nodes into a new node
fn merge_nodes(graph: &DiGraph<XmlNode, XmlWay>, group: &[NodeIndex]) -> XmlNode {
    // Implement merging logic here, e.g., averaging positions
    todo!()
}

// Placeholder for merging edges into a new edge
fn merge_edges(
    graph: &DiGraph<XmlNode, XmlWay>,
    source: NodeIndex,
    target: NodeIndex,
) -> XmlWay {
    todo!()
}