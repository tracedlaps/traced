use dioxus::prelude::*;
use traced_core::gps::GpsError;

use super::gps_status;
use crate::{
    gps::wire::Permission,
    recorder::RecorderCtx,
    tracks::{Preset, TRACKS},
    Route,
};

#[component]
pub fn Setup() -> Element {
    let ctx = use_context::<RecorderCtx>();
    // Index into TRACKS; None records the speedometer only.
    let mut selected = use_signal(|| Some(0));
    // Why the last Start didn't record.
    let mut refused = use_signal(|| None::<String>);
    let (mut use_mock, mut force_lm) = (ctx.use_mock, ctx.force_location_manager);

    if ctx.recording.read().is_some() {
        return rsx! {
            main {
                p { "A recording is in progress." }
                Link { class: "button", to: Route::Live {}, "Back to live" }
            }
        };
    }

    rsx! {
        main {
            h1 { "New recording" }
            ul { class: "tracks",
                li {
                    class: if selected().is_none() { "selected" },
                    onclick: move |_| selected.set(None),
                    "No track"
                    small { "Speedometer only" }
                }
                for (i, Preset { track, .. }) in TRACKS.iter().enumerate() {
                    li {
                        class: if selected() == Some(i) { "selected" },
                        onclick: move |_| selected.set(Some(i)),
                        "{track.name}"
                        small { "{track.length():.0} m · {track.sectors().count()} sectors" }
                    }
                }
            }
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
                onclick: move |_| async move {
                    match ctx.start(selected().map(|i| &TRACKS[i])).await {
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
}
