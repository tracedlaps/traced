use futures::{StreamExt, stream::BoxStream};
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};

use crate::{
    gps::{GPSFix, GpsError},
    speedometer::{Speedometer, SpeedometerSnapshot},
};

/// A stateful accumulator fed one fix at a time; publishes its own snapshot.
pub trait FixConsumer {
    fn on_fix(&mut self, fix: GPSFix);
}

/// Sole owner of one recording's fix stream; forwards each fix to its consumers.
pub struct Recorder {
    speedometer: Speedometer,
}

impl Recorder {
    /// Spawns the recording on the current tokio runtime.
    pub fn start(fixes: BoxStream<'static, Result<GPSFix, GpsError>>) -> Recording {
        let (speedometer, speed_rx) = Speedometer::new();
        let (stop, stop_rx) = oneshot::channel();
        Recording {
            speedometer: speed_rx,
            stop,
            task: tokio::spawn(Self { speedometer }.run(fixes, stop_rx)),
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
                    Some(Ok(fix)) => self.speedometer.on_fix(fix),
                    Some(Err(e)) => tracing::warn!("skipping GPS error: {e}"),
                    None => break,
                },
            }
        }
        RecordingSummary {
            speedometer: self.speedometer.snapshot(),
        }
    }
}

/// Handle to a running recording. Dropping it stops the recording.
pub struct Recording {
    pub speedometer: watch::Receiver<SpeedometerSnapshot>,
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
    pub speedometer: SpeedometerSnapshot,
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
        let mut rec = Recorder::start(fixes.boxed());
        while rec.speedometer.changed().await.is_ok() {} // closes when the stream ends
        let summary = rec.stop().await;
        assert_eq!(summary.speedometer.distance_m, 10.0);
    }

    #[tokio::test]
    async fn stop_ends_endless_stream() {
        let rec = Recorder::start(stream::pending().boxed());
        assert_eq!(rec.stop().await.speedometer, SpeedometerSnapshot::default());
    }

    #[tokio::test]
    async fn pushes_snapshots_then_closes() {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let mut rec = Recorder::start(rx.boxed());
        tx.unbounded_send(Ok(fix(0, 12.0))).unwrap();
        rec.speedometer.changed().await.unwrap();
        assert_eq!(rec.speedometer.borrow().current_speed_ms, 12.0);

        let mut speed = rec.speedometer.clone();
        drop(rec);
        assert!(speed.changed().await.is_err());
    }
}
