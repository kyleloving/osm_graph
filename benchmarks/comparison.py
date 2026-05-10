"""
graphways benchmark suite

Sections:
  1. Headline: graphways vs NetworkX on an OSMnx graph
  2. Staged: one-time graph lookup/build cost vs per-query cost
  3. Optional: r5py comparison (requires --pbf path + r5py installed)

Run:
    python benchmarks/comparison.py
    python benchmarks/comparison.py --pbf /path/to/extract.osm.pbf
    python benchmarks/comparison.py --skip-r5py
"""

from __future__ import annotations

import argparse
import math
import os
from pathlib import Path
import statistics
import time
from importlib.metadata import version

import graphways as gw
import networkx as nx
import osmnx as ox
from shapely.geometry import MultiPoint

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

PLACE = "Munich, Germany"
LAT, LON = 48.1351, 11.5820
NETWORK = "drive"
RADII = [5_000, 10_000, 20_000]
TIME_LIMITS_S = [300, 600, 900]
NUM_RUNS = 5

OSM_FILTERS = {
    "drive": (
        '["highway"]["area"!~"yes"]'
        '["highway"!~"abandoned|bridleway|bus_guideway|construction|corridor|cycleway|elevator|escalator|footway|no|path|pedestrian|planned|platform|proposed|raceway|razed|service|steps|track"]'
        '["motor_vehicle"!~"no"]["motorcar"!~"no"]'
        '["service"!~"alley|driveway|emergency_access|parking|parking_aisle|private"]'
    ),
    "walk": (
        '["highway"]["area"!~"yes"]'
        '["highway"!~"abandoned|bus_guideway|construction|corridor|elevator|escalator|motor|no|planned|platform|proposed|raceway|razed"]'
        '["foot"!~"no"]["service"!~"private"]'
    ),
    "bike": (
        '["highway"]["area"!~"yes"]'
        '["highway"!~"abandoned|bus_guideway|construction|corridor|elevator|escalator|footway|motor|no|planned|platform|proposed|raceway|razed|steps"]'
        '["bicycle"!~"no"]["service"!~"private"]'
    ),
}


def median_minmax(times: list[float]) -> tuple[float, float, float]:
    med = statistics.median(times)
    return med, min(times), max(times)


def err_bars(times: list[float]) -> tuple[float, float]:
    med, lo, hi = median_minmax(times)
    return med - lo, hi - med


def fnv1a(text: str) -> int:
    """Stable FNV-1a hash used by graphways for XML cache filenames."""
    hash_value = 14695981039346656037
    for byte in text.encode("utf-8"):
        hash_value ^= byte
        hash_value = (hash_value * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    return hash_value


def bbox_from_point(lat: float, lon: float, dist_m: float) -> str:
    earth_radius_m = 6_371_009.0
    delta_lat = (dist_m / earth_radius_m) * (180.0 / math.pi)
    delta_lon = delta_lat / math.cos(lat * math.pi / 180.0)
    south = lat - delta_lat
    west = lon - delta_lon
    north = lat + delta_lat
    east = lon + delta_lon
    return f"{south},{west},{north},{east}"


def overpass_query(radius_m: int) -> str:
    polygon_coord_str = bbox_from_point(LAT, LON, radius_m)
    osm_filter = OSM_FILTERS[NETWORK]
    return f"[out:xml][timeout:50];(way{osm_filter}({polygon_coord_str});>;);out;"


def cached_xml_path(radius_m: int) -> Path:
    query = overpass_query(radius_m)
    return Path(gw.cache_dir()) / f"{fnv1a(query):016x}.xml"


def load_cached_xml(radius_m: int) -> str | None:
    path = cached_xml_path(radius_m)
    if path.exists():
        return path.read_text(encoding="utf-8")
    return None


def convex_hulls_for_limits(G, lengths: dict, limits_s: list[int]) -> None:
    """Compute one cheap NetworkX/OSMnx baseline hull per time limit."""
    for limit in limits_s:
        coords = [
            (G.nodes[node]["x"], G.nodes[node]["y"])
            for node, travel_time in lengths.items()
            if travel_time <= limit
        ]
        if len(coords) >= 3:
            MultiPoint(coords).convex_hull


def graphways_graph(radius_m: int):
    if cached_xml := load_cached_xml(radius_m):
        return gw.SpatialGraph.from_osm(cached_xml, network=NETWORK)

    # Online fallback for first-time users. Benchmark output clearly labels the
    # warm-up phase, and timed query loops only use the returned graph object.
    return gw.SpatialGraph.from_place(PLACE, network=NETWORK, max_dist=radius_m)


# ===========================================================================
# 1. Headline benchmark: graphways vs NetworkX steady-state
# ===========================================================================


def headline_benchmark():
    print("=" * 72)
    print("1. graphways vs NetworkX (steady state, cache-only)")
    print("=" * 72)
    print(
        f"graphways {version('graphways')} from {gw.__file__}\n"
        f"osmnx {version('osmnx')}, networkx {version('networkx')}"
    )
    print()
    print("Method:")
    print("  - Graph construction and edge enrichment are pre-warmed.")
    print("  - Timers measure repeated query work on already-built graphs.")
    print("  - graphways computes reachability and triangulated contours.")
    print("  - NetworkX computes single-source Dijkstra on an OSMnx graph, then")
    print("    builds convex hulls from reachable nodes as a cheap geometry baseline.")
    print("  - This is not identical polygon quality; it is a practical baseline for")
    print("    the core repeated-query workload.")
    print()

    print("--- Warming caches ---")
    ox_graphs = {}
    gw_graphs = {}
    sizes = {}

    for radius in RADII:
        print(f"  r={radius:>5}m...")
        graph = graphways_graph(radius)
        G = ox.graph_from_point((LAT, LON), dist=radius, network_type=NETWORK)
        G = ox.add_edge_speeds(G)
        G = ox.add_edge_travel_times(G)
        center = ox.nearest_nodes(G, LON, LAT)

        gw_graphs[radius] = graph
        ox_graphs[radius] = (G, center)
        sizes[radius] = {
            "graphways": (graph.node_count(), graph.edge_count()),
            "networkx": (G.number_of_nodes(), G.number_of_edges()),
        }

        gn, ge = sizes[radius]["graphways"]
        on, oe = sizes[radius]["networkx"]
        print(
            f"    graphways {gn:>7} nodes / {ge:>7} edges | "
            f"networkx {on:>7} nodes / {oe:>7} edges"
        )

    print(f"\n--- Running ({NUM_RUNS} runs each) ---")
    results = []

    for radius in RADII:
        G, center = ox_graphs[radius]
        graph = gw_graphs[radius]
        gw_times = []
        nx_times = []

        for _ in range(NUM_RUNS):
            t0 = time.perf_counter()
            graph.isochrone((LAT, LON), minutes=[t / 60 for t in TIME_LIMITS_S])
            gw_times.append(time.perf_counter() - t0)

            t0 = time.perf_counter()
            lengths = nx.single_source_dijkstra_path_length(
                G, center, weight="travel_time"
            )
            convex_hulls_for_limits(G, lengths, TIME_LIMITS_S)
            nx_times.append(time.perf_counter() - t0)

        gw_med, _, _ = median_minmax(gw_times)
        nx_med, _, _ = median_minmax(nx_times)
        speedup = nx_med / gw_med if gw_med > 0 else float("inf")

        results.append(
            {
                "radius": radius,
                "gw_times": gw_times,
                "gw_median": gw_med,
                "nx_times": nx_times,
                "nx_median": nx_med,
                "speedup": speedup,
            }
        )
        print(
            f"  r={radius:>5}m  graphways={gw_med:.3f}s  "
            f"networkx={nx_med:.3f}s  ({speedup:.1f}x)"
        )

    print("\n--- Summary ---")
    print(
        f"{'Radius':>8} {'Nodes':>8} {'Edges':>8} "
        f"{'graphways':>12} {'networkx':>12} {'Speedup':>10}"
    )
    print("-" * 66)
    for row in results:
        nodes, edges = sizes[row["radius"]]["networkx"]
        print(
            f"{row['radius']:>7}m {nodes:>8} {edges:>8} "
            f"{row['gw_median']:>11.3f}s {row['nx_median']:>11.3f}s "
            f"{row['speedup']:>9.1f}x"
        )
    print()
    return results, sizes


# ===========================================================================
# 2. Staged benchmark: one-time graph lookup/build vs per-query cost
# ===========================================================================


def staged_benchmark():
    print("=" * 72)
    print("2. graphways one-time vs per-query costs (cached)")
    print("=" * 72)
    print()
    print("This separates cached graph construction/lookup from repeated")
    print("isochrone queries on an already-built graph. The first uncached build")
    print("may include geocoding and Overpass I/O and is intentionally not measured.")
    print()

    radius = 10_000

    # Pre-warm disk and in-memory caches.
    graphways_graph(radius)

    warm_build_times = []
    for _ in range(NUM_RUNS):
        t0 = time.perf_counter()
        graphways_graph(radius)
        warm_build_times.append(time.perf_counter() - t0)

    graph = graphways_graph(radius)
    query_times = []
    for _ in range(NUM_RUNS):
        t0 = time.perf_counter()
        graph.isochrone((LAT, LON), minutes=[t / 60 for t in TIME_LIMITS_S])
        query_times.append(time.perf_counter() - t0)

    warm_med, warm_min, warm_max = median_minmax(warm_build_times)
    query_med, query_min, query_max = median_minmax(query_times)

    print(f"--- r={radius}m, median of {NUM_RUNS} runs ---")
    print(
        "  cached SpatialGraph construction:    "
        f"{warm_med:.4f}s  (min {warm_min:.4f}, max {warm_max:.4f})"
    )
    print(
        f"  graph.isochrone() full pipeline:      "
        f"{query_med:.4f}s  (min {query_min:.4f}, max {query_max:.4f})"
    )
    print()
    print("For deeper staging, the Python API would need stable lower-level timing")
    print("entry points for traversal, contour extraction, and serialization.")
    print()

    return {
        "radius": radius,
        "warm_build_times": warm_build_times,
        "query_times": query_times,
    }


# ===========================================================================
# 3. Optional r5py comparison
# ===========================================================================


def r5py_benchmark(pbf_path: str | None):
    try:
        import geopandas as gpd
        import numpy as np
        import r5py
        from shapely.geometry import Point
    except ImportError as exc:
        print("=" * 72)
        print("3. r5py comparison - SKIPPED (r5py/geopandas not installed)")
        print("=" * 72)
        print(f"Import error: {exc}")
        print("Install optional dependencies and pass --pbf to enable.")
        print()
        return None
    except RuntimeError as exc:
        print("=" * 72)
        print("3. r5py comparison - SKIPPED (JVM failed to start)")
        print("=" * 72)
        print(f"JVM error: {exc}")
        print(
            "r5py requires a compatible Java JDK. Check `java -version` and JAVA_HOME."
        )
        print()
        return None

    if not pbf_path or not os.path.exists(pbf_path):
        print("=" * 72)
        print("3. r5py comparison - SKIPPED (no PBF file)")
        print("=" * 72)
        print("Pass --pbf /path/to/extract.osm.pbf to enable.")
        print()
        return None

    print("=" * 72)
    print("3. graphways vs r5py")
    print("=" * 72)
    print(f"r5py {version('r5py')}  PBF: {pbf_path}")
    print()
    print("r5py naturally computes travel-time matrices. This comparison samples")
    print("a destination grid around the origin, thresholds matrix results, and")
    print("builds convex hulls. It is useful context, not an identical workload.")
    print()

    radius = 10_000
    grid_step_m = 200

    print("--- Building r5py TransportNetwork ---")
    t0 = time.perf_counter()
    network = r5py.TransportNetwork(osm_pbf=pbf_path)
    r5_build = time.perf_counter() - t0
    print(f"  {r5_build:.2f}s")

    deg_per_m_lat = 1.0 / 111_320.0
    deg_per_m_lon = 1.0 / (111_320.0 * np.cos(np.radians(LAT)))
    half_lat = radius * deg_per_m_lat
    half_lon = radius * deg_per_m_lon
    step_lat = grid_step_m * deg_per_m_lat
    step_lon = grid_step_m * deg_per_m_lon

    lats = np.arange(LAT - half_lat, LAT + half_lat, step_lat)
    lons = np.arange(LON - half_lon, LON + half_lon, step_lon)
    grid_pts = [(la, lo) for la in lats for lo in lons]

    origins = gpd.GeoDataFrame(
        {"id": ["origin"]},
        geometry=[Point(LON, LAT)],
        crs="EPSG:4326",
    )
    destinations = gpd.GeoDataFrame(
        {"id": [f"d{i}" for i in range(len(grid_pts))]},
        geometry=[Point(lo, la) for la, lo in grid_pts],
        crs="EPSG:4326",
    )
    destination_lookup = destinations.set_index("id")
    print(f"  {len(grid_pts)} destination cells ({grid_step_m}m grid)")

    graph = graphways_graph(radius)
    gw_times = []
    for _ in range(NUM_RUNS):
        t0 = time.perf_counter()
        graph.isochrone((LAT, LON), minutes=[t / 60 for t in TIME_LIMITS_S])
        gw_times.append(time.perf_counter() - t0)

    r5_times = []
    for _ in range(NUM_RUNS):
        t0 = time.perf_counter()
        computer = r5py.TravelTimeMatrix(
            network,
            origins=origins,
            destinations=destinations,
            transport_modes=[r5py.TransportMode.CAR],
        )
        matrix = computer.compute_travel_times()
        for limit_s in TIME_LIMITS_S:
            reachable = matrix[matrix["travel_time"] <= limit_s / 60]
            if len(reachable) >= 3:
                pts = destination_lookup.loc[reachable["to_id"]]
                MultiPoint(list(pts.geometry)).convex_hull
        r5_times.append(time.perf_counter() - t0)

    gw_med, _, _ = median_minmax(gw_times)
    r5_med, _, _ = median_minmax(r5_times)

    print(f"\n--- Per-query (r={radius}m, median of {NUM_RUNS}) ---")
    print(f"  graphways:  {gw_med:.3f}s  (triangulated contours)")
    print(f"  r5py:       {r5_med:.3f}s  ({len(grid_pts)}-cell matrix + hull)")
    print()
    print("Use r5py for transit/multi-modal routing and matrix workloads.")
    print("Use graphways for fast local graph reachability and isochrone polygons.")
    print()

    return {
        "build": r5_build,
        "gw_median": gw_med,
        "r5_median": r5_med,
        "n_destinations": len(grid_pts),
    }


# ===========================================================================
# Chart
# ===========================================================================


def make_chart(headline_results, out_path: str) -> None:
    try:
        import matplotlib

        matplotlib.use("Agg", force=True)
        import matplotlib.pyplot as plt
        import numpy as np
    except ImportError:
        print("(matplotlib not installed - skipping chart)")
        return

    labels = [f"{row['radius'] // 1000}km" for row in headline_results]
    x = np.arange(len(labels))
    width = 0.35

    gw_med = [row["gw_median"] for row in headline_results]
    nx_med = [row["nx_median"] for row in headline_results]
    gw_err = np.array([err_bars(row["gw_times"]) for row in headline_results]).T
    nx_err = np.array([err_bars(row["nx_times"]) for row in headline_results]).T
    speedups = [row["speedup"] for row in headline_results]

    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(11, 4.5))
    fig.suptitle(
        "graphways vs NetworkX - compute time (cached graph, no network)",
        fontsize=12,
    )

    ax1.bar(x - width / 2, nx_med, width, label="networkx", color="#d95f02", alpha=0.85)
    ax1.bar(
        x + width / 2, gw_med, width, label="graphways", color="#1b9e77", alpha=0.85
    )
    ax1.errorbar(
        x - width / 2,
        nx_med,
        yerr=nx_err,
        fmt="none",
        color="black",
        capsize=4,
        linewidth=1,
    )
    ax1.errorbar(
        x + width / 2,
        gw_med,
        yerr=gw_err,
        fmt="none",
        color="black",
        capsize=4,
        linewidth=1,
    )
    ax1.set_yscale("log")
    ax1.set_ylabel("Time (s, log scale)")
    ax1.set_xticks(x)
    ax1.set_xticklabels(labels)
    ax1.legend()
    ax1.set_title("Compute time by radius (median +/- min/max)")

    ax2.bar(labels, speedups, color="#7570b3", alpha=0.85)
    for idx, speedup in enumerate(speedups):
        ax2.text(
            idx,
            speedup + max(speedups) * 0.02,
            f"{speedup:.1f}x",
            ha="center",
            va="bottom",
            fontsize=10,
        )
    ax2.set_ylabel("Speedup (x)")
    ax2.set_title("Speedup factor by radius")
    ax2.set_ylim(0, max(speedups) * 1.2)

    fig.tight_layout()
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    print(f"Chart saved to {out_path}")


# ===========================================================================
# Main
# ===========================================================================


def main() -> None:
    parser = argparse.ArgumentParser(description="graphways benchmark suite")
    parser.add_argument("--pbf", help="OSM PBF path for optional r5py comparison")
    parser.add_argument("--skip-r5py", action="store_true", help="Skip r5py section")
    args = parser.parse_args()

    headline_results, _ = headline_benchmark()
    staged_benchmark()

    out_path = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "performance.png"
    )
    make_chart(headline_results, out_path)

    if not args.skip_r5py:
        r5py_benchmark(args.pbf)


if __name__ == "__main__":
    main()
