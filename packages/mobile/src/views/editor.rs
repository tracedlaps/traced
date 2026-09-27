use std::{collections::BTreeSet, sync::Arc};

use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::track::{import, Track, TrackBuilder, TrackShape, Waypoint};
use uuid::Uuid;

use super::{
    backup::Backup,
    map::{LatLng, Map, MapEvent, Overlay, View},
    import_error, timed_m, track_error,
    track_map::Layout,
};
use crate::{recorder::RecorderCtx, storage, Route};

/// The Editor tab: create a track on the map, import another app's export, or pick a saved one.
#[component]
pub fn Import() -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let mut imported = use_signal(|| None::<Draft>);
    let mut error = use_signal(|| None::<String>);
    if let Some(initial) = imported() {
        return rsx! { Editor { initial, on_close: move |_| imported.set(None) } };
    }

    let pick = move |_| async move {
        match storage::pick_file().await {
            Ok(None) => {}
            Ok(Some(file)) => match std::fs::read_to_string(&file.path)
                .map_err(|e| e.to_string())
                .and_then(|text| import::waypoints(&text).map_err(|e| import_error(&e)))
            {
                Ok(w) => {
                    let stem = file.name.rsplit_once('.').map_or(&*file.name, |(stem, _)| stem);
                    error.set(None);
                    imported.set(Some(Draft::imported(stem.replace('_', " "), w)));
                }
                Err(e) => error.set(Some(format!("{}: {e}", file.name))),
            },
            Err(e) => error.set(Some(e)),
        }
    };

    rsx! {
        main { class: "form",
            h1 { {t!("editor")} }
            div { class: "scroll",
                Link { class: "button", to: Route::NewTrack {}, {t!("create-track")} }
                Link { class: "button secondary", to: Route::RecordTrack {}, {t!("record-track")} }
                button { class: "button secondary", onclick: pick, {t!("import-file")} }
                if let Some(e) = error() {
                    p { class: "error", "{e}" }
                }
                if !tracks.read().is_empty() {
                    section { class: "backup",
                        h2 { {t!("edit-saved-track")} }
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
    }
}

#[component]
pub fn NewTrack() -> Element {
    rsx! {
        Editor {
            initial: Draft::default(),
            on_close: move |_| _ = navigator().push(Route::Import {}),
        }
    }
}

#[component]
pub fn EditTrack(id: Uuid) -> Element {
    let spec = use_hook(|| storage::load_spec(id));
    match spec {
        Some(spec) => rsx! {
            Editor {
                initial: Draft::from_spec(&spec),
                on_close: move |_| _ = navigator().push(Route::Tracks {}),
            }
        },
        None => rsx! {
            main { class: "form",
                h1 { {t!("edit-track")} }
                p { class: "note", {t!("track-not-found")} }
            }
        },
    }
}

/// Lay out a track's waypoints on the map, name it, say whether it loops, and place its start,
/// finish and sector gates on waypoints. Saves (or deletes) the track `initial` was loaded from.
#[component]
pub(super) fn Editor(initial: Draft, on_close: EventHandler) -> Element {
    let mut tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let ctx = use_context::<RecorderCtx>();
    let id = initial.uuid;
    let original = use_hook(|| (initial.waypoints.len() >= 2).then(|| initial.builder(false)));
    let mut draft = use_signal(|| initial);
    // Earlier drafts, for undo; the name isn't undone.
    let mut history = use_signal(Vec::<Draft>::new);
    let mut mode = use_signal(|| Mode::Draw);
    let mut selected = use_signal(|| None::<usize>);
    let mut error = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| None::<usize>);

    // Length and sectors, or why the draft doesn't build; and the track drawn over the map.
    let preview = use_memo(move || {
        let d = draft.read();
        if d.waypoints.len() < 2 {
            return (Err(t!("too-few-waypoints")), None);
        }
        let built = d.builder(false).build();
        let summary = built
            .as_ref()
            .map(|t| format!("{:.0} m · {}", timed_m(t), t!("sector-count", count: t.sectors().count())))
            .map_err(track_error);
        // Still drawn when a placement is what fails, just with gates on the ends only.
        let track = built.ok().or_else(|| d.builder(true).build().ok());
        (summary, track.as_ref().map(overlay))
    });
    let view = use_memo(move || {
        let d = draft.read();
        View {
            points: d.waypoints.iter().map(|&w| w.into()).collect(),
            classes: (0..d.waypoints.len()).map(|i| d.class(i, selected() == Some(i))).collect(),
            closed: d.closed,
            midpoints: mode() == Mode::Draw,
            overlay: preview().1,
            ..View::default()
        }
    });
    // Its gates can't move, nor it go, while it's being recorded.
    let live = ctx.recording.read().is_some()
        && ctx.track.read().as_ref().map(|t| t.uuid) == id;
    let moved = original.as_ref().is_some_and(|o| {
        let d = draft.read();
        d.waypoints.len() >= 2 && !o.same_geometry(&d.builder(false))
    });

    let mut commit = move |d: Draft| history.write().push(draft.replace(d));
    let on_event = move |e: MapEvent| {
        let mut d = draft();
        match (e, mode()) {
            (MapEvent::TapMap(p), Mode::Draw) => d.insert(d.waypoints.len(), p.into()),
            (MapEvent::TapMap(_), _) => return,
            (MapEvent::TapMarker(i), Mode::Draw) => {
                return selected.set((selected() != Some(i)).then_some(i));
            }
            (MapEvent::TapMarker(i), m) => d.place(m, i),
            (MapEvent::Drag(i, p), _) => d.waypoints[i] = p.into(),
            (MapEvent::Insert(i, p), _) => d.insert(i + 1, p.into()),
        }
        if !matches!(e, MapEvent::Drag(..)) {
            selected.set(None);
        }
        commit(d);
    };
    let save = move |_| {
        let track = draft.read().builder(false);
        let Ok(built) = track.clone().build() else {
            return;
        };
        if storage::put_track(&track).is_none() {
            error.set(Some(t!("save-failed")));
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
            error.set(Some(t!("delete-failed")));
            return;
        }
        tracks.write().retain(|t| t.uuid != id);
        navigator().push(Route::Tracks {});
    };
    let closed = draft.read().closed;
    let modes: &[(Mode, &str)] = match closed {
        true => &[(Mode::Draw, "mode-draw"), (Mode::Start, "mode-gate"), (Mode::Sector, "mode-sectors")],
        false => &[
            (Mode::Draw, "mode-draw"),
            (Mode::Start, "mode-start"),
            (Mode::Finish, "mode-finish"),
            (Mode::Sector, "mode-sectors"),
        ],
    };

    rsx! {
        main { class: "map-editor",
            Map { view, on_event }
            div { class: "sheet form",
                div { class: "modes",
                    for &(m , label) in modes {
                        button {
                            class: if mode() == m { "button" } else { "button secondary" },
                            onclick: move |_| {
                                mode.set(m);
                                selected.set(None);
                            },
                            {t!(label)}
                        }
                    }
                }
                p { class: "note",
                    {t!(match mode() {
                        Mode::Draw if selected().is_some() => "hint-selected",
                        Mode::Draw => "hint-draw",
                        Mode::Sector => "hint-sectors",
                        _ => "hint-gate",
                    })}
                }
                div { class: "row",
                    button {
                        class: "button secondary",
                        disabled: history.read().is_empty(),
                        onclick: move |_| {
                            if let Some(prev) = history.write().pop() {
                                let name = draft.read().name.clone();
                                draft.set(Draft { name, ..prev });
                                selected.set(None);
                            }
                        },
                        {t!("undo")}
                    }
                    if let Some(i) = selected() {
                        button {
                            class: "stop",
                            onclick: move |_| {
                                let mut d = draft();
                                d.remove(i);
                                selected.set(None);
                                commit(d);
                            },
                            {t!("delete-point")}
                        }
                    }
                }
                label { class: "field",
                    {t!("name")}
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
                    {t!("closed-loop")}
                }
                match preview().0 {
                    Ok(summary) => rsx! { small { "{summary}" } },
                    Err(e) => rsx! { p { class: "error", "{e}" } },
                }
                if let Some(e) = error() {
                    p { class: "error", "{e}" }
                }
                if live {
                    p { class: "note", {t!("live-track-locked")} }
                }
                div { class: "row",
                    button {
                        disabled: preview().0.is_err() || draft.read().name.trim().is_empty() || live && moved,
                        onclick: save,
                        {t!("save-track")}
                    }
                    button {
                        class: "button secondary",
                        onclick: move |_| on_close(()),
                        {t!(if id.is_some() { "cancel" } else { "discard" })}
                    }
                }
                if let Some(id) = id {
                    section { class: "backup",
                        if let Some(n) = confirm_delete() {
                            p { class: "error",
                                {t!("delete-warning", count: n)}
                            }
                            button { class: "stop", onclick: move |_| delete(id), {t!("delete-track")} }
                            button { class: "button secondary", onclick: move |_| confirm_delete.set(None), {t!("keep-track")} }
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
                                {t!("delete-track")}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The track's `Layout`, placed over the map by un-projecting its view box's corners.
fn overlay(track: &Track) -> Overlay {
    let layout = Layout::new(track);
    let [x, y, w, h] = layout.view_box;
    // SVG y points down: 0.0 - y, not -y, so no "-0.0".
    let at = |x: f64, y: f64| LatLng::from(track.projection.unproject(Waypoint::new(x, 0.0 - y)));
    Overlay {
        view_box: format!("{x} {y} {w} {h}"),
        svg: layout.markup(None),
        bounds: [at(x, y + h), at(x + w, y)],
    }
}

/// What tapping does.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Tapping the map adds a waypoint; tapping one selects it.
    Draw,
    /// The start gate, or a Closed track's one gate.
    Start,
    Finish,
    /// Toggles a sector gate.
    Sector,
}

/// The track being edited, its gates as indices into `waypoints`.
#[derive(Clone, Default, PartialEq)]
pub(super) struct Draft {
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
    pub(super) fn imported(name: String, w: Vec<Waypoint>) -> Self {
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

    fn class(&self, i: usize, selected: bool) -> String {
        let class = match i {
            _ if i == self.start => "waypoint start",
            _ if !self.closed && i == self.finish => "waypoint finish",
            _ if self.sectors.contains(&i) => "waypoint gate",
            _ => "waypoint",
        };
        if selected { format!("{class} selected") } else { class.into() }
    }

    /// Adds `w` at `i`, its gates staying on their waypoints; one added right after a finish
    /// on the last waypoint takes the finish with it, so drawing a track extends it.
    fn insert(&mut self, i: usize, w: Waypoint) {
        let n = self.waypoints.len();
        let shift = |j: usize| if j >= i && n > 0 { j + 1 } else { j };
        self.sectors = self.sectors.iter().map(|&j| shift(j)).collect();
        self.start = shift(self.start);
        self.finish = if n > 0 && i == n && self.finish == n - 1 { n } else { shift(self.finish) };
        self.waypoints.insert(i, w);
    }

    /// Removes waypoint `i` and any sector gate on it; a start on it moves to the next
    /// waypoint, a finish to the previous one.
    fn remove(&mut self, i: usize) {
        self.waypoints.remove(i);
        let last = self.waypoints.len().saturating_sub(1);
        let shift = |j: usize| if j > i { j - 1 } else { j };
        self.sectors = self.sectors.iter().filter(|&&j| j != i).map(|&j| shift(j)).collect();
        self.start = shift(self.start).min(last);
        self.finish = match self.finish == i {
            true => i.saturating_sub(1),
            false => shift(self.finish),
        }
        .min(last);
        self.sectors.remove(&self.start);
        if !self.closed {
            self.sectors.remove(&self.finish);
        }
    }

    /// Moves a boundary gate onto waypoint `i` (replacing any sector gate there), or toggles one.
    fn place(&mut self, mode: Mode, i: usize) {
        match mode {
            Mode::Draw => return,
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

    fn p(i: usize) -> Waypoint {
        Waypoint::new(6.0 + i as f64 * 0.001, 49.5)
    }

    #[test]
    fn drawing_a_track_keeps_the_finish_on_its_end() {
        let mut draft = Draft::default();
        for i in 0..3 {
            draft.insert(i, p(i));
        }
        assert_eq!((draft.start, draft.finish), (0, 2));
        assert!(draft.builder(false).build().is_ok());
    }

    #[test]
    fn gates_stay_on_their_waypoints_through_inserts_and_removals() {
        let mut draft = Draft { waypoints: (0..4).map(p).collect(), finish: 3, ..Draft::default() };
        draft.place(Mode::Sector, 2);
        draft.insert(1, p(9));
        assert_eq!((draft.start, draft.finish), (0, 4));
        assert_eq!(draft.sectors, BTreeSet::from([3]));

        draft.remove(3); // the sector gate's waypoint
        assert!(draft.sectors.is_empty());
        draft.place(Mode::Sector, 2);
        draft.remove(0); // the start: onto the next
        assert_eq!((draft.start, draft.finish), (0, 2));
        assert_eq!(draft.sectors, BTreeSet::from([1]));
        draft.remove(2); // the finish: onto the previous, replacing the sector gate there
        assert_eq!((draft.start, draft.finish), (0, 1));
        assert!(draft.sectors.is_empty());
        draft.remove(0);
        draft.remove(0);
        assert_eq!((draft.start, draft.finish, draft.waypoints.len()), (0, 0, 0));
    }
}
