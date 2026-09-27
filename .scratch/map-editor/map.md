# Map Track Editor Map

## Destination

Create and edit tracks on a map: search for a place, tap to lay waypoints, drag/insert/delete them, place start/finish/sector gates, and see the track's `TrackSvg` render overlaid live. One editor for new, saved and imported tracks.

## Decisions

- **Rust owns state, Leaflet only draws.** `Draft` (waypoints + gate indices) stays the model; JS renders what Rust sends and reports gestures. One `document::eval` channel, JSON both ways:
  - Rust → JS: `Waypoints { points, classes }`, `Overlay { svg, bounds }`, `Search(q)`, `Fit`.
  - JS → Rust: `TapMap { lat, lng }`, `TapMarker(i)`, `Drag { i, lat, lng }`, `Insert { after, lat, lng }`, `SearchResults([..])`.
  - The eval script must stay on one live top-level `await` loop, or the channel closes (prototype lesson).
- **Leaflet vendored** (JS + CSS only, BSD-2 header kept) in `packages/mobile/assets/leaflet/`, loaded via `asset!`. No default marker/layer-control images: `asset!` hashes filenames, breaking the CSS's relative `url()`s. Markers are `divIcon`s; layer toggle is ours.
- **Layers:** Esri World Imagery (default) + OSM streets, attribution kept. OSM tiles confirmed working in the prototype's WebView.
- **Search:** Nominatim from WebView `fetch`, on Enter only (1 req/s policy), up to 5 results to pick from, fly to pick. Fallback if rejected: Rust HTTP with an identifying User-Agent.
- **Overlay:** `Layout` rendered to an SVG string (`format!`, no Dioxus-owned DOM handed to Leaflet) inside `L.svgOverlay`; bounds = `view_box` corners through `Projection::unproject`. Equirectangular vs Mercator distortion is invisible at track scale. `vector-effect: non-scaling-stroke` so zoom doesn't scale strokes. Fallback: per-sector `L.polyline`s of unprojected smoothed points.
- **Gestures:** tap map = append; drag marker = move; drag segment midpoint handle = insert; tap marker = select → "Delete point" (in Start/Finish/Sector mode, tap places a gate instead). Undo via a `Vec<Draft>` stack.
- **`Draft` ops** `insert(i, w)`, `remove(i)` remap gate indices (a drag just sets the waypoint); removing a gate's waypoint drops a sector gate, moves start to the next survivor and finish to the previous. Appending after a finish on the last waypoint takes the finish along, so drawing extends the track. Moving a saved track's waypoints already re-times its recordings (`same_geometry`).
- **Map `div` never conditionally rendered**; each editor mounts a fresh map, started from its `onmounted`.

## Out of scope

- Offline tile caching.
- Snapping waypoints to OSM roads.

## Slices — all done (2026-09-27)

1. Map shell: `/editor/new` route + "Create new track" button on the Editor tab, vendored Leaflet, satellite/streets toggle, search with result list.
2. Waypoints: `Draft::{insert, remove}` + tests, tap/drag/insert/select-and-delete, markers mirror `Draft`, 0–1 waypoints show markers only.
3. Overlay + save: live `svgOverlay` of `Layout::markup`, name/closed/modes/summary/save reused.
4. One editor: undo, "Fit track" (and fit on open), "My location" (`RecorderCtx::locate`, a one-shot device fix; also on opening an empty map), `EditTrack` and file import open in the map editor; the SVG-only editor is gone.

## Found on the way

- `Track::sectors` wrapped every sector with `rem_euclid(total)`, so an Open track gated on its two end waypoints (what drawing produces) timed 0 m. Now only Closed tracks wrap.
- A bare-string eval message (`"fit"`) has a `.search` (String.prototype.search): check strings before object fields in `map.js`.
- Midpoint handles show only on segments ≥ 64 px on screen, redrawn on zoom; dense imported tracks were unreadable otherwise.
- `TrackSvg` now renders `Layout::markup` as inner HTML, so the Live map's sector highlight swaps instantly instead of fading.
