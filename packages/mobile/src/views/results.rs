use dioxus::prelude::*;
use traced_core::{
    speedometer::SpeedometerSnapshot,
    track::{Completed, Lap, Session},
};

use super::{kmh, lap_time};
use crate::recorder::RecorderCtx;

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
    let laps = laps(session.as_ref());
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
        if !laps.is_empty() {
            table { class: "laps",
                tr { th { "#" } th { "Time" } th { "Splits" } }
                for (i, lap) in laps.iter().enumerate() {
                    tr {
                        td { "{i + 1}" }
                        td {
                            {lap_time(lap.duration().as_seconds_f64())}
                            if lap.gps_gap { span { class: "gap", title: "GPS dropout during this lap", " ⚠" } }
                        }
                        td { {lap.splits().map(|s| lap_time(s.as_seconds_f64())).collect::<Vec<_>>().join(" · ")} }
                    }
                }
            }
        }
    }
}

/// A Closed track's laps, or an Open track's run as a single lap.
pub fn laps(session: Option<&Session>) -> &[Lap] {
    match session.map(|s| &s.completed) {
        Some(Completed::Laps(laps)) => laps,
        Some(Completed::Run(run)) => run.as_slice(),
        None => &[],
    }
}
