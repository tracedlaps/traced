use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Boundary, Track};

/// One continuous timed attempt on a `Track`. Holds completed laps/runs only;
/// the one in progress lives in the Correlator's live snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub track_id: Uuid,
    pub completed: Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Completed {
    Laps(Vec<Lap>),
    Run(Option<Run>),
}

/// Raw GPS-time gate crossings, in `Track::gates_ordered` order: start gate,
/// each sector gate, end gate. Durations and splits are derived, never stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lap {
    crossings: Vec<DateTime<Utc>>,
    /// A fix dropout during the lap made its interpolated crossings unreliable.
    pub gps_gap: bool,
}

/// An Open track's single start-to-finish pass. Same shape as a `Lap`.
pub type Run = Lap;

impl Session {
    pub fn new(track: &Track) -> Self {
        let completed = match track.boundary {
            Boundary::Closed { .. } => Completed::Laps(Vec::new()),
            Boundary::Open { .. } => Completed::Run(None),
        };
        Self { track_id: track.uuid, completed }
    }

    /// A Closed track's laps, or an Open track's run as a single lap.
    pub fn laps(&self) -> &[Lap] {
        match &self.completed {
            Completed::Laps(laps) => laps,
            Completed::Run(run) => run.as_slice(),
        }
    }

    /// Appends a lap (Closed) or sets the run (Open, once only).
    pub(crate) fn record(&mut self, lap: Lap) {
        match &mut self.completed {
            Completed::Laps(laps) => laps.push(lap),
            Completed::Run(run) => {
                debug_assert!(run.is_none(), "an Open session has a single run");
                *run = Some(lap);
            }
        }
    }
}

impl Lap {
    /// `crossings` must hold ≥ 2 ascending timestamps, one per gate.
    pub(crate) fn new(crossings: Vec<DateTime<Utc>>, gps_gap: bool) -> Self {
        debug_assert!(crossings.len() >= 2 && crossings.is_sorted());
        Self { crossings, gps_gap }
    }

    pub fn crossings(&self) -> &[DateTime<Utc>] {
        &self.crossings
    }

    pub fn duration(&self) -> TimeDelta {
        self.crossings[self.crossings.len() - 1] - self.crossings[0]
    }

    /// How much slower than `reference` this lap was; negative when faster.
    pub fn delta(&self, reference: &Lap) -> TimeDelta {
        self.duration() - reference.duration()
    }

    /// Time spent in each sector, matching `Track::sectors`.
    pub fn splits(&self) -> impl Iterator<Item = TimeDelta> + '_ {
        self.crossings.windows(2).map(|w| w[1] - w[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{TrackShape, Waypoint};

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + TimeDelta::seconds(secs)
    }

    fn track(shape: TrackShape) -> Track {
        let waypoints = [Waypoint::new(-9.0, 38.7), Waypoint::new(-8.998, 38.7), Waypoint::new(-8.998, 38.701)];
        Track::builder("T", shape).waypoints(waypoints).build().unwrap()
    }

    #[test]
    fn lap_derives_duration_and_splits() {
        let lap = Lap::new(vec![at(0), at(20), at(45), at(60)], false);
        assert_eq!(lap.duration(), TimeDelta::seconds(60));
        assert_eq!(lap.splits().collect::<Vec<_>>(), [20, 25, 15].map(TimeDelta::seconds));
        assert_eq!(lap.delta(&Lap::new(vec![at(0), at(62)], false)), TimeDelta::seconds(-2));
    }

    #[test]
    fn closed_session_appends_laps() {
        let t = track(TrackShape::Closed { gate: Waypoint::new(-8.999, 38.7) });
        let mut s = Session::new(&t);
        s.record(Lap::new(vec![at(0), at(60)], false));
        s.record(Lap::new(vec![at(60), at(118)], true));
        assert_eq!(s.track_id, t.uuid);
        assert!(matches!(&s.completed, Completed::Laps(laps) if laps.len() == 2 && laps[1].gps_gap));
    }

    #[test]
    fn open_session_holds_one_run() {
        let mut s = Session::new(&track(TrackShape::Open {
            start: Waypoint::new(-8.9995, 38.7),
            finish: Waypoint::new(-8.998, 38.7005),
        }));
        assert_eq!(s.completed, Completed::Run(None));
        s.record(Lap::new(vec![at(0), at(30)], false));
        assert!(matches!(&s.completed, Completed::Run(Some(run)) if run.duration() == TimeDelta::seconds(30)));
    }
}
