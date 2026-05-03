"""
Type stubs for pysochrone — the compiled Rust extension.

All GeoJSON return values are strings.  Parse them with ``json.loads``.
"""

from __future__ import annotations

# ---------------------------------------------------------------------------
# Graph object
# ---------------------------------------------------------------------------

class Graph:
    """
    A road-network graph loaded from OpenStreetMap.

    Obtain via :func:`build_graph`.  Reuse across multiple queries to avoid
    redundant cache lookups.

    Attributes are read-only; all mutations happen inside Rust.
    """

    def node_count(self) -> int:
        """Number of nodes in the graph."""
        ...

    def edge_count(self) -> int:
        """Number of directed edges in the graph."""
        ...

    def nearest_node(
        self, lat: float, lon: float
    ) -> tuple[int, float, float] | None:
        """
        Return ``(osm_id, lat, lon)`` for the node nearest to ``(lat, lon)``.

        Uses the internal R-tree spatial index — O(log n).
        Returns ``None`` if the graph is empty.
        """
        ...

    def isochrones(
        self,
        lat: float,
        lon: float,
        time_limits: list[float],
        hull_type: str = "Concave",
    ) -> list[str]:
        """
        Compute isochrones from ``(lat, lon)`` using this graph.

        Parameters
        ----------
        lat, lon:
            Origin coordinates.
        time_limits:
            Travel-time thresholds in **seconds**.
        hull_type:
            ``"Convex"`` | ``"FastConcave"`` | ``"Concave"``

        Returns
        -------
        list[str]
            One GeoJSON geometry string per time limit, in the same order
            as ``time_limits``.
        """
        ...

    def route(
        self,
        origin_lat: float,
        origin_lon: float,
        dest_lat: float,
        dest_lon: float,
    ) -> str:
        """
        Find the fastest route between two coordinates using A*.

        The network type (drive/walk/bike) is inherited from the ``Graph``.

        Returns
        -------
        str
            GeoJSON ``Feature`` (LineString) with properties:

            - ``distance_m`` (float) — total route distance in metres
            - ``duration_s`` (float) — total travel time in seconds
            - ``cumulative_times_s`` (list[float]) — elapsed time at each waypoint
        """
        ...

    def fetch_pois(self, isochrone_geojson: str) -> str:
        """
        Fetch OSM points of interest within a given isochrone polygon.

        Parameters
        ----------
        isochrone_geojson:
            A GeoJSON geometry string, e.g. from :meth:`isochrones`.

        Returns
        -------
        str
            GeoJSON ``FeatureCollection``; each feature is a POI ``Point``
            with raw OSM tags as properties.
        """
        ...

    def nodes_geojson(self) -> str:
        """
        All graph nodes as a GeoJSON ``FeatureCollection`` of ``Point`` features.

        Properties per feature: ``id``, ``lat``, ``lon``.
        """
        ...

    def edges_geojson(self) -> str:
        """
        All graph edges as a GeoJSON ``FeatureCollection`` of ``LineString`` features.

        Properties per feature: ``highway``, ``length_m``, ``speed_kph``,
        ``drive_time_s``, ``walk_time_s``, ``bike_time_s``.
        """
        ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# Module-level functions
# ---------------------------------------------------------------------------

def build_graph(
    lat: float,
    lon: float,
    network_type: str,
    max_dist: float | None = None,
    retain_all: bool = False,
) -> Graph:
    """
    Build and return a road-network :class:`Graph` for the area around ``(lat, lon)``.

    The graph is cached — repeated calls for the same area and network type
    return the in-memory graph with no network I/O.

    Parameters
    ----------
    lat, lon:
        Centre of the query area.
    network_type:
        ``"Drive"`` | ``"DriveService"`` | ``"Walk"`` | ``"Bike"`` |
        ``"All"`` | ``"AllPrivate"``
    max_dist:
        Bounding-box radius in metres.  Default: 5 000 m.
    retain_all:
        Skip topological simplification.  Preserves every OSM node and edge;
        slower for downstream computation.
    """
    ...

def calc_isochrones(
    lat: float,
    lon: float,
    time_limits: list[float],
    network_type: str,
    hull_type: str,
    max_dist: float | None = None,
    retain_all: bool = False,
) -> list[str]:
    """
    Compute isochrones from a single origin point.

    Parameters
    ----------
    lat, lon:
        Origin coordinates.
    time_limits:
        Travel-time thresholds in **seconds**.
    network_type:
        ``"Drive"`` | ``"DriveService"`` | ``"Walk"`` | ``"Bike"`` |
        ``"All"`` | ``"AllPrivate"``
    hull_type:
        ``"Convex"`` | ``"FastConcave"`` | ``"Concave"``
    max_dist:
        Bounding-box radius in metres.  When ``None``, auto-sized from the
        largest time limit.
    retain_all:
        Skip topological simplification.

    Returns
    -------
    list[str]
        One GeoJSON geometry string per time limit, in the same order as
        ``time_limits``.
    """
    ...

def calc_route(
    origin_lat: float,
    origin_lon: float,
    dest_lat: float,
    dest_lon: float,
    network_type: str,
    max_dist: float | None = None,
    retain_all: bool = False,
) -> str:
    """
    Find the fastest route between two coordinates using A*.

    Parameters
    ----------
    origin_lat, origin_lon:
        Origin coordinates.
    dest_lat, dest_lon:
        Destination coordinates.
    network_type:
        ``"Drive"`` | ``"DriveService"`` | ``"Walk"`` | ``"Bike"`` |
        ``"All"`` | ``"AllPrivate"``
    max_dist:
        Bounding-box radius.  When ``None``, uses
        ``max(5000, 1.5 × straight-line distance)``.
    retain_all:
        Skip topological simplification.

    Returns
    -------
    str
        GeoJSON ``Feature`` (LineString) with properties:

        - ``distance_m`` (float) — total route distance in metres
        - ``duration_s`` (float) — total travel time in seconds
        - ``cumulative_times_s`` (list[float]) — elapsed time at each waypoint,
          starting at ``0.0`` and ending at ``duration_s``
    """
    ...

def geocode(place: str) -> tuple[float, float]:
    """
    Convert a place name to ``(lat, lon)`` via the Nominatim API.

    Parameters
    ----------
    place:
        Any Nominatim-supported query string (e.g. ``"Marienplatz, Munich, Germany"``).

    Returns
    -------
    tuple[float, float]
        ``(latitude, longitude)``
    """
    ...

def fetch_pois(isochrone_geojson: str) -> str:
    """
    Fetch OSM points of interest that fall within a given isochrone polygon.

    Parameters
    ----------
    isochrone_geojson:
        A GeoJSON geometry string, e.g. from :func:`calc_isochrones`.

    Returns
    -------
    str
        GeoJSON ``FeatureCollection``; each feature is a POI ``Point`` with
        raw OSM tags as properties.
    """
    ...

def cache_dir() -> str:
    """
    Return the path to the on-disk XML cache directory.

    Override the default by setting the ``OSM_GRAPH_CACHE_DIR`` environment variable.
    """
    ...

def clear_cache() -> None:
    """
    Clear both the in-memory (graph and XML) caches and the on-disk XML cache.
    """
    ...
