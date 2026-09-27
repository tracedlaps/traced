use dioxus::prelude::*;

use recorder::RecorderCtx;
use uuid::Uuid;
use views::{History, Import, Live, Results, SavedResults, Setup};

#[cfg(target_os = "android")]
mod android;
mod gps;
mod recorder;
mod storage;
mod tracks;
mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Setup {},
    #[route("/live")]
    Live {},
    #[route("/results")]
    Results {},
    #[route("/import")]
    Import {},
    #[route("/history")]
    History {},
    #[route("/history/:id")]
    SavedResults { id: Uuid },
}

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Root-owned, so a recording survives moving between screens.
    use_context_provider(RecorderCtx::new);
    let tracks = use_signal(storage::load_tracks);
    use_context_provider(|| tracks);

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        Router::<Route> {}
    }
}
