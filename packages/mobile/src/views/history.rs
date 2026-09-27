use std::sync::Arc;

use chrono::{DateTime, Local, Utc};
use dioxus::prelude::*;
use traced_core::track::{Completed, Lap, Session, Track};
use uuid::Uuid;

use super::{
    kmh, lap_time,
    results::{laps, Report},
};
use crate::{storage, Route};

#[component]
pub fn History() -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let recordings = storage::recordings();

    rsx! {
        main {
            h1 { "History" }
            if recordings.is_empty() {
                p { class: "note", "No recordings yet." }
            }
            ul { class: "tracks scroll",
                for r in recordings {
                    li {
                        Link { to: Route::SavedResults { id: r.id },
                            {track_name(&tracks.read(), r.session.as_ref())}
                            small { "{when(r.started_at)} · {outcome(r.session.as_ref())}" }
                            small { "{r.speedometer.distance_m / 1000.0:.2} km · top {kmh(r.speedometer.top_speed_ms)} km/h" }
                            if r.interrupted {
                                small { class: "interrupted", "Interrupted" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn SavedResults(id: Uuid) -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let Some(r) = storage::recordings().into_iter().find(|r| r.id == id) else {
        return rsx! {
            main {
                p { class: "note", "Recording not found." }
            }
        };
    };

    rsx! {
        main {
            h1 { {track_name(&tracks.read(), r.session.as_ref())} }
            small { {when(r.started_at)} }
            if r.interrupted {
                small { class: "interrupted", "Interrupted: the app closed mid-recording, kept up to its last fix." }
            }
            div { class: "scroll",
                Report { speed: r.speedometer, session: r.session }
            }
        }
    }
}

fn track_name(tracks: &[Arc<Track>], session: Option<&Session>) -> String {
    session
        .and_then(|s| tracks.iter().find(|t| t.uuid == s.track_id))
        .map_or("No track".into(), |t| t.name.clone())
}

fn when(at: DateTime<Utc>) -> String {
    at.with_timezone(&Local)
        .format("%a %-d %b, %H:%M")
        .to_string()
}

/// Laps and the best, the run's time, or why there's neither.
fn outcome(session: Option<&Session>) -> String {
    let Some(session) = session else {
        return "Speedometer only".into();
    };
    let Some(best) = laps(Some(session)).iter().map(Lap::duration).min() else {
        return "No completed laps".into();
    };
    let best = lap_time(best.as_seconds_f64());
    match &session.completed {
        Completed::Laps(laps) => format!("{} laps · best {best}", laps.len()),
        Completed::Run(_) => format!("Run {best}"),
    }
}
