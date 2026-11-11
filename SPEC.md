# gpx-trim

Zero-dependency Rust CLI. Parse only the GPX subset below. No XML crate. No network.

## Test command

`cargo test`

## GPX subset

Read `trkpt` elements with `lat`, `lon`, and a child `ele` in meters. Ignore everything else. Preserve point order.

## Command

`gpx-trim --input in.gpx --epsilon 10 --output out.gpx`

Simplify the track with Douglas-Peucker. Convert lat/lon to local meters with the equirectangular approximation around the first point: `x = (lon-lon0) * 111_320 * cos(lat0)`, `y = (lat-lat0) * 110_540`, degrees to radians for cos only. Epsilon is meters. Always keep the first and last point.

Write a minimal GPX file containing only the kept points, 6 decimal places for lat/lon and 1 for ele.

Stdout, exactly two lines:

```
distance_m 1234.5
elevation_gain_m 40.0
```

Distance is the haversine sum of the original points, earth radius 6_371_000 m, one decimal. Elevation gain sums positive ele deltas of the original points, one decimal. Do not count descents.

Fewer than 2 points: exit 2.

## Tests

Three points on a straight line in local meters, with the middle point 5 m off the line. Epsilon 10 keeps 2 points. Epsilon 1 keeps 3. A second fixture checks a known elevation gain.
