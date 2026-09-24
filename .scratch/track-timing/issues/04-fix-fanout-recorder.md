Type: grilling
Status: resolved

## Question

The `Correlator` needs live `GPSFix`es, but so does a planned real-time speedometer (top speed, distance travelled, average speed). `GpsProvider::stream_fixes()` returns a single-consumer `BoxStream` — who owns it, and how do multiple features see the same fixes? Is `GPSFix` cloned?

## Answer

- **`GPSFix` itself is already `Copy`** (`gps/mod.rs`) — duplicating the *value* is free. What can't be duplicated is the *stream* — `BoxStream` is a trait object, single-consumer by nature, and a real hardware GPS provider can't be polled from two places at once regardless.
- **Pattern**: a single owner, `Recorder`, holds the `Box<dyn GpsProvider>`, drives `stream_fixes()` in a loop, and calls `on_fix(fix)` on every registered consumer in order — no channel, no broadcast, no backpressure concern, since `GPSFix` is cheap to hand out by value and consumers are simple synchronous accumulators.
- **`FixConsumer` trait**: `fn on_fix(&mut self, fix: GPSFix)`. Implemented by `track::Correlator` and (future) `speedometer::Speedometer`. Each is a stateful accumulator exposing its own poll-able snapshot — matches the `LiveSessionSnapshot` pattern from [GPS-fix correlation algorithm](02-correlator-algorithm.md) and the project's UI-is-display-only convention.
- **Registration shape**: `Recorder` holds `Vec<Box<dyn FixConsumer>>`, not fixed named fields — a third consumer (e.g. raw-fix logging for later persistence/replay) is already plausible, and the open list costs nothing now.
- **Module placement**: `Recorder` is a sibling module to `track` and `speedometer` inside `traced_core` (e.g. `traced_core::recorder`), not nested inside `track` — it's track-agnostic (a speedometer can run with no track armed at all).
- **Naming note**: "Session" was already taken (see [Session/Lap/Run model](03-session-lap-run-model.md)) for the Track-attempt aggregate, so this fan-out owner is named `Recorder` instead — avoid confusing the two.
