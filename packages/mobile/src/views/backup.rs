use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::track::Track;

use crate::{prefs, recorder::RecorderCtx, storage};

/// Export everything (tracks, recordings, settings) to a file, or replace everything with one.
#[component]
pub fn Backup() -> Element {
    let mut tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    // Restoring would pull the recording's row out from under it.
    let recording = use_context::<RecorderCtx>().recording.read().is_some();
    // Last backup action's outcome: a message key, or the error.
    let mut status = use_signal(|| None::<Result<&str, String>>);
    let mut confirm_restore = use_signal(|| false);

    let export = move |_| async move {
        match storage::export_backup().await {
            Ok(true) => status.set(Some(Ok("backup-saved"))),
            Ok(false) => {}
            Err(e) => status.set(Some(Err(e))),
        }
    };
    let restore = move |_| async move {
        confirm_restore.set(false);
        match storage::restore_backup().await {
            Ok(true) => {
                tracks.set(storage::load_tracks());
                prefs::reload();
                status.set(Some(Ok("backup-restored")));
            }
            Ok(false) => {}
            Err(e) => status.set(Some(Err(e))),
        }
    };

    rsx! {
        section { class: "backup",
            h2 { {t!("backup")} }
            p { class: "note", {t!("backup-note")} }
            button { class: "button secondary", onclick: export, {t!("export-backup")} }
            if confirm_restore() {
                p { class: "error", {t!("restore-warning")} }
                button { class: "stop", onclick: restore, {t!("restore-choose")} }
                button { class: "button secondary", onclick: move |_| confirm_restore.set(false), {t!("cancel")} }
            } else {
                button {
                    class: "button secondary",
                    disabled: recording,
                    onclick: move |_| confirm_restore.set(true),
                    {t!("restore-backup")}
                }
            }
            match status() {
                Some(Ok(done)) => rsx! { p { class: "note", {t!(done)} } },
                Some(Err(e)) => rsx! { p { class: "error", {t!("backup-failed", error: e)} } },
                None => rsx! {},
            }
        }
    }
}
