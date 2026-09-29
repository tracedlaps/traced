use std::{fs, path::PathBuf, sync::Arc};

use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::{
    timing::best_lap,
    track::{export, Track},
};
use uuid::Uuid;

use super::{lap_time, timed_m};
use crate::{share, storage, units::Units};

/// A track's card (and GPX) to share, e.g. on WhatsApp, or save to the phone.
#[component]
pub fn ShareTrack(id: Uuid) -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let mut with_best = use_signal(|| true);
    let mut with_gpx = use_signal(|| true);
    // Last action's outcome: a message key, or the error.
    let mut status = use_signal(|| None::<Result<&str, String>>);
    let Some(track) = tracks.read().iter().find(|t| t.uuid == id).cloned() else {
        return rsx! { main { p { class: "error", {t!("track-not-found")} } } };
    };

    let best = best_lap(&storage::recordings(), id).map(|(_, lap)| lap.duration().as_seconds_f64());
    let stats: Vec<_> = [
        (t!("card-length"), Units::current().length(timed_m(&track)), false),
        (t!("card-sectors"), track.sectors().count().to_string(), false),
    ]
    .into_iter()
    .chain(best.filter(|_| with_best()).map(|s| (t!("card-best-lap"), lap_time(s), true)))
    .collect();
    let svg = share::card_svg(&track, &stats);
    let preview = svg.clone();

    let (card, gpx) = (svg.clone(), track.clone());
    let share = move |_| {
        let (svg, track) = (card.clone(), gpx.clone());
        async move {
            let files = files(&track, &svg, true, with_gpx());
            let done = match files {
                Ok(files) => storage::share_files(files, track.name.clone()).await.map(|()| None),
                Err(e) => Err(e),
            };
            status.set(done.transpose());
        }
    };
    let save = move |image: bool| {
        let (svg, track) = (svg.clone(), track.clone());
        async move {
            let saved = match files(&track, &svg, image, !image) {
                Ok(files) => {
                    let file = files[0].clone();
                    let name = file.file_name().unwrap().to_string_lossy().into_owned();
                    storage::save_file(file, name).await
                }
                Err(e) => Err(e),
            };
            match saved {
                Ok(true) => status.set(Some(Ok("share-saved"))),
                Ok(false) => {}
                Err(e) => status.set(Some(Err(e))),
            }
        }
    };
    let save_gpx = save.clone();

    rsx! {
        main { class: "share scroll",
            h1 { {t!("share-track")} }
            div { class: "card-preview", dangerous_inner_html: preview }
            if best.is_some() {
                label { class: "check",
                    input { r#type: "checkbox", checked: with_best(), onchange: move |e| with_best.set(e.checked()) }
                    {t!("share-with-best")}
                }
            }
            if cfg!(target_os = "android") {
                label { class: "check",
                    input { r#type: "checkbox", checked: with_gpx(), onchange: move |e| with_gpx.set(e.checked()) }
                    {t!("share-with-gpx")}
                }
                button { onclick: share, {t!("share")} }
            }
            button { class: "button secondary", onclick: move |_| save(true), {t!("save-image")} }
            button { class: "button secondary", onclick: move |_| save_gpx(false), {t!("save-gpx")} }
            match status() {
                Some(Ok(done)) => rsx! { p { class: "note", {t!(done)} } },
                Some(Err(e)) => rsx! { p { class: "error", {t!("share-failed", error: e)} } },
                None => rsx! {},
            }
        }
    }
}

/// Writes the card (as a PNG) and/or the track (as GPX) into a fresh share dir, card first.
fn files(track: &Track, svg: &str, image: bool, gpx: bool) -> Result<Vec<PathBuf>, String> {
    let dir = storage::share_dir().ok_or("no share dir")?;
    let stem = track.name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");
    let mut out = vec![];
    if image {
        out.push(dir.join(format!("{stem}.png")));
        fs::write(&out[0], share::card_png(svg)?).map_err(|e| e.to_string())?;
    }
    if gpx {
        let spec = storage::load_spec(track.uuid).ok_or("track not saved")?;
        let path = dir.join(format!("{stem}.gpx"));
        fs::write(&path, export::gpx(&spec)).map_err(|e| e.to_string())?;
        out.push(path);
    }
    Ok(out)
}
