import pysochrone
import json
import folium

def main():
    # Example coordinates and parameters
    lat, lon = 48.123456, 11.123456 # Munich, Germany
    max_dist = 10_000  # Maximum distance in meters for the bounding box
    time_limits = [300, 600, 900, 1200, 1500, 1800]  # Time limits in seconds
    network_type = "Drive" # Network Type: "Drive" (Others WIP)
    hull_type = "Concave"  # Hull type: "Convex" or "Concave" or "FastConcave"

    # Calculate isochrones
    isochrones_geojson = pysochrone.calc_isochrones(lat, lon, max_dist, time_limits, network_type, hull_type)

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
    m.save('isochrone.html')

if __name__ == "__main__":
    main()
