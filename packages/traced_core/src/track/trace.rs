//! A drive or walk traced into sparse waypoints, to author a `Track` from.

use std::sync::Arc;

use geo::{Distance, Euclidean, LineString, Point, Simplify};
use tokio::sync::watch;

use super::{Projection, Waypoint};
use crate::{gps::GPSFix, recorder::FixConsumer};

/// Fixes less accurate than this (m) are ignored, as by the `Speedometer`.
const MAX_ACCURACY_M: f32 = 25.0;
/// A fix is kept only this far (m) from the last kept one, or its accuracy if worse,
/// so standing still doesn't scribble. Position, not speed: devices may not report one.
const MIN_STEP_M: f64 = 3.0;
/// Douglas–Peucker tolerance (m): a waypoint is dropped if the trace stays this close without it.
const EPSILON_M: f64 = 1.5;
/// How far (m) the trace must go before coming back near its start counts as a loop.
const MIN_LOOP_M: f64 = 100.0;
/// How near (m) to the start counts as coming back to it.
const LOOP_GAP_M: f64 = 30.0;

/// Collects the fixes of a drive or walk; the `watch` sender holds the simplified trace so far.
pub struct Trace {
    tx: watch::Sender<Arc<[Waypoint]>>,
    projection: Option<Projection>,
    /// Kept fixes, projected.
    points: Vec<Point>,
}

impl Trace {
    pub fn new() -> (Self, watch::Receiver<Arc<[Waypoint]>>) {
        let (tx, rx) = watch::channel(Arc::from([]));
        (Self { tx, projection: None, points: Vec::new() }, rx)
    }

    /// The trace's waypoints; a loop driven more than once is cut where it first gets back to its start.
    pub fn into_waypoints(self) -> Vec<Waypoint> {
        let end = first_loop(&self.points).unwrap_or(self.points.len());
        self.simplified(&self.points[..end])
    }

    fn simplified(&self, points: &[Point]) -> Vec<Waypoint> {
        let Some(projection) = &self.projection else { return Vec::new() };
        let line: LineString = points.iter().copied().collect();
        line.simplify(EPSILON_M).points().map(|p| projection.unproject(p)).collect()
    }
}

/// Where the path, after `MIN_LOOP_M`, comes nearest its start while within `LOOP_GAP_M` of it.
fn first_loop(points: &[Point]) -> Option<usize> {
    let start = *points.first()?;
    let mut travelled = 0.0;
    let mut best: Option<(usize, f64)> = None;
    for (i, pair) in points.windows(2).enumerate() {
        travelled += Euclidean.distance(pair[0], pair[1]);
        let gap = Euclidean.distance(start, pair[1]);
        match best {
            Some((_, b)) if gap > b => break,
            _ if travelled >= MIN_LOOP_M && gap < LOOP_GAP_M => best = Some((i + 2, gap)),
            _ => {}
        }
    }
    best.map(|(end, _)| end)
}

impl FixConsumer for Trace {
    fn on_fix(&mut self, fix: GPSFix) {
        if fix.accuracy > MAX_ACCURACY_M {
            return;
        }
        let w = Waypoint::new(fix.longitude, fix.latitude);
        let p = self.projection.get_or_insert_with(|| Projection::new(w)).project(w);
        if let Some(&last) = self.points.last()
            && Euclidean.distance(last, p) < MIN_STEP_M.max(f64::from(fix.accuracy))
        {
            return;
        }
        self.points.push(p);
        let trace = self.simplified(&self.points);
        self.tx.send_replace(trace.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        track::{Track, TrackShape, correlator::tests::{around, fix, square}, import::looks_closed},
    };

    fn trace(fixes: impl IntoIterator<Item = GPSFix>) -> (Trace, watch::Receiver<Arc<[Waypoint]>>) {
        let (mut t, rx) = Trace::new();
        fixes.into_iter().for_each(|f| t.on_fix(f));
        (t, rx)
    }

    #[test]
    fn straight_leg_simplifies_to_its_ends() {
        let (t, rx) = trace((0..=10).map(|i| fix((i as f64 * 10.0, 0.0), i as f64)));
        assert_eq!(rx.borrow().len(), 2);
        assert_eq!(t.into_waypoints().len(), 2);
    }

    #[test]
    fn standing_still_and_inaccurate_fixes_add_nothing() {
        let wander = (0..20).map(|i| fix((i as f64 % 2.0, 0.0), i as f64));
        let bad = GPSFix { accuracy: 50.0, ..fix((500.0, 0.0), 20.0) };
        let (t, rx) = trace(wander.chain([bad]));
        assert_eq!(rx.borrow().len(), 1);
        assert_eq!(t.into_waypoints().len(), 1);
    }

    #[test]
    fn two_laps_give_one_loop_ending_near_the_start() {
        let (t, rx) = trace((0..=80).map(|i| fix(around(i as f64 * 10.0), i as f64)));
        assert!(rx.borrow().len() > 5, "live trace keeps both laps");
        let w = t.into_waypoints();
        assert_eq!(w.len(), 5, "four corners and back to the start: {w:?}");
        assert!(looks_closed(&w));
    }

    #[test]
    fn open_path_is_not_cut() {
        // Out 150 m and back to 40 m from the start: never within the loop gap.
        let out = (0..=15).map(|i| (i as f64 * 10.0, 0.0));
        let back = (0..=11).map(|i| (150.0 - i as f64 * 10.0, 20.0));
        let (t, _) = trace(out.chain(back).enumerate().map(|(i, p)| fix(p, i as f64)));
        let w = t.into_waypoints();
        assert_eq!(w.len(), 4);
        assert!(!looks_closed(&w));
    }

    #[test]
    fn a_jittery_drive_traces_a_track_like_the_one_driven() {
        use rand::RngExt;
        let mut rng = rand::rng();
        let mut jitter = || rng.random_range(-1.0..=1.0);
        // Two laps from the gate, 5 m apart, each fix up to 1 m off.
        let fixes: Vec<_> = (0..170)
            .map(|i| {
                let (x, y) = around(50.0 + i as f64 * 5.0);
                GPSFix { accuracy: 1.0, ..fix((x + jitter(), y + jitter()), i as f64) }
            })
            .collect();
        let w = trace(fixes).0.into_waypoints();
        assert!(w.len() < 20, "sparse: {}", w.len());
        let traced = Track::builder("Traced", TrackShape::Closed { gate: w[0] }).waypoints(w).build().unwrap();
        let driven = square(&[]);
        assert!((traced.length() - driven.length()).abs() < 20.0, "{} vs {}", traced.length(), driven.length());
    }
}
