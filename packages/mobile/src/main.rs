use dioxus::prelude::*;

use recorder::RecorderCtx;
use views::{Live, Results, Setup};

mod gps;
mod recorder;
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
}

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Root-owned, so a recording survives moving between screens.
    use_context_provider(RecorderCtx::new);

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        Router::<Route> {}
    }
}
