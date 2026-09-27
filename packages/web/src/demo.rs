//! The hero's phone: the app's Live screen, lapping Mondercange on a simulated GPS.
//! Drives like `MockGPSProvider::drive` (traced_core doesn't build for wasm): slows for corners,
//! brakes into and accelerates out of them, with each lap's sectors a little faster or slower.

use std::sync::LazyLock;

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

const CSV: &str = include_str!("../../mobile/tracks/mondercange_acl.csv");
/// Sector gates start at these waypoints, as in the app's preset.
const SECTOR_WAYPOINTS: [usize; 2] = [18, 32];
const TOP_MS: f64 = 70.0 / 3.6;
/// Cornering grip, acceleration and braking (m/s²), and the curvature window (m), as the mock's.
const GRIP: f64 = 8.0;
const ACCEL: f64 = 3.0;
const BRAKE: f64 = 6.0;
const WINDOW: f64 = 10.0;
/// Curve samples per waypoint gap.
const SMOOTH: usize = 8;
/// Lap seconds per real second, and the frame interval (ms).
const SPEEDUP: f64 = 4.0;
const TICK_MS: u32 = 33;

/// The track every ~1 m, in metres with y down, and the speed at each point.
struct Circuit {
    pts: Vec<(f64, f64)>,
    v: Vec<f64>,
    ds: f64,
    /// Point index where each sector starts; the start line is 0.
    sectors: [usize; 3],
    length: f64,
}

static CIRCUIT: LazyLock<Circuit> = LazyLock::new(|| {
    let ll: Vec<(f64, f64)> = CSV
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut f = l.split(',').map(|x| x.parse::<f64>());
            Some((f.next()?.ok()?, f.next()?.ok()?))
        })
        .collect();
    let (lat0, lon0) = ll[0];
    let r = 6_371_000f64.to_radians(); // metres per degree
    let way: Vec<_> = ll.iter().map(|&(la, lo)| ((lo - lon0) * r * lat0.to_radians().cos(), (lat0 - la) * r)).collect();
    // The app's Catmull-Rom curve (see mobile's `track_map::curves`), sampled SMOOTH times per waypoint.
    let n = way.len();
    let dist = |a: (f64, f64), b: (f64, f64)| (b.0 - a.0).hypot(b.1 - a.1);
    let tangent = |i: usize| {
        let (a, p, b) = (way[(i + n - 1) % n], way[i], way[(i + 1) % n]);
        let d = (dist(a, p) + dist(p, b)).max(f64::EPSILON);
        ((b.0 - a.0) / d, (b.1 - a.1) / d)
    };
    let mut way: Vec<_> = (0..n)
        .flat_map(|i| {
            let (p, q, m1, m2) = (way[i], way[(i + 1) % n], tangent(i), tangent((i + 1) % n));
            let l = dist(p, q) / 3.0;
            let (c1, c2) = ((p.0 + m1.0 * l, p.1 + m1.1 * l), (q.0 - m2.0 * l, q.1 - m2.1 * l));
            (0..SMOOTH).map(move |k| {
                let (t, u) = (k as f64 / SMOOTH as f64, 1.0 - k as f64 / SMOOTH as f64);
                let b = |p: f64, c1: f64, c2: f64, q: f64| u * u * u * p + 3.0 * u * t * (u * c1 + t * c2) + t * t * t * q;
                (b(p.0, c1.0, c2.0, q.0), b(p.1, c1.1, c2.1, q.1))
            })
        })
        .collect();
    way.push(way[0]);
    let mut at = vec![0.0];
    for w in way.windows(2) {
        at.push(at.last().unwrap() + (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1));
    }
    let length = *at.last().unwrap();
    let m = length.ceil() as usize;
    let ds = length / m as f64;
    let pts: Vec<_> = (0..m)
        .map(|i| {
            let d = i as f64 * ds;
            let s = at.partition_point(|&a| a <= d).clamp(1, way.len() - 1);
            let f = (d - at[s - 1]) / (at[s] - at[s - 1]);
            (way[s - 1].0 + f * (way[s].0 - way[s - 1].0), way[s - 1].1 + f * (way[s].1 - way[s - 1].1))
        })
        .collect();

    let w = (WINDOW / ds).round() as usize;
    let heading = |a: (f64, f64), b: (f64, f64)| (b.1 - a.1).atan2(b.0 - a.0);
    let mut v: Vec<f64> = (0..m)
        .map(|i| {
            let (a, b, c) = (pts[(i + m - w) % m], pts[i], pts[(i + w) % m]);
            let turn = heading(b, c) - heading(a, b);
            (GRIP * WINDOW / turn.sin().atan2(turn.cos()).abs()).sqrt().min(TOP_MS)
        })
        .collect();
    // Twice round, so the limits carry across the start line.
    for _ in 0..2 {
        for i in 0..m {
            v[(i + 1) % m] = v[(i + 1) % m].min((v[i].powi(2) + 2.0 * ACCEL * ds).sqrt());
        }
        for i in (0..m).rev() {
            v[i] = v[i].min((v[(i + 1) % m].powi(2) + 2.0 * BRAKE * ds).sqrt());
        }
    }
    let idx = |wp: usize| (at[wp * SMOOTH] / ds).round() as usize;
    Circuit { sectors: [0, idx(SECTOR_WAYPOINTS[0]), idx(SECTOR_WAYPOINTS[1])], pts, v, ds, length }
});

impl Circuit {
    fn sector(&self, i: usize) -> usize {
        self.sectors.iter().rposition(|&s| s <= i).unwrap()
    }

    /// Seconds to reach each point (and the line again, last) at `pace` per sector.
    fn lap(&self, pace: [f64; 3]) -> Vec<f64> {
        let m = self.pts.len();
        let mut t = vec![0.0];
        for i in 0..m {
            t.push(t[i] + 2.0 * self.ds / (self.v[i] + self.v[(i + 1) % m]) / pace[self.sector(i)]);
        }
        t
    }

    fn path(&self, from: usize, to: usize) -> String {
        let m = self.pts.len();
        (from..=to).map(|i| self.pts[i % m]).enumerate().map(|(k, (x, y))| format!("{}{x:.1} {y:.1}", if k == 0 { "M" } else { "L" })).collect()
    }
}

struct Sim {
    lap: u32,
    /// Seconds into this lap, and when this lap and the best reach each point.
    t: f64,
    cur: Vec<f64>,
    best: Vec<f64>,
    /// (time, delta to best) of the last lap, and of the last sector this lap.
    last_lap: Option<(f64, f64)>,
    last_sector: Option<(f64, f64)>,
    sector: usize,
    top_kmh: f64,
    /// Metres and seconds of the laps before this one.
    done_m: f64,
    done_s: f64,
    seed: u64,
}

impl Sim {
    /// Mid-way through lap 3, having just set a best.
    fn new() -> Self {
        let c = &*CIRCUIT;
        let best = c.lap([1.0; 3]);
        let lap = best[c.pts.len()];
        let mut sim = Sim {
            lap: 3,
            t: 0.46 * lap,
            cur: best.clone(),
            best,
            last_lap: Some((lap, -0.41)),
            last_sector: None,
            sector: 0,
            top_kmh: c.v.iter().cloned().fold(0.0, f64::max) * 3.6,
            done_m: 2.0 * c.length,
            done_s: 2.0 * lap + 0.41,
            seed: 0x9e37_79b9_7f4a_7c15,
        };
        sim.cur = c.lap(sim.pace());
        sim
    }

    /// Each sector 2.5% slower to 2% faster than the nominal lap.
    fn pace(&mut self) -> [f64; 3] {
        [(); 3].map(|_| {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            0.975 + (self.seed % 1000) as f64 / 1000.0 * 0.045
        })
    }

    /// The point index reached.
    fn index(&self) -> usize {
        self.cur.partition_point(|&x| x <= self.t).clamp(1, CIRCUIT.pts.len()) - 1
    }

    fn step(&mut self, dt: f64) {
        let c = &*CIRCUIT;
        let m = c.pts.len();
        self.t += dt;
        if self.t >= self.cur[m] {
            let lap = self.cur[m];
            self.last_lap = Some((lap, lap - self.best[m]));
            self.last_sector = None;
            if lap < self.best[m] {
                self.best = self.cur.clone();
            }
            (self.t, self.done_s, self.done_m, self.lap) = (self.t - lap, self.done_s + lap, self.done_m + c.length, self.lap + 1);
            self.cur = c.lap(self.pace());
        }
        let s = c.sector(self.index());
        if s > self.sector {
            let (a, b) = (c.sectors[self.sector], c.sectors[s]);
            let time = self.cur[b] - self.cur[a];
            self.last_sector = Some((time, time - (self.best[b] - self.best[a])));
        }
        self.sector = s;
        self.top_kmh = self.top_kmh.max(self.speed_kmh());
    }

    fn speed_kmh(&self) -> f64 {
        let i = self.index();
        CIRCUIT.ds / (self.cur[i + 1] - self.cur[i]) * 3.6
    }
}

fn lap_time(secs: f64) -> String {
    let tenths = (secs.max(0.0) * 10.0).floor() as u64;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

#[component]
fn Delta(secs: f64) -> Element {
    rsx! { span { class: if secs < 0.0 { "delta ahead" } else { "delta behind" }, "{secs:+.2}" } }
}

#[component]
pub fn Phone() -> Element {
    let mut sim = use_signal(Sim::new);
    use_future(move || async move {
        let still = document::eval("return matchMedia('(prefers-reduced-motion: reduce)').matches").join::<bool>().await;
        if still.unwrap_or(false) {
            return;
        }
        loop {
            TimeoutFuture::new(TICK_MS).await;
            sim.write().step(TICK_MS as f64 / 1000.0 * SPEEDUP);
        }
    });

    let c = &*CIRCUIT;
    let s = sim.read();
    let (m, i) = (c.pts.len(), s.index());
    let (x, y) = c.pts[i];
    let ends = [c.sectors[1], c.sectors[2], m];
    let (xs, ys) = (c.pts.iter().map(|p| p.0), c.pts.iter().map(|p| p.1));
    let (x0, x1) = xs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    let (y0, y1) = ys.fold((f64::MAX, f64::MIN), |(a, b), y| (a.min(y), b.max(y)));
    let pad = (x1 - x0).max(y1 - y0) * 0.04;
    let gate = |g: usize| {
        let (a, b) = (c.pts[g], c.pts[(g + 3) % m]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let k = pad * 1.2 / dx.hypot(dy);
        (a.0 - dy * k, a.1 + dx * k, a.0 + dy * k, a.1 - dx * k)
    };
    let avg = (s.done_m + i as f64 * c.ds) / (s.done_s + s.t) * 3.6;

    rsx! {
        div { class: "phone", aria_label: "Traced recording a lap: speed, lap clock and live delta",
            div { class: "screen",
                div { class: "speed", span { "{s.speed_kmh():.0}" } small { "km/h" } }
                dl { class: "stats",
                    dt { "Top" } dd { "{s.top_kmh:.0} km/h" }
                    dt { "Avg" } dd { "{avg:.0} km/h" }
                    dt { "Distance" } dd { "{(s.done_m + i as f64 * c.ds) / 1000.0:.2} km" }
                }
                svg { class: "track-map", view_box: "{x0 - pad} {y0 - pad} {x1 - x0 + 2.0 * pad} {y1 - y0 + 2.0 * pad}",
                    path { class: "outline", d: "{c.path(0, m)}" }
                    for k in 0..3 {
                        path { class: if k == s.sector { "sector current" } else { "sector" }, stroke: "var(--s{k})", d: c.path(c.sectors[k], ends[k]) }
                    }
                    for (k, g) in c.sectors.into_iter().enumerate() {
                        {
                            let (x1, y1, x2, y2) = gate(g);
                            rsx! { line { class: if k == 0 { "gate boundary" } else { "gate" }, x1, y1, x2, y2 } }
                        }
                    }
                    circle { class: "you", cx: x, cy: y, r: pad * 0.6 }
                }
                div { class: "session",
                    p { "Lap {s.lap}" }
                    div { class: "lap-clock",
                        p { class: "clock", {lap_time(s.t)} }
                        div { class: "live-delta", small { "Delta" } Delta { secs: s.cur[i] - s.best[i] } }
                    }
                    div { class: "sector-clock", color: "var(--s{s.sector})",
                        span { "Sector {s.sector + 1}" }
                        span { class: "clock small", {lap_time(s.t - s.cur[c.sectors[s.sector]])} }
                    }
                    if let Some((time, delta)) = s.last_sector {
                        p { "Last sector {lap_time(time)} " Delta { secs: delta } }
                    } else if let Some((time, delta)) = s.last_lap {
                        p { "Last lap {lap_time(time)} " Delta { secs: delta } }
                    }
                    p { class: "best", "Best {lap_time(s.best[m])}" }
                }
                p { class: "note", "GPS fix · Fused · 14 satellites · ±3 m" }
                div { class: "stop", "Stop" }
            }
        }
    }
}
