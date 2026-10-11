//! First run: what Traced does, that it is for tracks, then each permission it needs, explained before Android asks.

use dioxus::prelude::*;
use dioxus_i18n::t;

use traced_core::imu::{GForceSnapshot, GPeaks};

use super::{
    icons::{self, Icon},
    live::GMeterDial,
    track_map::{Layout, TrackSvg},
};
use crate::{gps::wire::Permission, recorder::RecorderCtx, storage, tracks::PRESETS, units::Units};

/// Whether the welcome is done; Settings can bring it back.
#[derive(Clone, Copy)]
pub struct Onboarded(pub Signal<bool>);

impl Onboarded {
    pub fn load() -> Self {
        Self(Signal::new(storage::setting("onboarded").as_deref() == Some("1")))
    }
}

fn set_onboarded(done: bool) {
    storage::put_setting("onboarded", if done { "1" } else { "0" });
    consume_context::<Onboarded>().0.set(done);
}

/// Shows the welcome again.
pub fn replay() {
    set_onboarded(false);
}

fn finish() {
    set_onboarded(true);
    // Strict phones next learn to let Traced run in the background.
    consume_context::<RecorderCtx>().guide_on_start();
}

/// Each page's title and text; its picture is drawn by `Art`.
const SLIDES: [(&str, &str); 3] =
    [("welcome-speed", "welcome-speed-body"), ("welcome-laps", "welcome-laps-body"), ("welcome-editor", "welcome-editor-body")];

/// The safety notice's reasons (icon and text), also in Settings.
pub const SAFETY: &[(&str, &str)] = &[
    (icons::FLAG, "safety-track"),
    (icons::NO_ENTRY, "safety-roads"),
    (icons::SHIELD, "safety-responsibility"),
];

/// A page after the slides: the safety notice, then each permission explained before Android asks.
#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Safety,
    Location,
    Notifications,
}

impl Ask {
    /// Its badge icon, title and reasons (icon and text).
    fn page(self) -> (&'static str, &'static str, &'static [(&'static str, &'static str)]) {
        match self {
            Self::Safety => (icons::WARNING, "safety-title", SAFETY),
            Self::Location => (
                icons::PIN,
                "location-title",
                &[
                    (icons::FLAG, "location-laps"),
                    (icons::CLOCK, "location-recording"),
                    (icons::PHONE, "location-screen-off"),
                ],
            ),
            Self::Notifications => (
                icons::BELL,
                "notifications-title",
                &[
                    (icons::PHONE, "notifications-recording"),
                    (icons::BACK, "notifications-back"),
                ],
            ),
        }
    }

    async fn request(self, ctx: RecorderCtx) {
        match self {
            Self::Safety => {}
            Self::Location => ctx.request_location().await,
            Self::Notifications => ctx.request_notifications().await,
        }
    }
}

/// The welcome pages, the safety notice, then each permission Traced still needs, explained.
#[component]
pub fn Onboarding() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let mut page = use_signal(|| 0usize);
    let asks = use_hook(|| {
        let location = (*ctx.permission.peek() == Some(Permission::Denied)).then_some(Ask::Location);
        [Some(Ask::Safety), location, ctx.needs_notifications().then_some(Ask::Notifications)].into_iter().flatten().collect::<Vec<_>>()
    });
    let total = SLIDES.len() + asks.len();
    // Past the last page. Not finished from a handler: an async one's task belongs to this
    // component, which finishing unmounts while the task still runs, and that froze the app.
    use_effect(move || {
        if page() >= total {
            finish();
        }
    });

    if let Some(&ask) = page().checked_sub(SLIDES.len()).and_then(|i| asks.get(i)) {
        let (badge, title, reasons) = ask.page();
        return rsx! {
            main { class: "welcome",
                div { class: "slide",
                    div { class: "badge", Icon { d: badge } }
                    h1 { {t!(title)} }
                    ul { class: "reasons",
                        for &(icon, text) in reasons {
                            li { Icon { d: icon } {t!(text)} }
                        }
                    }
                }
                button { onclick: move |_| async move { ask.request(ctx).await; page += 1 },
                    {if ask == Ask::Safety { t!("understand") } else { t!("continue") }}
                }
                // The notice can't be skipped, only acknowledged.
                if ask != Ask::Safety {
                    button { class: "link", onclick: move |_| page += 1, {t!("guide-skip")} }
                }
            }
        };
    }
    let Some(&(title, body)) = SLIDES.get(page()) else { return rsx! {} };
    rsx! {
        main { class: "welcome",
            // Skips the pages, not the permissions.
            button { class: "link skip", onclick: move |_| page.set(SLIDES.len()), {t!("skip")} }
            div { class: "slide",
                div { class: "art", Art { page: page() } }
                h1 { {t!(title)} }
                p { {t!(body)} }
            }
            div { class: "dots",
                for i in 0..SLIDES.len() {
                    span { class: if i == page() { "on" } }
                }
            }
            button { onclick: move |_| page += 1,
                {if page() + 1 == SLIDES.len() { t!("get-started") } else { t!("next") }}
            }
        }
    }
}

/// Each page's picture, from the app's own pieces: the speed and G-dial, then the bundled tracks.
#[component]
fn Art(page: usize) -> Element {
    let layouts = use_hook(|| PRESETS.iter().map(|p| Layout::new(&p.track.clone().build().expect("bundled track"))).collect::<Vec<_>>());
    match page {
        0 => rsx! {
            div { class: "speed-row",
                div { class: "speed", span { "87" } small { {Units::current().speed_unit()} } }
                GMeterDial { g: GForceSnapshot { lateral_g: 0.5, longitudinal_g: -0.3, aligned: true, peaks: GPeaks { braking: 0.8, accelerating: 0.4, left: 0.6, right: 0.7 } } }
            }
        },
        1 => rsx! {
            TrackSvg { layout: layouts[0].clone(), current: Some(1) }
            div { class: "lap", span { "1:12.8" } span { class: "delta ahead", "−0.42" } }
        },
        _ => rsx! { TrackSvg { layout: layouts[1].clone(), current: None } },
    }
}

/// What goes here, while nothing does yet.
#[component]
pub fn Empty(icon: &'static str, title: String, body: String, children: Element) -> Element {
    rsx! {
        div { class: "empty scroll",
            div { class: "empty-icon", Icon { d: icon } }
            strong { "{title}" }
            p { "{body}" }
            {children}
        }
    }
}
