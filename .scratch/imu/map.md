# IMU & G-Meter Map

## Destination

Live g-force dial on the Live screen, fed by the phone's accelerometer: a circle with a ball showing the felt force (lateral × longitudinal), peaks marked. Works whatever way the phone is mounted (handlebar, dash, facing the driver) by finding "down" from gravity and "forward" from GPS. Samples are journaled like fixes, and each session gets its max g-forces as a first simple analysis. Computed in `traced_core`, pushed via `watch`; the UI only displays.

## Decisions

- **Sensors** (Android, new `android/imu` Gradle module, `ImuPlugin.kt`): `TYPE_LINEAR_ACCELERATION` + `TYPE_GRAVITY` at `SENSOR_DELAY_GAME` (~50 Hz) on a private `HandlerThread`, into a bounded queue drained as JSON every 100 ms (same `{ok,data}` envelope and `load_plugin` as GPS). No linear sensor → raw `TYPE_ACCELEROMETER`, gravity split off by low-pass in core. No permission needed (< 200 Hz).
- **`traced_core::imu`**: `ImuSample { timestamp, accel: [f32; 3], gravity: [f32; 3] }` in device axes; timestamp anchored to the boot epoch like fixes, so samples and fixes share a clock.
- **Orientation, automatic**:
  - *Down*: the gravity vector; acceleration is projected onto the horizontal plane. Handles any tilt, portrait or landscape.
  - *Forward*: learned from GPS. While GPS speed changes clearly (|dv/dt| above a threshold, fixes accurate), the horizontal acceleration is along the travel axis, its sign given by dv/dt. Kept as a slowly adapting estimate, so a handlebar turning with steering or a mount that shifts re-aligns on its own. Lateral = down × forward.
  - Until forward is known, `aligned: false` (UI shows the ball greyed, "calibrating").
- **`GMeter`** consumer: sees fixes and samples; EMA ~0.2 s against vibration; publishes `GForceSnapshot { lateral_g, longitudinal_g, peaks, aligned }` on `watch`. Peaks only count while aligned.
- **Felt force** convention: ball in a bowl. Braking → ball up (forward), left turn → ball right.
- **Recorder** takes `Sources { fixes, imu: Option<stream> }` and `select!`s over both; the journal sees samples too.
- **Storage**: every sample journaled live in an `imu_samples` table (new migration, cascade with its recording), like `fixes`. `RecordingSummary` gains the samples and `RecordingSummary::replay` takes them, so peaks re-derive exactly (re-timing a moved track keeps them).
- **Mock**: `MockImu` synthesises samples from the mock drive (longitudinal = dv/dt, lateral = v × heading rate, plus noise, in a tilted device frame so alignment is exercised). Desktop and emulator show a working dial.
- **Web demo**: the landing page's `Phone` gets the dial, g from its own sim (dv/dt, v²·curvature), since traced_core doesn't build for wasm.

## Out of scope

- Gyro / lean angle, yaw rate.
- Braking/cornering analysis beyond max g (braking points, g over distance, traction circle plots).
- iOS bridge (later, next to the Android one).

## Status (2026-09-27)

Slices 1–7 done; 8 (on-device check) is yours. Notes from building them:
- Samples and fixes share the boot-anchored epoch clock; `ImuSample.gravity` points up (Android's convention).
- `FixConsumer::on_sample` (default no-op) carries samples to the `GMeter` and the journal.
- `RecordingSummary::replay` merges fixes and samples by time; live, they arrive in 100 ms batches, so replayed g can differ a hair.
- Fallback without fused sensors (no `TYPE_LINEAR_ACCELERATION`/`TYPE_GRAVITY`): gravity low-passed (τ 0.5 s) out of the raw accelerometer in `ImuPlugin.kt`.
- Lap peaks: `GMeter` keeps its aligned readings and `into_forces` slices them by each lap's first/last crossing; re-timing a moved track recomputes them.

## Slices

1. `imu` types + `GMeter`: projection, forward alignment from GPS, EMA, peaks; tests on synthetic samples (tilted frames, 90° mounts, sign of forward, re-alignment after a mount shift).
2. `Recorder` `Sources` + `MockImu` from the mock drive; `RecorderCtx.g` signal.
3. UI: `GMeter` dial on Live (SVG rings 0.5/1.0/1.5 g, crosshair, ball with 100 ms transition, peak marks, readout), en + pt strings.
4. Android: `ImuPlugin.kt`, wire DTOs + parse tests, `AndroidImuProvider`, start/stop with the recording.
5. Storage: `imu_samples` migration, journal, `replay` with samples, backup export/restore includes them.
6. Session max g: peak braking / acceleration / lateral (left & right) per session and per lap, in Results and History; CONTEXT.md terms (G-meter, Felt force, Forward alignment).
7. Web demo dial.
8. On-device check: phone mounted in a car/bike, compare the dial with GPS-derived g.
