use std::{collections::BTreeSet, sync::Arc};

use dioxus::prelude::*;
use traced_core::track::{import, Track, TrackBuilder, TrackShape, Waypoint};

use super::{
    backup::Backup,
    track_map::{svg_point, Layout, SvgPoint, TrackSvg},
};
use crate::{storage, Route};

/// Turns another app's export into a track: pick the file, name it, say whether it loops,
/// and tap waypoints to place sector gates. Start/finish sit on the first/last waypoint.
#[component]
pub fn Import() -> Element {
    let mut tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let mut waypoints = use_signal(Vec::<Waypoint>::new);
    let mut name = use_signal(String::new);
    let mut closed = use_signal(|| false);
    // Indices into `waypoints`.
    let mut sectors = use_signal(BTreeSet::<usize>::new);
    let mut error = use_signal(|| None::<String>);

    let draft = move |name: &str| builder(name, &waypoints.read(), closed(), &sectors.read());
    let preview = use_memo(move || {
        let w = waypoints.read();
        if w.len() < 2 {
            return None;
        }
        let built = draft("").build();
        let summary = built
            .as_ref()
            .map(|t| format!("{:.0} m · {} sectors", t.length(), t.sectors().count()))
            .map_err(|e| e.to_string());
        // Still drawn when a sector gate is what fails, just without the sectors.
        let track = built
            .ok()
            .or_else(|| builder("", &w, closed(), &BTreeSet::new()).build().ok())?;
        Some(Preview {
            layout: Layout::new(&track),
            points: w.iter().map(|p| svg_point(&track, *p)).collect(),
            summary,
        })
    });

    let pick = move |_| async move {
        match storage::pick_file().await {
            Ok(None) => {}
            Ok(Some(file)) => match std::fs::read_to_string(&file.path)
                .map_err(|e| e.to_string())
                .and_then(|text| import::waypoints(&text).map_err(|e| e.to_string()))
            {
                Ok(w) => {
                    let stem = file
                        .name
                        .rsplit_once('.')
                        .map_or(&*file.name, |(stem, _)| stem);
                    name.set(stem.replace('_', " "));
                    closed.set(import::looks_closed(&w));
                    sectors.write().clear();
                    waypoints.set(w);
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("{}: {e}", file.name))),
            },
            Err(e) => error.set(Some(e)),
        }
    };
    let mut discard = move || {
        waypoints.write().clear();
        name.write().clear();
        error.set(None);
    };
    let save = move |_| {
        let track = draft(&name.read());
        let Ok(built) = track.clone().build() else {
            return;
        };
        if storage::put_track(&track).is_none() {
            error.set(Some("Couldn't save the track.".into()));
            return;
        }
        tracks.write().push(Arc::new(built));
        discard();
        navigator().push(Route::Tracks {});
    };
    let last = waypoints.read().len().saturating_sub(1);

    rsx! {
        main { class: "form",
            h1 { "Import track" }
            div { class: "scroll",
                button { class: "button secondary", onclick: pick, "Choose a CSV, GPX or KML file" }
                if let Some(e) = error() {
                    p { class: "error", "{e}" }
                }
                if let Some(p) = preview() {
                    label { class: "field",
                        "Name"
                        input { r#type: "text", value: name, oninput: move |e| name.set(e.value()) }
                    }
                    label { class: "check",
                        input { r#type: "checkbox", checked: closed(), onchange: move |e| closed.set(e.checked()) }
                        "Closed loop, timed in laps"
                    }
                    p { class: "note", "Tap waypoints to place sector gates." }
                    TrackSvg { layout: p.layout.clone(), current: None,
                        for (i , (cx , cy)) in p.points.iter().copied().enumerate() {
                            // The first waypoint (and an Open track's last) is already a gate.
                            if i != 0 && (closed() || i != last) {
                                circle {
                                    class: if sectors.read().contains(&i) { "waypoint gate" } else { "waypoint" },
                                    cx,
                                    cy,
                                    r: p.layout.dot_r,
                                    onclick: move |_| {
                                        let mut s = sectors.write();
                                        if !s.remove(&i) {
                                            s.insert(i);
                                        }
                                    },
                                }
                            }
                        }
                    }
                    match &p.summary {
                        Ok(summary) => rsx! { small { "{summary}" } },
                        Err(e) => rsx! { p { class: "error", "{e}" } },
                    }
                }
                Backup {}
            }
            if let Some(p) = preview() {
                button {
                    disabled: p.summary.is_err() || name.read().trim().is_empty(),
                    onclick: save,
                    "Save track"
                }
                button { class: "button secondary", onclick: move |_| discard(), "Discard" }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct Preview {
    layout: Layout,
    /// Each waypoint, in the layout's coordinates.
    points: Vec<SvgPoint>,
    /// Length and sectors, or why the draft doesn't build.
    summary: Result<String, String>,
}

/// Start/finish (or a Closed track's gate) on the end waypoints, a sector gate on each of `sectors`.
fn builder(name: &str, w: &[Waypoint], closed: bool, sectors: &BTreeSet<usize>) -> TrackBuilder {
    let shape = match closed {
        true => TrackShape::Closed { gate: w[0] },
        false => TrackShape::Open {
            start: w[0],
            finish: w[w.len() - 1],
        },
    };
    let track = Track::builder(name.trim(), shape).waypoints(w.to_vec());
    sectors.iter().fold(track, |b, &i| b.sector_gate_at(w[i]))
}
