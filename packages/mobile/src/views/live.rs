use std::time::{Duration, Instant};

use traced_core::clock::Clock;

use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::{imu::GForceSnapshot, track::LiveStatus};

use super::{
    gps_status, lap_time,
    map::{Map, View},
    track_map::TrackMap,
    Delta,
};
use crate::{recorder::RecorderCtx, units::Units, Route};

#[component]
pub fn Live() -> Element {
    let ctx = use_context::<RecorderCtx>();
    if ctx.recording.read().is_none() {
        return rsx! {
            main {
                p { {t!("not-recording")} }
                Link { class: "button", to: Route::Tracks {}, {t!("new-recording")} }
            }
        };
    }
    // The recording outlives its fix stream when a mock run ends; its signals are cleared then.
    let ended = ctx.speed.read().is_none();
    let speed = ctx.speed.read().unwrap_or_default();
    let units = Units::current();
    let replaying = *ctx.replaying.read();
    let clock = ctx.recording.read().as_ref().map(|r| r.clock.clone()).unwrap_or_default();

    rsx! {
        main { class: "live",
            div { class: "speed-row",
                div { class: "speed",
                    span { "{units.speed_value(speed.current_speed_ms)}" }
                    small { "{units.speed_unit()}" }
                }
                GDial {}
            }
            dl { class: "stats",
                dt { {t!("stat-top")} } dd { "{units.speed(speed.top_speed_ms)}" }
                dt { {t!("stat-avg")} } dd { "{units.speed(speed.average_speed_ms)}" }
                dt { {t!("stat-distance")} } dd { "{units.distance(speed.distance_m)}" }
            }
            if ctx.track.read().is_none() && replaying.is_some() {
                ReplayMap {}
            } else {
                TrackMap {}
            }
            if let Some(live) = *ctx.live.read() {
                section { class: "session",
                    match live.status {
                        LiveStatus::Armed => rsx! { p { {t!("waiting-for-start")} } },
                        LiveStatus::InProgress { lap_anchor, sector_anchor, sector } => rsx! {
                            p { {t!("lap-n", n: live.laps + 1)} }
                            // Delta sits beside the clock so it never steals height from the map.
                            div { class: "lap-clock",
                                LiveClock { anchor: lap_anchor, clock: clock.clone(), class: "clock" }
                                if let Some(delta) = live.delta {
                                    div { class: "live-delta",
                                        small { {t!("delta")} }
                                        Delta { delta }
                                    }
                                }
                            }
                            // Same color as the sector on the map.
                            div { class: "sector-clock", style: "color: var(--sector-{sector % 4})",
                                span { {t!("sector-n", n: sector + 1)} }
                                LiveClock { anchor: sector_anchor, clock: clock.clone(), class: "clock small" }
                            }
                        },
                        LiveStatus::Finished => rsx! { p { {t!("finished")} } },
                    }
                    if let Some(last) = live.last_sector {
                        p {
                            {t!("last-sector", time: lap_time(last.as_seconds_f64()))} " "
                            if let Some(delta) = live.last_sector_delta { Delta { delta } }
                        }
                    }
                    if let Some(last) = live.last_lap {
                        p {
                            {t!("last-lap", time: lap_time(last.as_seconds_f64()))} " "
                            if let Some(delta) = live.last_lap_delta { Delta { delta } }
                        }
                    }
                    if let Some(best) = live.best {
                        p { class: "best", {t!("best", time: lap_time(best.as_seconds_f64()))} }
                    }
                }
            }
            if replaying.is_some() {
                ReplaySpeed { clock }
            } else {
                p { class: "note", {gps_status(*ctx.status.read())} }
            }
            if ended {
                p { class: "note", {if replaying.is_some() { t!("replay-ended") } else { t!("gps-ended") }} }
            }
            button {
                class: "stop",
                onclick: move |_| {
                    ctx.stop();
                    navigator().push(replaying.map_or(Route::Results {}, |id| Route::SavedResults { id }));
                },
                {if replaying.is_some() { t!("end-replay") } else { t!("stop") }}
            }
        }
    }
}

/// The felt g-force, like a car's g-meter: a ball in a bowl rolling opposite the acceleration
/// (braking rolls it up, a right turn left), with each side's g now and its peak.
/// Reads the signal itself, so only the dial re-renders.
#[component]
fn GDial() -> Element {
    let g = *use_context::<RecorderCtx>().g.read();
    rsx! { if let Some(g) = g { GMeterDial { g } } }
}

#[component]
pub fn GMeterDial(g: GForceSnapshot) -> Element {
    /// g at the bowl's rim.
    const MAX: f32 = 1.0;
    /// Radii of the bowl (holding the ball) and the outer band (holding the peaks).
    const BOWL: f32 = 0.62;
    const RIM: f32 = 0.97;
    // Screen coordinates, y down; the ball is held inside the bowl.
    let at = |deg: f32, r: f32| (deg.to_radians().cos() * r, deg.to_radians().sin() * r);
    let k = BOWL / g.lateral_g.hypot(g.longitudinal_g).max(MAX);
    let (x, y) = (-g.lateral_g * k, g.longitudinal_g * k);
    // Each side the ball rolls to (degrees clockwise from the right): its g now as a number in the
    // band, and its peak as an arc at that g across its quarter of the bowl.
    let (p, lon, lat) = (g.peaks, g.longitudinal_g, g.lateral_g);
    let sides = [(-90.0, -lon, p.braking), (90.0, lon, p.accelerating), (180.0, lat, p.right), (0.0, -lat, p.left)];
    let sides = sides.map(|(deg, now, v)| {
        let r = v.min(MAX) * BOWL / MAX;
        let ((x0, y0), (x1, y1)) = (at(deg - 40.0, r), at(deg + 40.0, r));
        (at(deg, (BOWL + RIM) / 2.0), now.max(0.0), format!("M{x0} {y0}A{r} {r} 0 0 1 {x1} {y1}"), v)
    });
    let d = std::f32::consts::FRAC_1_SQRT_2;
    rsx! {
        div { class: if g.aligned { "g-dial" } else { "g-dial calibrating" },
            svg { view_box: "-1 -1 2 2",
                for (r, class) in [(RIM, "rim"), (BOWL, "bowl"), (BOWL / 2.0, "bowl")] {
                    circle { class, r }
                }
                for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                    line { class: "spoke", x1: sx * d * BOWL, y1: sy * d * BOWL, x2: sx * d * RIM, y2: sy * d * RIM }
                }
                for ((x, y), now, arc, v) in sides {
                    text { x, y, "{now:.1}" }
                    if v > 0.05 {
                        path { class: "peak", d: arc }
                    }
                }
                circle { class: "ball", r: 0.08, style: "transform: translate({x}px, {y}px)" }
            }
            if !g.aligned {
                small { {t!("g-calibrating")} }
            }
        }
    }
}

/// A replay without a track: its whole path, the stretch driven so far in colour, and the dot
/// the map follows.
#[component]
fn ReplayMap() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let view = use_memo(move || {
        let fixes = ctx.replay_fixes.read().clone().unwrap_or_default();
        let fix = (ctx.fix)();
        // No fix before the first, or after the last, when its signals are cleared.
        let passed = match fix {
            Some(f) => fixes.partition_point(|p| p.timestamp <= f.timestamp),
            None if ctx.speed.read().is_none() => fixes.len(),
            None => 0,
        };
        View {
            trace: fixes.iter().map(|&f| f.into()).collect(),
            passed: Some(passed),
            me: fix.map(Into::into),
            ..View::default()
        }
    });
    rsx! {
        div { class: "recording-map",
            Map { view, on_event: |_| {}, locate: false, search: false }
        }
    }
}

/// How fast the replay plays.
#[component]
fn ReplaySpeed(clock: Clock) -> Element {
    let mut speed = use_signal(|| clock.speed());
    rsx! {
        div { class: "modes",
            for x in [1.0, 2.0, 5.0, 10.0] {
                button {
                    class: if speed() == x { "button" } else { "button secondary" },
                    onclick: {
                        let clock = clock.clone();
                        move |_| {
                            clock.set_speed(x);
                            speed.set(x);
                        }
                    },
                    "{x}×"
                }
            }
        }
    }
}

/// Ticks at 10 Hz (real time) on its own, so only the clock text re-renders. Reads `clock`,
/// the recording's, so a replay's runs at its speed.
#[component]
fn LiveClock(anchor: Instant, clock: Clock, class: &'static str) -> Element {
    let mut now = use_signal(|| clock.now());
    use_future(move || {
        let clock = clock.clone();
        async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                now.set(clock.now());
            }
        }
    });
    rsx! {
        p { class, {lap_time(now().saturating_duration_since(anchor).as_secs_f64())} }
    }
}
