Type: grilling
Status: resolved

## Question

First slice of the mobile app: build 06's UI side (`RecorderCtx`, bridge, `LiveClock`) on the driving mock. Which crate, which screens, what stays from the Dioxus template?

## Answer

- **No fullstack.** `traced_core` runs on-device; the mobile crate drops the template's `fullstack`/`server` features and its `ui`→`api` dependency. Re-add if sync/persistence ever needs a server.
- **All in `packages/mobile`.** `RecorderCtx`, screens and components live there. Extract to `ui` only if web/desktop ever show a recording (web would also need `web-time` for `Instant`).
- **Screens: Setup → Live → Results.** Setup picks a track (or none) and starts. Live shows speed stats, lap/sector, `LiveClock`, Stop. Results reads `last_summary`: laps with splits and the speedometer summary.
- **Dev loop: desktop + phone.** The mobile crate gets a `desktop` feature for fast `dx serve --platform desktop` on the mock; Android for real checks.
- **Mock seam.** Until a real provider exists, `RecorderCtx::start` builds a `MockGPSProvider::drive` per recording (it has to know which track to drive; no track → drive the first one). 06's app-owned `gps: Arc<dyn GpsProvider>` field arrives with the real provider.
- **Tracks are in code** (`TRACKS`), built once at startup: the Lisbon loop, plus a real local karting track once its waypoints are provided. No persistence yet.
- **Bridge tasks clear their own signal** when the channel closes, instead of `stop` clearing `speed`/`live`, which could race with a last pending update. They use `spawn_forever`, since a component-scoped `spawn` from Setup would be cancelled on navigating to Live.
