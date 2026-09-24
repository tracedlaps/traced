# Track & Timing Architecture Map

## Destination

A locked architecture decision (types, module boundaries, key traits — not full implementation) for a `traced_core::track` submodule modeling both closed-loop karting circuits and open-loop runs (e.g. speedtrike descents): track geometry, gates/sectors, sessions/laps/runs, and the GPS-fix-to-track correlation algorithm shape. Also settles how live `GPSFix` data fans out to multiple consumers (the track correlator and a future speedometer). Persistence, track-authoring UI, OpenStreetMap import, and the comparative-stats/PB "timing" module are explicitly not this destination.

## Notes

- Domain: Rust/Dioxus workspace `traced` — karting lap/sector timer + open-loop descent timer + real-time speedometer.
- Consult `/CONTEXT.md` for resolved vocabulary before using a term — it's the single source of truth for names in this domain.
- Every session on this map should default to `/grilling` + `/domain-modeling` unless the ticket type says otherwise.
- Standing preference (confirmed repeatedly across this map's charting session): favor derived-not-stored computation over cached/duplicated state, and the simplest mechanism that fits today's known consumers over generic/pluggable machinery for hypothetical future ones (YAGNI) — matches the user's global "write code as concisely as possible" instruction.

## Decisions so far

- [Track geometry & gate/sector model](issues/01-track-geometry-model.md) — two-layer Waypoint→Polyline, `Track` with `TrackShape` enum (Closed/Open) as a field, Gate = perpendicular line segment w/ derived direction, start/finish always two explicit fields, Sector always derived from ordered gates (never stored), flat local-plane coordinate math (no geodesic crate).
- [GPS-fix correlation algorithm](issues/02-correlator-algorithm.md) — `Correlator` interpolates exact crossing timestamps between fixes, runs an `Idle → Armed → InProgress → Finished` state machine, owns its `Session` 1:1, debounces jitter via monotonic distance-along-track, flags laps with a `gps_gap` bool on signal dropout.
- [Session/Lap/Run model](issues/03-session-lap-run-model.md) — `Session` references its `Track` by id (not owned), holds `Vec<Lap>` (Closed) or a single `Run` (Open); both store raw gate-crossing timestamps only, no cached durations; comparative stats (deltas/PB) explicitly deferred to a future `timing` module.
- [Fix fan-out: Recorder + FixConsumer](issues/04-fix-fanout-recorder.md) — single `Recorder` owns the `GpsProvider` stream and dispatches each `GPSFix` (by value — it's `Copy`) to a `Vec<Box<dyn FixConsumer>>`, avoiding any need to clone/share the stream itself.
- [Design Speedometer's internal metrics](issues/05-speedometer-metrics-design.md) — integrates distance via `speed_ms × dt` (not position-diff, avoids stationary GPS drift), skips distance across large timestamp gaps, rejects low-`accuracy` fixes from all accumulation uniformly, no in-place trip reset (fresh instance per trip).

## Not yet specified

- Exotic track geometries beyond simple Closed/Open — figure-8 (self-intersecting closed loop, ambiguous nearest-segment match near the crossing point), branching/network trails, multi-stage rally-style tracks. May extend `TrackShape` later; not sharp enough to ticket yet.

## Out of scope

- **Persistence** (saving tracks/sessions to disk/DB) — no persistence layer exists yet anywhere in the app; this map's types are in-memory only by design.
- **Track-authoring UI** (waypoint-placement tool, live polyline preview/tweak) — belongs to the `ui`/`web`/`mobile` crates, not `traced_core`'s domain model. `Track` is already source-agnostic (just consumes `Vec<Waypoint>`), so this can be built later without touching this map's types.
- **OpenStreetMap street-snapping import** for waypoint sourcing — an alternate `Waypoint` source, orthogonal to the `Track` type; future authoring-side feature, not a domain-model concern.
- **Comparative "timing" module** (delta-vs-best-lap, personal bests, cross-session stats) — deliberately a separate future map. This map only ensures `Session`/`Lap`/`Run` expose enough raw data (ordered laps, raw timestamps) for that module to be buildable later.
