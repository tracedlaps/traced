use std::{convert::identity, sync::Arc};

use dioxus::{core::spawn_forever, prelude::*};
use tokio::sync::watch;
use traced_core::{
    gps::{
        mock::{MockGPSConfig, MockGPSProvider},
        GPSFix, GpsProvider,
    },
    recorder::{Recorder, Recording, RecordingSummary},
    speedometer::SpeedometerSnapshot,
    track::{LiveSessionSnapshot, Track},
};

use crate::tracks::{Preset, TRACKS};

/// App-wide recording state. Components only read the snapshot signals; channels stay in here.
#[derive(Clone, Copy)]
pub struct RecorderCtx {
    pub recording: Signal<Option<Recording>>,
    pub last_summary: Signal<Option<RecordingSummary>>,
    /// The track of the current (or last) recording.
    pub track: Signal<Option<Arc<Track>>>,
    pub fix: Signal<Option<GPSFix>>,
    pub speed: Signal<Option<SpeedometerSnapshot>>,
    pub live: Signal<Option<LiveSessionSnapshot>>,
}

impl RecorderCtx {
    pub fn new() -> Self {
        // Owned by the root scope, where `spawn_forever` runs the tasks that write them.
        Self {
            recording: Signal::new_in_scope(None, ScopeId::ROOT),
            last_summary: Signal::new_in_scope(None, ScopeId::ROOT),
            track: Signal::new_in_scope(None, ScopeId::ROOT),
            fix: Signal::new_in_scope(None, ScopeId::ROOT),
            speed: Signal::new_in_scope(None, ScopeId::ROOT),
            live: Signal::new_in_scope(None, ScopeId::ROOT),
        }
    }

    /// Records against `preset`'s track, or with no track (speedometer only).
    pub fn start(mut self, preset: Option<&'static Preset>) {
        // Mock seam: drive the chosen track (or the first one) until a real provider exists.
        let driven = preset.unwrap_or(&TRACKS[0]);
        let track = preset.map(|p| p.track.clone());
        let gps = MockGPSProvider::drive(
            &driven.track,
            driven.mock_speed_kmh / 3.6,
            MockGPSConfig {
                jitter_m: 2.0,
                ..Default::default()
            },
        );
        let recording = Recorder::start(gps.stream_fixes(), track.clone());
        bridge(recording.fix.clone(), self.fix, identity);
        bridge(recording.speedometer.clone(), self.speed, Some);
        if let Some(live) = recording.session.clone() {
            bridge(live, self.live, Some);
        }
        self.track.set(track);
        self.last_summary.set(None);
        self.recording.set(Some(recording));
    }

    pub fn stop(mut self) {
        let Some(recording) = self.recording.write().take() else {
            return;
        };
        spawn_forever(async move { self.last_summary.set(Some(recording.stop().await)) });
    }
}

/// Copies each change into `signal` (through `into`), then clears it once the recording
/// drops the sender. `spawn_forever`: a component-scoped task would die when its screen unmounts.
fn bridge<T: Copy + 'static, U: 'static>(
    mut rx: watch::Receiver<T>,
    mut signal: Signal<Option<U>>,
    into: fn(T) -> Option<U>,
) {
    spawn_forever(async move {
        loop {
            signal.set(into(*rx.borrow_and_update()));
            if rx.changed().await.is_err() {
                break;
            }
        }
        signal.set(None);
    });
}
