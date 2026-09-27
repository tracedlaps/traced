use std::time::{Duration, Instant};

use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::track::LiveStatus;

use super::{gps_status, kmh, lap_time, track_map::TrackMap, Delta};
use crate::{recorder::RecorderCtx, Route};

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

    rsx! {
        main { class: "live",
            div { class: "speed-row",
                div { class: "speed",
                    span { "{kmh(speed.current_speed_ms)}" }
                    small { "km/h" }
                }
                GDial {}
            }
            dl { class: "stats",
                dt { {t!("stat-top")} } dd { "{kmh(speed.top_speed_ms)} km/h" }
                dt { {t!("stat-avg")} } dd { "{kmh(speed.average_speed_ms)} km/h" }
                dt { {t!("stat-distance")} } dd { "{speed.distance_m / 1000.0:.2} km" }
            }
            TrackMap {}
            if let Some(live) = *ctx.live.read() {
                section { class: "session",
                    match live.status {
                        LiveStatus::Armed => rsx! { p { {t!("waiting-for-start")} } },
                        LiveStatus::InProgress { lap_anchor, sector_anchor, sector } => rsx! {
                            p { {t!("lap-n", n: live.laps + 1)} }
                            // Delta sits beside the clock so it never steals height from the map.
                            div { class: "lap-clock",
                                LiveClock { anchor: lap_anchor, class: "clock" }
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
                                LiveClock { anchor: sector_anchor, class: "clock small" }
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
            p { class: "note", {gps_status(*ctx.status.read())} }
            if ended {
                p { class: "note", {t!("gps-ended")} }
            }
            button {
                class: "stop",
                onclick: move |_| {
                    ctx.stop();
                    navigator().push(Route::Results {});
                },
                {t!("stop")}
            }
        }
    }
}

/// The felt g-force: a ball in a bowl, rolling opposite the acceleration (braking rolls it up,
/// a left turn right), on rings every 0.5 g. Reads the signal itself, so only the dial re-renders.
#[component]
fn GDial() -> Element {
    const MAX: f32 = 1.5;
    let Some(g) = *use_context::<RecorderCtx>().g.read() else {
        return rsx! {};
    };
    // Screen coordinates, y down, held inside the outer ring.
    let at = |x: f32, y: f32| {
        let k = x.hypot(y).max(MAX) / MAX;
        (x / k, y / k)
    };
    let (x, y) = at(-g.lateral_g, g.longitudinal_g);
    let p = g.peaks;
    let peaks = [(0.0, -p.braking), (0.0, p.accelerating), (p.left, 0.0), (-p.right, 0.0)]
        .into_iter()
        .filter(|&(x, y)| x.hypot(y) > 0.05)
        .map(|(x, y)| at(x, y));
    rsx! {
        div { class: if g.aligned { "g-dial" } else { "g-dial calibrating" },
            svg { view_box: "-1.6 -1.6 3.2 3.2",
                for r in [0.5, 1.0, 1.5] {
                    circle { class: "ring", r }
                }
                line { class: "ring", x1: -MAX, x2: MAX }
                line { class: "ring", y1: -MAX, y2: MAX }
                for (cx, cy) in peaks {
                    circle { class: "peak", cx, cy, r: 0.06 }
                }
                circle { class: "ball", r: 0.16, style: "transform: translate({x}px, {y}px)" }
            }
            small {
                if g.aligned {
                    "{g.lateral_g.hypot(g.longitudinal_g):.2} g"
                } else {
                    {t!("g-calibrating")}
                }
            }
        }
    }
}

/// Ticks at 10 Hz on its own, so only the clock text re-renders.
#[component]
fn LiveClock(anchor: Instant, class: &'static str) -> Element {
    let mut now = use_signal(Instant::now);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            now.set(Instant::now());
        }
    });
    rsx! {
        p { class, {lap_time(now().saturating_duration_since(anchor).as_secs_f64())} }
    }
}
