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
    let mut tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    // Last backup action's outcome; also re-renders the list after a restore.
    let mut status = use_signal(|| None::<Result<&str, String>>);
    let mut confirm_restore = use_signal(|| false);
    let recordings = storage::recordings();

    let export = move |_| async move {
        match storage::export_backup().await {
            Ok(true) => status.set(Some(Ok("Backup saved."))),
            Ok(false) => {}
            Err(e) => status.set(Some(Err(e))),
        }
    };
    let restore = move |_| async move {
        confirm_restore.set(false);
        match storage::restore_backup().await {
            Ok(true) => {
                tracks.set(storage::load_tracks());
                status.set(Some(Ok("Backup restored.")));
            }
            Ok(false) => {}
            Err(e) => status.set(Some(Err(e))),
        }
    };

    rsx! {
        main {
            h1 { "History" }
            if recordings.is_empty() {
                p { class: "note", "No recordings yet." }
            }
            ul { class: "tracks",
                for r in recordings {
                    li {
                        Link { to: Route::SavedResults { id: r.id },
                            {track_name(&tracks.read(), r.session.as_ref())}
                            small { "{when(r.started_at)} · {outcome(r.session.as_ref())}" }
                            small { "{r.speedometer.distance_m / 1000.0:.2} km · top {kmh(r.speedometer.top_speed_ms)} km/h" }
                        }
                    }
                }
            }
            Link { class: "button", to: Route::Setup {}, "New recording" }
            section { class: "backup",
                h2 { "Backup" }
                p { class: "note", "Everything lives only on this phone. Export it to keep a copy." }
                button { class: "button secondary", onclick: export, "Export backup" }
                if confirm_restore() {
                    p { class: "error", "Restoring replaces every track and recording here with the backup's." }
                    button { class: "stop", onclick: restore, "Choose backup and replace" }
                    button { class: "button secondary", onclick: move |_| confirm_restore.set(false), "Cancel" }
                } else {
                    button { class: "button secondary", onclick: move |_| confirm_restore.set(true), "Restore backup" }
                }
                match status() {
                    Some(Ok(done)) => rsx! { p { class: "note", "{done}" } },
                    Some(Err(e)) => rsx! { p { class: "error", "{e}" } },
                    None => rsx! {},
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
                p { "Recording not found." }
                Link { class: "button", to: Route::History {}, "History" }
            }
        };
    };

    rsx! {
        main {
            h1 { {track_name(&tracks.read(), r.session.as_ref())} }
            small { {when(r.started_at)} }
            Report { speed: r.speedometer, session: r.session }
            Link { class: "button", to: Route::History {}, "History" }
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
