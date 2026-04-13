import time
import statistics
import osmnx as ox
import networkx as nx
import pysochrone

lat, lon = 48.1351, 11.5820  # Munich
radii = [5000, 10000, 20000]
time_limits = [300, 600, 900]
NUM_RUNS = 5

print("=== Warming caches ===")
graph_sizes = {}
for radius in radii:
    print(f"  Warming r={radius}m...")
    pysochrone.calc_isochrones(lat, lon, time_limits, "Drive", "Concave", max_dist=radius)
    G = ox.graph_from_point((lat, lon), dist=radius, network_type="drive")
    graph_sizes[radius] = (G.number_of_nodes(), G.number_of_edges())
    print(f"    Graph: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges")

print(f"\n=== Benchmarking (median of {NUM_RUNS} runs, cache-only) ===")
results = []

for radius in radii:
    pysochrone_times = []
    osmnx_times = []

    for run in range(NUM_RUNS):
        t0 = time.time()
        pysochrone.calc_isochrones(
            lat, lon, time_limits, "Drive", "Concave", max_dist=radius
        )
        pysochrone_times.append(time.time() - t0)

        t0 = time.time()
        G = ox.graph_from_point((lat, lon), dist=radius, network_type="drive")
        G = ox.add_edge_speeds(G)
        G = ox.add_edge_travel_times(G)
        center = ox.nearest_nodes(G, lon, lat)
        for limit in time_limits:
            subgraph = nx.ego_graph(G, center, radius=limit, distance="travel_time")
            nodes_gdf = ox.graph_to_gdfs(subgraph, edges=False)
            nodes_gdf.union_all().convex_hull
        osmnx_times.append(time.time() - t0)

    ps_median = statistics.median(pysochrone_times)
    ox_median = statistics.median(osmnx_times)
    speedup = ox_median / ps_median if ps_median > 0 else float("inf")

    results.append({
        "radius": radius,
        "pysochrone_median": ps_median,
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