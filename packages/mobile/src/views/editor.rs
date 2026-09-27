use std::{collections::BTreeSet, sync::Arc};

use dioxus::prelude::*;
use traced_core::track::{import, Track, TrackBuilder, TrackShape, Waypoint};
use uuid::Uuid;

use super::{
    backup::Backup,
    timed_m,
    track_map::{svg_point, Layout, SvgPoint, TrackSvg},
};
use crate::{recorder::RecorderCtx, storage, Route};

/// The Editor tab: import another app's export as a new track, or pick a saved one to edit.
#[component]
pub fn Import() -> Element {
    rsx! { Editor { id: None } }
}

#[component]
pub fn EditTrack(id: Uuid) -> Element {
    rsx! { Editor { id } }
}

/// Name a track, say whether it loops, and tap waypoints to place its start, finish and
/// sector gates. With `id`, edits (or deletes) that saved track; without, imports a file.
#[component]
fn Editor(id: Option<Uuid>) -> Element {
    let mut tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let ctx = use_context::<RecorderCtx>();
    let original = use_hook(|| id.and_then(storage::load_spec));
    let mut draft = use_signal(|| original.as_ref().map(Draft::from_spec).unwrap_or_default());
    let mut mode = use_signal(|| Mode::Start);
    let mut error = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| None::<usize>);

    let preview = use_memo(move || {
        let d = draft.read();
        if d.waypoints.len() < 2 {
            return None;
        }
        let built = d.builder(false).build();
        let summary = built
            .as_ref()
            .map(|t| format!("{:.0} m · {} sectors", timed_m(t), t.sectors().count()))
            .map_err(|e| e.to_string());
        // Still drawn when a placement is what fails, just with gates on the ends only.
        let track = built.ok().or_else(|| d.builder(true).build().ok())?;
        Some(Preview {
            layout: Layout::new(&track),
            points: d.waypoints.iter().map(|p| svg_point(&track, *p)).collect(),
            summary,
        })
    });
    // Its gates can't move, nor it go, while it's being recorded.
    let live = ctx.recording.read().is_some()
        && ctx.track.read().as_ref().map(|t| t.uuid) == id;
    let moved = original
        .as_ref()
        .is_some_and(|o| !o.same_geometry(&draft.read().builder(false)));

    let pick = move |_| async move {
        match storage::pick_file().await {
            Ok(None) => {}
            Ok(Some(file)) => match std::fs::read_to_string(&file.path)
                .map_err(|e| e.to_string())
                .and_then(|text| import::waypoints(&text).map_err(|e| e.to_string()))
            {
                Ok(w) => {
                    let stem = file.name.rsplit_once('.').map_or(&*file.name, |(stem, _)| stem);
                    draft.set(Draft::imported(stem.replace('_', " "), w));
                    mode.set(Mode::Start);
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("{}: {e}", file.name))),
            },
            Err(e) => error.set(Some(e)),
        }
    };
    let save = move |_| {
        let track = draft.read().builder(false);
        let Ok(built) = track.clone().build() else {
            return;
        };
        if storage::put_track(&track).is_none() {
            error.set(Some("Couldn't save the track.".into()));
            return;
        }
        let built = Arc::new(built);
        let mut all = tracks.write();
        match all.iter().position(|t| t.uuid == built.uuid) {
            Some(i) => all[i] = built,
            None => all.push(built),
        }
        drop(all);
        navigator().push(Route::Tracks {});
    };
    let mut delete = move |id: Uuid| {
        if storage::delete_track(id).is_none() {
            error.set(Some("Couldn't delete the track.".into()));
            return;
        }
        tracks.write().retain(|t| t.uuid != id);
        navigator().push(Route::Tracks {});
    };
    let closed = draft.read().closed;
    let modes: &[(Mode, &str)] = match closed {
        true => &[(Mode::Start, "Gate"), (Mode::Sector, "Sectors")],
        false => &[(Mode::Start, "Start"), (Mode::Finish, "Finish"), (Mode::Sector, "Sectors")],
    };

    rsx! {
        main { class: "form",
            h1 { if id.is_some() { "Edit track" } else { "Import track" } }
            div { class: "scroll",
                if id.is_none() {
                    button { class: "button secondary", onclick: pick, "Choose a CSV, GPX or KML file" }
                } else if original.is_none() {
                    p { class: "note", "Track not found." }
                }
                if let Some(e) = error() {
                    p { class: "error", "{e}" }
                }
                if let Some(p) = preview() {
                    label { class: "field",
                        "Name"
                        input { r#type: "text", value: "{draft.read().name}", oninput: move |e| draft.write().name = e.value() }
                    }
                    label { class: "check",
                        input {
                            r#type: "checkbox",
                            checked: closed,
                            onchange: move |e| {
                                draft.write().closed = e.checked();
                                if mode() == Mode::Finish {
                                    mode.set(Mode::Start);
                                }
                            },
                        }
                        "Closed loop, timed in laps"
                    }
                    div { class: "modes",
                        for &(m , label) in modes {
                            button {
                                class: if mode() == m { "button" } else { "button secondary" },
                                onclick: move |_| mode.set(m),
                                "{label}"
                            }
                        }
                    }
                    p { class: "note",
                        match mode() {
                            Mode::Sector => "Tap waypoints to add or remove sector gates.",
                            _ => "Tap a waypoint to move the gate there.",
                        }
                    }
                    TrackSvg { layout: p.layout.clone(), current: None,
                        for (i , (cx , cy)) in p.points.iter().copied().enumerate() {
                            circle {
                                class: draft.read().class(i),
                                cx,
                                cy,
                                r: p.layout.dot_r,
                                onclick: move |_| draft.write().place(mode(), i),
                            }
                        }
                    }
                    match &p.summary {
                        Ok(summary) => rsx! { small { "{summary}" } },
                        Err(e) => rsx! { p { class: "error", "{e}" } },
                    }
                    if live {
                        p { class: "note", "Stop the recording on this track to move its gates or delete it." }
                    }
                }
                if let Some(id) = id.filter(|_| original.is_some()) {
                    section { class: "backup",
                        if let Some(n) = confirm_delete() {
                            p { class: "error",
                                match n {
                                    0 => "Deletes the track.".to_string(),
                                    1 => "Deletes the track and its recording.".into(),
                                    n => format!("Deletes the track and its {n} recordings."),
                                }
                            }
                            button { class: "stop", onclick: move |_| delete(id), "Delete track" }
                            button { class: "button secondary", onclick: move |_| confirm_delete.set(None), "Keep track" }
                        } else {
                            button {
                                class: "button secondary",
                                disabled: live,
                                onclick: move |_| {
                                    let n = storage::recordings()
                                        .into_iter()
                                        .filter(|r| r.session.as_ref().is_some_and(|s| s.track_id == id))
                                        .count();
                                    confirm_delete.set(Some(n));
                                },
                                "Delete track"
                            }
                        }
                    }
                } else if id.is_none() && preview.read().is_none() {
                    if !tracks.read().is_empty() {
                        section { class: "backup",
                            h2 { "Edit a saved track" }
                            ul { class: "tracks",
                                for t in tracks.read().iter() {
                                    li {
                                        Link { to: Route::EditTrack { id: t.uuid }, "{t.name}" }
                                    }
                                }
                            }
                        }
                    }
                    Backup {}
                }
            }
            if let Some(p) = preview() {
                button {
                    disabled: p.summary.is_err() || draft.read().name.trim().is_empty() || live && moved,
                    onclick: save,
                    "Save track"
                }
                button {
                    class: "button secondary",
                    onclick: move |_| match id {
                        Some(_) => _ = navigator().push(Route::Tracks {}),
                        None => {
                            draft.set(Draft::default());
                            error.set(None);
                        }
                    },
                    if id.is_some() { "Cancel" } else { "Discard" }
                }
            }
        }
    }
}

/// What tapping a waypoint places.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// The start gate, or a Closed track's one gate.
    Start,
    Finish,
    /// Toggles a sector gate.
    Sector,
}

/// The track being edited, its gates as indices into `waypoints`.
#[derive(Clone, Default, PartialEq)]
struct Draft {
    /// A saved track's; `None` until an import is first saved.
    uuid: Option<Uuid>,
    name: String,
    closed: bool,
    start: usize,
    /// Unused while `closed`.
    finish: usize,
    sectors: BTreeSet<usize>,
    waypoints: Vec<Waypoint>,
}

impl Draft {
    /// Start/finish (or a Closed track's gate) on the end waypoints, no sectors.
    fn imported(name: String, w: Vec<Waypoint>) -> Self {
        Self {
            name,
            closed: import::looks_closed(&w),
            finish: w.len().saturating_sub(1),
            waypoints: w,
            ..Self::default()
        }
    }

    /// Each placement on its nearest waypoint (they're normally one of them).
    fn from_spec(spec: &TrackBuilder) -> Self {
        let w = spec.waypoint_list();
        let at = |p: Waypoint| {
            let d = |i: &usize| (w[*i].x() - p.x()).hypot(w[*i].y() - p.y());
            (0..w.len()).min_by(|a, b| d(a).total_cmp(&d(b))).unwrap_or(0)
        };
        let (closed, start, finish) = match spec.shape() {
            TrackShape::Closed { gate } => (true, at(gate), w.len().saturating_sub(1)),
            TrackShape::Open { start, finish } => (false, at(start), at(finish)),
        };
        Self {
            uuid: Some(spec.uuid()),
            name: spec.name().into(),
            closed,
            start,
            finish,
            sectors: spec.sector_placements().iter().map(|&p| at(p)).collect(),
            waypoints: w.to_vec(),
        }
    }

    /// The track as placed; `bare` keeps only gates on the end waypoints, so a bad placement still draws.
    fn builder(&self, bare: bool) -> TrackBuilder {
        let w = &self.waypoints;
        let (start, finish) = match bare {
            true => (0, w.len() - 1),
            false => (self.start, self.finish),
        };
        let shape = match self.closed {
            true => TrackShape::Closed { gate: w[start] },
            false => TrackShape::Open { start: w[start], finish: w[finish] },
        };
        let track = Track::builder(self.name.trim(), shape).waypoints(w.clone());
        let track = match self.uuid {
            Some(uuid) => track.with_uuid(uuid),
            None => track,
        };
        match bare {
            true => track,
            false => self.sectors.iter().fold(track, |b, &i| b.sector_gate_at(w[i])),
        }
    }

    fn is_boundary(&self, i: usize) -> bool {
        i == self.start || !self.closed && i == self.finish
    }

    fn class(&self, i: usize) -> &'static str {
        match i {
            _ if i == self.start => "waypoint start",
            _ if !self.closed && i == self.finish => "waypoint finish",
            _ if self.sectors.contains(&i) => "waypoint gate",
            _ => "waypoint",
        }
    }

    /// Moves a boundary gate onto waypoint `i` (replacing any sector gate there), or toggles one.
    fn place(&mut self, mode: Mode, i: usize) {
        match mode {
            Mode::Start => self.start = i,
            Mode::Finish => self.finish = i,
            Mode::Sector if self.is_boundary(i) => return,
            Mode::Sector => {
                if !self.sectors.remove(&i) {
                    self.sectors.insert(i);
                }
                return;
            }
        }
        self.sectors.remove(&i);
    }
}

#[derive(Clone, PartialEq)]
struct Preview {
    layout: Layout,
    /// Each waypoint, in the layout's coordinates.
    points: Vec<SvgPoint>,
    /// Length and sectors, or why the draft doesn't build.
    summary: Result<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::PRESETS;

    #[test]
    fn specs_round_trip_through_a_draft() {
        // All but the Lisbon loop place every gate on a waypoint.
        for p in &PRESETS[..3] {
            let draft = Draft::from_spec(&p.track);
            assert!(draft.builder(false).same_geometry(&p.track), "{}", p.track.name());
            assert_eq!(draft.builder(false).uuid(), p.track.uuid());
        }
    }

    #[test]
    fn moving_the_start_off_the_end_shortens_the_run() {
        let spec = &PRESETS[2].track; // Luxembourg Sprint, Open
        let mut draft = Draft::from_spec(spec);
        let full = draft.builder(false).build().unwrap().sectors().map(|s| s.length).sum::<f64>();
        draft.place(Mode::Start, 1);
        let track = draft.builder(false).build().unwrap();
        assert!(track.sectors().map(|s| s.length).sum::<f64>() < full);

        // A sector gate can't sit on the start, and the start replaces one there.
        draft.place(Mode::Sector, 1);
        assert!(!draft.sectors.contains(&1));
        draft.place(Mode::Sector, 3);
        draft.place(Mode::Start, 3);
        assert!(!draft.sectors.contains(&3));
    }
}
