use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::{
    braking::braking_zones,
    gps::GPSFix,
    imu::GForces,
    speedometer::SpeedometerSnapshot,
    timing::{along_lap, best_lap},
    track::{Session, Track},
};

use super::{
    lap_time,
    lap_chart::{LapChart, Quantity},
    track_map::{svg_point, Layout, SvgPoint, TrackSvg},
    Delta,
};
use crate::{recorder::RecorderCtx, storage, units::Units};

#[component]
pub fn Results() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let summary = ctx.last_summary.read();
    let Some(summary) = summary.as_ref() else {
        return rsx! {
            main {
                p { if ctx.recording.read().is_some() { {t!("recording-in-progress")} } else { {t!("finishing")} } }
            }
        };
    };

    rsx! {
        main {
            h1 { {t!("results")} }
            div { class: "scroll",
                Report { speed: summary.speedometer, session: summary.session.clone(), g: summary.g.clone(), fixes: summary.fixes.clone() }
            }
        }
    }
}

/// A finished recording's stats and laps, live or saved, and where the chosen lap braked.
#[component]
pub fn Report(speed: SpeedometerSnapshot, session: Option<Session>, g: Option<GForces>, fixes: Vec<GPSFix>) -> Element {
    let tracks = use_context::<Signal<Vec<Arc<Track>>>>();
    let mut chosen = use_signal(|| None::<usize>);
    let laps = session.as_ref().map_or(&[][..], Session::laps);
    let recordings = storage::recordings();
    let best = session.as_ref().and_then(|s| best_lap(&recordings, s.track_id)).map(|(_, lap)| lap);
    let units = Units::current();
    // The fastest clean lap until one is tapped.
    let shown = chosen().filter(|&i| i < laps.len()).or_else(|| (0..laps.len()).filter(|&i| !laps[i].gps_gap).min_by_key(|&i| laps[i].duration()));
    let track = session.as_ref().and_then(|s| tracks.read().iter().find(|t| t.uuid == s.track_id).cloned());
    // The lap and the PB (unless it is the PB), each fix with its distance in, and where each braked.
    let braking = track.as_ref().zip(shown).map(|(t, i)| {
        let (lap, pb) = (along_lap(t, &laps[i], &fixes), best.filter(|&b| *b != laps[i]).map(|_| storage::best_lap_along(t)).unwrap_or_default());
        let (zones, pb_zones) = (braking_zones(&lap), braking_zones(&pb));
        (i, lap, pb, zones, pb_zones)
    });
    rsx! {
        dl { class: "stats",
            dt { {t!("stat-top")} } dd { "{units.speed(speed.top_speed_ms)}" }
            dt { {t!("stat-avg")} } dd { "{units.speed(speed.average_speed_ms)}" }
            dt { {t!("stat-distance")} } dd { "{units.distance(speed.distance_m)}" }
            dt { {t!("stat-time")} } dd { {lap_time(speed.elapsed.as_secs_f64())} }
            if let Some(p) = g.as_ref().map(|g| g.peaks) {
                dt { {t!("g-braking")} } dd { "{p.braking:.2} g" }
                dt { {t!("g-accelerating")} } dd { "{p.accelerating:.2} g" }
                dt { {t!("g-lateral")} } dd { "{p.left:.2} g · {p.right:.2} g" }
            }
        }
        if session.is_some() && laps.is_empty() {
            p { {t!("no-completed-laps")} }
        }
        if best.is_some_and(|b| laps.contains(b)) {
            p { class: "pb", {t!("personal-best")} }
        }
        if !laps.is_empty() {
            table { class: "laps",
                tr {
                    th { "#" } th { {t!("stat-time")} } th { {t!("pb")} } th { {t!("splits")} }
                    if g.is_some() { th { {t!("g-lap")} } }
                }
                for (i, lap) in laps.iter().enumerate() {
                    tr { class: if shown == Some(i) { "shown" }, onclick: move |_| chosen.set(Some(i)),
                        td { "{i + 1}" }
                        td {
                            {lap_time(lap.duration().as_seconds_f64())}
                            if lap.gps_gap { span { class: "gap", title: t!("gps-gap"), " ⚠" } }
                        }
                        td {
                            match best {
                                Some(b) if b == lap => rsx! { span { class: "pb", {t!("pb")} } },
                                Some(b) => rsx! { Delta { delta: lap.delta(b) } },
                                None => rsx! {},
                            }
                        }
                        td { {lap.splits().map(|s| lap_time(s.as_seconds_f64())).collect::<Vec<_>>().join(" · ")} }
                        if let Some(p) = g.as_ref().and_then(|g| g.laps.get(i)) {
                            td { "{p.braking:.1} · {p.left.max(p.right):.1}" }
                        }
                    }
                }
            }
        }
        if let Some(t) = &track {
            BrakingMap {
                layout: Layout::new(t),
                zones: braking.iter().flat_map(|b| &b.3).map(|z| z.path.iter().map(|&w| svg_point(t, w)).collect()).collect(),
                pb: braking.iter().flat_map(|b| &b.4).filter_map(|z| z.path.first()).map(|&w| svg_point(t, w)).collect(),
            }
        }
        if let Some((i, lap, pb_lap, zones, pb)) = braking {
            LapChart {
                quantity: Quantity::Speed,
                number: i + 1,
                lap: lap.iter().map(|(d, f)| (*d, f.speed_ms)).collect(),
                pb: pb_lap.iter().map(|(d, f)| (*d, f.speed_ms)).collect(),
                zones: zones.iter().map(|z| (z.at_m, z.end_m)).collect(),
            }
            // Empty, so not drawn, when the GPS gave no altitude.
            LapChart {
                quantity: Quantity::Elevation,
                number: i + 1,
                lap: lap.iter().filter_map(|(d, f)| Some((*d, f.altitude_m?))).collect(),
                pb: pb_lap.iter().filter_map(|(d, f)| Some((*d, f.altitude_m?))).collect(),
                zones: zones.iter().map(|z| (z.at_m, z.end_m)).collect(),
            }
            h2 { {t!("braking-zones", lap: i + 1)} }
            if zones.is_empty() {
                p { {t!("no-braking-zones")} }
            } else {
                table { class: "laps",
                    tr {
                        th { "#" } th { {t!("braking-at")} } th { {t!("braking-speeds")} } th { {t!("braking-peak")} }
                        if !pb.is_empty() { th { {t!("braking-vs-pb")} } }
                    }
                    for (n, z) in zones.iter().enumerate() {
                        tr {
                            td { "{n + 1}" }
                            td { {units.length(z.at_m)} }
                            td { "{units.speed_value(z.entry_ms)} → {units.speed(z.exit_ms)}" }
                            td { "{z.peak_g:.2} g" }
                            if !pb.is_empty() {
                                td {
                                    if let Some(later) = z.counterpart(&pb).map(|p| z.at_m - p.at_m) {
                                        // Braking later than the PB shows green.
                                        span { class: if later >= 1.0 { "delta ahead" } else if later <= -1.0 { "delta behind" } else { "delta" },
                                            {format!("{}{}", if later < 0.0 { "−" } else { "+" }, units.length(later.abs()))}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The track with each braking zone drawn where it was driven, numbered at its braking point,
/// and the PB lap's braking points as rings.
#[component]
fn BrakingMap(layout: Layout, zones: Vec<Vec<SvgPoint>>, pb: Vec<SvgPoint>) -> Element {
    let r = layout.dot_r;
    rsx! {
        TrackSvg { layout, current: None,
            for (x, y) in pb {
                circle { class: "pb-brake", cx: x, cy: y, r }
            }
            for (n, path) in zones.iter().enumerate() {
                polyline { class: "brake", points: path.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect::<Vec<_>>().join(" ") }
                if let Some(&(x, y)) = path.first() {
                    circle { class: "brake-n", cx: x, cy: y, r: r * 1.4 }
                    text { class: "brake-n", x, y, font_size: r * 1.8, "{n + 1}" }
                }
            }
        }
    }
}
