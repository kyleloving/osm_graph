"""Compatibility shim for the old pysochrone import path.

Graphways was formerly osm_graph / pysochrone. New code should import
``graphways`` directly.
"""

from warnings import warn

from graphways import *  # noqa: F401,F403

warn(
    "pysochrone has been renamed to graphways; import graphways instead",
    DeprecationWarning,
    stacklevel=2,
)
