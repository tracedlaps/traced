use dioxus::prelude::*;

use recorder::RecorderCtx;
use uuid::Uuid;
use views::{EditTrack, History, Import, Live, NewTrack, Results, SavedResults, Shell, Speedometer, Tracks};

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
    #[layout(Shell)]
        #[route("/history")]
        History {},
        #[route("/history/:id")]
        SavedResults { id: Uuid },
        #[route("/")]
        Tracks {},
        #[route("/speedometer")]
        Speedometer {},
        #[route("/editor")]
        Import {},
        #[route("/editor/new")]
        NewTrack {},
        #[route("/editor/:id")]
        EditTrack { id: Uuid },
        #[route("/results")]
        Results {},
    #[end_layout]
    // Full screen, so nothing is mistapped mid-drive.
    #[route("/live")]
    Live {},
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
