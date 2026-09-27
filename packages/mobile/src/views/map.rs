use dioxus::{document::Eval, prelude::*};
use serde::{Deserialize, Serialize};
use traced_core::track::Waypoint;

use crate::recorder::RecorderCtx;

/// Leaflet, then the map side of the channel. Leaflet only runs once per app, not per map.
const MAP_JS: &str = concat!(
    "if (!window.L) {\n",
    include_str!("../../assets/leaflet/leaflet.js"),
    "\n}\n",
    include_str!("map.js"),
);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

impl From<Waypoint> for LatLng {
    fn from(w: Waypoint) -> Self {
        Self { lat: w.y(), lng: w.x() }
    }
}

impl From<LatLng> for Waypoint {
    fn from(p: LatLng) -> Self {
        Waypoint::new(p.lng, p.lat)
    }
}

/// What the map draws: the waypoint markers (with their CSS classes) and the track over them.
#[derive(Clone, Default, PartialEq, Serialize)]
pub struct View {
    pub points: Vec<LatLng>,
    pub classes: Vec<String>,
    pub closed: bool,
    /// Handles between waypoints, dragged to insert one.
    pub midpoints: bool,
    pub overlay: Option<Overlay>,
}

/// SVG elements drawn over `bounds` (south-west, north-east).
#[derive(Clone, PartialEq, Serialize)]
pub struct Overlay {
    pub view_box: String,
    pub svg: String,
    pub bounds: [LatLng; 2],
}

/// What the user did to the track on the map.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapEvent {
    TapMap(LatLng),
    TapMarker(usize),
    Drag(usize, LatLng),
    /// A waypoint inserted after the one at the index.
    Insert(usize, LatLng),
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum ToMap {
    View(View),
    Search(String),
    Fly(LatLng),
    Layer(Layer),
    Fit,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FromMap {
    Search(SearchReply),
    Edit(MapEvent),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SearchReply {
    Results(Vec<Place>),
    SearchFailed(String),
}

#[derive(Clone, PartialEq, Deserialize)]
struct Place {
    name: String,
    #[serde(flatten)]
    at: LatLng,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Layer {
    Satellite,
    Streets,
}

/// A Leaflet map of `view`, with place search, a satellite/streets toggle and buttons to fit
/// the track or go to the phone's location. Fits the track on open, or locates when there is none.
#[component]
pub fn Map(view: ReadSignal<View>, on_event: EventHandler<MapEvent>) -> Element {
    let ctx = use_context::<RecorderCtx>();
    let mut map = use_signal(|| None::<Eval>);
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<Place>::new);
    let mut note = use_signal(|| None::<String>);
    let mut layer = use_signal(|| Layer::Satellite);

    let send = move |msg: ToMap| {
        if let Some(map) = map() {
            _ = map.send(msg);
        }
    };
    use_effect(move || send(ToMap::View(view())));
    let locate = move || async move {
        note.set(Some("Finding your location…".into()));
        match ctx.locate().await {
            Some(fix) => {
                note.set(None);
                send(ToMap::Fly(LatLng { lat: fix.latitude, lng: fix.longitude }));
            }
            None => note.set(Some("Couldn't find your location.".into())),
        }
    };

    rsx! {
        document::Stylesheet { href: asset!("/assets/leaflet/leaflet.css") }
        div { class: "map",
            // Never conditional: re-rendering it would lose the map.
            div {
                id: "map",
                // Started once the div exists, which Leaflet needs.
                onmounted: move |_| async move {
                    let mut eval = document::eval(MAP_JS);
                    map.set(Some(eval));
                    if view.peek().points.is_empty() {
                        spawn(locate());
                    }
                    while let Ok(msg) = eval.recv::<FromMap>().await {
                        match msg {
                            FromMap::Edit(e) => on_event(e),
                            FromMap::Search(SearchReply::Results(r)) if r.is_empty() => note.set(Some("No places found.".into())),
                            FromMap::Search(SearchReply::Results(r)) => {
                                note.set(None);
                                results.set(r);
                            }
                            FromMap::Search(SearchReply::SearchFailed(e)) => note.set(Some(format!("Search failed: {e}"))),
                        }
                    }
                },
            }
            form {
                class: "map-search",
                onsubmit: move |e| {
                    e.prevent_default();
                    let q = query.read().trim().to_string();
                    if !q.is_empty() {
                        note.set(None);
                        send(ToMap::Search(q));
                    }
                },
                input {
                    r#type: "search",
                    placeholder: "Search for a place",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
                if let Some(n) = note() {
                    p { class: "note", onclick: move |_| note.set(None), "{n}" }
                }
                ul {
                    for place in results() {
                        li {
                            onclick: move |_| {
                                results.write().clear();
                                send(ToMap::Fly(place.at));
                            },
                            "{place.name}"
                        }
                    }
                }
            }
            div { class: "map-buttons",
                button {
                    onclick: move |_| {
                        let next = match layer() {
                            Layer::Satellite => Layer::Streets,
                            Layer::Streets => Layer::Satellite,
                        };
                        layer.set(next);
                        send(ToMap::Layer(next));
                    },
                    match layer() {
                        Layer::Satellite => "Streets",
                        Layer::Streets => "Satellite",
                    }
                }
                button { onclick: move |_| locate(), "My location" }
                if !view.read().points.is_empty() {
                    button { onclick: move |_| send(ToMap::Fit), "Fit track" }
                }
            }
        }
    }
}
