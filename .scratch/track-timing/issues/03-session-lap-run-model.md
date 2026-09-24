Type: grilling
Status: resolved

## Question

What's the vocabulary and data shape for a single timed attempt on a `Track`, for both the repeating-loop (Closed) and single-pass (Open) cases? What does it need to expose for a future comparative-stats module without being redesigned later?

## Answer

- **Vocabulary**: `Session` = one continuous timed attempt on a `Track`. `Lap` = one completed loop of a `Closed` track (start_gate crossing to the next). `Run` = one completed pass of an `Open` track (start_gate to finish_gate) — deliberately not called "Lap" since nothing loops; "Lap" would misleadingly imply returning to start.
- **`Session` shape**: references its `Track` by id (not an owned copy) + holds either an ordered `Vec<Lap>` (Closed) or a single `Run` (Open), mirroring the `TrackShape` enum split decided in [Track geometry & gate/sector model](01-track-geometry-model.md).
- **Storage principle**: `Lap`/`Run` store only raw gate-crossing timestamps. Duration, sector splits, and deltas are all derived on demand — same "derive, don't duplicate" principle as `Sector` (01), keeping the timestamps as the single source of truth with no cache-invalidation problem.
- **Live vs completed**: while a lap/run is in progress, its partial state lives only in the `Correlator`'s `LiveSessionSnapshot` (see [GPS-fix correlation algorithm](02-correlator-algorithm.md)) — `Session.laps`/`Session.run` only ever contains *completed* entries, appended once the relevant gate is crossed.
- **Scope boundary with future comparative stats**: cross-session ("all-time PB") comparison needs persistence, which is out of scope entirely (see map). In-session comparison (e.g. delta vs best lap so far this session) is possible because `Session` already holds all prior laps' raw timestamps — but the delta/PB *algorithm itself* belongs to a future `timing` module, not this one. This map's only obligation is to not need redesigning when that module arrives.
