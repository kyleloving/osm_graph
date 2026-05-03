import time
import statistics
import osmnx as ox
import networkx as nx
import pysochrone
from shapely.geometry import MultiPoint  # transitive dep of osmnx/geopandas

lat, lon = 48.1351, 11.5820  # Munich
radii = [5000, 10000, 20000]
time_limits = [300, 600, 900]
NUM_RUNS = 5

# ---------------------------------------------------------------------------
# Warm caches and pre-enrich osmnx graphs
# Graph construction and edge enrichment are one-time costs — we measure
# steady-state compute only, which is the fairest apples-to-apples comparison.
# ---------------------------------------------------------------------------
print("=== Warming caches ===")
ox_graphs = {}
graph_sizes = {}
for radius in radii:
    print(f"  Warming r={radius}m...")
    pysochrone.calc_isochrones(lat, lon, time_limits, "Drive", "Concave", max_dist=radius)
    G = ox.graph_from_point((lat, lon), dist=radius, network_type="drive")
    G = ox.add_edge_speeds(G)
    G = ox.add_edge_travel_times(G)
    center = ox.nearest_nodes(G, lon, lat)
    ox_graphs[radius] = (G, center)
    graph_sizes[radius] = (G.number_of_nodes(), G.number_of_edges())
    print(f"    Graph: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges")

# ---------------------------------------------------------------------------
# Benchmark: pysochrone vs osmnx
# osmnx: single Dijkstra pass + direct coordinate access (no GeoDataFrame)
# ---------------------------------------------------------------------------
print(f"\n=== Benchmarking (median of {NUM_RUNS} runs, cache-only) ===")
results = []

for radius in radii:
    pysochrone_times = []
    osmnx_times = []
    G, center = ox_graphs[radius]

    for _ in range(NUM_RUNS):
        t0 = time.time()
        pysochrone.calc_isochrones(
            lat, lon, time_limits, "Drive", "Concave", max_dist=radius
        )
        pysochrone_times.append(time.time() - t0)

        t0 = time.time()
        lengths = nx.single_source_dijkstra_path_length(G, center, weight="travel_time")
        for limit in time_limits:
            reachable = [n for n, t in lengths.items() if t <= limit]
            coords = [(G.nodes[n]["x"], G.nodes[n]["y"]) for n in reachable]
            MultiPoint(coords).convex_hull
        osmnx_times.append(time.time() - t0)

    ps_median = statistics.median(pysochrone_times)
    ox_median = statistics.median(osmnx_times)
    speedup = ox_median / ps_median if ps_median > 0 else float("inf")

    results.append({
        "radius": radius,
        "pysochrone_times": pysochrone_times,
        "pysochrone_median": ps_median,
        "osmnx_times": osmnx_times,
        "osmnx_median": ox_median,
        "speedup": speedup,
    })
    print(f"r={radius:>5}m: pysochrone={ps_median:.3f}s, osmnx={ox_median:.3f}s, speedup={speedup:.1f}x")

print("\n=== Summary ===")
print(f"{'Radius':>8} {'Nodes':>8} {'Edges':>8} {'pysochrone':>12} {'osmnx':>12} {'Speedup':>10}")
print("-" * 62)
for r in results:
    nodes, edges = graph_sizes[r["radius"]]
    print(
        f"{r['radius']:>7}m {nodes:>8} {edges:>8} "
        f"{r['pysochrone_median']:>11.3f}s {r['osmnx_median']:>11.3f}s "
        f"{r['speedup']:>9.1f}x"
    )

# ---------------------------------------------------------------------------
# Chart
# ---------------------------------------------------------------------------
try:
    import matplotlib.pyplot as plt
    import numpy as np
    import os

    labels = [f"{r['radius'] // 1000}km" for r in results]
    x = np.arange(len(labels))
    width = 0.35

    def err_bars(times):
        med = statistics.median(times)
        return med - min(times), max(times) - med

    ps_med = [r["pysochrone_median"] for r in results]
    ox_med = [r["osmnx_median"]      for r in results]
    ps_err = np.array([err_bars(r["pysochrone_times"]) for r in results]).T
    ox_err = np.array([err_bars(r["osmnx_times"])      for r in results]).T
    speedups = [r["speedup"] for r in results]

    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(11, 4.5))
    fig.suptitle("pysochrone vs osmnx — compute time (cached graph, no network)", fontsize=12)

    # Left: grouped bars with min/max error bars, log scale
    ax1.bar(x - width / 2, ox_med, width, label="osmnx",      color="#d95f02", alpha=0.85)
    ax1.bar(x + width / 2, ps_med, width, label="pysochrone", color="#1b9e77", alpha=0.85)
    ax1.errorbar(x - width / 2, ox_med, yerr=ox_err, fmt="none", color="black", capsize=4, linewidth=1)
    ax1.errorbar(x + width / 2, ps_med, yerr=ps_err, fmt="none", color="black", capsize=4, linewidth=1)
    ax1.set_yscale("log")
    ax1.set_ylabel("Time (s, log scale)")
    ax1.set_xticks(x)
    ax1.set_xticklabels(labels)
    ax1.legend()
    ax1.set_title("Compute time by radius (median ± min/max)")

    # Right: speedup bars
    ax2.bar(labels, speedups, color="#7570b3", alpha=0.85)
    for xi, s in enumerate(speedups):
        ax2.text(xi, s + max(speedups) * 0.02, f"{s:.1f}×",
                 ha="center", va="bottom", fontsize=10)
    ax2.set_ylabel("Speedup (×)")
    ax2.set_title("Speedup factor by radius")
    ax2.set_ylim(0, max(speedups) * 1.2)

    fig.tight_layout()
    out_path = os.path.join(os.path.dirname(__file__), "performance.png")
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    print(f"\nChart saved to {out_path}")
except ImportError:
    print("\n(matplotlib not installed — skipping chart generation)")
