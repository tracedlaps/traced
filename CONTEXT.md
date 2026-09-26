# Traced

GPS-based karting lap/sector timer and open-loop (e.g. speedtrike descent) run timer, with real-time speedometer feedback.

## Language

### GPS ingestion

**GPSFix**:
A single timestamped position reading (lat/lon, speed, optional altitude/heading, accuracy) from a `GpsProvider`. `Copy`, cheap to duplicate by value.

**GpsProvider**:
Trait abstracting a source of live `GPSFix`es as an async stream (real device or mock). Only ever consumed by one `Recorder` at a time — the stream itself is not shareable.

**Recorder**:
Created per recording: takes a `GpsProvider`'s fix stream (the app keeps the provider), and forwards each `GPSFix` by value to its consumers, a `Speedometer` and an optional `Correlator`. Sole owner of the stream: the answer to "how do multiple features see the same fixes" without cloning a stream.

**Recording**:
The handle returned by `Recorder::start` for one recording: typed `watch` receivers for the latest raw `GPSFix` and each consumer's snapshot, plus `stop(self) -> RecordingSummary`. Dropping it stops the recording.

**RecordingSummary**:
What a finished recording hands back: the completed `Session` (if a track was armed) and the final `SpeedometerSnapshot`.

**FixConsumer**:
Trait for anything that reacts to a live fix stream (`Track::Correlator`, `Speedometer`). Stateful accumulator fed one fix at a time; publishes its snapshot on a `watch` channel after each fix (latest value only, pushed on change).
_Avoid_: Subscriber, listener (implies pub/sub — this is direct dispatch from one owner, not a broadcast channel).

### Track geometry

**Waypoint**:
A single GPS coordinate placed by the user while authoring a `Track`. Sparse — the raw authoring input, regardless of source (manual placement, future map-import).

**Polyline**:
Dense, distance-indexed point sequence derived from a `Track`'s `Waypoint`s, used for runtime geometry (nearest-point projection, distance-along-track). Never authored directly.

**Track**:
A predefined path with a `Boundary` (its start/finish gate(s)) and zero or more `sector_gates`, ordered along the `Polyline` from the start gate. The unit of "what am I timing myself against."

**TrackShape**:
Builder input: where the boundary gate(s) go, as `Waypoint`s. `Closed { gate }` (start == finish; laps repeat) or `Open { start, finish }` (distinct; single pass).

**Boundary**:
The built counterpart of `TrackShape`, holding real `Gate`s. A field on `Track`; Closed's start == finish is unrepresentable rather than checked.

**Gate**:
A line segment crossing the `Polyline`, perpendicular to track direction at that point (direction derived from the polyline's tangent, never authored by hand). Crossing it is a timed event.
_Avoid_: Checkpoint, marker (Gate specifically implies the line-segment-with-direction geometry, not just a point).

**Sector**:
The stretch of track between two consecutive gates (`start_gate` → first `sector_gate` → … → `finish_gate`). Always derived on demand from the ordered gate list, never stored — one source of truth.

### Real-time feedback

**Speedometer**:
Stateful `FixConsumer` that accumulates current/top/average speed and distance travelled from the live fix stream, independent of any `Track`/`Session`. Integrates distance via `speed_ms × dt`, not position differencing, to avoid stationary GPS position jitter.

**SpeedometerSnapshot**:
The read-only view of a `Speedometer`'s current state (current speed, top speed, distance, average speed, elapsed), pushed to the UI on change. Same convention as `LiveSessionSnapshot`.

### Timing

**Correlator**:
Stateful `FixConsumer` that matches live fixes against an armed `Track`'s geometry to detect gate crossings, interpolating the exact crossing timestamp between consecutive fixes. Owns a `LiveSessionSnapshot` for its `Session`.

**LiveSessionSnapshot**:
The read-only view of a `Correlator`'s current state (status, current lap and sector, lap and sector clock anchors, last lap/sector times), pushed to the UI on change. The UI never computes timing itself; its one exception is rendering an anchor's `elapsed()` for the live clocks.

**Session**:
One continuous timed attempt on a `Track`: a reference to the `Track` plus, depending on `TrackShape`, either an ordered `Vec<Lap>` (Closed) or a single `Run` (Open). Laps/Runs store only raw gate-crossing timestamps — durations and deltas are derived, never stored.
_Avoid_: Recording, attempt (Session is the specific term for a Track-timing attempt; don't confuse with `Recorder`, which is the GPS fix fan-out owner and has no notion of Track).

**Lap**:
One completed loop of a `Closed` `Track` — from one start_gate crossing to the next.

**Run**:
One completed pass of an `Open` `Track`, start_gate to finish_gate. The Open-track equivalent of a Lap; not called "Lap" because nothing loops.

## Implementation status (2026-09-24)

`packages/traced_core/src/track/mod.rs` is implemented and tested: `Projection` (project/unproject), `TrackBuilder::build()` (project → close if Closed → densify to 2 m → place boundary + sector gates → validate order → sort sectors from the start gate), `Track::{length, gates_ordered, sectors}`, `Gate::segment`, and `Sector<'a>` as a borrowed view.

Full design record (class diagram, build/runtime pipeline, decisions log through #31): https://claude.ai/artifact/46KB9Ye4fo6WEbfunHwRAT

The short version, for anyone picking this up:
- `geo` crate, not a hand-rolled geo module. All geometry math runs in a **projected local Cartesian plane** (equirectangular, origin = first waypoint).
- `TrackShape` (placement `Waypoint`s in) → `Boundary` (`Gate`s out). Both are enums, so a wrong placement for the track kind can't be written.
- `Track` has no public constructor. `TrackBuilder::build()` is the only way in, because a `Gate` can't be derived before the `Polyline` exists.
- Build errors: `TooFewWaypoints`, `PlacementOffTrack` (>30 m from the polyline), `PlacementBeyondTrack` (past an open end), `FinishBeforeStart`, `SectorOutsideTrack`.

`recorder` + `speedometer` implemented (2026-09-26): `FixConsumer`, `Speedometer` (trapezoid `speed_ms × dt`, 5 s gap skip, >25 m accuracy reject; the `watch::Sender` holds the snapshot, stored `average_speed_ms` = distance ÷ integrated time, excluding dropouts), `Recorder::start(fixes) -> Recording`, `Recording::stop() -> RecordingSummary`. No `Correlator` slot yet. `traced_cli` records the mock stream until Ctrl-C.

`track::session` implemented (2026-09-26): `Session { track_id, completed: Completed::{Laps(Vec<Lap>), Run(Option<Run>)} }`, `Run = Lap` (same shape), `Lap` holds GPS-time `crossings` in `gates_ordered` order + `gps_gap`; `duration()`/`splits()` derived. `Session::record` and `Lap::new` are crate-private, for the Correlator.

`track::correlator` implemented (2026-09-26) and wired into `Recorder::start(fixes, Option<Arc<Track>>)`: only the next gate in `gates_ordered` is tested per fix (segment intersection, forward-only, interpolated time), a dropout > 2 s sets `gps_gap`, and re-crossing the same gate needs the fix to have been > 30 m from it first. `LiveSessionSnapshot { status: Armed | InProgress { anchor, sector } | Finished, laps, last_lap }`.

`MockGPSProvider::drive(&track, speed_ms, config)` (2026-09-26): constant-speed fixes along the polyline (via `Track::point_at`), endless laps on Closed tracks; every mock now stamps fixes on send and applies `config.jitter_m`. `traced_cli` times laps of its Lisbon loop.
