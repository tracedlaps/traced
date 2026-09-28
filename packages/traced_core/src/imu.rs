//! Accelerometer samples and the `GMeter` they feed.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{gps::GPSFix, recorder::FixConsumer, track::Session};

/// Standard gravity (m/s²).
pub const G: f32 = 9.80665;
/// Time constant (s) of the smoothing that takes out engine and road vibration.
const SMOOTHING_S: f32 = 0.2;
/// At most one snapshot per this, however fast samples come.
const PUBLISH_EVERY: Duration = Duration::from_millis(50);
/// GPS showing the vehicle accelerating at least this much (m/s², speed change and turn together),
/// at least this fast (m/s), over at most this gap, shows which way is forward.
const ALIGN_MIN_ACCEL: f32 = 1.0;
const ALIGN_MIN_SPEED: f32 = 3.0;
const ALIGN_MAX_GAP: Duration = Duration::from_secs(2);
/// Weight of each such acceleration in the forward estimate, so it follows a mount that moves.
const ALIGN_RATE: f32 = 0.2;
/// Accelerations needed before the estimate is trusted.
const ALIGN_EVENTS: u32 = 3;
/// Fixes less accurate than this (m) teach nothing.
const MAX_ACCURACY_M: f32 = 25.0;

type Vec3 = [f32; 3];

/// One accelerometer reading in the phone's own axes (m/s²), as Android reports them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImuSample {
    pub timestamp: DateTime<Utc>,
    /// Acceleration, gravity removed.
    pub accel: Vec3,
    /// Gravity as the accelerometer reads it: pointing up, away from the ground.
    pub gravity: Vec3,
}

/// The largest g-forces so far, each as a magnitude.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GPeaks {
    pub braking: f32,
    pub accelerating: f32,
    pub left: f32,
    pub right: f32,
}

impl GPeaks {
    fn add(&mut self, longitudinal: f32, lateral: f32) {
        (self.braking, self.accelerating) = (self.braking.max(-longitudinal), self.accelerating.max(longitudinal));
        (self.left, self.right) = (self.left.max(-lateral), self.right.max(lateral));
    }
}

/// A finished recording's g-forces: its peaks, and each completed lap's (or the run's).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct GForces {
    pub peaks: GPeaks,
    pub laps: Vec<GPeaks>,
}

/// The vehicle's acceleration (g) in its own frame, smoothed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GForceSnapshot {
    /// Positive turning right.
    pub lateral_g: f32,
    /// Positive speeding up, negative braking.
    pub longitudinal_g: f32,
    /// False until GPS has shown which way is forward; until then the rest stays zero.
    pub aligned: bool,
    pub peaks: GPeaks,
}

/// Turns device-frame samples into vehicle-frame g-forces, whatever way the phone is mounted:
/// gravity gives down, and GPS gives forward. Between two fixes GPS knows the vehicle's
/// acceleration in its own frame (speed change ahead, speed × turn rate to the side), and the
/// phone felt that same acceleration in device axes; together they pin forward down. So braking,
/// speeding up and cornering all teach it.
pub struct GMeter {
    tx: watch::Sender<GForceSnapshot>,
    snapshot: GForceSnapshot,
    last_sample: Option<DateTime<Utc>>,
    published: Option<DateTime<Utc>>,
    /// Horizontal acceleration, smoothed, in device axes.
    smooth: Vec3,
    /// Horizontal acceleration summed since the last fix, and how many samples.
    since_fix: (Vec3, u32),
    /// Up, in device axes, from the latest sample.
    up: Vec3,
    last_fix: Option<GPSFix>,
    /// Running forward estimate in device axes; not unit length.
    forward: Vec3,
    events: u32,
    /// Every aligned reading so far, (time, longitudinal, lateral), to find a lap's peaks.
    readings: Vec<(DateTime<Utc>, f32, f32)>,
}

impl GMeter {
    pub fn new() -> (Self, watch::Receiver<GForceSnapshot>) {
        let (tx, rx) = watch::channel(GForceSnapshot::default());
        let meter = Self {
            tx,
            snapshot: GForceSnapshot::default(),
            last_sample: None,
            published: None,
            smooth: [0.0; 3],
            since_fix: ([0.0; 3], 0),
            up: [0.0; 3],
            last_fix: None,
            forward: [0.0; 3],
            events: 0,
            readings: Vec::new(),
        };
        (meter, rx)
    }

    pub fn snapshot(&self) -> GForceSnapshot {
        self.snapshot
    }

    /// The peaks overall, and within each of `session`'s laps.
    pub fn into_forces(self, session: Option<&Session>) -> GForces {
        let laps = session.map_or(&[][..], Session::laps).iter().map(|lap| {
            let (from, to) = (lap.crossings()[0], *lap.crossings().last().unwrap());
            let mut p = GPeaks::default();
            let start = self.readings.partition_point(|r| r.0 < from);
            self.readings[start..].iter().take_while(|r| r.0 <= to).for_each(|&(_, lon, lat)| p.add(lon, lat));
            p
        });
        GForces { peaks: self.snapshot.peaks, laps: laps.collect() }
    }
}

impl FixConsumer for GMeter {
    fn on_sample(&mut self, s: ImuSample) {
        let Some(up) = unit(s.gravity) else { return };
        self.up = up;
        let h = horizontal(s.accel, up);
        let dt = self.last_sample.replace(s.timestamp).map(|t| (s.timestamp - t).as_seconds_f32());
        let a = dt.map_or(1.0, |dt| 1.0 - (-dt.max(0.0) / SMOOTHING_S).exp());
        self.smooth = add(self.smooth, scale(sub(h, self.smooth), a));
        self.since_fix = (add(self.since_fix.0, h), self.since_fix.1 + 1);

        let forward = unit(horizontal(self.forward, up)).filter(|_| self.events >= ALIGN_EVENTS);
        if let Some(fwd) = forward {
            let (lon, lat) = (dot(self.smooth, fwd) / G, dot(self.smooth, cross(fwd, up)) / G);
            self.snapshot.peaks.add(lon, lat);
            self.readings.push((s.timestamp, lon, lat));
            (self.snapshot.longitudinal_g, self.snapshot.lateral_g) = (lon, lat);
        }
        self.snapshot.aligned = forward.is_some();
        if self.published.is_none_or(|t| (s.timestamp - t).to_std().is_ok_and(|d| d >= PUBLISH_EVERY)) {
            self.published = Some(s.timestamp);
            self.tx.send_replace(self.snapshot);
        }
    }

    fn on_fix(&mut self, fix: GPSFix) {
        let (sum, n) = std::mem::take(&mut self.since_fix);
        if fix.accuracy > MAX_ACCURACY_M {
            return;
        }
        let Some(prev) = self.last_fix.replace(fix) else { return };
        let Ok(dt) = (fix.timestamp - prev.timestamp).to_std() else { return };
        if dt.is_zero() || dt > ALIGN_MAX_GAP || n == 0 || prev.speed_ms.min(fix.speed_ms) < ALIGN_MIN_SPEED {
            return;
        }
        let dt = dt.as_secs_f32();
        // The vehicle's acceleration as GPS saw it: ahead, and to the right (compass heading turns clockwise).
        let turn = match (prev.heading_deg, fix.heading_deg) {
            (Some(a), Some(b)) => ((b - a + 540.0).rem_euclid(360.0) - 180.0).to_radians(),
            _ => 0.0,
        };
        let (ahead, right) = ((fix.speed_ms - prev.speed_ms) / dt, (prev.speed_ms + fix.speed_ms) / 2.0 * turn / dt);
        let gps = ahead.hypot(right);
        let mean = scale(sum, 1.0 / n as f32);
        // The phone must have felt it too, or the samples and the fix are out of step.
        if gps < ALIGN_MIN_ACCEL || norm(mean) < gps / 2.0 {
            return;
        }
        // mean = ahead·F + right·R with R = F × up, so ahead·mean − right·(mean × up) = gps²·F.
        let Some(seen) = unit(sub(scale(mean, ahead), scale(cross(mean, self.up), right))) else { return };
        self.forward = add(scale(self.forward, 1.0 - ALIGN_RATE), scale(seen, ALIGN_RATE));
        self.events += 1;
    }
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    add(a, scale(b, -1.0))
}

fn scale(a: Vec3, k: f32) -> Vec3 {
    a.map(|x| x * k)
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

fn unit(a: Vec3) -> Option<Vec3> {
    let n = norm(a);
    (n > 1e-6).then(|| scale(a, 1.0 / n))
}

/// `a` without its component along the unit vector `up`.
fn horizontal(a: Vec3, up: Vec3) -> Vec3 {
    sub(a, scale(up, dot(a, up)))
}

/// How a phone sits in a vehicle: a rotation from the vehicle's (forward, left, up) axes to the device's.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mount([Vec3; 3]);

impl Mount {
    /// Turned by `yaw`, then `pitch`, then `roll` (rad) about the vehicle's up, left and forward axes.
    pub(crate) fn new(yaw: f32, pitch: f32, roll: f32) -> Self {
        let (sy, cy, sp, cp, sr, cr) = (yaw.sin(), yaw.cos(), pitch.sin(), pitch.cos(), roll.sin(), roll.cos());
        // Rz(yaw)·Ry(pitch)·Rx(roll), transposed: each row is a vehicle axis in device terms.
        let r = [[cy * cp, cy * sp * sr - sy * cr, cy * sp * cr + sy * sr], [sy * cp, sy * sp * sr + cy * cr, sy * sp * cr - cy * sr], [-sp, cp * sr, cp * cr]];
        Self(std::array::from_fn(|i| std::array::from_fn(|j| r[j][i])))
    }

    /// What the phone reads while the vehicle accelerates by `longitudinal` and `lateral` (m/s², right positive).
    pub(crate) fn sample(&self, timestamp: DateTime<Utc>, longitudinal: f32, lateral: f32) -> ImuSample {
        let [f, l, u] = self.0;
        ImuSample { timestamp, accel: add(scale(f, longitudinal), scale(l, -lateral)), gravity: scale(u, G) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speedometer::tests::fix;

    const HZ: i64 = 50;

    struct Drive {
        meter: GMeter,
        mount: Mount,
        t_ms: i64,
        speed: f32,
        /// Compass heading (rad).
        heading: f32,
    }

    impl Drive {
        /// Starts at 20 m/s.
        fn new(mount: Mount) -> Self {
            Self { meter: GMeter::new().0, mount, t_ms: 0, speed: 20.0, heading: 0.0 }
        }

        /// One second at a constant acceleration (m/s², right positive), then the fix that ends it.
        fn second(&mut self, longitudinal: f32, lateral: f32) {
            for _ in 0..HZ {
                self.t_ms += 1000 / HZ;
                let at = DateTime::UNIX_EPOCH + chrono::Duration::milliseconds(self.t_ms);
                self.meter.on_sample(self.mount.sample(at, longitudinal, lateral));
            }
            let v = self.speed + longitudinal / 2.0;
            (self.speed, self.heading) = (self.speed + longitudinal, self.heading + lateral / v);
            let heading = Some(self.heading.to_degrees().rem_euclid(360.0));
            self.meter.on_fix(GPSFix { heading_deg: heading, ..fix(self.t_ms / 1000, self.speed) });
        }

        /// Speeds up then brakes `n` times.
        fn learn(&mut self, n: usize) {
            self.second(0.0, 0.0);
            for _ in 0..n {
                self.second(3.0, 0.0);
                self.second(-3.0, 0.0);
            }
        }

        fn g(&self) -> GForceSnapshot {
            self.meter.snapshot()
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.02
    }

    #[test]
    fn mount_keeps_gravity_and_axes() {
        let m = Mount::new(0.7, -1.1, 2.3);
        let s = m.sample(DateTime::UNIX_EPOCH, 1.0, 0.0);
        assert!(close(norm(s.gravity), G) && close(norm(s.accel), 1.0));
        assert!(close(dot(s.accel, s.gravity), 0.0));
        let [f, l, u] = m.0;
        assert!(close(dot(cross(f, l), u), 1.0), "right-handed");
    }

    #[test]
    fn unaligned_until_gps_shows_forward() {
        let mut d = Drive::new(Mount::new(0.0, 0.0, 0.0));
        d.second(5.0, 5.0);
        assert_eq!(d.g(), GForceSnapshot::default());
        d.learn(1);
        assert!(!d.g().aligned);
        d.learn(1);
        assert!(d.g().aligned);
    }

    #[test]
    fn reads_the_vehicle_frame_on_a_tilted_sideways_mount() {
        // Landscape, leaning back, facing the driver.
        let mut d = Drive::new(Mount::new(std::f32::consts::PI, 1.2, std::f32::consts::FRAC_PI_2));
        d.learn(3);
        d.second(0.0, 0.5 * G);
        assert!(close(d.g().lateral_g, 0.5) && close(d.g().longitudinal_g, 0.0), "{:?}", d.g());
        d.second(-0.8 * G, -0.3 * G);
        assert!(close(d.g().longitudinal_g, -0.8) && close(d.g().lateral_g, -0.3), "{:?}", d.g());
        let p = d.g().peaks;
        assert!(close(p.braking, 0.8) && close(p.right, 0.5) && close(p.left, 0.3), "{p:?}");
    }

    #[test]
    fn follows_a_mount_that_turns() {
        let mut d = Drive::new(Mount::new(0.3, 0.2, 0.0));
        d.learn(3);
        d.mount = Mount::new(0.3 + std::f32::consts::FRAC_PI_2, 0.2, 0.0);
        d.learn(10);
        d.second(-0.5 * G, 0.0);
        assert!(close(d.g().longitudinal_g, -0.5), "{:?}", d.g());
    }

    #[test]
    fn corners_alone_teach_forward() {
        let mut d = Drive::new(Mount::new(-2.0, 0.9, 0.4));
        d.second(0.0, 0.0);
        for _ in 0..3 {
            d.second(0.0, 0.6 * G);
            d.second(0.0, -0.4 * G);
        }
        assert!(d.g().aligned);
        d.second(-0.7 * G, 0.2 * G);
        assert!(close(d.g().longitudinal_g, -0.7) && close(d.g().lateral_g, 0.2), "{:?}", d.g());
    }

    #[test]
    fn slow_or_gentle_driving_teaches_nothing() {
        let mut d = Drive::new(Mount::new(0.0, 0.0, 0.0));
        d.speed = 1.0;
        d.second(0.0, 0.0);
        for _ in 0..5 {
            d.second(1.5, 0.0); // too slow
            d.second(-1.5, 0.0);
        }
        d.speed = 20.0;
        for _ in 0..5 {
            d.second(0.6, 0.5); // too gentle
            d.second(-0.6, -0.5);
        }
        assert!(!d.g().aligned);
    }

    #[test]
    fn lap_peaks_only_count_their_lap() {
        use crate::track::{Completed, Lap};
        let mut d = Drive::new(Mount::new(0.4, 0.3, 0.0));
        d.learn(3); // 7 s
        d.second(-0.5 * G, 0.0);
        d.second(0.0, 0.3 * G);
        d.second(-0.9 * G, 0.0);
        let at = |ms| DateTime::UNIX_EPOCH + chrono::Duration::milliseconds(ms);
        let lap = Lap::new(vec![at(7_500), at(9_000)], false);
        let session = Session { track_id: uuid::Uuid::nil(), completed: Completed::Laps(vec![lap]) };
        let g = d.meter.into_forces(Some(&session));
        assert!(close(g.peaks.braking, 0.9));
        let [lap] = g.laps[..] else { panic!("one lap") };
        assert!(close(lap.braking, 0.5) && close(lap.right, 0.3), "{lap:?}");
    }

    #[test]
    fn publishes_at_most_every_50_ms() {
        let (mut meter, mut rx) = GMeter::new();
        let mount = Mount::new(0.0, 0.0, 0.0);
        let mut published = 0;
        for ms in 0..1000 {
            meter.on_sample(mount.sample(DateTime::UNIX_EPOCH + chrono::Duration::milliseconds(ms), 0.0, 0.0));
            published += usize::from(rx.has_changed().unwrap());
            rx.mark_unchanged();
        }
        assert_eq!(published, 20);
    }
}
