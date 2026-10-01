use dioxus::prelude::*;
use dioxus_i18n::t;

use super::guide::BackgroundGuide;
use crate::{recorder::RecorderCtx, storage::PickedFile, update::Latest, Route};

// 24×24 stroke icons.
pub(crate) const HISTORY: &str = "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l3 3";
pub(crate) const TRACKS: &str = "M5 21V4M5 4h11l-2 4 2 4H5";
pub(crate) const SPEEDO: &str = "M4.5 17a9 9 0 1 1 15 0M12 13l4-5";
pub(crate) const EDITOR: &str = "M4 20h4L19 9l-4-4L4 16zM13 7l4 4";
pub(crate) const SETTINGS: &str = "M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3M14 2v4M8 10v4M16 18v4";

/// Every screen but Live: the screen above, the tab bar below, and a way back to a recording in progress.
#[component]
pub fn Shell() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let route = use_route::<Route>();
    let incoming = use_context::<Signal<Option<PickedFile>>>();
    // A newer release, offered until followed to Settings, where it's installed.
    let latest = use_context::<Latest>();
    let mut seen = use_signal(|| false);
    let update = latest().and_then(Result::ok).and_then(|r| r.into_iter().next()).filter(|_| !seen() && route != Route::Settings {});
    use_effect(move || {
        if incoming.read().is_some() {
            navigator().push(Route::Import {});
        }
    });

    rsx! {
        div { class: "shell",
            if ctx.recording.read().is_some() {
                Link { class: "live-bar", to: Route::Live {}, {if ctx.replaying.read().is_some() { t!("back-to-replay") } else { t!("back-to-live") }} }
            } else if let Some(release) = update {
                Link { class: "update-bar", to: Route::Settings {}, onclick: move |_| seen.set(true), {t!("update-available", version: release.version)} }
            }
            if let Some(guide) = ctx.guide.read().clone() {
                BackgroundGuide { guide }
            } else {
                Outlet::<Route> {}
            }
            nav { class: "tabs",
                Tab { to: Route::History {}, icon: HISTORY, label: t!("tab-history"),
                    active: matches!(route, Route::History {} | Route::SavedResults { .. }) }
                Tab { to: Route::Tracks {}, icon: TRACKS, label: t!("tab-tracks"), active: matches!(route, Route::Tracks {} | Route::ShareTrack { .. }) }
                Tab { to: Route::Speedometer {}, icon: SPEEDO, label: t!("tab-speed"), active: route == Route::Speedometer {} }
                Tab { to: Route::Import {}, icon: EDITOR, label: t!("tab-editor"),
                    active: matches!(route, Route::Import {} | Route::NewTrack {} | Route::RecordTrack {} | Route::EditTrack { .. }) }
                Tab { to: Route::Settings {}, icon: SETTINGS, label: t!("tab-settings"), active: route == Route::Settings {} }
            }
        }
    }
}

#[component]
fn Tab(
    to: Route,
    icon: &'static str,
    label: String,
    active: bool,
) -> Element {
    rsx! {
        Link { class: if active { "tab active" } else { "tab" }, to,
            svg { view_box: "0 0 24 24", path { d: icon } }
            "{label}"
        }
    }
}
