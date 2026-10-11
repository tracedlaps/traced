use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::{timing::best_lap, track::Track};

use super::{
    gps_status, icons, lap_time,
    onboarding::Empty,
    refusal, timed_m,
    track_map::{Layout, TrackSvg},
};
use crate::{gps::wire::Permission, recorder::RecorderCtx, storage, units::Units, Route};

#[component]
pub fn Tracks() -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let mut selected = use_signal(|| 0);
    let recordings = storage::recordings();
    let thumbs = use_memo(move || {
        tracks
            .read()
            .iter()
            .map(|t| Layout::new(t))
            .collect::<Vec<_>>()
    });

    if tracks.read().is_empty() {
        return rsx! {
            main {
                h1 { {t!("tracks")} }
                Empty { icon: icons::FLAG, title: t!("no-tracks"), body: t!("no-tracks-body"),
                    Link { class: "button", to: Route::Import {}, {t!("add-track")} }
                }
            }
        };
    }

    rsx! {
        main {
            h1 { {t!("tracks")} }
            ul { class: "tracks scroll",
                for (i, track) in tracks.read().iter().enumerate() {
                    li {
                        class: if selected() == i { "track-card selected" } else { "track-card" },
                        onclick: move |_| selected.set(i),
                        div {
                            "{track.name}"
                            small {
                                "{Units::current().length(timed_m(track))} · " {t!("sector-count", count: track.sectors().count())}
                                if let Some((_, pb)) = best_lap(&recordings, track.uuid) {
                                    " · " {t!("pb")} " {lap_time(pb.duration().as_seconds_f64())}"
                                }
                            }
                            if !track.tags.is_empty() {
                                div { class: "tags",
                                    for tag in &track.tags {
                                        span { "{tag}" }
                                    }
                                }
                            }
                        }
                        if let Some(layout) = thumbs.read().get(i) {
                            TrackSvg { layout: layout.clone(), current: None }
                        }
                    }
                }
            }
            if let Some(t) = tracks.read().get(selected()) {
                div { class: "row",
                    Link { class: "button secondary", to: Route::EditTrack { id: t.uuid }, {t!("edit-track")} }
                    Link { class: "button secondary", to: Route::ShareTrack { id: t.uuid }, {t!("share")} }
                }
            }
            StartPanel { track: Some(selected()) }
        }
    }
}

#[component]
pub fn Speedometer() -> Element {
    rsx! {
        main {
            h1 { {t!("speedometer")} }
            div { class: "speed idle scroll",
                span { "0" }
                small { {t!("speedometer-idle", unit: Units::current().speed_unit())} }
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

    rsx! {
        button {
            // The shell's bar leads back to a recording in progress.
            disabled: ctx.recording.read().is_some() || ctx.capture.read().is_some(),
            onclick: move |_| async move {
                if ctx.guide_on_start() {
                    return;
                }
                match ctx.start(track.and_then(|i| tracks.read().get(i).cloned())).await {
                    Ok(()) => {
                        refused.set(None);
                        navigator().push(Route::Live {});
                    }
                    Err(e) => refused.set(Some(refusal(e))),
                }
            },
            {t!("start")}
        }
    }
}
