use std::{convert::identity, sync::Arc};

use chrono::Utc;

use dioxus::{core::spawn_forever, prelude::*};
use futures::{stream::BoxStream, StreamExt};
use tokio::sync::watch;
use traced_core::{
    gps::{
        mock::{MockGPSConfig, MockGPSProvider},
        GPSFix, GpsError, GpsProvider, GpsStatus,
    },
    recorder::{Recorder, Recording, RecordingSummary},
    speedometer::SpeedometerSnapshot,
    track::{LiveSessionSnapshot, Track, Waypoint},
};

use uuid::Uuid;

use crate::{
    gps::wire::Permission,
    storage,
    tracks::{mock_speed_kmh, PRESETS},
};
#[cfg(target_os = "android")]
use {crate::gps::android::AndroidGpsProvider, dioxus::logger::tracing::warn};

type Fixes = BoxStream<'static, Result<GPSFix, GpsError>>;

/// App-wide recording state. Components only read the snapshot signals; channels stay in here.
#[derive(Clone, Copy)]
pub struct RecorderCtx {
    pub recording: Signal<Option<Recording>>,
    /// A track being traced (see `capture`): not a recording, so neither saved nor timed.
    pub capture: Signal<Option<Recording>>,
    /// The capture's path so far.
    pub trace: Signal<Option<Arc<[Waypoint]>>>,
    pub last_summary: Signal<Option<RecordingSummary>>,
    /// The current recording's id in the store; `None` if it isn't being saved.
    saved: Signal<Option<Uuid>>,
    /// The track of the current (or last) recording.
    pub track: Signal<Option<Arc<Track>>>,
    pub fix: Signal<Option<GPSFix>>,
    pub speed: Signal<Option<SpeedometerSnapshot>>,
    pub live: Signal<Option<LiveSessionSnapshot>>,
    /// The device GPS, or the mock drive's while it runs.
    pub status: Signal<Option<GpsStatus>>,
    /// Location permission; `None` without a device GPS.
    pub permission: Signal<Option<Permission>>,
    /// Dev switches: drive the mock instead of the device GPS, and skip Fused.
    pub use_mock: Signal<bool>,
    pub force_location_manager: Signal<bool>,
    /// App-lifetime device GPS (leaked once at startup); `None` if the plugin failed to load.
    #[cfg(target_os = "android")]
    device: Option<&'static AndroidGpsProvider>,
}

impl RecorderCtx {
    pub fn new() -> Self {
        #[cfg(target_os = "android")]
        let device = match AndroidGpsProvider::new() {
            Ok(mut gps) => {
                let permission = match gps.init() {
                    Err(GpsError::PermissionDenied) => Permission::Denied,
                    _ => Permission::Granted,
                };
                Some((&*Box::leak(Box::new(gps)), permission))
            }
            Err(e) => {
                warn!("no device GPS: {e}");
                None
            }
        };
        #[cfg(not(target_os = "android"))]
        let device: Option<((), Permission)> = None;
        // Owned by the root scope, where `spawn_forever` runs the tasks that write them.
        let ctx = Self {
            recording: Signal::new_in_scope(None, ScopeId::ROOT),
            capture: Signal::new_in_scope(None, ScopeId::ROOT),
            trace: Signal::new_in_scope(None, ScopeId::ROOT),
            last_summary: Signal::new_in_scope(None, ScopeId::ROOT),
            saved: Signal::new_in_scope(None, ScopeId::ROOT),
            track: Signal::new_in_scope(None, ScopeId::ROOT),
            fix: Signal::new_in_scope(None, ScopeId::ROOT),
            speed: Signal::new_in_scope(None, ScopeId::ROOT),
            live: Signal::new_in_scope(None, ScopeId::ROOT),
            status: Signal::new_in_scope(None, ScopeId::ROOT),
            permission: Signal::new_in_scope(
                device.map(|(_, permission)| permission),
                ScopeId::ROOT,
            ),
            use_mock: Signal::new_in_scope(!cfg!(target_os = "android"), ScopeId::ROOT),
            force_location_manager: Signal::new_in_scope(false, ScopeId::ROOT),
            #[cfg(target_os = "android")]
            device: device.map(|(gps, _)| gps),
        };
        #[cfg(target_os = "android")]
        if let Some(gps) = ctx.device {
            bridge(gps.status(), ctx.status, Some);
        }
        ctx
    }

    /// Records against `track`, or with none (speedometer only). On the device GPS,
    /// first asks for the location permission if needed; fails if denied or location is off.
    pub async fn start(mut self, track: Option<Arc<Track>>) -> Result<(), GpsError> {
        let fixes = match self.device_fixes().await? {
            Some(fixes) => fixes,
            None => self.mock_fixes(track.clone()),
        };
        let reference = track.as_deref().and_then(storage::reference);
        let started_at = Utc::now();
        let (saved, journal) = storage::begin(started_at, track.as_deref()).unzip();
        let recording = Recorder::start(fixes, track.clone(), reference, false, started_at, journal);
        self.show(&recording);
        if let Some(live) = recording.session.clone() {
            bridge(live, self.live, Some);
        }
        self.track.set(track);
        self.last_summary.set(None);
        self.saved.set(saved);
        self.recording.set(Some(recording));
        self.keep_screen_on(true);
        Ok(())
    }

    /// Traces a drive or walk to make a track from, on the device GPS (asking for the location
    /// permission if needed) or the mock driving the first preset.
    pub async fn capture(mut self) -> Result<(), GpsError> {
        let fixes = match self.device_fixes().await? {
            Some(fixes) => fixes,
            None => self.mock_fixes(None),
        };
        let capture = Recorder::start(fixes, None, None, true, Utc::now(), None);
        self.show(&capture);
        bridge(capture.trace.clone().expect("tracing"), self.trace, Some);
        self.capture.set(Some(capture));
        self.keep_screen_on(true);
        Ok(())
    }

    /// Stops the capture; its path's waypoints, if one was running.
    pub async fn end_capture(mut self) -> Option<Vec<Waypoint>> {
        let capture = self.capture.write().take()?;
        self.keep_screen_on(false);
        capture.stop().await.trace
    }

    pub fn stop(mut self) {
        let Some(recording) = self.recording.write().take() else {
            return;
        };
        self.keep_screen_on(false);
        let saved = self.saved.write().take();
        spawn_forever(async move {
            let summary = recording.stop().await;
            if let Some(id) = saved {
                storage::finish(id, &summary);
            }
            self.last_summary.set(Some(summary));
        });
    }
}

impl RecorderCtx {
    /// Mirrors the fixes and speedometer of `recording` into their signals.
    fn show(self, recording: &Recording) {
        bridge(recording.fix.clone(), self.fix, identity);
        bridge(recording.speedometer.clone(), self.speed, Some);
    }

    /// Drives the chosen track (or the first preset) on the mock GPS.
    fn mock_fixes(self, track: Option<Arc<Track>>) -> Fixes {
        let driven = track
            .unwrap_or_else(|| Arc::new(PRESETS[0].track.clone().build().expect("valid preset")));
        let mut gps = MockGPSProvider::drive(
            &driven,
            mock_speed_kmh(&driven) / 3.6,
            MockGPSConfig {
                jitter_m: 0.5,
                pace_jitter: 0.03,
                ..Default::default()
            },
        );
        let _ = gps.init();
        bridge(gps.status(), self.status, Some);
        gps.stream_fixes()
    }

    /// Where the phone is: the recording's (or capture's) latest fix, or the device GPS's first, if one comes soon.
    pub async fn locate(self) -> Option<GPSFix> {
        if self.recording.peek().is_some() || self.capture.peek().is_some() {
            return *self.fix.peek();
        }
        let mut fixes = self.device_fixes().await.ok()??;
        let first = async { while let Some(fix) = fixes.next().await { if let Ok(fix) = fix { return Some(fix) } } None };
        tokio::time::timeout(std::time::Duration::from_secs(20), first).await.ok()?
    }

    /// The device GPS's fixes, unless there is none or the mock is switched on.
    #[cfg(target_os = "android")]
    async fn device_fixes(mut self) -> Result<Option<Fixes>, GpsError> {
        if *self.use_mock.peek() {
            return Ok(None);
        }
        let gps = self
            .device
            .ok_or_else(|| GpsError::Unavailable("the GPS plugin failed to load".into()))?;
        let permission = gps.request_permission().await?;
        self.permission.set(Some(permission));
        if permission != Permission::Granted {
            return Err(GpsError::PermissionDenied);
        }
        if !gps.location_enabled()? {
            return Err(GpsError::LocationDisabled);
        }
        Ok(Some(gps.stream(*self.force_location_manager.peek())))
    }

    #[cfg(not(target_os = "android"))]
    async fn device_fixes(self) -> Result<Option<Fixes>, GpsError> {
        Ok(None)
    }

    fn keep_screen_on(self, _on: bool) {
        #[cfg(target_os = "android")]
        if let Some(Err(e)) = self.device.map(|gps| gps.keep_screen_on(_on)) {
            warn!("keep screen on: {e}");
        }
    }

    /// Opens the app's system settings, where a blocked permission can be granted.
    pub fn open_settings(self) {
        #[cfg(target_os = "android")]
        if let Some(Err(e)) = self.device.map(|gps| gps.open_app_settings()) {
            warn!("open settings: {e}");
        }
    }
}

/// Copies each change into `signal` (through `into`), then clears it once the recording
/// drops the sender. `spawn_forever`: a component-scoped task would die when its screen unmounts.
fn bridge<T: Clone + 'static, U: 'static>(
    mut rx: watch::Receiver<T>,
    mut signal: Signal<Option<U>>,
    into: fn(T) -> Option<U>,
) {
    spawn_forever(async move {
        loop {
            signal.set(into(rx.borrow_and_update().clone()));
            if rx.changed().await.is_err() {
                break;
            }
        }
        signal.set(None);
    });
}
