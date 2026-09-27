# Best Lap & Delta-to-Best Map

## Destination

Live delta-to-best while recording, against the all-time best lap (or run) on the armed `Track`, plus personal-best markers in Results, History and the track list. Computed in `traced_core`, pushed via `LiveSessionSnapshot`; the UI only displays.

## Decisions

- **Reference lap** = all-time best on the track: fastest `Lap` across every stored recording of it, `gps_gap` laps excluded (interrupted recordings count). A faster clean lap mid-session replaces it immediately. Derived at recording start, never stored.
- **Continuous delta**: between gates, `delta = (t − lap_start) − reference.time_at(distance_along_track)`. At each gate crossing it snaps to the exact crossing-time delta, `(c[i] − c[0]) − (ref.c[i] − ref.c[0])`. With no reference, no delta.
- **`Track::locate(point, hint) -> f64`**: distance along the polyline of the nearest point, searched in a window around the last distance (hairpins and parallel straights), wrapping at the gate on Closed tracks.
- **`traced_core::timing`** (the comparative module deferred by the track-timing map):
  - `best_lap(&[SavedRecording], track_id) -> Option<(Uuid, Lap)>`: pure, the store stays dumb.
  - `Reference { lap, trace: Vec<(f64, TimeDelta)> }`, built from the best lap's stored fixes (sliced to its crossings, each located on the track); `time_at(d)` interpolates.
- **Correlator owns the delta**: `Correlator::new(track, Option<Reference>)`, threaded through `Recorder::start`. It keeps the in-progress lap's trace so a faster clean lap becomes the new reference. `LiveSessionSnapshot` gains `delta`, `best`, `last_lap_delta`, `last_sector_delta`.
- Open tracks: same mechanism, the best run.

## Out of scope

- Session-best reference mode (all-time only).
- Theoretical best (sum of best sectors).

## Slices

1. `Track::locate` + tests (square, U-turn with parallel legs, Closed wrap).
2. `timing::{best_lap, Reference}` + tests.
3. Correlator: reference, live delta, gate snap, promotion on a faster clean lap; tests on `square()`.
4. Wiring: `Recorder::start`, mobile `storage::reference(track)` loaded in `RecorderCtx::start`; CONTEXT.md terms (Reference lap, Delta, Personal best).
5. UI: Live ±delta badge (green ahead / red behind), best time, sector delta flash; Results best lap + "New PB" + per-lap delta; PB in History and the track list; mock per-lap speed variance so delta is visible in dev.
