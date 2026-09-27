use std::{sync::Arc, time::Instant};

use chrono::{DateTime, TimeDelta, Utc};
use geo::{Coord, Point};
use tokio::sync::watch;

use super::{Boundary, Gate, Lap, Session, Track};
use crate::{
    gps::GPSFix,
    recorder::FixConsumer,
    timing::{Progress, Reference},
};

/// A longer gap between fixes makes interpolated crossings unreliable: the lap gets `gps_gap`.
const MAX_GAP: TimeDelta = TimeDelta::seconds(2);
/// Distance (m) to get from a gate before it can be crossed again, so jitter can't re-trigger it.
const REARM_DISTANCE: f64 = 30.0;

/// Deltas are against the reference lap: positive is behind it.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct LiveSessionSnapshot {
    pub status: LiveStatus,
    pub laps: usize,
    pub last_lap: Option<TimeDelta>,
    pub last_sector: Option<TimeDelta>,
    /// The reference lap's time: the best to beat.
    pub best: Option<TimeDelta>,
    /// The lap so far, at the latest fix; exact at each gate.
    pub delta: Option<TimeDelta>,
    pub last_lap_delta: Option<TimeDelta>,
    pub last_sector_delta: Option<TimeDelta>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum LiveStatus {
    /// Waiting for the first start-gate crossing.
    #[default]
    Armed,
    /// Anchors are the lap and current sector starts on the device clock:
    /// the UI shows their `elapsed()`.
    InProgress { lap_anchor: Instant, sector_anchor: Instant, sector: usize },
    Finished,
}

/// Detects forward gate crossings against an armed `Track`, filling its `Session`, and
/// compares the lap in progress against a `Reference`, replaced by any faster clean lap.
pub struct Correlator {
    track: Arc<Track>,
    session: Session,
    reference: Option<Reference>,
    tx: watch::Sender<LiveSessionSnapshot>,
    state: State,
    prev: Option<(Coord, DateTime<Utc>)>,
    /// Been `REARM_DISTANCE` from the last crossed gate since crossing it.
    rearmed: bool,
}

enum State {
    Armed,
    InProgress(Current),
    Finished,
}

/// The lap in progress.
struct Current {
    crossings: Vec<DateTime<Utc>>,
    gps_gap: bool,
    progress: Progress,
    /// Its fixes, to make it the reference if it's the best.
    fixes: Vec<GPSFix>,
}

impl Current {
    /// Crossing the start gate at `at`, seen by `fix` at `p`.
    fn new(track: &Track, at: DateTime<Utc>, gps_gap: bool, fix: GPSFix, p: Coord) -> Self {
        let mut progress = Progress::new(track);
        progress.advance(track, p);
        Self { crossings: vec![at], gps_gap, progress, fixes: vec![fix] }
    }
}

impl Correlator {
    pub fn new(track: Arc<Track>, reference: Option<Reference>) -> (Self, watch::Receiver<LiveSessionSnapshot>) {
        let best = reference.as_ref().map(|r| r.lap.duration());
        let (tx, rx) = watch::channel(LiveSessionSnapshot { best, ..Default::default() });
        let session = Session::new(&track);
        (Self { track, session, reference, tx, state: State::Armed, prev: None, rearmed: false }, rx)
    }

    /// Completed laps/run; a lap still in progress is discarded.
    pub fn into_session(self) -> Session {
        self.session
    }

    /// Between gates: the lap so far against the reference's time at the same distance.
    fn track_delta(&mut self, p: Coord, t: DateTime<Utc>) {
        let (State::InProgress(lap), Some(r)) = (&mut self.state, &self.reference) else { return };
        let delta = (t - lap.crossings[0]) - r.time_at(lap.progress.advance(&self.track, p));
        self.tx.send_modify(|s| s.delta = Some(delta));
    }
}

impl FixConsumer for Correlator {
    fn on_fix(&mut self, fix: GPSFix) {
        let (p, t1) = (self.track.projection.project(Point::new(fix.longitude, fix.latitude)).0, fix.timestamp);
        if self.prev.is_some_and(|(_, t0)| t1 <= t0) {
            return; // out of order
        }
        let Some((p0, t0)) = self.prev.replace((p, t1)) else { return };
        let gap = t1 - t0 > MAX_GAP;
        let next = match &mut self.state {
            State::Finished => return,
            State::Armed => 0,
            State::InProgress(lap) => {
                lap.gps_gap |= gap;
                lap.fixes.push(fix);
                lap.crossings.len()
            }
        };

        let gate = self.track.gates_ordered().nth(next).expect("state never passes the last gate");
        // Same gate again (Closed, no sectors) before leaving it.
        let blocked = next.checked_sub(1).and_then(|i| self.track.gates_ordered().nth(i)).is_some_and(|last| {
            let d = p - last.center.0;
            self.rearmed |= d.x.hypot(d.y) > REARM_DISTANCE;
            std::ptr::eq(last, gate) && !self.rearmed
        });
        let Some(frac) = crossing(p0, p, gate).filter(|_| !blocked) else {
            return self.track_delta(p, t1);
        };
        let at = t0 + (t1 - t0).to_std().expect("t1 > t0").mul_f64(frac);
        self.rearmed = false;

        let lap = match std::mem::replace(&mut self.state, State::Finished) {
            State::InProgress(mut lap) => {
                lap.crossings.push(at);
                lap
            }
            _ => Current::new(&self.track, at, gap, fix, p),
        };
        let i = lap.crossings.len() - 1;
        let last_sector = i.checked_sub(1).map(|j| at - lap.crossings[j]);
        // Exact at a gate: the reference's own crossing of it, where between gates it's interpolated.
        let ref_at = |k: usize| self.reference.as_ref().and_then(|r| Some(*r.lap.crossings().get(k)? - r.lap.crossings()[0]));
        let delta = ref_at(i).filter(|_| i > 0).map(|r| at - lap.crossings[0] - r);
        let sector_delta = i.checked_sub(1).and_then(|j| Some(at - lap.crossings[j] - (ref_at(i)? - ref_at(j)?)));
        let mut done = None;
        self.state = if i + 1 < self.track.gates_ordered().count() {
            State::InProgress(lap)
        } else {
            let finished = Lap::new(lap.crossings, lap.gps_gap);
            done = Some(finished.duration());
            if !finished.gps_gap && self.reference.as_ref().is_none_or(|r| finished.duration() < r.lap.duration()) {
                self.reference = Some(Reference::new(&self.track, finished.clone(), &lap.fixes));
            }
            self.session.record(finished);
            match self.track.boundary {
                Boundary::Closed { .. } => State::InProgress(Current::new(&self.track, at, gap, fix, p)),
                Boundary::Open { .. } => State::Finished,
            }
        };

        // Back-date to when the crossing happened, on the device's monotonic clock.
        let anchor = Instant::now() - (t1 - at).to_std().unwrap_or_default();
        let best = self.reference.as_ref().map(|r| r.lap.duration());
        self.tx.send_modify(|s| {
            s.status = match (&self.state, s.status) {
                (State::InProgress(lap), LiveStatus::InProgress { lap_anchor, .. }) if lap.crossings.len() > 1 => {
                    LiveStatus::InProgress { lap_anchor, sector_anchor: anchor, sector: lap.crossings.len() - 1 }
                }
                (State::InProgress(_), _) => LiveStatus::InProgress { lap_anchor: anchor, sector_anchor: anchor, sector: 0 },
                _ => LiveStatus::Finished,
            };
            s.best = best;
            if last_sector.is_some() {
                s.last_sector = last_sector;
                s.last_sector_delta = sector_delta;
                s.delta = delta;
            }
            if done.is_some() {
                s.laps += 1;
                s.last_lap = done;
                s.last_lap_delta = delta;
            }
        });
    }
}

/// Fraction along `a → b` at which it crosses `gate` forwards, if it does.
fn crossing(a: Coord, b: Coord, gate: &Gate) -> Option<f64> {
    let cross = |p: Coord, q: Coord| p.x * q.y - p.y * q.x;
    let (d, g) = (b - a, gate.segment());
    if d.x * gate.direction.x + d.y * gate.direction.y <= 0.0 {
        return None; // not moving forwards, so also never parallel to the gate
    }
    let (w, e) = (g.start - a, g.delta());
    let (t, u) = (cross(w, e) / cross(d, e), cross(w, d) / cross(d, e));
    // t > 0: a fix exactly on the gate counts once, as the end of the incoming segment.
    (0.0 < t && t <= 1.0 && (0.0..=1.0).contains(&u)).then_some(t)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        timing::Reference,
        track::{Completed, TrackBuilder, TrackShape, Waypoint},
    };

    const ORIGIN: Waypoint = Point(geo::coord! { x: -9.0, y: 38.7 });

    fn local(x: f64, y: f64) -> Waypoint {
        super::super::Projection::new(ORIGIN).unproject(Point::new(x, y))
    }

    /// 100 m square loop, first leg east. Gate at 50 m along; sectors optional.
    pub(crate) fn square(sectors: &[(f64, f64)]) -> Arc<Track> {
        Arc::new(square_spec(sectors).build().unwrap())
    }

    pub(crate) fn square_spec(sectors: &[(f64, f64)]) -> TrackBuilder {
        let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)].map(|(x, y)| local(x, y));
        let b = Track::builder("Square", TrackShape::Closed { gate: local(50.0, 0.0) }).waypoints(corners);
        sectors.iter().fold(b, |b, &(x, y)| b.sector_gate_at(local(x, y)))
    }

    /// Position `s` metres around the square.
    pub(crate) fn around(s: f64) -> (f64, f64) {
        match s.rem_euclid(400.0) {
            s if s < 100.0 => (s, 0.0),
            s if s < 200.0 => (100.0, s - 100.0),
            s if s < 300.0 => (300.0 - s, 100.0),
            s => (0.0, 400.0 - s),
        }
    }

    /// A fix at local `(x, y)`, `secs` after the epoch.
    pub(crate) fn fix((x, y): (f64, f64), secs: f64) -> GPSFix {
        let w = local(x, y);
        let timestamp = DateTime::UNIX_EPOCH + TimeDelta::milliseconds((secs * 1000.0) as i64);
        GPSFix { longitude: w.x(), latitude: w.y(), timestamp, ..Default::default() }
    }

    /// Drives at 10 m/s, one fix per `s` in metres (t = s / 10).
    fn drive(c: &mut Correlator, s: impl IntoIterator<Item = f64>) {
        drive_at(c, 10.0, s);
    }

    fn drive_at(c: &mut Correlator, speed: f64, s: impl IntoIterator<Item = f64>) -> Vec<GPSFix> {
        let fixes: Vec<_> = s.into_iter().map(|s| fix(around(s), s / speed)).collect();
        fixes.iter().for_each(|&f| c.on_fix(f));
        fixes
    }

    /// One lap of `track` at `speed` m/s, as a reference.
    fn reference(track: &Arc<Track>, speed: f64) -> Reference {
        let (mut c, _rx) = Correlator::new(track.clone(), None);
        let fixes = drive_at(&mut c, speed, steps(25.0, 485.0));
        Reference::new(track, laps(c).swap_remove(0), &fixes)
    }

    fn near(got: Option<TimeDelta>, want: f64) -> bool {
        got.is_some_and(|g| (g - TimeDelta::milliseconds((want * 1000.0) as i64)).abs() < TimeDelta::milliseconds(2))
    }

    fn steps(from: f64, to: f64) -> impl Iterator<Item = f64> {
        (0..).map(move |i| from + 10.0 * i as f64).take_while(move |s| *s <= to)
    }

    fn laps(c: Correlator) -> Vec<Lap> {
        match c.into_session().completed {
            Completed::Laps(laps) => laps,
            Completed::Run(_) => panic!("closed track"),
        }
    }

    fn secs(s: f64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + TimeDelta::milliseconds((s * 1000.0) as i64)
    }

    #[test]
    fn closed_lap_interpolates_crossings() {
        let (mut c, rx) = Correlator::new(square(&[]), None);
        drive(&mut c, steps(25.0, 485.0));
        let s = *rx.borrow();
        assert_eq!((s.laps, s.last_lap), (1, Some(TimeDelta::seconds(40))));
        assert!(matches!(s.status, LiveStatus::InProgress { sector: 0, .. }));
        let laps = laps(c);
        assert_eq!(laps[0].crossings(), [secs(5.0), secs(45.0)]);
        assert!(!laps[0].gps_gap);
    }

    #[test]
    fn sector_splits() {
        let (mut c, rx) = Correlator::new(square(&[(100.0, 50.0), (50.0, 100.0)]), None);
        drive(&mut c, steps(25.0, 455.0));
        // Past the lap line: the last sector closed the lap.
        assert_eq!(rx.borrow().last_sector, Some(TimeDelta::seconds(20)));
        let splits: Vec<_> = laps(c)[0].splits().collect();
        assert_eq!(splits, [10, 10, 20].map(TimeDelta::seconds));
    }

    #[test]
    fn jitter_at_the_gate_does_not_count() {
        let (mut c, rx) = Correlator::new(square(&[]), None);
        drive(&mut c, [45.0, 55.0]);
        [(45.0, 0.0), (55.0, 0.0)].into_iter().enumerate().for_each(|(i, p)| c.on_fix(fix(p, 6.0 + i as f64)));
        assert_eq!(rx.borrow().laps, 0);
        assert!(laps(c).is_empty());
    }

    #[test]
    fn backwards_crossing_does_not_start() {
        let (mut c, rx) = Correlator::new(square(&[]), None);
        c.on_fix(fix((55.0, 0.0), 0.0));
        c.on_fix(fix((45.0, 0.0), 1.0));
        assert_eq!(rx.borrow().status, LiveStatus::Armed);
    }

    #[test]
    fn dropout_flags_the_lap() {
        let (mut c, _rx) = Correlator::new(square(&[]), None);
        drive(&mut c, steps(25.0, 205.0).chain(steps(245.0, 465.0)));
        assert!(laps(c)[0].gps_gap);
    }

    #[test]
    fn delta_against_the_reference() {
        let track = square(&[(100.0, 50.0)]);
        let (mut c, rx) = Correlator::new(track.clone(), Some(reference(&track, 10.0)));
        assert_eq!(rx.borrow().best, Some(TimeDelta::seconds(40)));
        // 8 m/s: 195 m into the lap takes 24.375 s against the 10 m/s reference's 19.5 s.
        drive_at(&mut c, 8.0, steps(25.0, 245.0));
        assert!(near(rx.borrow().delta, 4.875), "{:?}", rx.borrow());
        // First sector (100 m): 12.5 s against 10 s.
        assert!(near(rx.borrow().last_sector_delta, 2.5));
        drive_at(&mut c, 8.0, steps(255.0, 455.0));
        let s = *rx.borrow();
        assert!(near(s.last_lap_delta, 10.0) && near(s.delta, 10.0) && near(s.last_sector_delta, 7.5), "{s:?}");
        assert_eq!(s.best, Some(TimeDelta::seconds(40)));
    }

    #[test]
    fn a_faster_clean_lap_becomes_the_reference() {
        let track = square(&[]);
        let (mut c, rx) = Correlator::new(track.clone(), Some(reference(&track, 8.0)));
        drive(&mut c, steps(25.0, 685.0));
        let s = *rx.borrow();
        assert!(near(s.last_lap_delta, -10.0), "{s:?}");
        assert_eq!(s.best, Some(TimeDelta::seconds(40)));
        // Second lap, same pace as the new reference.
        assert!(near(s.delta, 0.0), "{s:?}");
    }

    #[test]
    fn a_lap_with_a_gap_never_becomes_the_reference() {
        let (mut c, rx) = Correlator::new(square(&[]), None);
        drive(&mut c, steps(25.0, 205.0).chain(steps(245.0, 685.0)));
        let s = *rx.borrow();
        assert_eq!((s.laps, s.best, s.delta, s.last_lap_delta), (1, None, None, None));
    }

    #[test]
    fn open_track_finishes_after_one_run() {
        let straight = [local(0.0, 0.0), local(200.0, 0.0)];
        let open = TrackShape::Open { start: local(50.0, 0.0), finish: local(150.0, 0.0) };
        let (mut c, rx) = Correlator::new(Arc::new(Track::builder("Line", open).waypoints(straight).build().unwrap()), None);
        steps(25.0, 195.0).for_each(|s| c.on_fix(fix((s, 0.0), s / 10.0)));
        assert_eq!(rx.borrow().status, LiveStatus::Finished);
        let Completed::Run(Some(run)) = c.into_session().completed else { panic!("no run") };
        assert_eq!(run.duration(), TimeDelta::seconds(10));
    }
}
