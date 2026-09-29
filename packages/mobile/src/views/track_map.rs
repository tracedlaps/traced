use dioxus::prelude::*;
use traced_core::track::{Boundary, Gate, LiveStatus, Track, Waypoint};

use crate::{
    prefs::{Choice, MapView},
    recorder::RecorderCtx,
};

/// The recording's track outline and gates, with a dot at the latest fix. North up,
/// drawn in the track's own projected metres (SVG y points down, hence the flipped y).
/// Follows the dot, fading out at the edges, or shows the whole track; tapping switches.
#[component]
pub fn TrackMap() -> Element {
    let ctx = use_context::<RecorderCtx>();
    let mut view = use_context::<Signal<MapView>>();
    let toggle = move |_| {
        let next = if view() == MapView::Follow { MapView::Overview } else { MapView::Follow };
        next.save();
        view.set(next);
    };
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
    // A replay's fixes come `speed`× as often, so the glide shortens to match.
    let glide = 1.0 / ctx.recording.read().as_ref().map_or(1.0, |r| r.clock.speed());
    let current = match ctx.live.read().as_ref().map(|l| l.status) {
        Some(LiveStatus::InProgress { sector, .. }) => Some(sector),
        _ => None,
    };

    if view() == MapView::Follow {
        // Waits at the start gate for the first fix, so the zoom doesn't jump when it comes.
        let (x, y) = dot.unwrap_or(layout.gates[0].0);
        let w = layout.follow_window();
        // Ten times as tall as wide, then sliced to the element: the scale comes from its width alone,
        // so it holds while the panels below grow and shrink the map's height.
        let (h, x, y) = (10.0 * w, -x, -y);
        let (left, top) = (-w / 2.0, -h / 2.0);
        // The track glides under a still dot, at the pace the dot would have.
        return rsx! {
            svg {
                class: "track-map follow",
                view_box: "{left} {top} {w} {h}",
                preserve_aspect_ratio: "xMidYMid slice",
                onclick: toggle,
                g {
                    style: "transform: translate({x}px, {y}px); transition-duration: {glide}s",
                    dangerous_inner_html: layout.markup(current, None),
                }
                if dot.is_some() {
                    circle { class: "you", r: w / 40.0 }
                }
            }
        };
    }

    rsx! {
        TrackSvg { layout: layout.clone(), current, onclick: toggle,
            if let Some((x, y)) = dot {
                // Positioned by transform, so CSS can glide it from one fix to the next.
                circle {
                    class: "you",
                    r: layout.dot_r,
                    style: "transform: translate({x}px, {y}px); transition-duration: {glide}s",
                }
            }
        }
    }
}

/// A track's outline, sectors (`current` highlighted) and gates; `children` draw on top,
/// in the same coordinates (see `svg_point`).
#[component]
pub fn TrackSvg(layout: Layout, current: Option<usize>, onclick: Option<EventHandler<MouseEvent>>, children: Element) -> Element {
    let [x, y, w, h] = layout.view_box;
    rsx! {
        svg { class: "track-map", view_box: "{x} {y} {w} {h}", onclick: move |e| if let Some(f) = onclick { f.call(e) },
            g { dangerous_inner_html: layout.markup(current, Some(layout.gate_half())) }
            {children}
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Layout {
    /// x, y, width, height.
    pub view_box: [f64; 4],
    outline: String,
    /// One path per sector, in travel order.
    sectors: Vec<String>,
    /// Gate centres, half the gate as a vector from there, and whether it's a start/finish gate (vs a sector gate).
    gates: Vec<(SvgPoint, SvgPoint, bool)>,
    /// The drawing's larger side, without padding.
    extent: f64,
    /// Chevrons along the outline, pointing the way it's driven.
    arrows: Vec<String>,
    pub dot_r: f64,
}

impl Layout {
    pub fn new(track: &Track) -> Self {
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
                let half = ((seg.end.x - seg.start.x) / 2.0, (seg.start.y - seg.end.y) / 2.0);
                ((seg.start.x + half.0, half.1 - seg.start.y), half, boundary)
            })
            .collect();

        let (points, splits) = with_gate_points(track, &ordered, closed);
        let beziers = curves(&points, closed);
        let curves: Vec<_> = beziers
            .iter()
            .map(|[(ax, ay), (bx, by), (x, y)]| format!(" C{ax:.1} {ay:.1} {bx:.1} {by:.1} {x:.1} {y:.1}"))
            .collect();
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

        // Gates fit in the padding, drawn at `gate_half`.
        let (min_x, min_y, max_x, max_y) = points.iter().fold(
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
            |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        );
        let extent = (max_x - min_x).max(max_y - min_y);
        let pad = extent * 0.05;

        Self {
            view_box: [
                min_x - pad,
                min_y - pad,
                max_x - min_x + 2.0 * pad,
                max_y - min_y + 2.0 * pad,
            ],
            outline: path(0, curves.len()) + if closed { " Z" } else { "" },
            sectors,
            gates,
            extent,
            arrows: arrows(&points, &beziers, extent / 6.0, extent / 60.0),
            dot_r: extent / 50.0,
        }
    }
}

impl Layout {
    /// Gates' half-length when drawing the whole track: the same share of it on every track,
    /// where their real 30 m would be a stub on a long one and a bar across a short one.
    pub fn gate_half(&self) -> f64 {
        self.extent / 24.0
    }

    /// The width (m) seen when following the dot: most of a karting track,
    /// or a long stretch of road.
    pub fn follow_window(&self) -> f64 {
        (self.extent / 1.5).clamp(200.0, 600.0)
    }

    /// The outline, sectors (`current` highlighted) and gates as SVG elements, for `TrackSvg`
    /// and the map editor's overlay alike. Gates are `gate_half` either side, or their real size.
    pub fn markup(&self, current: Option<usize>, gate_half: Option<f64>) -> String {
        let sectors = self.sectors.iter().enumerate().map(|(k, d)| {
            let class = if current == Some(k) { "sector current" } else { "sector" };
            format!(r#"<path class="{class}" style="stroke: var(--sector-{})" d="{d}"/>"#, k % 4)
        });
        let gates = self.gates.iter().map(|&((x, y), (dx, dy), boundary)| {
            let k = gate_half.map_or(1.0, |h| h / dx.hypot(dy));
            let (x1, y1, x2, y2) = (x - dx * k, y - dy * k, x + dx * k, y + dy * k);
            let class = if boundary { "gate boundary" } else { "gate" };
            format!(r#"<line class="{class}" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}"/>"#)
        });
        let arrows = self.arrows.iter().map(|d| format!(r#"<path class="arrow" d="{d}"/>"#));
        format!(r#"<path class="outline" d="{}"/>"#, self.outline) + &sectors.chain(gates).chain(arrows).collect::<String>()
    }
}

pub type SvgPoint = (f64, f64);

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

/// A `^` chevron of half-size `size` every `spacing` along the curves (first half a spacing in),
/// pointing along them; placed by chord length, so only roughly evenly.
fn arrows(points: &[SvgPoint], beziers: &[[SvgPoint; 3]], spacing: f64, size: f64) -> Vec<String> {
    let mut next = spacing / 2.0;
    let mut travelled = 0.0;
    let mut arrows = Vec::new();
    for (&p0, &[p1, p2, p3]) in points.iter().zip(beziers) {
        let len = dist(p0, p3);
        while len > 0.0 && next < travelled + len {
            let t = (next - travelled) / len;
            let u = 1.0 - t;
            let lerp = |w: [f64; 4]| (w[0] * p0.0 + w[1] * p1.0 + w[2] * p2.0 + w[3] * p3.0, w[0] * p0.1 + w[1] * p1.1 + w[2] * p2.1 + w[3] * p3.1);
            let (x, y) = lerp([u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t]);
            let (dx, dy) = lerp([-u * u, u * u - 2.0 * u * t, 2.0 * u * t - t * t, t * t]);
            let d = dx.hypot(dy);
            if d > 0.0 {
                let (dx, dy) = (dx / d * size, dy / d * size);
                arrows.push(format!(
                    "M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}",
                    x - dx - dy, y - dy + dx, x + dx, y + dy, x - dx + dy, y - dy - dx
                ));
            }
            next += spacing;
        }
        travelled += len;
    }
    arrows
}

/// A smooth curve through `points` as one cubic Bézier (control points and end) per consecutive pair:
/// Catmull-Rom, with each tangent scaled to its segment's length so uneven waypoint spacing
/// doesn't overshoot. Display only; timing uses the straight-segment polyline.
fn curves(points: &[SvgPoint], closed: bool) -> Vec<[SvgPoint; 3]> {
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
            [(p.0 + m1.0 * l, p.1 + m1.1 * l), (q.0 - m2.0 * l, q.1 - m2.1 * l), q]
        })
        .collect()
}

pub fn svg_point(track: &Track, w: Waypoint) -> SvgPoint {
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
        assert!(closed[4][2] == (0.0, 0.0) && closed.iter().flatten().all(|p| p.0.is_finite() && p.1.is_finite()));
        let open = curves(&square, false);
        assert_eq!(open.len(), 4);
        assert_eq!(open[3][2], (0.0, 10.0));
    }

    #[test]
    fn arrows_point_the_way_the_waypoints_run() {
        let line = [(0.0, 0.0), (100.0, 0.0)];
        let arrows = arrows(&line, &curves(&line, false), 50.0, 5.0);
        // Tips (the middle point) ahead of their arms in +x.
        assert_eq!(arrows, ["M20.0 5.0L30.0 0.0L20.0 -5.0", "M70.0 5.0L80.0 0.0L70.0 -5.0"]);
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

    #[test]
    fn gates_are_drawn_centred_across_the_track() {
        for track in &tracks() {
            let l = Layout::new(track);
            for (&(c, half, _), gate) in l.gates.iter().zip(track.gates_ordered()) {
                assert!(dist(c, (gate.center.x(), -gate.center.y())) < 1e-6, "{}", track.name);
                // Parallel to the real gate.
                let seg = gate.segment();
                let (dx, dy) = (seg.end.x - seg.start.x, seg.start.y - seg.end.y);
                assert!((half.0 * dy - half.1 * dx).abs() < 1e-6, "{}", track.name);
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
                l.view_box.map(|v| v.to_string()).join(" "),
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
