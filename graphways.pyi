"""
Type stubs for graphways — the compiled Rust extension.

All GeoJSON return values are strings.  Parse them with ``json.loads``.
"""

from __future__ import annotations

# ---------------------------------------------------------------------------
# Graph object
# ---------------------------------------------------------------------------

class Reachability:
    """
    One-sided reachability field produced by ``SpatialGraph.reachable_from``.

    Isochrones are display projections of this result; the traversal itself is
    retained so callers can inspect nodes and timings without rebuilding the graph.
    """

    @property
    def max_time_s(self) -> float: ...

    def node_count(self) -> int: ...

    def travel_time_to_node_id(self, node_id: int) -> float | None: ...

    def nodes(self) -> list[dict[str, float | int]]:
        """
        Return reachable nodes with ``node_id``, ``lat``, ``lon``, and
        ``travel_time_s``.
        """
        ...

    def isochrones(self, time_limits: list[float]) -> list[str]:
        """
        Build one GeoJSON isochrone geometry per travel-time threshold.
        """
        ...

    def __repr__(self) -> str: ...

class BetweenReachability:
    """
    Two-sided reachability field produced by ``SpatialGraph.reachable_between``.

    Nodes in this result satisfy ``origin -> node -> destination`` within the
    traversal budget.
    """

    @property
    def max_time_s(self) -> float: ...

    @property
    def traversal_budget_s(self) -> float: ...

    @property
    def stop_time_s(self) -> float: ...

    @property
    def buffer_s(self) -> float: ...

    @property
    def direct_time_s(self) -> float: ...

    def node_count(self) -> int: ...

    def slack_at_node_id(self, node_id: int) -> float | None: ...

    def nodes(self) -> list[dict[str, float | int]]:
        """
        Return nodes with ``node_id``, ``lat``, ``lon``, ``inbound_time_s``,
        ``outbound_time_s``, and ``slack_s``.
        """
        ...

    def slack_polygon(self, min_slack_s: float = 0.0) -> str | None:
        """
        Build a GeoJSON polygon enclosing nodes with at least ``min_slack_s``.
        """
        ...

    def slack_polygons(self, min_slack_values: list[float]) -> list[str | None]:
        """
        Build one slack polygon per minimum-slack threshold.
        """
        ...

    def __repr__(self) -> str: ...

class SpatialGraph:
    """
    A road-network graph loaded from OpenStreetMap.

    Obtain via :func:`build_graph`.  Reuse across multiple queries to avoid
    redundant cache lookups.

    Attributes are read-only; all mutations happen inside Rust.
    """

    @staticmethod
    def from_pbf(
        path: str,
        network_type: str,
        retain_all: bool = False,
    ) -> SpatialGraph:
        """
        Load a local OSM PBF file into a reusable ``SpatialGraph``.

        ``network_type`` accepts ``"Drive"``, ``"DriveService"``, ``"Walk"``,
        ``"Bike"``, ``"All"``, or ``"AllPrivate"``.
        """
        ...

    @staticmethod
    def from_osm(
        xml: str,
        network_type: str,
        retain_all: bool = False,
    ) -> SpatialGraph:
        """
        Parse an OSM XML string into a reusable ``SpatialGraph``.

        ``network_type`` accepts ``"Drive"``, ``"DriveService"``, ``"Walk"``,
        ``"Bike"``, ``"All"``, or ``"AllPrivate"``.
        """
        ...

    @staticmethod
    def from_place(
        place: str,
        network_type: str,
        max_dist: float | None = None,
        retain_all: bool = False,
    ) -> SpatialGraph:
        """
        Geocode a place name and build a reusable ``SpatialGraph`` around it.

        ``network_type`` accepts ``"Drive"``, ``"DriveService"``, ``"Walk"``,
        ``"Bike"``, ``"All"``, or ``"AllPrivate"``.
        """
        ...

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

    def isochrone(
        self,
        origin: tuple[float, float],
        minutes: list[float],
    ) -> list[str]:
        """
        Compute isochrones from ``(lat, lon)`` using this graph.

        Parameters
        ----------
        origin:
            ``(lat, lon)`` origin coordinates.
        minutes:
            Travel-time thresholds in minutes.
        Returns
        -------
        list[str]
            One GeoJSON geometry string per time limit, in the same order
            as ``time_limits``.
        """
        ...

    def route(
        self,
        origin: tuple[float, float],
        destination: tuple[float, float],
    ) -> str:
        """
        Find the fastest route between two coordinates using A*.

        The network type (drive/walk/bike) is inherited from the ``SpatialGraph``.

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

    def snap_point(self, lat: float, lon: float) -> dict[str, float | int] | None:
        """
        Return snap diagnostics for the nearest graph node to ``(lat, lon)``.

        Keys: ``input_lat``, ``input_lon``, ``node_id``, ``node_lat``,
        ``node_lon``, and ``distance_m``.
        """
        ...

    def reachable(self, origin: tuple[float, float], minutes: float) -> Reachability:
        """
        Compute one-sided reachability from ``(lat, lon)`` within ``max_time_s``.
        """
        ...

    def reachable_between(
        self,
        origin: tuple[float, float],
        destination: tuple[float, float],
        max_time_s: float,
        stop_time_s: float = 0.0,
        buffer_s: float = 0.0,
    ) -> BetweenReachability:
        """
        Compute two-sided reachability for nodes that fit within:

        ``origin -> node -> destination + stop_time_s + buffer_s <= max_time_s``.
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

Graph = SpatialGraph

# ---------------------------------------------------------------------------
# Module-level functions
# ---------------------------------------------------------------------------

def build_graph(
    lat: float,
    lon: float,
    network_type: str,
    max_dist: float | None = None,
    retain_all: bool = False,
) -> SpatialGraph:
    """
    Build and return a road-network :class:`SpatialGraph` for the area around ``(lat, lon)``.

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
