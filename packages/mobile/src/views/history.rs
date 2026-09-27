use std::sync::Arc;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::{
    store::SavedRecording,
    timing::best_lap,
    track::{Completed, Lap, Session, Track},
};
use uuid::Uuid;

use super::{kmh, lap_time, results::Report};
use crate::{i18n::local_date, storage, Route};

#[component]
pub fn History() -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let recordings = storage::recordings();

    rsx! {
        main {
            h1 { {t!("history")} }
            if recordings.is_empty() {
                p { class: "note", {t!("no-recordings")} }
            }
            ul { class: "tracks scroll",
                for r in recordings.iter() {
                    li {
                        Link { to: Route::SavedResults { id: r.id },
                            {track_name(&tracks.read(), r.session.as_ref())}
                            small { "{when(r.started_at)} · {outcome(&recordings, r.session.as_ref())}" }
                            small { "{r.speedometer.distance_m / 1000.0:.2} km · " {t!("stat-top")} " {kmh(r.speedometer.top_speed_ms)} km/h" }
                            if r.interrupted {
                                small { class: "interrupted", {t!("interrupted")} }
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
                p { class: "note", {t!("recording-not-found")} }
            }
        };
    };

    rsx! {
        main {
            h1 { {track_name(&tracks.read(), r.session.as_ref())} }
            small { {when(r.started_at)} }
            if r.interrupted {
                small { class: "interrupted", {t!("interrupted-detail")} }
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
        .map_or_else(|| t!("no-track"), |t| t.name.clone())
}

fn when(at: DateTime<Utc>) -> String {
    local_date(at, "%a %-d %b, %H:%M")
}

/// Laps and the best, the run's time, or why there's neither; PB if it holds the track's.
fn outcome(recordings: &[SavedRecording], session: Option<&Session>) -> String {
    let Some(session) = session else {
        return t!("speedometer-only");
    };
    let Some(best) = session.laps().iter().map(Lap::duration).min() else {
        return t!("no-completed-laps");
    };
    let pb = best_lap(recordings, session.track_id).is_some_and(|(_, pb)| session.laps().contains(pb));
    let best = lap_time(best.as_seconds_f64());
    let pb = if pb { format!(" · {}", t!("pb")) } else { String::new() };
    match &session.completed {
        Completed::Laps(laps) => t!("outcome-laps", count: laps.len(), best: best) + &pb,
        Completed::Run(_) => t!("outcome-run", best: best) + &pb,
    }
}
