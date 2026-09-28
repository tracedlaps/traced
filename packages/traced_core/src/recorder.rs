use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use futures::{StreamExt, stream::{self, BoxStream}};
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};

use crate::{
    clock::Clock,
    gps::{GPSFix, GpsError},
    imu::{Alignment, GForceSnapshot, GForces, GMeter, ImuSample},
    speedometer::{Speedometer, SpeedometerSnapshot},
    timing::Reference,
    track::{Correlator, LiveSessionSnapshot, Session, Track, Waypoint, trace::Trace},
};

/// A stateful accumulator fed one fix at a time; publishes its own snapshot.
pub trait FixConsumer {
    fn on_fix(&mut self, fix: GPSFix);
    /// An accelerometer sample; most consumers only want fixes.
    fn on_sample(&mut self, _sample: ImuSample) {}
}

/// What a recording listens to.
pub struct Sources {
    pub fixes: BoxStream<'static, Result<GPSFix, GpsError>>,
    /// The accelerometer, if there is one.
    pub imu: Option<BoxStream<'static, ImuSample>>,
    /// The time they arrive on: the device's, or a replay's.
    pub clock: Clock,
    /// Where an earlier recording found the phone sat in its mount, to start from.
    pub alignment: Option<Alignment>,
}

impl From<BoxStream<'static, Result<GPSFix, GpsError>>> for Sources {
    fn from(fixes: BoxStream<'static, Result<GPSFix, GpsError>>) -> Self {
        Self { fixes, imu: None, clock: Clock::default(), alignment: None }
    }
}

impl Sources {
    /// Saved fixes and samples again, each when it came in, on a replay clock at `speed`×.
    /// With `timed` (from, to), what came in until shortly before `from` and after `to` is sent
    /// at once: skipped, yet still seen (so distance counts it and a gate crossed at `from` is detected).
    pub fn replay(fixes: Vec<GPSFix>, samples: Vec<ImuSample>, speed: f64, timed: Option<(DateTime<Utc>, DateTime<Utc>)>) -> Self {
        /// Played either side of `timed`, to see it coming and going.
        const LEAD: TimeDelta = TimeDelta::seconds(3);
        fn paced<T: Send + 'static>(
            clock: &Clock,
            (t0, t1): (DateTime<Utc>, DateTime<Utc>),
            items: Vec<T>,
            at: fn(&T) -> DateTime<Utc>,
        ) -> BoxStream<'static, T> {
            let (clock, start) = (clock.clone(), clock.now());
            stream::iter(items)
                .then(move |item| {
                    let (clock, wait) = (clock.clone(), (at(&item).min(t1) - t0).to_std().unwrap_or_default());
                    async move {
                        clock.sleep_until(start + wait).await;
                        item
                    }
                })
                .boxed()
        }
        let clock = Clock::replay(speed);
        let first = fixes.iter().map(|f| f.timestamp).chain(samples.iter().map(|s| s.timestamp)).min().unwrap_or_default();
        let window = timed.map_or((first, DateTime::<Utc>::MAX_UTC), |(from, to)| (from - LEAD, to + LEAD));
        Self {
            fixes: paced(&clock, window, fixes, |f| f.timestamp).map(Ok).boxed(),
            imu: (!samples.is_empty()).then(|| paced(&clock, window, samples, |s| s.timestamp)),
            clock,
            alignment: None,
        }
    }
}

/// Sole owner of one recording's fix stream; forwards each fix to its consumers.
pub struct Recorder {
    fix: watch::Sender<Option<GPSFix>>,
    speedometer: Speedometer,
    correlator: Option<Correlator>,
    trace: Option<Trace>,
    g_meter: Option<GMeter>,
    /// Sees each fix too, e.g. to save it as it arrives.
    journal: Option<Box<dyn FixConsumer + Send>>,
    started_at: DateTime<Utc>,
    fixes: Vec<GPSFix>,
    samples: Vec<ImuSample>,
}

impl Recorder {
    /// Spawns the recording on the current tokio runtime. With a `track`, it's also timed,
    /// against `reference` if there is one. With `trace`, its path is traced to author a track from.
    pub fn start(
        sources: impl Into<Sources>,
        track: Option<Arc<Track>>,
        reference: Option<Reference>,
        trace: bool,
        started_at: DateTime<Utc>,
        journal: Option<Box<dyn FixConsumer + Send>>,
    ) -> Recording {
        let (speedometer, speed_rx) = Speedometer::new();
        let sources = sources.into();
        let (correlator, session) = track.map(|t| Correlator::new(t, reference)).unzip();
        let correlator = correlator.map(|c| c.on(sources.clock.clone()));
        let (trace, trace_rx) = trace.then(Trace::new).unzip();
        let (mut g_meter, g_rx) = sources.imu.is_some().then(GMeter::new).unzip();
        if let (Some(g), Some(a)) = (&mut g_meter, sources.alignment) {
            g.resume(a);
        }
        let (fix, fix_rx) = watch::channel(None);
        let (stop, stop_rx) = oneshot::channel();
        Recording {
            clock: sources.clock.clone(),
            fix: fix_rx,
            speedometer: speed_rx,
            session,
            trace: trace_rx,
            g: g_rx,
            stop,
            task: tokio::spawn(
                Self {
                    fix,
                    speedometer,
                    correlator,
                    trace,
                    g_meter,
                    journal,
                    started_at,
                    fixes: Vec::new(),
                    samples: Vec::new(),
                }
                .run(sources, stop_rx),
            ),
        }
    }

    async fn run(
        mut self,
        Sources { mut fixes, imu, .. }: Sources,
        mut stop: oneshot::Receiver<()>,
    ) -> RecordingSummary {
        // Fused: it may end before the fixes do.
        let mut imu = stream::iter(imu).flatten().fuse();
        loop {
            tokio::select! {
                // Fires on `stop()` and when the `Recording` is dropped.
                _ = &mut stop => break,
                Some(sample) = imu.next() => {
                    self.samples.push(sample);
                    if let Some(g) = &mut self.g_meter {
                        g.on_sample(sample);
                    }
                    if let Some(j) = &mut self.journal {
                        j.on_sample(sample);
                    }
                }
                item = fixes.next() => match item {
                    Some(Ok(fix)) => {
                        self.fix.send_replace(Some(fix));
                        self.fixes.push(fix);
                        self.speedometer.on_fix(fix);
                        if let Some(c) = &mut self.correlator {
                            c.on_fix(fix);
                        }
                        if let Some(t) = &mut self.trace {
                            t.on_fix(fix);
                        }
                        if let Some(g) = &mut self.g_meter {
                            g.on_fix(fix);
                        }
                        if let Some(j) = &mut self.journal {
                            j.on_fix(fix);
                        }
                    }
                    Some(Err(e)) => tracing::warn!("skipping GPS error: {e}"),
                    None => break,
                },
            }
        }
        let session = self.correlator.map(Correlator::into_session);
        RecordingSummary {
            started_at: self.started_at,
            fixes: self.fixes,
            g: self.g_meter.map(|g| g.into_forces(session.as_ref())),
            samples: self.samples,
            session,
            trace: self.trace.map(Trace::into_waypoints),
            speedometer: self.speedometer.snapshot(),
        }
    }
}

/// Handle to a running recording. Dropping it stops the recording.
pub struct Recording {
    /// What its lap clocks read on; set a replay's speed through it.
    pub clock: Clock,
    /// The latest raw fix, e.g. to show where you are; `None` until the first one.
    pub fix: watch::Receiver<Option<GPSFix>>,
    pub speedometer: watch::Receiver<SpeedometerSnapshot>,
    /// `None` when recording without a track.
    pub session: Option<watch::Receiver<LiveSessionSnapshot>>,
    /// The simplified path so far; `None` unless tracing.
    pub trace: Option<watch::Receiver<Arc<[Waypoint]>>>,
    /// `None` without an accelerometer.
    pub g: Option<watch::Receiver<GForceSnapshot>>,
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
    /// The traced path's waypoints, when tracing.
    pub trace: Option<Vec<Waypoint>>,
    /// Every fix received, in order: enough to replay or re-time the recording later.
    pub fixes: Vec<GPSFix>,
    /// `None` without an accelerometer.
    pub g: Option<GForces>,
    /// Every accelerometer sample received, in order.
    pub samples: Vec<ImuSample>,
}

impl RecordingSummary {
    /// Re-derives a recording from its fixes and samples, as if they had just come in live
    /// (in time order, so g-forces may differ a hair from live, where they arrive in batches).
    pub fn replay(
        started_at: DateTime<Utc>,
        fixes: Vec<GPSFix>,
        samples: Vec<ImuSample>,
        track: Option<Arc<Track>>,
    ) -> Self {
        let (mut speedometer, _) = Speedometer::new();
        let mut correlator = track.map(|t| Correlator::new(t, None).0);
        let mut g_meter = (!samples.is_empty()).then(|| GMeter::new().0);
        let mut pending = samples.iter().peekable();
        for &fix in &fixes {
            if let Some(g) = &mut g_meter {
                while let Some(&s) = pending.next_if(|s| s.timestamp <= fix.timestamp) {
                    g.on_sample(s);
                }
                g.on_fix(fix);
            }
            speedometer.on_fix(fix);
            if let Some(c) = &mut correlator {
                c.on_fix(fix);
            }
        }
        if let Some(g) = &mut g_meter {
            pending.for_each(|&s| g.on_sample(s));
        }
        let session = correlator.map(Correlator::into_session);
        Self {
            started_at,
            g: g_meter.map(|g| g.into_forces(session.as_ref())),
            session,
            speedometer: speedometer.snapshot(),
            trace: None,
            fixes,
            samples,
        }
    }
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
        let mut rec = Recorder::start(fixes.boxed(), None, None, false, Utc::now(), None);
        while rec.speedometer.changed().await.is_ok() {} // closes when the stream ends
        let summary = rec.stop().await;
        assert_eq!(summary.speedometer.distance_m, 10.0);
        assert_eq!(summary.fixes, [fix(0, 10.0), fix(1, 10.0)]);
    }

    #[tokio::test]
    async fn stop_ends_endless_stream() {
        let rec = Recorder::start(stream::pending().boxed(), None, None, false, Utc::now(), None);
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
        let rec = Recorder::start(stream::pending().boxed(), Some(Arc::new(track)), None, false, Utc::now(), None);
        assert!(rec.session.is_some());
        assert!(rec.stop().await.session.is_some());
    }

    #[tokio::test]
    async fn trace_publishes_the_path_and_hands_back_its_waypoints() {
        use crate::track::correlator::tests::fix;
        let fixes = (0..=10).map(|i| Ok(fix((i as f64 * 10.0, 0.0), i as f64)));
        let mut rec = Recorder::start(stream::iter(fixes).boxed(), None, None, true, Utc::now(), None);
        let mut trace = rec.trace.clone().unwrap();
        while rec.speedometer.changed().await.is_ok() {}
        assert_eq!(trace.borrow_and_update().len(), 2);
        assert_eq!(rec.stop().await.trace.map(|w| w.len()), Some(2));
    }

    struct Collect(Arc<std::sync::Mutex<Vec<GPSFix>>>);
    impl FixConsumer for Collect {
        fn on_fix(&mut self, fix: GPSFix) {
            self.0.lock().unwrap().push(fix);
        }
    }

    #[tokio::test]
    async fn journal_sees_every_fix_and_replaying_them_reproduces_the_recording() {
        use crate::track::{Completed, correlator::tests::{around, fix, square}};
        let track = square(&[(100.0, 50.0)]);
        let fixes: Vec<_> = (0..100).map(|i| fix(around(i as f64 * 10.0), i as f64)).collect();
        let journal = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut rec = Recorder::start(
            stream::iter(fixes.clone().into_iter().map(Ok)).boxed(),
            Some(track.clone()),
            None,
            false,
            Utc::now(),
            Some(Box::new(Collect(journal.clone()))),
        );
        while rec.speedometer.changed().await.is_ok() {}
        let live = rec.stop().await;
        assert_eq!(*journal.lock().unwrap(), fixes);
        assert!(matches!(&live.session.as_ref().unwrap().completed, Completed::Laps(l) if l.len() == 2));
        let journaled = journal.lock().unwrap().clone();
        assert_eq!(RecordingSummary::replay(live.started_at, journaled, Vec::new(), Some(track)), live);
    }

    #[tokio::test]
    async fn replayed_sources_time_the_same_laps_on_their_clock() {
        use crate::track::correlator::tests::{around, fix, square};
        let track = square(&[(100.0, 50.0)]);
        let fixes: Vec<_> = (0..100).map(|i| fix(around(i as f64 * 10.0), i as f64)).collect();
        let saved = RecordingSummary::replay(Utc::now(), fixes.clone(), Vec::new(), Some(track.clone()));
        let (real, sources) = (std::time::Instant::now(), Sources::replay(fixes, Vec::new(), 1000.0, None));
        let mut rec = Recorder::start(sources, Some(track), None, false, saved.started_at, None);
        while rec.speedometer.changed().await.is_ok() {}
        assert!(real.elapsed() < std::time::Duration::from_secs(1)); // 99 s at 1000×
        assert!(rec.clock.now() > real + std::time::Duration::from_secs(90));
        assert_eq!(rec.stop().await, saved);
    }

    #[tokio::test]
    async fn replay_of_the_timed_part_skips_the_ends_but_still_sees_them() {
        use crate::track::correlator::tests::{around, fix, square};
        let track = square(&[(100.0, 50.0)]);
        // Parked for a minute, two laps, then parked another.
        let parked = |t0: f64| (0..60).map(move |i| fix(around(0.0), t0 + i as f64));
        let laps = (0..=80).map(|i| fix(around(i as f64 * 10.0), i as f64));
        let fixes: Vec<_> = parked(-60.0).chain(laps).chain(parked(81.0)).collect();
        let saved = RecordingSummary::replay(Utc::now(), fixes.clone(), Vec::new(), Some(track.clone()));
        let timed = saved.session.as_ref().and_then(Session::timed);
        let (real, sources) = (std::time::Instant::now(), Sources::replay(fixes, Vec::new(), 100.0, timed));
        let mut rec = Recorder::start(sources, Some(track), None, false, saved.started_at, None);
        while rec.speedometer.changed().await.is_ok() {}
        // ~86 s, 3 s either side of the laps, at 100×: not 200 s.
        assert!(real.elapsed() < std::time::Duration::from_millis(1300), "{:?}", real.elapsed());
        assert_eq!(rec.stop().await, saved);
    }

    #[tokio::test]
    async fn pushes_snapshots_then_closes() {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let mut rec = Recorder::start(rx.boxed(), None, None, false, Utc::now(), None);
        tx.unbounded_send(Ok(fix(0, 12.0))).unwrap();
        rec.speedometer.changed().await.unwrap();
        assert_eq!(rec.speedometer.borrow().current_speed_ms, 12.0);
        assert_eq!(rec.fix.borrow().map(|f| f.speed_ms), Some(12.0));

        let mut speed = rec.speedometer.clone();
        drop(rec);
        assert!(speed.changed().await.is_err());
    }
}
