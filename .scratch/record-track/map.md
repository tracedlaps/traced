# Record a Track Map

## Destination

Create a track by driving or walking it: "Record a track" on the Editor tab shows the trace growing on the map as you go; Stop turns it into sparse waypoints and opens them in the map editor, where gates are placed and the track saved. A capture is not a run: it isn't saved as a recording nor shown in History.

## Decisions

- **Trace math lives in core** (`traced_core::track::trace`). A `Trace` `FixConsumer`:
  - rejects fixes with accuracy > 25 m (the `Speedometer`'s cutoff) and fixes closer than 3 m (or their accuracy) to the last kept one, so standing still doesn't scribble. Position, not speed: Android reports speed 0 when it has none;
  - keeps the accepted points projected (`Projection` from the first one);
  - after each accepted fix, publishes the simplified trace (`geo::Simplify`, Douglas–Peucker, ε ≈ 1.5 m, unprojected) on a `watch<Arc<[Waypoint]>>`. Re-simplifying the whole trace per fix is O(n); a 10-minute 1 Hz capture is ~600 points, fine.
  - `into_waypoints()` = the same simplified trace, then if `import::looks_closed`, cut at the first return to the start after ≥ 100 m so two laps give one loop. Live and final shapes match because they share the simplifier.
- **`Recorder` gets an optional `Trace` slot**, like `correlator`; `Recording.trace: Option<watch::Receiver<Arc<[Waypoint]>>>`, `RecordingSummary.trace: Option<Vec<Waypoint>>`. `start` is at 5 args, so this may be the point to take an options struct.
- **No journal, not stored.** `RecorderCtx::capture()` starts a recording with `Trace`, no track, no `storage::begin`. A crash loses the capture (accepted for v1).
- **UI only displays.** Capture screen bridges `Recording.trace` into a signal and hands it to `Map`; no trace logic in the UI (see UI-display-only rule).
- **Map:** `View` gains `trace: Vec<LatLng>` (a `L.polyline`, no markers) and `me: Option<LatLng>` (a position dot). The map follows `me` while capturing unless the user has panned away.
- **Cancel and Stop.** Cancel discards the capture and goes back to the Editor tab; Stop opens the editor.
- **Default name** from Nominatim reverse geocoding of the trace's first waypoint (`/reverse?format=jsonv2&zoom=17`, its `address.road`, else `name`), via the map's WebView `fetch` like search: `ToMap::Reverse(LatLng)` → `FromMap` place name. Asked once, on Stop. No answer (offline, rejected, no road) → "Recorded track". The name field stays editable.
- **Gates are placed after recording, in the editor**, not marked mid-drive. The editor opens with `Draft::imported` (start/finish on the end waypoints, or one gate if closed).

## Out of scope

- Marking gates while recording.
- Keeping captures in History / crash recovery of captures.
- Snapping the trace to roads.

## Slices — 1–4 done (2026-09-27), 5 pending a real capture

1. **Core `Trace` + tests:** filtering, simplifying, loop cutting, published snapshots. Round trip: `MockGPSProvider::drive` a preset with jitter → `Trace` → builds, length close to the source's.
2. **Recorder wiring:** optional `Trace` slot, `Recording.trace`, `RecordingSummary.trace`; `RecorderCtx::capture()` (device GPS, or the mock driving a preset on desktop).
3. **Capture screen:** `/editor/record` + Editor-tab button; GPS state/permission (pull shared bits out of `StartPanel`), distance, `Map` with live trace + position dot, Cancel, Stop → `Editor { initial: Draft::imported(..) }`; "No usable trace" if < 2 waypoints. Leaving the screen stops the capture.
4. **Name from the street:** reverse geocode on Stop, "Recorded track" fallback.
5. **Tune ε and thresholds** on a real capture pulled off the phone.

## Open

- ε = 1.5 m and the 3 m step are guesses (slice 5). On the emulator a 10 m-step square left pairs of waypoints at some corners.

## Found on the way

- `Recorder::start` takes `trace: bool`; `bridge` takes `Clone` values (the trace is an `Arc<[Waypoint]>`).
- `Map` takes `locate: false` so the capture screen doesn't open a second device stream to find you.
- Distance on the capture screen is the `Speedometer`'s (speed × time), so it reads 0 m on emulator `geo fix` fixes, which carry no speed.
