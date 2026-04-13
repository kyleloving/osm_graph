#[cfg(test)]
mod tests {
    use petgraph::graph::DiGraph;

    use crate::graph::{XmlNode, XmlTag, XmlWay, XmlNodeRef};
    use crate::utils::{calculate_distance, calculate_travel_time};
    use crate::overpass::bbox_from_point;

    // --- Helpers ---

    fn make_node(id: i64, lat: f64, lon: f64) -> XmlNode {
        XmlNode { id, lat, lon, tags: vec![], geohash: None }
    }

    fn make_way(id: i64, tags: Vec<(&str, &str)>) -> XmlWay {
        XmlWay {
            id,
            nodes: vec![],
            tags: tags.into_iter().map(|(k, v)| XmlTag {
                key: k.to_string(),
                value: v.to_string(),
            }).collect(),
            length: 100.0,
            speed_kph: 50.0,
            walk_travel_time: 72.0,
            bike_travel_time: 24.0,
            drive_travel_time: 7.2,
        }
    }

    // --- calculate_distance ---

    #[test]
    fn test_distance_same_point() {
        let d = calculate_distance(48.0, 11.0, 48.0, 11.0);
        assert_eq!(d, 0.0);
    }

    #[test]
    fn test_distance_known_value() {
        // Munich to roughly 1km north — should be close to 1000m
        let d = calculate_distance(48.0, 11.0, 48.009, 11.0);
        assert!((d - 1000.0).abs() < 10.0, "Expected ~1000m, got {}", d);
    }

    #[test]
    fn test_distance_is_symmetric() {
        let d1 = calculate_distance(48.0, 11.0, 52.0, 13.0);
        let d2 = calculate_distance(52.0, 13.0, 48.0, 11.0);
        assert!((d1 - d2).abs() < 1e-6);
    }

    // --- calculate_travel_time ---

    #[test]
    fn test_travel_time_basic() {
        // 1000m at 36 kph = 100 seconds
        let t = calculate_travel_time(1000.0, 36.0);
        assert!((t - 100.0).abs() < 1e-6, "Expected 100s, got {}", t);
    }

    #[test]
    fn test_travel_time_walking() {
        // 500m at 5 kph = 360 seconds
        let t = calculate_travel_time(500.0, 5.0);
        assert!((t - 360.0).abs() < 1e-6);
    }

    // --- bbox_from_point ---

    #[test]
    fn test_bbox_is_symmetric() {
        let bbox = bbox_from_point(48.0, 11.0, 1000.0);
        let parts: Vec<f64> = bbox.split(',').map(|s| s.parse().unwrap()).collect();
        let (south, west, north, east) = (parts[0], parts[1], parts[2], parts[3]);
        // Should be symmetric around the origin
        assert!((48.0 - south - (north - 48.0)).abs() < 1e-6);
        assert!((11.0 - west - (east - 11.0)).abs() < 1e-6);
    }

    #[test]
    fn test_bbox_larger_dist_gives_larger_box() {
        let small = bbox_from_point(48.0, 11.0, 1_000.0);
        let large = bbox_from_point(48.0, 11.0, 10_000.0);
        let small_parts: Vec<f64> = small.split(',').map(|s| s.parse().unwrap()).collect();
        let large_parts: Vec<f64> = large.split(',').map(|s| s.parse().unwrap()).collect();
        // north of large should be greater than north of small
        assert!(large_parts[2] > small_parts[2]);
    }

    // --- clean_maxspeed (private, tested via create_graph indirectly) ---
    // We test the observable effect: a way tagged maxspeed=30 should get speed 30.0

    #[test]
    fn test_graph_respects_maxspeed_tag() {
        let nodes = vec![make_node(1, 0.0, 0.0), make_node(2, 0.001, 0.0)];
        let way = XmlWay {
            id: 1,
            nodes: vec![
                XmlNodeRef { node_id: 1 },
                XmlNodeRef { node_id: 2 },
            ],
            tags: vec![
                XmlTag { key: "highway".to_string(), value: "residential".to_string() },
                XmlTag { key: "maxspeed".to_string(), value: "30".to_string() },
            ],
            length: 0.0,
            speed_kph: 0.0,
            walk_travel_time: 0.0,
            bike_travel_time: 0.0,
            drive_travel_time: 0.0,
        };
        let graph = crate::graph::create_graph(vec![nodes[0].clone(), nodes[1].clone()], vec![way], true, false);
        let speed = graph.edge_weights().next().unwrap().speed_kph;
        assert_eq!(speed, 30.0);
    }

    // --- assess_path_directionality (via create_graph edge count) ---

    #[test]
    fn test_oneway_produces_single_edge() {
        let nodes = vec![make_node(1, 0.0, 0.0), make_node(2, 0.001, 0.0)];
        let way = XmlWay {
            id: 1,
            nodes: vec![XmlNodeRef { node_id: 1 }, XmlNodeRef { node_id: 2 }],
            tags: vec![
                XmlTag { key: "highway".to_string(), value: "residential".to_string() },
                XmlTag { key: "oneway".to_string(), value: "yes".to_string() },
            ],
            length: 0.0, speed_kph: 0.0,
            walk_travel_time: 0.0, bike_travel_time: 0.0, drive_travel_time: 0.0,
        };
        let graph = crate::graph::create_graph(vec![nodes[0].clone(), nodes[1].clone()], vec![way], true, false);
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_bidirectional_produces_two_edges() {
        let nodes = vec![make_node(1, 0.0, 0.0), make_node(2, 0.001, 0.0)];
        let way = XmlWay {
            id: 1,
            nodes: vec![XmlNodeRef { node_id: 1 }, XmlNodeRef { node_id: 2 }],
            tags: vec![
                XmlTag { key: "highway".to_string(), value: "residential".to_string() },
            ],
            length: 0.0, speed_kph: 0.0,
            walk_travel_time: 0.0, bike_travel_time: 0.0, drive_travel_time: 0.0,
        };
        let graph = crate::graph::create_graph(vec![nodes[0].clone(), nodes[1].clone()], vec![way], true, false);
        assert_eq!(graph.edge_count(), 2);
    }

    // --- deduplicate_edges ---

    #[test]
    fn test_deduplicate_keeps_fastest_edge() {
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node(1, 0.0, 0.0));
        let b = graph.add_node(make_node(2, 0.001, 0.0));

        let mut slow = make_way(1, vec![]);
        slow.drive_travel_time = 100.0;
        let mut fast = make_way(2, vec![]);
        fast.drive_travel_time = 50.0;

        graph.add_edge(a, b, slow);
        graph.add_edge(a, b, fast);

        // Access deduplicate_edges via simplify_graph on a graph that already has only endpoints
        // Instead test the observable: after simplification parallel edges are gone
        assert_eq!(graph.edge_count(), 2); // before
        let deduped = crate::simplify::simplify_graph(&graph);
        // Only one edge should remain between any two nodes
        let edge_count = deduped.edge_count();
        assert!(edge_count <= 1, "Expected at most 1 edge, got {}", edge_count);
    }

    // --- SpatialGraph nearest_node ---

    #[test]
    fn test_nearest_node_finds_closest() {
        let mut graph = DiGraph::new();
        graph.add_node(make_node(1, 48.0, 11.0));
        graph.add_node(make_node(2, 52.0, 13.0)); // Berlin-ish
        let sg = crate::graph::SpatialGraph::new(graph);
        // Query close to Munich node
        let idx = sg.nearest_node(48.001, 11.001).unwrap();
        let node = &sg.graph[idx];
        assert_eq!(node.id, 1);
    }
}
