# OSM_graph
*Quickly generate isochrones for Python and Rust!*

This library provides a set of tools for generating isochrones and reverse isochrones from geographic coordinates. It leverages OpenStreetMap data to construct road networks and calculate areas accessible within specified time limits. The library is designed for both Rust and Python, offering high performance and easy integration into data science workflows.

![Isochrones](image.png)

## Features
- Graph Construction: Parses OpenStreetMap data to construct a graph representing the road network.
- Isochrone Calculation: Generates isochrones, areas reachable within a given time frame from a start point, using Dijkstra's algorithm.
- Reverse Isochrone Calculation: Determines areas from which a point can be reached within a given time frame.
- Concave and Convex Hulls: Supports generating both concave and convex hulls around isochrones for more accurate or simplified geographical shapes.
- Caching: Implements caching mechanisms to store and retrieve pre-calculated graphs for faster access.
Python Integration: Offers Python bindings to use the library's functionalities directly in Python scripts, notebooks, and applications.
- Concurrency Support: Utilizes Rust's concurrency features for efficient isochrone calculation over large datasets.
- GeoJSON Output: Converts isochrones into GeoJSON format for easy visualization and integration with mapping tools.

## Installation
To use the library in Rust, add it to your Cargo.toml:

```toml
[dependencies]
osm-graph = "0.1.0"
```

For Python, 

```bash
pip install pysochrone
```

Or, ensure you have Rust and maturin installed, then build and install the Python package:

```bash
maturin develop
```

## Usage
Rust
```rust
use osm_graph::{calculate_isochrones_from_point, HullType};

async fn main() {
    let isochrones = calculate_isochrones_from_point(
        48.123456, 11.123456, 5000.0, vec![600.0, 1200.0, 1800.0], HullType::Convex
    ).await.unwrap();
    
    // Process isochrones...
}
```

Python

```python
import pysochrone

isochrones = pysochrone.calc_isochrones(48.123456, 11.123456, 5000, [600, 1200, 1800], "Convex")
print(isochrones)
```

## Roadmap
- Customizable Speed Limits: Allow users to specify custom speed limits for different road types.
- Support for Pedestrian and Bicycle Networks: Expand the graph construction to support pedestrian and bicycle network types.
- Additional Roadnetwork analytics.
- Advanced Caching Strategies: Implement more sophisticated caching mechanisms for dynamic query parameters.
- Interactive Visualization Tools: Develop a set of tools for interactive visualization of isochrones in web applications.
- API Integration: Provide integration options with third-party APIs for enhanced data accuracy and features.
- Optimization and Parallel Computing: Further optimize the graph algorithms and explore parallel computing options for large-scale data.

## Contributing
Contributions are welcome! Please submit pull requests, open issues for discussion, and suggest new features or improvements.

## License
This library is licensed under MIT License.