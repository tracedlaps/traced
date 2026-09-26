# Android GPS Provider Map

## Destination

A real `GpsProvider` for Android phones. It is a Kotlin plugin bridged with `#[manganis::ffi]` and replaces the mock on device. Fixes should be as accurate as the phone allows and carry every useful field (position, speed, bearing, altitude, and each field's accuracy). iOS comes later as a sibling module with the same shape.

## Notes

- The prior art is `../traced_prototype` (`packages/gps/`, `packages/mobile/android/MainActivity.kt`, `.scratch/gps-sensor-layer/`). Reuse what it proved, and replace its once-a-second `getCurrentLocation()` polling.
- dx 0.7.10 bundles the plugin folder as a Gradle submodule, so the plugin's own `build.gradle.kts` can declare dependencies (Play Services included).
- `#[manganis::ffi]` only lets Rust call Kotlin, and supports primitives plus `String`/`Option<String>`. Kotlin cannot call back into Rust.

## Decisions

- **Location:** everything platform-specific lives in `packages/mobile`: `src/gps/` for Rust, `android/gps/` for the Gradle module. A future iOS bridge sits next to it. `traced_core` stays pure, with no manganis, Gradle or serde.
- **JSON across the FFI (like Tauri):** every Kotlin method returns (and takes, when it has arguments) a JSON string. Replies are `{"ok":true,"data":…}` or `{"ok":false,"error":"<code>","detail":"…"}`.
  - Kotlin uses `org.json`, which is built into Android, so no serialization compiler plugin is needed.
  - Rust uses typed `serde` structs and one generic `invoke<T: DeserializeOwned>` that maps error codes to `GpsError`.
  - `FixDto` (`camelCase`) converts into `GPSFix`.
- **Source:** Fused first, `LocationManager` as fallback. `start` checks `GoogleApiAvailability.isGooglePlayServicesAvailable() == SUCCESS`.
  - If Play Services is available: Fused `requestLocationUpdates` with `PRIORITY_HIGH_ACCURACY`, `setMinUpdateIntervalMillis(0)`, and `setMaxUpdateDelayMillis(0)` so updates aren't batched.
  - Otherwise, or if Fused throws: `LocationManager` `GPS_PROVIDER` with `minTime=0` and `minDistance=0`.
  - The source in use is reported in the status.
- **Transport:** push on the Kotlin side, polling on the Rust side. Both sources' listeners, running on a `HandlerThread`, add fixes to one bounded queue that keeps the newest ~64. Rust calls `drainFixes()` every 100 ms. This gets subscription-rate fixes without hand-written JNI callbacks.
- **Fix fields:**
  - Always present: `lat`, `lon`, `accuracyM`, `timeMs`, `elapsedRealtimeNs`, `isMock`, `source`.
  - `null` unless the matching `has*()` check is true: `altitudeM`, `verticalAccuracyM`, `speedMps`, `speedAccuracyMps`, `bearingDeg`, `bearingAccuracyDeg`.
  - `timeMs` is epoch time computed from the monotonic clock (`elapsedRealtimeNanos` plus the boot time), as in the prototype, because the wall clock can jump.
- **Satellites:** a `GnssStatus.Callback` (works with both sources) counts the satellites used in the fix.
- **Status is pushed**, not polled: the provider exposes a `watch::Receiver<GpsStatus>`, consistent with [snapshot delivery](../track-timing/issues/06-snapshot-delivery.md). Android doesn't report HDOP, so `GpsStatus::Fixed` becomes `{ satellites, accuracy_m, source }`.
- **Plugin construction:** load the class with `activity.getClassLoader().loadClass(..)` and wrap it with `GpsPlugin::from_global_ref`. The macro's `new()` uses `FindClass`, which can't see app classes from tokio threads.
- **Permission:** use the prototype's custom `MainActivity` (set by `application.android_main_activity`), whose `registerForActivityResult` launcher is wired into the plugin's companion object. `requestPermissions` waits (up to 60 s: users take longer than 5 s to answer, and a timeout reads as a denial) for the result, so Rust calls it inside `spawn_blocking`. It is asked for lazily, the first time the user starts a recording.
- **Background:** v1 keeps the screen on while recording (`FLAG_KEEP_SCREEN_ON`). A foreground service is deferred.

## Kotlin surface

```kotlin
class GpsPlugin(activity: Activity) {
    fun checkPermissions(): String   // {granted}
    fun requestPermissions(): String // {granted}, waits for the dialog
    fun start(optsJson: String): String // {intervalMs, forceLocationManager} → {source}
    fun stop(): String
    fun drainFixes(): String         // [fix, …]
    fun status(): String             // {locationEnabled, satellitesUsed, source}
}
```

## Phases

1. **Core (`traced_core::gps`):**
   - `GPSFix` gains `vertical_accuracy_m`, `speed_accuracy_ms` and `heading_accuracy_deg`, all `Option<f32>`.
   - Reshape `GpsStatus::Fixed`.
   - Add `GpsError::LocationDisabled`.
   - Add a status `watch` to the `GpsProvider` trait and update the mock.
2. **Kotlin plugin** at `packages/mobile/android/gps/`: namespace `dev.traced.gps`, minSdk 26, dependencies `play-services-location:21.3.0` and `core-ktx`.
3. **Rust bridge** at `packages/mobile/src/gps/`:
   - `wire.rs`: DTOs plus `invoke`. Not Android-gated, and unit-tested on the desktop.
   - `android.rs`: the ffi block, `construct_plugin`, and `AndroidGpsProvider`.
   - Dependencies: `manganis` (Android target only), `serde`, `serde_json`.
4. **App:**
   - Add `packages/mobile/Dioxus.toml` with `[permissions.location]` (fine), `[android] min_sdk = 26`, `features = ["android.hardware.location.gps"]` and `android_main_activity`.
   - `RecorderCtx::start` uses the real provider on Android and the mock elsewhere, with a developer switch for the mock on the phone.
   - The Setup screen shows the permission and GPS state and which source is in use.
   - Keep the screen on while recording.
5. **Test on device:**
   - In the emulator: `adb shell pm grant`, then `adb emu geo fix` or GPX playback.
   - On the phone: compare the fix rate and accuracy of Fused and `LocationManager` (`forceLocationManager`) on a real drive.

## Out of scope

- iOS bridge (same shape, later).
- Background recording with a foreground service.
- Raw GNSS / NMEA (true HDOP, 10 Hz chip modes).
- Hand-written JNI push callbacks (`CallbackSystem`). The queue drain makes them unnecessary unless device testing shows otherwise.
