use dioxus::{document::Eval, prelude::*};
use dioxus_i18n::t;
use serde::{Deserialize, Serialize};
use traced_core::{gps::GPSFix, track::Waypoint};

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

impl From<GPSFix> for LatLng {
    fn from(f: GPSFix) -> Self {
        Self { lat: f.latitude, lng: f.longitude }
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
    /// A path being traced, drawn as a line.
    pub trace: Vec<LatLng>,
    /// When replaying `trace`: how many of its points are behind; the rest is drawn faint.
    pub passed: Option<usize>,
    /// Where the phone is; the map follows it until panned.
    pub me: Option<LatLng>,
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

/// The street (else the place) at `p`, from Nominatim, through the WebView like search.
pub async fn street_at(p: LatLng) -> Option<String> {
    let js = format!(
        "try {{ const r = await (await fetch('https://nominatim.openstreetmap.org/reverse?format=jsonv2&zoom=17&lat={}&lon={}')).json(); \
         return r.address?.road ?? r.name ?? null; }} catch {{ return null; }}",
        p.lat, p.lng
    );
    document::eval(&js).join::<Option<String>>().await.ok().flatten().filter(|s| !s.is_empty())
}

/// A Leaflet map of `view`, with place search, a satellite/streets toggle and buttons to fit
/// the track or go to the phone's location. Fits the track on open, or, unless `!locate`, locates when there is none.
#[component]
pub fn Map(
    view: ReadSignal<View>,
    on_event: EventHandler<MapEvent>,
    #[props(default = true)] locate: bool,
    #[props(default = true)] search: bool,
) -> Element {
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
    let find_me = move || async move {
        note.set(Some(t!("finding-location")));
        match ctx.locate().await {
            Some(fix) => {
                note.set(None);
                send(ToMap::Fly(LatLng { lat: fix.latitude, lng: fix.longitude }));
            }
            None => note.set(Some(t!("location-not-found"))),
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
                    if locate && view.peek().points.is_empty() {
                        spawn(find_me());
                    }
                    while let Ok(msg) = eval.recv::<FromMap>().await {
                        match msg {
                            FromMap::Edit(e) => on_event(e),
                            FromMap::Search(SearchReply::Results(r)) if r.is_empty() => note.set(Some(t!("no-places"))),
                            FromMap::Search(SearchReply::Results(r)) => {
                                note.set(None);
                                results.set(r);
                            }
                            FromMap::Search(SearchReply::SearchFailed(e)) => note.set(Some(t!("search-failed", error: e))),
                        }
                    }
                },
            }
            form {
                class: "map-search",
                hidden: !search,
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
                    placeholder: t!("search-place"),
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
                        Layer::Satellite => t!("layer-streets"),
                        Layer::Streets => t!("layer-satellite"),
                    }
                }
                button { onclick: move |_| find_me(), {t!("my-location")} }
                if !view.read().points.is_empty() || !view.read().trace.is_empty() {
                    button { onclick: move |_| send(ToMap::Fit), {t!("fit-track")} }
                }
            }
        }
    }
}
