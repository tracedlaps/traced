use std::sync::{Arc, LazyLock};

use traced_core::track::{Track, TrackShape, Waypoint};

/// Tracks available to record against. In code until tracks can be authored and saved.
pub static TRACKS: LazyLock<Vec<Arc<Track>>> = LazyLock::new(|| {
    // ~170 m x 110 m loop in Lisbon, gate and a sector midway along the first two legs.
    let lisbon = Track::builder(
        "Lisbon test loop",
        TrackShape::Closed {
            gate: Waypoint::new(-8.999, 38.7),
        },
    )
    .waypoints([
        Waypoint::new(-9.0, 38.7),
        Waypoint::new(-8.998, 38.7),
        Waypoint::new(-8.998, 38.701),
        Waypoint::new(-9.0, 38.701),
    ])
    .sector_gate_at(Waypoint::new(-8.998, 38.7005))
    .build()
    .expect("valid track");

    // Exported waypoints (lat,lon first); sectors start at waypoints 1, 19 and 33.
    let karting = waypoints(include_str!("../tracks/mondercange_acl.csv"));
    let mondercange = Track::builder(
        "Mondercange ACL Karting",
        TrackShape::Closed { gate: karting[0] },
    )
    .sector_gate_at(karting[18])
    .sector_gate_at(karting[32])
    .waypoints(karting)
    .build()
    .expect("valid track");

    // Speedtrike descent, top to bottom in file order; no sectors.
    let trike = waypoints(include_str!("../tracks/senhora_da_lage.csv"));
    let shape = TrackShape::Open {
        start: trike[0],
        finish: trike[trike.len() - 1],
    };
    let senhora_da_lage = Track::builder("Senhora da Lage Trike Descent", shape)
        .waypoints(trike)
        .build()
        .expect("valid track");

    vec![
        Arc::new(mondercange),
        Arc::new(senhora_da_lage),
        Arc::new(lisbon),
    ]
});

/// Waypoints from a CSV export with a header and `lat,lon` as its first columns.
fn waypoints(csv: &str) -> Vec<Waypoint> {
    csv.lines()
        .skip(1)
        .map(|line| {
            let mut cols = line.split(',').map(|c| c.parse().expect("numeric lat,lon"));
            let (lat, lon) = (cols.next().unwrap(), cols.next().unwrap());
            Waypoint::new(lon, lat)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_build() {
        let mondercange = &TRACKS[0];
        let sectors: Vec<_> = mondercange.sectors().map(|s| s.length.round()).collect();
        assert_eq!(mondercange.waypoints.len(), 41);
        assert_eq!(sectors.len(), 3);
        assert!((mondercange.length() - 856.0).abs() < 5.0, "{sectors:?}");

        let descent = &TRACKS[1];
        assert_eq!(descent.sectors().count(), 1);
        assert!((descent.length() - 2553.0).abs() < 5.0);
    }
}
