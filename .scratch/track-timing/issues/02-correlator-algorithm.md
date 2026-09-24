Type: grilling
Status: resolved

## Question

How does the live `Correlator` turn a stream of discrete `GPSFix`es into precise gate-crossing events against an armed `Track`? What's its lifecycle, and how does it handle GPS noise/dropouts?

## Answer

- **Crossing timestamp precision**: interpolate the exact crossing instant along the `fix[i] → fix[i+1]` segment via distance ratio to the gate line intersection, rather than just timestamping the first fix after crossing — a fix-interval's worth of jitter (~1s+) is material error on a ~60s karting lap.
- **Directionality**: a crossing only counts if movement is forward relative to the gate's derived direction (see [Track geometry & gate/sector model](01-track-geometry-model.md)) — prevents a stopped/backing-up vehicle from double-counting near a gate.
- **State machine**: `Idle` (no track armed) → `Armed` (track loaded, waiting for first start-gate crossing) → `InProgress` (accumulating current lap/run) → loops back to `InProgress` on each start-gate re-cross for `Closed` tracks, or → `Finished` once the finish gate is crossed (`Open` tracks, or a manually ended `Closed` session). Gives UI a clean discrete status to poll instead of inferring state from raw data.
- **Session ownership**: `Correlator` owns its `Session` 1:1 and appends completed laps / finishes the run directly — no event bus or external push needed; there's no second consumer of "lap completed" to justify that indirection yet.
- **Jitter near a gate**: only accept a crossing if cumulative distance-along-polyline is monotonically increasing since the last accepted crossing; ignore any crossing that would represent backward progress. Handles the common case (GPS noise while stationary/slow near a line); full sensor-fusion noise filtering is out of scope.
- **GPS signal gaps mid-lap**: a dropout longer than the normal fix interval makes the interpolated crossing time unreliable. Flag the affected `Lap`/`Run` (a `gps_gap: bool` is sufficient for v1) rather than silently trusting the interpolation — prevents a corrupted lap from quietly becoming someone's best lap once the future `timing` module exists.
