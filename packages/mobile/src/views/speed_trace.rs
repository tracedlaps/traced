//! A lap's speed against distance past the start gate, the PB's alongside, braking zones shaded.

use dioxus::prelude::*;
use dioxus_i18n::t;

use crate::units::Units;

/// The drawing, in its own units; it scales to the screen's width.
const W: f64 = 360.0;
const H: f64 = 190.0;
/// Room for speeds on the left, zone numbers above and distances below.
const LEFT: f64 = 28.0;
const TOP: f64 = 14.0;
const BOTTOM: f64 = 18.0;

/// `lap` and `pb` are (distance, m/s) per fix; `zones` (from, to) distances, numbered as on the map.
/// Touching the chart reads both speeds at that distance.
#[component]
pub fn SpeedTrace(number: usize, lap: Vec<(f64, f32)>, pb: Vec<(f64, f32)>, zones: Vec<(f64, f64)>) -> Element {
    let units = Units::current();
    let mut width = use_signal(|| W);
    let mut touched = use_signal(|| None::<f64>);
    let length = lap.iter().chain(&pb).map(|p| p.0).fold(0.0, f64::max);
    let top = lap.iter().chain(&pb).map(|p| p.1 as f64 * units.per_ms()).fold(0.0, f64::max);
    if length <= 0.0 || top <= 0.0 {
        return rsx! {};
    }
    let (v_step, d_step) = (nice(top), nice(length * units.per_m()) / units.per_m());
    let v_max = (top / v_step).ceil() * v_step;
    let x = move |d: f64| LEFT + d / length * (W - LEFT);
    let y = move |ms: f32| TOP + (1.0 - ms as f64 * units.per_ms() / v_max) * (H - TOP - BOTTOM);
    let line = |trace: &[(f64, f32)]| trace.iter().map(|&(d, v)| format!("{:.1},{:.1}", x(d), y(v))).collect::<Vec<_>>().join(" ");
    let read = move |e: PointerEvent| {
        let px = e.element_coordinates().x * W / width();
        touched.set(Some(((px - LEFT) / (W - LEFT) * length).clamp(0.0, length)));
    };
    let at = touched().map(|d| (d, speed_at(&lap, d), speed_at(&pb, d)));

    rsx! {
        div { class: "trace-legend",
            span { class: "key lap", {t!("trace-lap", lap: number)} }
            if !pb.is_empty() { span { class: "key pb", {t!("pb")} } }
        }
        p { class: "trace-readout",
            match at {
                Some((d, v, p)) => rsx! {
                    "{units.length(d)}"
                    if let Some(v) = v { " · " strong { "{units.speed(v)}" } }
                    if let Some(p) = p { " · " {t!("pb")} " {units.speed(p)}" }
                },
                None => rsx! { {t!("trace-hint")} },
            }
        }
        svg {
            class: "speed-trace",
            view_box: "0 0 {W} {H}",
            onresize: move |e| width.set(e.get_border_box_size().map_or(W, |s| s.width).max(1.0)),
            onpointerdown: read,
            onpointermove: read,
            for (n, &(from, to)) in zones.iter().enumerate() {
                rect { class: "zone", x: x(from), y: TOP, width: (x(to) - x(from)).max(1.0), height: H - TOP - BOTTOM }
                text { class: "axis zone-n", x: (x(from) + x(to)) / 2.0, y: TOP - 3.0, "{n + 1}" }
            }
            for i in 0..=(v_max / v_step) as usize {
                line { class: "grid", x1: LEFT, x2: W, y1: y((i as f64 * v_step / units.per_ms()) as f32), y2: y((i as f64 * v_step / units.per_ms()) as f32) }
                text { class: "axis v", x: LEFT - 4.0, y: y((i as f64 * v_step / units.per_ms()) as f32), {label(i as f64 * v_step)} }
            }
            for i in 0..=(length / d_step) as usize {
                text { class: "axis d", x: x(i as f64 * d_step), y: H - 4.0, {label(i as f64 * d_step * units.per_m())} }
            }
            polyline { class: "pb-line", points: line(&pb) }
            polyline { class: "lap-line", points: line(&lap) }
            if let Some((d, v, p)) = at {
                line { class: "cross", x1: x(d), x2: x(d), y1: TOP, y2: H - BOTTOM }
                if let Some(p) = p { circle { class: "dot pb", cx: x(d), cy: y(p), r: 4 } }
                if let Some(v) = v { circle { class: "dot lap", cx: x(d), cy: y(v), r: 4 } }
            }
        }
    }
}

/// A round step (1, 2 or 5 × a power of ten) splitting `span` into at most 5.
fn nice(span: f64) -> f64 {
    let p = 10f64.powf(span.log10().floor());
    [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * p).find(|s| span / s <= 5.0).unwrap_or(10.0 * p)
}

/// An axis value, without float noise (499.99999 ft).
fn label(v: f64) -> String {
    format!("{}", (v * 10.0).round() / 10.0)
}

/// The speed `d` metres in, between the fixes either side; `None` outside the trace.
fn speed_at(trace: &[(f64, f32)], d: f64) -> Option<f32> {
    let i = trace.partition_point(|p| p.0 < d);
    let ((d0, v0), (d1, v1)) = (*trace.get(i.checked_sub(1)?)?, *trace.get(i)?);
    Some(if d1 > d0 { v0 + (v1 - v0) * ((d - d0) / (d1 - d0)) as f32 } else { v1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_steps_are_round() {
        assert_eq!([nice(130.0), nice(2600.0), nice(40.0), nice(9.0)], [50.0, 1000.0, 10.0, 2.0]);
    }

    #[test]
    fn speed_at_interpolates_inside_the_trace() {
        let trace = [(0.0, 10.0), (10.0, 20.0), (30.0, 10.0)];
        assert_eq!([speed_at(&trace, 5.0), speed_at(&trace, 20.0), speed_at(&trace, 31.0)], [Some(15.0), Some(15.0), None]);
    }
}
