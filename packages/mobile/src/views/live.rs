use std::time::{Duration, Instant};

use dioxus::prelude::*;
use traced_core::track::LiveStatus;

use super::{kmh, lap_time, track_map::TrackMap};
use crate::{recorder::RecorderCtx, Route};

#[component]
pub fn Live() -> Element {
    let ctx = use_context::<RecorderCtx>();
    if ctx.recording.read().is_none() {
        return rsx! {
            main {
                p { "Not recording." }
                Link { class: "button", to: Route::Setup {}, "New recording" }
            }
        };
    }
    // The recording outlives its fix stream when a mock run ends; its signals are cleared then.
    let ended = ctx.speed.read().is_none();
    let speed = ctx.speed.read().unwrap_or_default();

    rsx! {
        main {
            div { class: "speed",
                span { "{kmh(speed.current_speed_ms)}" }
                small { "km/h" }
            }
            dl { class: "stats",
                dt { "Top" } dd { "{kmh(speed.top_speed_ms)} km/h" }
                dt { "Avg" } dd { "{kmh(speed.average_speed_ms)} km/h" }
                dt { "Distance" } dd { "{speed.distance_m / 1000.0:.2} km" }
            }
            TrackMap {}
            if let Some(live) = *ctx.live.read() {
                section { class: "session",
                    match live.status {
                        LiveStatus::Armed => rsx! { p { "Waiting for the start line…" } },
                        LiveStatus::InProgress { lap_anchor, sector_anchor, sector } => rsx! {
                            p { "Lap {live.laps + 1}" }
                            LiveClock { anchor: lap_anchor, class: "clock" }
                            // Same color as the sector on the map.
                            div { class: "sector-clock", style: "color: var(--sector-{sector % 4})",
                                span { "Sector {sector + 1}" }
                                LiveClock { anchor: sector_anchor, class: "clock small" }
                            }
                        },
                        LiveStatus::Finished => rsx! { p { "Finished" } },
                    }
                    if let Some(last) = live.last_sector {
                        p { "Last sector {lap_time(last.as_seconds_f64())}" }
                    }
                    if let Some(last) = live.last_lap {
                        p { "Last lap {lap_time(last.as_seconds_f64())}" }
                    }
                }
            }
            if ended {
                p { class: "note", "GPS stream ended. Stop to see the results." }
            }
            button {
                class: "stop",
                onclick: move |_| {
                    ctx.stop();
                    navigator().push(Route::Results {});
                },
                "Stop"
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
