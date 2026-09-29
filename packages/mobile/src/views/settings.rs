use dioxus::prelude::*;
use dioxus_i18n::t;

use super::{backup::Backup, update::Updates};
use crate::{
    prefs::{Choice, Language, MapView, Theme},
    recorder::RecorderCtx,
    units::Units,
};

#[component]
pub fn Settings() -> Element {
    rsx! {
        main { class: "form",
            h1 { {t!("settings")} }
            div { class: "scroll",
                {choices::<Units>()}
                {choices::<Language>()}
                {choices::<Theme>()}
                {choices::<MapView>()}
                Backup {}
                Updates {}
                // Debug builds only: none of this is for drivers.
                if cfg!(all(debug_assertions, target_os = "android")) {
                    Developer {}
                }
            }
        }
    }
}

/// A preference's heading and a button per option.
fn choices<T: Choice>() -> Element {
    let mut chosen = consume_context::<Signal<T>>();
    rsx! {
        section { class: "backup",
            h2 { {t!(T::KEY)} }
            div { class: "modes",
                for &(option , value) in T::OPTIONS {
                    button {
                        class: if chosen() == option { "button" } else { "button secondary" },
                        onclick: move |_| {
                            option.save();
                            chosen.set(option);
                        },
                        {t!(&format!("{}-{value}", T::KEY))}
                    }
                }
            }
        }
    }
}

#[component]
fn Developer() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let (mut use_mock, mut force_lm) = (ctx.use_mock, ctx.force_location_manager);
    rsx! {
        section { class: "backup",
            h2 { {t!("developer")} }
            label { class: "check",
                input { r#type: "checkbox", checked: use_mock(), onchange: move |e| use_mock.set(e.checked()) }
                {t!("mock-drive")}
            }
            label { class: "check",
                input { r#type: "checkbox", checked: force_lm(), onchange: move |e| force_lm.set(e.checked()) }
                {t!("force-location-manager")}
            }
        }
    }
}
