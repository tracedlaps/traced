use std::{convert::identity, sync::Arc};

use chrono::{DateTime, Utc};

use dioxus::{core::spawn_forever, prelude::*};
use futures::{stream::BoxStream, StreamExt};
use tokio::sync::watch;
use traced_core::{
    gps::{
        mock::{MockGPSConfig, MockGPSProvider},
        GPSFix, GpsError, GpsProvider, GpsStatus,
    },
    imu::{GForceSnapshot, ImuSample},
    recorder::{Recorder, Recording, RecordingSummary, Sources},
    speedometer::SpeedometerSnapshot,
    track::{LiveSessionSnapshot, Track, Waypoint},
};

use uuid::Uuid;

use crate::{
    background::Check,
    gps::wire::Permission,
    storage,
    tracks::{mock_speed_kmh, PRESETS},
};
#[cfg(target_os = "android")]
use {
    crate::{background::AndroidBackground, gps::android::AndroidGpsProvider, imu::AndroidImu},
    dioxus::logger::tracing::warn,
};

type Fixes = BoxStream<'static, Result<GPSFix, GpsError>>;

/// Why the guide to allowing Traced in the background is showing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Guide {
    /// On Start, on a phone whose battery manager ignores the system exemption, while Traced isn't allowed.
    Start,
    /// The last recording was killed.
    Killed,
}

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
    /// The saved recording being replayed instead, if it is one.
    pub replaying: Signal<Option<Uuid>>,
    /// Its fixes, to show the whole path.
    pub replay_fixes: Signal<Option<Arc<[GPSFix]>>>,
    /// The track of the current (or last) recording.
    pub track: Signal<Option<Arc<Track>>>,
    pub fix: Signal<Option<GPSFix>>,
    pub speed: Signal<Option<SpeedometerSnapshot>>,
    /// `None` without an accelerometer.
    pub g: Signal<Option<GForceSnapshot>>,
    pub live: Signal<Option<LiveSessionSnapshot>>,
    /// The device GPS, or the mock drive's while it runs.
    pub status: Signal<Option<GpsStatus>>,
    /// Location permission; `None` without a device GPS.
    pub permission: Signal<Option<Permission>>,
    /// Dev switches: drive the mock instead of the device GPS, and skip Fused.
    pub use_mock: Signal<bool>,
    pub force_location_manager: Signal<bool>,
    /// The background guide, while it shows.
    pub guide: Signal<Option<Guide>>,
    /// Not now was chosen: no `Guide::Start` again until the app restarts.
    guide_skipped: Signal<bool>,
    /// The system's battery exemption isn't enough on this phone (see `Guide::Start`).
    strict: bool,
    /// App-lifetime device GPS (leaked once at startup); `None` if the plugin failed to load.
    #[cfg(target_os = "android")]
    device: Option<&'static AndroidGpsProvider>,
    /// App-lifetime screen-off keeper; `None` if its plugin failed to load.
    #[cfg(target_os = "android")]
    background: Option<&'static AndroidBackground>,
    /// App-lifetime accelerometer; `None` if its plugin failed to load.
    #[cfg(target_os = "android")]
    imu: Option<&'static AndroidImu>,
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
        #[cfg(target_os = "android")]
        let background = AndroidBackground::new().map(|b| &*Box::leak(Box::new(b)));
        #[cfg(target_os = "android")]
        let check = background.and_then(|b| b.check().inspect_err(|e| warn!("background check: {e}")).ok());
        #[cfg(not(target_os = "android"))]
        let check: Option<Check> = None;
        // Owned by the root scope, where `spawn_forever` runs the tasks that write them.
        let ctx = Self {
            recording: Signal::new_in_scope(None, ScopeId::ROOT),
            capture: Signal::new_in_scope(None, ScopeId::ROOT),
            trace: Signal::new_in_scope(None, ScopeId::ROOT),
            last_summary: Signal::new_in_scope(None, ScopeId::ROOT),
            saved: Signal::new_in_scope(None, ScopeId::ROOT),
            replaying: Signal::new_in_scope(None, ScopeId::ROOT),
            replay_fixes: Signal::new_in_scope(None, ScopeId::ROOT),
            track: Signal::new_in_scope(None, ScopeId::ROOT),
            fix: Signal::new_in_scope(None, ScopeId::ROOT),
            speed: Signal::new_in_scope(None, ScopeId::ROOT),
            g: Signal::new_in_scope(None, ScopeId::ROOT),
            live: Signal::new_in_scope(None, ScopeId::ROOT),
            status: Signal::new_in_scope(None, ScopeId::ROOT),
            permission: Signal::new_in_scope(
                device.map(|(_, permission)| permission),
                ScopeId::ROOT,
            ),
            use_mock: Signal::new_in_scope(!cfg!(target_os = "android"), ScopeId::ROOT),
            force_location_manager: Signal::new_in_scope(false, ScopeId::ROOT),
            guide: Signal::new_in_scope(check.filter(|c| c.killed).map(|_| Guide::Killed), ScopeId::ROOT),
            guide_skipped: Signal::new_in_scope(false, ScopeId::ROOT),
            strict: check.is_some_and(|c| c.strict),
            #[cfg(target_os = "android")]
            device: device.map(|(gps, _)| gps),
            #[cfg(target_os = "android")]
            background,
            #[cfg(target_os = "android")]
            imu: AndroidImu::new().map(|imu| &*Box::leak(Box::new(imu))),
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
        let sources = match self.device_fixes().await? {
            Some(fixes) => Sources { fixes, imu: self.device_imu(), clock: Default::default(), alignment: storage::last_alignment() },
            None => self.mock_sources(track.clone()),
        };
        let reference = track.as_deref().and_then(storage::reference);
        let started_at = Utc::now();
        let (saved, journal) = storage::begin(started_at, track.as_deref()).unzip();
        let recording = Recorder::start(sources, track.clone(), reference, false, started_at, journal);
        self.last_summary.set(None);
        self.saved.set(saved);
        self.run(recording, track);
        Ok(())
    }

    /// Shows the guide instead of recording, if this is a strict phone where Traced isn't allowed in the background.
    pub fn guide_on_start(mut self) -> bool {
        let show = self.strict && !*self.use_mock.peek() && !*self.guide_skipped.peek() && !self.exempt();
        if show {
            self.guide.set(Some(Guide::Start));
        }
        show
    }

    /// Closes the guide, unless it's still needed (`false`).
    pub fn guide_done(mut self) -> bool {
        let done = *self.guide.peek() == Some(Guide::Killed) || self.exempt();
        if done {
            self.guide.set(None);
        }
        done
    }

    /// Closes the guide even if it's still needed, until the app restarts.
    pub fn skip_guide(mut self) {
        self.guide_skipped.set(true);
        self.guide.set(None);
    }

    /// Whether the recording's notification still needs the user's permission (Android 13+).
    pub fn needs_notifications(self) -> bool {
        #[cfg(target_os = "android")]
        return self.background.is_some_and(|b| b.check().is_ok_and(|c| !c.notifications));
        #[cfg(not(target_os = "android"))]
        false
    }

    /// Shows the system's notification permission dialog, if it's still needed.
    pub async fn request_notifications(self) {
        #[cfg(target_os = "android")]
        if let Some(b) = self.background {
            if let Ok(Err(e)) = tokio::task::spawn_blocking(move || b.request_notifications()).await {
                warn!("notification permission: {e}");
            }
        }
    }

    fn exempt(self) -> bool {
        #[cfg(target_os = "android")]
        return self.background.is_none_or(|b| b.check().is_ok_and(|c| c.exempt));
        #[cfg(not(target_os = "android"))]
        true
    }

    /// Plays saved recording `id` back on the live screen (only around its `timed` part, if given),
    /// timed against `track` again; not saved. `false` if it has no fixes.
    pub fn replay(mut self, id: Uuid, track: Option<Arc<Track>>, timed: Option<(DateTime<Utc>, DateTime<Utc>)>) -> bool {
        let Some((fixes, samples)) = storage::replay_data(id) else {
            return false;
        };
        let reference = track.as_deref().and_then(storage::reference);
        self.replaying.set(Some(id));
        self.replay_fixes.set(Some(fixes.as_slice().into()));
        let sources = Sources::replay(fixes, samples, 1.0, timed);
        self.run(Recorder::start(sources, track.clone(), reference, false, Utc::now(), None), track);
        true
    }

    /// Traces a drive or walk to make a track from, on the device GPS (asking for the location
    /// permission if needed) or the mock driving the first preset.
    pub async fn capture(mut self) -> Result<(), GpsError> {
        let sources = match self.device_fixes().await? {
            Some(fixes) => fixes.into(),
            None => self.mock_sources(None),
        };
        let capture = Recorder::start(sources, None, None, true, Utc::now(), None);
        self.show(&capture);
        bridge(capture.trace.clone().expect("tracing"), self.trace, Some);
        self.capture.set(Some(capture));
        self.set_recording(true);
        Ok(())
    }

    /// Stops the capture; its path's waypoints, if one was running.
    pub async fn end_capture(mut self) -> Option<Vec<Waypoint>> {
        let capture = self.capture.write().take()?;
        self.set_recording(false);
        capture.stop().await.trace
    }

    pub fn stop(mut self) {
        let Some(recording) = self.recording.write().take() else {
            return;
        };
        self.set_recording(false);
        if self.replaying.write().take().is_some() {
            self.replay_fixes.set(None);
            return; // dropping it ends the replay
        }
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
    fn run(mut self, recording: Recording, track: Option<Arc<Track>>) {
        self.show(&recording);
        if let Some(live) = recording.session.clone() {
            bridge(live, self.live, Some);
        }
        self.track.set(track);
        self.recording.set(Some(recording));
        self.set_recording(true);
    }

    /// Mirrors the fixes and speedometer of `recording` into their signals.
    fn show(self, recording: &Recording) {
        bridge(recording.fix.clone(), self.fix, identity);
        bridge(recording.speedometer.clone(), self.speed, Some);
        if let Some(g) = recording.g.clone() {
            bridge(g, self.g, Some);
        }
    }

    /// Drives the chosen track (or the first preset) on the mock GPS and accelerometer.
    fn mock_sources(self, track: Option<Arc<Track>>) -> Sources {
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
        Sources { fixes: gps.stream_fixes(), imu: Some(gps.imu()), clock: Default::default(), alignment: storage::last_alignment() }
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

    /// Shows the system's location permission dialog, if the device GPS still needs it.
    pub async fn request_location(self) {
        #[cfg(target_os = "android")]
        if let Some(gps) = self.device {
            match gps.request_permission().await {
                Ok(permission) => { let mut signal = self.permission; signal.set(Some(permission)) }
                Err(e) => warn!("location permission: {e}"),
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    async fn device_fixes(self) -> Result<Option<Fixes>, GpsError> {
        Ok(None)
    }

    /// The device accelerometer's samples, if it has one.
    fn device_imu(self) -> Option<BoxStream<'static, ImuSample>> {
        #[cfg(target_os = "android")]
        return self.imu.map(|imu| imu.stream());
        #[cfg(not(target_os = "android"))]
        None
    }

    fn set_recording(self, _on: bool) {
        #[cfg(target_os = "android")]
        if let Some(Err(e)) = self.background.map(|b| b.set_recording(_on)) {
            warn!("set recording: {e}");
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
