# Engine Benchmarks

This benchmark compares Graphways against dedicated routing engines in their
steady-state serving mode.

It intentionally separates setup from query latency:

- Graphways loads an in-process `SpatialGraph`.
- OSRM serves a preprocessed `.osrm` graph over HTTP.
- Valhalla serves prebuilt routing tiles over HTTP.

Do not include engine preprocessing in route or isochrone latency numbers.

## Inputs

Use the same OSM extract for every engine when possible.

```powershell
$PBF = "C:\path\to\munich.osm.pbf"
```

## OSRM Setup

OSRM does not consume a raw PBF at query time. Preprocess once, then run
`osrm-routed`.

```powershell
docker run --rm -t -v ${PWD}:/data osrm/osrm-backend osrm-extract -p /opt/car.lua /data/munich.osm.pbf
docker run --rm -t -v ${PWD}:/data osrm/osrm-backend osrm-partition /data/munich.osrm
docker run --rm -t -v ${PWD}:/data osrm/osrm-backend osrm-customize /data/munich.osrm
docker run --rm -t -i -p 5000:5000 -v ${PWD}:/data osrm/osrm-backend osrm-routed --algorithm mld /data/munich.osrm
```

The benchmark expects OSRM at `http://localhost:5000`.

## Valhalla Setup

Valhalla also needs prebuilt tiles.

```powershell
docker run --rm -v ${PWD}:/data ghcr.io/gis-ops/docker-valhalla/valhalla:latest valhalla_build_config --mjolnir-tile-dir /data/valhalla_tiles --mjolnir-tile-extract /data/valhalla_tiles.tar --mjolnir-timezone /data/timezone.sqlite --mjolnir-admin /data/admin.sqlite > valhalla.json
docker run --rm -v ${PWD}:/data ghcr.io/gis-ops/docker-valhalla/valhalla:latest valhalla_build_tiles -c /data/valhalla.json /data/munich.osm.pbf
docker run --rm -p 8002:8002 -v ${PWD}:/data ghcr.io/gis-ops/docker-valhalla/valhalla:latest valhalla_service /data/valhalla.json 1
```

The benchmark expects Valhalla at `http://localhost:8002`.

## Run

```powershell
python benchmarks/engines/engines.py --pbf C:\path\to\munich.osm.pbf
```

With both services running:

```powershell
python benchmarks/engines/engines.py `
  --pbf C:\path\to\munich.osm.pbf `
  --osrm http://localhost:5000 `
  --valhalla http://localhost:8002
```

For a lighter smoke test:

```powershell
python benchmarks/engines/engines.py --place "Munich, Germany" --radius 10000 --pairs 20
```

## What Is Fair To Compare?

Route latency is the cleanest comparison:

- Graphways: `graph.route(...)`
- OSRM: `/route/v1/...`
- Valhalla: `/route`

Isochrones are only native for Graphways and Valhalla:

- Graphways: `graph.isochrone(...)`
- Valhalla: `/isochrone`

OSRM has no native isochrone endpoint, so this harness does not pretend it does.
