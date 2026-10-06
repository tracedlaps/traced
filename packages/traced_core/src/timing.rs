//! Comparing laps against the best one on the same track.

use chrono::TimeDelta;
use geo::{Coord, Point};
use uuid::Uuid;

use crate::{
    gps::GPSFix,
    store::SavedRecording,
    track::{Lap, Track},
};

/// The fastest lap (or run) on `track_id` across `recordings`, with its recording's id.
/// Laps with a GPS gap don't count: their crossings are unreliable.
pub fn best_lap(recordings: &[SavedRecording], track_id: Uuid) -> Option<(Uuid, &Lap)> {
    recordings
        .iter()
        .filter_map(|r| r.session.as_ref().filter(|s| s.track_id == track_id).map(|s| (r.id, s)))
        .flat_map(|(id, s)| s.laps().iter().map(move |lap| (id, lap)))
        .filter(|(_, lap)| !lap.gps_gap)
        .min_by_key(|(_, lap)| lap.duration())
}

/// A lap to race against: the time it took to get each distance past the start gate.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub lap: Lap,
    /// (distance past the start gate, time since the start crossing), both strictly increasing.
    trace: Vec<(f64, TimeDelta)>,
}

impl Reference {
    /// `fixes` may span the whole recording; only those during `lap` are used.
    pub fn new(track: &Track, lap: Lap, fixes: &[GPSFix]) -> Self {
        let start = lap.crossings()[0];
        let length = track.lap_length();
        let mut trace = vec![(0.0, TimeDelta::zero())];
        for (d, f) in along_lap(track, &lap, fixes) {
            // Jitter steps back, and interpolation needs the distances increasing.
            if trace[trace.len() - 1].0 < d && d < length {
                trace.push((d, f.timestamp - start));
            }
        }
        trace.push((length, lap.duration()));
        Self { lap, trace }
    }

    /// The time the reference took to get `d` metres past the start gate, clamped to the lap.
    pub fn time_at(&self, d: f64) -> TimeDelta {
        let i = self.trace.partition_point(|(x, _)| *x < d).clamp(1, self.trace.len() - 1);
        let ((d0, t0), (d1, t1)) = (self.trace[i - 1], self.trace[i]);
        let frac = ((d - d0) / (d1 - d0)).clamp(0.0, 1.0);
        t0 + TimeDelta::nanoseconds(((t1 - t0).num_nanoseconds().unwrap_or(0) as f64 * frac) as i64)
    }
}

/// `lap`'s fixes, each with its distance past the start gate (m). `fixes` may span the whole recording.
pub fn along_lap(track: &Track, lap: &Lap, fixes: &[GPSFix]) -> Vec<(f64, GPSFix)> {
    let (start, end) = (lap.crossings()[0], lap.crossings()[lap.crossings().len() - 1]);
    let mut progress = Progress::new(track);
    fixes
        .iter()
        .filter(|f| start < f.timestamp && f.timestamp < end)
        .map(|&f| (progress.advance(track, track.projection.project(Point::new(f.longitude, f.latitude)).0), f))
        .collect()
}

/// Distance travelled past the start gate, following fixes along the track. Unwraps a
/// Closed loop's seam; jitter can step it back.
pub(crate) struct Progress {
    /// Where the last fix was located on the polyline.
    at: f64,
    travelled: f64,
}

impl Progress {
    /// At the start gate.
    pub(crate) fn new(track: &Track) -> Self {
        let start = track.gates_ordered().next().expect("a track has a start gate");
        Self { at: start.distance_along_track, travelled: 0.0 }
    }

    /// Moves to `p` (projected), returning the distance travelled.
    pub(crate) fn advance(&mut self, track: &Track, p: Coord) -> f64 {
        let d = track.locate(p, Some(self.at));
        let half = track.length() / 2.0;
        self.travelled += (d - self.at + half).rem_euclid(2.0 * half) - half;
        self.at = d;
        self.travelled
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::{
        recorder::FixConsumer,
        track::{
            Completed, Correlator, Session,
            correlator::tests::{around, fix, square},
        },
    };

    fn secs(s: f64) -> TimeDelta {
        TimeDelta::milliseconds((s * 1000.0) as i64)
    }

    fn lap(s: &[f64]) -> Lap {
        let at = s.iter().map(|&s| DateTime::<Utc>::UNIX_EPOCH + secs(s)).collect();
        Lap::new(at, false)
    }

    fn saved(track_id: Uuid, laps: Vec<Lap>) -> SavedRecording {
        let session = Session { track_id, completed: Completed::Laps(laps) };
        SavedRecording {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            session: Some(session),
            speedometer: Default::default(),
            g: None,
            interrupted: false,
        }
    }

    #[test]
    fn best_lap_is_the_fastest_clean_one_on_the_track() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut gap = lap(&[0.0, 40.0]);
        gap.gps_gap = true;
        let mut no_track = saved(a, vec![]);
        no_track.session = None;
        let recordings = [
            saved(a, vec![lap(&[0.0, 42.0]), gap]),
            saved(b, vec![lap(&[0.0, 30.0])]),
            no_track,
            saved(a, vec![lap(&[0.0, 45.0]), lap(&[45.0, 86.0])]),
        ];
        let (id, best) = best_lap(&recordings, a).unwrap();
        assert_eq!((id, best.duration()), (recordings[3].id, secs(41.0)));
        assert!(best_lap(&recordings, Uuid::new_v4()).is_none());
    }

    /// Laps the 400 m square (gate 50 m along) from `(s, t)` samples, as the Correlator does.
    fn reference(samples: impl IntoIterator<Item = (f64, f64)>) -> Reference {
        let track = square(&[]);
        let fixes: Vec<_> = samples.into_iter().map(|(s, t)| fix(around(s), t)).collect();
        let (mut c, _rx) = Correlator::new(track.clone(), None);
        fixes.iter().for_each(|&f| c.on_fix(f));
        let lap = c.into_session().laps()[0].clone();
        Reference::new(&track, lap, &fixes)
    }

    fn assert_near(got: TimeDelta, want: f64) {
        assert!((got - secs(want)).abs() < TimeDelta::milliseconds(1), "{got} != {want}s");
    }

    #[test]
    fn reference_interpolates_time_by_distance() {
        // 10 m/s for 250 m, then 5 m/s: crosses the gate at 5 s and again at 65 s.
        let t = |s: f64| if s <= 250.0 { s / 10.0 } else { 25.0 + (s - 250.0) / 5.0 };
        let r = reference((0..48).map(|i| 25.0 + 10.0 * i as f64).map(|s| (s, t(s))));
        assert_near(r.lap.duration(), 60.0);
        for (d, want) in [(-5.0, 0.0), (0.0, 0.0), (105.0, 10.5), (300.0, 40.0), (400.0, 60.0), (500.0, 60.0)] {
            assert_near(r.time_at(d), want);
        }
    }

    #[test]
    fn reference_skips_fixes_that_step_backwards() {
        // Jitter just past the gate: a fix behind the last one.
        let r = reference([(40.0, 4.0), (60.0, 6.0), (57.0, 6.5), (70.0, 7.0)].into_iter().chain(
            (0..40).map(|i| 80.0 + 10.0 * i as f64).map(|s| (s, s / 10.0)),
        ));
        assert!(r.trace.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1), "{:?}", r.trace);
        assert_near(r.time_at(20.0), 2.0);
    }
}
