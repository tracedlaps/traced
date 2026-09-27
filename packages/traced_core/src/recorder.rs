use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::{StreamExt, stream::BoxStream};
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};

use crate::{
    gps::{GPSFix, GpsError},
    speedometer::{Speedometer, SpeedometerSnapshot},
    track::{Correlator, LiveSessionSnapshot, Session, Track},
};

/// A stateful accumulator fed one fix at a time; publishes its own snapshot.
pub trait FixConsumer {
    fn on_fix(&mut self, fix: GPSFix);
}

/// Sole owner of one recording's fix stream; forwards each fix to its consumers.
pub struct Recorder {
    fix: watch::Sender<Option<GPSFix>>,
    speedometer: Speedometer,
    correlator: Option<Correlator>,
    started_at: DateTime<Utc>,
    fixes: Vec<GPSFix>,
}

impl Recorder {
    /// Spawns the recording on the current tokio runtime. With a `track`, it's also timed.
    pub fn start(
        fixes: BoxStream<'static, Result<GPSFix, GpsError>>,
        track: Option<Arc<Track>>,
    ) -> Recording {
        let (speedometer, speed_rx) = Speedometer::new();
        let (correlator, session) = track.map(Correlator::new).unzip();
        let (fix, fix_rx) = watch::channel(None);
        let (stop, stop_rx) = oneshot::channel();
        Recording {
            fix: fix_rx,
            speedometer: speed_rx,
            session,
            stop,
            task: tokio::spawn(
                Self {
                    fix,
                    speedometer,
                    correlator,
                    started_at: Utc::now(),
                    fixes: Vec::new(),
                }
                .run(fixes, stop_rx),
            ),
        }
    }

    async fn run(
        mut self,
        mut fixes: BoxStream<'static, Result<GPSFix, GpsError>>,
        mut stop: oneshot::Receiver<()>,
    ) -> RecordingSummary {
        loop {
            tokio::select! {
                // Fires on `stop()` and when the `Recording` is dropped.
                _ = &mut stop => break,
                item = fixes.next() => match item {
                    Some(Ok(fix)) => {
                        self.fix.send_replace(Some(fix));
                        self.fixes.push(fix);
                        self.speedometer.on_fix(fix);
                        if let Some(c) = &mut self.correlator {
                            c.on_fix(fix);
                        }
                    }
                    Some(Err(e)) => tracing::warn!("skipping GPS error: {e}"),
                    None => break,
                },
            }
        }
        RecordingSummary {
            started_at: self.started_at,
            fixes: self.fixes,
            session: self.correlator.map(Correlator::into_session),
            speedometer: self.speedometer.snapshot(),
        }
    }
}

/// Handle to a running recording. Dropping it stops the recording.
pub struct Recording {
    /// The latest raw fix, e.g. to show where you are; `None` until the first one.
    pub fix: watch::Receiver<Option<GPSFix>>,
    pub speedometer: watch::Receiver<SpeedometerSnapshot>,
    /// `None` when recording without a track.
    pub session: Option<watch::Receiver<LiveSessionSnapshot>>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<RecordingSummary>,
}

impl Recording {
    pub async fn stop(self) -> RecordingSummary {
        let _ = self.stop.send(()); // Err if the stream already ended
        self.task.await.expect("recorder task panicked")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordingSummary {
    /// Wall-clock time the recording started, not the first fix's.
    pub started_at: DateTime<Utc>,
    pub session: Option<Session>,
    pub speedometer: SpeedometerSnapshot,
    /// Every fix received, in order: enough to replay or re-time the recording later.
    pub fixes: Vec<GPSFix>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speedometer::tests::fix;
    use futures::stream;

    #[tokio::test]
    async fn stream_end_finishes_recording() {
        let fixes = stream::iter([
            Ok(fix(0, 10.0)),
            Err(GpsError::DeviceNotFound),
            Ok(fix(1, 10.0)),
        ]);
        let mut rec = Recorder::start(fixes.boxed(), None);
        while rec.speedometer.changed().await.is_ok() {} // closes when the stream ends
        let summary = rec.stop().await;
        assert_eq!(summary.speedometer.distance_m, 10.0);
        assert_eq!(summary.fixes, [fix(0, 10.0), fix(1, 10.0)]);
    }

    #[tokio::test]
    async fn stop_ends_endless_stream() {
        let rec = Recorder::start(stream::pending().boxed(), None);
        assert_eq!(rec.stop().await.speedometer, SpeedometerSnapshot::default());
    }

    #[tokio::test]
    async fn track_adds_session() {
        use crate::track::{TrackShape, Waypoint};
        let open = TrackShape::Open {
            start: Waypoint::new(-8.9995, 38.7),
            finish: Waypoint::new(-8.9985, 38.7),
        };
        let track = Track::builder("Line", open)
            .waypoints([Waypoint::new(-9.0, 38.7), Waypoint::new(-8.998, 38.7)])
            .build()
            .unwrap();
        let rec = Recorder::start(stream::pending().boxed(), Some(Arc::new(track)));
        assert!(rec.session.is_some());
        assert!(rec.stop().await.session.is_some());
    }

    #[tokio::test]
    async fn pushes_snapshots_then_closes() {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let mut rec = Recorder::start(rx.boxed(), None);
        tx.unbounded_send(Ok(fix(0, 12.0))).unwrap();
        rec.speedometer.changed().await.unwrap();
        assert_eq!(rec.speedometer.borrow().current_speed_ms, 12.0);
        assert_eq!(rec.fix.borrow().map(|f| f.speed_ms), Some(12.0));

        let mut speed = rec.speedometer.clone();
        drop(rec);
        assert!(speed.changed().await.is_err());
    }
}
