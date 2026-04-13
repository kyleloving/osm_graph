import pysochrone
import json
import folium
import time


def isochrone_example():
    """Compare simplified vs unsimplified isochrones from a lat/lon point."""
    lat, lon = 48.123456, 11.123456  # Munich, Germany
    max_dist = 10_000
    time_limits = [300, 600, 900]
    network_type = "Drive"
    hull_type = "Concave"

    t0 = time.time()
    unsimplified = pysochrone.calc_isochrones(lat, lon, time_limits, network_type, hull_type, retain_all=True)
    print(f"Unsimplified: {time.time() - t0:.2f}s")

    t0 = time.time()
    simplified = pysochrone.calc_isochrones(lat, lon, time_limits, network_type, hull_type)
    print(f"Simplified:   {time.time() - t0:.2f}s")

    m = folium.Map(location=[lat, lon], zoom_start=13, tiles="Cartodb Positron")
    folium.Marker(location=[lat, lon], tooltip="Origin").add_to(m)

    for i, geojson_str in enumerate(unsimplified):
        folium.GeoJson(
            json.loads(geojson_str),
            name=f'Unsimplified: {(i+1)*5} min',
            style_function=lambda _, c='blue': {'fillColor': c, 'color': c, 'weight': 2, 'fillOpacity': 0.15}
        ).add_to(m)

    for i, geojson_str in enumerate(simplified):
        folium.GeoJson(
            json.loads(geojson_str),
            name=f'Simplified: {(i+1)*5} min',
            style_function=lambda _, c='red': {'fillColor': c, 'color': c, 'weight': 2, 'fillOpacity': 0.15}
        ).add_to(m)

    folium.LayerControl().add_to(m)
    m.save('isochrone_comparison.html')
    print("Saved to isochrone_comparison.html")


def geocoding_example():
    """Geocode a place name and generate isochrones from it."""
    place = "Marienplatz, Munich, Germany"
    print(f"Geocoding '{place}'...")

    t0 = time.time()
    lat, lon = pysochrone.geocode(place)
    print(f"  -> ({lat:.6f}, {lon:.6f}) in {time.time() - t0:.2f}s")

    isochrones = pysochrone.calc_isochrones(lat, lon, [300, 600, 900], "Walk", "Concave")

    m = folium.Map(location=[lat, lon], zoom_start=14, tiles="Cartodb Positron")
    folium.Marker(location=[lat, lon], tooltip=place).add_to(m)

    colors = ['green', 'orange', 'red']
    labels = ['5 min walk', '10 min walk', '15 min walk']
    for i, geojson_str in enumerate(isochrones):
        c = colors[i]
        folium.GeoJson(
            json.loads(geojson_str),
            name=labels[i],
            style_function=lambda _, c=c: {'fillColor': c, 'color': c, 'weight': 2, 'fillOpacity': 0.2}
        ).add_to(m)

    folium.LayerControl().add_to(m)
    m.save('geocoding_example.html')
    print("Saved to geocoding_example.html")


def routing_example():
    """Route between two geocoded locations."""
    origin_place = "Marienplatz, Munich, Germany"
    dest_place = "English Garden, Munich, Germany"

    print(f"Geocoding origin and destination...")
    origin = pysochrone.geocode(origin_place)
    dest = pysochrone.geocode(dest_place)
    print(f"  Origin: {origin}")
    print(f"  Dest:   {dest}")

    t0 = time.time()
    route_geojson = pysochrone.calc_route(
        origin[0], origin[1],
        dest[0], dest[1],
        "Drive"
    )
    elapsed = time.time() - t0
    route = json.loads(route_geojson)
    props = route['properties']
    print(f"  Distance: {props['distance_m']:.0f}m, Duration: {props['duration_s']:.0f}s ({elapsed:.2f}s compute)")

    m = folium.Map(location=[origin[0], origin[1]], zoom_start=13, tiles="Cartodb Positron")
    folium.Marker(location=list(origin), tooltip=origin_place, icon=folium.Icon(color='green')).add_to(m)
    folium.Marker(location=list(dest), tooltip=dest_place, icon=folium.Icon(color='red')).add_to(m)
    folium.GeoJson(route, name='Route', style_function=lambda _: {'color': 'blue', 'weight': 4}).add_to(m)

    folium.LayerControl().add_to(m)
    m.save('routing_example.html')
    print("Saved to routing_example.html")


if __name__ == "__main__":
    print("=== Isochrone comparison ===")
    isochrone_example()

    print("\n=== Geocoding + isochrones ===")
    geocoding_example()

    print("\n=== Routing ===")
    routing_example()
