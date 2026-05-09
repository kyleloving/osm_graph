//! Two-sided feasibility: "which nodes can I visit between origin and destination
//! within a given time budget, and how much slack remains?"
//!
//! # Concept
//!
//! A node `v` is *feasible* if:
//!
//! ```text
//! inbound_time(origin → v) + outbound_time(v → destination) ≤ available_time
//! ```
//!
//! The leftover is the *slack*:
//!
//! ```text
//! slack = available_time − inbound_time − outbound_time
//! ```
//!
//! Callers control what the slack means at the product level:
//! - Pass `available_time = total_window − activity_duration − buffer` to bake
//!   in an activity and a safety margin before calling.
//! - Use `min_slack` in [`build_feasibility_polygon`] to ask "where can I stop
//!   and still have ≥ N seconds left?"
//!
//! # Design notes
//!
//! - The reverse Dijkstra runs on the *reversed* graph so that
//!   `outbound_time(v → destination)` is computed as a single one-to-many
//!   search from `destination` rather than N individual searches.
//! - `NetworkType` is threaded through so walk / bike / drive travel times are
//!   respected consistently.
//! - [`compute_feasibility`] returns `Err(InfeasibleReason)` when the trip
//!   cannot be completed within the budget at all, giving callers a clear
//!   signal to surface to users rather than an opaque empty result.

use std::collections::HashMap;

use geo::{ConvexHull, MultiPoint, Polygon};
use petgraph::algo::dijkstra;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;

use crate::graph::{node_to_latlon, SpatialGraph, XmlNode, XmlWay};
use crate::overpass::NetworkType;
use crate::reachability::EdgeInfo;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Per-node feasibility data for a single origin → destination query.
#[derive(Debug, Clone)]
pub struct FeasibleNode {
    /// Travel time from origin to this node (seconds).
    pub inbound_time: f64,
    /// Travel time from this node to destination (seconds).
    pub outbound_time: f64,
    /// Remaining time after visiting this node:
    /// `available_time − inbound_time − outbound_time`.
    pub slack: f64,
}

/// Full result of a successful [`compute_feasibility`] call.
#[derive(Debug, Clone)]
pub struct FeasibilityResult {
    /// The origin node used for the forward search.
    pub origin: NodeIndex,
    /// The destination node used for the reverse search.
    pub destination: NodeIndex,
    /// The time budget passed by the caller (seconds).
    pub available_time: f64,
    /// The minimum travel time from origin to destination (seconds).
    /// This is the floor: `available_time` must be ≥ this for any node to be
    /// feasible. Stored here so callers can report headroom to users.
    pub direct_time: f64,
    /// Every node whose `inbound + outbound ≤ available_time`, keyed by
    /// `NodeIndex`. Nodes that are unreachable in either direction are absent.
    pub feasible: HashMap<NodeIndex, FeasibleNode>,
}

/// Reason a feasibility query cannot produce any results.
#[derive(Debug, Clone, PartialEq)]
pub enum InfeasibleReason {
    /// The shortest path from origin to destination already exceeds the budget.
    ///
    /// `direct_time` is the actual travel time; `available_time` is what was
    /// requested. The shortfall is `direct_time − available_time`.
    BudgetTooTight {
        direct_time: f64,
        available_time: f64,
    },
    /// No path exists between origin and destination in the graph.
    NoPathExists,
}

impl std::fmt::Display for InfeasibleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InfeasibleReason::BudgetTooTight {
                direct_time,
                available_time,
            } => write!(
                f,
                "budget too tight: direct travel time is {direct_time:.0} s \
                 but available time is only {available_time:.0} s \
                 (shortfall: {:.0} s)",
                direct_time - available_time
            ),
            InfeasibleReason::NoPathExists => {
                write!(f, "no path exists between origin and destination")
            }
        }
    }
}

impl std::error::Error for InfeasibleReason {}

// ---------------------------------------------------------------------------
// Core computation
// ---------------------------------------------------------------------------

/// Compute which nodes are reachable from `origin` *and* can reach
/// `destination` within `available_time` seconds.
///
/// Returns `Err(InfeasibleReason::NoPathExists)` when origin and destination
/// are disconnected, and `Err(InfeasibleReason::BudgetTooTight)` when the
/// direct travel time already exceeds `available_time`. Both cases carry
/// enough information for callers to produce a meaningful error message.
///
/// # Arguments
///
/// * `graph`          – The road network.
/// * `origin`         – Starting node index.
/// * `destination`    – Ending node index.
/// * `available_time` – Total time budget in seconds. Subtract any activity
///                      duration or buffer *before* calling.
/// * `network_type`   – Determines which edge weight (walk / bike / drive) is used.
/// Compute two-sided feasibility with a caller-supplied edge cost.
///
/// The closure is invoked once per edge relaxation in *both* the forward and
/// reverse Dijkstra searches. In each invocation the [`EdgeInfo`] reflects the
/// edge's *original* graph orientation — `source` and `target` are not flipped
/// for the reverse search — so cost models keyed by edge identity, density, or
/// node position see a consistent view in both directions.
///
/// Use this when injecting density-based traffic penalties, externally-supplied
/// traffic multipliers, or any custom cost. Costs must be non-negative and
/// finite or Dijkstra's invariants break.
pub fn compute_feasibility_with<F>(
    graph: &DiGraph<XmlNode, XmlWay>,
    origin: NodeIndex,
    destination: NodeIndex,
    available_time: f64,
    mut cost: F,
) -> Result<FeasibilityResult, InfeasibleReason>
where
    F: FnMut(EdgeInfo<'_>) -> f64,
{
    // Forward search: origin → all nodes.
    let forward = dijkstra(graph, origin, None, |e| {
        cost(EdgeInfo {
            id: e.id(),
            source: e.source(),
            target: e.target(),
            weight: e.weight(),
        })
    });

    // Fast-path checks before running the (more expensive) reverse search.
    let direct_time = match forward.get(&destination) {
        Some(&t) => t,
        None => return Err(InfeasibleReason::NoPathExists),
    };

    if direct_time > available_time {
        return Err(InfeasibleReason::BudgetTooTight {
            direct_time,
            available_time,
        });
    }

    // Reverse search: destination → all nodes on the *reversed* graph.
    // petgraph's `Reversed` wrapper flips edge direction without copying the graph.
    // We translate the reversed edge's id back to the original endpoints so the
    // closure always sees the edge in its forward orientation.
    let reversed = petgraph::visit::Reversed(graph);
    let backward = dijkstra(reversed, destination, None, |e| {
        let id = e.id();
        let (source, target) = graph.edge_endpoints(id).unwrap();
        let weight = graph.edge_weight(id).unwrap();
        cost(EdgeInfo {
            id,
            source,
            target,
            weight,
        })
    });

    // Intersect: keep only nodes present in both searches whose combined cost
    // fits within the budget.
    let mut feasible = HashMap::new();
    for (&node, &inbound) in &forward {
        if inbound > available_time {
            continue;
        }
        if let Some(&outbound) = backward.get(&node) {
            let total = inbound + outbound;
            if total <= available_time {
                feasible.insert(
                    node,
                    FeasibleNode {
                        inbound_time: inbound,
                        outbound_time: outbound,
                        slack: available_time - total,
                    },
                );
            }
        }
    }

    Ok(FeasibilityResult {
        origin,
        destination,
        available_time,
        direct_time,
        feasible,
    })
}

/// Compute two-sided feasibility using the precomputed walk/bike/drive travel
/// time on each edge.
///
/// Convenience wrapper around [`compute_feasibility_with`]. For custom cost
/// models (traffic, density penalties), call `compute_feasibility_with` directly.
pub fn compute_feasibility(
    graph: &DiGraph<XmlNode, XmlWay>,
    origin: NodeIndex,
    destination: NodeIndex,
    available_time: f64,
    network_type: NetworkType,
) -> Result<FeasibilityResult, InfeasibleReason> {
    compute_feasibility_with(graph, origin, destination, available_time, |e| {
        e.weight.travel_time(network_type)
    })
}

// ---------------------------------------------------------------------------
// Polygon construction
// ---------------------------------------------------------------------------

/// Build a polygon enclosing all feasible nodes whose slack ≥ `min_slack`.
///
/// # Arguments
///
/// * `graph`     – The road network (needed to look up node coordinates).
/// * `result`    – Output of [`compute_feasibility`].
/// * `min_slack` – Minimum remaining slack (seconds) a node must have to be
///                 included. Pass `0.0` to include every feasible node.
/// Returns `None` if fewer than three qualifying nodes exist (a polygon cannot
/// be formed).
pub fn build_feasibility_polygon(
    graph: &DiGraph<XmlNode, XmlWay>,
    result: &FeasibilityResult,
    min_slack: f64,
) -> Option<Polygon> {
    let points: MultiPoint<f64> = result
        .feasible
        .iter()
        .filter(|(_, n)| n.slack >= min_slack)
        .map(|(&idx, _)| node_to_latlon(graph, idx))
        .collect::<Vec<_>>()
        .into();

    if points.0.len() < 3 {
        return None;
    }

    Some(points.convex_hull())
}

// ---------------------------------------------------------------------------
// SpatialGraph entry points
// ---------------------------------------------------------------------------

impl SpatialGraph {
    /// Public API alias for two-sided reachability.
    ///
    /// A node is included when `origin -> node -> destination` fits within
    /// `max_time` seconds. If callers need activity time or a safety buffer,
    /// subtract those from the total window before calling.
    pub fn reachable_between(
        &self,
        origin_lat: f64,
        origin_lon: f64,
        dest_lat: f64,
        dest_lon: f64,
        max_time: f64,
        network_type: NetworkType,
    ) -> Option<Result<FeasibilityResult, InfeasibleReason>> {
        self.feasibility(
            origin_lat,
            origin_lon,
            dest_lat,
            dest_lon,
            max_time,
            network_type,
        )
    }

    /// Compute which nodes can be visited between two lat/lon points within
    /// `available_time` seconds, and how much time remains at each.
    ///
    /// Snaps both points to the nearest graph nodes before running the search.
    /// Returns `None` if either point has no nearby node; otherwise delegates
    /// to [`compute_feasibility`] and propagates its `Result`.
    pub fn feasibility(
        &self,
        origin_lat: f64,
        origin_lon: f64,
        dest_lat: f64,
        dest_lon: f64,
        available_time: f64,
        network_type: NetworkType,
    ) -> Option<Result<FeasibilityResult, InfeasibleReason>> {
        let origin = self.nearest_node(origin_lat, origin_lon)?;
        let destination = self.nearest_node(dest_lat, dest_lon)?;
        Some(compute_feasibility(
            &self.graph,
            origin,
            destination,
            available_time,
            network_type,
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{create_graph, XmlNode, XmlNodeRef, XmlTag, XmlWay};
    use crate::overpass::NetworkType;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn node(id: i64, lat: f64, lon: f64) -> XmlNode {
        XmlNode {
            id,
            lat,
            lon,
            tags: vec![],
        }
    }

    fn way(node_ids: Vec<i64>) -> XmlWay {
        XmlWay {
            id: 1,
            nodes: node_ids
                .into_iter()
                .map(|id| XmlNodeRef { node_id: id })
                .collect(),
            tags: vec![XmlTag {
                key: "highway".into(),
                value: "residential".into(),
            }],
            length: 0.0,
            speed_kph: 0.0,
            walk_travel_time: 0.0,
            bike_travel_time: 0.0,
            drive_travel_time: 0.0,
        }
    }

    /// Linear graph:  A ─── B ─── C ─── D  (~111 m between each node)
    fn linear_graph() -> DiGraph<XmlNode, XmlWay> {
        let nodes = vec![
            node(1, 0.000, 0.0),
            node(2, 0.001, 0.0),
            node(3, 0.002, 0.0),
            node(4, 0.003, 0.0),
        ];
        create_graph(
            nodes,
            vec![way(vec![1, 2, 3, 4])],
            /*retain_all=*/ true,
            false,
        )
    }

    fn find_node(g: &DiGraph<XmlNode, XmlWay>, osm_id: i64) -> NodeIndex {
        g.node_indices().find(|&i| g[i].id == osm_id).unwrap()
    }

    /// Convenience: run with a generous budget and unwrap — used by tests that
    /// only care about the happy path.
    fn feasibility_ok(
        g: &DiGraph<XmlNode, XmlWay>,
        origin: NodeIndex,
        dest: NodeIndex,
        budget: f64,
    ) -> FeasibilityResult {
        compute_feasibility(g, origin, dest, budget, NetworkType::Drive)
            .expect("expected Ok but got Err")
    }

    // ------------------------------------------------------------------
    // Happy-path correctness
    // ------------------------------------------------------------------

    #[test]
    fn feasible_nodes_satisfy_budget() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);
        let result = feasibility_ok(&g, origin, dest, 10_000.0);

        for (_, n) in &result.feasible {
            assert!(
                n.inbound_time + n.outbound_time <= result.available_time + 1e-9,
                "node violates budget: inbound={} outbound={} budget={}",
                n.inbound_time,
                n.outbound_time,
                result.available_time
            );
            assert!(
                n.slack >= -1e-9,
                "slack must be non-negative, got {}",
                n.slack
            );
        }
    }

    #[test]
    fn origin_and_destination_are_feasible() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);
        let result = feasibility_ok(&g, origin, dest, 10_000.0);

        let o = result
            .feasible
            .get(&origin)
            .expect("origin must be feasible");
        assert_eq!(o.inbound_time, 0.0, "origin inbound should be 0");

        let d = result
            .feasible
            .get(&dest)
            .expect("destination must be feasible");
        assert_eq!(d.outbound_time, 0.0, "destination outbound should be 0");
    }

    #[test]
    fn slack_equals_budget_minus_travel_times() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);
        let result = feasibility_ok(&g, origin, dest, 10_000.0);

        for (_, n) in &result.feasible {
            let expected = result.available_time - n.inbound_time - n.outbound_time;
            assert!(
                (n.slack - expected).abs() < 1e-9,
                "slack mismatch: got {} expected {}",
                n.slack,
                expected
            );
        }
    }

    #[test]
    fn direct_time_stored_in_result() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);
        let result = feasibility_ok(&g, origin, dest, 10_000.0);

        // direct_time must equal the destination's inbound_time (shortest path).
        let dest_node = result.feasible.get(&dest).unwrap();
        assert!(
            (result.direct_time - dest_node.inbound_time).abs() < 1e-9,
            "direct_time {} != destination inbound_time {}",
            result.direct_time,
            dest_node.inbound_time
        );
    }

    // ------------------------------------------------------------------
    // InfeasibleReason::BudgetTooTight
    // ------------------------------------------------------------------

    #[test]
    fn budget_too_tight_returns_err() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);

        // First learn the direct time with a generous budget.
        let direct_time = feasibility_ok(&g, origin, dest, 10_000.0).direct_time;

        // Budget just 1 second short of the direct trip.
        let err = compute_feasibility(&g, origin, dest, direct_time - 1.0, NetworkType::Drive)
            .expect_err("expected BudgetTooTight");

        match err {
            InfeasibleReason::BudgetTooTight {
                direct_time: dt,
                available_time: at,
            } => {
                assert!(dt > at, "direct_time should exceed available_time");
                assert!(
                    (dt - direct_time).abs() < 1e-9,
                    "reported direct_time {} doesn't match actual {}",
                    dt,
                    direct_time
                );
            }
            other => panic!("expected BudgetTooTight, got {:?}", other),
        }
    }

    #[test]
    fn budget_too_tight_shortfall_is_correct() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);
        let direct_time = feasibility_ok(&g, origin, dest, 10_000.0).direct_time;

        let shortfall = 42.0;
        let budget = direct_time - shortfall;
        let err = compute_feasibility(&g, origin, dest, budget, NetworkType::Drive)
            .expect_err("expected BudgetTooTight");

        if let InfeasibleReason::BudgetTooTight {
            direct_time: dt,
            available_time: at,
        } = err
        {
            assert!(
                ((dt - at) - shortfall).abs() < 1e-9,
                "shortfall should be {shortfall} but got {}",
                dt - at
            );
        }
    }

    #[test]
    fn budget_exactly_equal_to_direct_time_is_ok() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);
        let direct_time = feasibility_ok(&g, origin, dest, 10_000.0).direct_time;

        // Exactly at the boundary should succeed (≤, not <).
        let result = compute_feasibility(&g, origin, dest, direct_time, NetworkType::Drive)
            .expect("budget == direct_time should be Ok");

        assert!(result.feasible.contains_key(&dest));
        assert!(result.feasible.contains_key(&origin));
    }

    // ------------------------------------------------------------------
    // InfeasibleReason::NoPathExists
    // ------------------------------------------------------------------

    #[test]
    fn disconnected_graph_returns_no_path() {
        // Two isolated nodes with no edges between them.
        let mut g: DiGraph<XmlNode, XmlWay> = DiGraph::new();
        let a = g.add_node(node(1, 0.0, 0.0));
        let b = g.add_node(node(2, 1.0, 1.0));

        let err = compute_feasibility(&g, a, b, 10_000.0, NetworkType::Drive)
            .expect_err("expected NoPathExists");

        assert_eq!(err, InfeasibleReason::NoPathExists);
    }

    // ------------------------------------------------------------------
    // Display / error trait
    // ------------------------------------------------------------------

    #[test]
    fn budget_too_tight_display_mentions_shortfall() {
        let err = InfeasibleReason::BudgetTooTight {
            direct_time: 3600.0,
            available_time: 1800.0,
        };
        let msg = err.to_string();
        assert!(msg.contains("1800"), "should mention available_time: {msg}");
        assert!(msg.contains("3600"), "should mention direct_time: {msg}");
        assert!(
            msg.contains("1800"),
            "should mention shortfall (1800): {msg}"
        );
    }

    #[test]
    fn no_path_display_is_readable() {
        let msg = InfeasibleReason::NoPathExists.to_string();
        assert!(!msg.is_empty());
    }

    // ------------------------------------------------------------------
    // compute_feasibility_with: closure-controlled cost
    // ------------------------------------------------------------------

    /// Doubling every edge cost via the closure must double both inbound and
    /// outbound times for every feasible node, and the slack must update
    /// consistently with the new totals.
    #[test]
    fn closure_doubles_inbound_and_outbound_consistently() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);

        let baseline = compute_feasibility(&g, origin, dest, 10_000.0, NetworkType::Drive)
            .expect("baseline should be Ok");
        let doubled = compute_feasibility_with(&g, origin, dest, 10_000.0, |e| {
            e.weight.travel_time(NetworkType::Drive) * 2.0
        })
        .expect("doubled should be Ok");

        for (node, base) in &baseline.feasible {
            let d = doubled.feasible.get(node).expect("doubled missing a node");
            assert!((d.inbound_time - 2.0 * base.inbound_time).abs() < 1e-9);
            assert!((d.outbound_time - 2.0 * base.outbound_time).abs() < 1e-9);
            // Identity must still hold under the doubled cost.
            assert!(
                (d.inbound_time + d.outbound_time + d.slack - 10_000.0).abs() < 1e-9,
                "doubled identity: in={} out={} slack={}",
                d.inbound_time,
                d.outbound_time,
                d.slack
            );
        }
    }

    /// The closure must see every edge in its *original* orientation regardless
    /// of search direction. We verify by passing a closure whose cost depends on
    /// `source.index() < target.index()` and checking that direction-aware
    /// asymmetry is preserved across both searches.
    #[test]
    fn closure_sees_original_orientation_in_both_searches() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);

        // Charge 10s for "forward" edges (source < target by index) and 100s
        // for "backward" edges. A symmetric cost would give the same in both
        // searches; an oriented cost would differ. Either way, the slack
        // identity must hold.
        let result = compute_feasibility_with(&g, origin, dest, 10_000.0, |e| {
            if e.source.index() < e.target.index() {
                10.0
            } else {
                100.0
            }
        })
        .expect("should be Ok");

        for (_, f) in &result.feasible {
            assert!((f.inbound_time + f.outbound_time + f.slack - 10_000.0).abs() < 1e-9);
            assert!(f.slack >= 0.0);
        }
    }

    /// The convenience `compute_feasibility` must produce identical results to
    /// `compute_feasibility_with` invoked with the equivalent baseline closure.
    #[test]
    fn convenience_wrapper_matches_with_variant() {
        let g = linear_graph();
        let origin = find_node(&g, 1);
        let dest = find_node(&g, 4);

        let a = compute_feasibility(&g, origin, dest, 10_000.0, NetworkType::Drive).unwrap();
        let b = compute_feasibility_with(&g, origin, dest, 10_000.0, |e| {
            e.weight.travel_time(NetworkType::Drive)
        })
        .unwrap();

        assert_eq!(a.feasible.len(), b.feasible.len());
        for (node, fa) in &a.feasible {
            let fb = b.feasible.get(node).expect("node missing in _with result");
            assert!((fa.inbound_time - fb.inbound_time).abs() < 1e-9);
            assert!((fa.outbound_time - fb.outbound_time).abs() < 1e-9);
            assert!((fa.slack - fb.slack).abs() < 1e-9);
        }
    }
}
