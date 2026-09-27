use std::{sync::Arc, time::Instant};

use chrono::{DateTime, TimeDelta, Utc};
use geo::{Coord, Point};
use tokio::sync::watch;

use super::{Boundary, Gate, Lap, Session, Track};
use crate::{gps::GPSFix, recorder::FixConsumer};

/// A longer gap between fixes makes interpolated crossings unreliable: the lap gets `gps_gap`.
const MAX_GAP: TimeDelta = TimeDelta::seconds(2);
/// Distance (m) to get from a gate before it can be crossed again, so jitter can't re-trigger it.
const REARM_DISTANCE: f64 = 30.0;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct LiveSessionSnapshot {
    pub status: LiveStatus,
    pub laps: usize,
    pub last_lap: Option<TimeDelta>,
    pub last_sector: Option<TimeDelta>,
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

/// Detects forward gate crossings against an armed `Track`, filling its `Session`.
pub struct Correlator {
    track: Arc<Track>,
    session: Session,
    tx: watch::Sender<LiveSessionSnapshot>,
    state: State,
    prev: Option<(Coord, DateTime<Utc>)>,
    /// Been `REARM_DISTANCE` from the last crossed gate since crossing it.
    rearmed: bool,
}

enum State {
    Armed,
    InProgress { crossings: Vec<DateTime<Utc>>, gps_gap: bool },
    Finished,
}

impl Correlator {
    pub fn new(track: Arc<Track>) -> (Self, watch::Receiver<LiveSessionSnapshot>) {
        let (tx, rx) = watch::channel(LiveSessionSnapshot::default());
        let session = Session::new(&track);
        (Self { track, session, tx, state: State::Armed, prev: None, rearmed: false }, rx)
    }

    /// Completed laps/run; a lap still in progress is discarded.
    pub fn into_session(self) -> Session {
        self.session
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
            State::InProgress { crossings, gps_gap } => {
                *gps_gap |= gap;
                crossings.len()
            }
        };

        let gate = self.track.gates_ordered().nth(next).expect("state never passes the last gate");
        if let Some(last) = next.checked_sub(1).and_then(|i| self.track.gates_ordered().nth(i)) {
            let d = p - last.center.0;
            self.rearmed |= d.x.hypot(d.y) > REARM_DISTANCE;
            if std::ptr::eq(last, gate) && !self.rearmed {
                return; // same gate again (Closed, no sectors) before leaving it
            }
        }
        let Some(frac) = crossing(p0, p, gate) else { return };
        let at = t0 + (t1 - t0).to_std().expect("t1 > t0").mul_f64(frac);
        self.rearmed = false;

        let (mut crossings, gps_gap) = match std::mem::replace(&mut self.state, State::Finished) {
            State::InProgress { crossings, gps_gap } => (crossings, gps_gap),
            _ => (Vec::new(), gap),
        };
        crossings.push(at);
        let last_sector = crossings.len().checked_sub(2).map(|i| at - crossings[i]);
        let mut done = None;
        self.state = if crossings.len() < self.track.gates_ordered().count() {
            State::InProgress { crossings, gps_gap }
        } else {
            let lap = Lap::new(crossings, gps_gap);
            done = Some(lap.duration());
            self.session.record(lap);
            match self.track.boundary {
                Boundary::Closed { .. } => State::InProgress { crossings: vec![at], gps_gap: gap },
                Boundary::Open { .. } => State::Finished,
            }
        };

        // Back-date to when the crossing happened, on the device's monotonic clock.
        let anchor = Instant::now() - (t1 - at).to_std().unwrap_or_default();
        self.tx.send_modify(|s| {
            s.status = match (&self.state, s.status) {
                (State::InProgress { crossings, .. }, LiveStatus::InProgress { lap_anchor, .. }) if crossings.len() > 1 => {
                    LiveStatus::InProgress { lap_anchor, sector_anchor: anchor, sector: crossings.len() - 1 }
                }
                (State::InProgress { .. }, _) => {
                    LiveStatus::InProgress { lap_anchor: anchor, sector_anchor: anchor, sector: 0 }
                }
                _ => LiveStatus::Finished,
            };
            s.last_sector = last_sector.or(s.last_sector);
            if done.is_some() {
                s.laps += 1;
                s.last_lap = done;
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
    use crate::track::{Completed, TrackShape, Waypoint};

    const ORIGIN: Waypoint = Point(geo::coord! { x: -9.0, y: 38.7 });

    fn local(x: f64, y: f64) -> Waypoint {
        super::super::Projection::new(ORIGIN).unproject(Point::new(x, y))
    }

    /// 100 m square loop, first leg east. Gate at 50 m along; sectors optional.
    pub(crate) fn square(sectors: &[(f64, f64)]) -> Arc<Track> {
        let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)].map(|(x, y)| local(x, y));
        let b = Track::builder("Square", TrackShape::Closed { gate: local(50.0, 0.0) }).waypoints(corners);
        Arc::new(sectors.iter().fold(b, |b, &(x, y)| b.sector_gate_at(local(x, y))).build().unwrap())
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
        s.into_iter().for_each(|s| c.on_fix(fix(around(s), s / 10.0)));
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
        let (mut c, rx) = Correlator::new(square(&[]));
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
        let (mut c, rx) = Correlator::new(square(&[(100.0, 50.0), (50.0, 100.0)]));
        drive(&mut c, steps(25.0, 455.0));
        // Past the lap line: the last sector closed the lap.
        assert_eq!(rx.borrow().last_sector, Some(TimeDelta::seconds(20)));
        let splits: Vec<_> = laps(c)[0].splits().collect();
        assert_eq!(splits, [10, 10, 20].map(TimeDelta::seconds));
    }

    #[test]
    fn jitter_at_the_gate_does_not_count() {
        let (mut c, rx) = Correlator::new(square(&[]));
        drive(&mut c, [45.0, 55.0]);
        [(45.0, 0.0), (55.0, 0.0)].into_iter().enumerate().for_each(|(i, p)| c.on_fix(fix(p, 6.0 + i as f64)));
        assert_eq!(rx.borrow().laps, 0);
        assert!(laps(c).is_empty());
    }

    #[test]
    fn backwards_crossing_does_not_start() {
        let (mut c, rx) = Correlator::new(square(&[]));
        c.on_fix(fix((55.0, 0.0), 0.0));
        c.on_fix(fix((45.0, 0.0), 1.0));
        assert_eq!(rx.borrow().status, LiveStatus::Armed);
    }

    #[test]
    fn dropout_flags_the_lap() {
        let (mut c, _rx) = Correlator::new(square(&[]));
        drive(&mut c, steps(25.0, 205.0).chain(steps(245.0, 465.0)));
        assert!(laps(c)[0].gps_gap);
    }

    #[test]
    fn open_track_finishes_after_one_run() {
        let straight = [local(0.0, 0.0), local(200.0, 0.0)];
        let open = TrackShape::Open { start: local(50.0, 0.0), finish: local(150.0, 0.0) };
        let (mut c, rx) = Correlator::new(Arc::new(Track::builder("Line", open).waypoints(straight).build().unwrap()));
        steps(25.0, 195.0).for_each(|s| c.on_fix(fix((s, 0.0), s / 10.0)));
        assert_eq!(rx.borrow().status, LiveStatus::Finished);
        let Completed::Run(Some(run)) = c.into_session().completed else { panic!("no run") };
        assert_eq!(run.duration(), TimeDelta::seconds(10));
    }
}
