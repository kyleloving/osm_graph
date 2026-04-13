import pysochrone
import json
import folium
import time

def main():
    lat, lon = 48.123456, 11.123456  # Munich, Germany
    max_dist = 10_000
    time_limits = [300, 600, 900, 1200, 1500, 1800]
    network_type = "Drive"
    hull_type = "Concave"

    t0 = time.time()
    unsimplified = pysochrone.calc_isochrones(lat, lon, max_dist, time_limits, network_type, hull_type, True)
    print(f"Unsimplified: {time.time() - t0:.2f}s")

    t0 = time.time()
    simplified = pysochrone.calc_isochrones(lat, lon, max_dist, time_limits, network_type, hull_type)
    print(f"Simplified:   {time.time() - t0:.2f}s")

    m = folium.Map(location=[lat, lon], zoom_start=13)
    folium.Marker(location=[lat, lon]).add_to(m)

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

if __name__ == "__main__":
    main()
