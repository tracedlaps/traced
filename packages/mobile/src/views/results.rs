use dioxus::prelude::*;
use traced_core::{speedometer::SpeedometerSnapshot, timing::best_lap, track::Session};

use super::{kmh, lap_time, Delta};
use crate::{recorder::RecorderCtx, storage};

#[component]
pub fn Results() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let summary = ctx.last_summary.read();
    let Some(summary) = summary.as_ref() else {
        return rsx! {
            main {
                p { if ctx.recording.read().is_some() { "Recording in progress." } else { "Finishing…" } }
            }
        };
    };

    rsx! {
        main {
            h1 { "Results" }
            div { class: "scroll",
                Report { speed: summary.speedometer, session: summary.session.clone() }
            }
        }
    }
}

/// A finished recording's stats and laps, live or saved.
#[component]
pub fn Report(speed: SpeedometerSnapshot, session: Option<Session>) -> Element {
    let laps = session.as_ref().map_or(&[][..], Session::laps);
    let recordings = storage::recordings();
    let best = session.as_ref().and_then(|s| best_lap(&recordings, s.track_id)).map(|(_, lap)| lap);
    rsx! {
        dl { class: "stats",
            dt { "Top" } dd { "{kmh(speed.top_speed_ms)} km/h" }
            dt { "Avg" } dd { "{kmh(speed.average_speed_ms)} km/h" }
            dt { "Distance" } dd { "{speed.distance_m / 1000.0:.2} km" }
            dt { "Time" } dd { {lap_time(speed.elapsed.as_secs_f64())} }
        }
        if session.is_some() && laps.is_empty() {
            p { "No completed laps." }
        }
        if best.is_some_and(|b| laps.contains(b)) {
            p { class: "pb", "Personal best on this track" }
        }
        if !laps.is_empty() {
            table { class: "laps",
                tr { th { "#" } th { "Time" } th { "PB" } th { "Splits" } }
                for (i, lap) in laps.iter().enumerate() {
                    tr {
                        td { "{i + 1}" }
                        td {
                            {lap_time(lap.duration().as_seconds_f64())}
                            if lap.gps_gap { span { class: "gap", title: "GPS dropout during this lap", " ⚠" } }
                        }
                        td {
                            match best {
                                Some(b) if b == lap => rsx! { span { class: "pb", "PB" } },
                                Some(b) => rsx! { Delta { delta: lap.delta(b) } },
                                None => rsx! {},
                            }
                        }
                        td { {lap.splits().map(|s| lap_time(s.as_seconds_f64())).collect::<Vec<_>>().join(" · ")} }
                    }
                }
            }
        }
    }
}
