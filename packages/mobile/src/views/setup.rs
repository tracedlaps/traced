use dioxus::prelude::*;

use crate::{
    recorder::RecorderCtx,
    tracks::{Preset, TRACKS},
    Route,
};

#[component]
pub fn Setup() -> Element {
    let ctx = use_context::<RecorderCtx>();
    // Index into TRACKS; None records the speedometer only.
    let mut selected = use_signal(|| Some(0));

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
            button {
                onclick: move |_| {
                    ctx.start(selected().map(|i| &TRACKS[i]));
                    navigator().push(Route::Live {});
                },
                "Start"
            }
        }
    }
}
