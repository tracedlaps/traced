use std::sync::Arc;

use dioxus::{core::spawn_forever, prelude::*};
use tokio::sync::watch;
use traced_core::{
    gps::{
        mock::{MockGPSConfig, MockGPSProvider},
        GpsProvider,
    },
    recorder::{Recorder, Recording, RecordingSummary},
    speedometer::SpeedometerSnapshot,
    track::{LiveSessionSnapshot, Track},
};

use crate::tracks::TRACKS;

/// App-wide recording state. Components only read the snapshot signals; channels stay in here.
#[derive(Clone, Copy)]
pub struct RecorderCtx {
    pub recording: Signal<Option<Recording>>,
    pub last_summary: Signal<Option<RecordingSummary>>,
    pub speed: Signal<Option<SpeedometerSnapshot>>,
    pub live: Signal<Option<LiveSessionSnapshot>>,
}

impl RecorderCtx {
    pub fn new() -> Self {
        // Owned by the root scope, where `spawn_forever` runs the tasks that write them.
        Self {
            recording: Signal::new_in_scope(None, ScopeId::ROOT),
            last_summary: Signal::new_in_scope(None, ScopeId::ROOT),
            speed: Signal::new_in_scope(None, ScopeId::ROOT),
            live: Signal::new_in_scope(None, ScopeId::ROOT),
        }
    }

    pub fn start(mut self, track: Option<Arc<Track>>) {
        // Mock seam: drive the chosen track (or the first one) until a real provider exists.
        let driven = track.clone().unwrap_or_else(|| TRACKS[0].clone());
        let gps = MockGPSProvider::drive(
            &driven,
            10.0,
            MockGPSConfig {
                jitter_m: 2.0,
                ..Default::default()
            },
        );
        let recording = Recorder::start(gps.stream_fixes(), track);
        bridge(recording.speedometer.clone(), self.speed);
        if let Some(live) = recording.session.clone() {
            bridge(live, self.live);
        }
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

/// Copies each change into `signal`, then clears it once the recording drops the sender.
/// `spawn_forever`: a component-scoped task would die when its screen unmounts.
fn bridge<T: Copy + 'static>(mut rx: watch::Receiver<T>, mut signal: Signal<Option<T>>) {
    spawn_forever(async move {
        loop {
            signal.set(Some(*rx.borrow_and_update()));
            if rx.changed().await.is_err() {
                break;
            }
        }
        signal.set(None);
    });
}
