use dioxus::prelude::*;
use dioxus_i18n::t;
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
                p { if ctx.recording.read().is_some() { {t!("recording-in-progress")} } else { {t!("finishing")} } }
            }
        };
    };

    rsx! {
        main {
            h1 { {t!("results")} }
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
            dt { {t!("stat-top")} } dd { "{kmh(speed.top_speed_ms)} km/h" }
            dt { {t!("stat-avg")} } dd { "{kmh(speed.average_speed_ms)} km/h" }
            dt { {t!("stat-distance")} } dd { "{speed.distance_m / 1000.0:.2} km" }
            dt { {t!("stat-time")} } dd { {lap_time(speed.elapsed.as_secs_f64())} }
        }
        if session.is_some() && laps.is_empty() {
            p { {t!("no-completed-laps")} }
        }
        if best.is_some_and(|b| laps.contains(b)) {
            p { class: "pb", {t!("personal-best")} }
        }
        if !laps.is_empty() {
            table { class: "laps",
                tr { th { "#" } th { {t!("stat-time")} } th { {t!("pb")} } th { {t!("splits")} } }
                for (i, lap) in laps.iter().enumerate() {
                    tr {
                        td { "{i + 1}" }
                        td {
                            {lap_time(lap.duration().as_seconds_f64())}
                            if lap.gps_gap { span { class: "gap", title: t!("gps-gap"), " ⚠" } }
                        }
                        td {
                            match best {
                                Some(b) if b == lap => rsx! { span { class: "pb", {t!("pb")} } },
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
