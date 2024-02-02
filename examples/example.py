import pysochrone
import json
import folium
import time 

def main():
    # Example coordinates and parameters
    lat, lon = 48.123456, 11.123456  # Munich, Germany
    max_dist = 10_000  # Maximum distance in meters for the bounding box
    time_limits = [300, 600, 900, 1200, 1500, 1800]  # Time limits in seconds
    network_type = "Drive" # Network Type: "Drive" (Others WIP)
    hull_type = "Convex"  # Hull type: "Convex" or "Concave"

    # Calculate isochrones
    for i in range(3):
        print(f'Run #{i+1}')
        print('------------')
        isochrones_geojson = pysochrone.calc_isochrones(lat, lon, max_dist, time_limits, network_type, hull_type)
        print('------------')

    # Initialize a Folium map
    m = folium.Map(location=[lat, lon], zoom_start=13)

    folium.Marker(location=[lat, lon]).add_to(m)

    # Add each isochrone as a GeoJSON layer to the map
    for i, geojson_str in enumerate(isochrones_geojson):
        geojson = json.loads(geojson_str)
        folium.GeoJson(
            geojson,
            name=f'Isochrone: {(i+1)*5} Minutes',
            style_function= lambda _ :{
                'fillColor': 'red',
                'color': 'red',
                'weight': 2,
                'fillOpacity': 0.2
            }
        ).add_to(m)

    # Add layer control and display the map
    folium.LayerControl().add_to(m)
    m.show_in_browser()

if __name__ == "__main__":
    main()
