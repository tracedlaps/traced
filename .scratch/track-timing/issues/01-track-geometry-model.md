Type: grilling
Status: resolved

## Question

Module name/boundary, and the shape of `Track`/`Waypoint`/`Polyline`/`Gate`/`Sector`: how is a track authored, represented, and geometrically queried, for both closed-loop (karting) and open-loop (descent) cases? What coordinate math backs it?

## Answer

- **Module**: `traced_core::track`, a submodule (mirrors `traced_core::gps`), not a new crate. Split out later only if it grows.
- **Two-layer geometry**: `Waypoint` (sparse, user-placed GPS coords, the authoring input — source-agnostic, works the same whether manually clicked or future-imported) derives a `Polyline` (dense, distance-indexed point sequence) used for all runtime geometry — nearest-point projection, distance-along-track, crossing math. Never conflate the two; `Polyline` is always derived, never authored directly.
- **`Track`**: holds `waypoints`, derived `polyline`, a `TrackShape` field, `start_gate: Gate`, `finish_gate: Gate` (always both present — for `Closed`, `start_gate == finish_gate`, validated at construction), and `sector_gates: Vec<Gate>` (0+, intermediate only, ordered).
- **`TrackShape`**: a plain enum field (`Closed | Open`) on `Track`, not a trait split or generic marker — Closed/Open differ only in whether `start_gate == finish_gate` and whether laps repeat, not in fundamental shape. A trait/generic split is unwarranted until a second real structural difference appears.
- **Scenario check**: even a free-descent speedtrike run is still a `Track` — a predefined waypoint path (however coarse) is authored once so gate-crossing math has geometry to project onto. A truly path-free mode (only a start/finish GPS radius, no line at all) would be a different, simpler concept — not needed now.
- **`Gate`**: a line segment perpendicular to track direction at a point, direction auto-derived from the polyline's tangent at authoring time (never manually set by the user). Chosen over a circular geofence because it gives a clean crossing-instant via sign-change of cross-product between consecutive fixes, with no false-trigger loitering issue a radius geofence has.
- **`Sector`**: never stored — always derived on demand as consecutive pairs from `[start_gate] + sector_gates + [finish_gate]`. Storing separately would risk desync from the gate list.
- **Coordinate math**: local tangent-plane (equirectangular/ENU) flat approximation around the track's centroid — not a geodesic/`geo`-crate approach. Valid at the scale involved (karting tracks to multi-km descents); geodesic curvature error is negligible and not worth a new dependency. Reassess only if a track spans tens of km.
- **Known edge case, not solved here**: figure-8 (self-intersecting closed) tracks break the "nearest segment" assumption near the self-intersection. Parked in the map's "Not yet specified" — not needed for v1.
