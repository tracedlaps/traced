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

use super::{
    lap_time,
    map::{Map, View},
    results::Report,
    track_map::{Layout, TrackSvg},
};
use crate::{i18n::local_date, recorder::RecorderCtx, storage, units::Units, Route};

#[component]
pub fn History() -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let recordings = storage::recordings();
    let units = Units::current();

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
                            small { "{units.distance(r.speedometer.distance_m)} · " {t!("stat-top")} " {units.speed(r.speedometer.top_speed_ms)}" }
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
    let ctx = use_context::<RecorderCtx>();
    let mut trim = use_signal(|| true);
    let Some(r) = storage::recordings().into_iter().find(|r| r.id == id) else {
        return rsx! {
            main {
                p { class: "note", {t!("recording-not-found")} }
            }
        };
    };
    let timed = r.session.as_ref().and_then(Session::timed);
    let track = r.session.as_ref().and_then(|s| tracks.read().iter().find(|t| t.uuid == s.track_id).cloned());

    rsx! {
        main {
            h1 { {track_name(&tracks.read(), r.session.as_ref())} }
            small { {when(r.started_at)} }
            if r.interrupted {
                small { class: "interrupted", {t!("interrupted-detail")} }
            }
            button {
                // The shell's bar leads back to a recording (or replay) in progress.
                disabled: ctx.recording.read().is_some(),
                onclick: move |_| {
                    if ctx.replay(id, track.clone(), timed.filter(|_| trim())) {
                        navigator().push(Route::Live {});
                    }
                },
                {t!("replay")}
            }
            if timed.is_some() {
                label { class: "check",
                    input { r#type: "checkbox", checked: trim(), onchange: move |e| trim.set(e.checked()) }
                    {t!("replay-trim")}
                }
            }
            div { class: "scroll",
                // Its track, or without one, where it went.
                match &track {
                    Some(t) => rsx! { TrackSvg { layout: Layout::new(t), current: None } },
                    None if r.session.is_none() => rsx! { RecordingMap { id } },
                    None => rsx! {},
                }
                Report { speed: r.speedometer, session: r.session, g: r.g }
            }
        }
    }
}

/// Where recording `id` went, on the map.
#[component]
fn RecordingMap(id: Uuid) -> Element {
    let view = use_signal(|| View {
        trace: storage::fixes(id).into_iter().map(Into::into).collect(),
        ..View::default()
    });
    if view.read().trace.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "recording-map",
            Map { view, on_event: |_| {}, locate: false, search: false }
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
