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
/// Android's back button and gesture (see MainActivity) go back a screen, leaving Traced only from the first.
pub(crate) fn use_back() {
    use_hook(|| {
        spawn(async {
            let mut back = document::eval("window.tracedBack = () => window.tracedCanGoBack === true && (dioxus.send(0), true); await new Promise(() => {});");
            while back.recv::<u8>().await.is_ok() {
                navigator().go_back();
            }
        })
    });
    let route = use_route::<Route>();
    use_effect(use_reactive!(|route| {
        let _ = route;
        document::eval(&format!("window.tracedCanGoBack = {}", navigator().can_go_back()));
    }));
}

/// The tabs, in the bar's order.
const TABS: [Route; 5] = [Route::History {}, Route::Tracks {}, Route::Speedometer {}, Route::Import {}, Route::Settings {}];

/// A sideways drag on a tab's own screen (`window.tracedTab`, its index) moves the screen with the finger;
/// let go far enough and it slides out, the next tab (-1 or 1) slides in. Past the first and last tab it
/// only gives a little. Not from maps, the speed chart or fields, where dragging sideways means something else.
const SWIPES: &str = r#"
    window.tracedSwipes?.abort();
    window.tracedSwipes = new AbortController();
    const options = { passive: true, signal: window.tracedSwipes.signal };
    const screen = () => document.querySelector(".shell > main");
    const move = (el, x, ms) => {
        el.style.transition = ms ? `transform ${ms}ms ease-out, opacity ${ms}ms ease-out` : "none";
        el.style.transform = x ? `translateX(${x}px)` : "";
        el.style.opacity = x ? 1 - Math.min(Math.abs(x) / innerWidth, 1) / 2 : "";
    };
    let x, y, dx, step, el, dragging, skip;
    document.addEventListener("touchstart", e => {
        ({ clientX: x, clientY: y } = e.touches[0]);
        el = screen();
        dragging = false;
        skip = !el || window.tracedTab < 0 || e.touches.length > 1 || !!e.target.closest(".leaflet-container, .speed-trace, input, textarea, select");
    }, options);
    document.addEventListener("touchmove", e => {
        if (skip) return;
        const t = e.touches[0];
        dx = t.clientX - x;
        if (!dragging) {
            if (Math.abs(t.clientY - y) > 12) skip = true; // scrolling
            dragging = !skip && Math.abs(dx) > 12;
            if (!dragging) return;
        }
        step = dx < 0 ? 1 : -1;
        const edge = window.tracedTab + step < 0 || window.tracedTab + step > 4;
        move(el, edge ? dx / 5 : dx, 0);
        if (edge) step = 0;
    }, options);
    document.addEventListener("touchend", () => {
        if (!dragging) return;
        if (step && Math.abs(dx) > Math.min(80, innerWidth / 4)) {
            move(el, -step * innerWidth, 150);
            // Rust calls it once the next tab is showing: it comes in from the other side.
            window.tracedEnter = () => {
                window.tracedEnter = null;
                const next = screen();
                move(next, step * innerWidth / 3, 0);
                next.getBoundingClientRect();
                move(next, 0, 200);
            };
            setTimeout(() => dioxus.send(step), 150);
            setTimeout(() => window.tracedEnter?.(), 800); // never left off-screen
        } else {
            move(el, 0, 200);
        }
    }, options);
    await new Promise(() => {});
"#;

fn use_swipes() {
    use_hook(|| {
        spawn(async {
            let mut swipes = document::eval(SWIPES);
            while let Ok(step) = swipes.recv::<isize>().await {
                let current = router().current::<Route>();
                if let Some(next) = TABS.iter().position(|t| *t == current).and_then(|i| TABS.get(i.wrapping_add_signed(step))) {
                    navigator().push(next.clone());
                }
            }
        })
    });
    let route = use_route::<Route>();
    use_effect(use_reactive!(|route| {
        let tab = TABS.iter().position(|t| *t == route).map_or(-1, |i| i as isize);
        document::eval(&format!("window.tracedTab = {tab}; window.tracedEnter?.()"));
    }));
    // Live has no tabs.
    use_drop(|| _ = document::eval("window.tracedSwipes?.abort(); window.tracedTab = -1"));
}

#[component]
pub fn Shell() -> Element {
    use_back();
    use_swipes();
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
