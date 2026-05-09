use crate::cache;
use crate::error::OsmGraphError;
use crate::graph::{self, SpatialGraph};
use crate::overpass;
use crate::overpass::NetworkType;
use crate::reachability::{compute_reachability, ReachabilityResult};
use geo::{ConvexHull, LineString, MultiPoint, Polygon};
use petgraph::prelude::*;
use spade::{DelaunayTriangulation, HasPosition, Point2, Triangulation};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const SATURATED_REUSE_RATIO: f64 = 0.99;
const CONTOUR_KEY_SCALE: f64 = 10.0;

pub fn calculate_isochrones(
    graph: &DiGraph<graph::XmlNode, graph::XmlWay>,
    start_node: NodeIndex,
    time_limits: Vec<f64>,
) -> Vec<Polygon> {
    let max_cost = time_limits.iter().cloned().fold(0.0_f64, f64::max);
    let result = compute_reachability(graph, start_node, max_cost, NetworkType::Drive);
    build_isochrone_polygons(graph, &result, &time_limits)
}

/// Build one isochrone polygon per requested time limit from a precomputed
/// `ReachabilityResult`. Polygons are built in parallel (one scoped thread per
/// limit). The returned vector is in the same order as `time_limits`.
///
/// Limits greater than `result.max_cost` are clamped to `max_cost` — the result
/// only contains nodes that were searched within that budget.
pub fn build_isochrone_polygons(
    graph: &DiGraph<graph::XmlNode, graph::XmlWay>,
    result: &ReachabilityResult,
    time_limits: &[f64],
) -> Vec<Polygon> {
    let mut node_times: Vec<(NodeIndex, f64)> =
        result.distances.iter().map(|(&n, &t)| (n, t)).collect();
    node_times.sort_by(|a, b| a.1.total_cmp(&b.1));
    if node_times.is_empty() {
        return time_limits.iter().map(|_| empty_polygon()).collect();
    }

    let max_seen = node_times
        .iter()
        .map(|(_, time)| *time)
        .fold(0.0_f64, f64::max);

    build_triangulated_isochrones(graph, &node_times, time_limits, max_seen)
}

fn is_saturated_limit(node_times: &[(NodeIndex, f64)], limit: f64) -> bool {
    if node_times.is_empty() {
        return false;
    }
    let reachable = upper_bound_node_times(node_times, limit);
    (reachable as f64 / node_times.len() as f64) >= SATURATED_REUSE_RATIO
}

fn convex_hull_from_points(points: Vec<(f64, f64)>) -> Polygon {
    if points.len() < 3 {
        return empty_polygon();
    }

    let points: MultiPoint<f64> = points.into();
    points.convex_hull()
}

fn empty_polygon() -> Polygon {
    Polygon::new(LineString::new(vec![]), vec![])
}

fn upper_bound_node_times(node_times: &[(NodeIndex, f64)], time: f64) -> usize {
    node_times.partition_point(|(_, candidate)| *candidate <= time)
}

#[derive(Clone, Copy)]
struct IsoVertex {
    position: Point2<f64>,
    lat: f64,
    lon: f64,
    time: f64,
}

impl HasPosition for IsoVertex {
    type Scalar = f64;

    fn position(&self) -> Point2<f64> {
        self.position
    }
}

struct TriangulatedSurface {
    triangulation: DelaunayTriangulation<IsoVertex>,
}

#[derive(Clone, Copy)]
struct ContourPoint {
    x: f64,
    y: f64,
    lat: f64,
    lon: f64,
}

#[derive(Clone, Copy)]
struct ContourSegment {
    from: ContourPoint,
    to: ContourPoint,
}

type ContourKey = (i64, i64);

fn build_triangulated_isochrones(
    graph: &DiGraph<graph::XmlNode, graph::XmlWay>,
    node_times: &[(NodeIndex, f64)],
    time_limits: &[f64],
    max_seen: f64,
) -> Vec<Polygon> {
    let all_points = || {
        node_times
            .iter()
            .map(|(node, _)| graph::node_to_latlon(graph, *node))
            .collect()
    };

    let Some(surface) = TriangulatedSurface::from_graph_times(graph, node_times) else {
        return time_limits
            .iter()
            .map(|_| convex_hull_from_points(all_points()))
            .collect();
    };

    let saturated_polygon = time_limits
        .iter()
        .any(|&limit| limit >= max_seen || is_saturated_limit(node_times, limit))
        .then(|| convex_hull_from_points(all_points()));

    time_limits
        .iter()
        .map(|&limit| {
            if limit >= max_seen || is_saturated_limit(node_times, limit) {
                if let Some(polygon) = &saturated_polygon {
                    return polygon.clone();
                }
            }
            surface.contour_polygon(limit)
        })
        .collect()
}

impl TriangulatedSurface {
    fn from_graph_times(
        graph: &DiGraph<graph::XmlNode, graph::XmlWay>,
        node_times: &[(NodeIndex, f64)],
    ) -> Option<Self> {
        if node_times.len() < 3 {
            return None;
        }

        let origin_lat = node_times
            .iter()
            .map(|(node, _)| graph[*node].lat)
            .sum::<f64>()
            / node_times.len() as f64;
        let cos_lat = origin_lat.to_radians().cos();
        let mut seen = HashSet::new();
        let mut vertices = Vec::with_capacity(node_times.len());

        for &(node, time) in node_times {
            let osm_node = &graph[node];
            let x = osm_node.lon * 111_320.0 * cos_lat;
            let y = osm_node.lat * 111_320.0;
            let key = (
                (x * CONTOUR_KEY_SCALE) as i64,
                (y * CONTOUR_KEY_SCALE) as i64,
            );
            if seen.insert(key) {
                vertices.push(IsoVertex {
                    position: Point2::new(x, y),
                    lat: osm_node.lat,
                    lon: osm_node.lon,
                    time,
                });
            }
        }

        if vertices.len() < 3 {
            return None;
        }

        DelaunayTriangulation::bulk_load(vertices)
            .ok()
            .map(|triangulation| Self { triangulation })
    }

    fn contour_polygon(&self, limit: f64) -> Polygon {
        let segments = self.contour_segments(limit);
        if segments.is_empty() {
            return empty_polygon();
        }

        if let Some(ring) = largest_closed_ring(&segments) {
            return Polygon::new(LineString::from(ring), vec![]);
        }

        let mut points = Vec::with_capacity(segments.len() * 2);
        for segment in segments {
            points.push((segment.from.lat, segment.from.lon));
            points.push((segment.to.lat, segment.to.lon));
        }
        convex_hull_from_points(points)
    }

    fn contour_segments(&self, limit: f64) -> Vec<ContourSegment> {
        let mut segments = Vec::new();
        for face in self.triangulation.inner_faces() {
            let vertices = face.vertices();
            let triangle = [
                *vertices[0].data(),
                *vertices[1].data(),
                *vertices[2].data(),
            ];
            if let Some(segment) = triangle_contour_segment(triangle, limit) {
                segments.push(segment);
            }
        }
        segments
    }
}

fn triangle_contour_segment(vertices: [IsoVertex; 3], limit: f64) -> Option<ContourSegment> {
    let edges = [
        (vertices[0], vertices[1]),
        (vertices[1], vertices[2]),
        (vertices[2], vertices[0]),
    ];
    let mut points = Vec::with_capacity(2);

    for (from, to) in edges {
        let crosses =
            (from.time <= limit && to.time > limit) || (to.time <= limit && from.time > limit);
        if crosses {
            let ratio = ((limit - from.time) / (to.time - from.time)).clamp(0.0, 1.0);
            points.push(interpolate_contour_point(from, to, ratio));
        }
    }

    if points.len() == 2 && contour_key(points[0]) != contour_key(points[1]) {
        Some(ContourSegment {
            from: points[0],
            to: points[1],
        })
    } else {
        None
    }
}

fn interpolate_contour_point(from: IsoVertex, to: IsoVertex, ratio: f64) -> ContourPoint {
    ContourPoint {
        x: from.position.x + (to.position.x - from.position.x) * ratio,
        y: from.position.y + (to.position.y - from.position.y) * ratio,
        lat: from.lat + (to.lat - from.lat) * ratio,
        lon: from.lon + (to.lon - from.lon) * ratio,
    }
}

fn largest_closed_ring(segments: &[ContourSegment]) -> Option<Vec<(f64, f64)>> {
    let mut adjacency: HashMap<ContourKey, Vec<ContourKey>> = HashMap::new();
    let mut points: HashMap<ContourKey, ContourPoint> = HashMap::new();
    let mut unused = HashSet::new();

    for segment in segments {
        let from = contour_key(segment.from);
        let to = contour_key(segment.to);
        if from == to {
            continue;
        }
        points.entry(from).or_insert(segment.from);
        points.entry(to).or_insert(segment.to);
        adjacency.entry(from).or_default().push(to);
        adjacency.entry(to).or_default().push(from);
        unused.insert(normalized_edge(from, to));
    }

    let mut best_ring = None;
    let mut best_area = 0.0;

    while let Some(&(start, first_next)) = unused.iter().next() {
        let mut ring_keys = vec![start];
        let mut previous = start;
        let mut current = first_next;
        let mut closed = false;

        loop {
            unused.remove(&normalized_edge(previous, current));
            ring_keys.push(current);

            if current == start {
                closed = true;
                break;
            }

            let Some(next) = adjacency.get(&current).and_then(|neighbors| {
                neighbors.iter().copied().find(|candidate| {
                    *candidate != previous && unused.contains(&normalized_edge(current, *candidate))
                })
            }) else {
                break;
            };

            previous = current;
            current = next;
        }

        if closed && ring_keys.len() >= 4 {
            let ring_points: Vec<ContourPoint> = ring_keys
                .iter()
                .filter_map(|key| points.get(key).copied())
                .collect();
            let area = projected_ring_area(&ring_points).abs();
            if area > best_area {
                best_area = area;
                best_ring = Some(
                    ring_points
                        .into_iter()
                        .map(|point| (point.lat, point.lon))
                        .collect(),
                );
            }
        }
    }

    best_ring
}

fn contour_key(point: ContourPoint) -> ContourKey {
    (
        (point.x * CONTOUR_KEY_SCALE).round() as i64,
        (point.y * CONTOUR_KEY_SCALE).round() as i64,
    )
}

fn normalized_edge(a: ContourKey, b: ContourKey) -> (ContourKey, ContourKey) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn projected_ring_area(ring: &[ContourPoint]) -> f64 {
    if ring.len() < 4 {
        return 0.0;
    }

    ring.windows(2)
        .map(|pair| pair[0].x * pair[1].y - pair[1].x * pair[0].y)
        .sum::<f64>()
        * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> ContourPoint {
        ContourPoint {
            x,
            y,
            lat: y,
            lon: x,
        }
    }

    #[test]
    fn contour_segments_stitch_into_closed_ring() {
        let segments = vec![
            ContourSegment {
                from: point(0.0, 0.0),
                to: point(1.0, 0.0),
            },
            ContourSegment {
                from: point(1.0, 0.0),
                to: point(1.0, 1.0),
            },
            ContourSegment {
                from: point(1.0, 1.0),
                to: point(0.0, 1.0),
            },
            ContourSegment {
                from: point(0.0, 1.0),
                to: point(0.0, 0.0),
            },
        ];

        let ring = largest_closed_ring(&segments).expect("square should close");
        assert_eq!(ring.first(), ring.last());
        assert_eq!(ring.len(), 5);
    }
}

pub fn calculate_isochrones_concurrently(
    graph: std::sync::Arc<DiGraph<graph::XmlNode, graph::XmlWay>>,
    start_node: NodeIndex,
    time_limits: Vec<f64>,
    network_type: overpass::NetworkType,
) -> Vec<Polygon> {
    let max_cost = time_limits.iter().cloned().fold(0.0_f64, f64::max);
    let result = compute_reachability(&graph, start_node, max_cost, network_type);
    build_isochrone_polygons(&graph, &result, &time_limits)
}

impl SpatialGraph {
    /// Build isochrone polygons for one or more time limits from a lat/lon origin.
    ///
    /// Each polygon encloses all nodes reachable within the corresponding time
    /// limit. The returned `Vec` is in the same order as `time_limits`.
    ///
    /// Returns `None` if no graph node is found near `(lat, lon)`.
    pub fn isochrones(
        &self,
        lat: f64,
        lon: f64,
        time_limits: Vec<f64>,
        network_type: NetworkType,
    ) -> Option<Vec<Polygon>> {
        let start_node = self.nearest_node(lat, lon)?;
        Some(calculate_isochrones_concurrently(
            Arc::clone(&self.graph),
            start_node,
            time_limits,
            network_type,
        ))
    }
}

pub async fn calculate_isochrones_from_point(
    lat: f64,
    lon: f64,
    max_dist: Option<f64>,
    time_limits: Vec<f64>,
    network_type: overpass::NetworkType,
    retain_all: bool,
) -> Result<(Vec<Polygon>, SpatialGraph), OsmGraphError> {
    // Auto-size bounding box if not provided.
    // Use max time limit * a generous speed + 20% buffer to ensure the
    // isochrone never saturates into a square at the bbox boundary.
    let max_speed_m_per_s = match network_type {
        NetworkType::Walk => 5.0 / 3.6,
        NetworkType::Bike => 25.0 / 3.6,
        NetworkType::Drive
        | NetworkType::DriveService
        | NetworkType::All
        | NetworkType::AllPrivate => 120.0 / 3.6,
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
            cached_xml // in-memory hit
        } else if let Some(disk_xml) = cache::check_disk_xml_cache(&query) {
            cache::insert_into_xml_cache(query.clone(), disk_xml.clone())?; // promote to memory
            disk_xml // disk hit
        } else {
            let fetched =
                overpass::make_request("https://overpass-api.de/api/interpreter", &query).await?;
            cache::write_disk_xml_cache(&query, &fetched); // persist to disk (best-effort)
            cache::insert_into_xml_cache(query.clone(), fetched.clone())?;
            fetched // network fetch
        };

        let parsed = graph::parse_xml(&xml)?;
        if parsed.nodes.is_empty() {
            return Err(OsmGraphError::EmptyGraph);
        }
        let bidirectional = matches!(network_type, NetworkType::Walk);
        let g = graph::create_graph(parsed.nodes, parsed.ways, retain_all, bidirectional);
        let sg = SpatialGraph::new(g);
        cache::insert_into_cache(graph_key, sg.clone())?;
        sg
    };

    let node_index = sg
        .nearest_node(lat, lon)
        .ok_or(OsmGraphError::NodeNotFound)?;
    let shared_graph = Arc::clone(&sg.graph); // O(1) refcount bump — no graph copy
    let isochrones =
        calculate_isochrones_concurrently(shared_graph, node_index, time_limits, network_type);

    Ok((isochrones, sg))
}
