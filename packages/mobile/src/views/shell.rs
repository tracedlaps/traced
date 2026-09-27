use dioxus::prelude::*;

use crate::{recorder::RecorderCtx, Route};

// 24×24 stroke icons.
const HISTORY: &str = "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l3 3";
const TRACKS: &str = "M5 21V4M5 4h11l-2 4 2 4H5";
const SPEEDO: &str = "M4.5 17a9 9 0 1 1 15 0M12 13l4-5";
const EDITOR: &str = "M4 20h4L19 9l-4-4L4 16zM13 7l4 4";

/// Every screen but Live: the screen above, the tab bar below, and a way back to a recording in progress.
#[component]
pub fn Shell() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let route = use_route::<Route>();

    rsx! {
        div { class: "shell",
            if ctx.recording.read().is_some() {
                Link { class: "live-bar", to: Route::Live {}, "Recording · back to live" }
            }
            Outlet::<Route> {}
            nav { class: "tabs",
                Tab { to: Route::History {}, icon: HISTORY, label: "History",
                    active: matches!(route, Route::History {} | Route::SavedResults { .. }) }
                Tab { to: Route::Tracks {}, icon: TRACKS, label: "Tracks", active: route == Route::Tracks {} }
                Tab { to: Route::Speedometer {}, icon: SPEEDO, label: "Speed", active: route == Route::Speedometer {} }
                Tab { to: Route::Import {}, icon: EDITOR, label: "Editor",
                    active: matches!(route, Route::Import {} | Route::NewTrack {} | Route::RecordTrack {} | Route::EditTrack { .. }) }
            }
        }
    }
}

#[component]
fn Tab(
    to: Route,
    icon: &'static str,
    label: &'static str,
    active: bool,
) -> Element {
    rsx! {
        Link { class: if active { "tab active" } else { "tab" }, to,
            svg { view_box: "0 0 24 24", path { d: icon } }
            "{label}"
        }
    }
}
