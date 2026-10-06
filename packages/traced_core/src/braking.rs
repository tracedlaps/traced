//! Where a lap braked, from GPS speed alone: works without an accelerometer and on old recordings.

use chrono::TimeDelta;

use crate::{gps::GPSFix, imu::G, track::Waypoint};

/// Slowing by at least this (m/s, ~11 km/h) from a top speed makes a braking zone…
const MIN_DROP_MS: f32 = 3.0;
/// …which ends once speed picks up this much from its lowest.
const RISE_MS: f32 = 1.0;
/// Braking starts at the first slowing over `ONSET_OVER` at least this share of the zone's peak:
/// speed bled off before it is coasting. Relative, as a descent brakes far gentler than a kart.
const ONSET_SHARE: f32 = 0.5;
const ONSET_OVER: TimeDelta = TimeDelta::milliseconds(500);
/// Peak deceleration is averaged over this, to smooth GPS speed noise.
const PEAK_OVER: TimeDelta = TimeDelta::seconds(1);
/// Zones of two laps starting within this (m) brake for the same corner.
const SAME_CORNER_M: f64 = 50.0;

#[derive(Debug, Clone, PartialEq)]
pub struct BrakingZone {
    /// Where braking started and where it was slowest, in metres past the start gate.
    pub at_m: f64,
    pub end_m: f64,
    pub entry_ms: f32,
    pub exit_ms: f32,
    /// Hardest deceleration (g).
    pub peak_g: f32,
    /// Where it was driven, from braking to slowest.
    pub path: Vec<Waypoint>,
}

impl BrakingZone {
    /// The zone in `others` braking for the same corner: the one starting nearest, if close enough.
    pub fn counterpart<'a>(&self, others: &'a [BrakingZone]) -> Option<&'a BrakingZone> {
        others
            .iter()
            .filter(|o| (o.at_m - self.at_m).abs() <= SAME_CORNER_M)
            .min_by(|a, b| (a.at_m - self.at_m).abs().total_cmp(&(b.at_m - self.at_m).abs()))
    }
}

/// A lap's braking zones, in order, from its fixes with their distances (see `timing::along_lap`).
pub fn braking_zones(lap: &[(f64, GPSFix)]) -> Vec<BrakingZone> {
    let v = |i: usize| lap[i].1.speed_ms;
    let (mut zones, mut top, mut low, mut braking) = (Vec::new(), 0, 0, false);
    for i in 0..lap.len() {
        if !braking {
            if v(i) >= v(top) {
                top = i;
            } else if v(top) - v(i) >= MIN_DROP_MS {
                (braking, low) = (true, i);
            }
        } else if v(i) <= v(low) {
            low = i;
        } else if v(i) - v(low) >= RISE_MS {
            zones.push(zone(&lap[top..=low]));
            (braking, top) = (false, i);
        }
    }
    if braking {
        zones.push(zone(&lap[top..=low]));
    }
    zones
}

/// The zone from top speed to lowest, (distance, fix) each.
fn zone(fixes: &[(f64, GPSFix)]) -> BrakingZone {
    let peak = (0..fixes.len()).map(|i| slowing(fixes, i, PEAK_OVER)).fold(0.0, f32::max);
    let hard = peak * ONSET_SHARE;
    let onset = (0..fixes.len()).find(|&i| slowing(fixes, i, ONSET_OVER) >= hard).unwrap_or(0);
    // The window saw it coming: start at the first fix that slows that hard to the next.
    let onset = (onset..fixes.len()).find(|&i| slowing(fixes, i, TimeDelta::nanoseconds(1)) >= hard).unwrap_or(onset);
    let ((at_m, entry), (end_m, exit)) = (fixes[onset], fixes[fixes.len() - 1]);
    let path = fixes[onset..].iter().map(|(_, f)| Waypoint::new(f.longitude, f.latitude)).collect();
    BrakingZone { at_m, end_m, entry_ms: entry.speed_ms, exit_ms: exit.speed_ms, peak_g: peak / G, path }
}

/// Slowing (m/s²) from fix `i` to the first at least `over` later, or the last.
fn slowing(fixes: &[(f64, GPSFix)], i: usize, over: TimeDelta) -> f32 {
    let a = fixes[i].1;
    let b = fixes[i..].iter().find(|(_, b)| b.timestamp - a.timestamp >= over).unwrap_or(&fixes[fixes.len() - 1]).1;
    let dt = (b.timestamp - a.timestamp).as_seconds_f32();
    if dt > 0.0 { (a.speed_ms - b.speed_ms) / dt } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        timing::along_lap,
        track::{
            correlator::tests::{around, fix, square},
            Lap,
        },
    };

    fn zones(lap: (Lap, Vec<GPSFix>)) -> Vec<BrakingZone> {
        braking_zones(&along_lap(&square(&[]), &lap.0, &lap.1))
    }

    /// One lap of the 400 m square (gate at 50 m), 10 fixes/s at `speed(s)` m/s, `s` metres round.
    fn lap(speed: impl Fn(f64) -> f32) -> (Lap, Vec<GPSFix>) {
        let (mut s, mut t, mut driven) = (40.0, 0.0, Vec::new());
        while s < 470.0 {
            let v = speed(s);
            driven.push((s, GPSFix { speed_ms: v, ..fix(around(s), t) }));
            (s, t) = (s + v as f64 * 0.1, t + 0.1);
        }
        let cross = |gate| driven.iter().find(|(s, _)| *s >= gate).unwrap().1.timestamp;
        (Lap::new(vec![cross(50.0), cross(450.0)], false), driven.into_iter().map(|(_, f)| f).collect())
    }

    /// `v0` m/s, braking at 8 m/s² from `brake_at` m down to 8 m/s, then back up at 4 m/s².
    fn corner(brake_at: f64, v0: f64) -> impl Fn(f64) -> f32 {
        let slow = (v0 * v0 - 64.0) / 16.0; // metres to get down to 8 m/s
        move |s| {
            let d = s - brake_at;
            let v2 = if d < 0.0 { v0 * v0 } else if d < slow { v0 * v0 - 16.0 * d } else { 64.0 + 8.0 * (d - slow) };
            v2.sqrt().min(v0) as f32
        }
    }

    #[test]
    fn finds_where_and_how_hard_a_corner_brakes() {
        let [ref z] = zones(lap(corner(250.0, 20.0)))[..] else { panic!("one zone") };
        assert!((z.at_m - 200.0).abs() < 3.0 && (z.end_m - 221.0).abs() < 3.0, "{z:?}"); // 250 m round is 200 m past the gate
        assert!((z.entry_ms - 20.0).abs() < 0.5 && (z.exit_ms - 8.0).abs() < 0.5, "{z:?}");
        assert!((z.peak_g - 8.0 / G).abs() < 0.1, "{z:?}");
        assert_eq!(z.path.len(), 16, "1.5 s of fixes, both ends");
    }

    #[test]
    fn steady_or_gently_slowing_laps_have_none() {
        assert!(zones(lap(|_| 15.0)).is_empty());
        assert!(zones(lap(|s| (20.0 - (s - 40.0) / 200.0) as f32)).is_empty()); // 2 m/s over the lap
    }

    #[test]
    fn coasting_before_the_brakes_isnt_braking() {
        // Lifts 100 m early, losing 2.5 m/s, then brakes.
        let brake = corner(250.0, 17.5);
        let [ref z] = zones(lap(|s| if s < 250.0 { 20.0 - (s - 150.0).max(0.0) as f32 / 40.0 } else { brake(s) }))[..] else { panic!("one zone") };
        assert!((z.at_m - 200.0).abs() < 3.0 && (z.entry_ms - 17.5).abs() < 0.5, "{z:?}");
    }

    #[test]
    fn gentle_braking_starts_where_it_does_not_at_top_speed() {
        // Coasts from 20 to 18 m/s over 100 m, then brakes at 1 m/s² (0.1 g) down to 12.
        let [ref z] = zones(lap(|s| match s - 150.0 {
            d if d < 0.0 => 20.0,
            d if d < 100.0 => 20.0 - d as f32 / 50.0,
            d => (324.0 - 2.0 * (d - 100.0)).max(144.0).sqrt() as f32,
        }))[..] else { panic!("one zone") };
        assert!((z.at_m - 200.0).abs() < 3.0 && (z.peak_g - 1.0 / G).abs() < 0.02, "{z:?}");
    }

    #[test]
    fn counterpart_is_the_nearest_zone_for_the_same_corner() {
        let z = |at_m| BrakingZone { at_m, end_m: at_m + 20.0, entry_ms: 20.0, exit_ms: 8.0, peak_g: 0.8, path: Vec::new() };
        let pb = [z(100.0), z(180.0), z(400.0)];
        assert_eq!(z(170.0).counterpart(&pb), Some(&pb[1]));
        assert_eq!(z(300.0).counterpart(&pb), None);
    }
}
