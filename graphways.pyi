"""
Type stubs for graphways — the compiled Rust extension.

All GeoJSON return values are strings.  Parse them with ``json.loads``.
"""

from __future__ import annotations

# ---------------------------------------------------------------------------
# Graph object
# ---------------------------------------------------------------------------

class ReachableGraph:
    """
    Travel-time-labeled graph view produced by ``SpatialGraph.reachable``.

    Cheap inspection methods operate on the parent graph plus reachable labels.
    Constrained routing and isochrones materialize a bounded subgraph internally.
    """

    @property
    def max_time_s(self) -> float: ...

    def node_count(self) -> int: ...

    def edge_count(self) -> int: ...

    def contains_node(self, node_id: int) -> bool: ...

    def nearest_node(
        self, lat: float, lon: float
    ) -> tuple[int, float, float] | None: ...

    def travel_time_to_node_id(self, node_id: int) -> float | None: ...

    def nodes(self) -> list[dict[str, float | int]]:
        """
        Return reachable nodes with ``node_id``, ``lat``, ``lon``, and
        ``travel_time_s``.
        """
        ...

    def nodes_geojson(self) -> str:
        """
        Return reachable nodes as a GeoJSON ``FeatureCollection`` of points.
        """
        ...

    def edges_geojson(self) -> str:
        """
        Return edges whose source and target nodes are both reachable.
        """
        ...

    def to_geojson(self) -> str:
        """
        Return reachable nodes and edges in one GeoJSON ``FeatureCollection``.
        """
        ...

    def isochrone(
        self,
        origin: tuple[float, float],
        minutes: list[float],
    ) -> list[str]:
        """
        Compute isochrones within this reachable subgraph.
        """
        ...

    def route(
        self,
        origin: tuple[float, float],
        destination: tuple[float, float],
    ) -> str:
        """
        Find the fastest route constrained to this reachable subgraph.
        """
        ...

    def __repr__(self) -> str: ...

class PrismGraph:
    """
    Network-time prism produced by ``SpatialGraph.prism``.

    Cheap inspection methods operate on the parent graph plus prism labels.
    Constrained routing and isochrones materialize a bounded subgraph internally.
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

    def edge_count(self) -> int: ...

    def contains_node(self, node_id: int) -> bool: ...

    def nearest_node(
        self, lat: float, lon: float
    ) -> tuple[int, float, float] | None: ...

    def slack_at_node_id(self, node_id: int) -> float | None: ...

    def nodes(self) -> list[dict[str, float | int]]:
        """
        Return nodes with ``node_id``, ``lat``, ``lon``, ``inbound_time_s``,
        ``outbound_time_s``, and ``slack_s``.
        """
        ...

    def nodes_geojson(self) -> str:
        """
        Return prism nodes as a GeoJSON ``FeatureCollection`` of points.
        """
        ...

    def edges_geojson(self) -> str:
        """
        Return edges whose source and target nodes are both inside the prism.
        """
        ...

    def to_geojson(self) -> str:
        """
        Return prism nodes and edges in one GeoJSON ``FeatureCollection``.
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

    def isochrone(
        self,
        origin: tuple[float, float],
        minutes: list[float],
    ) -> list[str]:
        """
        Compute isochrones constrained to this prism subgraph.
        """
        ...

    def route(
        self,
        origin: tuple[float, float],
        destination: tuple[float, float],
    ) -> str:
        """
        Find the fastest route constrained to this prism subgraph.
        """
        ...

    def __repr__(self) -> str: ...

class SpatialGraph:
    """
    A road-network graph loaded from OpenStreetMap.

    Construct once with :meth:`from_place`, :meth:`from_pbf`, or :meth:`from_osm`.
    Reuse across multiple queries to avoid redundant cache lookups.

    Attributes are read-only; all mutations happen inside Rust.
    """

    @staticmethod
    def from_pbf(
        path: str,
        network: str,
        retain_all: bool = False,
    ) -> SpatialGraph:
        """
        Load a local OSM PBF file into a reusable ``SpatialGraph``.

        ``network`` accepts ``"drive"``, ``"drive_service"``, ``"walk"``,
        ``"bike"``, ``"all"``, or ``"all_private"``.
        """
        ...

    @staticmethod
    def from_osm(
        xml: str,
        network: str,
        retain_all: bool = False,
    ) -> SpatialGraph:
        """
        Parse an OSM XML string into a reusable ``SpatialGraph``.

        ``network`` accepts ``"drive"``, ``"drive_service"``, ``"walk"``,
        ``"bike"``, ``"all"``, or ``"all_private"``.
        """
        ...

    @staticmethod
    def from_place(
        place: str,
        network: str,
        max_dist: float | None = None,
        retain_all: bool = False,
    ) -> SpatialGraph:
        """
        Geocode a place name and build a reusable ``SpatialGraph`` around it.

        ``network`` accepts ``"drive"``, ``"drive_service"``, ``"walk"``,
        ``"bike"``, ``"all"``, or ``"all_private"``.
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
            A GeoJSON geometry string, e.g. from :meth:`isochrone`.

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

    def reachable(self, origin: tuple[float, float], minutes: float) -> ReachableGraph:
        """
        Compute one-sided reachability from ``(lat, lon)`` within ``minutes``.
        """
        ...

    def prism(
        self,
        origin: tuple[float, float],
        destination: tuple[float, float],
        max_minutes: float,
        stop_minutes: float = 0.0,
        buffer_minutes: float = 0.0,
    ) -> PrismGraph:
        """
        Return the network-time prism for nodes that fit within:

        ``origin -> node -> destination + stop_minutes + buffer_minutes <= max_minutes``.
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
        A GeoJSON geometry string, e.g. from :meth:`SpatialGraph.isochrone`.

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
