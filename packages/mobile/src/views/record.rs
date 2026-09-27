use std::sync::Arc;

use dioxus::prelude::*;
use traced_core::{gps::GpsError, timing::best_lap, track::Track};

use super::{gps_status, lap_time, timed_m};
use crate::{gps::wire::Permission, recorder::RecorderCtx, storage, Route};

#[component]
pub fn Tracks() -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let mut selected = use_signal(|| 0);
    let recordings = storage::recordings();

    if tracks.read().is_empty() {
        return rsx! {
            main {
                h1 { "Tracks" }
                p { class: "note", "No tracks yet." }
                Link { class: "button", to: Route::Import {}, "Import a track" }
            }
        };
    }

    rsx! {
        main {
            h1 { "Tracks" }
            ul { class: "tracks scroll",
                for (i, track) in tracks.read().iter().enumerate() {
                    li {
                        class: if selected() == i { "selected" },
                        onclick: move |_| selected.set(i),
                        "{track.name}"
                        small {
                            "{timed_m(track):.0} m · {track.sectors().count()} sectors"
                            if let Some((_, pb)) = best_lap(&recordings, track.uuid) {
                                " · PB {lap_time(pb.duration().as_seconds_f64())}"
                            }
                        }
                    }
                }
            }
            if let Some(t) = tracks.read().get(selected()) {
                Link { class: "button secondary", to: Route::EditTrack { id: t.uuid }, "Edit track" }
            }
            StartPanel { track: Some(selected()) }
        }
    }
}

#[component]
pub fn Speedometer() -> Element {
    rsx! {
        main {
            h1 { "Speedometer" }
            div { class: "speed idle scroll",
                span { "0" }
                small { "km/h · top, average and distance, no track" }
            }
            StartPanel { track: None }
        }
    }
}

/// GPS state, permission help and Start for `track` (an index into the tracks; None records the speedometer only).
#[component]
fn StartPanel(track: Option<usize>) -> Element {
    let ctx = use_context::<RecorderCtx>();
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    // Why the last Start didn't record.
    let mut refused = use_signal(|| None::<String>);
    let (mut use_mock, mut force_lm) = (ctx.use_mock, ctx.force_location_manager);

    rsx! {
        section { class: "gps",
            p { {gps_status(*ctx.status.read())} }
            match *ctx.permission.read() {
                Some(Permission::Denied) => rsx! { small { "Location permission is asked for on Start" } },
                Some(Permission::Granted) => rsx! { small { "Location permission granted" } },
                Some(Permission::Blocked) => rsx! {
                    small { "Location permission is blocked. Allow it in Settings, then Start again." }
                    button { onclick: move |_| ctx.open_settings(), "Open settings" }
                },
                None => rsx! {},
            }
            if let Some(why) = refused() {
                p { class: "error", "{why}" }
            }
            if cfg!(target_os = "android") {
                details {
                    summary { "Developer" }
                    label {
                        input { r#type: "checkbox", checked: use_mock(), onchange: move |e| use_mock.set(e.checked()) }
                        "Mock drive instead of the GPS"
                    }
                    label {
                        input { r#type: "checkbox", checked: force_lm(), onchange: move |e| force_lm.set(e.checked()) }
                        "LocationManager instead of Fused"
                    }
                }
            }
        }
        button {
            // The shell's bar leads back to a recording in progress.
            disabled: ctx.recording.read().is_some(),
            onclick: move |_| async move {
                match ctx.start(track.and_then(|i| tracks.read().get(i).cloned())).await {
                    Ok(()) => {
                        refused.set(None);
                        navigator().push(Route::Live {});
                    }
                    Err(e) => refused.set(Some(match e {
                        GpsError::PermissionDenied => "Location permission denied. It's needed to record.".into(),
                        GpsError::LocationDisabled => "Location is turned off. Turn it on to record.".into(),
                        e => e.to_string(),
                    })),
                }
            },
            "Start"
        }
    }
}
