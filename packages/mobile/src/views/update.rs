use dioxus::prelude::*;
use dioxus_i18n::t;

use crate::update::{self, Latest, VERSION};

/// This build's version and, when there are newer ones, an install button and what's new in each.
/// Play builds show only the version: Play does the updating.
#[component]
pub fn Updates() -> Element {
    let mut latest = use_context::<Latest>();
    let mut installing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    rsx! {
        section { class: "backup",
            h2 { {t!("updates")} }
            p { class: "note", {t!("update-version", version : VERSION)} }
            if !cfg!(feature = "play") {
                match latest() {
                    None => rsx! {
                        p { class: "note", {t!("update-checking")} }
                    },
                    Some(Ok(releases)) if releases.is_empty() => rsx! {
                        p { class: "note", {t!("update-none")} }
                    },
                    Some(Ok(releases)) => {
                        let release = releases[0].clone();
                        rsx! {
                            button {
                                class: "button",
                                disabled: installing(),
                                onclick: move |_| {
                                    let release = release.clone();
                                    async move {
                                        installing.set(true);
                                        error.set(update::install(release).await.err());
                                        installing.set(false);
                                    }
                                },
                                if installing() {
                                    {t!("update-downloading")}
                                } else {
                                    {t!("update-install", version : release.version.clone())}
                                }
                            }
                            for release in releases.into_iter().filter(|r| !r.changes.is_empty()) {
                                h3 { {t!("update-whats-new", version : release.version)} }
                                ul { class: "notes",
                                    for change in release.changes {
                                        li { "{change}" }
                                    }
                                }
                            }
                        }
                    }
                    Some(Err(e)) => rsx! {
                        p { class: "error", {t!("update-check-failed", error : update::describe(e))} }
                        button { class: "button secondary", onclick: move |_| latest.restart(), {t!("try-again")} }
                    },
                }
            }
            if let Some(e) = error() {
                p { class: "error", {t!("update-failed", error : update::describe(e))} }
            }
        }
    }
}
