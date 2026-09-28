use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::sync::watch;

/// The time a recording runs on: the device's for a live one, or a replay's, which runs at an
/// adjustable speed. Lap clocks are anchored and read on it, so they keep pace with a replay.
#[derive(Clone, Debug, Default)]
pub struct Clock(Option<Arc<Replay>>);

#[derive(Debug)]
struct Replay {
    /// Real and virtual time at the last speed change.
    base: Mutex<(Instant, Instant)>,
    speed: watch::Sender<f64>,
}

impl PartialEq for Clock {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (a, b) => a.is_none() && b.is_none(),
        }
    }
}

impl Clock {
    /// A clock starting now, running `speed` times faster than real time.
    pub fn replay(speed: f64) -> Self {
        let now = Instant::now();
        Self(Some(Arc::new(Replay { base: Mutex::new((now, now)), speed: watch::Sender::new(speed) })))
    }

    pub fn now(&self) -> Instant {
        let Some(r) = &self.0 else { return Instant::now() };
        let (real, virt) = *r.base.lock().unwrap();
        virt + real.elapsed().mul_f64(*r.speed.borrow())
    }

    /// 1 for the device clock.
    pub fn speed(&self) -> f64 {
        self.0.as_ref().map_or(1.0, |r| *r.speed.borrow())
    }

    /// Changes a replay's speed from now on; the device clock ignores it.
    pub fn set_speed(&self, speed: f64) {
        let Some(r) = &self.0 else { return };
        let mut base = r.base.lock().unwrap();
        let now = Instant::now();
        *base = (now, base.1 + (now - base.0).mul_f64(*r.speed.borrow()));
        r.speed.send_replace(speed);
    }

    /// Waits until the clock reads `at`, re-timing the wait if the speed changes meanwhile.
    pub async fn sleep_until(&self, at: Instant) {
        let Some(r) = &self.0 else { return tokio::time::sleep_until(at.into()).await };
        let mut speed = r.speed.subscribe();
        loop {
            let left = at.saturating_duration_since(self.now());
            if left.is_zero() {
                return;
            }
            let real: Duration = left.div_f64(*speed.borrow_and_update());
            tokio::select! {
                _ = tokio::time::sleep(real) => {}
                _ = speed.changed() => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replay_runs_at_its_speed_and_retimes_a_wait_on_change() {
        let clock = Clock::replay(100.0);
        let (start, real) = (clock.now(), Instant::now());
        let speed_up = async {
            tokio::time::sleep(Duration::from_millis(50)).await; // 5 s in
            clock.set_speed(1000.0);
        };
        tokio::join!(clock.sleep_until(start + Duration::from_secs(10)), speed_up);
        // 50 ms at 100×, then the other 5 s at 1000×: ~55 ms, not the 100 ms at 100×.
        assert!(real.elapsed() < Duration::from_millis(80), "{:?}", real.elapsed());
        assert!(clock.now() >= start + Duration::from_secs(10));
    }
}
