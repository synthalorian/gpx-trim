# gpx-trim

Simplify a GPX track with Douglas-Peucker and report distance and elevation gain.

Author: synthalorian

## Build

```sh
cargo build --release
```

Binary: `target/release/gpx-trim`

## Test

```sh
cargo test
```

## Usage

```sh
gpx-trim --input in.gpx --epsilon 10 --output out.gpx
```

Reads `trkpt` points that have `lat`, `lon`, and a child `ele`. Ignores everything else. Writes a minimal GPX file of the points kept by Douglas-Peucker (latitude and longitude to 6 decimal places, elevation to 1). Prints exactly two lines:

```
distance_m 1234.5
elevation_gain_m 40.0
```

`distance_m` is the haversine length of the original track in meters (earth radius 6_371_000). `elevation_gain_m` is the sum of positive elevation changes on the original track. Descents are not counted. Epsilon is the simplification tolerance in meters.

Exit status 2 means the file has fewer than 2 track points.

No network. No dependencies beyond the Rust standard library.

MIT License. Copyright (c) 2026 synthalorian.
