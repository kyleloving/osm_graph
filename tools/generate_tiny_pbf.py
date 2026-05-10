#!/usr/bin/env python3
"""
Generate tests/fixtures/tiny_map.osm.pbf without external dependencies.

The fixture intentionally uses only basic OSM PBF primitives:
  - OSMHeader block with OsmSchema-V0.6
  - OSMData block with plain Node and Way messages
  - uncompressed Blob.raw payloads

It mirrors tests/fixtures/tiny_map.osm.
"""

from __future__ import annotations

from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "tests" / "fixtures" / "tiny_map.osm.pbf"

NODES = [
    (1, 48.0000000, 11.0000000, []),
    (2, 48.0010000, 11.0000000, []),
    (3, 48.0020000, 11.0000000, []),
    (4, 48.0030000, 11.0000000, []),
    (5, 48.0020000, 11.0010000, []),
    (100, 48.0015000, 11.0005000, [("amenity", "cafe"), ("name", "Fixture Cafe")]),
]

WAYS = [
    (
        10,
        [1, 2, 3],
        [("highway", "residential"), ("name", "Main Test Street"), ("maxspeed", "30 mph")],
    ),
    (20, [3, 4], [("highway", "primary"), ("oneway", "-1")]),
    (30, [2, 5], [("highway", "service"), ("service", "driveway")]),
    (40, [4, 5], [("highway", "footway")]),
]


def varint(value: int) -> bytes:
    out = bytearray()
    while value >= 0x80:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)


def zigzag(value: int) -> int:
    return (value << 1) ^ (value >> 63)


def key(field: int, wire_type: int) -> bytes:
    return varint((field << 3) | wire_type)


def bytes_field(field: int, payload: bytes) -> bytes:
    return key(field, 2) + varint(len(payload)) + payload


def string_field(field: int, value: str) -> bytes:
    return bytes_field(field, value.encode("utf-8"))


def int32_field(field: int, value: int) -> bytes:
    return key(field, 0) + varint(value)


def int64_field(field: int, value: int) -> bytes:
    return key(field, 0) + varint(value)


def sint64_field(field: int, value: int) -> bytes:
    return key(field, 0) + varint(zigzag(value))


def packed_uint32_field(field: int, values: list[int]) -> bytes:
    payload = b"".join(varint(value) for value in values)
    return bytes_field(field, payload)


def packed_sint64_field(field: int, values: list[int]) -> bytes:
    payload = b"".join(varint(zigzag(value)) for value in values)
    return bytes_field(field, payload)


def string_table() -> tuple[bytes, dict[str, int]]:
    values = [""]
    for _, _, _, tags in NODES:
        for k, v in tags:
            values.extend([k, v])
    for _, _, tags in WAYS:
        for k, v in tags:
            values.extend([k, v])

    unique = []
    seen = set()
    for value in values:
        if value not in seen:
            seen.add(value)
            unique.append(value)

    indexes = {value: i for i, value in enumerate(unique)}
    payload = b"".join(bytes_field(1, value.encode("utf-8")) for value in unique)
    return payload, indexes


def tag_indexes(tags: list[tuple[str, str]], indexes: dict[str, int]) -> tuple[list[int], list[int]]:
    return [indexes[k] for k, _ in tags], [indexes[v] for _, v in tags]


def coord(value: float) -> int:
    # PrimitiveBlock granularity defaults to 100 nanodegrees.
    return round(value * 1_000_000_000 / 100)


def node_message(node_id: int, lat: float, lon: float, tags: list[tuple[str, str]], indexes: dict[str, int]) -> bytes:
    keys, vals = tag_indexes(tags, indexes)
    payload = bytearray()
    payload += sint64_field(1, node_id)
    if keys:
        payload += packed_uint32_field(2, keys)
        payload += packed_uint32_field(3, vals)
    payload += sint64_field(8, coord(lat))
    payload += sint64_field(9, coord(lon))
    return bytes(payload)


def way_message(way_id: int, refs: list[int], tags: list[tuple[str, str]], indexes: dict[str, int]) -> bytes:
    keys, vals = tag_indexes(tags, indexes)
    deltas = []
    previous = 0
    for ref in refs:
        deltas.append(ref - previous)
        previous = ref

    payload = bytearray()
    payload += int64_field(1, way_id)
    payload += packed_uint32_field(2, keys)
    payload += packed_uint32_field(3, vals)
    payload += packed_sint64_field(8, deltas)
    return bytes(payload)


def primitive_block() -> bytes:
    table, indexes = string_table()
    group = bytearray()
    for node in NODES:
        group += bytes_field(1, node_message(*node, indexes))
    for way in WAYS:
        group += bytes_field(3, way_message(*way, indexes))

    block = bytearray()
    block += bytes_field(1, table)
    block += bytes_field(2, bytes(group))
    return bytes(block)


def header_block() -> bytes:
    return string_field(4, "OsmSchema-V0.6")


def blob(raw: bytes) -> bytes:
    return bytes_field(1, raw)


def write_fileblock(handle, block_type: str, payload: bytes) -> None:
    blob_payload = blob(payload)
    header = string_field(1, block_type) + int32_field(3, len(blob_payload))
    handle.write(struct.pack(">I", len(header)))
    handle.write(header)
    handle.write(blob_payload)


def main() -> None:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("wb") as handle:
        write_fileblock(handle, "OSMHeader", header_block())
        write_fileblock(handle, "OSMData", primitive_block())
    print(f"Wrote {OUT}")


if __name__ == "__main__":
    main()
