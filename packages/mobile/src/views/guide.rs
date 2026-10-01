use dioxus::prelude::*;
use dioxus_i18n::t;

use crate::recorder::{Guide, RecorderCtx};

/// How to let Traced run in the background, in the phone's Settings: no app can switch it on itself.
#[component]
pub fn BackgroundGuide(guide: Guide) -> Element {
    let ctx = use_context::<RecorderCtx>();
    let killed = guide == Guide::Killed;
    // Done tapped while Traced still isn't allowed.
    let mut not_yet = use_signal(|| false);

    rsx! {
        main {
            h1 { {if killed { t!("guide-killed-title") } else { t!("guide-title") }} }
            p { {if killed { t!("guide-killed") } else { t!("guide-why") }} }
            ol { class: "notes",
                li { {t!("guide-step-settings")} }
                li { {t!("guide-step-battery")} }
                li { {t!("guide-step-allow")} }
            }
            button { onclick: move |_| ctx.open_settings(), {t!("open-settings")} }
            if not_yet() {
                p { class: "error", {t!("guide-not-yet")} }
            }
            button { class: "button secondary", onclick: move |_| not_yet.set(!ctx.guide_done()), {t!("guide-done")} }
            if !killed {
                button { class: "link", onclick: move |_| ctx.skip_guide(), {t!("guide-skip")} }
            }
        }
    }
}
