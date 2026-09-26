Type: grilling
Status: resolved

## Question

[Fix fan-out](04-fix-fanout-recorder.md) had the `Recorder` own `Vec<Box<dyn FixConsumer>>`, but the UI must see each consumer's snapshot (`SpeedometerSnapshot`, `LiveSessionSnapshot`). Who holds the snapshot, how does it reach a Dioxus UI, and what does a recording's lifecycle look like end to end?

## Answer

### Delivery model
- **Push, not poll.** Dioxus is reactive: the UI updates when a value changes, so snapshots are pushed on change. This reverses the earlier "UI polls on a timer" convention. The UI still computes nothing, with one exception (live clock, below).
- **One `tokio::sync::watch` channel per consumer.** A consumer holds the `Sender<Snapshot>` and sends after every `on_fix`. The channel keeps only the latest value, so a slow render skips stale values instead of queueing them. When the sender drops, `changed()` returns `Err`, which the UI treats as "recording ended". `traced_core` stays free of Dioxus types.
- **One snapshot per consumer**, not a combined app snapshot.

### Recorder (`traced_core::recorder`)
- **One `Recorder` per recording.** Start builds fresh consumers and stop drops them. There's no add/remove API. Switching tracks means stopping and starting again, which also resets the speedometer (accepted).
- **Concrete, not `dyn`:** `Speedometer` plus `Option<Correlator>` as fields. This supersedes 04's `Vec<Box<dyn FixConsumer>>`: typed receivers and a typed summary would otherwise need downcasting. `FixConsumer` stays as the trait each consumer implements (`on_fix` only). A third consumer means adding a field.
- **Takes the stream, not the provider:** `Recorder::start(fixes: BoxStream<Result<GPSFix, GpsError>>, track: Option<Arc<Track>>) -> Recording`. The app owns the `GpsProvider` for its whole life (hardware on/off is separate from recording on/off, and the UI keeps `status()` between recordings). This also supersedes 04, and makes the Recorder testable with a hand-written stream.
- **Runs on its own `tokio::spawn` task.** Dioxus desktop/mobile 0.7 run on a multi-threaded tokio runtime (`dioxus-desktop`'s default `tokio_runtime` feature), so spawning from the app works.
- **Errors:** a `GpsError` item is logged and skipped. Consumers already handle dropouts (05 skips gaps, 02 sets the `gps_gap` flag). When the stream ends (`None`), the recording finishes normally.
- **`Arc<Track>`** is shared read-only by the Correlator and the UI.
- **The Correlator is always created armed.** The `Idle` state from 02 is dropped, because "no track" is now `correlator: None`.

### Handle
```rust
pub struct Recording {
    pub speedometer: watch::Receiver<SpeedometerSnapshot>,
    pub session: Option<watch::Receiver<LiveSessionSnapshot>>, // None when no track
    stop: oneshot::Sender<()>,
    task: JoinHandle<RecordingSummary>,
}
impl Recording { pub async fn stop(self) -> RecordingSummary; }

pub struct RecordingSummary { pub session: Option<Session>, pub speedometer: SpeedometerSnapshot }
```
- `stop(self)` consumes the handle. Dropping a `Recording` drops the stop sender, and the loop treats a closed stop channel as "stop", so a forgotten handle can't leak a running recording (its summary is discarded).
- The live snapshot stays small. Completed laps come back once, in `RecordingSummary`.

### Live clock
- A snapshot carries an **anchor**, and the UI shows `anchor.elapsed()`. This is the one allowed UI-side computation.
- The anchor is a **monotonic `Instant`, back-dated**: on detecting a crossing, `anchor = Instant::now() - (latest_fix.time - crossing.time)`. That translates the interpolated GPS-time crossing into the device's monotonic clock, avoiding GPS-vs-device clock skew and the up-to-one-fix-interval detection delay.
- Official lap and run times stay in GPS time inside the `Session`. The display switches to them when a lap completes.
- `std::time::Instant` panics on wasm. Use `web-time` if the web crate ever shows a live recording.

### UI (Dioxus)
- **An app-level context** provided at the root:
  ```rust
  struct RecorderCtx {
      gps: Arc<dyn GpsProvider>,
      recording: Signal<Option<Recording>>,
      last_summary: Signal<Option<RecordingSummary>>,
      speed: Signal<Option<SpeedometerSnapshot>>,
      live: Signal<Option<LiveSessionSnapshot>>,
  }
  ```
  A recording survives moving between screens. A route-scoped owner was rejected because leaving the page would silently stop the recording.
- **Bridge in one place:** on start, the ctx `spawn`s (Dioxus `spawn`, since `Signal` isn't `Send`) one task per receiver that copies each change into `speed`/`live`. Each task ends by itself when its sender drops. Components only read signals and never touch channels, so there's no resubscription or keyed-remount trick.
- **Stop:** `recording.write().take()`, then spawn a task that awaits `stop()` → `last_summary`, and clears `speed`/`live` to `None`. "Not recording" looks different from "recording but stationary". The results screen reads `last_summary`.
- **`LiveClock { anchor: Option<Instant> }`** is its own leaf component with a ~10 Hz `use_future` interval, so each tick re-renders only the clock text. `None` shows `--:--.-`.
