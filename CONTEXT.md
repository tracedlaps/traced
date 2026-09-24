# Traced

GPS-based karting lap/sector timer and open-loop (e.g. speedtrike descent) run timer, with real-time speedometer feedback.

## Language

### GPS ingestion

**GPSFix**:
A single timestamped position reading (lat/lon, speed, optional altitude/heading, accuracy) from a `GpsProvider`. `Copy`, cheap to duplicate by value.

**GpsProvider**:
Trait abstracting a source of live `GPSFix`es as an async stream (real device or mock). Only ever consumed by one `Recorder` at a time — the stream itself is not shareable.

**Recorder**:
Owns a `GpsProvider`, drives its fix stream, and forwards each `GPSFix` (by value) to every registered `FixConsumer`. Sole owner of the raw stream — the answer to "how do multiple features see the same fixes" without cloning a stream.

**FixConsumer**:
Trait for anything that reacts to a live fix stream (`Track::Correlator`, `Speedometer`). Stateful accumulator fed one fix at a time; exposes its state via a poll-able snapshot rather than pushing events.
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
The poll-able, read-only view of a `Speedometer`'s current state (current speed, top speed, distance, average speed, elapsed). Same poll-not-push convention as `LiveSessionSnapshot`.

### Timing

**Correlator**:
Stateful `FixConsumer` that matches live fixes against an armed `Track`'s geometry to detect gate crossings, interpolating the exact crossing timestamp between consecutive fixes. Owns a `LiveSessionSnapshot` for its `Session`.

**LiveSessionSnapshot**:
The poll-able, read-only view of a `Correlator`'s current state (e.g. current lap number, elapsed time since last gate). What UI code reads — UI never computes timing itself.

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

Next up: the Correlator's gate-crossing/interpolation algorithm (it tests fixes against `Gate::segment()`) and Session/Lap/Run bookkeeping. Neither is designed yet.
