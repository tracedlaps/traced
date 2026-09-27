use dioxus::prelude::*;
use traced_core::track::{Boundary, Gate, LiveStatus, Track, Waypoint};

use crate::recorder::RecorderCtx;

/// The recording's track outline and gates, with a dot at the latest fix. North up,
/// drawn in the track's own projected metres (SVG y points down, hence the flipped y).
#[component]
pub fn TrackMap() -> Element {
    let ctx = use_context::<RecorderCtx>();
    // Only re-derived when the track changes, not on every fix.
    let layout = use_memo(move || ctx.track.read().as_deref().map(Layout::new));
    let Some(layout) = layout() else {
        return rsx! {};
    };
    let dot = ctx
        .track
        .read()
        .as_deref()
        .zip(*ctx.fix.read())
        .map(|(track, fix)| svg_point(track, Waypoint::new(fix.longitude, fix.latitude)));
    let current = match ctx.live.read().as_ref().map(|l| l.status) {
        Some(LiveStatus::InProgress { sector, .. }) => Some(sector),
        _ => None,
    };

    rsx! {
        svg { class: "track-map", view_box: "{layout.view_box}",
            path { class: "outline", d: "{layout.outline}" }
            for (k , d) in layout.sectors.into_iter().enumerate() {
                path {
                    class: if current == Some(k) { "sector current" } else { "sector" },
                    style: "stroke: var(--sector-{k % 4})",
                    d,
                }
            }
            for ((x1 , y1) , (x2 , y2) , boundary) in layout.gates {
                line {
                    class: if boundary { "gate boundary" } else { "gate" },
                    x1,
                    y1,
                    x2,
                    y2,
                }
            }
            if let Some((x, y)) = dot {
                // Positioned by transform, so CSS can glide it from one fix to the next.
                circle {
                    class: "you",
                    r: layout.dot_r,
                    style: "transform: translate({x}px, {y}px)",
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct Layout {
    view_box: String,
    outline: String,
    /// One path per sector, in travel order.
    sectors: Vec<String>,
    /// Gate ends, and whether it's a start/finish gate (vs a sector gate).
    gates: Vec<(SvgPoint, SvgPoint, bool)>,
    dot_r: f64,
}

impl Layout {
    fn new(track: &Track) -> Self {
        let closed = matches!(track.boundary, Boundary::Closed { .. });
        // Travel order; a Closed track's start gate is also its finish, so it's listed once.
        let mut ordered: Vec<_> = track.gates_ordered().collect();
        if closed {
            ordered.pop();
        }
        let gates: Vec<_> = ordered
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let seg = g.segment();
                let boundary = i == 0 || i == ordered.len() - 1 && !closed;
                (
                    (seg.start.x, -seg.start.y),
                    (seg.end.x, -seg.end.y),
                    boundary,
                )
            })
            .collect();

        let (points, splits) = with_gate_points(track, &ordered, closed);
        let curves = curves(&points, closed);
        let path = |from: usize, len: usize| {
            let (x, y) = points[from];
            let curves = (0..len).map(|j| curves[(from + j) % curves.len()].as_str());
            format!("M{x:.1} {y:.1}{}", curves.collect::<String>())
        };
        let sectors = (0..if closed {
            splits.len()
        } else {
            splits.len() - 1
        })
            .map(|k| {
                let (from, to) = (splits[k], splits[(k + 1) % splits.len()]);
                // Wraps past the seam on a Closed track; a lone gate's sector is the whole lap.
                match (to + curves.len() - from) % curves.len() {
                    0 => path(from, curves.len()),
                    len => path(from, len),
                }
            })
            .collect();

        let all = points
            .iter()
            .chain(gates.iter().flat_map(|(a, b, _)| [a, b]));
        let (min_x, min_y, max_x, max_y) = all.fold(
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
            |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        );
        let extent = (max_x - min_x).max(max_y - min_y);
        let pad = extent * 0.05;

        Self {
            view_box: format!(
                "{} {} {} {}",
                min_x - pad,
                min_y - pad,
                max_x - min_x + 2.0 * pad,
                max_y - min_y + 2.0 * pad
            ),
            outline: path(0, curves.len()) + if closed { " Z" } else { "" },
            sectors,
            gates,
            dot_r: extent / 50.0,
        }
    }
}

type SvgPoint = (f64, f64);

fn dist(a: SvgPoint, b: SvgPoint) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// The waypoints as SVG points, plus a point wherever a gate falls between two of them,
/// and the index of each gate's point (in `gates` order), so sectors split exactly on gates.
fn with_gate_points(track: &Track, gates: &[&Gate], closed: bool) -> (Vec<SvgPoint>, Vec<usize>) {
    /// Closer than this (m) to a waypoint, a gate reuses it instead of adding a point.
    const SNAP: f64 = 0.01;
    let waypoints: Vec<_> = track
        .waypoints
        .iter()
        .map(|w| svg_point(track, *w))
        .collect();
    let n = waypoints.len();
    let mut pending: Vec<_> = gates
        .iter()
        .map(|g| g.distance_along_track)
        .enumerate()
        .collect();
    pending.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut pending = pending.into_iter().peekable();

    let (mut points, mut splits, mut travelled) = (Vec::new(), vec![0; gates.len()], 0.0);
    for i in 0..if closed { n } else { n - 1 } {
        let (a, b) = (waypoints[i], waypoints[(i + 1) % n]);
        let len = dist(a, b);
        points.push(a);
        while let Some(&(g, d)) = pending.peek().filter(|&&(_, d)| d < travelled + len - SNAP) {
            if d - travelled > SNAP {
                let t = (d - travelled) / len;
                points.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
            splits[g] = points.len() - 1;
            pending.next();
        }
        travelled += len;
    }
    if !closed {
        points.push(waypoints[n - 1]);
    }
    // Left over: on the very end (an Open finish), or the seam back to a Closed start.
    for (g, _) in pending {
        splits[g] = if closed { 0 } else { points.len() - 1 };
    }
    (points, splits)
}

/// A smooth curve through `points` as one SVG cubic Bézier (` C…`) per consecutive pair:
/// Catmull-Rom, with each tangent scaled to its segment's length so uneven waypoint spacing
/// doesn't overshoot. Display only; timing uses the straight-segment polyline.
fn curves(points: &[SvgPoint], closed: bool) -> Vec<String> {
    let n = points.len() as isize;
    let at = |i: isize| match closed {
        true => points[i.rem_euclid(n) as usize],
        false => points[i.clamp(0, n - 1) as usize],
    };
    // Direction through point i, from its neighbours; one-sided at an open end.
    let tangent = |i: isize| {
        let (a, p, b) = (at(i - 1), at(i), at(i + 1));
        let d = dist(a, p) + dist(p, b);
        if d == 0.0 {
            (0.0, 0.0)
        } else {
            ((b.0 - a.0) / d, (b.1 - a.1) / d)
        }
    };
    (0..if closed { n } else { n - 1 })
        .map(|i| {
            let (p, q, m1, m2) = (at(i), at(i + 1), tangent(i), tangent(i + 1));
            let l = dist(p, q) / 3.0;
            let (c1, c2) = (
                (p.0 + m1.0 * l, p.1 + m1.1 * l),
                (q.0 - m2.0 * l, q.1 - m2.1 * l),
            );
            format!(
                " C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                c1.0, c1.1, c2.0, c2.1, q.0, q.1
            )
        })
        .collect()
}

fn svg_point(track: &Track, w: Waypoint) -> SvgPoint {
    let p = track.projection.project(w);
    (p.x(), 0.0 - p.y()) // not -p.y(): no "-0.0" in paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::PRESETS;

    fn tracks() -> Vec<Track> {
        PRESETS
            .iter()
            .map(|p| p.track.clone().build().unwrap())
            .collect()
    }

    #[test]
    fn curves_pass_through_every_point() {
        let square = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (10.0, 10.0),
            (0.0, 10.0),
        ];
        let closed = curves(&square, true);
        assert_eq!(closed.len(), 5);
        assert!(closed[4].ends_with(" 0.0 0.0") && !closed.concat().contains("NaN"));
        let open = curves(&square, false);
        assert_eq!(open.len(), 4);
        assert!(open[3].ends_with(" 0.0 10.0"));
    }

    #[test]
    fn sectors_start_on_their_gates() {
        for track in &tracks() {
            let l = Layout::new(track);
            assert_eq!(l.sectors.len(), track.sectors().count(), "{}", track.name);
            for (sector, gate) in l.sectors.iter().zip(track.gates_ordered()) {
                let (x, y) = (gate.center.x(), 0.0 - gate.center.y()); // 0.0 - y: no "-0.0"
                assert!(
                    sector.starts_with(&format!("M{x:.1} {y:.1} C")),
                    "{}: {sector:.40}",
                    track.name
                );
            }
        }
    }

    /// Writes each track's map to the target dir for eyeballing: `cargo test -p mobile -- --ignored`.
    #[test]
    #[ignore]
    fn render_track_maps() {
        for (i, track) in tracks().iter().enumerate() {
            let l = Layout::new(track);
            let colors = ["#2563eb", "#16a34a", "#9333ea", "#0891b2"];
            let stroke = |d: &str, color: &str, opacity: f64| {
                format!(
                    r#"<path d="{d}" fill="none" stroke="{color}" stroke-opacity="{opacity}" stroke-width="{}"/>"#,
                    l.dot_r
                )
            };
            let sectors: String = l
                .sectors
                .iter()
                .enumerate()
                .map(|(k, d)| stroke(d, colors[k % 4], 0.35))
                .collect();
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{}" width="800" style="background:white">{}{sectors}</svg>"#,
                l.view_box,
                stroke(&l.outline, "#e7e5e4", 1.0),
            );
            std::fs::write(
                format!(
                    "{}/../../target/track_map_{i}.svg",
                    env!("CARGO_MANIFEST_DIR")
                ),
                svg,
            )
            .unwrap();
        }
    }
}
