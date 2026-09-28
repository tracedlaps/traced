use dioxus::{core::spawn_forever, prelude::*};
use dioxus_i18n::t;

use super::{
    editor::{Draft, Editor},
    gps_status,
    map::{street_at, Map, View},
    refusal,
};
use crate::{gps::wire::Permission, recorder::RecorderCtx, units::Units, Route};

/// Trace a drive or walk on the map, then open it in the editor to place gates and save.
/// Leaving the screen before Stop discards it.
#[component]
pub fn RecordTrack() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let mut traced = use_signal(|| None::<Draft>);
    let mut error = use_signal(|| None::<String>);
    let mut stopping = use_signal(|| false);

    let start = move || async move {
        error.set(None);
        if let Err(e) = ctx.capture().await {
            error.set(Some(refusal(e)));
        }
    };
    use_hook(|| spawn(start()));
    use_drop(move || _ = spawn_forever(async move { ctx.end_capture().await; }));

    let view = use_memo(move || View {
        trace: ctx.trace.read().iter().flat_map(|t| t.iter()).map(|&w| w.into()).collect(),
        me: (ctx.fix)().map(Into::into),
        ..View::default()
    });
    let stop = move |_| async move {
        stopping.set(true);
        let w = ctx.end_capture().await.unwrap_or_default();
        let name = match w.first() {
            Some(&first) => street_at(first.into()).await,
            None => None,
        };
        traced.set(Some(Draft::imported(name.unwrap_or_else(|| t!("recorded-track")), w)));
    };
    let cancel = move |_| async move {
        ctx.end_capture().await;
        navigator().push(Route::Import {});
    };

    if let Some(initial) = traced() {
        return rsx! {
            Editor { initial, on_close: move |_| _ = navigator().push(Route::Import {}) }
        };
    }
    let capturing = ctx.capture.read().is_some();
    let distance = ctx.speed.read().unwrap_or_default().distance_m;

    rsx! {
        main { class: "map-editor",
            Map { view, on_event: |_| {}, locate: false }
            div { class: "sheet form",
                p { class: "note",
                    if stopping() {
                        {t!("finishing")}
                    } else if capturing {
                        {t!("record-hint")}
                    } else if error().is_none() {
                        {t!("starting-gps")}
                    }
                }
                if let Some(e) = error() {
                    p { class: "error", "{e}" }
                    div { class: "row",
                        button { onclick: move |_| start(), {t!("try-again")} }
                        if *ctx.permission.read() == Some(Permission::Blocked) {
                            button { class: "button secondary", onclick: move |_| ctx.open_settings(), {t!("open-settings")} }
                        }
                    }
                }
                small { "{gps_status(*ctx.status.read())} · {Units::current().length(distance)}" }
                div { class: "row",
                    button {
                        disabled: stopping() || view.read().trace.len() < 2,
                        onclick: stop,
                        {t!("stop")}
                    }
                    button { class: "button secondary", disabled: stopping(), onclick: cancel, {t!("cancel")} }
                }
            }
        }
    }
}
