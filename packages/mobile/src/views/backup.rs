use std::sync::Arc;

use dioxus::prelude::*;
use traced_core::track::Track;

use crate::{recorder::RecorderCtx, storage};

/// Export everything to a file, or replace everything with one.
#[component]
pub fn Backup() -> Element {
    let mut tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    // Restoring would pull the recording's row out from under it.
    let recording = use_context::<RecorderCtx>().recording.read().is_some();
    // Last backup action's outcome.
    let mut status = use_signal(|| None::<Result<&str, String>>);
    let mut confirm_restore = use_signal(|| false);

    let export = move |_| async move {
        match storage::export_backup().await {
            Ok(true) => status.set(Some(Ok("Backup saved."))),
            Ok(false) => {}
            Err(e) => status.set(Some(Err(e))),
        }
    };
    let restore = move |_| async move {
        confirm_restore.set(false);
        match storage::restore_backup().await {
            Ok(true) => {
                tracks.set(storage::load_tracks());
                status.set(Some(Ok("Backup restored.")));
            }
            Ok(false) => {}
            Err(e) => status.set(Some(Err(e))),
        }
    };

    rsx! {
        section { class: "backup",
            h2 { "Backup" }
            p { class: "note", "Everything lives only on this phone. Export it to keep a copy." }
            button { class: "button secondary", onclick: export, "Export backup" }
            if confirm_restore() {
                p { class: "error", "Restoring replaces every track and recording here with the backup's." }
                button { class: "stop", onclick: restore, "Choose backup and replace" }
                button { class: "button secondary", onclick: move |_| confirm_restore.set(false), "Cancel" }
            } else {
                button {
                    class: "button secondary",
                    disabled: recording,
                    onclick: move |_| confirm_restore.set(true),
                    "Restore backup"
                }
            }
            match status() {
                Some(Ok(done)) => rsx! { p { class: "note", "{done}" } },
                Some(Err(e)) => rsx! { p { class: "error", "{e}" } },
                None => rsx! {},
            }
        }
    }
}
