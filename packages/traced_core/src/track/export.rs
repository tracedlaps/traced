//! A track as GPX, which `import::track` reads back whole.

use super::{TrackBuilder, TrackShape, Waypoint};

/// GPX 1.1: the waypoints as a track segment, the gates as `wpt`s typed `start`, `finish` or `sector`.
pub fn gpx(track: &TrackBuilder) -> String {
    let point = |tag, w: &Waypoint, body: &str| format!(r#"<{tag} lat="{}" lon="{}">{body}</{tag}>"#, w.y(), w.x());
    let gate = |kind, w: &Waypoint| point("wpt", w, &format!("<type>{kind}</type>"));
    let (start, finish) = match track.shape() {
        TrackShape::Closed { gate } => (gate, None),
        TrackShape::Open { start, finish } => (start, Some(finish)),
    };
    let gates: Vec<_> = std::iter::once(gate("start", &start))
        .chain(finish.map(|f| gate("finish", &f)))
        .chain(track.sector_placements().iter().map(|w| gate("sector", w)))
        .collect();
    let path: Vec<_> = track.waypoint_list().iter().map(|w| point("trkpt", w, "")).collect();
    let (name, keywords) = (escape(track.name()), escape(&track.tag_list().join(", ")));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="Traced" xmlns="http://www.topografix.com/GPX/1/1">
<metadata><name>{name}</name><keywords>{keywords}</keywords></metadata>
{}
<trk><name>{name}</name><trkseg>
{}
</trkseg></trk>
</gpx>
"#,
        gates.join("\n"),
        path.join("\n"),
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Track, TrackShape, Waypoint, import};

    fn round_trip(track: &TrackBuilder) -> TrackBuilder {
        let back = import::track(&gpx(track)).unwrap();
        assert_eq!(back.name.as_deref(), Some(track.name()));
        assert_eq!(back.tags, track.tag_list());
        let rebuilt = back.builder().unwrap();
        assert!(rebuilt.same_geometry(track), "{rebuilt:?}");
        rebuilt
    }

    #[test]
    fn closed_track_round_trips() {
        let w = import::waypoints(include_str!("../../../mobile/tracks/mondercange_acl.csv")).unwrap();
        let track = Track::builder("Mondercange ACL Karting", TrackShape::Closed { gate: w[0] })
            .tags(["karting", "outdoor"])
            .sector_gate_at(w[18])
            .sector_gate_at(w[32])
            .waypoints(w);
        round_trip(&track).build().unwrap();
    }

    #[test]
    fn open_track_round_trips() {
        let w: Vec<_> = (0..5).map(|i| Waypoint::new(-8.304 + i as f64 * 0.001, 40.877)).collect();
        let track = Track::builder("Descent", TrackShape::Open { start: w[1], finish: w[4] })
            .sector_gate_at(w[3])
            .sector_gate_at(w[2])
            .waypoints(w);
        round_trip(&track);
    }

    #[test]
    fn escapes_the_name() {
        let w = [Waypoint::new(6.21, 49.81), Waypoint::new(6.22, 49.82)];
        let track = Track::builder(r#"Tom & Jerry's <"fast"> loop"#, TrackShape::Open { start: w[0], finish: w[1] })
            .tags(["a&b"])
            .waypoints(w);
        round_trip(&track);
    }
}
