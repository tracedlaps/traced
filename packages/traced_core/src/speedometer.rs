use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{gps::GPSFix, recorder::FixConsumer};

/// Fixes less accurate than this are ignored entirely.
const MAX_ACCURACY_M: f32 = 25.0;
/// A longer gap between fixes is a dropout: it adds no distance.
const MAX_GAP: Duration = Duration::from_secs(5);

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeedometerSnapshot {
    pub current_speed_ms: f32,
    pub top_speed_ms: f32,
    pub distance_m: f64,
    /// Distance over the time it was integrated across, so dropouts don't drag it down.
    pub average_speed_ms: f64,
    pub elapsed: Duration,
}

/// Accumulates speed/distance from the fix stream; the `watch` sender holds the snapshot.
pub struct Speedometer {
    tx: watch::Sender<SpeedometerSnapshot>,
    first: Option<DateTime<Utc>>,
    last: Option<DateTime<Utc>>,
    /// Time `distance_m` was integrated over: `elapsed` minus dropouts.
    integrated: Duration,
}

impl Speedometer {
    pub fn new() -> (Self, watch::Receiver<SpeedometerSnapshot>) {
        let (tx, rx) = watch::channel(SpeedometerSnapshot::default());
        (
            Self {
                tx,
                first: None,
                last: None,
                integrated: Duration::ZERO,
            },
            rx,
        )
    }

    pub fn snapshot(&self) -> SpeedometerSnapshot {
        *self.tx.borrow()
    }
}

impl FixConsumer for Speedometer {
    fn on_fix(&mut self, fix: GPSFix) {
        if fix.accuracy > MAX_ACCURACY_M {
            return;
        }
        let first = *self.first.get_or_insert(fix.timestamp);
        let dt = self
            .last
            .replace(fix.timestamp)
            .and_then(|last| (fix.timestamp - last).to_std().ok());
        let dt = dt.filter(|dt| !dt.is_zero() && *dt <= MAX_GAP);
        if let Some(dt) = dt {
            self.integrated += dt;
        }
        let integrated = self.integrated.as_secs_f64();
        self.tx.send_modify(|s| {
            if let Some(dt) = dt {
                // Trapezoid: `current_speed_ms` still holds the previous fix's speed.
                s.distance_m +=
                    f64::from(s.current_speed_ms + fix.speed_ms) / 2.0 * dt.as_secs_f64();
                s.average_speed_ms = s.distance_m / integrated;
            }
            s.current_speed_ms = fix.speed_ms;
            s.top_speed_ms = s.top_speed_ms.max(fix.speed_ms);
            s.elapsed = (fix.timestamp - first).to_std().unwrap_or_default();
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn fix(secs: i64, speed_ms: f32) -> GPSFix {
        GPSFix {
            speed_ms,
            accuracy: 5.0,
            timestamp: DateTime::UNIX_EPOCH + chrono::Duration::seconds(secs),
            ..Default::default()
        }
    }

    fn feed(fixes: impl IntoIterator<Item = GPSFix>) -> SpeedometerSnapshot {
        let (mut s, _rx) = Speedometer::new();
        fixes.into_iter().for_each(|f| s.on_fix(f));
        s.snapshot()
    }

    #[test]
    fn integrates_distance_with_trapezoid() {
        let s = feed([fix(0, 10.0), fix(1, 20.0), fix(2, 20.0)]);
        assert_eq!(s.distance_m, 35.0);
        assert_eq!((s.current_speed_ms, s.top_speed_ms), (20.0, 20.0));
        assert_eq!(s.elapsed, Duration::from_secs(2));
        assert_eq!(s.average_speed_ms, 17.5);
    }

    #[test]
    fn gap_adds_no_distance() {
        let s = feed([fix(0, 10.0), fix(1, 10.0), fix(60, 10.0), fix(61, 10.0)]);
        assert_eq!((s.distance_m, s.average_speed_ms), (20.0, 10.0));
        assert_eq!(s.elapsed, Duration::from_secs(61));
    }

    #[test]
    fn inaccurate_fix_is_ignored() {
        let bad = GPSFix {
            accuracy: 100.0,
            ..fix(1, 99.0)
        };
        let s = feed([fix(0, 10.0), bad, fix(2, 10.0)]);
        assert_eq!((s.top_speed_ms, s.distance_m), (10.0, 20.0));
    }

    #[test]
    fn out_of_order_fix_adds_no_distance() {
        assert_eq!(feed([fix(5, 10.0), fix(4, 10.0)]).distance_m, 0.0);
    }

    #[test]
    fn empty_average_is_zero() {
        assert_eq!(feed([fix(0, 10.0)]).average_speed_ms, 0.0);
    }

    #[test]
    fn pushes_snapshot_on_fix() {
        let (mut s, mut rx) = Speedometer::new();
        s.on_fix(fix(0, 7.0));
        assert!(rx.has_changed().unwrap());
        assert_eq!(rx.borrow_and_update().current_speed_ms, 7.0);
    }
}
