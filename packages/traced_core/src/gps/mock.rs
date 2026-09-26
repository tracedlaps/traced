use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream::{self, BoxStream, StreamExt};
use rand::RngExt;
use tokio::sync::watch;

use crate::gps::{FixSource, GPSFix, GpsProvider, GpsStatus};
use crate::track::{Boundary, Projection, Track};

#[derive(Debug)]
pub struct MockGPSConfig {
    pub interval: Duration,
    pub loop_fixes: bool,
    /// Max random offset (m) added to each fix's position, per axis, when sent.
    pub jitter_m: f64,
}

impl Default for MockGPSConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            loop_fixes: true,
            jitter_m: 0.0,
        }
    }
}

pub struct MockGPSProvider {
    config: MockGPSConfig,
    fixes: Vec<GPSFix>,
    last_fix: Arc<Mutex<GPSFix>>,
    status: watch::Sender<GpsStatus>,
}

impl GpsProvider for MockGPSProvider {
    fn init(&mut self) -> Result<(), super::GpsError> {
        tracing::info!("Initializing MockGPSProvider");
        self.status.send_replace(GpsStatus::Searching);
        Ok(())
    }

    fn deinit(&self) {
        tracing::info!("De-initializing MockGPSProvider");
    }

    fn stream_fixes(&self) -> BoxStream<'static, Result<super::GPSFix, super::GpsError>> {
        let (interval, jitter) = (self.config.interval, self.config.jitter_m);
        let fixes = self.fixes.clone();
        let last_fix = self.last_fix.clone();
        let status = self.status.clone();

        let iter: Box<dyn Iterator<Item = GPSFix> + Send> = if self.config.loop_fixes {
            Box::new(fixes.into_iter().cycle())
        } else {
            Box::new(fixes.into_iter())
        };

        stream::iter(iter)
            .then(move |fix| async move {
                tokio::time::sleep(interval).await;
                // Stamped on send, like a real device, so looped fixes keep moving forward in time.
                Ok(GPSFix { timestamp: chrono::Utc::now(), ..jittered(fix, jitter) })
            })
            .inspect(move |result| {
                if let Ok(fix) = result {
                    *last_fix.lock().unwrap() = *fix;
                    status.send_replace(GpsStatus::Fixed { satellites: 8, accuracy_m: fix.accuracy, source: FixSource::Mock });
                }
            })
            .boxed()
    }

    fn get_fix(&self) -> Option<super::GPSFix> {
        Some(*self.last_fix.lock().unwrap())
    }

    fn status(&self) -> watch::Receiver<GpsStatus> {
        self.status.subscribe()
    }
}

impl MockGPSProvider {
    pub fn new(fixes: Vec<GPSFix>, config: MockGPSConfig) -> Self {
        Self {
            config,
            fixes,
            last_fix: Arc::new(Mutex::new(GPSFix::default())),
            status: watch::Sender::new(GpsStatus::default()),
        }
    }

    pub fn random(samples: u8, config: MockGPSConfig) -> Self {
        let mut rng = rand::rng();
        let fixes = (0..samples)
            .map(|_| GPSFix {
                latitude: rng.random_range(-90.0..=90.0),
                longitude: rng.random_range(-180.0..=180.0),
                speed_ms: rng.random_range(0.0..30.0),
                altitude_m: Some(rng.random_range(0.0..3000.0)),
                heading_deg: Some(rng.random_range(0.0..360.0)),
                accuracy: rng.random_range(1.0..20.0),
                ..Default::default()
            })
            .collect();
        Self::new(fixes, config)
    }

    /// Drives `track` at a constant ~`speed_ms`, one fix per `config.interval`: endless laps
    /// on a Closed track (overrides `loop_fixes`), a single pass on an Open one.
    /// Speed is nudged so a lap/run is a whole number of fixes, which keeps its time exact.
    /// Fixes sit half a fix either side of each gate, so the first fix is just before the
    /// start line and an Open run carries straight on past its gates' ends.
    pub fn drive(track: &Track, speed_ms: f64, mut config: MockGPSConfig) -> Self {
        let length = track.length();
        let (start, span) = match &track.boundary {
            Boundary::Closed { gate } => (gate.distance_along_track, length),
            Boundary::Open { start, finish } => (start.distance_along_track, finish.distance_along_track - start.distance_along_track),
        };
        let n = (span / (speed_ms * config.interval.as_secs_f64())).round().max(1.0) as usize;
        let step = span / n as f64;
        let at = |d: f64| match &track.boundary {
            Boundary::Closed { .. } => track.point_at(d.rem_euclid(length)),
            Boundary::Open { start, finish } => {
                // Run-up/run-out: straight on along the nearest boundary gate's direction.
                let gate = match d {
                    d if d < start.distance_along_track => start,
                    d if d > finish.distance_along_track => finish,
                    d => return track.point_at(d),
                };
                track.projection.unproject((gate.center.0 + gate.direction * (d - gate.distance_along_track)).into())
            }
        };
        // A Closed lap's last point is its first; an Open run adds its run-up and run-out.
        let closed = matches!(track.boundary, Boundary::Closed { .. });
        let points: Vec<_> = (0..n + 2 * usize::from(!closed)).map(|i| at(start + (i as f64 - 0.5) * step)).collect();
        let fixes = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let next = points.get(i + 1).or(closed.then_some(&points[0])).unwrap_or(p);
                let d = track.projection.project(*next) - track.projection.project(*p);
                GPSFix {
                    latitude: p.y(),
                    longitude: p.x(),
                    speed_ms: (step / config.interval.as_secs_f64()) as f32,
                    heading_deg: Some(d.x().atan2(d.y()).to_degrees().rem_euclid(360.0) as f32),
                    accuracy: config.jitter_m.max(1.0) as f32,
                    ..Default::default()
                }
            })
            .collect();
        config.loop_fixes = closed;
        Self::new(fixes, config)
    }
}

/// `fix` moved by up to `jitter_m` metres on each axis.
fn jittered(fix: GPSFix, jitter_m: f64) -> GPSFix {
    if jitter_m <= 0.0 {
        return fix;
    }
    let mut rng = rand::rng();
    let offset = geo::Point::new(rng.random_range(-jitter_m..=jitter_m), rng.random_range(-jitter_m..=jitter_m));
    let p = Projection::new(geo::Point::new(fix.longitude, fix.latitude)).unproject(offset);
    GPSFix { latitude: p.y(), longitude: p.x(), ..fix }
}

impl Default for MockGPSProvider {
    fn default() -> Self {
        Self::random(10, MockGPSConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::FixConsumer;
    use crate::track::{Completed, Correlator, TrackShape, Waypoint};
    use chrono::{DateTime, TimeDelta};
    use futures::StreamExt;

    fn lisbon_loop() -> Track {
        Track::builder("Loop", TrackShape::Closed { gate: Waypoint::new(-8.999, 38.7) })
            .waypoints([
                Waypoint::new(-9.0, 38.7),
                Waypoint::new(-8.998, 38.7),
                Waypoint::new(-8.998, 38.701),
                Waypoint::new(-9.0, 38.701),
            ])
            .build()
            .unwrap()
    }

    #[test]
    fn drive_times_whole_laps() {
        let track = lisbon_loop();
        let mock = MockGPSProvider::drive(&track, 10.0, MockGPSConfig::default());
        let n = mock.fixes.len();
        assert_eq!(n, (track.length() / 10.0).round() as usize);
        assert!(mock.config.loop_fixes);

        // Starts just before the gate: two laps and a bit, one fix per second.
        let (mut c, _rx) = Correlator::new(std::sync::Arc::new(track));
        for (i, fix) in mock.fixes.iter().cycle().take(2 * n + 2).enumerate() {
            c.on_fix(GPSFix { timestamp: DateTime::UNIX_EPOCH + TimeDelta::seconds(i as i64), ..*fix });
        }
        let Completed::Laps(laps) = c.into_session().completed else { panic!("closed track") };
        assert_eq!(laps.len(), 2);
        let first = laps[0].crossings()[0] - DateTime::UNIX_EPOCH;
        assert!((first - TimeDelta::milliseconds(500)).abs() < TimeDelta::milliseconds(1));
        for lap in &laps {
            assert!((lap.duration() - TimeDelta::seconds(n as i64)).abs() < TimeDelta::milliseconds(1));
        }
    }

    #[test]
    fn drive_times_an_open_run() {
        // 200 m straight east with gates at both ends: the run needs a run-up and a run-out.
        let open = TrackShape::Open { start: Waypoint::new(-9.0, 38.7), finish: Waypoint::new(-8.99769, 38.7) };
        let track = Track::builder("Line", open)
            .waypoints([Waypoint::new(-9.0, 38.7), Waypoint::new(-8.99769, 38.7)])
            .build()
            .unwrap();
        let mock = MockGPSProvider::drive(&track, 10.0, MockGPSConfig::default());
        assert!(!mock.config.loop_fixes);

        let (mut c, _rx) = Correlator::new(std::sync::Arc::new(track));
        for (i, fix) in mock.fixes.iter().enumerate() {
            c.on_fix(GPSFix { timestamp: DateTime::UNIX_EPOCH + TimeDelta::seconds(i as i64), ..*fix });
        }
        let Completed::Run(Some(run)) = c.into_session().completed else { panic!("no run") };
        assert!((run.duration() - TimeDelta::seconds(mock.fixes.len() as i64 - 2)).abs() < TimeDelta::milliseconds(1));
    }

    #[test]
    fn drive_heads_along_the_track() {
        let mock = MockGPSProvider::drive(&lisbon_loop(), 10.0, MockGPSConfig::default());
        // First leg runs due east.
        assert!((mock.fixes[0].heading_deg.unwrap() - 90.0).abs() < 1e-3);
    }

    #[tokio::test]
    async fn stream_stamps_and_jitters_on_send() {
        let config = MockGPSConfig { interval: Duration::from_millis(1), loop_fixes: true, jitter_m: 5.0 };
        let fix = GPSFix { latitude: 38.7, longitude: -9.0, ..Default::default() };
        let sent: Vec<_> = MockGPSProvider::new(vec![fix], config).stream_fixes().take(3).map(Result::unwrap).collect().await;
        assert!(sent.windows(2).all(|w| w[0].timestamp < w[1].timestamp));
        let origin = Projection::new(geo::Point::new(-9.0, 38.7));
        for s in &sent {
            let d = origin.project(geo::Point::new(s.longitude, s.latitude));
            assert!(d.x().abs() <= 5.0 + 1e-6 && d.y().abs() <= 5.0 + 1e-6);
        }
    }
}
